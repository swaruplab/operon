#!/usr/bin/env python3
"""
mrvi_template.py — End-to-end MrVI workflow.

Configure the CONFIGURATION block, then run end-to-end:
  1. Load + HVG-filter
  2. Setup + train
  3. U + Z latents
  4. UMAP + leiden on U
  5. Cohort-mean sample-distance clustermap
  6. (Optional) per-cell sample-distance matrices
  7. (Optional) DA + DE between sample groups
  8. Save augmented AnnData + trained model
"""

import os
import sys
from pathlib import Path

import scanpy as sc
import scvi
from scvi.external import MRVI
import numpy as np
import torch
import matplotlib.pyplot as plt

# ============================================================================
# CONFIGURATION
# ============================================================================

INPUT_H5AD       = "data/cohort_raw.h5ad"      # raw counts in .X
OUTPUT_H5AD      = "results/cohort_mrvi.h5ad"
MODEL_DIR        = "models/mrvi_cohort"
RESULTS_DIR      = "results"
FIG_DIR          = "figures"

# Required setup keys
SAMPLE_KEY       = "sample_id"                 # biological sample column
BATCH_KEY        = None                        # technical batch (None for none)
LABELS_KEY       = "cell_type"                 # optional, set None if missing

# Model
N_LATENT         = 20
N_HIDDEN         = 128
N_LAYERS         = 2

# Training
MAX_EPOCHS       = 400
DEVICE           = "gpu"                       # "gpu" or "cpu"

# Pre-filter
N_HVG            = 4000

# Optional sample-group comparison
GROUP_COL        = "group"                     # column with sample-level groupings; set None to skip DA/DE
GROUP1           = "Disease"
GROUP2           = "Control"

# Per-cell sample-distance matrix (expensive — set False if you only want cohort-mean)
COMPUTE_PER_CELL_DISTANCES = False

# Downstream clustering
LEIDEN_RES       = 0.5

# ============================================================================
# SETUP
# ============================================================================

for d in (Path(OUTPUT_H5AD).parent, FIG_DIR, RESULTS_DIR):
    Path(d).mkdir(parents=True, exist_ok=True)
sc.settings.figdir = FIG_DIR

if DEVICE == "gpu" and not torch.cuda.is_available():
    print("WARNING: no CUDA — falling back to CPU.")
    DEVICE = "cpu"
accelerator = "gpu" if DEVICE == "gpu" else "cpu"

# ============================================================================
# 1. LOAD + HVG-FILTER
# ============================================================================

print("=" * 80)
print(f"LOAD: {INPUT_H5AD}")
print("=" * 80)
adata = sc.read_h5ad(INPUT_H5AD)
print(f"  {adata.n_obs} cells × {adata.n_vars} genes")
print(f"  Samples: {adata.obs[SAMPLE_KEY].nunique()}")
print(f"  Cells per sample: median = {int(adata.obs[SAMPLE_KEY].value_counts().median())}")

sc.pp.filter_cells(adata, min_genes=200)
sc.pp.filter_genes(adata, min_cells=3)

print(f"\nSelecting top {N_HVG} HVGs (across-sample stable) …")
sc.pp.highly_variable_genes(
    adata,
    n_top_genes=N_HVG,
    flavor="seurat_v3",
    batch_key=SAMPLE_KEY,
)
adata = adata[:, adata.var["highly_variable"]].copy()
print(f"  After HVG: {adata.n_obs} × {adata.n_vars}")

# ============================================================================
# 2. SETUP + TRAIN
# ============================================================================

print("\n" + "=" * 80)
print("SETUP + TRAIN")
print("=" * 80)

setup_kwargs = {"sample_key": SAMPLE_KEY}
if BATCH_KEY:  setup_kwargs["batch_key"]  = BATCH_KEY
if LABELS_KEY: setup_kwargs["labels_key"] = LABELS_KEY
MRVI.setup_anndata(adata, **setup_kwargs)

model = MRVI(
    adata,
    n_hidden=N_HIDDEN,
    n_latent_u=N_LATENT,
    n_latent_z=N_LATENT,
    n_layers=N_LAYERS,
)

print(f"Training on {accelerator.upper()} for ≤ {MAX_EPOCHS} epochs …")
model.train(
    max_epochs=MAX_EPOCHS,
    accelerator=accelerator,
    devices=1,
    early_stopping=True,
)
model.save(MODEL_DIR, save_anndata=False, overwrite=True)
print(f"Model → {MODEL_DIR}")

# ============================================================================
# 3. U + Z LATENTS
# ============================================================================

print("\nComputing U + Z latents …")
adata.obsm["U_mrvi"] = model.get_latent_representation(give_z=False)
adata.obsm["Z_mrvi"] = model.get_latent_representation(give_z=True)

# ============================================================================
# 4. UMAP + LEIDEN ON U
# ============================================================================

