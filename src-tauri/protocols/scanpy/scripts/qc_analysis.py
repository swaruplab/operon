#!/usr/bin/env python3
"""
Quality Control Analysis Script for Scanpy

Performs comprehensive quality control on single-cell RNA-seq data,
including calculating metrics, generating QC plots, and filtering cells.

Usage:
    python qc_analysis.py <input_file> [--output <output_file>]
"""

import argparse
import numpy as np
import scanpy as sc
import matplotlib.pyplot as plt


def calculate_qc_metrics(adata, mt_threshold=5, min_genes=200, min_cells=3):
    """
    Calculate QC metrics and filter cells/genes.

    Parameters:
    -----------
    adata : AnnData
        Annotated data matrix
    mt_threshold : float
        Maximum percentage of mitochondrial genes (default: 5)
    min_genes : int
        Minimum number of genes per cell (default: 200)
    min_cells : int
        Minimum number of cells per gene (default: 3)

    Returns:
    --------
    AnnData
        Filtered annotated data matrix
    """
    # Identify mitochondrial genes (assumes gene names follow standard conventions)
    adata.var['mt'] = adata.var_names.str.startswith(('MT-', 'mt-', 'Mt-'))

    # Calculate QC metrics
    sc.pp.calculate_qc_metrics(adata, qc_vars=['mt'], percent_top=None,
                                log1p=False, inplace=True)

    print("\n=== QC Metrics Summary ===")
    print(f"Total cells: {adata.n_obs}")
    print(f"Total genes: {adata.n_vars}")
    print(f"Mean genes per cell: {adata.obs['n_genes_by_counts'].mean():.2f}")
    print(f"Mean counts per cell: {adata.obs['total_counts'].mean():.2f}")
    print(f"Mean mitochondrial %: {adata.obs['pct_counts_mt'].mean():.2f}")

    return adata


def generate_qc_plots(adata, output_prefix='qc'):
    """
    Generate comprehensive QC plots.

    Parameters:
    -----------
    adata : AnnData
        Annotated data matrix
    output_prefix : str
        Prefix for saved figure files
    """
    # Create figure directory if it doesn't exist
    import os
    os.makedirs('figures', exist_ok=True)

    # Violin plots for QC metrics
    sc.pl.violin(adata, ['n_genes_by_counts', 'total_counts', 'pct_counts_mt'],
                 jitter=0.4, multi_panel=True, save=f'_{output_prefix}_violin.pdf')

    # Scatter plots
    sc.pl.scatter(adata, x='total_counts', y='pct_counts_mt',
                  save=f'_{output_prefix}_mt_scatter.pdf')
    sc.pl.scatter(adata, x='total_counts', y='n_genes_by_counts',
                  save=f'_{output_prefix}_genes_scatter.pdf')

    # Highest expressing genes
    sc.pl.highest_expr_genes(adata, n_top=20,
                              save=f'_{output_prefix}_highest_expr.pdf')

    print(f"\nQC plots saved to figures/ directory with prefix '{output_prefix}'")


