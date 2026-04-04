#!/usr/bin/env python3
"""
scRNA-seq Analysis Template
Modify paths and parameters below, then run.
"""

import scanpy as sc
import numpy as np
import matplotlib.pyplot as plt

# === CONFIGURATION ===
INPUT_PATH = "data/raw_counts.h5ad"  # or .h5, or path to 10X folder
OUTPUT_DIR = "results/"
FIGURES_DIR = "figures/"
SAMPLE_NAME = "sample1"

# QC thresholds
MIN_GENES = 200
MAX_GENES = 5000
MAX_MITO_PCT = 20
MIN_CELLS = 3

# Analysis parameters
N_TOP_GENES = 2000
N_PCS = 30
N_NEIGHBORS = 15
LEIDEN_RESOLUTION = 0.5
RANDOM_STATE = 42

# === SETUP ===
sc.settings.figdir = FIGURES_DIR
sc.settings.verbosity = 3

# === LOAD ===
adata = sc.read_h5ad(INPUT_PATH)
print(f"Loaded: {adata.shape[0]} cells x {adata.shape[1]} genes")

# === QC ===
adata.var['mt'] = adata.var_names.str.startswith('MT-')
sc.pp.calculate_qc_metrics(adata, qc_vars=['mt'], percent_top=None, inplace=True)

sc.pl.violin(adata, ['n_genes_by_counts', 'total_counts', 'pct_counts_mt'], multi_panel=True, save=f'_{SAMPLE_NAME}_qc.png')

adata = adata[adata.obs.n_genes_by_counts > MIN_GENES, :]
adata = adata[adata.obs.n_genes_by_counts < MAX_GENES, :]
adata = adata[adata.obs.pct_counts_mt < MAX_MITO_PCT, :]
sc.pp.filter_genes(adata, min_cells=MIN_CELLS)

print(f"After QC: {adata.shape[0]} cells x {adata.shape[1]} genes")
adata.write(f"{OUTPUT_DIR}{SAMPLE_NAME}_qc.h5ad")

# === NORMALIZE ===
adata.raw = adata.copy()
sc.pp.normalize_total(adata, target_sum=1e4)
sc.pp.log1p(adata)

# === HVG + PCA ===
sc.pp.highly_variable_genes(adata, n_top_genes=N_TOP_GENES, flavor='seurat_v3')
adata = adata[:, adata.var.highly_variable]
sc.tl.pca(adata, n_comps=N_PCS, random_state=RANDOM_STATE)
sc.pl.pca_variance_ratio(adata, n_pcs=N_PCS, save=f'_{SAMPLE_NAME}_pca.png')

# === CLUSTER ===
sc.pp.neighbors(adata, n_neighbors=N_NEIGHBORS, n_pcs=N_PCS, random_state=RANDOM_STATE)
sc.tl.umap(adata, random_state=RANDOM_STATE)
sc.tl.leiden(adata, resolution=LEIDEN_RESOLUTION, random_state=RANDOM_STATE)

sc.pl.umap(adata, color=['leiden'], save=f'_{SAMPLE_NAME}_clusters.png')

# === MARKERS ===
sc.tl.rank_genes_groups(adata, groupby='leiden', method='wilcoxon')
sc.pl.rank_genes_groups(adata, n_genes=10, save=f'_{SAMPLE_NAME}_markers.png')

# === SAVE ===
adata.write(f"{OUTPUT_DIR}{SAMPLE_NAME}_analyzed.h5ad")
print("Done!")
