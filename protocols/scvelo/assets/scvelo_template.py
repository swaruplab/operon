#!/usr/bin/env python3
"""
scvelo_template.py — end-to-end RNA velocity workflow.

Configure the CONFIGURATION block, then run end-to-end:
  1. Load AnnData (with spliced/unspliced layers)
  2. Spliced/unspliced proportion diagnostic
  3. filter_and_normalize + moments
  4. (Optional) recover_dynamics — the slow step for dynamical mode
  5. velocity + velocity_graph
  6. Streamline + grid plots
  7. (Optional) latent_time
  8. Driver gene ranking
  9. (Optional) differential kinetics
  10. Save augmented AnnData
"""

import os
from pathlib import Path

import scvelo as scv
import scanpy as sc

# ============================================================================
# CONFIGURATION
# ============================================================================

# Input — an AnnData with adata.layers['spliced'] and ['unspliced']
INPUT_H5AD       = "data/adata.h5ad"
CLUSTER_COL      = "clusters"
BASIS            = "umap"

# Output
OUTPUT_H5AD      = "results/adata_velocity.h5ad"
FIG_DIR          = "figures"

# Preprocessing
MIN_SHARED       = 20
N_TOP_GENES      = 2000
N_PCS            = 30
N_NEIGHBORS      = 30

# Velocity model
MODE             = "dynamical"        # "deterministic" | "stochastic" | "dynamical"
RECOVER_DYNAMICS = True               # required for dynamical; ignored otherwise
COMPUTE_LATENT_TIME = True            # requires dynamical
RUN_DIFF_KINETICS = False             # requires dynamical
N_DK_GENES       = 50

# Performance
N_JOBS           = 8

# ============================================================================
# SETUP
# ============================================================================

Path(FIG_DIR).mkdir(parents=True, exist_ok=True)
Path(OUTPUT_H5AD).parent.mkdir(parents=True, exist_ok=True)
scv.settings.figdir = FIG_DIR
scv.settings.set_figure_params('scvelo', dpi=80, facecolor='white')

# ============================================================================
# 1. LOAD
# ============================================================================

print("=" * 80)
print(f"LOAD: {INPUT_H5AD}")
print("=" * 80)
adata = scv.read(INPUT_H5AD)
assert "spliced"   in adata.layers, "Missing adata.layers['spliced']"
assert "unspliced" in adata.layers, "Missing adata.layers['unspliced']"
assert CLUSTER_COL in adata.obs.columns, f"CLUSTER_COL '{CLUSTER_COL}' missing"
print(f"  {adata.n_obs} cells × {adata.n_vars} genes")

# ============================================================================
# 2. SPLICED/UNSPLICED DIAGNOSTIC
# ============================================================================

print("\n=== Spliced/unspliced proportions ===")
try:
    scv.pl.proportions(adata, save="proportions.pdf")
except Exception as e:
    print(f"  (skipped: {e})")

# ============================================================================
# 3. PREPROCESS
# ============================================================================

print("\n=== Preprocess ===")
scv.pp.filter_and_normalize(adata,
                             min_shared_counts=MIN_SHARED,
                             n_top_genes=N_TOP_GENES)
scv.pp.moments(adata, n_pcs=N_PCS, n_neighbors=N_NEIGHBORS)

# ============================================================================
# 4. RECOVER DYNAMICS (slow step for dynamical mode)
# ============================================================================

if RECOVER_DYNAMICS or MODE == "dynamical":
    print(f"\n=== Recover dynamics (n_jobs={N_JOBS}) ===")
    scv.tl.recover_dynamics(adata, n_jobs=N_JOBS)

# ============================================================================
# 5. VELOCITY + GRAPH
# ============================================================================

print(f"\n=== Velocity (mode={MODE}) + graph ===")
scv.tl.velocity(adata, mode=MODE)
scv.tl.velocity_graph(adata)

# ============================================================================
# 6. STREAMLINE + GRID PLOTS
# ============================================================================

print("\n=== Streamline plots ===")
scv.pl.velocity_embedding_stream(adata, basis=BASIS, color=CLUSTER_COL,
                                  save="velocity_stream.pdf")
scv.pl.velocity_embedding_grid(adata, basis=BASIS, color=CLUSTER_COL,
                                arrow_length=3, arrow_size=2,
                                save="velocity_grid.pdf")

# ============================================================================
# 7. LATENT TIME
# ============================================================================

if COMPUTE_LATENT_TIME and MODE == "dynamical":
    print("\n=== Latent time ===")
    scv.tl.latent_time(adata)
    scv.pl.scatter(adata, color="latent_time", color_map="gnuplot", size=80,
                    save="latent_time.pdf")

# ============================================================================
# 8. DRIVER GENES
# ============================================================================

print("\n=== Driver genes ===")
if MODE == "dynamical":
    scv.tl.rank_dynamical_genes(adata, groupby=CLUSTER_COL)
    df = scv.get_df(adata, 'rank_dynamical_genes/names')
    print(df.head().to_string())

    # Top likelihood gene heatmap along latent time (if computed)
    if COMPUTE_LATENT_TIME and "latent_time" in adata.obs.columns:
        top_genes = adata.var['fit_likelihood'].sort_values(ascending=False).index[:300]
        scv.pl.heatmap(adata, var_names=top_genes,
                        sortby="latent_time", col_color=CLUSTER_COL,
                        n_convolve=100, yticklabels=True,
                        figsize=(8, 12), save="latent_time_heatmap.pdf")
else:
    scv.tl.rank_velocity_genes(adata, groupby=CLUSTER_COL, min_corr=0.3)
    df = scv.DataFrame(adata.uns['rank_velocity_genes']['names'])
    print(df.head().to_string())

# ============================================================================
# 9. DIFFERENTIAL KINETICS
# ============================================================================

if RUN_DIFF_KINETICS and MODE == "dynamical":
    print(f"\n=== Differential kinetics (top {N_DK_GENES} genes) ===")
    top_genes = adata.var['fit_likelihood'].sort_values(ascending=False).index[:N_DK_GENES]
    scv.tl.differential_kinetic_test(
        adata, var_names=list(top_genes), groupby=CLUSTER_COL
    )

    print("Re-computing velocity with diff_kinetics=True …")
    scv.tl.velocity(adata, diff_kinetics=True)
    scv.tl.velocity_graph(adata)
    scv.pl.velocity_embedding_stream(adata, basis=BASIS, color=CLUSTER_COL,
                                      save="velocity_stream_diffkin.pdf")

# ============================================================================
# 10. CONFIDENCE
# ============================================================================

print("\n=== Velocity confidence ===")
try:
    scv.tl.velocity_confidence(adata)
    scv.pl.scatter(adata, c=['velocity_length', 'velocity_confidence'],
                    cmap='coolwarm', perc=[5, 95],
                    save='velocity_confidence.pdf')
except Exception as e:
    print(f"  (skipped: {e})")

# ============================================================================
# 11. SAVE
# ============================================================================

print(f"\n=== Save → {OUTPUT_H5AD} ===")
adata.write(OUTPUT_H5AD)
print("Done.")
print(f"  Figures: {FIG_DIR}/")
