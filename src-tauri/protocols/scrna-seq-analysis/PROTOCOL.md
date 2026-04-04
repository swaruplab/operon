# Single-Cell RNA-seq Analysis Protocol

Standard workflow for processing and analyzing single-cell RNA sequencing data using Scanpy/Seurat.

## Environment
- Preferred: conda env with scanpy, anndata, scvi-tools
- Alternative: R with Seurat, SingleCellExperiment
- Required Python packages: scanpy, anndata, matplotlib, numpy, pandas, leidenalg

## Workflow Steps

1. **Data Loading**: Load 10X Genomics output (h5, mtx, or h5ad) with `scanpy.read_10x_h5()` or `scanpy.read_h5ad()`
2. **QC Filtering**: Filter cells (min_genes=200, max_genes=5000), filter genes (min_cells=3), compute mito % (`sc.pp.calculate_qc_metrics`), filter mito > 20%
3. **Doublet Detection**: Run scrublet or scDblFinder before normalization
4. **Normalization**: `sc.pp.normalize_total(target_sum=1e4)` then `sc.pp.log1p()`
5. **HVG Selection**: `sc.pp.highly_variable_genes(n_top_genes=2000, flavor='seurat_v3')`
6. **Dimensionality Reduction**: PCA with `sc.tl.pca(n_comps=50)`
7. **Batch Correction** (if multi-sample): Harmony (`sc.external.pp.harmony_integrate`) or scVI
8. **Neighborhood Graph**: `sc.pp.neighbors(n_neighbors=15, n_pcs=30)`
9. **Clustering**: `sc.tl.leiden(resolution=0.5)` — try multiple resolutions
10. **Visualization**: `sc.tl.umap()` then `sc.pl.umap(color=['leiden', 'sample', 'n_genes'])`
11. **Marker Genes**: `sc.tl.rank_genes_groups(groupby='leiden', method='wilcoxon')`
12. **Cell Type Annotation**: Based on known markers or automated with CellTypist/scType

## Conventions
- Save all plots as PNG to a `figures/` subdirectory
- Use `figsize=(8, 6)` for single plots, `figsize=(12, 8)` for multi-panel
- Always set `random_state=42` for reproducibility
- Save processed h5ad after each major step: `adata.write('data_qc.h5ad')`
- Use SLURM array jobs for processing multiple samples in parallel

## Common Pitfalls
- Always copy `adata.raw = adata.copy()` before subsetting to HVGs
- Check batch effects BEFORE clustering, not after
- Normalize BEFORE log-transform, never the reverse
- Resolution 0.5-1.0 is typical for Leiden; higher = more clusters
