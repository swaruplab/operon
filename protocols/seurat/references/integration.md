# Seurat Integration — v4 vs v5, and Method Choice

Two API surfaces are worth knowing: the v4 **anchor-based** workflow (still supported and used in many tutorials) and the v5 **`IntegrateLayers`** workflow (recommended for new analyses). Mechanically they call similar algorithms; structurally v5 is much cleaner.

## v5 — Layers + `IntegrateLayers` (recommended)

The v5 workflow keeps every sample's cells in one Seurat object. Sample identity lives as a per-cell metadata column. The RNA assay is split into named **layers** (one per sample) before integration and joined back after.

```r
# All cells from all samples in one object — already standard practice in scanpy
obj <- merge(seurat1, y = c(seurat2, seurat3, seurat4), add.cell.ids = c("s1","s2","s3","s4"))

# Sample column must exist in obj@meta.data
obj$sample <- substr(rownames(obj@meta.data), 1, 2)

# Split the RNA assay into per-sample layers
obj[["RNA"]] <- split(obj[["RNA"]], f = obj$sample)
# Layers in obj[["RNA"]] are now: counts.s1, counts.s2, counts.s3, counts.s4

# Standard preprocessing — Seurat runs each layer independently
obj <- NormalizeData(obj)
obj <- FindVariableFeatures(obj)
obj <- ScaleData(obj)
obj <- RunPCA(obj)
```

At this point `pca` is the **unintegrated** embedding — useful baseline for "before integration" plots.

```r
DimPlot(obj, reduction = "pca", group.by = "sample") + ggtitle("Before integration")
```

Now integrate:

```r
obj <- IntegrateLayers(
  object         = obj,
  method         = HarmonyIntegration,      # or CCA, RPCA, FastMNN, scVI
  orig.reduction = "pca",
  new.reduction  = "harmony",
  verbose        = FALSE
)

# Downstream uses the new corrected embedding
obj <- FindNeighbors(obj, reduction = "harmony", dims = 1:30)
obj <- FindClusters (obj, resolution = 1)
obj <- RunUMAP     (obj, dims = 1:30, reduction = "harmony")

DimPlot(obj, reduction = "umap", group.by = "sample") + ggtitle("After Harmony")
DimPlot(obj, reduction = "umap", group.by = "seurat_clusters", label = TRUE)
```

Finally, rejoin layers for DE / downstream:

```r
obj[["RNA"]] <- JoinLayers(obj[["RNA"]])
```

After `JoinLayers`, `FindMarkers` / `FindAllMarkers` work on the unified count matrix.

## v4 — anchor-based (still works, but verbose)

The v4 idiom takes a **list** of per-sample Seurat objects, finds integration anchors, and produces a single integrated assay.

```r
# List of per-sample objects, each preprocessed independently
sample_list <- list(s1 = seurat1, s2 = seurat2, s3 = seurat3, s4 = seurat4)
sample_list <- lapply(sample_list, function(x) {
  x <- NormalizeData(x)
  x <- FindVariableFeatures(x, selection.method = "vst", nfeatures = 2000)
})

features <- SelectIntegrationFeatures(object.list = sample_list, nfeatures = 2000)

anchors <- FindIntegrationAnchors(
  object.list    = sample_list,
  anchor.features = features,
  reduction       = "cca",                 # or "rpca" — faster, recommended for big datasets
  dims            = 1:30
)

obj <- IntegrateData(
  anchorset            = anchors,
  normalization.method = "LogNormalize",   # or "SCT" for the SCT path
  dims                 = 1:30
)

# Now obj has a new "integrated" assay alongside the original "RNA" assay
DefaultAssay(obj) <- "integrated"
obj <- ScaleData(obj)
obj <- RunPCA(obj, npcs = 30)
obj <- FindNeighbors(obj, dims = 1:30)
obj <- FindClusters (obj, resolution = 0.5)
obj <- RunUMAP     (obj, dims = 1:30)

# For DE, switch back to RNA assay
DefaultAssay(obj) <- "RNA"
```

The v4 path is fine if you've inherited code that uses it — there's no urgent reason to rewrite. New analyses should use v5.

## Method choice — `IntegrateLayers(method = ...)`

