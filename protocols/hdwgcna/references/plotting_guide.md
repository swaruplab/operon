# hdWGCNA Visualization Cookbook

Recipes for the most common figures, organized by analysis stage. All examples assume a fully-processed Seurat object with `ConstructNetwork` + `ModuleEigengenes` + `ModuleConnectivity` + `ResetModuleNames` already run.

## Setup

```r
library(Seurat)
library(hdWGCNA)
library(tidyverse)
library(cowplot)
library(patchwork)

theme_set(theme_cowplot())
```

## 1. Network construction diagnostics

### Soft-power selection plot

```r
plot_list <- PlotSoftPowers(seurat_obj)
wrap_plots(plot_list, ncol = 2)
ggsave('figures/soft_power_selection.pdf', width = 12, height = 8)
```

### Dendrogram with module colors

```r
PlotDendrogram(
  seurat_obj,
  main = 'INH hdWGCNA Dendrogram'
)
```

This is the single most useful diagnostic — inspect after every parameter change.

### kME distribution per module

```r
p <- PlotKMEs(seurat_obj, ncol = 5)
print(p)
ggsave('figures/kme_distributions.pdf', width = 14, height = 8)
```

Modules with bimodal kME distributions are often "two modules glued together" — bump `mergeCutHeight` lower or `deepSplit` higher to split them.

---

## 2. Module activity in cells

### Module Feature Plot — UMAP × module

The default high-information overview: one UMAP per module, coloured by hME.

```r
plot_list <- ModuleFeaturePlot(
  seurat_obj,
  features = 'hMEs',          # 'MEs' for raw, 'scores' for UCell hub signatures
  order    = TRUE,             # plot high-ME cells on top
  reduction = 'umap'
)
wrap_plots(plot_list, ncol = 6)
ggsave('figures/module_feature_umap.pdf', width = 18, height = 12)
```

### Dot plot — modules across cell groups

For comparing all modules across discrete groups (cell types, clusters, conditions). The single most informative figure for understanding "which module is where."

```r
hMEs <- GetMEs(seurat_obj, harmonized = TRUE)
modules <- GetModules(seurat_obj)
mods <- levels(modules$module)
mods <- mods[mods != 'grey']

seurat_obj@meta.data <- cbind(seurat_obj@meta.data, hMEs)

DotPlot(seurat_obj, features = mods, group.by = 'cell_type') +
  RotatedAxis() +
  scale_color_gradient2(high = 'red', mid = 'grey95', low = 'blue') +
  ggtitle('Module activity by cell type')
ggsave('figures/module_dotplot_celltype.pdf', width = 10, height = 6)
```

Stratify by condition:
```r
DotPlot(seurat_obj, features = mods, group.by = 'cell_type', split.by = 'condition') +
  RotatedAxis() + scale_color_gradient2(high = 'red', mid = 'grey95', low = 'blue')
```

### Violin plot — single module across groups

```r
target_module <- 'INH-M3'
VlnPlot(seurat_obj, features = target_module, group.by = 'cell_type',
        pt.size = 0, split.by = 'condition') +
  ggtitle(paste0(target_module, ' activity'))
```

### Module radar plot — module activity across sub-groups

Good for "how does this cell type's module activity differ between sub-clusters?"

```r
ModuleRadarPlot(
  seurat_obj,
  group.by  = 'cluster_label',
  barcodes  = seurat_obj@meta.data %>% filter(cell_type == 'INH') %>% rownames(),
  axis.label.size = 4,
  grid.label.size = 4
) + ggtitle('INH modules across subclusters')
```

---

## 3. Hub gene networks

### Per-module network (igraph layout)

```r
ModuleNetworkPlot(
  seurat_obj,
  n_hubs   = 10,
  outdir   = 'figures/module_networks/'
)
# Writes one PDF per module to outdir/
```

### Multi-module network

Shows how hub genes from different modules connect to each other.

```r
HubGeneNetworkPlot(
  seurat_obj,
  n_hubs    = 5,              # hubs per module
  n_other   = 10,             # extra non-hub genes per module
  edge_prop = 0.075,          # density (lower = sparser)
  mods      = c('INH-M1', 'INH-M3', 'INH-M5')   # or 'all'
)
```

### Module–module correlogram

How similar are modules to each other (based on hME / ME / hub-gene-score similarity)?

```r
ModuleCorrelogram(
  seurat_obj,
  features  = 'hMEs',           # or 'MEs', 'scores'
  cor.method = 'pearson',
  exclude_grey = TRUE
)
```

Highly correlated module pairs are candidates for merging — they may represent a single biology that `ConstructNetwork` over-split.

---

## 4. Differential MEs

### Lollipop (single comparison)

```r
PlotDMEsLollipop(
  seurat_obj, DMEs,
  group.by    = 'cell_type',
  comparison  = 'AD_vs_Ctrl',
  wgcna_name  = 'INH'
) + ggtitle('AD vs Control DME')
```

X marks indicate non-significant modules. Dot size = number of genes per module.

### Volcano

