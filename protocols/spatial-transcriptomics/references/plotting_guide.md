# Spatial Visualization Guide

Spatial data has one big advantage over standard scRNA-seq: you can plot it on the tissue. Use that. This guide is organized by figure type, with platform-specific notes inline.

## Cluster map — "color-by-cluster on the tissue"

The single most useful plot in spatial transcriptomics. Always make it first.

```python
import squidpy as sq

# Cell-based platforms (Xenium, CosMx, MERFISH, Slide-seq):
sq.pl.spatial_scatter(adata, color='leiden', shape=None, size=10)

# Visium — use scanpy's spatial plot (overlays H&E by default):
sc.pl.spatial(adata, color='leiden', img_key='hires', alpha=0.8, size=1.4)
```

**Tips:**
- For cell-based: `shape=None` plots points. `shape='hex'` or `shape='square'` for spot-style platforms.
- `size`: tweak for visibility. Tiny tissue + thousands of cells → reduce size to ~1.
- Use the same color palette across samples for consistency (set `sc.settings.set_figure_params(palette='tab20')`).

## Gene expression in space

```python
# Single gene
sq.pl.spatial_scatter(adata, color='CD3D', shape=None,
                       cmap='viridis', vmax='p99')

# Multiple genes — auto-arranges into panels
sq.pl.spatial_scatter(adata, color=['CD3D', 'EPCAM', 'COL1A1'],
                       shape=None, ncols=3, cmap='viridis')

# Visium with H&E underneath
sc.pl.spatial(adata, color=['CD3D', 'EPCAM'], img_key='hires',
              alpha=0.7, cmap='viridis', vmax='p99')
```

**Tips:**
- `vmax='p99'` clips the top 1% — single outlier cells otherwise wash out the heatmap.
- For sparse genes, switch to `vmin=0, vmax=2` (raw log-normalized) so 0-expression cells stay neutral grey.
- `groups=['cluster_3']` (on `color='leiden'`) plots only one cluster in color; everything else in light grey — great for highlighting a specific population.

## H&E overlay (Visium-specific)

```python
sc.pl.spatial(adata, color=None, img_key='hires')   # H&E alone
sc.pl.spatial(adata, color='leiden', img_key='hires', alpha=0.7)  # H&E + clusters
sc.pl.spatial(adata, color='CD3D', img_key='lowres', alpha=0.6)  # faster for previews
```

**Crop to a region:**
```python
sc.pl.spatial(adata, color='leiden', img_key='hires',
              crop_coord=(x_min, x_max, y_min, y_max), alpha=0.7)
```

`crop_coord` is in pixel coordinates of the chosen image. Most useful for zooming into a tumor edge or a specific anatomical structure.

## Morphology overlay (Xenium-specific)

```python
sq.pl.spatial_scatter(adata, color='leiden',
                       img=True,        # show morphology image
                       img_alpha=0.5,
                       size=8)
```

For full control of the underlying image:
```python
import tifffile
morph = tifffile.imread('path/to/morphology_focus.ome.tif')
# Compose your own matplotlib figure with morph as background
```

## Neighborhood enrichment heatmap

```python
sq.pl.nhood_enrichment(adata, cluster_key='leiden', method='single',
                        figsize=(8, 8), annotate=True)
```

`method='single' | 'complete' | 'average'` chooses linkage for clustering the heatmap. Try a couple — sometimes the ordering with `complete` reveals modules that `single` doesn't.

## Co-occurrence curves

```python
sq.pl.co_occurrence(adata, cluster_key='leiden', clusters=['0', '5', '12'])
```

Plots conditional probability vs. radius for each cluster. Read the curves left-to-right:
- Steep climb early → tight co-localization (touching neighbors)
- Plateau at distance → loose co-localization
- Bump then decay → ring-like spatial pattern (e.g. tumor-infiltrating lymphocytes)

## Multi-panel figure templates

A common publication figure: 6 panels showing different aspects of the same slide.

```python
fig, axs = plt.subplots(2, 3, figsize=(18, 12))

# Top row: H&E, cluster map, marker gene
sc.pl.spatial(adata, ax=axs[0,0], img_key='hires', color=None, show=False)
sq.pl.spatial_scatter(adata, ax=axs[0,1], color='leiden', shape=None, size=4, show=False)
sq.pl.spatial_scatter(adata, ax=axs[0,2], color='CD3D', shape=None,
                       cmap='viridis', vmax='p99', size=4, show=False)

# Bottom row: enrichment, co-occurrence, top spatial gene
sq.pl.nhood_enrichment(adata, cluster_key='leiden', ax=axs[1,0], show=False)
sq.pl.co_occurrence(adata, cluster_key='leiden', ax=axs[1,1], clusters='0', show=False)
sq.pl.spatial_scatter(adata, ax=axs[1,2], color=adata.uns['moranI'].index[0],
                       shape=None, cmap='magma', vmax='p99', size=4, show=False)

plt.tight_layout()
plt.savefig('figures/overview.pdf', dpi=300, bbox_inches='tight')
```

## Niche / domain visualization

If you've computed niches (BANKSY, CellCharter, or the naive majority-vote proxy from SKILL.md):

```python
sq.pl.spatial_scatter(adata, color='niche', shape=None, size=4)

# Side-by-side cell type and niche
fig, axs = plt.subplots(1, 2, figsize=(14, 6))
sq.pl.spatial_scatter(adata, color='leiden', ax=axs[0], shape=None, size=4, show=False)
sq.pl.spatial_scatter(adata, color='niche', ax=axs[1], shape=None, size=4, show=False)
axs[0].set_title('Cell type (leiden)')
axs[1].set_title('Niche')
plt.tight_layout()
```

## Choosing palettes

For categorical (clusters, cell types):
- `tab10` / `tab20` — distinct hues, good for ≤20 categories.
- `husl` (via seaborn) — perceptually uniform, scales well to 30+ categories.

For continuous (gene expression, scores):
- `viridis` — perceptually uniform default. Good for non-zero-anchored data.
- `magma` — better contrast for sparse data.
- `RdBu_r` — diverging, use for z-scores or fold changes.

Set globally:
```python
sc.settings.set_figure_params(scanpy=True, dpi=150, dpi_save=300,
                               facecolor='white', vector_friendly=True)
```

## Saving for publication

Always use `dpi=300` for raster figures. Prefer vector formats (PDF, SVG) for line art:

```python
sq.pl.spatial_scatter(adata, color='leiden', shape=None, size=4,
                       save='_clusters.pdf', dpi=300)
```

PNG vs. PDF rule of thumb:
- **Cluster maps / H&E overlays** → PNG (rasterized large images are smaller as PNG than PDF).
- **Heatmaps / co-occurrence curves / bar plots** → PDF (vector preserves crispness at any zoom).

## Performance notes

- Plotting 200k+ cells with `sq.pl.spatial_scatter` is slow. For previews use a random subsample: `sq.pl.spatial_scatter(adata[::5], ...)` (every 5th cell).
- For very large datasets, downsample to a 4k-pixel grid using `datashader`:
  ```python
  import datashader as ds, datashader.transfer_functions as tf
  canvas = ds.Canvas(plot_width=2000, plot_height=2000)
  agg = canvas.points(adata.obs.assign(x=adata.obsm['spatial'][:,0],
                                        y=adata.obsm['spatial'][:,1]),
                       'x', 'y', ds.count_cat('leiden'))
  img = tf.shade(agg, how='log')
  ```
  This handles millions of points at interactive speeds.
- Hi-res H&E images can be 100+ MB. Use `img_key='lowres'` for development; switch to `'hires'` only for final figures.