def filter_data_quantile(adata, gene_q=(0.05, 0.99), count_q=(0.05, 0.99),
                         mt_q=0.99, mt_ceiling=20.0, min_cells=3):
    """
    Filter cells and genes using quantile-based thresholds derived from the
    dataset's own QC-metric distributions. This adapts to each dataset
    instead of penalizing shallow libraries with a hardcoded 200-gene floor
    or letting high-MT tissues through with a 5% MT cap.

    Parameters
    ----------
    adata : AnnData
        Annotated data matrix.
    gene_q : (float, float)
        Lower / upper percentile on `n_genes_by_counts`. Default (0.05, 0.99)
        keeps the 95% interval below the 99th percentile (drops empty droplets
        and likely doublets).
    count_q : (float, float)
        Lower / upper percentile on `total_counts`. Default (0.05, 0.99).
    mt_q : float
        Upper percentile on `pct_counts_mt`. Default 0.99.
    mt_ceiling : float
        Absolute MT% ceiling that's enforced even if the quantile is higher.
        Prevents a uniformly bad dataset from passing high-MT cells through.
    min_cells : int
        Minimum cells per gene (constant noise floor, not dataset-quantile-
        dependent).

    Returns
    -------
    AnnData
        Filtered annotated data matrix.
    """
    n_cells_before, n_genes_before = adata.n_obs, adata.n_vars

    gene_lo = np.quantile(adata.obs['n_genes_by_counts'], gene_q[0])
    gene_hi = np.quantile(adata.obs['n_genes_by_counts'], gene_q[1])
    count_lo = np.quantile(adata.obs['total_counts'], count_q[0])
    count_hi = np.quantile(adata.obs['total_counts'], count_q[1])
    mt_hi = min(np.quantile(adata.obs['pct_counts_mt'], mt_q), mt_ceiling)

    print(f"\n=== Quantile QC thresholds (derived from this dataset) ===")
    print(f"  n_genes_by_counts ∈ [{gene_lo:.0f}, {gene_hi:.0f}]  ({gene_q[0]:.0%} – {gene_q[1]:.0%})")
    print(f"  total_counts      ∈ [{count_lo:.0f}, {count_hi:.0f}]  ({count_q[0]:.0%} – {count_q[1]:.0%})")
    print(f"  pct_counts_mt     < {mt_hi:.2f}  (min of {mt_q:.0%} percentile and {mt_ceiling}% ceiling)")

    adata = adata[(adata.obs['n_genes_by_counts'] >= gene_lo) &
                  (adata.obs['n_genes_by_counts'] <= gene_hi) &
                  (adata.obs['total_counts'] >= count_lo) &
                  (adata.obs['total_counts'] <= count_hi) &
                  (adata.obs['pct_counts_mt'] < mt_hi), :].copy()
    sc.pp.filter_genes(adata, min_cells=min_cells)

    print(f"\n=== Filtering Results ===")
    print(f"Cells: {n_cells_before} -> {adata.n_obs} ({adata.n_obs/n_cells_before*100:.1f}% retained)")
    print(f"Genes: {n_genes_before} -> {adata.n_vars} ({adata.n_vars/n_genes_before*100:.1f}% retained)")
    return adata


def filter_data(adata, mt_threshold=5, min_genes=200, max_genes=None,
                min_counts=None, max_counts=None, min_cells=3):
    """
    Hard-threshold cell/gene filtering (legacy mode). Use `filter_data_quantile`
    for the per-dataset adaptive default. Kept for cross-dataset reproducibility
    workflows where every sample must be filtered against the same numbers.

    Parameters
    ----------
    adata : AnnData
        Annotated data matrix.
    mt_threshold : float
        Maximum percentage of mitochondrial genes.
    min_genes : int
        Minimum number of genes per cell.
    max_genes : int, optional
        Maximum number of genes per cell.
    min_counts : int, optional
        Minimum number of counts per cell.
    max_counts : int, optional
        Maximum number of counts per cell.
    min_cells : int
        Minimum number of cells per gene.

    Returns
    -------
    AnnData
        Filtered annotated data matrix.
    """
    n_cells_before = adata.n_obs
    n_genes_before = adata.n_vars

    # Filter cells
    sc.pp.filter_cells(adata, min_genes=min_genes)
    if max_genes:
        adata = adata[adata.obs['n_genes_by_counts'] < max_genes, :]
    if min_counts:
        adata = adata[adata.obs['total_counts'] >= min_counts, :]
    if max_counts:
        adata = adata[adata.obs['total_counts'] < max_counts, :]

    # Filter by mitochondrial percentage
    adata = adata[adata.obs['pct_counts_mt'] < mt_threshold, :]

    # Filter genes
    sc.pp.filter_genes(adata, min_cells=min_cells)

    print(f"\n=== Filtering Results (hard thresholds) ===")
    print(f"Cells: {n_cells_before} -> {adata.n_obs} ({adata.n_obs/n_cells_before*100:.1f}% retained)")
    print(f"Genes: {n_genes_before} -> {adata.n_vars} ({adata.n_vars/n_genes_before*100:.1f}% retained)")

    return adata


