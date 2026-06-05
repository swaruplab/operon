---
name: resolvi
description: ResolVI — variational autoencoder for denoising imaging-based spatial transcriptomics (Xenium, MERFISH, CosMx). Removes ambient background and resolves wrong segmentation by jointly modeling true cell expression, mis-assigned neighbor counts, and unspecific background. Outputs a latent representation, denoised expression, and (semi-supervised) cell-type predictions. Built on scvi-tools; GPU recommended.
license: BSD-3-Clause
metadata:
---

# ResolVI: Resolving Single-Cell Resolution in Spatial Transcriptomics

## Overview

[ResolVI](https://docs.scvi-tools.org/en/latest/user_guide/models/resolvi.html) is a scvi-tools model that **denoises imaging-based spatial transcriptomics** (Xenium, MERFISH, CosMx, etc.) by jointly modeling three sources of observed counts in each cell:

| Component | Symbol | What it represents |
|---|---|---|
| α₀ | `alpha_0` | True expression from this cell |
| α₁ | `alpha_1` | Expression "leaked" from neighbor cells due to wrong segmentation |
| α₂ | `alpha_2` | Unspecific ambient background |

The generative model uses a **mixture-of-Gaussians prior** over cell embeddings and **weighted diffusion** over the spatial neighbor graph to apportion observed counts among the three sources. The result is a per-cell latent representation that captures the *true* biological signal, plus denoised expression that strips off background and neighbor contamination.

**Why this matters**: imaging-based ST has three fundamental noise sources that bulk denoisers (CellBender for ambient, Scrublet for doublets) don't address well — segmentation errors smear transcripts across adjacent cells, and the cell's actual transcript count is conflated with neighbors. ResolVI is the model purpose-built for this.

## When to Use This Skill

- High-resolution imaging-based spatial data: **Xenium, MERFISH (Vizgen MERSCOPE), CosMx**
- When markers from one cell type are leaking into adjacent cells in your clustering
- When you want a clean per-cell latent representation for downstream UMAP / clustering / DE
- (Semi-supervised) cell-type prediction from a partially-labeled subset
- Niche-abundance analysis (changes in cellular composition between conditions)

**Not for**:
- Low-resolution spatial (Visium spots) — ResolVI's neighbor-mis-assignment model doesn't apply when each spot already contains multiple cells. Use cell2location / RCTD instead (see `spatial-transcriptomics` protocol's deconvolution section).
- scRNA-seq without spatial coords — no spatial graph to diffuse over. Use scVI / scANVI.
- Bulk denoising of ambient — CellBender is better at that specific job.
- CPU-only servers — training a ResolVI model on a typical 100k-cell dataset takes hours on CPU, ~20-30 minutes on GPU.

## Prerequisites

- Python 3.9+
- An imaging-ST AnnData object with `adata.obsm['spatial']` (cell centroids in µm or pixels)
- Cell-type labels (full or partial, depending on supervision mode)
- **GPU strongly recommended** — set `accelerator="gpu", devices=1` on training

```bash
# Core install — scvi-tools includes RESOLVI in scvi.external
pip install scvi-tools

# Verify GPU detection
python -c "import torch; print('CUDA:', torch.cuda.is_available())"
```

## Quick Start

```python
import scanpy as sc
import scvi
import numpy as np

# ── 1. Load your spatial AnnData (Xenium / MERFISH / CosMx) ──────────────
adata = sc.read_h5ad("xenium_filtered.h5ad")
assert "spatial" in adata.obsm, "ResolVI requires adata.obsm['spatial']"
# adata.X should be RAW counts (not log-normalized). scvi-tools handles
# normalization internally via its negative-binomial likelihood.

# Cell-type labels — for semi-supervised mode, mark unlabelled as 'Unknown'.
# In unsupervised mode this column is just an unused placeholder.
adata.obs["celltype_resolvi"] = adata.obs["cell_type"].fillna("Unknown")

# ── 2. Setup AnnData for the RESOLVI model ───────────────────────────────
from scvi.external import RESOLVI

RESOLVI.setup_anndata(
    adata,
    layer            = None,                      # uses adata.X (raw counts)
    labels_key       = "celltype_resolvi",        # cell-type column (semi-supervised)
    batch_key        = "sample_id",                # optional, drop if single sample
    unlabeled_category = "Unknown",
)

# ── 3. Build the spatial neighbor graph ResolVI uses ─────────────────────
# (Skip if you already ran `sq.gr.spatial_neighbors` and have it in obsp.)
import squidpy as sq
sq.gr.spatial_neighbors(adata, coord_type="generic", n_neighs=20)

# ── 4. Train ─────────────────────────────────────────────────────────────
model = RESOLVI(
    adata,
    n_hidden    = 128,
    n_latent    = 20,
    n_layers    = 2,
    dropout_rate = 0.1,
    semisupervised = True,                         # use labels if available
)
model.train(
    max_epochs = 200,
    accelerator = "gpu", devices = 1,
    early_stopping = True,
)
# Save the trained model alongside the data
model.save("models/resolvi_xenium", save_anndata=False, overwrite=True)
```

## Using the Trained Model

### Latent representation → UMAP / clusters

```python
adata.obsm["X_resolvi"] = model.get_latent_representation()

sc.pp.neighbors(adata, use_rep="X_resolvi")
sc.tl.umap(adata)
sc.tl.leiden(adata, resolution=0.5)
sc.pl.umap(adata, color=["leiden", "cell_type"])
```

The `X_resolvi` latent space is denoised — the same biology, with neighbor contamination and ambient stripped out. Clusters from this space typically look "cleaner" than clusters from raw counts.

### Denoised expression

```python
# Per-cell, per-gene estimate of TRUE expression (alpha_0 component)
denoised = model.get_normalized_expression(
    library_size = 1e4,
    return_mean  = True,                          # use posterior mean
)
# denoised is an AnnData / DataFrame depending on input — see the scvi-tools docs

# Compare to raw expression for a few marker genes:
sc.pl.umap(adata, color=["CD3D", "MS4A1", "CD14"], layer=None)       # raw
adata.layers["resolvi_denoised"] = denoised
sc.pl.umap(adata, color=["CD3D", "MS4A1", "CD14"], layer="resolvi_denoised")  # cleaned
```

A clean denoised UMAP will show CD3D confined to T cells, MS4A1 confined to B cells. If it still spreads across multiple clusters, ResolVI hasn't fully converged — run more epochs or check that the neighbor graph was built correctly.

### Differential expression on denoised counts

```python
de_df = model.differential_expression(
    groupby = "leiden",
    group1  = "0", group2 = "1",
    mode    = "vanilla",                          # or "change" for effect-size based
)
# Returns a per-gene DE table with proper posterior probabilities (LFC + Bayes factor)
```

### Cell-type prediction (semi-supervised mode)

```python
# Predicted label for every cell, including the original 'Unknown' set
preds = model.predict()
adata.obs["resolvi_predicted_celltype"] = preds

# Confidence per cell
probs = model.predict(soft=True)
adata.obs["resolvi_confidence"] = probs.max(axis=1)
```

If you trained with `semisupervised=True` and labels for ~10-30% of cells, `predict()` fills in the rest. Confidence scores let you flag uncertain calls for manual review.

### Differential niche abundance

Are certain cell types over-represented in some spatial neighborhoods compared to others?

```python
niche_df = model.differential_niche_abundance(
    groupby = "condition",                        # column with the conditions to compare
    group1  = "disease", group2 = "control",
)
# Returns log-fold-changes in per-niche cell-type abundance between conditions
```

This is the cleanest "which cell type changed where" analysis for spatial data — closer to ground truth than naive cell-counting because it uses the joint model's understanding of segmentation errors.

## Key Parameters

### Model setup
- `n_latent` (20): higher = more expressive, but more compute. 20-30 is typical.
- `n_hidden` (128): hidden-layer width in the neural net. Default usually fine.
- `n_layers` (2): network depth. 1-3 reasonable.
- `dropout_rate` (0.1): regularization.
- `semisupervised` (True): use labels during training if available. False = pure unsupervised.

### Training
- `max_epochs` (200): more epochs help if ELBO is still descending. Watch the loss curve.
- `early_stopping` (True): stops when validation loss stops improving.
- `batch_size`: lower if running into GPU memory issues (default 256 is usually fine).

### Spatial graph (`sq.gr.spatial_neighbors`)
- `n_neighs` (20): how many neighbors each cell sees in the ResolVI diffusion. Higher = more smoothing (good for sparse panels), lower = preserves more sharp boundaries (good for dense panels).

## Best Practices

- **Always use raw counts**, not log-normalized. ResolVI uses a negative binomial likelihood internally.
- **Build the spatial neighbor graph first** with `squidpy.gr.spatial_neighbors` and the same coordinate convention. ResolVI reads from `obsp["spatial_connectivities"]`.
- **GPU is not optional for any real-sized dataset.** 100k cells × 300 genes × 200 epochs is ~5 minutes on an A100, ~6 hours on CPU.
- **Validate against raw UMAP.** A clean denoised UMAP should look like a sharper version of the raw UMAP, not a fundamentally different topology. If clusters appear/disappear, inspect.
- **For multi-sample studies**, pass `batch_key`. The model integrates across batches in the latent space (Harmony-like effect).
- **Semi-supervised mode is more useful than fully-supervised.** Label ~10-30% of cells from canonical markers, let ResolVI infer the rest. Saves manual annotation effort and is more robust than fully training on possibly-noisy labels.
- **Save the model.** Re-training a ResolVI from scratch is expensive. `model.save()` persists weights + config.

## When ResolVI Output Looks Wrong

| Symptom | Likely cause | Fix |
|---|---|---|
| Denoised UMAP looks identical to raw | Model didn't converge | Bump `max_epochs`; check ELBO curve |
| Denoised UMAP has a totally different topology | Over-corrected, or neighbor graph is wrong | Inspect `obsp["spatial_connectivities"]` shape and connections; rebuild with `sq.gr.spatial_neighbors` |
| `predict()` returns 'Unknown' for everyone | Semi-supervised but no labels actually got registered | Confirm `setup_anndata(labels_key=...)` and that the column has values OTHER than the unlabeled category |
| OOM during training | Dataset too big for one GPU | Lower `batch_size`; use `accelerator='gpu', devices=1` (multi-GPU not always faster) |

## End-to-End Template

`assets/resolvi_template.py` — single parameterized script. Configure data path, label column, and batch column, then run end-to-end.

## Convenience Scripts

- `scripts/run_resolvi.py` — CLI wrapper: train, save model, write denoised AnnData + latent embedding

## References

- [scvi-tools ResolVI docs](https://docs.scvi-tools.org/en/latest/user_guide/models/resolvi.html)
- [scvi-tools tutorials](https://docs.scvi-tools.org/en/latest/tutorials/index.html) — Spatial section
- Original ResolVI paper (preprint expected via the scvi-tools team; check the docs page for the citation)
- Related Operon protocol: [`spatial-transcriptomics`](../spatial-transcriptomics/SKILL.md) for QC + clustering pipeline that ResolVI sits inside