```r
PlotDMEsVolcano(
  seurat_obj, DMEs,
  plot_labels = TRUE,
  pval_cutoff = 0.05,
  log2fc_thresh = 0.1,
  wgcna_name  = 'INH'
) + ggtitle('AD vs Control DME volcano')
```

For figures: combine the volcano with a module dot plot showing the top 3 DME-significant modules across cell types.

---

## 5. Module–trait correlation heatmap

```r
PlotModuleTraitCorrelation(
  seurat_obj,
  label        = 'fdr',
  label_symbol = 'stars',
  text_size    = 2.5,
  high_color   = 'yellow',
  mid_color    = 'black',
  low_color    = 'purple',
  plot_max     = 0.2,
  combine      = TRUE,
  wgcna_name   = 'INH'
)
ggsave('figures/module_trait_heatmap.pdf', width = 8, height = 10)
```

For a custom palette (e.g. for journals that disallow black backgrounds):

```r
PlotModuleTraitCorrelation(
  seurat_obj,
  label = 'fdr', label_symbol = 'stars',
  high_color = '#D7191C',   # red
  mid_color  = 'white',
  low_color  = '#2C7BB6',   # blue
  combine = TRUE,
  wgcna_name = 'INH'
)
```

---

## 6. Enrichment — dot and bar plots

### EnrichrDotPlot — preferred for figures

```r
EnrichrDotPlot(
  seurat_obj,
  mods       = 'all',
  database   = 'GO_Biological_Process_2023',
  n_terms    = 2,                  # top 2 per module
  term_size  = 8,
  p_adj      = FALSE,
  wgcna_name = 'INH'
) + ggtitle('GO BP enrichment')
```

For multiple databases side-by-side, run twice and patchwork:

```r
p_go <- EnrichrDotPlot(seurat_obj, database = 'GO_Biological_Process_2023', n_terms = 2)
p_kegg <- EnrichrDotPlot(seurat_obj, database = 'KEGG_2021_Human',           n_terms = 2)
p_go | p_kegg
```

### EnrichrBarPlot — for supplementary figures

```r
EnrichrBarPlot(
  seurat_obj,
  outdir     = 'figures/enrichr/',
  n_terms    = 10,
  plot_size  = c(5, 7),
  logscale   = TRUE,
  wgcna_name = 'INH'
)
# Writes figures/enrichr/INH-M1.pdf, INH-M2.pdf, ...
```

---

## 7. Composite "module summary" figure

A common one-stop figure for a paper: four panels capturing one module's full story.

```r
target_mod <- 'INH-M3'

# (a) UMAP coloured by hME
p1 <- ModuleFeaturePlot(seurat_obj, features = 'hMEs', module_names = target_mod,
                        order = TRUE)[[1]] + ggtitle(paste0(target_mod, ' hME'))

# (b) Hub gene table → render as a small text grob
hub_df <- GetHubGenes(seurat_obj, n_hubs = 15) %>% filter(module == target_mod)
p2 <- ggplot(hub_df, aes(x = reorder(gene_name, kME), y = kME)) +
  geom_col(fill = '#1f77b4') +
  coord_flip() + xlab(NULL) +
  ggtitle(paste0(target_mod, ' top hubs'))

# (c) Module across conditions (violin)
p3 <- VlnPlot(seurat_obj, features = target_mod, group.by = 'condition',
              pt.size = 0) + NoLegend() + ggtitle('By condition')

# (d) Top GO terms (extract from EnrichrTable)
enrich_df <- GetEnrichrTable(seurat_obj) %>%
  filter(module == target_mod, db == 'GO_Biological_Process_2023') %>%
  arrange(P.value) %>% head(8)
p4 <- ggplot(enrich_df,
             aes(x = reorder(Term, -log10(P.value)), y = -log10(P.value))) +
  geom_col(fill = '#d62728') + coord_flip() +
  xlab(NULL) + ggtitle('Top GO BP')

(p1 | p2) / (p3 | p4)
ggsave(paste0('figures/', target_mod, '_summary.pdf'),
       width = 12, height = 9)
```

---

## Palette choices

| Use case | Recommended palette |
|---|---|
| Categorical clusters / modules (≤ 20) | `RColorBrewer::brewer.pal(n, 'Paired')` or `ggsci::pal_d3` |
| Categorical (> 20) | `Polychrome::createPalette(n, c('#ff0000', '#00ff00', '#0000ff'))` |
| Diverging (correlation, fold change) | `colorRampPalette(c('#2C7BB6', 'white', '#D7191C'))` or `RdBu_r` |
| Sequential (kME, expression) | `viridis::magma` or `viridis::viridis` |
| Module–trait heatmap (hdWGCNA default) | `low='purple', mid='black', high='yellow'` — high-contrast on screen |

## Saving for publication

PDF for vector (line plots, heatmaps); PNG with `dpi=300` for raster (UMAPs with many points).

```r
ggsave('figures/figure_1.pdf',  plot = p, width = 10, height = 6, useDingbats = FALSE)
ggsave('figures/figure_1.png',  plot = p, width = 10, height = 6, dpi = 300)
```

`useDingbats = FALSE` is needed for PDFs going into Illustrator — otherwise small symbols render as proprietary glyphs.
