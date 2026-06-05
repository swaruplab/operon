# Developer Patterns — Extending SingleCellExperiment

This guide is for package authors building tools on top of SCE, or for analysts adding many custom slots to an SCE for a particular pipeline. Casual users can skip — the main SKILL covers everything you need for analysis.

## When to subclass SCE

You **shouldn't** subclass SCE for most use cases. The standard slots (`assays`, `colData`, `rowData`, `reducedDims`, `altExps`, `metadata`) cover almost everything. Subclass only when:

- You have multi-element data that doesn't fit the assay/altExp model (e.g. cell-cell distance matrices)
- You're providing a new specialized matrix type (the canonical example: `LinearEmbeddingMatrix` in the SCE package itself)
- Your tool maintains complex per-cell state that needs to survive subsetting

In nearly all other cases: add columns to `colData`, use `metadata` for study-level info, or stash things under `int_colData(sce)` / `int_metadata(sce)` with a package-prefixed key.

## Internal vs external fields

SCE exposes two parallel storage systems:

| Visible to user | Hidden from user | Use for |
|---|---|---|
| `colData(sce)` | `int_colData(sce)` | Cell-level metadata |
| `rowData(sce)` | `int_rowData(sce)` | Feature-level metadata |
| `metadata(sce)` | `int_metadata(sce)` | Study-level metadata |

The internal versions are **not** for user access — they're for package internals that need to attach things to the SCE without cluttering the user-visible interface.

## The "Inception" nesting pattern — multi-package etiquette

If two packages both want to store something called "X" on a SCE, they'll collide. The convention is to nest under your package name:

```r
# WRONG: direct slot collision risk
AsetX_bad <- function(sce) {
  int_colData(sce)$X <- runif(ncol(sce))   # what if another package also wants "X"?
  sce
}

# RIGHT: namespaced under your package
AsetX_good <- function(sce) {
  int_colData(sce)$mypackage <- DataFrame(X = runif(ncol(sce)))
  sce
}

# Reading it back
AgetX <- function(sce) {
  int_colData(sce)$mypackage$X
}
```

Result: every package's internal data lives in its own DataFrame, no collisions, easy to audit (`names(int_colData(sce))` shows which packages have decorated the object).

## Defining a new SCE subclass

The minimum recipe — say you want a `MyExperiment` that's an SCE with one extra integer slot:

```r
library(SingleCellExperiment)

setClass(
  "MyExperiment",
  contains = "SingleCellExperiment",
  slots = c(myslot = "integer")
)

# Constructor
MyExperiment <- function(..., myslot = integer(0)) {
  sce <- SingleCellExperiment(...)
  .myexperiment_from_sce(sce, myslot = myslot)
}

.myexperiment_from_sce <- function(sce, myslot) {
  new("MyExperiment", sce, myslot = myslot)
}

# Validity method
setValidity2("MyExperiment", function(object) {
  if (length(object@myslot) != ncol(object)) {
    return("myslot must match number of cells")
  }
  TRUE
})

# Accessor
setGeneric("myslot", function(x, ...) standardGeneric("myslot"))
setMethod("myslot", "MyExperiment", function(x) x@myslot)

# Setter
setGeneric("myslot<-", function(x, ..., value) standardGeneric("myslot<-"))
setReplaceMethod("myslot", "MyExperiment", function(x, value) {
  x@myslot <- as.integer(value)
  validObject(x)
  x
})
```

### Surviving subsetting

If `myslot` is per-cell (length = `ncol(sce)`), it needs to be subsetted along with the cells when the user does `sce[, idx]`. Add a method:

```r
setMethod("[", "MyExperiment", function(x, i, j, ..., drop = FALSE) {
  out <- callNextMethod()                          # let SCE do its thing
  if (!missing(j)) {
    if (is.logical(j)) j <- which(j)
    if (is.character(j)) j <- match(j, colnames(x))
    out@myslot <- x@myslot[j]
  }
  out
})
```

Without this, the user does `sce[, 1:10]` and your `myslot` still has the original length — silently broken.

## Design rationale (from the dev vignette)

Three SCE design decisions are worth knowing if you're building on it:

### 1. `reducedDims` as a SimpleList, not a fixed slot

Different dimensionality reductions have different shapes (PCA → 50 components, UMAP → 2). A single slot can't hold them all, so SCE uses a named list. The trade-off: you have to access by name (`reducedDim(sce, "UMAP")`) rather than as a typed slot.

### 2. Inheriting from `RangedSummarizedExperiment` (not `SummarizedExperiment`)

Some assays (e.g. ATAC) need genomic coordinates per feature. Inheriting from `RangedSummarizedExperiment` provides that for free without needing a separate class. The downside: you get a `rowRanges` slot you may not use.

### 3. `altExps` as nested SCEs (not `MultiAssayExperiment`)

`MultiAssayExperiment` is the canonical Bioc class for truly multi-omics datasets where each modality may have its own cells. For single-sample multi-feature scenarios (spike-ins, HTOs, ADT — all share the SAME cells), `MultiAssayExperiment` adds overhead. SCE's `altExps` keeps everything in one object, sharing cell IDs automatically.

## Validity methods

Subclass validity catches bugs early. Run `validObject(sce)` after every constructor:

```r
setValidity2("MyExperiment", function(object) {
  errors <- character()

  if (length(object@myslot) != ncol(object))
    errors <- c(errors, "myslot length must match ncol(object)")

  if (any(is.na(object@myslot)))
    errors <- c(errors, "myslot must not contain NA")

  if (length(errors) > 0) errors else TRUE
})
```

`setValidity2` (from S4Vectors) is preferred over base `setValidity` because it catches the validity errors from parent classes automatically.

## Working with package-namespaced internal storage

The OSCA/Bioconductor convention: when your package needs to attach state, use a single internal entry keyed by your package name:

```r
.pkg <- "mypackage"

.get_internal <- function(sce, field) {
  pkg_data <- int_metadata(sce)[[.pkg]]
  if (is.null(pkg_data)) return(NULL)
  pkg_data[[field]]
}

.set_internal <- function(sce, field, value) {
  pkg_data <- int_metadata(sce)[[.pkg]]
  if (is.null(pkg_data)) pkg_data <- list()
  pkg_data[[field]] <- value
  int_metadata(sce)[[.pkg]] <- pkg_data
  sce
}

# Usage
sce <- .set_internal(sce, "version", "1.0.0")
.get_internal(sce, "version")
# [1] "1.0.0"
```

## When to push upstream into SCE itself

If multiple packages independently invent the same slot with the same scientific meaning (e.g. they all want a "knn graph" slot), that's a signal it should live in SCE itself. The SCE maintainers welcome these consolidations — open a GitHub issue at `Bioconductor/SingleCellExperiment` to start the conversation.

The current `colLabels()` and `sizeFactors()` convenience setters started this way — they used to be ad-hoc storage by individual packages.

## Bioconductor coding conventions

- **Snake_case for internal helpers** (`.my_helper`), camelCase for exported functions (`myFunction`).
- **Avoid `<-` inside method bodies for slots** — use `@` only in the method definition itself; expose `slot()` for users.
- **Use `setGeneric` + `setMethod` even for SCE subclasses** — inherits dispatch correctly with multiple subclasses.
- **`validObject()` after every constructor exit point.**

For the canonical reference on Bioconductor S4 class design, see Hervé Pagès's vignette in the `S4Vectors` package, plus the OSCA book's appendix on `SummarizedExperiment` internals.
