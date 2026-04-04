# Spatial Transcriptomics Analysis Protocol

Workflow for analyzing 10X Visium and other spatial transcriptomics data.

## Environment
- Python: scanpy, squidpy, anndata, matplotlib, seaborn
- R: Seurat (v5+), STUtility, SpatialExperiment
- Required: tissue H&E image aligned with spots

## Workflow Steps

1. **Data Loading**: `scanpy.read_visium(path)` or `Seurat::Load10X_Spatial()`
2. **QC**: Filter spots by total counts, genes detected, mito %. Visualize QC on tissue image.
3. **Normalization**: Same as scRNA-seq — `normalize_total` + `log1p`
4. **HVG + PCA**: Standard dimensionality reduction
5. **Clustering**: Leiden on PCA space — overlay clusters on tissue image
6. **Spatial Neighbors**: `squidpy.gr.spatial_neighbors()` — uses physical coordinates
7. **Spatially Variable Genes**: `squidpy.gr.spatial_autocorr(mode='moran')` for Moran's I
8. **Deconvolution** (optional): cell2location, RCTD, or SPOTlight for cell type proportions per spot
9. **Niche Analysis**: `squidpy.gr.nhood_enrichment()` for cell type co-localization
10. **Ligand-Receptor**: `squidpy.gr.ligrec()` for spatially-resolved cell communication

## Visualization
- Always overlay results on the tissue H&E image
- Use `scanpy.pl.spatial(adata, color='gene_name', img_key='hires')` for gene expression
- Side-by-side: H&E | Clusters | Gene expression
- Use consistent color palettes across related plots

## Conventions
- Preserve spatial coordinates — never shuffle spot order
- Save figures at 300 DPI with tissue image backdrop
- Report number of spots, median genes/spot, and tissue coverage
- For multi-sample: process individually, then integrate with Harmony or scVI
