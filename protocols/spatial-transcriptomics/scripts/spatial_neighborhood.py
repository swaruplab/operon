#!/usr/bin/env python3
"""
Spatial neighborhood analysis — builds the spatial graph, computes
neighborhood enrichment, co-occurrence, and spatially variable genes
(Moran's I), saves results and figures.

Assumes the input AnnData already has:
  - adata.obsm['spatial'] (n_obs, 2)
  - normalized + log1p data in adata.X
  - a cluster label in adata.obs[--cluster-key] (default 'leiden')

Usage:
    python spatial_neighborhood.py filtered.h5ad --cluster-key leiden --output annotated.h5ad
"""

import argparse
import os

import numpy as np
import scanpy as sc
import squidpy as sq
import matplotlib.pyplot as plt


def build_graph(adata, platform, n_neighs=6, radius=None):
    """Build the spatial neighborhood graph appropriate for the platform."""
    if platform.lower() == 'visium':
        # Visium spots sit on a hexagonal grid.
        sq.gr.spatial_neighbors(adata, coord_type='grid', n_neighs=n_neighs, n_rings=1)
    else:
        # Cell-based: Delaunay-style k-NN, or radius if requested.
        if radius is not None:
            sq.gr.spatial_neighbors(adata, coord_type='generic', radius=radius)
        else:
            sq.gr.spatial_neighbors(adata, coord_type='generic', n_neighs=n_neighs)
    print(f"Spatial graph built: {adata.obsp['spatial_connectivities'].nnz} edges")


def neighborhood_enrichment(adata, cluster_key='leiden', n_perms=1000, n_jobs=4):
    sq.gr.nhood_enrichment(adata, cluster_key=cluster_key,
                            n_perms=n_perms, n_jobs=n_jobs, seed=0)
    print(f"Neighborhood enrichment computed for '{cluster_key}'.")


def co_occurrence(adata, cluster_key='leiden', max_radius=500, n_steps=50, n_jobs=4):
    interval = np.linspace(0, max_radius, n_steps)
    sq.gr.co_occurrence(adata, cluster_key=cluster_key, interval=interval, n_jobs=n_jobs)
    print(f"Co-occurrence computed across {n_steps} radii up to {max_radius}.")


def spatial_autocorrelation(adata, n_perms=1000, n_jobs=4, mode='moran'):
    sq.gr.spatial_autocorr(adata, mode=mode, n_perms=n_perms, n_jobs=n_jobs)
    result_key = 'moranI' if mode == 'moran' else 'gearyC'
    df = adata.uns[result_key]
    print(f"\n=== Top 20 spatially variable genes (Moran's I) ===")
    print(df.head(20).to_string())
    return df


def save_figures(adata, cluster_key='leiden', fig_dir='figures'):
    os.makedirs(fig_dir, exist_ok=True)
    sc.settings.figdir = fig_dir

    # Cluster map
    try:
        sq.pl.spatial_scatter(adata, color=cluster_key, shape=None, size=4,
                               save=f'{fig_dir}/cluster_map.pdf')
    except Exception as e:
        print(f"(spatial scatter failed: {e})")

    # Neighborhood enrichment
    try:
        sq.pl.nhood_enrichment(adata, cluster_key=cluster_key, method='single',
                                figsize=(8, 8),
                                save=f'{fig_dir}/nhood_enrichment.pdf')
    except Exception as e:
        print(f"(nhood enrichment plot failed: {e})")

    # Co-occurrence — plot for the first cluster as a sample
    try:
        first_cluster = str(adata.obs[cluster_key].unique()[0])
        sq.pl.co_occurrence(adata, cluster_key=cluster_key, clusters=first_cluster,
                             save=f'{fig_dir}/co_occurrence_{first_cluster}.pdf')
    except Exception as e:
        print(f"(co-occurrence plot failed: {e})")

    # Top spatially variable genes
    if 'moranI' in adata.uns:
        try:
            top4 = list(adata.uns['moranI'].index[:4])
            sq.pl.spatial_scatter(adata, color=top4, shape=None, ncols=2, size=4,
                                   cmap='magma', vmax='p99',
                                   save=f'{fig_dir}/top_moran_genes.pdf')
        except Exception as e:
            print(f"(moran top genes plot failed: {e})")


def main():
    parser = argparse.ArgumentParser(description='Spatial neighborhood analysis')
    parser.add_argument('input', help='Input h5ad with normalized data and cluster labels')
    parser.add_argument('--platform', default='generic',
                        choices=['visium', 'xenium', 'cosmx', 'merfish', 'slideseq', 'geomx', 'generic'],
                        help='Source platform (controls graph type)')
    parser.add_argument('--cluster-key', default='leiden',
                        help='adata.obs column with cluster labels (default leiden)')
    parser.add_argument('--n-neighs', type=int, default=6,
                        help='Spatial neighbors per cell (default 6)')
    parser.add_argument('--radius', type=float, default=None,
                        help='Use radius-based neighbors instead of k-NN (in coord units)')
    parser.add_argument('--max-radius-co', type=float, default=500,
                        help='Max radius for co-occurrence (default 500)')
    parser.add_argument('--n-perms', type=int, default=1000)
    parser.add_argument('--n-jobs', type=int, default=4)
    parser.add_argument('--output', default='annotated.h5ad')
    parser.add_argument('--skip-moran', action='store_true',
                        help="Skip Moran's I (it's the slowest step)")
    parser.add_argument('--skip-plots', action='store_true')

    args = parser.parse_args()
    sc.settings.verbosity = 2

    print(f"Loading {args.input}…")
    adata = sc.read_h5ad(args.input)
    print(f"Loaded: {adata.n_obs} cells/spots × {adata.n_vars} genes")

    if args.cluster_key not in adata.obs.columns:
        raise SystemExit(f"--cluster-key '{args.cluster_key}' not in adata.obs. "
                         f"Available: {list(adata.obs.columns)}")

    build_graph(adata, args.platform, n_neighs=args.n_neighs, radius=args.radius)
    neighborhood_enrichment(adata, cluster_key=args.cluster_key,
                             n_perms=args.n_perms, n_jobs=args.n_jobs)
    co_occurrence(adata, cluster_key=args.cluster_key,
                   max_radius=args.max_radius_co, n_jobs=args.n_jobs)

    if not args.skip_moran:
        spatial_autocorrelation(adata, n_perms=args.n_perms,
                                 n_jobs=args.n_jobs, mode='moran')

    if not args.skip_plots:
        save_figures(adata, cluster_key=args.cluster_key)

    print(f"\nWriting {args.output}…")
    adata.write_h5ad(args.output)
    print("Done.")


if __name__ == '__main__':
    main()
