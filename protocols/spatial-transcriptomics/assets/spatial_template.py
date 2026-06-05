#!/usr/bin/env python3
"""
Spatial Transcriptomics — End-to-End Template

Single parameterized script for any of: Visium, Xenium, CosMx, MERFISH,
Slide-seq, GeoMx. Set PLATFORM and INPUT_PATH below, then run end-to-end.

The pipeline:
  1. Load data (platform-aware)
  2. Strip control probes (panel-based platforms)
  3. Compute QC metrics and apply quantile-based filtering
  4. Normalize + log1p
  5. HVG → PCA → neighbors → UMAP → leiden
  6. Build spatial graph (squidpy)
  7. Neighborhood enrichment + co-occurrence + Moran's I
  8. Save figures and an annotated .h5ad
"""

import os
from pathlib import Path

import numpy as np
import pandas as pd
import scanpy as sc
import squidpy as sq
import matplotlib.pyplot as plt

# ============================================================================
# CONFIGURATION
# ============================================================================

PLATFORM = 'visium'                          # visium | xenium | cosmx | merfish | slideseq | geomx
INPUT_PATH = 'data/sample_outs/'             # spaceranger out dir, h5ad, or platform-specific dir
OUTPUT_FILE = 'results/annotated.h5ad'
FIGURES_DIR = 'figures/'

# QC parameters — quantile-based by default
USE_QUANTILE_QC = True
GENE_Q = (0.05, 0.99)
COUNT_Q = (0.05, 0.99)
MT_Q_HI = 0.99
MT_HARD_CEILING = 20.0
HARD_MIN_GENES = 50          # only used if USE_QUANTILE_QC = False
HARD_MT_THRESHOLD = 5
MIN_CELLS = 3                # gene-level floor (constant across modes)

# Preprocessing
N_TOP_GENES = 2000           # set None to skip HVG selection (recommended for small panels)
N_PCS = 30
N_NEIGHBORS = 15
LEIDEN_RESOLUTION = 0.5

# Spatial neighborhood
N_SPATIAL_NEIGHS = 6         # k-NN spatial neighbors; raised for Slide-seq beads
MAX_RADIUS_CO = 500.0        # co-occurrence max radius (units = platform-native, often µm)
N_PERMS = 1000               # permutation count for nhood_enrichment + autocorr
N_JOBS = 4

# Scanpy / squidpy settings
sc.settings.verbosity = 3
sc.settings.set_figure_params(dpi=80, facecolor='white')
sc.settings.figdir = FIGURES_DIR
os.makedirs(FIGURES_DIR, exist_ok=True)
os.makedirs(Path(OUTPUT_FILE).parent or '.', exist_ok=True)

# ============================================================================
# 1. LOAD DATA (platform-aware)
# ============================================================================

print("=" * 80)
print(f"LOADING {PLATFORM.upper()} DATA")
print("=" * 80)

if PLATFORM == 'visium':
    adata = sc.read_visium(INPUT_PATH)
elif PLATFORM == 'xenium':
    adata = sq.read.xenium(INPUT_PATH)
elif PLATFORM == 'cosmx':
    adata = sq.read.nanostring(
        INPUT_PATH,
        counts_file='exprMat_file.csv',
        meta_file='metadata_file.csv',
    )
elif PLATFORM == 'merfish':
    adata = sq.read.vizgen(
        INPUT_PATH,
        counts_file='cell_by_gene.csv',
        meta_file='cell_metadata.csv',
    )
elif PLATFORM in ('slideseq', 'geomx'):
    # Assume an h5ad pre-built with adata.obsm['spatial'] set.
    adata = sc.read_h5ad(INPUT_PATH)
else:
    raise ValueError(f"Unknown PLATFORM={PLATFORM!r}")

adata.var_names_make_unique()
assert 'spatial' in adata.obsm, "adata.obsm['spatial'] missing — required for spatial analysis"

print(f"Loaded: {adata.n_obs} cells/spots × {adata.n_vars} genes")
print(f"Spatial coord range: x ∈ [{adata.obsm['spatial'][:,0].min():.0f}, {adata.obsm['spatial'][:,0].max():.0f}], "
      f"y ∈ [{adata.obsm['spatial'][:,1].min():.0f}, {adata.obsm['spatial'][:,1].max():.0f}]")