| Method | Speed | Memory | Quality on highly different batches | Notes |
|---|---|---|---|---|
| `CCAIntegration` | Slow | High | Best | The default. Anchor-based. |
| `RPCAIntegration` | Fast | Medium | Good (better when batches differ a lot) | Recommended over CCA for ≥ 100k cells |
| `HarmonyIntegration` | Fastest | Low | Good | Most popular; very fast even on millions of cells |
| `FastMNNIntegration` | Medium | Medium | Good | Mutual-nearest-neighbor style |
| `scVIIntegration` | Slow (training) | High | Best on heterogeneous data | Needs Python + scvi-tools via reticulate |

**My pick by scenario**:

- **Default** (< 100k cells, 2-10 samples): Harmony. It's fast, well-tested, scales linearly.
- **Heterogeneous tissues / strong batch effects**: RPCA. Anchor-based is more conservative than Harmony.
- **Very large cohorts** (≥ 500k cells, ≥ 50 samples): scVI. The deep model handles complex cohort structures.
- **CITE-seq or multimodal**: stay with the v5 layers framework; Seurat's WNN handles modality fusion separately.

## SCT vs LogNormalize integration

`SCTransform` is Seurat's regularized negative-binomial normalization. For integration:

```r
# Per-sample SCT
obj_list <- SplitObject(obj, split.by = "sample")
obj_list <- lapply(obj_list, function(x) SCTransform(x, vars.to.regress = "percent.mt"))

# v4 SCT integration
features <- SelectIntegrationFeatures(object.list = obj_list, nfeatures = 3000)
obj_list <- PrepSCTIntegration(object.list = obj_list, anchor.features = features)
anchors  <- FindIntegrationAnchors(object.list = obj_list,
                                    normalization.method = "SCT",
                                    anchor.features = features)
obj <- IntegrateData(anchorset = anchors, normalization.method = "SCT")

# v5 SCT integration
# (You can also just call SCTransform on the layered object and pass normalization.method = "SCT" to IntegrateLayers)
obj <- SCTransform(obj)
obj <- RunPCA(obj)
obj <- IntegrateLayers(
  object               = obj,
  method               = CCAIntegration,
  normalization.method = "SCT",
  verbose              = FALSE
)
```

### When to use SCT
- Highly variable library sizes between samples
- Low-depth data (10X v2 chemistry, drop-seq)
- When `LogNormalize + ScaleData` produces visible technical artifacts

### When NOT to use SCT
- Very large datasets — SCT is much slower than LogNormalize
- Datasets where downstream tools require log-normalized counts in `data` (some integrations expect that)

## Critical post-integration steps

### `JoinLayers()` for DE

Marker / DE functions expect a single `counts` and `data` matrix, not per-sample layers. After v5 integration:

```r
obj[["RNA"]] <- JoinLayers(obj[["RNA"]])
markers <- FindAllMarkers(obj)
```

Skipping this leads to confusing errors or weirdly partial DE tables.

### `PrepSCTFindMarkers()` for SCT-integrated data

When integration was on the SCT assay, SCT residuals were computed per-sample. `FindMarkers` on SCT residuals across samples is **wrong** unless you recompute first:

```r
obj <- PrepSCTFindMarkers(obj)
markers <- FindMarkers(obj, ident.1 = "T_CD4", ident.2 = "T_CD8", assay = "SCT")
```

For LogNormalize-integrated data, this step is not needed.

## Diagnostic plots

```r
# Sample-mixing on UMAP — should be uniform after integration
DimPlot(obj, reduction = "umap", group.by = "sample")

# Per-cluster sample composition — should be balanced unless biology says otherwise
prop.table(table(obj$seurat_clusters, obj$sample), margin = 1)

# Inspect specific marker genes across samples
FeaturePlot(obj, features = c("CD3D", "MS4A1"), split.by = "sample")

# Cluster-level marker concordance across samples
DotPlot(obj, features = c("CD3D", "MS4A1", "CD14"),
        group.by = "seurat_clusters", split.by = "sample")
```

If samples segregate within clusters after integration → under-integrated; bump `dims`, try a different method.
If real cell types disappear after integration → over-integrated; revert to RPCA, lower `theta` for Harmony.

## Common pitfalls

- **Forgetting `JoinLayers()` before DE** — `FindMarkers` errors or returns weird partial results.
- **Forgetting `PrepSCTFindMarkers()`** for SCT-integrated data — DE results are silently wrong.
- **Setting `DefaultAssay(obj) <- "integrated"` for DE** in v4. Integrated assay is for embedding only; switch back to `"RNA"` before DE.
- **Splitting by a column with `NA` values** — `IntegrateLayers` errors if any cells have NA in the split column.
- **Mixing v4 and v5 idioms** in one analysis. Pick one and stay with it.
