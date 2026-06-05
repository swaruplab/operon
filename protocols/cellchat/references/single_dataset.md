# Single-Dataset CellChat — Deep Dive

Comprehensive reference for the standard per-dataset pipeline. Every function in the order it should run, what it modifies, and what to inspect at each step.

## The CellChat object

The CellChat S4 object carries everything: input data, database, intermediate results, and final networks. Key slots:

| Slot | Filled by | Contains |
|---|---|---|
| `@data.input` | `createCellChat` | Log-normalized expression (genes × cells) |
| `@data.signaling` | `subsetData` | Same matrix subset to genes in the database |
| `@meta` / `@idents` | `createCellChat` | Cell metadata + active cell-group factor |
| `@DB` | manual assignment | Active ligand-receptor database |
| `@LR` | `identifyOverExpressedInteractions` | Filtered L-R pairs with over-expressed components |
| `@var.features` / `@DEG` | `identifyOverExpressedGenes` | Per-cell-group marker genes |
| `@net` | `computeCommunProb` | L-R-level probability tensors + p-values + counts/weights |
| `@netP` | `computeCommunProbPathway` | Pathway-level aggregated network + centrality |
| `@patterns` | `identifyCommunicationPatterns` | NMF outgoing/incoming pattern matrices |

`@net` and `@netP` are the only ones that matter for downstream comparison and visualization.

## Step 1 — `createCellChat`

Three accepted input shapes.

### From Seurat (most common)

```r
library(Seurat); library(CellChat)
data.input <- GetAssayData(seurat_obj, layer = "data", assay = "RNA")  # log-normalized
meta       <- seurat_obj@meta.data
cellchat   <- createCellChat(object = data.input, meta = meta,
                             group.by = "cell_type")
```

### From SingleCellExperiment

```r
library(SingleCellExperiment)
data.input <- logcounts(sce)
meta       <- colData(sce) %>% as.data.frame()
cellchat   <- createCellChat(object = data.input, meta = meta,
                             group.by = "cell_type")
```

### From raw matrix + metadata

```r
data.input <- log1p(counts_matrix / Matrix::colSums(counts_matrix) * 1e4)  # quick normalize
meta       <- data.frame(cell_type = annotations, row.names = colnames(data.input))
cellchat   <- createCellChat(object = data.input, meta = meta,
                             group.by = "cell_type")
```

### Inspect after creation

```r
cellchat
# An object of class CellChat created from a single dataset
#  1234 genes.
#  5000 cells.
levels(cellchat@idents)
# [1] "B"      "CD4_T"  "CD8_T"  "Mono"   "NK"     "Fibro"
table(cellchat@idents)
```

If cell groups < 10 cells, they'll be dropped by `filterCommunication`. If a group has < 50 cells, expect noisy inference.

## Step 2 — Pick the database

CellChat ships three databases:

```r
CellChatDB.human       # ~3000 L-R pairs
CellChatDB.mouse       # ~2000 L-R pairs
CellChatDB.zebrafish   # smaller, less curated
```

Each has three categories:

```r
showDatabaseCategory(CellChatDB.human)
# Secreted Signaling     1199
# ECM-Receptor            421
# Cell-Cell Contact       319
```

Most users start with Secreted Signaling:

```r
CellChatDB.use <- subsetDB(CellChatDB.human,
                            search = "Secreted Signaling",
                            key    = "annotation")
cellchat@DB <- CellChatDB.use
```

To use the full database:

```r
cellchat@DB <- CellChatDB.human
```

You can also restrict by specific pathways:

```r
# Only chemokines and TGFβ
CellChatDB.use <- subsetDB(CellChatDB.human,
                            search = c("CXCL", "CCL", "TGFb"),
                            key    = "pathway_name")
```

## Step 3 — Identify candidate L-R pairs

Three pre-processing calls. Skipping any of them silently degrades inference quality.

```r
cellchat <- subsetData(cellchat)                          # restrict to genes in @DB
cellchat <- identifyOverExpressedGenes(cellchat)          # per-group DE
cellchat <- identifyOverExpressedInteractions(cellchat)   # keep L-R where both sides are DE somewhere
```

After this, `length(cellchat@LR)` reflects how many L-R pairs survived. Expect ~30-50% of the input DB to pass.

## Step 4 — Compute communication probability

The core inference step.

```r
future::plan("multisession", workers = 4)   # parallel
cellchat <- computeCommunProb(cellchat,
                              type = "triMean",
                              raw.use = TRUE,
                              population.size = FALSE,
                              trim = 0.25,                # ignored unless type = "truncatedMean"
                              nboot = 100)
```