def main():
    parser = argparse.ArgumentParser(description='QC analysis for single-cell data')
    parser.add_argument('input', help='Input file (h5ad, 10X mtx, csv, etc.)')
    parser.add_argument('--output', default='qc_filtered.h5ad',
                        help='Output file name (default: qc_filtered.h5ad)')
    parser.add_argument('--hard-thresholds', action='store_true',
                        help='Use fixed hard thresholds instead of dataset-derived quantiles. '
                             'Useful for cross-dataset reproducibility.')
    parser.add_argument('--mt-threshold', type=float, default=5,
                        help='Max mitochondrial percentage (hard-thresholds mode only, default: 5)')
    parser.add_argument('--min-genes', type=int, default=200,
                        help='Min genes per cell (hard-thresholds mode only, default: 200)')
    parser.add_argument('--gene-q-lo', type=float, default=0.05,
                        help='Lower quantile on n_genes_by_counts (default: 0.05)')
    parser.add_argument('--gene-q-hi', type=float, default=0.99,
                        help='Upper quantile on n_genes_by_counts (default: 0.99)')
    parser.add_argument('--count-q-lo', type=float, default=0.05,
                        help='Lower quantile on total_counts (default: 0.05)')
    parser.add_argument('--count-q-hi', type=float, default=0.99,
                        help='Upper quantile on total_counts (default: 0.99)')
    parser.add_argument('--mt-q-hi', type=float, default=0.99,
                        help='Upper quantile on pct_counts_mt (default: 0.99)')
    parser.add_argument('--mt-ceiling', type=float, default=20.0,
                        help='Absolute MT%% ceiling enforced regardless of quantile (default: 20)')
    parser.add_argument('--min-cells', type=int, default=3,
                        help='Min cells per gene (default: 3)')
    parser.add_argument('--skip-plots', action='store_true',
                        help='Skip generating QC plots')

    args = parser.parse_args()

    # Configure scanpy
    sc.settings.verbosity = 2
    sc.settings.set_figure_params(dpi=300, facecolor='white')
    sc.settings.figdir = './figures/'

    print(f"Loading data from: {args.input}")

    # Load data based on file extension
    if args.input.endswith('.h5ad'):
        adata = sc.read_h5ad(args.input)
    elif args.input.endswith('.h5'):
        adata = sc.read_10x_h5(args.input)
    elif args.input.endswith('.csv'):
        adata = sc.read_csv(args.input)
    else:
        # Try reading as 10X mtx directory
        adata = sc.read_10x_mtx(args.input)

    print(f"Loaded data: {adata.n_obs} cells x {adata.n_vars} genes")

    # Calculate QC metrics
    adata = calculate_qc_metrics(adata, mt_threshold=args.mt_threshold,
                                  min_genes=args.min_genes, min_cells=args.min_cells)

    # Generate QC plots (before filtering)
    if not args.skip_plots:
        print("\nGenerating QC plots (before filtering)...")
        generate_qc_plots(adata, output_prefix='qc_before')

    # Filter data — quantile-based by default, hard thresholds on opt-in
    if args.hard_thresholds:
        adata = filter_data(adata, mt_threshold=args.mt_threshold,
                            min_genes=args.min_genes, min_cells=args.min_cells)
    else:
        adata = filter_data_quantile(
            adata,
            gene_q=(args.gene_q_lo, args.gene_q_hi),
            count_q=(args.count_q_lo, args.count_q_hi),
            mt_q=args.mt_q_hi,
            mt_ceiling=args.mt_ceiling,
            min_cells=args.min_cells,
        )

    # Generate QC plots (after filtering)
    if not args.skip_plots:
        print("\nGenerating QC plots (after filtering)...")
        generate_qc_plots(adata, output_prefix='qc_after')

    # Save filtered data
    print(f"\nSaving filtered data to: {args.output}")
    adata.write_h5ad(args.output)

    print("\n=== QC Analysis Complete ===")


if __name__ == "__main__":
    main()
