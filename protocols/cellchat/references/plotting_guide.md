# CellChat Visualization Cookbook

CellChat has 25+ plotting functions. This guide groups them by what question they answer.

## Setup

```r
library(CellChat)
library(patchwork)
library(ComplexHeatmap)
library(circlize)
```

## Question 1: "Where does pathway X signal between cell types?"

### Aggregated network — circle / chord / heatmap

```r
pathways.show <- "CXCL"

# Circle — most flexible, scales well to ~20 cell groups
netVisual_aggregate(cellchat, signaling = pathways.show, layout = "circle")

# Chord — visually compact, good for ≤ 15 groups
netVisual_aggregate(cellchat, signaling = pathways.show, layout = "chord")

# Heatmap — cleanest for figures, doesn't need igraph layout
netVisual_heatmap(cellchat, signaling = pathways.show, color.heatmap = "Reds")
```

### Hierarchy plot — when you want directional bias

Shows "which cells receive from which" on left, "everyone else" on right. Best when you want to highlight a specific receiving population.

```r
vertex.receiver <- seq(1, 4)   # indices of receivers in @idents
netVisual_aggregate(cellchat, signaling = pathways.show, vertex.receiver = vertex.receiver)
```

### Grouped chord — when you want to compress cell-type axes

```r
group.cellType <- c(rep("FIB", 4), rep("DC", 4), rep("TC", 4))
names(group.cellType) <- levels(cellchat@idents)
netVisual_chord_cell(cellchat, signaling = pathways.show, group = group.cellType,
                     title.name = paste0(pathways.show, " signaling"))
```

## Question 2: "Which L-R pairs drive pathway X?"

```r
# Per-pathway contribution bar chart
netAnalysis_contribution(cellchat, signaling = pathways.show)

# Pull the ranked L-R list
pairLR.show <- extractEnrichedLR(cellchat, signaling = pathways.show,
                                  geneLR.return = FALSE)

# Visualize the top L-R
LR.show <- pairLR.show[1, ]
netVisual_individual(cellchat, signaling = pathways.show, pairLR.use = LR.show, layout = "circle")
netVisual_individual(cellchat, signaling = pathways.show, pairLR.use = LR.show, layout = "chord")

# Gene-level expression of the pathway
plotGeneExpression(cellchat, signaling = pathways.show, enriched.only = TRUE, type = "violin")
plotGeneExpression(cellchat, signaling = pathways.show, enriched.only = FALSE, type = "dot")
```

## Question 3: "What signals do cell type X send / receive?"

### Single-source / single-target bubble

```r
# All significant L-R from group 4 to groups 5-11
netVisual_bubble(cellchat, sources.use = 4, targets.use = 5:11,
                 remove.isolate = FALSE)

# Same, restricted to specific pathways
netVisual_bubble(cellchat, sources.use = 4, targets.use = 5:11,
                 signaling = c("CCL", "CXCL"))

# Custom L-R pair list
pairLR.use <- extractEnrichedLR(cellchat, signaling = c("CCL", "CXCL", "FGF"))
netVisual_bubble(cellchat, pairLR.use = pairLR.use,
                 sources.use = c(3, 4), targets.use = 5:8,
                 remove.isolate = TRUE)
```

### Chord at the L-R level

```r
# All L-R from one source to many targets (good when bubble is too sparse)
netVisual_chord_gene(cellchat, sources.use = 4, targets.use = 5:11,
                     lab.cex = 0.5, legend.pos.y = 30)

# Same restricted to specific pathways
netVisual_chord_gene(cellchat, sources.use = c(1, 2, 3, 4), targets.use = 5:11,
                     signaling = c("CCL", "CXCL"), legend.pos.x = 8)

# At the pathway level (slot = "netP" → aggregated across L-R)
netVisual_chord_gene(cellchat, sources.use = c(1, 2, 3, 4), targets.use = 5:11,
                     slot.name = "netP", legend.pos.x = 10)
```

## Question 4: "Which cell type is the dominant sender / receiver?"

### Signaling role — per pathway

```r
# Heatmap: per-cell-group dominant senders / receivers / mediators / influencers
netAnalysis_signalingRole_network(cellchat, signaling = "CXCL",
                                   width = 8, height = 2.5, font.size = 10)
```

### Signaling role — global

```r
# 2D scatter — total outgoing vs incoming signaling per cell group
gg1 <- netAnalysis_signalingRole_scatter(cellchat)

# Restrict to specific pathways
gg2 <- netAnalysis_signalingRole_scatter(cellchat, signaling = c("CXCL", "CCL"))
gg1 + gg2

# Cell-type × pathway heatmaps
ht1 <- netAnalysis_signalingRole_heatmap(cellchat, pattern = "outgoing")
ht2 <- netAnalysis_signalingRole_heatmap(cellchat, pattern = "incoming")
ht1 + ht2

# Restrict to specific pathways
netAnalysis_signalingRole_heatmap(cellchat, signaling = c("CXCL", "CCL"))
```

## Question 5: "Are there modules of co-signaling cells?"

NMF-based pattern discovery. The river / sankey plot is the standard output.

