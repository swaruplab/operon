---
name: singlecellexperiment
description: SingleCellExperiment (SCE) — the canonical Bioconductor S4 container for single-cell genomics. Covers SCE construction, assay/colData/rowData/reducedDims/altExps accessors, sizeFactors and labels, iteration via applySCE, and the standard scater + scran preprocessing pipeline (QC → normalization → HVG → PCA → clustering → markers). Sister container to Seurat — pick SCE when integrating with the Bioconductor ecosystem (scran, scater, scDblFinder, miloR, MAST, batchelor).
license: GPL-3.0
metadata:
---

# SingleCellExperiment: Bioconductor scRNA-seq Container

## Overview

[SingleCellExperiment](https://bioconductor.org/packages/release/bioc/html/SingleCellExperiment.html) (SCE) is the canonical Bioconductor S4 class for storing scRNA-seq data — the equivalent of Seurat's `Seurat` object or scanpy's `AnnData`. It extends `RangedSummarizedExperiment` with single-cell-specific slots: `reducedDims`, `altExps` (alternative experiments — spike-ins, hashtags, ADT), and `sizeFactors`.

The SCE object is the data backbone for the entire Bioconductor scRNA-seq ecosystem:

| Step | Package | What it does |
|---|---|---|
| QC + visualization | [scater](https://bioconductor.org/packages/scater) | Per-cell QC metrics, normalization, plotting |
| Normalization + HVG | [scran](https://bioconductor.org/packages/scran) | Deconvolution sizeFactors, modelGeneVar |
| Clustering / DE | [scran](https://bioconductor.org/packages/scran), [bluster](https://bioconductor.org/packages/bluster) | Graph clustering + marker discovery |
| Doublets | [scDblFinder](https://bioconductor.org/packages/scDblFinder) | Doublet calling |
| Integration | [batchelor](https://bioconductor.org/packages/batchelor) | MNN, Harmony, etc. |
| Trajectory | [TSCAN](https://bioconductor.org/packages/TSCAN), [tradeSeq](https://bioconductor.org/packages/tradeSeq) | Pseudotime + gene-along-trajectory tests |
| DA | [miloR](https://bioconductor.org/packages/miloR) | Neighborhood differential abundance |

Most of these `read sce → ... → return sce`, so the SCE flows through the whole pipeline as a single object.

## When to Use This Skill

- You're already in the Bioconductor ecosystem (DESeq2, edgeR, limma, multi-omics tools)
- You need clean integration with scran's deconvolution sizeFactors (statistically principled for very sparse data)
- Working with spike-ins, hashtags (HTOs), or ADT data — SCE's `altExps` handles these natively
- Building a package — SCE is the reference S4 class for scRNA-seq data
- Reading h5ad / Seurat .rds INTO Bioconductor for downstream analyses (e.g. miloR for DA)

**Not for**:
- Pure exploratory analysis where Seurat or scanpy is faster — both have richer interactive defaults
- Multimodal CITE-seq where Seurat's WNN is the cleaner toolkit
- ATAC-seq — use `archr` (Bioconductor-aware) or `snapatac2`

## Prerequisites

- R ≥ 4.6
- BiocManager
- The standard scRNA-seq Bioconductor stack: scater, scran, BiocSingular, BiocParallel
- 16-32 GB RAM for ~100k cells; 64+ GB for ~500k+

### Installation

```r
if (!require("BiocManager", quietly = TRUE))
    install.packages("BiocManager")

BiocManager::install(c(
  "SingleCellExperiment",
  "scater",            # QC + plotting
  "scran",             # normalization + HVG + clustering
  "scDblFinder",       # doublets
  "batchelor",         # multi-sample integration
  "DropletUtils",      # empty drops + 10X reading
  "BiocSingular",      # fast PCA backends
  "BiocParallel"       # parallelism
))
```

For multi-sample comparison and DA: `BiocManager::install(c("miloR", "MAST"))`.

For trajectory: `BiocManager::install(c("TSCAN", "tradeSeq", "slingshot"))`.

## The SCE Class — Structure

```
             SingleCellExperiment
             ┌──────────────────────────────────────────────────┐
   assays(sce) [counts, logcounts, ...]   one named matrix per assay
   rowData(sce)     ────  features × n_features_cols     gene metadata
   colData(sce)     ────  cells × n_cell_cols           cell metadata
   reducedDims(sce) ────  list of cells × N matrices    PCA, UMAP, tSNE
   altExps(sce)     ────  named list of mini SCEs       spike-ins, HTOs, ADT
   metadata(sce)    ────  list of misc                   study-level info
   sizeFactors(sce) ────  vector of length n_cells      normalization factors
             └──────────────────────────────────────────────────┘
```

Rows = features. Columns = cells. (Same convention as `SummarizedExperiment`.)

## Quick Start — Build an SCE

### From a count matrix + metadata

```r
library(SingleCellExperiment)
library(S4Vectors)

# Count matrix: genes × cells (sparse Matrix preferred)
counts <- as(matrix(rpois(100*50, lambda=3), nrow=100, ncol=50),
              "dgCMatrix")
rownames(counts) <- paste0("Gene", 1:100)
colnames(counts) <- paste0("Cell", 1:50)

sce <- SingleCellExperiment(
  assays   = list(counts = counts),
  colData  = DataFrame(label = sample(letters[1:3], 50, replace = TRUE)),
  rowData  = DataFrame(length = rnorm(100, 1000, 200)),
  metadata = list(study = "GSE111111")
)

sce
# class: SingleCellExperiment
# dim: 100 50
# metadata(1): study
# assays(1): counts
# rownames(100): Gene1 Gene2 ... Gene100
# rowData names(1): length
# colnames(50): Cell1 Cell2 ... Cell50
# colData names(1): label
# reducedDimNames(0):
# mainExpName: NULL
# altExpNames(0):
```

### From 10X cellranger output

```r
library(DropletUtils)

sce <- read10xCounts("/path/to/filtered_feature_bc_matrix/")
rownames(sce) <- uniquifyFeatureNames(rowData(sce)$ID, rowData(sce)$Symbol)
sce
```

`read10xCounts` reads either the `filtered` or `raw` (with empty droplets) matrices.

### From a Seurat object

```r
library(Seurat)
seurat_obj <- readRDS("pbmc3k.rds")
sce <- as.SingleCellExperiment(seurat_obj)
# Or for v5 with layers:
sce <- as.SingleCellExperiment(seurat_obj, assay = "RNA")
```

### From a scanpy h5ad

```r
library(zellkonverter)
sce <- readH5AD("data.h5ad")
```

## Accessors

```r
# Assays
counts(sce)             # alias for assay(sce, "counts")
logcounts(sce)          # alias for assay(sce, "logcounts")
assay(sce, "name")
assays(sce)             # SimpleList of all assays

# Metadata
colData(sce)            # cell metadata DataFrame
rowData(sce)            # gene metadata DataFrame
metadata(sce)           # study-level list

# Reduced dimensions
reducedDim(sce, "PCA")  # single reduction
reducedDims(sce)        # SimpleList of all reductions
reducedDimNames(sce)

# Alternative experiments (spike-ins, ADT, HTO)
altExp(sce, "spike-ins")
altExps(sce)
altExpNames(sce)

# Standardized slots
sizeFactors(sce)        # per-cell normalization factors
colLabels(sce)          # per-cell cluster / type labels (convenience)
```

Setters mirror getters:
```r
counts(sce) <- new_count_matrix
colData(sce)$cell_type <- predicted_labels
reducedDim(sce, "PCA") <- pca_coords
altExp(sce, "ADT") <- adt_sce
sizeFactors(sce) <- size_factors
colLabels(sce) <- cluster_ids
```

## Standard scRNA-seq Pipeline — scater + scran

```r
library(scater)
library(scran)
library(scDblFinder)
library(BiocParallel)

# ── 1. Load ────────────────────────────────────────────────────────────────
sce <- read10xCounts("/path/to/filtered_feature_bc_matrix/")
rownames(sce) <- uniquifyFeatureNames(rowData(sce)$ID, rowData(sce)$Symbol)

# ── 2. QC ──────────────────────────────────────────────────────────────────
# Identify mitochondrial genes
is_mt <- grepl("^MT-", rownames(sce))
sce <- addPerCellQC(sce, subsets = list(MT = is_mt))
# Adds colData columns: sum, detected, subsets_MT_sum, subsets_MT_detected, subsets_MT_percent

# Plot QC violins
plotColData(sce, x = "Sample", y = "subsets_MT_percent")
plotColData(sce, x = "sum", y = "detected", colour_by = "subsets_MT_percent")

# Filter using fixed thresholds OR quickPerCellQC's adaptive thresholding
qc <- quickPerCellQC(sce, sub.fields = "subsets_MT_percent")
sce <- sce[, !qc$discard]
message("After QC: ", ncol(sce), " cells")

# ── 3. Doublet detection ───────────────────────────────────────────────────
sce <- scDblFinder(sce)
sce <- sce[, sce$scDblFinder.class == "singlet"]

# ── 4. Normalization (scran's deconvolution method) ────────────────────────
# Cluster cells coarsely first, then compute per-cluster sizeFactors
clust <- quickCluster(sce)
sce <- computeSumFactors(sce, clusters = clust)
sce <- logNormCounts(sce)
# Adds assay(sce, "logcounts")

# ── 5. HVGs ─────────────────────────────────────────────────────────────────
dec <- modelGeneVar(sce)
hvgs <- getTopHVGs(dec, n = 2000)

# ── 6. PCA + UMAP ──────────────────────────────────────────────────────────
library(BiocSingular)
set.seed(1)
sce <- runPCA(sce, subset_row = hvgs, ncomponents = 30,
              BSPARAM = IrlbaParam())   # fast PCA
sce <- runUMAP(sce, dimred = "PCA")

# ── 7. Clustering ──────────────────────────────────────────────────────────
library(bluster)
sce$cluster <- clusterCells(sce, use.dimred = "PCA",
                              BLUSPARAM = NNGraphParam(cluster.fun = "louvain"))
colLabels(sce) <- sce$cluster

# Or use a higher-level wrapper from scran:
# sce$cluster <- clusterSNNGraph(sce, use.dimred = "PCA")

# ── 8. Marker genes ────────────────────────────────────────────────────────
markers <- findMarkers(sce, groups = sce$cluster,
                        direction = "up", lfc = 0.25)
# markers is a List of DataFrames — one per cluster
top_markers <- lapply(markers, function(x) head(rownames(x), 10))

# ── 9. Visualize ──────────────────────────────────────────────────────────
plotUMAP(sce, colour_by = "cluster", text_by = "cluster")
plotUMAP(sce, colour_by = "CD3D")
plotHeatmap(sce, features = unlist(top_markers),
            order_columns_by = "cluster", center = TRUE)
plotExpression(sce, features = c("CD3D", "MS4A1"), x = "cluster")

# ── 10. Save ───────────────────────────────────────────────────────────────
saveRDS(sce, "sce_processed.rds")
```

Convenience: `Rscript scripts/build_sce.R --input data/filtered_feature_bc_matrix --out sce.rds`.

## Alternative Experiments — Spike-ins, HTOs, ADT

When you have additional features that aren't endogenous mRNA (ERCC spike-ins, CITE-seq antibody capture, hashtag oligos), store them as `altExps` so they don't pollute the main assay.

```r
# Suppose `sce` has all features (genes + ERCC + ADT) in one matrix
is_ercc <- grepl("^ERCC-", rownames(sce))
is_adt  <- grepl("^A-",    rownames(sce))

# Split into altExps — main keeps endogenous genes only
sce <- splitAltExps(sce,
                     f = ifelse(is_ercc, "ERCC",
                                ifelse(is_adt, "ADT", "Gene"))
                    )

# The main experiment now has only endogenous genes:
sce
# altExpNames(2): ERCC ADT

# Access each:
altExp(sce, "ERCC")     # mini SCE with ERCC spike-ins
altExp(sce, "ADT")      # mini SCE with antibody-capture data
```

After splitting, subsetting `sce[, cell_subset]` automatically subsets all altExps too — they stay in sync with the cell columns.

## Iteration with `applySCE`

For functions you want to apply to **both** the main experiment and each altExp:

```r
# A simple per-experiment function
total_counts <- function(x) colSums(counts(x))

# Apply to main + each altExp at once
totals <- applySCE(sce, FUN = total_counts)
# totals is a List of per-altExp results

# With shared arguments
totals <- applySCE(sce, FUN = total_counts, multiplier = 10)

# With per-altExp argument overrides
totals <- applySCE(
  sce, FUN = total_counts,
  multiplier = 10,
  ALT.ARGS = list(
    ERCC = list(subset.row = 2),
    ADT  = list(subset.row = 3:5)
  )
)
```

When `FUN` returns an SCE, the default `SIMPLIFY = TRUE` reorganizes the results back into a single SCE.

## Multi-Sample Integration with batchelor

```r
library(batchelor)

# Each sample is one SCE, with a `batch` column in colData
sce1$batch <- "donor1"
sce2$batch <- "donor2"
sce3$batch <- "donor3"

# Pre-process per-sample (HVG + log-norm) before integration
sce_list <- lapply(list(sce1, sce2, sce3), function(s) {
  s <- logNormCounts(s)
  s
})

# MNN integration
merged <- fastMNN(sce_list, k = 20, d = 50)
# merged is an SCE with reducedDim "corrected"

# Then standard downstream on the merged object
merged <- runUMAP(merged, dimred = "corrected")
plotUMAP(merged, colour_by = "batch")
```

For Harmony integration:
```r
library(harmony)
merged <- RunHarmony(merged, group.by.vars = "batch", reduction = "PCA")
```

## Interop with Seurat / scanpy

```r
# SCE → Seurat
library(Seurat)
seurat_obj <- as.Seurat(sce, counts = "counts", data = "logcounts")

# Seurat → SCE
sce <- as.SingleCellExperiment(seurat_obj)

# SCE → h5ad (scanpy)
library(zellkonverter)
writeH5AD(sce, "data.h5ad")

# h5ad → SCE
sce <- readH5AD("data.h5ad")
```

`zellkonverter` is the canonical bridge between SCE and AnnData. It preserves assays, colData, reducedDims, and altExps.

## Key Parameters

### `addPerCellQC(subsets = ...)`
- Always include the MT subset; optionally ribosomal (`"^RP[SL]"` for human)

### `quickCluster()` (before `computeSumFactors`)
- Pre-clustering quality affects size-factor accuracy. Use `min.size = 200` for stable results.

### `modelGeneVar()` → `getTopHVGs(n = 2000)`
- 2000 is fine for most analyses. Bump to 4000 for highly heterogeneous tissues.

### `runPCA(BSPARAM = IrlbaParam())`
- Use `IrlbaParam()` for fast irlba-based PCA. Default exact PCA is slow on large datasets.

### `clusterCells(BLUSPARAM = NNGraphParam(...))`
- `cluster.fun = "louvain"` or `"leiden"`. Leiden requires the `leiden` package + a small bit of setup.

## Best Practices

- **Use sparse matrices.** Store counts as `dgCMatrix` (Matrix package). Dense matrices for ≥ 10k cells blow up memory.
- **`scran::computeSumFactors` is statistically better than library-size normalization.** Don't replace it with raw library-size sizeFactors unless you have a specific reason.
- **`uniquifyFeatureNames(rowData$ID, rowData$Symbol)`** after `read10xCounts` — handles duplicate gene symbols (which are common).
- **Always run scDblFinder on per-sample SCEs before integration.** Cross-sample doublet detection can be misleading.
- **Use `BiocParallel::MulticoreParam(workers = N)` for parallelizable steps** like `findMarkers` and `scDblFinder` — large speedups.
- **For storage, `HDF5Array` + `DelayedArray` backed SCEs** keep counts on disk. Critical for atlas-scale datasets that don't fit in RAM.
- **For interop, `zellkonverter::writeH5AD / readH5AD`** is the canonical bridge to scanpy.

## Developer Notes — Subclassing SCE

If you're writing a package that extends SCE (e.g. a new modality container), use the internal field protection pattern:

```r
# DON'T directly assign to slots — packages can collide on slot names.
# DO use the int_colData / int_metadata helpers, nested under your package name:

setX_good <- function(sce, x) {
  # Store under your package namespace
  int_colData(sce)$mypackage <- DataFrame(X = x)
  sce
}

# Read back the same way
getX <- function(sce) int_colData(sce)$mypackage$X
```

This "inception-style" nesting prevents collision with other packages that may want to use the same field name.

Source: [Developer vignette](https://bioconductor.org/packages/release/bioc/vignettes/SingleCellExperiment/inst/doc/devel.html). See [references/dev_patterns.md](references/dev_patterns.md) for full subclassing patterns.

## End-to-End Template

`assets/sce_template.R` — single parameterized script through the full scater + scran pipeline.

## Convenience Scripts

- `scripts/build_sce.R` — load 10X / .h5 / Seurat → SCE → standard pipeline → save

## References

- [SingleCellExperiment Bioconductor page](https://bioconductor.org/packages/release/bioc/html/SingleCellExperiment.html)
- [Intro vignette](https://bioconductor.org/packages/release/bioc/vignettes/SingleCellExperiment/inst/doc/intro.html)
- [Apply vignette](https://bioconductor.org/packages/release/bioc/vignettes/SingleCellExperiment/inst/doc/apply.html)
- [Developer vignette](https://bioconductor.org/packages/release/bioc/vignettes/SingleCellExperiment/inst/doc/devel.html)
- [Orchestrating Single-Cell Analysis with Bioconductor (OSCA) book](https://bioconductor.org/books/release/OSCA/) — the comprehensive Bioconductor scRNA-seq guide
- Amezquita et al. (2020), *Orchestrating single-cell analysis with Bioconductor*, *Nature Methods*
- Related Operon protocols: [`seurat`](../seurat/SKILL.md) (sister container), [`hdwgcna`](../hdwgcna/SKILL.md), [`cellchat`](../cellchat/SKILL.md), [`archr`](../archr/SKILL.md)
