# Seurat Visualization Cookbook

Beyond the 7 core plotting calls in the main SKILL — this is the "publication-figure" guide for Seurat. Patterns for composing multi-panel figures, custom palettes, raster vs vector output, and ggplot2 customization.

## The 7 core plots — quick reference

| Function | What it shows | Common knobs |
|---|---|---|
| `DimPlot` | Categorical labels on UMAP/PCA/tSNE | `group.by`, `split.by`, `label`, `cols` |
| `FeaturePlot` | Continuous expression on UMAP | `features`, `min.cutoff`, `max.cutoff`, `split.by`, `blend` |
| `VlnPlot` | Per-cluster expression distribution | `features`, `group.by`, `split.by`, `pt.size` |
| `RidgePlot` | Same as VlnPlot but ridge-style | `features`, `ncol` |
| `DotPlot` | Many markers × many clusters | `features`, `group.by`, `cluster.idents`, `scale` |
| `DoHeatmap` | Single-cell heatmap | `features`, `cells`, `size`, `angle`, `slot` |
| `FeatureScatter` | Two features as scatter | `feature1`, `feature2`, `group.by` |

## Combining plots — patchwork

Seurat returns ggplot2 objects; combine with [patchwork](https://patchwork.data-imaginist.com/) operators:

```r
library(patchwork)

p1 <- DimPlot(pbmc, reduction = "umap", group.by = "cell_type")
p2 <- DimPlot(pbmc, reduction = "umap", group.by = "condition")
p3 <- FeaturePlot(pbmc, features = "CD3D")
p4 <- FeaturePlot(pbmc, features = "MS4A1")

# Side by side
p1 | p2

# Grid layout
(p1 | p2) / (p3 | p4)

# Specify exact layout
p1 + p2 + p3 + p4 + plot_layout(ncol = 2)

# Annotate panels
(p1 | p2) / (p3 | p4) +
  plot_annotation(title = "Figure 1", tag_levels = "A")

# Apply theme/legend to all panels at once
((p1 | p2) / (p3 | p4)) & NoLegend()
```

## Customising colours

### Cluster / cell-type palettes

```r
# Default uses ggplot2's hue scale. Override with explicit colours:
my_palette <- c("Naive CD4 T" = "#1f77b4", "CD14+ Mono" = "#ff7f0e",
                "Memory CD4 T" = "#2ca02c", "B" = "#d62728",
                "CD8 T" = "#9467bd", "FCGR3A+ Mono" = "#8c564b",
                "NK" = "#e377c2", "DC" = "#7f7f7f", "Platelet" = "#bcbd22")

DimPlot(pbmc, cols = my_palette)
DotPlot (pbmc, features = markers) + scale_color_gradientn(colours = c("blue", "white", "red"))
```

For categorical scales with many levels, use a Polychrome / pals palette:

```r
library(Polychrome)
my_pal <- createPalette(N = 20, seedcolors = c("#ff0000", "#00ff00", "#0000ff"))
names(my_pal) <- levels(pbmc$cell_type)
DimPlot(pbmc, cols = my_pal)
```

### Continuous expression palettes

```r
# Default: blue → red. Customize:
FeaturePlot(pbmc, features = "CD3D") +
  scale_color_gradientn(colours = c("lightgrey", "blue"))

# Diverging for log-fold-change:
FeaturePlot(pbmc, features = "logFC_geneX") +
  scale_color_gradient2(low = "#2C7BB6", mid = "white", high = "#D7191C",
                         midpoint = 0)

# Viridis (perceptually uniform — recommended for grayscale-friendly figures):
library(viridis)
FeaturePlot(pbmc, features = "CD3D") + scale_color_viridis(option = "magma")
```

## Clipping outliers

A few extreme cells often dominate FeaturePlot colour scales. Clip via quantile:

```r
FeaturePlot(pbmc, features = c("CD3D", "MS4A1"),
             min.cutoff = "q10",       # bottom 10% → minimum colour
             max.cutoff = "q90")       # top 10% → maximum colour
```

Or by explicit numeric value:

```r
FeaturePlot(pbmc, features = "n_genes", min.cutoff = 500, max.cutoff = 5000)
```

## Split by condition / sample

```r
# Each condition gets its own panel
DimPlot(pbmc, reduction = "umap", split.by = "condition", ncol = 2)
FeaturePlot(pbmc, features = "CD3D", split.by = "condition")

# Per-cluster expression × condition
VlnPlot(pbmc, features = "CD3D", split.by = "condition", group.by = "seurat_clusters")
```

## Labelling

```r
# Label clusters on UMAP
DimPlot(pbmc, reduction = "umap", label = TRUE, label.size = 5, repel = TRUE)

# Label specific cells
top_cells <- TopCells(object = pbmc[["pca"]], dim = 1, ncells = 10)
LabelPoints(plot = DimPlot(pbmc, reduction = "pca"), points = top_cells, repel = TRUE)

# Add ID column to a manually-labelled plot
p <- DimPlot(pbmc, label = FALSE)
LabelClusters(plot = p, id = "ident", repel = TRUE)
```

## Common publication figure templates

### 4-panel "story" figure

```r
library(patchwork)

p1 <- DimPlot(pbmc, label = TRUE) + NoLegend() + ggtitle("Cell types")
p2 <- DimPlot(pbmc, group.by = "condition") + ggtitle("Condition")
p3 <- FeaturePlot(pbmc, features = "CD3D", min.cutoff = "q10", max.cutoff = "q90") +
        ggtitle("CD3D (T cells)")
p4 <- DotPlot(pbmc, features = c("CD3D", "MS4A1", "CD14", "NKG7", "FCER1A")) +
        RotatedAxis()

((p1 | p2) / (p3 | p4)) +
  plot_annotation(title = "PBMC overview", tag_levels = "A")

ggsave("figures/pbmc_overview.pdf", width = 12, height = 10, useDingbats = FALSE)
```

### Marker heatmap with cell-type bar

```r
top_markers <- markers %>%
  group_by(cluster) %>%
  slice_max(n = 10, order_by = avg_log2FC) %>%
  pull(gene)

DoHeatmap(
  subset(pbmc, downsample = 100),         # cap cells per cluster for speed
  features  = top_markers,
  group.by  = "cell_type",
  size      = 3,
  angle     = 90,
  raster    = TRUE                         # rasterize cells (smaller PDF)
) + NoLegend()
ggsave("figures/marker_heatmap.pdf", width = 14, height = 12)
```

### Comparison violin (condition split)

```r
genes_of_interest <- c("IFNG", "IL10", "TGFB1", "IL6")
VlnPlot(
  pbmc,
  features = genes_of_interest,
  group.by = "cell_type",
  split.by = "condition",
  pt.size  = 0,                          # remove dots for clean publication look
  ncol     = 2,
  cols     = c("control" = "#1f77b4", "disease" = "#d62728")
)
```

## Performance — large datasets

```r
# Raster scatter points (UMAPs / DimPlots become small images, not vectors)
DimPlot(pbmc_500k, raster = TRUE, raster.dpi = c(1024, 1024))

# Downsample before heatmap (DoHeatmap on 500k cells is unusable)
DoHeatmap(subset(pbmc, downsample = 200), features = markers)

# For FeaturePlot on huge datasets, also raster the points:
FeaturePlot(pbmc_500k, features = "CD3D", raster = TRUE,
             raster.dpi = c(1024, 1024))
```

## Saving for publication

```r
# Vector formats for line plots / heatmaps (small files, perfect zoom)
ggsave("figures/fig1.pdf", plot = p, width = 12, height = 9, useDingbats = FALSE)
ggsave("figures/fig1.svg", plot = p, width = 12, height = 9)

# PNG for raster-heavy plots (large UMAPs with many cells)
ggsave("figures/umap.png", plot = p, width = 8, height = 8, dpi = 300)

# When DoHeatmap is huge, save as PNG to avoid 100+ MB PDFs
ggsave("figures/heatmap.png", plot = p, width = 14, height = 12, dpi = 300)
```

`useDingbats = FALSE` is needed for PDFs intended for Adobe Illustrator (otherwise small symbols render as proprietary glyphs).

## Themes + axis cosmetics

```r
# Larger font for posters
p + theme(text = element_text(size = 18))

# Clean classic look
p + theme_classic()

# Remove all theme baggage (clean for layouts)
p + theme_void()

# Specific Seurat-provided themes
p + DarkTheme()
p + FontSize(x.title = 20, y.title = 20, x.text = 14, y.text = 14)
p + NoAxes()
p + NoLegend()

# Combinations
p + theme_classic() + NoLegend() + theme(plot.title = element_text(hjust = 0.5))
```

## Interactive exploration

```r
# Hover to inspect cells
HoverLocator(DimPlot(pbmc))

# Manually lasso-select cells
selected <- CellSelector(plot = DimPlot(pbmc), object = pbmc, ident = "selected")
```

`CellSelector` adds a new identity to the object based on your lasso. Useful for picking out a sub-population for downstream sub-analysis without writing a programmatic filter.