print(f"\nUMAP + leiden on U (resolution={LEIDEN_RES}) …")
sc.pp.neighbors(adata, use_rep="U_mrvi")
sc.tl.umap(adata)
sc.tl.leiden(adata, resolution=LEIDEN_RES)

color_keys = ["leiden", SAMPLE_KEY]
if LABELS_KEY and LABELS_KEY in adata.obs.columns:
    color_keys.append(LABELS_KEY)
sc.pl.umap(adata, color=color_keys, save="_mrvi_U_clusters.pdf")

# ============================================================================
# 5. COHORT-MEAN SAMPLE-DISTANCE MATRIX
# ============================================================================

print("\nComputing cohort-mean sample-distance matrix …")
try:
    mean_dist = model.get_local_sample_distances(adata=adata, keep_cell=False)
    mean_dist = np.asarray(mean_dist)
    np.save(f"{RESULTS_DIR}/sample_distance_mean.npy", mean_dist)
    print(f"  shape: {mean_dist.shape} → {RESULTS_DIR}/sample_distance_mean.npy")

    # Clustermap
    try:
        import seaborn as sns
        sample_ids = sorted(adata.obs[SAMPLE_KEY].unique())
        g = sns.clustermap(
            mean_dist, xticklabels=sample_ids, yticklabels=sample_ids,
            figsize=(10, 10), cmap="viridis",
            row_cluster=True, col_cluster=True,
        )
        g.savefig(f"{FIG_DIR}/sample_distance_clustermap.pdf")
        plt.close()
        print(f"  Clustermap → {FIG_DIR}/sample_distance_clustermap.pdf")
    except ImportError:
        print("  (skipping clustermap — install seaborn)")
except Exception as e:
    print(f"  WARNING: sample-distance computation failed: {e}")

# ============================================================================
# 6. (OPTIONAL) PER-CELL SAMPLE-DISTANCE MATRICES
# ============================================================================

if COMPUTE_PER_CELL_DISTANCES:
    print("\nComputing per-cell sample-distance matrices …")
    try:
        per_cell_dist = model.get_local_sample_distances(adata=adata, keep_cell=True)
        per_cell_dist = np.asarray(per_cell_dist)
        np.save(f"{RESULTS_DIR}/sample_distance_per_cell.npy", per_cell_dist)
        print(f"  shape: {per_cell_dist.shape} → "
              f"{RESULTS_DIR}/sample_distance_per_cell.npy")
    except Exception as e:
        print(f"  WARNING: per-cell sample-distance failed: {e}")

# ============================================================================
# 7. (OPTIONAL) DA + DE BETWEEN SAMPLE GROUPS
# ============================================================================

if GROUP_COL and GROUP1 and GROUP2:
    if GROUP_COL not in adata.obs.columns:
        print(f"\nWARNING: GROUP_COL '{GROUP_COL}' not in obs — skipping DA/DE")
    else:
        print(f"\n=== Differential abundance ({GROUP1} vs {GROUP2}) ===")
        try:
            da_df = model.differential_abundance(
                adata=adata,
                sample_cov_keys=[GROUP_COL],
                group1=GROUP1, group2=GROUP2,
            )
            if hasattr(da_df, "to_csv"):
                da_df.to_csv(f"{RESULTS_DIR}/da_{GROUP1}_vs_{GROUP2}.tsv", sep="\t")
                if "log2FC" in da_df.columns:
                    adata.obs["DA_log2FC"] = da_df["log2FC"].values
                    sc.pl.umap(adata, color=["DA_log2FC", "leiden"],
                                vmin=-2, vmax=2, cmap="RdBu_r",
                                save="_mrvi_DA.pdf")
            print(f"  → {RESULTS_DIR}/da_{GROUP1}_vs_{GROUP2}.tsv")
        except Exception as e:
            print(f"  WARNING: DA failed: {e}")

        print(f"\n=== Differential expression ({GROUP1} vs {GROUP2}) ===")
        try:
            de_df = model.differential_expression(
                adata=adata,
                sample_cov_keys=[GROUP_COL],
                group1=GROUP1, group2=GROUP2,
            )
            if hasattr(de_df, "to_csv"):
                de_df.to_csv(f"{RESULTS_DIR}/de_{GROUP1}_vs_{GROUP2}.tsv", sep="\t")
            print(f"  → {RESULTS_DIR}/de_{GROUP1}_vs_{GROUP2}.tsv")
        except Exception as e:
            print(f"  WARNING: DE failed: {e}")

# ============================================================================
# 8. SAVE
# ============================================================================

print(f"\nWriting {OUTPUT_H5AD} …")
adata.write_h5ad(OUTPUT_H5AD)
print("\nDone.")
print(f"  Output:     {OUTPUT_H5AD}")
print(f"  Model:      {MODEL_DIR}")
print(f"  Figures:    {FIG_DIR}/")
print(f"  Tables:     {RESULTS_DIR}/")
