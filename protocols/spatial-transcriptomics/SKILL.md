---
name: spatial-transcriptomics
description: Spatial transcriptomics analysis pipeline using squidpy + scanpy. Handles Visium, Xenium, CosMx, MERFISH, Slide-seq, and GeoMx data. Covers loading, quantile-based QC, normalization, clustering, spatial neighborhood analysis (enrichment, co-occurrence, spatially variable genes), and visualization. Includes minimal sections on cell-cell communication, niche detection, and Visium deconvolution.
license: BSD-3-Clause license
metadata:
---

# Spatial Transcriptomics: Squidpy + Scanpy Pipeline

## Overview

End-to-end pipeline for spatially-resolved transcriptomics. Combines [scanpy](https://scanpy.readthedocs.io) for general single-cell preprocessing with [squidpy](https://squidpy.readthedocs.io) for spatial-aware analyses (neighborhood graphs, co-occurrence, spatially variable genes, image overlay). Platform-aware loading covers the major commercial and academic technologies.

## When to Use This Skill

- Loading data from any of: Visium (10X), Xenium (10X), CosMx (Nanostring), MERFISH, Slide-seq, GeoMx (Nanostring)
- Quality control adapted to each platform's properties (spot vs. cell resolution, gene panel size, segmentation accuracy)
- Standard preprocessing (normalization, HVG, PCA, neighbors, leiden) — same as scRNA-seq
- Spatial-specific analyses: neighborhood enrichment, co-occurrence, Moran's I / Geary's C
- Visualizing spatial gene expression and cluster maps, with H&E overlay for Visium / Xenium
- Light-touch entry points for niche detection, cell-cell communication, and deconvolution (deeper analysis lives in dedicated protocols)

## Quick Start

### Imports and settings

```python
import scanpy as sc
import squidpy as sq
import pandas as pd
import numpy as np
import matplotlib.pyplot as plt

sc.settings.verbosity = 3
sc.settings.set_figure_params(dpi=80, facecolor='white')
sc.settings.figdir = './figures/'
```

### Loading data — pick your platform

```python
# Visium (10X) — spot-level, H&E image included
adata = sc.read_visium('path/to/spaceranger_outs/', count_file='filtered_feature_bc_matrix.h5')

# Xenium (10X) — subcellular, hundreds of genes, segmentation polygons
adata = sq.read.xenium('path/to/xenium_output/')

# CosMx (Nanostring) — high-plex (~1000 genes), FOV-based
adata = sq.read.nanostring(
    'path/to/cosmx_output/',
    counts_file='exprMat_file.csv',
    meta_file='metadata_file.csv',
)

# MERFISH — varies by source; common path is a Vizgen output dir
adata = sq.read.vizgen(
    'path/to/merfish_output/',
    counts_file='cell_by_gene.csv',
    meta_file='cell_metadata.csv',
)

# Slide-seq — bead-level, often h5ad or CSV
adata = sc.read_h5ad('path/to/slideseq.h5ad')
# Spatial coords must live in adata.obsm['spatial'] as an (n_obs, 2) array.

# GeoMx (Nanostring) — region-of-interest (ROI) profiling, not single cell
# Usually loaded from a count matrix + ROI metadata CSV
adata = sc.read_csv('path/to/geomx_counts.csv').T  # genes × ROI → ROI × genes
adata.obs = pd.read_csv('path/to/geomx_metadata.csv', index_col=0)
```

**Where coordinates live, by platform:**

| Platform | `adata.obsm['spatial']` | Image data |
|---|---|---|
| Visium | spot pixel coords | `adata.uns['spatial'][library_id]['images']` (H&E) |
| Xenium | cell centroids (µm) | morphology image in `adata.uns['spatial']` |
| CosMx | cell centroids (px) | optional IF images in `adata.uns['spatial']` |
| MERFISH | cell centroids (µm) | none by default |
| Slide-seq | bead coords (µm) | none |
| GeoMx | ROI centroids | per-ROI images optional |

Full per-platform details, including frequent gotchas, in [references/loading_guide.md](references/loading_guide.md).

### Quality control (quantile-based, platform-aware)

Hard cutoffs (e.g. `min_genes=200`) are wrong for spatial: Xenium panels have ~300 genes total, CosMx ~1000, Visium spots cover multiple cells. Use quantile thresholds derived from this dataset's own distribution.

```python
# Per-cell / per-spot QC
adata.var['mt'] = adata.var_names.str.startswith(('MT-', 'mt-', 'Mt-'))
sc.pp.calculate_qc_metrics(adata, qc_vars=['mt'], percent_top=None,
                            log1p=False, inplace=True)

# Quantile thresholds — adapt to each platform
gene_lo = np.quantile(adata.obs['n_genes_by_counts'], 0.05)
gene_hi = np.quantile(adata.obs['n_genes_by_counts'], 0.99)
count_lo = np.quantile(adata.obs['total_counts'], 0.05)
count_hi = np.quantile(adata.obs['total_counts'], 0.99)
mt_hi = min(np.quantile(adata.obs['pct_counts_mt'], 0.99), 20.0)

print(f"QC thresholds: n_genes ∈ [{gene_lo:.0f}, {gene_hi:.0f}], "
      f"total_counts ∈ [{count_lo:.0f}, {count_hi:.0f}], "
      f"pct_counts_mt < {mt_hi:.2f}")

adata = adata[(adata.obs['n_genes_by_counts'] >= gene_lo) &
              (adata.obs['n_genes_by_counts'] <= gene_hi) &
              (adata.obs['total_counts'] >= count_lo) &
              (adata.obs['total_counts'] <= count_hi) &
              (adata.obs['pct_counts_mt'] < mt_hi), :].copy()
sc.pp.filter_genes(adata, min_cells=3)
```

**Spatial-specific checks** (beyond per-cell QC):

- **Tissue-area outliers** — sparse beads/spots far from the main tissue mass are often background. Inspect `sq.pl.spatial_scatter(adata, color='total_counts')` and crop manually if needed.
- **FOV/tile edges** (CosMx, Xenium) — cells at FOV boundaries can be double-counted or truncated. Some loaders flag these; others require you to filter on `fov` boundaries.
- **Imaging artifacts** (Visium) — fiducial frame, fold artifacts. Inspect H&E overlay before downstream analysis.

For a turnkey QC script: `python scripts/spatial_qc.py input.h5ad --platform visium --output filtered.h5ad`

### Normalization (same as scRNA-seq)

```python
sc.pp.normalize_total(adata, target_sum=1e4)
sc.pp.log1p(adata)
adata.raw = adata
```

For Visium spots (mixed cell content) and GeoMx ROIs, `sc.pp.normalize_total` plus log1p is fine. Some users prefer SCTransform (R) or scTransform-equivalent (`scvi-tools`) — out of scope here.

### Feature selection, PCA, neighbors, clustering

```python
sc.pp.highly_variable_genes(adata, n_top_genes=2000, flavor='seurat_v3')
sc.pp.scale(adata, max_value=10)
sc.tl.pca(adata, n_comps=50, use_highly_variable=True)
sc.pp.neighbors(adata, n_neighbors=15, n_pcs=30)
sc.tl.umap(adata)
sc.tl.leiden(adata, resolution=0.5)
```

**Panel-size note:** Xenium/CosMx panels are small (~300-1000 genes) — set `n_top_genes` to the panel size or skip HVG selection entirely.

### Spatial neighborhood analysis (squidpy)

This is the part scanpy alone can't do. Build a spatial graph and compute neighborhood-aware statistics.

```python
# Build spatial neighborhood graph
sq.gr.spatial_neighbors(adata, coord_type='generic', n_neighs=6)   # for cell-based platforms
# For Visium spots (hexagonal lattice):
# sq.gr.spatial_neighbors(adata, coord_type='grid', n_neighs=6, n_rings=1)

# Cluster-cluster spatial enrichment (does cluster A live next to cluster B?)
sq.gr.nhood_enrichment(adata, cluster_key='leiden')
sq.pl.nhood_enrichment(adata, cluster_key='leiden', method='single')

# Co-occurrence — at what radii do cluster pairs co-localize?
sq.gr.co_occurrence(adata, cluster_key='leiden')
sq.pl.co_occurrence(adata, cluster_key='leiden')

# Spatially variable genes (Moran's I)
sq.gr.spatial_autocorr(adata, mode='moran', n_jobs=4)
adata.uns['moranI'].head(20)  # top spatially variable genes
```

For the full spatial analysis toolkit (interaction matrix, Ripley statistics, image features) see [references/spatial_analysis.md](references/spatial_analysis.md).

### Visualization

```python
# Spatial scatter — cluster map
sq.pl.spatial_scatter(adata, color='leiden', shape=None)

# Gene expression in space
sq.pl.spatial_scatter(adata, color=['CD3D', 'EPCAM', 'COL1A1'], shape=None)

# Visium-specific: H&E overlay
sc.pl.spatial(adata, color='leiden', img_key='hires', alpha=0.7)

# Xenium-specific: morphology image background
sq.pl.spatial_scatter(adata, color='leiden', img_alpha=0.5)
```

More patterns (multi-panel figures, choosing palettes, image cropping) in [references/plotting_guide.md](references/plotting_guide.md).

## Minimal sections — deeper analyses

The protocol below covers the standard workflow. Three further analyses are common but big enough to deserve their own dedicated protocols; we provide *entry points* here, not full pipelines.

### Cell-cell communication (minimal)

Once cells have cluster / cell-type labels, the standard question is "which cell types signal to which?". For spatial data, ligand-receptor analysis can be **restricted to physically proximate cells** (more biologically faithful than scRNA-seq–only methods).

```python
# Option A: LIANA-py — multi-method consensus, supports spatial weighting
# pip install liana
import liana as li
li.mt.rank_aggregate(adata, groupby='leiden', resource_name='consensus',
                     use_raw=False, verbose=True)
# Spatial-aware: weight LR scores by neighborhood proximity
li.mt.bivar(adata, x_name='CXCL12', y_name='CXCR4', x_layer=None, y_layer=None,
            connectivity_key='spatial_connectivities')
```

```python
# Option B: COMMOT — explicit optimal-transport CCC restricted to spatial neighbors
# pip install commot
import commot as ct
ct.tl.spatial_communication(adata, database_name='CellChatDB',
                             dis_thr=500, heteromeric=True)
```

For a complete cell-cell communication pipeline (database curation, cross-method consensus, differential CCC across conditions), use a dedicated `spatial-communication` protocol.

### Niche / spatial domain detection (minimal)

Clusters from leiden capture transcriptomic identity but not spatial structure. "Niches" / "domains" are spatially coherent regions that may mix cell types.

```python
# Option A: Quick proxy via cluster-aware neighborhood majority vote
import scipy.sparse as sp
# Use spatial neighbors graph from sq.gr.spatial_neighbors
conn = adata.obsp['spatial_connectivities']
labels = adata.obs['leiden'].astype(str).values
# For each cell, the dominant cluster label among its neighbors → niche assignment
from collections import Counter
def niche_of(i):
    nbrs = sp.find(conn[i])[1]
    return Counter(labels[nbrs]).most_common(1)[0][0] if len(nbrs) else labels[i]
adata.obs['niche_proxy'] = [niche_of(i) for i in range(adata.n_obs)]
```

```python
# Option B: BANKSY — joint cell-type + niche detection
# pip install banksy_py
import banksy_py as bk
# Higher lambda_param = more emphasis on neighborhood signal vs. self
bk.cluster.banksy(adata, lambda_param=0.2, k_geom=6)
```

For a complete niche-detection pipeline (BANKSY, CellCharter, GraphST comparison + benchmarking), use a dedicated `spatial-niches` protocol.

### Visium spot deconvolution (minimal)

Visium spots cover multiple cells. Deconvolution estimates cell-type proportions per spot using a single-cell reference.

```python
# Option A: cell2location — Bayesian, GPU-accelerated, gold standard for Visium
# pip install cell2location
import cell2location as c2l
# (1) Train reference signatures from an scRNA-seq AnnData
c2l.models.RegressionModel.setup_anndata(ref_adata, labels_key='cell_type')
ref_model = c2l.models.RegressionModel(ref_adata)
ref_model.train(max_epochs=250)
inf_aver = ref_model.export_posterior(ref_adata)
# (2) Map onto Visium spots
c2l.models.Cell2location.setup_anndata(adata)
sp_model = c2l.models.Cell2location(adata, cell_state_df=inf_aver,
                                     N_cells_per_location=8, detection_alpha=200)
sp_model.train(max_epochs=30000)
adata = sp_model.export_posterior(adata)
```

```python
# Option B: RCTD — robust cell-type decomposition, lighter weight
# Available via the spacexr R package; call from Python via rpy2 or shell out.
```

For a complete deconvolution pipeline (reference curation, hyperparameter selection, validation, downstream niche analysis on deconvolved estimates), use a dedicated `spatial-deconvolution` protocol.

## End-to-end template

`assets/spatial_template.py` is a single parameterized script — set `PLATFORM = 'visium' | 'xenium' | 'cosmx' | 'merfish' | 'slideseq' | 'geomx'` at the top and run end-to-end.

```bash
python assets/spatial_template.py
```

## Convenience scripts

- `scripts/spatial_qc.py` — quantile-based QC, platform-aware. Outputs filtered .h5ad and QC plots.
- `scripts/spatial_neighborhood.py` — spatial-neighbors graph, neighborhood enrichment, co-occurrence, Moran's I. Outputs annotated .h5ad and figures.

## Key Parameters to Adjust

### Spatial neighborhood graph (`sq.gr.spatial_neighbors`)
- `n_neighs`: Neighbors per cell. Defaults: cell-based platforms → 6; Visium spots → 6 (hexagonal); Slide-seq beads → 10.
- `coord_type`: `'generic'` for cells / beads, `'grid'` for Visium hexagonal lattice.
- `radius`: Use distance-based neighbors instead of k-NN when comparing across resolutions.

### QC (quantile-based; see [references/loading_guide.md](references/loading_guide.md))
- `gene_q_lo / gene_q_hi`: 5th / 99th percentile defaults on `n_genes_by_counts`.
- `mt_ceiling`: 20% absolute MT% cap — caps the quantile if the dataset has unusually high MT.

### Niche detection
- `lambda_param` (BANKSY): 0.0 → pure cell-type; 1.0 → pure niche.
- `k_geom`: spatial neighbor count for the niche embedding.

## Best Practices

- **Inspect the H&E or morphology image FIRST**. Tissue folds, edge artifacts, and detached tissue can dominate downstream stats. `sc.pl.spatial(adata, img_key='hires')` for Visium; for Xenium load the morphology TIFF separately.
- **Don't blindly apply scRNA-seq HVG selection to small-panel platforms** (Xenium ~300 genes, GeoMx ~1500-2000 in WTA). Either skip HVG selection or set `n_top_genes` to panel size.
- **Spatial neighbors graph choice matters**: use `coord_type='grid'` for Visium spots (hex lattice), `'generic'` for cell-based platforms. The `n_rings` parameter on grid gives multi-ring neighborhoods if needed.
- **Co-occurrence runs at multiple radii**. Inspect the curves — a single "co-localized" decision at one radius is misleading.
- **Multi-sample integration**: integrate cell-type embeddings (Harmony/scVI) BEFORE building per-sample spatial graphs. Don't try to merge spatial coordinates across samples.

## References

- [scanpy documentation](https://scanpy.readthedocs.io)
- [squidpy documentation](https://squidpy.readthedocs.io)
- [squidpy spatial tutorials](https://squidpy.readthedocs.io/en/stable/notebooks/tutorials.html)
- 10X Genomics Visium / Xenium analysis guides
- Nanostring CosMx / GeoMx data formats
- BANKSY (Singhal et al., 2024), CellCharter (Varrone et al., 2024) for niche detection
- cell2location (Kleshchevnikov et al., 2022) for Visium deconvolution
