# Squidpy Spatial Analysis Reference

The scanpy preprocessing pipeline (HVG, PCA, neighbors, leiden) doesn't know anything about the physical layout of cells. Squidpy fills that gap. This reference covers the analyses that **require spatial coordinates**, not the ones shared with regular scRNA-seq.

## The spatial neighborhood graph (foundation)

Almost every squidpy function depends on a precomputed spatial graph stored in `adata.obsp['spatial_connectivities']` and `adata.obsp['spatial_distances']`.

```python
import squidpy as sq

# Cell-based platforms (Xenium, CosMx, MERFISH, Slide-seq):
sq.gr.spatial_neighbors(adata, coord_type='generic', n_neighs=6)

# Visium (hexagonal lattice):
sq.gr.spatial_neighbors(adata, coord_type='grid', n_neighs=6, n_rings=1)

# Radius-based instead of k-NN — useful for comparing across resolutions:
sq.gr.spatial_neighbors(adata, coord_type='generic', radius=100.0)
```

**Choosing parameters:**
- `n_neighs=6` is a reasonable default for cell-based platforms; bumping to 10–20 captures broader context.
- `coord_type='grid'` is required for Visium — the function builds a regular hex lattice instead of a Delaunay graph.
- For radius-based graphs, pick a radius that matches biological scale (e.g. ~50 µm for direct neighbors, ~200 µm for paracrine signaling).

## Neighborhood enrichment

"Does cluster A live next to cluster B more often than chance?" A permutation test on the spatial graph.

```python
sq.gr.nhood_enrichment(adata, cluster_key='leiden', n_perms=1000, n_jobs=4)
sq.pl.nhood_enrichment(adata, cluster_key='leiden', method='single',
                        figsize=(8, 8))
```

**Interpretation:**
- Z-scores in `adata.uns['leiden_nhood_enrichment']['zscore']`. Positive = more co-localized than chance; negative = avoided.
- The heatmap is the standard output. For complex tissue, also cluster the heatmap rows/columns (`method='complete'` or run a linkage on the z-score matrix).
- Diagonal (cluster A next to cluster A) is usually highly positive — that's just same-type tissue regions.

## Co-occurrence

Similar to neighborhood enrichment but **at multiple radii**. Asks: as I expand the search radius around a cluster-A cell, how does the conditional probability of finding cluster B change?

```python
sq.gr.co_occurrence(adata, cluster_key='leiden',
                     interval=np.linspace(0, 500, 50))  # adjust max radius
sq.pl.co_occurrence(adata, cluster_key='leiden', clusters='0')  # one ref cluster
```

**When to use over `nhood_enrichment`:**
- When the biology is multi-scale (immune cells form rings around tumor → distinct signature at radius ~50 µm vs. ~200 µm).
- When you want to identify the *characteristic distance* of an interaction.

## Spatially variable genes (Moran's I / Geary's C)

Which genes have expression patterns that **depend on position**? Moran's I = 1 → strong spatial structure; 0 → random; negative → checkerboard.

```python
sq.gr.spatial_autocorr(adata, mode='moran', n_perms=1000, n_jobs=4)
# Result lives in adata.uns['moranI']

# Top 20 spatially variable genes
adata.uns['moranI'].head(20)

# Plot the top 4
top_genes = adata.uns['moranI'].index[:4].tolist()
sq.pl.spatial_scatter(adata, color=top_genes, shape=None, ncols=2)
```

**Notes:**
- Moran's I is fast but assumes you care about *local* structure. For *long-range* spatial gradients, Geary's C (`mode='geary'`) is sometimes preferred.
- Higher `n_perms` = better p-values but slower. 1000 is usually enough for screening.
- Run on raw counts AND log-normalized — they sometimes disagree, and you'll want to know why.

## Interaction matrix

Number of contacts between cluster pairs. Lower-level than nhood_enrichment (no statistical test, just counts).

```python
sq.gr.interaction_matrix(adata, cluster_key='leiden', normalized=True)
sq.pl.interaction_matrix(adata, cluster_key='leiden')
```

Useful when you want raw co-localization counts, not a significance test.

## Centrality scores

Per-cluster graph-theoretic scores: degree, closeness, average clustering. Reveals which cell types act as "hubs" in tissue.

```python
sq.gr.centrality_scores(adata, cluster_key='leiden')
sq.pl.centrality_scores(adata, cluster_key='leiden')
```

