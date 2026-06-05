---
name: mrvi
description: MrVI — multi-resolution variational inference for multi-sample scRNA-seq. Two-level hierarchical model that learns both a sample-unaware cell-state latent (u) and a sample-aware latent (z). Outputs per-cell sample-distance matrices for stratification discovery, plus differential-abundance / differential-expression between sample groups at single-cell resolution. Built on scvi-tools; GPU recommended.
license: BSD-3-Clause
metadata:
---

# MrVI: Multi-Resolution Variational Inference

## Overview

[MrVI](https://docs.scvi-tools.org/en/latest/user_guide/models/mrvi.html) tackles a recurring problem in multi-sample / multi-donor scRNA-seq: **the same cell type can behave differently between samples**, and you want to discover those differences without committing up front to one cluster resolution.

The model is a two-level hierarchical VAE:

| Level | Latent | What it represents |
|---|---|---|
| 1 | `u_n` | Cell state, **batch-corrected and sample-unaware** — like a clean scVI embedding |
| 2 | `z_n` | Cell state **with sample effects added back in** — same cell type in different samples lives at slightly different points in z |

That layered design lets you do two things you can't do with vanilla scVI:

1. **Per-cell sample-distance matrices** — for each cell, "how does this exact cell look across all samples?" Reveals stratification structure invisible at the cluster level.
2. **Differential abundance / DE at single-cell resolution** — compare sample groups without forcing a clustering first.

Decoder: multi-head attention over batch + sample covariates. Likelihood: negative binomial on raw counts.

## When to Use This Skill

- Multi-donor / multi-condition scRNA-seq where you want to find **patient-level subgroups** based on molecular profiles, not pre-defined clinical metadata.
- Differential expression / abundance comparisons across sample groups where you don't want to commit to a leiden resolution first.
- Exploratory analysis on cohort studies (≥ 10 samples) — MrVI shines when you have many samples.
- When scVI batch correction is too aggressive — MrVI preserves sample-level variation in `z` while still giving you a clean `u` for clustering.

**Not for**:
- Single-sample analyses — MrVI's whole point is sample-level variation. Use scVI.
- Spatial data with low sample count — see [`resolvi`](../resolvi/SKILL.md) instead.
- ATAC-seq or other modalities — MrVI is RNA-specific (NB likelihood).
- CPU-only — like all scvi-tools models, training is much faster on GPU.

## Prerequisites

- Python 3.9+
- An scRNA-seq AnnData with raw counts in `.X`
- A sample column (`sample_id`, `donor`, `patient`, etc.) — the core covariate MrVI tracks
- Optional: batch column (different from sample — batch = technical, sample = biological)
- Optional: cell-type labels (improves analysis but not required)
- **GPU strongly recommended**

```bash
pip install scvi-tools
```

## Quick Start

```python
import scanpy as sc
import scvi
from scvi.external import MRVI
import torch

# ── 1. Load + sanity-check ──────────────────────────────────────────────
adata = sc.read_h5ad("cohort_data.h5ad")
# Required: raw counts in adata.X, sample column in adata.obs
assert "sample_id" in adata.obs.columns

# Standard pre-filter (MrVI does NOT do QC itself)
sc.pp.filter_cells(adata, min_genes=200)
sc.pp.filter_genes(adata, min_cells=3)

# Optionally select HVGs — MrVI scales linearly in n_genes, so trimming helps
sc.pp.highly_variable_genes(
    adata, n_top_genes=4000, flavor="seurat_v3",
    batch_key="sample_id"
)
adata = adata[:, adata.var["highly_variable"]].copy()

# ── 2. Setup ────────────────────────────────────────────────────────────
MRVI.setup_anndata(
    adata,
    sample_key  = "sample_id",         # CORE: per-sample target covariate
    batch_key   = "batch",             # OPTIONAL: nuisance batch column (different from sample)
    labels_key  = "cell_type",         # OPTIONAL: improves analysis if available
)

# ── 3. Build + train ────────────────────────────────────────────────────
model = MRVI(
    adata,
    n_hidden = 128,
    n_latent_u = 20,                   # cell-state latent dimensions
    n_latent_z = 20,                   # sample-aware latent dimensions
    n_layers   = 2,
)

model.train(
    max_epochs       = 400,
    accelerator      = "gpu",
    devices          = 1,
    early_stopping   = True,
)

model.save("models/mrvi_cohort", save_anndata=False, overwrite=True)
```

## What MrVI Gives You

### 1. Two latent representations

```python
# u: sample-unaware (clean cell-state, like batch-corrected scVI)
adata.obsm["U_mrvi"] = model.get_latent_representation(give_z=False)

# z: sample-aware (cell-state + sample effects)
adata.obsm["Z_mrvi"] = model.get_latent_representation(give_z=True)
```

Use `U_mrvi` for clustering and UMAP that you want clean of sample effects:

```python
sc.pp.neighbors(adata, use_rep="U_mrvi")
sc.tl.umap(adata)
sc.tl.leiden(adata, resolution=0.5)
sc.pl.umap(adata, color=["leiden", "sample_id", "cell_type"])
```

Use `Z_mrvi` for analyses that should *preserve* sample effects (most of what's below).

### 2. Per-cell sample-distance matrices — the killer feature

For each cell, MrVI can compute "how different is this cell's profile across the N samples in the cohort?" This is a per-cell N × N matrix that you can mean-pool or cluster to find sample subgroups.

```python
sample_dists = model.get_local_sample_representation(
    adata = adata,
    # batch_size = 32,   # lower if OOM
)
# Shape: (n_cells, n_samples, n_latent_z)
# Each cell has its own per-sample "where would I sit if I were sample X"

# Pairwise distance matrix per cell: (n_cells, n_samples, n_samples)
dist_mat = model.get_local_sample_distances(
    adata = adata,
    keep_cell = True,           # per-cell matrices (False → mean-pooled)
)
```

### 3. Cohort-level sample distances

Average those per-cell matrices to get a single N × N sample-distance matrix you can hierarchically cluster:

```python
mean_dist = model.get_local_sample_distances(adata = adata, keep_cell = False)
# Shape: (n_samples, n_samples)

import scipy.cluster.hierarchy as sch
import matplotlib.pyplot as plt
import seaborn as sns

linkage = sch.linkage(mean_dist, method="average")
sns.clustermap(mean_dist, row_linkage=linkage, col_linkage=linkage,
                figsize=(8, 8), cmap="viridis")
plt.savefig("figures/sample_distance_clustermap.pdf")
```

The clustered heatmap reveals sample subgroups (e.g. responders vs non-responders) emerging from the molecular data alone, without any pre-defined grouping.

### 4. Per-cell differential abundance + differential expression

Compare two sample groups at single-cell resolution — no clustering required.

```python
# Define your sample groups
adata.obs["group"] = adata.obs["sample_id"].map({
    "P01": "Disease", "P02": "Disease", ...,
    "P10": "Control", "P11": "Control", ...,
})

# Differential abundance — which cells become more/less common in disease?
da_df = model.differential_abundance(
    adata = adata,
    sample_cov_keys = ["group"],
    group1 = "Disease", group2 = "Control",
)
# Returns per-cell log-fold-change in abundance + significance

# Map onto the UMAP — where in the manifold does abundance change?
adata.obs["DA_log2FC"] = da_df["log2FC"].values
sc.pl.umap(adata, color=["DA_log2FC", "leiden"],
            vmin=-2, vmax=2, cmap="RdBu_r")
```

For DE (per-gene log-fold-change between sample groups, single-cell-resolution):

```python
de_df = model.differential_expression(
    adata = adata,
    sample_cov_keys = ["group"],
    group1 = "Disease", group2 = "Control",
)
# Per-gene DE values aggregated per-cell — you can also stratify by cluster
```

## Key Parameters

### Model architecture
- `n_latent_u` (20): dimensions of the sample-unaware latent. Same intuition as scVI's `n_latent`.
- `n_latent_z` (20): dimensions of the sample-aware latent. Often kept equal to `n_latent_u`.
- `n_hidden` (128): neural-net width.
- `n_layers` (2): network depth.

### Training
- `max_epochs` (400): MrVI typically needs more epochs than scVI to converge — the hierarchical model has more parameters.
- `early_stopping` (True): stops when validation loss stops dropping. Recommended.

### Setup
- `sample_key`: **required** — the column MrVI builds its `z` representation around. Must be categorical.
- `batch_key`: technical batch (different from sample). E.g. "10X chemistry version" or "library prep date."
- `labels_key`: optional cell-type column. Improves downstream DE / DA analyses by stratifying.

## Best Practices

- **Raw counts in `.X`**, not log-normalized. The NB likelihood needs counts.
- **Use HVG selection before training** to keep `n_genes ≤ 5000`. Training time is linear in `n_genes`.
- **More samples = better.** With < 5 samples, MrVI's sample-distance analysis is underpowered. Aim for ≥ 10, ideally 20-50.
- **Sample ≠ Batch.** Sample = biological unit (donor, patient). Batch = technical (run, chemistry). Pass them separately. If they're identical, just pass `sample_key`.
- **For DA / DE, group sample IDs into sample-level covariates first.** MrVI computes per-cell statistics by aggregating over sample-level grouping — your `group1`/`group2` should be sample-level categories.
- **Validate the U embedding first.** Before trusting any per-cell sample-distance analysis, confirm UMAP-on-U gives a sensible cell-type structure. If U is noisy, everything downstream is unreliable.
- **Cohort-mean distance vs per-cell distance — both useful.** Mean for "which samples cluster together overall"; per-cell for "in which cell type does that grouping break down."

## When MrVI Output Looks Wrong

| Symptom | Likely cause | Fix |
|---|---|---|
| UMAP-on-U still shows sample-segregation | Under-trained or `batch_key` wasn't set | More epochs; verify the batch/sample distinction |
| Sample distances are uniform | Under-trained, or your samples really are similar | Inspect the loss curve; sometimes the biology is just homogeneous |
| `differential_abundance` returns NaN for many cells | Sample groups are too unbalanced | Re-balance, or remove samples in tiny groups |
| OOM during `get_local_sample_distances` | Per-cell N×N matrix is large | Use `keep_cell=False` for cohort-mean, or batch through cells manually |

## End-to-End Template

`assets/mrvi_template.py` — single parameterized script. Set sample / batch / group columns and the comparison groups, run end-to-end.

## Convenience Scripts

- `scripts/run_mrvi.py` — CLI wrapper: train, save model, write augmented AnnData

## References

- [scvi-tools MrVI docs](https://docs.scvi-tools.org/en/latest/user_guide/models/mrvi.html)
- [scvi-tools tutorials](https://docs.scvi-tools.org/en/latest/tutorials/index.html) — multi-sample section
- Boyeau et al. (2024 preprint), *Deep generative modeling for population-scale single-cell genomics* (the MrVI paper; check the scvi-tools docs for the current citation)
- Related Operon protocols:
  - [`scanpy`](../scanpy/SKILL.md) — upstream QC + HVG selection
  - [`hdwgcna`](../hdwgcna/SKILL.md) — alternative cohort-level co-expression analysis (R-based)
