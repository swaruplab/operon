#!/usr/bin/env python3
"""
Spatial transcriptomics QC — platform-aware, quantile-based by default.

Loads a spatial dataset (any platform that produces an h5ad with
adata.obsm['spatial']), computes QC metrics, applies quantile-based
filtering adapted to the dataset's own distributions, and writes a
filtered .h5ad plus QC figures.

Usage:
    python spatial_qc.py input.h5ad --platform xenium --output filtered.h5ad
    python spatial_qc.py outs/ --platform visium  # spaceranger output dir
    python spatial_qc.py input.h5ad --hard-thresholds --min-genes 50  # legacy mode
"""

import argparse
import os
from pathlib import Path

import numpy as np
import scanpy as sc
import squidpy as sq
import matplotlib.pyplot as plt


def load_by_platform(path, platform):
    """Dispatch to the right loader per platform. Returns an AnnData."""
    p = Path(path)
    platform = platform.lower()

    if platform == 'visium':
        # spaceranger output dir
        adata = sc.read_visium(str(p))
    elif platform == 'xenium':
        adata = sq.read.xenium(str(p))
    elif platform == 'cosmx':
        adata = sq.read.nanostring(
            str(p),
            counts_file='exprMat_file.csv',
            meta_file='metadata_file.csv',
        )
    elif platform == 'merfish':
        adata = sq.read.vizgen(
            str(p),
            counts_file='cell_by_gene.csv',
            meta_file='cell_metadata.csv',
        )
    elif platform in ('slideseq', 'slide-seq', 'geomx', 'h5ad'):
        # Assume the file is already an h5ad with adata.obsm['spatial'] set.
        adata = sc.read_h5ad(str(p))
    else:
        raise ValueError(f"Unknown platform: {platform!r}. "
                         "Use one of: visium, xenium, cosmx, merfish, slideseq, geomx, h5ad")

    adata.var_names_make_unique()

    # Sanity check: spatial coords must be present and shaped (n_obs, 2)
    if 'spatial' not in adata.obsm:
        raise RuntimeError(
            f"Loaded {adata.n_obs} obs but adata.obsm['spatial'] is missing. "
            "Set spatial coordinates manually before continuing."
        )
    if adata.obsm['spatial'].shape != (adata.n_obs, 2):
        raise RuntimeError(
            f"adata.obsm['spatial'] has shape {adata.obsm['spatial'].shape}, "
            f"expected ({adata.n_obs}, 2)."
        )
    return adata


def remove_control_probes(adata, platform):
    """Strip negative-probe / blank-probe rows for panel-based platforms."""
    masks = {
        'cosmx': adata.var_names.str.startswith(('NegPrb', 'SystemControl', 'Negative')),
        'merfish': adata.var_names.str.startswith(('Blank', 'blank')),
        'xenium': adata.var_names.str.startswith(('NegControl', 'antisense_', 'BLANK_')),
    }
    mask = masks.get(platform.lower())
    if mask is None or mask.sum() == 0:
        return adata
    print(f"Removing {int(mask.sum())} control/blank probes for {platform}.")
    return adata[:, ~mask].copy()


def annotate_qc_metrics(adata):
    """Identify mito genes and compute QC metrics inplace."""
    adata.var['mt'] = adata.var_names.str.startswith(('MT-', 'mt-', 'Mt-'))
    sc.pp.calculate_qc_metrics(adata, qc_vars=['mt'], percent_top=None,
                                log1p=False, inplace=True)
    # Some spatial platforms have no MT probes — column is all 0, that's fine.
    print(f"QC metrics computed. {adata.var['mt'].sum()} MT genes detected.")


def quantile_filter(adata, gene_q=(0.05, 0.99), count_q=(0.05, 0.99),
                    mt_q=0.99, mt_ceiling=20.0, min_cells=3):
    """Quantile-based filtering. Returns the filtered AnnData."""
    n_cells_before, n_genes_before = adata.n_obs, adata.n_vars

    gene_lo = float(np.quantile(adata.obs['n_genes_by_counts'], gene_q[0]))
    gene_hi = float(np.quantile(adata.obs['n_genes_by_counts'], gene_q[1]))
    count_lo = float(np.quantile(adata.obs['total_counts'], count_q[0]))
    count_hi = float(np.quantile(adata.obs['total_counts'], count_q[1]))
    mt_hi = float(min(np.quantile(adata.obs['pct_counts_mt'], mt_q), mt_ceiling))

    print(f"\n=== Quantile QC thresholds (from this dataset) ===")
    print(f"  n_genes_by_counts ∈ [{gene_lo:.0f}, {gene_hi:.0f}]  ({gene_q[0]:.0%} – {gene_q[1]:.0%})")
    print(f"  total_counts      ∈ [{count_lo:.0f}, {count_hi:.0f}]  ({count_q[0]:.0%} – {count_q[1]:.0%})")
    print(f"  pct_counts_mt     < {mt_hi:.2f}  (min of {mt_q:.0%}-ile and {mt_ceiling}% ceiling)")

    keep = (
        (adata.obs['n_genes_by_counts'] >= gene_lo) &
        (adata.obs['n_genes_by_counts'] <= gene_hi) &
        (adata.obs['total_counts'] >= count_lo) &
        (adata.obs['total_counts'] <= count_hi) &
        (adata.obs['pct_counts_mt'] < mt_hi)
    )
    adata = adata[keep, :].copy()
    sc.pp.filter_genes(adata, min_cells=min_cells)

    print(f"\nCells: {n_cells_before} -> {adata.n_obs} ({adata.n_obs/n_cells_before*100:.1f}% retained)")
    print(f"Genes: {n_genes_before} -> {adata.n_vars} ({adata.n_vars/n_genes_before*100:.1f}% retained)")
    return adata


