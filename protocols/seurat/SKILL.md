---
name: seurat
description: scRNA-seq analysis with Seurat v5 (R) — the standard R-based pipeline. Covers QC, normalization (LogNormalize + SCTransform), HVG selection, scaling, PCA, neighbors, leiden/Louvain clustering, UMAP/t-SNE, marker gene identification (FindMarkers / FindAllMarkers), and visualization (DimPlot / FeaturePlot / VlnPlot / DotPlot / DoHeatmap). Multi-sample integration via Seurat v5's IntegrateLayers — CCA, RPCA, Harmony, FastMNN, or scVI. Optional dependencies (presto, BPCells, glmGamPoi) substantially speed up large datasets.
license: MIT
metadata:
---

# Seurat v5: scRNA-seq Analysis in R

## Overview

[Seurat](https://satijalab.org/seurat/) is the de facto R-based scRNA-seq pipeline from the Satija Lab. It handles the full workflow from filtered count matrices through clustering and marker analysis, plus multi-sample integration, multimodal data (CITE-seq), and integration with downstream tools like Azimuth (label transfer), Signac (chromatin), and SeuratWrappers (additional integration methods).

**Seurat v5** (released 2023, current as of v5.x) introduces a few important architectural shifts from v4:

1. **Layers** — assays are split into named layers (e.g. `counts`, `data`, `scale.data`), and integration happens across layers rather than across separate objects.
2. **`IntegrateLayers()`** — one call replaces the v4 `SelectIntegrationFeatures → FindIntegrationAnchors → IntegrateData` chain. Supports CCA, RPCA, Harmony, FastMNN, and scVI through a single API.
3. **BPCells backend** — out-of-memory matrices for atlas-scale datasets (millions of cells on modest RAM).
4. **`presto`** — accelerated `FindAllMarkers` (10-100× faster).

## When to Use This Skill

- Standard R-based scRNA-seq analysis from filtered 10X matrices, h5, or count tables
- Multi-sample / multi-condition integration where you want canonical anchor-based or Harmony/scVI integration in R
- CITE-seq / multimodal analysis (Seurat's WNN — weighted nearest neighbors)
- Cell-type annotation via Azimuth (Satija Lab's reference atlas) or manual markers
- Workflows that need to integrate with the broader R/Bioconductor ecosystem (deseq2, hdwgcna, cellchat)

**Not for**:
- Pure Python ecosystems — use `scanpy`
- ATAC-seq — use `archr` or `snapatac2`
- Spatial transcriptomics — use `spatial-transcriptomics` protocol
- Atlas-scale (≥ 5M cells) with limited RAM — even with BPCells, scVI / scanpy in Python tend to scale better

## Prerequisites

- R ≥ 4.0 (R ≥ 4.3 recommended)
- ~32 GB RAM for ~100k cells; ~64-128 GB for ~500k+ (lower with BPCells)
- Linux / macOS preferred for performance; Windows works for analysis

### Installation

```r
# Core Seurat
install.packages('Seurat')
library(Seurat)
packageVersion('Seurat')   # check you're on v5.x

# Strongly-recommended performance dependencies
setRepositories(ind = 1:3, addURLs = c(
  'https://satijalab.r-universe.dev',
  'https://bnprks.r-universe.dev/'
))
install.packages(c("BPCells", "presto", "glmGamPoi"))

# Optional companion packages
install.packages('Signac')                                       # chromatin
remotes::install_github("satijalab/seurat-data",    quiet = TRUE)
remotes::install_github("satijalab/azimuth",        quiet = TRUE)
remotes::install_github("satijalab/seurat-wrappers", quiet = TRUE)

# Integration backends used by IntegrateLayers
install.packages('harmony')
BiocManager::install('batchelor')                                # FastMNN
# scVI integration needs a separate conda env — see Integration section
```

## Quick Start — Standard Single-Sample Pipeline

This is the PBMC 3K tutorial in compressed form. Substitute your own data path.

```r
library(Seurat)
library(dplyr)
library(patchwork)
set.seed(1)

# ── 1. Load data ────────────────────────────────────────────────────────────
# From cellranger (10X) — directory of barcodes/features/matrix.mtx.gz
pbmc.data <- Read10X(data.dir = "/path/to/filtered_feature_bc_matrix/")
# Or from h5: Read10X_h5("filtered_feature_bc_matrix.h5")

pbmc <- CreateSeuratObject(
  counts       = pbmc.data,
  project      = "pbmc3k",
  min.cells    = 3,         # gene must be in ≥ 3 cells
  min.features = 200        # cell must have ≥ 200 features
)

# ── 2. QC ───────────────────────────────────────────────────────────────────
pbmc[["percent.mt"]] <- PercentageFeatureSet(pbmc, pattern = "^MT-")
# (For mouse: pattern = "^mt-"; for nuclei: include ribosomal too with "^Rp[ls]")

VlnPlot(pbmc, features = c("nFeature_RNA", "nCount_RNA", "percent.mt"), ncol = 3)
FeatureScatter(pbmc, feature1 = "nCount_RNA", feature2 = "percent.mt")
FeatureScatter(pbmc, feature1 = "nCount_RNA", feature2 = "nFeature_RNA")

pbmc <- subset(pbmc, subset = nFeature_RNA > 200 &
                              nFeature_RNA < 2500 &
                              percent.mt   < 5)

# ── 3. Normalization + HVG ─────────────────────────────────────────────────
pbmc <- NormalizeData(pbmc, normalization.method = "LogNormalize", scale.factor = 10000)
pbmc <- FindVariableFeatures(pbmc, selection.method = "vst", nfeatures = 2000)

# Alternative: SCTransform (better for low-depth / variable-depth data)
# pbmc <- SCTransform(pbmc, vars.to.regress = "percent.mt", verbose = FALSE)

# ── 4. Scale + PCA ─────────────────────────────────────────────────────────
pbmc <- ScaleData(pbmc, features = rownames(pbmc))
pbmc <- RunPCA(pbmc, features = VariableFeatures(object = pbmc))

ElbowPlot(pbmc, ndims = 50)     # pick n_pcs from the elbow

# ── 5. Neighbors + clusters + UMAP ─────────────────────────────────────────
pbmc <- FindNeighbors(pbmc, dims = 1:10)
pbmc <- FindClusters(pbmc, resolution = 0.5)
pbmc <- RunUMAP(pbmc, dims = 1:10)

DimPlot(pbmc, reduction = "umap", label = TRUE)

# ── 6. Marker genes ─────────────────────────────────────────────────────────
# All clusters at once — fast with presto installed
markers <- FindAllMarkers(pbmc, only.pos = TRUE,
                           min.pct = 0.25,
                           logfc.threshold = 0.25)

# Top 5 markers per cluster
markers %>% group_by(cluster) %>% slice_max(n = 5, order_by = avg_log2FC)

# Specific pairwise comparison
clust5_vs_03 <- FindMarkers(pbmc, ident.1 = 5, ident.2 = c(0, 3))

# ── 7. Visualize markers ───────────────────────────────────────────────────
VlnPlot(pbmc, features = c("MS4A1", "CD79A"))                       # B-cell markers
FeaturePlot(pbmc, features = c("CD3D", "MS4A1", "CD14", "LYZ"))     # canonical PBMC
DoHeatmap(pbmc, features = markers %>% group_by(cluster) %>%
                            slice_max(n = 10, order_by = avg_log2FC) %>%
                            pull(gene)) + NoLegend()

# ── 8. Cell-type annotation ────────────────────────────────────────────────
new.cluster.ids <- c("Naive CD4 T", "CD14+ Mono", "Memory CD4 T", "B", "CD8 T",
                      "FCGR3A+ Mono", "NK", "DC", "Platelet")
names(new.cluster.ids) <- levels(pbmc)
pbmc <- RenameIdents(pbmc, new.cluster.ids)
DimPlot(pbmc, reduction = "umap", label = TRUE, pt.size = 0.5) + NoLegend()

# ── 9. Save ─────────────────────────────────────────────────────────────────
saveRDS(pbmc, "pbmc3k_final.rds")
```

Convenience: `Rscript scripts/build_seurat.R --input data/filtered_feature_bc_matrix --out pbmc.rds`.

Source: [PBMC 3K tutorial](https://satijalab.org/seurat/articles/pbmc3k_tutorial).

---

## Multi-Sample Integration — Seurat v5

The v5 way: keep everything in one Seurat object, split the RNA assay into per-sample layers, run preprocessing on the joined object, then `IntegrateLayers()`.

```r
library(Seurat)

# Assume `obj` is a Seurat object with sample membership in obj$Method (or any column)
obj[["RNA"]] <- split(obj[["RNA"]], f = obj$Method)

# Standard preprocessing on the now-layered object
obj <- NormalizeData(obj)
obj <- FindVariableFeatures(obj)
obj <- ScaleData(obj)
obj <- RunPCA(obj)

# Single-line integration — pick a method
obj <- IntegrateLayers(
  object         = obj,
  method         = CCAIntegration,         # one of: CCAIntegration | RPCAIntegration | HarmonyIntegration | FastMNNIntegration | scVIIntegration
  orig.reduction = "pca",
  new.reduction  = "integrated.cca",
  verbose        = FALSE
)

# Re-cluster / re-UMAP on the integrated embedding
obj <- FindNeighbors(obj, reduction = "integrated.cca", dims = 1:30)
obj <- FindClusters (obj, resolution = 1)
obj <- RunUMAP     (obj, dims = 1:30, reduction = "integrated.cca")

# Rejoin layers for downstream (DE on the integrated object)
obj[["RNA"]] <- JoinLayers(obj[["RNA"]])
```

### Choosing an integration method

| Method | When |
|---|---|
| `CCAIntegration` | Default, well-tested. Good for balanced batches. |
| `RPCAIntegration` | Faster than CCA, recommended when batches are very different. |
| `HarmonyIntegration` | Fast, scalable. Most popular for large cohorts. |
| `FastMNNIntegration` | When you need mutual-nearest-neighbor logic. |
| `scVIIntegration` | Deep VAE; best for very heterogeneous / large datasets. Needs a separate conda environment. |

### SCTransform path

```r
obj[["RNA"]] <- split(obj[["RNA"]], f = obj$Method)
obj <- SCTransform(obj)
obj <- RunPCA(obj)

obj <- IntegrateLayers(
  object               = obj,
  method               = CCAIntegration,
  normalization.method = "SCT",
  verbose              = FALSE
)

obj <- FindNeighbors(obj, reduction = "integrated.dr", dims = 1:30)
obj <- FindClusters (obj, resolution = 0.6)
obj <- RunUMAP     (obj, dims = 1:30, reduction = "integrated.dr")

# IMPORTANT: SCT residuals are per-sample; recompute before DE
obj <- PrepSCTFindMarkers(obj)
```

Source: [v5 integration](https://satijalab.org/seurat/articles/seurat5_integration). For the v4 anchor-based workflow + when each style applies, see [references/integration.md](references/integration.md).

---

## Visualization Cookbook

The 7 core plotting functions and when to use each:

```r
# DimPlot — UMAP / t-SNE / PCA scatter (cluster colors)
DimPlot(pbmc, reduction = "umap", label = TRUE, group.by = "seurat_clusters")
DimPlot(pbmc, reduction = "umap", split.by = "condition")    # side-by-side per condition

# FeaturePlot — gene expression on UMAP
FeaturePlot(pbmc, features = c("CD3D", "MS4A1"),
             min.cutoff = "q10", max.cutoff = "q90")          # clip outliers via quantile
FeaturePlot(pbmc, features = c("CD4", "CD8A"), blend = TRUE)   # co-expression

# VlnPlot — per-cluster violin distributions
VlnPlot(pbmc, features = c("MS4A1", "CD79A"))
VlnPlot(pbmc, features = "MS4A1", split.by = "condition")     # per-cluster × per-condition

# RidgePlot — same content as VlnPlot, ridge style
RidgePlot(pbmc, features = c("CD3D", "MS4A1"), ncol = 2)

# DotPlot — many markers × many clusters
DotPlot(pbmc, features = c("CD3D", "MS4A1", "CD14", "NKG7")) + RotatedAxis()

# DoHeatmap — single-cell heatmap, downsampled for speed
DoHeatmap(subset(pbmc, downsample = 100), features = top10_markers, size = 3) + NoLegend()

# FeatureScatter — two features against each other
FeatureScatter(pbmc, feature1 = "LYZ", feature2 = "CCL5")

# Helpers
NoLegend()                                            # remove legend
NoAxes()                                              # remove axes
DarkTheme()                                           # dark background
LabelClusters(plot = p, id = "ident")                 # label clusters in-place
LabelPoints(plot = plot1, points = top10, repel = TRUE)
HoverLocator(plot = p)                                # interactive tooltips
```

Combine with patchwork:
```r
plot1 + plot2                              # side by side
plot1 / plot2                              # stacked
(plot1 + plot2) & NoLegend()               # apply NoLegend to both
```

See [references/visualization.md](references/visualization.md) for cookbook patterns: multi-panel figures, custom palettes, publication-ready output.

Source: [visualization vignette](https://satijalab.org/seurat/articles/visualization_vignette).

---

## Key Parameters to Adjust

### QC subset thresholds
- `nFeature_RNA > 200` (cells with too few genes are doublets / empty)
- `nFeature_RNA < 2500` (heuristic doublet cap; raise for nuclei or rich tissues)
- `percent.mt < 5` (PBMC); raise to 15-20% for nuclei / tumor tissue

### `FindVariableFeatures`
- `nfeatures = 2000` (default); 3000-5000 if your tissue is heterogeneous

### PCA / `dims`
- Pick `dims` from `ElbowPlot()` — usually 10-30. Don't over-pick (noise dimensions).

### `FindClusters(resolution = ...)`
- 0.5 → coarse clusters (cell types)
- 1.0-1.5 → finer sub-clusters (cell states)
- For DE / marker analysis, start coarser; for trajectory / niche, go finer.

### `IntegrateLayers(method = ...)`
- See the table above. Try Harmony first (fast); fall back to CCA/RPCA if Harmony over-corrects.

---

## Best Practices

- **Use Seurat v5 layers, not v4 separate objects.** Splitting via `split(obj[["RNA"]], f = ...)` is the new idiom.
- **Install `presto` immediately.** `FindAllMarkers` is the slow step in most analyses; presto speeds it up 10-100×.
- **Use `BPCells` for atlas-scale.** Out-of-memory matrices let you analyze millions of cells on 32 GB.
- **Run `JoinLayers()` before downstream DE.** After integration, the RNA assay still has per-sample layers; some functions expect a single joined layer.
- **`PrepSCTFindMarkers()` is mandatory before DE on SCT-integrated data.** Without it, `FindMarkers` produces wrong results.
- **`percent.mt` cutoff varies by tissue.** PBMC: 5%. Nuclei: 1% (nuclei have no mito RNA). Tumor: 15-20% (lytic cells).
- **Save intermediate `.rds` files** between major stages — Seurat objects can be large and re-running clusters can change cluster IDs if `set.seed` isn't pinned.

---

## End-to-End Template

`assets/seurat_template.R` — single parameterized script for single-sample OR multi-sample (via INPUT_MODE toggle) through to integration, clusters, markers, and annotated output.

## Convenience Scripts

- `scripts/build_seurat.R` — single-sample standard pipeline (10X / h5 / Read10X) → annotated RDS
- `scripts/integrate_seurat.R` — multi-sample v5 IntegrateLayers (any of the 5 methods)

---

## References

- [Seurat home](https://satijalab.org/seurat/) — Satija Lab
- [Installation v5](https://satijalab.org/seurat/articles/install_v5)
- [PBMC 3K tutorial](https://satijalab.org/seurat/articles/pbmc3k_tutorial) — the canonical entry point
- [v5 integration](https://satijalab.org/seurat/articles/seurat5_integration) — `IntegrateLayers`
- [v4 integration](https://satijalab.org/seurat/articles/integration_introduction.html) — anchor-based, still supported
- [Visualization vignette](https://satijalab.org/seurat/articles/visualization_vignette)
- Hao et al. (2024), *Dictionary learning for integrative, multimodal and scalable single-cell analysis*, *Nature Biotechnology* (Seurat v5 paper)
- Related Operon protocols: [`hdwgcna`](../hdwgcna/SKILL.md), [`cellchat`](../cellchat/SKILL.md), [`singlecellexperiment`](../singlecellexperiment/SKILL.md)
