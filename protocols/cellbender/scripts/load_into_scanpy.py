#!/usr/bin/env python3
"""
load_into_scanpy.py — read a CellBender filtered output into scanpy and
run standard preprocessing.

Usage:
    python load_into_scanpy.py SAMPLE_CB.h5 [--output processed.h5ad]

Steps:
  1. anndata_from_h5 → AnnData
  2. Keep cells with cell_probability > 0.5
  3. Standard scanpy preprocessing: filter_genes, normalize_total, log1p, HVG, scale, PCA, neighbors, UMAP, leiden
  4. Write processed h5ad
"""

import argparse
import sys
from pathlib import Path

try:
    import scanpy as sc
    import numpy as np
    from cellbender.remove_background.downstream import anndata_from_h5
except ImportError as e:
    sys.exit(f"Missing dependency: {e}. Install with: pip install scanpy cellbender")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("input", help="CellBender output .h5 (typically the *_filtered.h5)")
    parser.add_argument("--output", default="processed.h5ad",
                        help="Output .h5ad path (default: processed.h5ad)")
    parser.add_argument("--cell-prob-thresh", type=float, default=0.5,
                        help="Drop cells with cell_probability below this (default: 0.5)")
    parser.add_argument("--n-top-genes", type=int, default=2000,
                        help="HVG count (default: 2000)")
    parser.add_argument("--resolution", type=float, default=0.5,
                        help="leiden resolution (default: 0.5)")
    args = parser.parse_args()

    print(f"Loading {args.input} …")
    adata = anndata_from_h5(args.input)
    print(f"  loaded: {adata.n_obs} barcodes × {adata.n_vars} genes")

    # Keep called cells
    if "cell_probability" in adata.obs:
        mask = adata.obs["cell_probability"] > args.cell_prob_thresh
        adata = adata[mask].copy()
        print(f"  after cell_probability > {args.cell_prob_thresh}: {adata.n_obs} cells")
    else:
        print("  (no cell_probability column — assuming all are cells)")

    # Standard preprocessing
    print("Filtering low-prevalence genes …")
    sc.pp.filter_genes(adata, min_cells=3)

    print("Normalize + log1p …")
    sc.pp.normalize_total(adata, target_sum=1e4)
    sc.pp.log1p(adata)
    adata.raw = adata

    print(f"Selecting {args.n_top_genes} HVGs …")
    sc.pp.highly_variable_genes(adata, n_top_genes=args.n_top_genes,
                                 flavor="seurat_v3", layer=None,
                                 inplace=True)
    sc.pp.scale(adata, max_value=10)

    print("PCA + neighbors + UMAP + leiden …")
    n_pcs = min(50, adata.n_vars - 1, adata.n_obs - 1)
    sc.tl.pca(adata, n_comps=n_pcs, use_highly_variable=True)
    sc.pp.neighbors(adata, n_neighbors=15, n_pcs=min(30, n_pcs))
    sc.tl.umap(adata)
    sc.tl.leiden(adata, resolution=args.resolution)

    print(f"Clusters found: {adata.obs['leiden'].nunique()}")
    out_path = Path(args.output)
    print(f"Writing {out_path} …")
    adata.write_h5ad(out_path)
    print("Done.")


if __name__ == "__main__":
    main()