| Argument | What it does |
|---|---|
| `type` | `"triMean"` (default, robust), `"truncatedMean"`, `"thresholdedMean"`, `"median"` |
| `raw.use` | Use raw expression (`TRUE`, default) vs. projected via PPI (`projectData` first) |
| `population.size` | Multiply by cell-group fraction. Use when group sizes vary 10×+ |
| `trim` | Trim fraction for `truncatedMean` |
| `nboot` | Bootstrap rounds for permutation p-value (100 = default, 1000 = stricter) |

### Optional PPI projection (smoothing)

For sparse data or low-expression interactions, project expression through the human PPI network first:

```r
cellchat <- projectData(cellchat, PPI.human)
# Then either pass raw.use = FALSE OR the projected matrix is used automatically
cellchat <- computeCommunProb(cellchat, type = "triMean", raw.use = FALSE)
```

PPI projection helps modestly with very sparse droplet data and can be skipped for ≥ 100 cells/group.

## Step 5 — Filter and aggregate

```r
cellchat <- filterCommunication(cellchat, min.cells = 10)
cellchat <- computeCommunProbPathway(cellchat)
cellchat <- aggregateNet(cellchat)
```

- `min.cells = 10` drops L-R from groups with too few cells. Bump to 50-100 for ≥ 100k cell datasets.
- `computeCommunProbPathway` aggregates L-R pairs that share a `pathway_name` annotation.
- `aggregateNet` produces the cell-cell summary matrices used by most visualizations.

After this, inspect:

```r
dim(cellchat@net$prob)
# [1] 6 6 853   — 6 cell groups, 6 cell groups, 853 L-R pairs

dim(cellchat@netP$prob)
# [1] 6 6 64    — 6 × 6 × 64 pathways

cellchat@net$count[1:3, 1:3]
#        B  CD4_T  CD8_T
#   B    0     12     8
#   CD4 18      0    15
```

## Step 6 — Centrality

```r
cellchat <- netAnalysis_computeCentrality(cellchat, slot.name = "netP")
```

**Always pass `slot.name = "netP"`** — running on `@net` (L-R level) is hours slower and rarely informative. The centrality scores live at `cellchat@netP$centr` and feed `netAnalysis_signalingRole_*` plots.

## Step 7 — Save

```r
saveRDS(cellchat, "results/cellchat_<condition>.rds")
```

The CellChat object can be large (50-500 MB depending on dataset). The `@net$prob` tensor scales as O(N² × LR) so very many cell groups (> 30) inflate it quickly. Consider coarse-grained cell-type labels.

## Common diagnostics

### "Why are my modules empty?"

```r
df <- subsetCommunication(cellchat)
if (nrow(df) == 0) {
  # No significant communications found. Causes:
  #  1. Filter too strict: lower thresh.fc / thresh.p in identifyOverExpressedGenes
  #  2. Group sizes too small: re-cluster to coarser cell types
  #  3. Database mismatch: are you using CellChatDB.mouse on human data?
}
```

### "Why is one cell type doing everything?"

```r
rowSums(cellchat@net$count)
# B      CD4_T  CD8_T  Mono   NK     Fibro
# 4      12     6      89     5      11
# Mono sending most signals — usually macrophage / fibroblast secretome
# dominates. Run population.size = TRUE in computeCommunProb to correct.
```

### "Centrality says X is the top sender but the chord plot doesn't show that."

Centrality is per-pathway; visualizations aggregate. Run `netAnalysis_signalingRole_scatter(cellchat, signaling = "<pathway>")` for the per-pathway view.

## Pipeline checklist

```r
cellchat <- createCellChat(object = data.input, meta = meta, group.by = "cell_type")
cellchat@DB <- subsetDB(CellChatDB.human, search = "Secreted Signaling", key = "annotation")
cellchat <- subsetData(cellchat)
cellchat <- identifyOverExpressedGenes(cellchat)
cellchat <- identifyOverExpressedInteractions(cellchat)
cellchat <- computeCommunProb(cellchat, type = "triMean")
cellchat <- filterCommunication(cellchat, min.cells = 10)
cellchat <- computeCommunProbPathway(cellchat)
cellchat <- aggregateNet(cellchat)
cellchat <- netAnalysis_computeCentrality(cellchat, slot.name = "netP")
saveRDS(cellchat, "results/cellchat.rds")
```

If you ever lose track of which step you're on, `cellchat@options$parameter` lists every CellChat call's arguments in order. Useful for reproducing or auditing prior runs.