Stromal cells and macrophages often score as high-degree hubs — they touch lots of different cell types.

## Ripley's statistics

Point-pattern statistics — do cells of a cluster cluster together, spread out, or distribute randomly across scales?

```python
sq.gr.ripley(adata, cluster_key='leiden', mode='L', max_dist=500)
sq.pl.ripley(adata, cluster_key='leiden', mode='L')
```

`mode='L'` is the variance-stabilized form of Ripley's K. A monotonically increasing curve = self-clustering; flat = random; decreasing past some radius = repulsion.

## Image features (Visium + Xenium morphology images)

Squidpy can extract texture / intensity features from the underlying H&E or morphology image and align them to spots/cells.

```python
img = sq.im.ImageContainer('path/to/tissue_hires.tiff')
sq.im.calculate_image_features(
    adata, img,
    library_id='sample_1',
    features=['summary', 'texture', 'histogram'],
    key_added='image_features',
)
# Now adata.obsm['image_features'] aligns image-derived features with transcriptomics
```

Useful for: classifying spots into tissue compartments (tumor / stroma / lymphocyte) using H&E features, or finding genes correlated with morphology.

## Differential expression by spatial cluster

Standard scanpy DE on spatial clusters:

```python
sc.tl.rank_genes_groups(adata, 'leiden', method='wilcoxon')
sc.pl.rank_genes_groups(adata, n_genes=20)
```

Add **spatial context** by restricting comparisons to spatially adjacent clusters (the comparison that makes biological sense — a tumor cluster vs. its neighboring stroma, not vs. some distant stromal cluster):

```python
# Pull the neighborhood enrichment z-scores
nhood = pd.DataFrame(adata.uns['leiden_nhood_enrichment']['zscore'])

# For each cluster, find its top-3 spatial neighbors and do pairwise DE
for cluster in adata.obs['leiden'].unique():
    top_neighbors = nhood[cluster].sort_values(ascending=False)[1:4].index.tolist()
    for nb in top_neighbors:
        sc.tl.rank_genes_groups(adata, 'leiden', groups=[cluster], reference=nb,
                                key_added=f'de_{cluster}_vs_{nb}', method='wilcoxon')
```

## Multi-sample integration

Squidpy itself doesn't integrate samples — that's a scanpy / scvi-tools job. Standard recipe:

1. **Per-sample**: Load, QC, normalize, build spatial graph (do NOT integrate yet — graphs are per-slide).
2. **Across samples**: Concatenate AnnData (`ad.concat`), run integration on the transcriptomic embedding (Harmony, scVI), then re-cluster using the integrated embedding.
3. **Per-sample again**: Apply the integrated cluster labels back into each sample's AnnData and re-run spatial analyses per-sample (because the spatial graph is per-slide).

```python
import anndata as ad
adata_all = ad.concat([adata_s1, adata_s2, adata_s3], join='outer', label='sample',
                       keys=['s1', 's2', 's3'])

# Standard integration (Harmony shown — works on the PCA embedding)
sc.external.pp.harmony_integrate(adata_all, key='sample')
sc.pp.neighbors(adata_all, use_rep='X_pca_harmony')
sc.tl.leiden(adata_all, resolution=0.5)

# Back per-sample for spatial analyses:
for sample_id, sub in adata_all.obs.groupby('sample'):
    sub_adata = adata_all[sub.index].copy()
    sq.gr.spatial_neighbors(sub_adata, coord_type='generic', n_neighs=6)
    sq.gr.nhood_enrichment(sub_adata, cluster_key='leiden')
    # ...
```

**Don't try to merge spatial coords across samples** — different slides have different coordinate origins. The right unit of analysis for spatial statistics is one slide.

## Common pitfalls

- **Forgetting to re-run `sq.gr.spatial_neighbors` after subsetting.** Subsetting `adata` doesn't update `obsp['spatial_connectivities']` — the matrix still has the original dimensions. Always recompute after `adata[mask, :]`.
- **Using `coord_type='generic'` on Visium.** It works (Delaunay graph on hex spots) but is wrong — use `coord_type='grid'`.
- **High n_perms on huge datasets.** For 100k+ cells, `n_perms=100` is usually fine for screening; bump to 1000 only on the final results.
- **Treating Slide-seq beads as cells.** They're transcript pileups; spatial statistics on beads can be misleading without cell-typing.
