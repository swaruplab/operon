#!/usr/bin/env python3
"""
resolvi_template.py — End-to-end ResolVI workflow for imaging-ST data.

Configure the CONFIGURATION block, then run end-to-end:
  1. Load AnnData + sanity-check spatial coords
  2. Build spatial neighbor graph (if not already present)
  3. Setup RESOLVI on the AnnData
  4. Train the model
  5. Compute latent + denoised + predictions
  6. UMAP + clustering on the latent
  7. Plot before/after diagnostics
  8. Save augmented AnnData + trained model
"""

import os
import sys
from pathlib import Path

import scanpy as sc
import squidpy as sq
import scvi
from scvi.external import RESOLVI
import numpy as np
import torch

# ============================================================================
# CONFIGURATION
# ============================================================================

INPUT_H5AD      = "data/xenium_filtered.h5ad"  # raw counts in .X
OUTPUT_H5AD     = "results/xenium_resolvi.h5ad"
MODEL_DIR       = "models/resolvi_xenium"
FIG_DIR         = "figures"

LABEL_COL       = "cell_type"                  # adata.obs column with labels
BATCH_COL       = "sample_id"                  # set to None for single sample
UNLABELED_TAG   = "Unknown"                    # cells in LABEL_COL with this value count as unlabeled

# Spatial graph
N_NEIGHS        = 20

# Model architecture
N_LATENT        = 20
N_HIDDEN        = 128
N_LAYERS        = 2
SEMISUPERVISED  = True

# Training
MAX_EPOCHS      = 200
EARLY_STOPPING  = True
DEVICE          = "gpu"        # "gpu" or "cpu"

# Downstream clustering
LEIDEN_RES      = 0.5

# Validation markers — used for before/after UMAP comparison
MARKERS         = ["CD3D", "MS4A1", "CD14", "EPCAM", "COL1A1"]

# ============================================================================
# SETUP
# ============================================================================

os.makedirs(Path(OUTPUT_H5AD).parent or ".", exist_ok=True)
os.makedirs(FIG_DIR, exist_ok=True)
sc.settings.figdir = FIG_DIR

if DEVICE == "gpu" and not torch.cuda.is_available():
    print("WARNING: no CUDA. Falling back to CPU.")
    DEVICE = "cpu"
accelerator = "gpu" if DEVICE == "gpu" else "cpu"

# ============================================================================
# 1. LOAD + SANITY-CHECK
# ============================================================================

print("=" * 80)
print(f"LOAD: {INPUT_H5AD}")
print("=" * 80)
adata = sc.read_h5ad(INPUT_H5AD)
print(f"  {adata.n_obs} cells × {adata.n_vars} genes")

assert "spatial" in adata.obsm, "ResolVI requires adata.obsm['spatial']"
assert LABEL_COL in adata.obs.columns, f"LABEL_COL '{LABEL_COL}' missing"

# Fill NA cell-type labels with UNLABELED_TAG
adata.obs["celltype_resolvi"] = (
    adata.obs[LABEL_COL].astype("object").fillna(UNLABELED_TAG).astype("category")
)
n_labeled = (adata.obs["celltype_resolvi"] != UNLABELED_TAG).sum()
print(f"  Labeled: {n_labeled:,} / {adata.n_obs:,}")

# ============================================================================
# 2. SPATIAL NEIGHBORS
# ============================================================================

if "spatial_connectivities" not in adata.obsp:
    print(f"\nBuilding spatial graph (n_neighs={N_NEIGHS}) …")
    sq.gr.spatial_neighbors(adata, coord_type="generic", n_neighs=N_NEIGHS)
print(f"  Edges: {adata.obsp['spatial_connectivities'].nnz}")

# ============================================================================
# 3. SETUP + 4. TRAIN
# ============================================================================

print("\n" + "=" * 80)
print("SETUP + TRAIN")
print("=" * 80)

RESOLVI.setup_anndata(
    adata,
    labels_key="celltype_resolvi",
    batch_key=BATCH_COL,
    unlabeled_category=UNLABELED_TAG,
)

model = RESOLVI(
    adata,
    n_hidden=N_HIDDEN,
    n_latent=N_LATENT,
    n_layers=N_LAYERS,
    dropout_rate=0.1,
    semisupervised=SEMISUPERVISED,
)

print(f"Training on {accelerator.upper()} for ≤ {MAX_EPOCHS} epochs …")
model.train(
    max_epochs=MAX_EPOCHS,
    accelerator=accelerator,
    devices=1,
    early_stopping=EARLY_STOPPING,
)

Path(MODEL_DIR).parent.mkdir(parents=True, exist_ok=True)
model.save(MODEL_DIR, save_anndata=False, overwrite=True)
print(f"Model → {MODEL_DIR}")

# ============================================================================
# 5. LATENT + DENOISED + PREDICTIONS
# ============================================================================

print("\nComputing latent + denoised + predictions …")
adata.obsm["X_resolvi"] = model.get_latent_representation()

try:
    denoised = model.get_normalized_expression(library_size=1e4, return_mean=True)
    if hasattr(denoised, "values"):
        adata.layers["resolvi_denoised"] = denoised.values.astype(np.float32)
    else:
        adata.layers["resolvi_denoised"] = denoised.astype(np.float32)
except Exception as e:
    print(f"  (denoised expression skipped: {e})")

if SEMISUPERVISED:
    try:
        adata.obs["resolvi_predicted"] = model.predict()
        soft = model.predict(soft=True)
        adata.obs["resolvi_confidence"] = soft.max(axis=1)
    except Exception as e:
        print(f"  (predictions skipped: {e})")

# ============================================================================
# 6. UMAP + LEIDEN
# ============================================================================

print(f"\nUMAP + leiden (resolution={LEIDEN_RES}) on X_resolvi …")
sc.pp.neighbors(adata, use_rep="X_resolvi")
sc.tl.umap(adata)
sc.tl.leiden(adata, resolution=LEIDEN_RES)

sc.pl.umap(adata, color=["leiden", LABEL_COL], save="_resolvi_clusters.pdf")
if SEMISUPERVISED and "resolvi_predicted" in adata.obs:
    sc.pl.umap(adata, color=["resolvi_predicted", "resolvi_confidence"],
                save="_resolvi_predictions.pdf")

# ============================================================================
# 7. BEFORE/AFTER DIAGNOSTICS
# ============================================================================

print("\nBefore/after marker comparison …")
available_markers = [m for m in MARKERS if m in adata.var_names]
if available_markers:
    sc.pl.umap(adata, color=available_markers, layer=None,
                save="_markers_raw.pdf")
    if "resolvi_denoised" in adata.layers:
        sc.pl.umap(adata, color=available_markers, layer="resolvi_denoised",
                    save="_markers_denoised.pdf")
    print(f"  Plotted {len(available_markers)} markers — compare _raw vs _denoised.")

# Optional spatial scatter (squidpy)
try:
    sq.pl.spatial_scatter(adata, color="leiden", shape=None, size=4,
                           save=f"{FIG_DIR}/spatial_clusters.pdf")
except Exception as e:
    print(f"  (spatial scatter skipped: {e})")

# ============================================================================
# 8. SAVE
# ============================================================================

print(f"\nWriting {OUTPUT_H5AD} …")
adata.write_h5ad(OUTPUT_H5AD)

print("\nDone.")
print(f"  Output:    {OUTPUT_H5AD}")
print(f"  Model:     {MODEL_DIR}")
print(f"  Figures:   {FIG_DIR}/")