```r
library(NMF); library(ggalluvial)

# Pick k from the elbow of the selectK curves
selectK(cellchat, pattern = "outgoing")
cellchat <- identifyCommunicationPatterns(cellchat, pattern = "outgoing", k = 4)

# River — flow from cells → patterns → pathways
netAnalysis_river(cellchat, pattern = "outgoing")

# Dot — same info, smaller figure
netAnalysis_dot(cellchat, pattern = "outgoing")

# Repeat for incoming
selectK(cellchat, pattern = "incoming")
cellchat <- identifyCommunicationPatterns(cellchat, pattern = "incoming", k = 3)
netAnalysis_river(cellchat, pattern = "incoming")
```

## Question 6: "Which pathways are conserved vs context-specific between conditions?"

(Requires a merged comparison object — see [multi_dataset_comparison.md](multi_dataset_comparison.md).)

```r
# Manifold of pathways across conditions
cellchat <- computeNetSimilarityPairwise(cellchat, type = "functional")
cellchat <- netEmbedding(cellchat,            type = "functional")
cellchat <- netClustering(cellchat,           type = "functional")
netVisual_embeddingPairwise(cellchat, type = "functional", label.size = 3.5)

# Same, structural
cellchat <- computeNetSimilarityPairwise(cellchat, type = "structural")
cellchat <- netEmbedding(cellchat,            type = "structural")
cellchat <- netClustering(cellchat,           type = "structural")
netVisual_embeddingPairwise(cellchat, type = "structural", label.size = 3.5)

# Rank pathways by how different they are between conditions
rankSimilarity(cellchat, type = "functional")
```

## Question 7: "What changed between conditions?"

```r
# Overall: count + weight bar plots
gg1 <- compareInteractions(cellchat, show.legend = FALSE, group = c(1, 2))
gg2 <- compareInteractions(cellchat, show.legend = FALSE, group = c(1, 2), measure = "weight")
gg1 + gg2

# Differential cell-cell network (red = up in condition 2)
netVisual_diffInteraction(cellchat, weight.scale = TRUE)
netVisual_diffInteraction(cellchat, weight.scale = TRUE, measure = "weight")

# Pathways ranked by total information flow, condition-coloured
rankNet(cellchat, mode = "comparison", measure = "weight", stacked = TRUE,  do.stat = TRUE)
rankNet(cellchat, mode = "comparison", measure = "weight", stacked = FALSE, do.stat = TRUE)

# Bubble of L-R pairs in both conditions
netVisual_bubble(cellchat, sources.use = 4, targets.use = 5:11,
                 comparison = c(1, 2), angle.x = 45)

# Same, restricted to "more in condition 2"
netVisual_bubble(cellchat, sources.use = 4, targets.use = 5:11,
                 comparison = c(1, 2), max.dataset = 2,
                 title.name = "Increased signaling in LS",
                 remove.isolate = TRUE, angle.x = 45)
```

## Composite "story" figure

A common publication figure: pathway-level rank, differential network, top-pathway visualization, and dysregulated L-R bubble.

```r
# 4-panel figure
library(patchwork)

p1 <- rankNet(cellchat, mode = "comparison", measure = "weight",
              stacked = TRUE, do.stat = TRUE) +
        ggtitle("All pathways by information flow")

# Save the diff-interaction network as a saved image
pdf("figures/diff_interaction.pdf", width = 5, height = 5)
netVisual_diffInteraction(cellchat, weight.scale = TRUE, measure = "weight")
dev.off()

p3 <- netVisual_heatmap(cellchat, measure = "weight")   # comparison heatmap

p4 <- netVisual_bubble(cellchat,
        pairLR.use = pairLR.use.up,
        sources.use = 4, targets.use = 5:11,
        comparison = c(1, 2), angle.x = 90,
        remove.isolate = TRUE,
        title.name = "Up-regulated L-R in disease"
      )

(p1 | p3) / (p4)
ggsave("figures/comparison_overview.pdf", width = 14, height = 10)
```

## Saving plots properly

CellChat mixes ggplot, ComplexHeatmap, and base-R graphics — three different save conventions:

```r
# ggplot — use ggsave
p <- netAnalysis_signalingRole_scatter(cellchat)
ggsave("figures/role_scatter.pdf", plot = p, width = 6, height = 6)

# ComplexHeatmap — use pdf() / dev.off()
pdf("figures/role_heatmap.pdf", width = 10, height = 6)
draw(netAnalysis_signalingRole_heatmap(cellchat, pattern = "outgoing"))
dev.off()

# Base-R graphics (circle / chord / hierarchy) — use pdf() / dev.off()
pdf("figures/cxcl_circle.pdf", width = 6, height = 6)
netVisual_aggregate(cellchat, signaling = "CXCL", layout = "circle")
dev.off()
```

## Palette choices

CellChat respects standard R palette conventions:

```r
# Default palette per cell group — change globally
options(ggsci.palette = "lancet")     # OK if you've installed ggsci

# Per-call colour override
netVisual_heatmap(cellchat, signaling = "CXCL",
                   color.heatmap = "Reds")   # any colorRampPalette name

# Custom group colours
groupColors <- setNames(RColorBrewer::brewer.pal(length(levels(cellchat@idents)), "Set2"),
                         levels(cellchat@idents))
netVisual_aggregate(cellchat, signaling = "CXCL", layout = "circle",
                     color.use = groupColors)
```