# ============================================================================
# 2. STRIP CONTROL PROBES (panel platforms)
# ============================================================================

control_masks = {
    'cosmx': adata.var_names.str.startswith(('NegPrb', 'SystemControl', 'Negative')),
    'merfish': adata.var_names.str.startswith(('Blank', 'blank')),
    'xenium': adata.var_names.str.startswith(('NegControl', 'antisense_', 'BLANK_')),
}
if PLATFORM in control_masks:
    mask = control_masks[PLATFORM]
    if mask.sum() > 0:
        print(f"Removing {int(mask.sum())} control/blank probes.")
        adata = adata[:, ~mask].copy()

# ============================================================================
# 3. QC + FILTERING (quantile-based)
# ============================================================================

print("\n" + "=" * 80)
print("QUALITY CONTROL")
print("=" * 80)

adata.var['mt'] = adata.var_names.str.startswith(('MT-', 'mt-', 'Mt-'))
sc.pp.calculate_qc_metrics(adata, qc_vars=['mt'], percent_top=None,
                            log1p=False, inplace=True)

# QC plots before filtering
sc.pl.violin(adata, ['n_genes_by_counts', 'total_counts', 'pct_counts_mt'],
             jitter=0.4, multi_panel=True, save='_qc_before.pdf')
try:
    sq.pl.spatial_scatter(adata, color='total_counts', shape=None,
                           vmax='p99', save=f'{FIGURES_DIR}/qc_before_spatial.pdf')
except Exception as e:
    print(f"(spatial QC scatter skipped: {e})")

print(f"\nBefore filtering: {adata.n_obs} cells/spots, {adata.n_vars} genes")

if USE_QUANTILE_QC:
    gene_lo = float(np.quantile(adata.obs['n_genes_by_counts'], GENE_Q[0]))
    gene_hi = float(np.quantile(adata.obs['n_genes_by_counts'], GENE_Q[1]))
    count_lo = float(np.quantile(adata.obs['total_counts'], COUNT_Q[0]))
    count_hi = float(np.quantile(adata.obs['total_counts'], COUNT_Q[1]))
    mt_hi = float(min(np.quantile(adata.obs['pct_counts_mt'], MT_Q_HI), MT_HARD_CEILING))
    print(f"Quantile QC thresholds:")
    print(f"  n_genes_by_counts ∈ [{gene_lo:.0f}, {gene_hi:.0f}]")
    print(f"  total_counts      ∈ [{count_lo:.0f}, {count_hi:.0f}]")
    print(f"  pct_counts_mt     < {mt_hi:.2f}")
    adata = adata[
        (adata.obs['n_genes_by_counts'] >= gene_lo) &
        (adata.obs['n_genes_by_counts'] <= gene_hi) &
        (adata.obs['total_counts'] >= count_lo) &
        (adata.obs['total_counts'] <= count_hi) &
        (adata.obs['pct_counts_mt'] < mt_hi), :
    ].copy()
else:
    sc.pp.filter_cells(adata, min_genes=HARD_MIN_GENES)
    adata = adata[adata.obs.pct_counts_mt < HARD_MT_THRESHOLD, :].copy()

sc.pp.filter_genes(adata, min_cells=MIN_CELLS)
print(f"After filtering:  {adata.n_obs} cells/spots, {adata.n_vars} genes")

# ============================================================================
# 4. NORMALIZATION
# ============================================================================

print("\n" + "=" * 80)
print("NORMALIZATION")
print("=" * 80)

sc.pp.normalize_total(adata, target_sum=1e4)
sc.pp.log1p(adata)
adata.raw = adata

# ============================================================================
# 5. HVG + PCA + NEIGHBORS + UMAP + LEIDEN
# ============================================================================

print("\n" + "=" * 80)
print("DIMENSIONALITY REDUCTION + CLUSTERING")
print("=" * 80)

