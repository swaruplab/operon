# Multi-Dataset Comparison — Deep Dive

Two scenarios are covered here:
- **Same cell types in both** — straightforward merge, run Pipeline 2 below.
- **Different cell types between datasets** — use `liftCellChat` first (last section), then merge.

The same comparison functions apply to either case after merging.

## Prerequisite: identical preprocessing per condition

CellChat's comparison validity depends on every per-condition object being built with the same parameters. Run the single-dataset pipeline twice with matched arguments:

```r
preprocess <- function(seurat_subset, condition_name) {
  data.input <- GetAssayData(seurat_subset, layer = "data", assay = "RNA")
  meta       <- seurat_subset@meta.data
  cellchat   <- createCellChat(object = data.input, meta = meta, group.by = "cell_type")
  cellchat@DB <- subsetDB(CellChatDB.human, search = "Secreted Signaling", key = "annotation")
  cellchat <- subsetData(cellchat)
  cellchat <- identifyOverExpressedGenes(cellchat)
  cellchat <- identifyOverExpressedInteractions(cellchat)
  cellchat <- computeCommunProb(cellchat, type = "triMean")
  cellchat <- filterCommunication(cellchat, min.cells = 10)
  cellchat <- computeCommunProbPathway(cellchat)
  cellchat <- aggregateNet(cellchat)
  cellchat <- netAnalysis_computeCentrality(cellchat, slot.name = "netP")
  saveRDS(cellchat, paste0("results/cellchat_", condition_name, ".rds"))
  cellchat
}

cellchat.NL <- preprocess(subset(seurat_obj, condition == "NL"), "NL")
cellchat.LS <- preprocess(subset(seurat_obj, condition == "LS"), "LS")
```

If you skip this and use mismatched DB filters / `min.cells` / etc., any "differential signaling" you find may just reflect the parameter mismatch.

## Merging

```r
object.list <- list(NL = cellchat.NL, LS = cellchat.LS)
cellchat    <- mergeCellChat(object.list,
                              add.names = names(object.list),
                              cell.prefix = FALSE)
```

`add.names` tags each object's signaling networks; the resulting `cellchat` object has a `datasets` factor accessible via `cellchat@meta$datasets` and used by every comparison function.

`cell.prefix = TRUE` prepends the condition name to each cell barcode — needed when cells in different conditions might share barcodes (e.g. running multiple Seurat objects through the same loop).

## 1. Overall interaction comparison

```r
gg1 <- compareInteractions(cellchat, show.legend = FALSE, group = c(1, 2),
                            measure = "count")
gg2 <- compareInteractions(cellchat, show.legend = FALSE, group = c(1, 2),
                            measure = "weight")
gg1 + gg2
```

- `count` — number of significant L-R pairs
- `weight` — summed communication probability

Differences here are global. Big delta = the perturbed condition fundamentally rewires signaling; small delta = pathway-specific changes.

## 2. Differential network — cell-cell level

```r
# Difference of weights / counts. Red = up in 2nd condition, blue = down.
par(mfrow = c(1, 2), xpd = TRUE)
netVisual_diffInteraction(cellchat, weight.scale = TRUE, measure = "count")
netVisual_diffInteraction(cellchat, weight.scale = TRUE, measure = "weight")

# Heatmap variant
gg1 <- netVisual_heatmap(cellchat)
gg2 <- netVisual_heatmap(cellchat, measure = "weight")
gg1 + gg2
```

Heatmap is best for figures; diff-interaction circle plot is best for "at-a-glance" insights about which cell pairs changed most.

## 3. Pathway-level changes — `rankNet`

The single most useful plot in the comparison pipeline. Ranks every pathway by total information flow, coloured by condition.

```r
# Stacked bar — pathways sorted by total flow across both conditions
gg1 <- rankNet(cellchat, mode = "comparison", measure = "weight",
               stacked = TRUE, do.stat = TRUE)

# Side-by-side — easier to spot the "all in condition A" or "all in condition B" pathways
gg2 <- rankNet(cellchat, mode = "comparison", measure = "weight",
               stacked = FALSE, do.stat = TRUE)
gg1 + gg2
```

`do.stat = TRUE` runs a paired permutation test per pathway. Asterisks on the bars mark significant differences.

Pathways at the top of the side-by-side plot but with one bar much taller than the other → strong candidates for the dysregulation analysis below.