def hard_filter(adata, mt_threshold=5, min_genes=50, min_cells=3):
    """Legacy hard-threshold filtering. For cross-dataset reproducibility."""
    n_cells_before, n_genes_before = adata.n_obs, adata.n_vars
    sc.pp.filter_cells(adata, min_genes=min_genes)
    adata = adata[adata.obs.pct_counts_mt < mt_threshold, :].copy()
    sc.pp.filter_genes(adata, min_cells=min_cells)
    print(f"\nCells: {n_cells_before} -> {adata.n_obs} ({adata.n_obs/n_cells_before*100:.1f}% retained)")
    print(f"Genes: {n_genes_before} -> {adata.n_vars} ({adata.n_vars/n_genes_before*100:.1f}% retained)")
    return adata


def plot_qc(adata, prefix, fig_dir='figures'):
    """QC plots: violin, scatter, and spatial scatter of total_counts."""
    os.makedirs(fig_dir, exist_ok=True)
    sc.settings.figdir = fig_dir

    sc.pl.violin(adata, ['n_genes_by_counts', 'total_counts', 'pct_counts_mt'],
                 jitter=0.4, multi_panel=True, save=f'_{prefix}_violin.pdf')

    sc.pl.scatter(adata, x='total_counts', y='pct_counts_mt',
                   save=f'_{prefix}_mt.pdf')
    sc.pl.scatter(adata, x='total_counts', y='n_genes_by_counts',
                   save=f'_{prefix}_genes.pdf')

    # Spatial scatter of total_counts is the single most useful spatial QC plot —
    # exposes tissue edges, fold artifacts, off-tissue beads.
    try:
        sq.pl.spatial_scatter(adata, color='total_counts', shape=None,
                               vmax='p99', save=f'{fig_dir}/{prefix}_spatial_counts.pdf')
    except Exception as e:
        print(f"(Skipping spatial scatter: {e})")


def main():
    parser = argparse.ArgumentParser(description='Spatial transcriptomics QC')
    parser.add_argument('input', help='Path to data (h5ad, spaceranger out dir, or platform-specific dir)')
    parser.add_argument('--platform', required=True,
                        choices=['visium', 'xenium', 'cosmx', 'merfish', 'slideseq', 'geomx', 'h5ad'],
                        help='Source platform')
    parser.add_argument('--output', default='filtered.h5ad',
                        help='Output h5ad file (default: filtered.h5ad)')
    parser.add_argument('--hard-thresholds', action='store_true',
                        help='Use fixed thresholds (--min-genes / --mt-threshold) instead of quantiles')
    parser.add_argument('--mt-threshold', type=float, default=5,
                        help='Max pct_counts_mt (hard-thresholds mode only, default 5)')
    parser.add_argument('--min-genes', type=int, default=50,
                        help='Min n_genes_by_counts (hard-thresholds mode only, default 50)')
    parser.add_argument('--gene-q-lo', type=float, default=0.05)
    parser.add_argument('--gene-q-hi', type=float, default=0.99)
    parser.add_argument('--count-q-lo', type=float, default=0.05)
    parser.add_argument('--count-q-hi', type=float, default=0.99)
    parser.add_argument('--mt-q-hi', type=float, default=0.99)
    parser.add_argument('--mt-ceiling', type=float, default=20.0)
    parser.add_argument('--min-cells', type=int, default=3,
                        help='Min cells per gene (default 3)')
    parser.add_argument('--skip-plots', action='store_true')

    args = parser.parse_args()
    sc.settings.verbosity = 2

    print(f"Loading {args.input} as {args.platform}…")
    adata = load_by_platform(args.input, args.platform)
    print(f"Loaded: {adata.n_obs} cells/spots × {adata.n_vars} genes")

    adata = remove_control_probes(adata, args.platform)
    annotate_qc_metrics(adata)

    if not args.skip_plots:
        plot_qc(adata, prefix='before')

    if args.hard_thresholds:
        adata = hard_filter(adata, mt_threshold=args.mt_threshold,
                            min_genes=args.min_genes, min_cells=args.min_cells)
    else:
        adata = quantile_filter(
            adata,
            gene_q=(args.gene_q_lo, args.gene_q_hi),
            count_q=(args.count_q_lo, args.count_q_hi),
            mt_q=args.mt_q_hi,
            mt_ceiling=args.mt_ceiling,
            min_cells=args.min_cells,
        )

    if not args.skip_plots:
        plot_qc(adata, prefix='after')

    print(f"\nWriting {args.output}…")
    adata.write_h5ad(args.output)
    print("Done.")


if __name__ == '__main__':
    main()
