# Clustering Parameter Guide

## Leiden Resolution
- 0.1-0.3: Very coarse (major cell lineages)
- 0.5: Standard starting point
- 0.8-1.0: Fine-grained (subtypes visible)
- 1.5+: Very fine (may over-cluster)

## Choosing Resolution
1. Run multiple resolutions: [0.2, 0.4, 0.6, 0.8, 1.0, 1.5]
2. Plot clustree to visualize stability
3. Check known marker genes across resolutions
4. Pick resolution where known cell types separate cleanly

## Number of PCs
- Default: 30 PCs
- Use elbow plot: `sc.pl.pca_variance_ratio(adata, n_pcs=50)`
- For complex tissues: 40-50 PCs
- For simple (e.g., PBMC): 15-20 PCs

## Neighbors
- n_neighbors=15 is standard
- Increase to 30 for smoother UMAP (loses rare populations)
- Decrease to 10 for more local structure
