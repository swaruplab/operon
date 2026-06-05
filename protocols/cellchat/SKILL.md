---
name: cellchat
description: Cell-cell communication analysis with CellChat (R). Covers (1) the standard single-dataset pipeline — database setup, communication inference, pathway-level aggregation, centrality, communication-pattern discovery, similarity-based clustering; (2) multi-dataset comparison — differential interactions, altered pathways, dysregulated ligand-receptor pairs; (3) the lifting workflow for comparing datasets with different cellular compositions. Works on Seurat objects, SingleCellExperiment, or counts+metadata.
license: GPL-3.0-or-later
metadata:
---

# CellChat: Cell-Cell Communication Inference

## Overview

[CellChat](https://github.com/jinworks/CellChat) infers, visualizes, and compares intercellular communication networks from single-cell and spatial transcriptomics. Each communication event is a **ligand–receptor (L-R) interaction** between two cell groups; CellChat aggregates L-R pairs into **signaling pathways** and computes per-pair / per-pathway communication probabilities. Two object-level slots carry the inferred network: `@net` (L-R level) and `@netP` (pathway level).

Built-in databases (CellChatDB) cover human and mouse with hundreds of curated L-R pairs organized by signaling category: **Secreted Signaling** (e.g. cytokines, chemokines), **ECM-Receptor**, and **Cell-Cell Contact**. You can subset to one category for a focused analysis.

## When to Use This Skill

- Inferring cell-cell signaling between annotated cell types from one scRNA-seq / snRNA-seq / spatial dataset
- Comparing communication between two conditions (disease vs control, treated vs untreated, two timepoints)
- Comparing communication between datasets with **different cell compositions** (e.g. embryonic E13 vs E14 with non-overlapping cell types — uses `liftCellChat`)
- Identifying dysregulated L-R pairs between conditions
- Discovering communication patterns (NMF-based outgoing / incoming pattern modules)
- Functional / structural similarity clustering of pathways

**Not for**: gene-level differential expression (use Seurat / DESeq2), gene-set enrichment of communication (use hdwgcna + Enrichr), trajectory analysis (use scVelo / Slingshot).

## Prerequisites

- R ≥ 4.1
- A Seurat object (v4 or v5) with annotated cell types **OR** a counts matrix + cell-level metadata data frame
- Recommend ≥ 50 cells per cell type for stable inference

```r
# Install CellChat (one-time)
install.packages("BiocManager")
BiocManager::install(c("ComplexHeatmap", "NMF", "circlize"))
devtools::install_github("jinworks/CellChat")
```

Optional Python dep for embedding plots: `pip install umap-learn`.

---

## Pipeline 1 — Single-Dataset Analysis

The standard pipeline takes a Seurat-like object with cell-type labels and returns an annotated `CellChat` object with inferred communication at L-R and pathway levels.

```r
library(CellChat)
library(patchwork)
options(stringsAsFactors = FALSE)
future::plan("multisession", workers = 4)

# ── 1. Build the CellChat object ────────────────────────────────────────────
data.input <- GetAssayData(seurat_obj, layer = "data", assay = "RNA")  # log-normalized counts
meta       <- seurat_obj@meta.data                                       # must contain cell-type column
cellchat   <- createCellChat(object = data.input, meta = meta, group.by = "cell_type")

# ── 2. Pick the database ────────────────────────────────────────────────────
CellChatDB <- CellChatDB.human    # or CellChatDB.mouse
showDatabaseCategory(CellChatDB)  # peek at the three categories

# Subset to one signaling category (or skip this line to use all)
CellChatDB.use <- subsetDB(CellChatDB, search = "Secreted Signaling", key = "annotation")
cellchat@DB    <- CellChatDB.use

# ── 3. Identify over-expressed L-R interactions ─────────────────────────────
cellchat <- subsetData(cellchat)                       # keep only genes in the DB
cellchat <- identifyOverExpressedGenes(cellchat)       # per-cell-group marker genes
cellchat <- identifyOverExpressedInteractions(cellchat) # L-R pairs where both sides are over-expressed

# ── 4. Infer the communication network ──────────────────────────────────────
cellchat <- computeCommunProb(cellchat, type = "triMean")  # robust to outliers; "truncatedMean" also OK
cellchat <- filterCommunication(cellchat, min.cells = 10)  # drop groups with < 10 cells
cellchat <- computeCommunProbPathway(cellchat)             # aggregate L-R → pathway
cellchat <- aggregateNet(cellchat)                          # cell-cell weight / count matrices

# ── 5. Centrality + signaling roles ─────────────────────────────────────────
cellchat <- netAnalysis_computeCentrality(cellchat, slot.name = "netP")

saveRDS(cellchat, "results/cellchat_single.rds")
```

The result object now contains:

| Slot | What |
|---|---|
| `cellchat@net$prob` | L-R-level communication probability (group × group × LRpair) |
| `cellchat@net$pval` | Permutation p-values |
| `cellchat@net$count` / `cellchat@net$weight` | Aggregated cell-cell matrices |
| `cellchat@netP$prob` | Pathway-level communication probability (group × group × pathway) |
| `cellchat@netP$centr` | Per-pathway centrality scores |
| `cellchat@idents` | Cell-group labels (used by all visualizations) |

### Extract communications

```r
df.net  <- subsetCommunication(cellchat)                                 # all L-R hits
df.path <- subsetCommunication(cellchat, slot.name = "netP")              # pathway-level
df.sub  <- subsetCommunication(cellchat, signaling = c("CXCL", "TGFb"))   # specific pathways
df.dir  <- subsetCommunication(cellchat, sources.use = c(1,2), targets.use = c(4,5))
```

### Visualize one pathway

```r
pathways.show <- c("CXCL")

# Aggregated cell-cell network for the pathway
netVisual_aggregate(cellchat, signaling = pathways.show, layout = "circle")
netVisual_aggregate(cellchat, signaling = pathways.show, layout = "chord")
netVisual_heatmap (cellchat, signaling = pathways.show, color.heatmap = "Reds")

# Hierarchy plot: which cell groups receive from which
vertex.receiver <- seq(1, 4)
netVisual_aggregate(cellchat, signaling = pathways.show, vertex.receiver = vertex.receiver)

# L-R pair contribution to the pathway
netAnalysis_contribution(cellchat, signaling = pathways.show)

# Individual L-R pair visualization
pairLR.CXCL <- extractEnrichedLR(cellchat, signaling = "CXCL", geneLR.return = FALSE)
LR.show <- pairLR.CXCL[1, ]
netVisual_individual(cellchat, signaling = pathways.show, pairLR.use = LR.show, layout = "circle")
```

### Centrality — which cell type drives each signal?

```r
# Per-pathway: dominant senders, receivers, mediators, influencers
netAnalysis_signalingRole_network(cellchat, signaling = "CXCL",
                                   width = 8, height = 2.5, font.size = 10)

# Global: 2D scatter of cells as senders × receivers
gg1 <- netAnalysis_signalingRole_scatter(cellchat)

# Outgoing / incoming pattern heatmaps (cell × pathway)
ht1 <- netAnalysis_signalingRole_heatmap(cellchat, pattern = "outgoing")
ht2 <- netAnalysis_signalingRole_heatmap(cellchat, pattern = "incoming")
ht1 + ht2
```

### Communication pattern discovery (NMF)

CellChat factorizes the network into "patterns" — modules of cell groups that share outgoing or incoming signaling profiles.

```r
library(NMF); library(ggalluvial)

selectK(cellchat, pattern = "outgoing")   # plot the rank-selection curve
cellchat <- identifyCommunicationPatterns(cellchat, pattern = "outgoing", k = 4)
netAnalysis_river(cellchat, pattern = "outgoing")
netAnalysis_dot  (cellchat, pattern = "outgoing")

selectK(cellchat, pattern = "incoming")
cellchat <- identifyCommunicationPatterns(cellchat, pattern = "incoming", k = 3)
netAnalysis_river(cellchat, pattern = "incoming")
```

Choose `k` from where the cophenetic / Frobenius / silhouette curves elbow.

### Pathway similarity & clustering

```r
# Functional similarity = "do the same cells send/receive?"
cellchat <- computeNetSimilarity(cellchat, type = "functional")
cellchat <- netEmbedding(cellchat, type = "functional")
cellchat <- netClustering(cellchat, type = "functional")
netVisual_embedding(cellchat, type = "functional", label.size = 3.5)

# Structural similarity = "is the wiring topology the same?"
cellchat <- computeNetSimilarity(cellchat, type = "structural")
cellchat <- netEmbedding(cellchat, type = "structural")
cellchat <- netClustering(cellchat, type = "structural")
netVisual_embedding(cellchat, type = "structural", label.size = 3.5)
```

Convenience: `Rscript scripts/build_cellchat.R --rds seurat.rds --group-by cell_type --species human --signaling 'Secreted Signaling' --out cellchat.rds`. See [references/single_dataset.md](references/single_dataset.md) for parameter tuning, slot-by-slot output, and DB customization.

Source: [CellChat-vignette.html](https://htmlpreview.github.io/?https://github.com/jinworks/CellChat/blob/master/tutorial/CellChat-vignette.html).

---

## Pipeline 2 — Multi-Dataset Comparison

Standard comparison: two conditions (NL vs LS, AD vs Ctrl, treated vs untreated), **same cell types in both**. Run Pipeline 1 once per condition, then merge.

```r
cellchat.NL <- readRDS("results/cellchat_NL.rds")
cellchat.LS <- readRDS("results/cellchat_LS.rds")

object.list <- list(NL = cellchat.NL, LS = cellchat.LS)
cellchat    <- mergeCellChat(object.list, add.names = names(object.list))
```

### 1. Overall comparison

```r
# Total interaction counts and weights between conditions
gg1 <- compareInteractions(cellchat, show.legend = FALSE, group = c(1, 2))
gg2 <- compareInteractions(cellchat, show.legend = FALSE, group = c(1, 2), measure = "weight")
gg1 + gg2
```

### 2. Differential cell-cell network

```r
# Red = increased in LS, blue = decreased
netVisual_diffInteraction(cellchat, weight.scale = TRUE)
netVisual_diffInteraction(cellchat, weight.scale = TRUE, measure = "weight")

# Heatmap version
gg1 <- netVisual_heatmap(cellchat)
gg2 <- netVisual_heatmap(cellchat, measure = "weight")
gg1 + gg2
```

### 3. Altered signaling pathways

```r
# Bar plot — pathways ranked by total information flow, condition-coloured
gg1 <- rankNet(cellchat, mode = "comparison", measure = "weight",
               stacked = TRUE, do.stat = TRUE)
gg2 <- rankNet(cellchat, mode = "comparison", measure = "weight",
               stacked = FALSE, do.stat = TRUE)
gg1 + gg2
```

### 4. Dysregulated L-R pairs

This is the hardest-to-find but most actionable analysis. For each L-R pair, test whether the underlying ligand / receptor genes are differentially expressed across conditions, then keep only the pairs with significant changes.

```r
# Compute per-gene DE between datasets (datasets metadata column was added by mergeCellChat)
pos.dataset    <- "LS"                                  # the "perturbed" condition
features.name  <- paste0(pos.dataset, ".merged")

cellchat <- identifyOverExpressedGenes(
  cellchat, group.dataset = "datasets",
  pos.dataset   = pos.dataset,
  features.name = features.name,
  only.pos      = FALSE,
  thresh.pc     = 0.1, thresh.fc = 0.05, thresh.p = 0.05,
  group.DE.combined = FALSE
)

# Map per-gene DE results onto the communication network
net <- netMappingDEG(cellchat, features.name = features.name, variable.all = TRUE)

# Up- and down-regulated L-R pairs (in the perturbed condition)
net.up   <- subsetCommunication(cellchat, net = net, datasets = "LS",
                                 ligand.logFC = 0.05, receptor.logFC = NULL)
net.down <- subsetCommunication(cellchat, net = net, datasets = "NL",
                                 ligand.logFC = -0.05, receptor.logFC = NULL)

# Visualize up-regulated pairs from cell-type 4 to 5-11
pairLR.use.up <- net.up[, "interaction_name", drop = FALSE]
netVisual_bubble(cellchat,
  pairLR.use  = pairLR.use.up,
  sources.use = 4, targets.use = 5:11,
  comparison  = c(1, 2),
  angle.x     = 90,
  remove.isolate = TRUE,
  title.name  = "Up-regulated signaling in LS"
)

# Functional enrichment of up- vs down-regulated L-R sets
computeEnrichmentScore(net.up,   species = "human", variable.both = TRUE)
computeEnrichmentScore(net.down, species = "human", variable.both = TRUE)
```

### 5. Pathway-level manifold (when conditions look very different)

```r
cellchat <- computeNetSimilarityPairwise(cellchat, type = "functional")
cellchat <- netEmbedding(cellchat, type = "functional")
cellchat <- netClustering(cellchat, type = "functional")
netVisual_embeddingPairwise(cellchat, type = "functional", label.size = 3.5)

# Rank pathways by how different they are between conditions
rankSimilarity(cellchat, type = "functional")
```

Conserved pathways cluster together across conditions; context-specific ones spread apart.

Convenience: `Rscript scripts/compare_cellchat.R --condition-a cellchat_NL.rds --condition-b cellchat_LS.rds --out merged.rds`.

Source: [Comparison_analysis_of_multiple_datasets.html](https://htmlpreview.github.io/?https://github.com/jinworks/CellChat/blob/master/tutorial/Comparison_analysis_of_multiple_datasets.html).

---

## Pipeline 3 — Comparison with Different Cellular Compositions

When two datasets have **different cell types** (e.g. E13 has neuroblasts that E14 doesn't, or one disease tissue has tumor cells absent from the control), you can't merge directly — the network matrices have different dimensions. Solution: **lift** each object to a shared cell-type universe.

```r
# Build per-dataset CellChat objects (Pipeline 1, separately)
cellchat.E13 <- readRDS("results/cellchat_E13.rds")
cellchat.E14 <- readRDS("results/cellchat_E14.rds")

# Define the union cell-type list (typically the larger / more comprehensive set)
group.new <- levels(cellchat.E14@idents)   # use E14's cell types as the reference

# Lift E13 to the E14 cell-type universe
# Updates only the communication network slots — @net, @netP, @idents.
# Missing groups become rows/columns of zeros in the network matrices.
cellchat.E13 <- liftCellChat(cellchat.E13, group.new)

# Now both objects have matching dimensions — merge as usual
object.list <- list(E13 = cellchat.E13, E14 = cellchat.E14)
cellchat    <- mergeCellChat(object.list, add.names = names(object.list),
                              cell.prefix = TRUE)

# Apply the standard Pipeline 2 comparison functions
compareInteractions(cellchat, group = c(1, 2))
netVisual_diffInteraction(cellchat, weight.scale = TRUE)
rankNet(cellchat, mode = "comparison", stacked = TRUE, do.stat = TRUE)
netVisual_bubble(cellchat, sources.use = 4, targets.use = c(5:11),
                  comparison = c(1, 2), angle.x = 45)
```

**What lifting does** (and doesn't):
- Updates `@net`, `@netP`, `@idents` to use the new cell-type list.
- Leaves expression data (`@data.input`, `@data.signaling`) alone — these still represent only the cells the original object contained.
- Cell types only in `group.new` (not in the original object) get **zero** entries in the network matrices — they participate as nodes but never as senders/receivers.

**When to use vs. not**:
- Use lifting when cellular composition differs and you want a fair side-by-side visualization at the same set of nodes.
- Don't use lifting when you're testing "did communication X disappear in disease?" if disease cells genuinely don't exist in control — that's a meaningful biological difference, and lifting hides it.

Source: [Comparison_analysis_of_multiple_datasets_with_different_cellular_compositions.html](https://htmlpreview.github.io/?https://github.com/jinworks/CellChat/blob/master/tutorial/Comparison_analysis_of_multiple_datasets_with_different_cellular_compositions.html).

---

## Visualization Cookbook

A short tour. Full patterns in [references/plotting_guide.md](references/plotting_guide.md).

### Multi-pathway bubble plot

```r
# All significant L-R pairs from cell-group 4 to groups 5-11
netVisual_bubble(cellchat, sources.use = 4, targets.use = 5:11,
                  remove.isolate = FALSE)

# Restrict to specific pathways
netVisual_bubble(cellchat, sources.use = 4, targets.use = 5:11,
                  signaling = c("CCL", "CXCL"))

# Custom L-R pair set
pairLR.use <- extractEnrichedLR(cellchat, signaling = c("CCL", "CXCL", "FGF"))
netVisual_bubble(cellchat, pairLR.use = pairLR.use,
                  sources.use = c(3, 4), targets.use = 5:8)
```

### Chord at the L-R level

```r
# All L-R from one source to many targets
netVisual_chord_gene(cellchat, sources.use = 4, targets.use = 5:11,
                      lab.cex = 0.5, legend.pos.y = 30)

# Group cell types into super-groups before drawing
group.cellType <- c(rep("FIB", 4), rep("DC", 4), rep("TC", 4))
names(group.cellType) <- levels(cellchat@idents)
netVisual_chord_cell(cellchat, signaling = "CXCL", group = group.cellType,
                      title.name = "CXCL signaling")
```

### Gene expression of a pathway

```r
plotGeneExpression(cellchat, signaling = "CXCL", enriched.only = TRUE, type = "violin")

# In comparison mode, split by dataset
plotGeneExpression(cellchat, signaling = "CXCL", split.by = "datasets",
                    colors.ggplot = TRUE, type = "violin")
```

---

## Key Parameters to Adjust

### `createCellChat` / `group.by`
- Cell-type column should be a meaningful, stable annotation — running CellChat on coarse `cell_type` is far more interpretable than on fine `leiden_cluster`.

### `subsetDB(search = ...)`
- `"Secreted Signaling"` — cytokines, chemokines, growth factors (most common)
- `"ECM-Receptor"` — integrins, matrix
- `"Cell-Cell Contact"` — notch, ephrin
- Omit `subsetDB` to use all three (slower but more comprehensive)

### `computeCommunProb`
- `type = "triMean"` (default): robust to outliers
- `type = "truncatedMean"` + `trim = 0.1`: more stringent
- `population.size = TRUE`: down-weights small populations (use when groups vary 10×+ in size)

### `filterCommunication(min.cells = 10)`
- For HPC datasets with 100k+ cells, bump to 50-100 — single-cell-resolution false positives are an issue.

### `identifyCommunicationPatterns(k = ...)`
- Pick `k` from `selectK()` curves. Don't go higher than where the cophenetic stops dropping.

---

## Best Practices

- **Always use log-normalized counts**, not raw counts — pass `GetAssayData(seurat_obj, layer = "data")`.
- **Run identifyOverExpressedInteractions** before `computeCommunProb` — skipping it pushes CellChat to consider every L-R pair globally and inflates false positives.
- **Save the per-condition object before merging.** `mergeCellChat` doesn't preserve everything — `netAnalysis_computeCentrality` results are accessible only via the per-condition object.
- **`netAnalysis_computeCentrality` must be run on `@netP`** (`slot.name = "netP"`). Running it on `@net` (L-R level) is hours slower and rarely useful.
- **Permutation p-values are conservative** — a "non-significant" L-R may still be biologically real if you have low cell counts per group. Inspect `@net$prob` values directly when borderline.
- **For multi-dataset comparison**, run all preprocessing identically across conditions (same `subsetDB` filter, same `min.cells`, same `type`). Mismatched parameters create artifactual "differences."
- **For lifting**, build the per-condition objects on the union gene set first — otherwise a gene present in dataset A but missing from dataset B will silently kill L-R pairs after the lift.

---

## End-to-End Template

`assets/cellchat_template.R` is a single parameterized script — edit the CONFIGURATION block and run end-to-end through all three pipelines.

## Convenience Scripts

- `scripts/build_cellchat.R` — single-dataset analysis (Pipeline 1).
- `scripts/compare_cellchat.R` — multi-dataset merging + comparison (Pipeline 2); supports `--lift` for Pipeline 3.

---

## References

- [CellChat GitHub](https://github.com/jinworks/CellChat) — Jin et al., maintained at jinworks/CellChat
- [Single-dataset vignette](https://htmlpreview.github.io/?https://github.com/jinworks/CellChat/blob/master/tutorial/CellChat-vignette.html)
- [Multi-dataset comparison](https://htmlpreview.github.io/?https://github.com/jinworks/CellChat/blob/master/tutorial/Comparison_analysis_of_multiple_datasets.html)
- [Different cell compositions](https://htmlpreview.github.io/?https://github.com/jinworks/CellChat/blob/master/tutorial/Comparison_analysis_of_multiple_datasets_with_different_cellular_compositions.html)
- Jin et al. (2021), *Inference and analysis of cell-cell communication using CellChat*, *Nature Communications*
- Jin, Plikus, Nie (2025), *CellChat for systematic analysis of cell–cell communication from single-cell transcriptomics*, *Nature Protocols*