## 4. Dysregulated L-R pairs — the actionable bit

`rankNet` tells you which pathways changed. To get down to the **specific ligand-receptor pairs that drove the change**, run the differential expression workflow CellChat layers on top of the merged object.

```r
# Compute per-gene DE across conditions
pos.dataset   <- "LS"                          # the perturbed / case condition
features.name <- paste0(pos.dataset, ".merged")

cellchat <- identifyOverExpressedGenes(
  cellchat,
  group.dataset = "datasets",                  # the factor mergeCellChat added
  pos.dataset   = pos.dataset,
  features.name = features.name,
  only.pos      = FALSE,
  thresh.pc     = 0.1,                         # min fraction expressing
  thresh.fc     = 0.05,                        # min log-fold-change
  thresh.p      = 0.05,
  group.DE.combined = FALSE
)

# Map per-gene DE back onto the network — each L-R pair now has logFC annotations
net <- netMappingDEG(cellchat, features.name = features.name, variable.all = TRUE)

# Up-regulated L-R in the perturbed condition
net.up <- subsetCommunication(cellchat, net = net,
                               datasets       = "LS",
                               ligand.logFC   = 0.05,
                               receptor.logFC = NULL)     # set NULL to ignore receptor DE

# Down-regulated L-R (up in the reference)
net.down <- subsetCommunication(cellchat, net = net,
                                 datasets       = "NL",
                                 ligand.logFC   = -0.05,
                                 receptor.logFC = NULL)

cat("Up:  ", nrow(net.up), "L-R pairs\n")
cat("Down:", nrow(net.down), "L-R pairs\n")
```

### Bubble plots of dysregulated pairs

```r
# Up-regulated
pairLR.use.up <- net.up[, "interaction_name", drop = FALSE]
gg1 <- netVisual_bubble(cellchat,
  pairLR.use     = pairLR.use.up,
  sources.use    = 4, targets.use = 5:11,
  comparison     = c(1, 2),
  angle.x        = 90,
  remove.isolate = TRUE,
  title.name     = "Up-regulated signaling in LS"
)

# Down-regulated
pairLR.use.down <- net.down[, "interaction_name", drop = FALSE]
gg2 <- netVisual_bubble(cellchat,
  pairLR.use     = pairLR.use.down,
  sources.use    = 4, targets.use = 5:11,
  comparison     = c(1, 2),
  angle.x        = 90,
  remove.isolate = TRUE,
  title.name     = "Down-regulated signaling in LS"
)
gg1 + gg2
```

### Enrichment of dysregulated L-R pairs

```r
computeEnrichmentScore(net.up,   species = "human", variable.both = TRUE)
computeEnrichmentScore(net.down, species = "human", variable.both = TRUE)
```

This produces a ranked GO / pathway enrichment of the dysregulated ligand/receptor genes. Useful for translating "100 L-R pairs are up" into biology.

## 5. Pathway-level manifold

When two conditions have many shared pathways but different topology, manifold learning over pathways shows clusters of conserved (close together across conditions) vs. context-specific (far apart) signaling.

```r
# Functional similarity — share senders/receivers?
cellchat <- computeNetSimilarityPairwise(cellchat, type = "functional")
cellchat <- netEmbedding(cellchat,            type = "functional")
cellchat <- netClustering(cellchat,           type = "functional")
netVisual_embeddingPairwise(cellchat, type = "functional", label.size = 3.5)

# Structural similarity — same wiring topology?
cellchat <- computeNetSimilarityPairwise(cellchat, type = "structural")
cellchat <- netEmbedding(cellchat,            type = "structural")
cellchat <- netClustering(cellchat,           type = "structural")
netVisual_embeddingPairwise(cellchat, type = "structural", label.size = 3.5)

# Rank pathways by manifold distance between conditions
rankSimilarity(cellchat, type = "functional")
```

`rankSimilarity` ranks pathways by how different they are between conditions. The top of the list = your context-specific signaling.

## 6. Single-pathway visualization across conditions

For a pathway of interest from `rankNet`:

```r
pathways.show <- c("CXCL")

# Side-by-side circle plots
par(mfrow = c(1, 2), xpd = TRUE)
for (i in seq_along(object.list)) {
  netVisual_aggregate(object.list[[i]], signaling = pathways.show, layout = "circle",
                      signaling.name = paste(pathways.show, names(object.list)[i]))
}

# Side-by-side heatmaps
ht <- list()
for (i in seq_along(object.list)) {
  ht[[i]] <- netVisual_heatmap(object.list[[i]], signaling = pathways.show,
                                color.heatmap = "Reds",
                                title.name = paste(pathways.show, "in", names(object.list)[i]))
}
ComplexHeatmap::draw(ht[[1]] + ht[[2]], ht_gap = unit(0.5, "cm"))

# Gene expression of the pathway across conditions
plotGeneExpression(cellchat, signaling = pathways.show, split.by = "datasets",
                    colors.ggplot = TRUE, type = "violin")
```

## Pipeline 3 — Lifting for different cellular compositions

When two datasets contain different cell types, the per-condition network matrices have incompatible dimensions and `mergeCellChat` either fails or silently misaligns. `liftCellChat` solves this by padding each object to a common cell-type set.

```r
# Build per-condition objects first (Pipeline 1, separately)
cellchat.E13 <- readRDS("results/cellchat_E13.rds")
cellchat.E14 <- readRDS("results/cellchat_E14.rds")

# Define the union cell-type list — typically the larger / reference dataset's
group.new <- levels(cellchat.E14@idents)

# Lift the smaller / different object to that universe
cellchat.E13 <- liftCellChat(cellchat.E13, group.new)
```

`liftCellChat` modifies only `@net`, `@netP`, `@idents` — the cells / expression data are unchanged, just the network slots. Missing cell types become rows/columns of zeros in the communication matrices, so they appear as nodes in all plots but never as active senders/receivers.

```r
# Now merge as in Pipeline 2
object.list <- list(E13 = cellchat.E13, E14 = cellchat.E14)
cellchat    <- mergeCellChat(object.list,
                              add.names    = names(object.list),
                              cell.prefix  = TRUE)

# Every comparison function in this document now works
compareInteractions(cellchat, group = c(1, 2))
netVisual_diffInteraction(cellchat, weight.scale = TRUE)
rankNet(cellchat, mode = "comparison", stacked = TRUE, do.stat = TRUE)
```

### When NOT to lift

If a cell type is genuinely missing in one condition (control has no tumor cells; tumor sample has no naive immune cells), lifting **hides this biological difference** by treating the missing cells as a population that just doesn't communicate.

In that case, either:
- Subset both datasets to shared cell types and skip lifting, OR
- Acknowledge the asymmetry explicitly in your figures.

Lifting is for **technical** composition differences (one cohort fluorescence-sorted, the other not; different sampling depths revealing rare populations) where the missing cells are presumed to also exist in the "missing" condition.

## Comparison checklist

```r
# Per-condition (twice, with identical parameters)
cellchat.A <- preprocess(seurat_A, "A")
cellchat.B <- preprocess(seurat_B, "B")

# Merge (with optional lift)
if (cell_types_differ) {
  cellchat.A <- liftCellChat(cellchat.A, levels(cellchat.B@idents))
}
object.list <- list(A = cellchat.A, B = cellchat.B)
cellchat    <- mergeCellChat(object.list, add.names = names(object.list))

# Standard battery
compareInteractions(cellchat, group = c(1, 2))
netVisual_diffInteraction(cellchat, weight.scale = TRUE)
rankNet(cellchat, mode = "comparison", stacked = TRUE, do.stat = TRUE)

# Dysregulated L-R analysis
cellchat <- identifyOverExpressedGenes(cellchat, group.dataset = "datasets",
                                        pos.dataset = "B",
                                        features.name = "B.merged",
                                        only.pos = FALSE, thresh.fc = 0.05)
net <- netMappingDEG(cellchat, features.name = "B.merged", variable.all = TRUE)
net.up   <- subsetCommunication(cellchat, net = net, datasets = "B", ligand.logFC = 0.05)
net.down <- subsetCommunication(cellchat, net = net, datasets = "A", ligand.logFC = -0.05)

# Manifold (only if many pathways)
cellchat <- computeNetSimilarityPairwise(cellchat, type = "functional")
cellchat <- netEmbedding(cellchat, type = "functional")
cellchat <- netClustering(cellchat, type = "functional")
netVisual_embeddingPairwise(cellchat, type = "functional")
```