# Skip HVG for small-panel platforms — there aren't enough genes to be picky.
use_hvg = N_TOP_GENES is not None and adata.n_vars > N_TOP_GENES * 1.5
if use_hvg:
    sc.pp.highly_variable_genes(adata, n_top_genes=N_TOP_GENES, flavor='seurat_v3')
    print(f"Selected {int(adata.var['highly_variable'].sum())} HVGs.")
else:
    print(f"Using all {adata.n_vars} genes (panel is too small for HVG).")

sc.pp.scale(adata, max_value=10)
sc.tl.pca(adata, n_comps=min(N_PCS, adata.n_vars - 1, adata.n_obs - 1),
          use_highly_variable=use_hvg)
sc.pp.neighbors(adata, n_neighbors=N_NEIGHBORS, n_pcs=N_PCS)
sc.tl.umap(adata)
sc.tl.leiden(adata, resolution=LEIDEN_RESOLUTION)

print(f"Clusters found: {adata.obs['leiden'].nunique()}")
sc.pl.umap(adata, color='leiden', save='_clusters_umap.pdf')

# ============================================================================
# 6. SPATIAL NEIGHBORHOOD GRAPH
# ============================================================================

print("\n" + "=" * 80)
print("SPATIAL NEIGHBORHOOD GRAPH")
print("=" * 80)

if PLATFORM == 'visium':
    sq.gr.spatial_neighbors(adata, coord_type='grid',
                             n_neighs=N_SPATIAL_NEIGHS, n_rings=1)
else:
    sq.gr.spatial_neighbors(adata, coord_type='generic', n_neighs=N_SPATIAL_NEIGHS)
print(f"Spatial graph: {adata.obsp['spatial_connectivities'].nnz} edges")

# ============================================================================
# 7. SPATIAL ANALYSES
# ============================================================================

print("\n" + "=" * 80)
print("SPATIAL ANALYSES")
print("=" * 80)

# Neighborhood enrichment
sq.gr.nhood_enrichment(adata, cluster_key='leiden',
                        n_perms=N_PERMS, n_jobs=N_JOBS, seed=0)
sq.pl.nhood_enrichment(adata, cluster_key='leiden', method='single',
                        figsize=(8, 8), save=f'{FIGURES_DIR}/nhood_enrichment.pdf')

# Co-occurrence
interval = np.linspace(0, MAX_RADIUS_CO, 50)
sq.gr.co_occurrence(adata, cluster_key='leiden', interval=interval, n_jobs=N_JOBS)
first_cluster = str(sorted(adata.obs['leiden'].unique())[0])
sq.pl.co_occurrence(adata, cluster_key='leiden', clusters=first_cluster,
                     save=f'{FIGURES_DIR}/co_occurrence_cluster{first_cluster}.pdf')

# Moran's I (spatially variable genes)
sq.gr.spatial_autocorr(adata, mode='moran', n_perms=N_PERMS, n_jobs=N_JOBS)
print(f"\nTop 10 spatially variable genes (Moran's I):")
print(adata.uns['moranI'].head(10).to_string())

# Plot top 4 spatially variable genes in the tissue
top4 = list(adata.uns['moranI'].index[:4])
try:
    sq.pl.spatial_scatter(adata, color=top4, shape=None, ncols=2, size=4,
                           cmap='magma', vmax='p99',
                           save=f'{FIGURES_DIR}/top_moran.pdf')
except Exception as e:
    print(f"(top moran plot skipped: {e})")

# ============================================================================
# 8. CLUSTER MAP
# ============================================================================

if PLATFORM == 'visium':
    sc.pl.spatial(adata, color='leiden', img_key='hires', alpha=0.8,
                   save='_clusters_spatial.pdf')
else:
    try:
        sq.pl.spatial_scatter(adata, color='leiden', shape=None, size=4,
                               save=f'{FIGURES_DIR}/clusters_spatial.pdf')
    except Exception as e:
        print(f"(spatial cluster plot skipped: {e})")

# ============================================================================
# SAVE
# ============================================================================

print(f"\nWriting {OUTPUT_FILE}…")
adata.write_h5ad(OUTPUT_FILE)
print("\nDone. Inspect figures in:", FIGURES_DIR)
