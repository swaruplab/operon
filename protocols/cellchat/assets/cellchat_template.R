#!/usr/bin/env Rscript
# cellchat_template.R — End-to-end CellChat analysis.
#
# Configure the variables in the CONFIGURATION block, then run end-to-end:
#   - Build per-condition CellChat objects (Pipeline 1)
#   - Optionally lift one onto the other (Pipeline 3, different compositions)
#   - Merge + comparison (Pipeline 2)
#   - Dysregulated L-R analysis
#   - All key visualizations

suppressPackageStartupMessages({
  library(Seurat)
  library(CellChat)
  library(patchwork)
  library(NMF)
  library(ggalluvial)
  library(tidyverse)
})

# ============================================================================
# CONFIGURATION — edit these
# ============================================================================

# Input — a Seurat object that contains BOTH conditions, with a `condition`
# metadata column and a `cell_type` column. We'll subset per condition.
INPUT_RDS          <- "data/annotated_seurat.rds"
CELL_TYPE_COL      <- "cell_type"
CONDITION_COL      <- "condition"
CONDITION_A        <- "Ctrl"
CONDITION_B        <- "Disease"

# CellChat DB
SPECIES            <- "human"             # human | mouse | zebrafish
SIGNALING_TYPE     <- "Secreted Signaling"   # or "ECM-Receptor", "Cell-Cell Contact", "all"

# Inference
COMMUNPROB_TYPE    <- "triMean"
MIN_CELLS          <- 10

# Lifting — set to TRUE if A and B have different cell-type sets
LIFT_A_TO_B        <- FALSE
LIFT_B_TO_A        <- FALSE

# Pattern analysis
N_OUTGOING_PATTERNS <- 4
N_INCOMING_PATTERNS <- 3

# Dysregulated L-R
THRESH_FC          <- 0.05

# Performance
THREADS            <- 4

# Output
RESULTS_DIR        <- "results"
FIGURES_DIR        <- "figures"

# ============================================================================
# SETUP
# ============================================================================

dir.create(RESULTS_DIR, recursive = TRUE, showWarnings = FALSE)
dir.create(FIGURES_DIR, recursive = TRUE, showWarnings = FALSE)
options(stringsAsFactors = FALSE)
future::plan("multisession", workers = THREADS)

db <- switch(SPECIES,
             human     = CellChatDB.human,
             mouse     = CellChatDB.mouse,
             zebrafish = CellChatDB.zebrafish,
             stop("SPECIES must be human, mouse, or zebrafish"))
if (SIGNALING_TYPE != "all") {
  db <- subsetDB(db, search = SIGNALING_TYPE, key = "annotation")
}

# ── Helper: per-condition Pipeline 1 ────────────────────────────────────────
build_one <- function(seurat_subset, condition_name, db) {
  message("\n=== Building CellChat for ", condition_name, " ===")
  data.input <- GetAssayData(seurat_subset, layer = "data", assay = "RNA")
  meta       <- seurat_subset@meta.data
  cellchat   <- createCellChat(object = data.input, meta = meta,
                                group.by = CELL_TYPE_COL)
  cellchat@DB <- db
  cellchat <- subsetData(cellchat)
  cellchat <- identifyOverExpressedGenes(cellchat)
  cellchat <- identifyOverExpressedInteractions(cellchat)
  cellchat <- computeCommunProb(cellchat, type = COMMUNPROB_TYPE)
  cellchat <- filterCommunication(cellchat, min.cells = MIN_CELLS)
  cellchat <- computeCommunProbPathway(cellchat)
  cellchat <- aggregateNet(cellchat)
  cellchat <- netAnalysis_computeCentrality(cellchat, slot.name = "netP")
  saveRDS(cellchat,
          file.path(RESULTS_DIR, paste0("cellchat_", condition_name, ".rds")))
  cellchat
}

# ============================================================================
# 1. LOAD AND BUILD PER-CONDITION OBJECTS
# ============================================================================

message("Loading ", INPUT_RDS)
seurat_obj <- readRDS(INPUT_RDS)

seurat_a <- subset(seurat_obj, subset = !!sym(CONDITION_COL) == CONDITION_A)
seurat_b <- subset(seurat_obj, subset = !!sym(CONDITION_COL) == CONDITION_B)
message("Cells in ", CONDITION_A, ": ", ncol(seurat_a))
message("Cells in ", CONDITION_B, ": ", ncol(seurat_b))

cellchat.a <- build_one(seurat_a, CONDITION_A, db)
cellchat.b <- build_one(seurat_b, CONDITION_B, db)

# ============================================================================
# 2. PER-CONDITION VISUALIZATIONS
# ============================================================================

message("\n=== Per-condition visualizations ===")

for (cond in list(list(cc = cellchat.a, name = CONDITION_A),
                  list(cc = cellchat.b, name = CONDITION_B))) {
  cc <- cond$cc; nm <- cond$name

  # Outgoing / incoming role heatmaps
  pdf(file.path(FIGURES_DIR, paste0("signaling_role_", nm, ".pdf")),
      width = 12, height = 5)
  ht1 <- netAnalysis_signalingRole_heatmap(cc, pattern = "outgoing")
  ht2 <- netAnalysis_signalingRole_heatmap(cc, pattern = "incoming")
  ComplexHeatmap::draw(ht1 + ht2, ht_gap = grid::unit(0.5, "cm"))
  dev.off()

  # Sender-receiver scatter
  gg <- netAnalysis_signalingRole_scatter(cc) +
        ggtitle(paste("Sender × Receiver —", nm))
  ggsave(file.path(FIGURES_DIR, paste0("role_scatter_", nm, ".pdf")),
         plot = gg, width = 6, height = 6)
}

# ============================================================================
# 3. (OPTIONAL) LIFT ONE OBJECT TO MATCH THE OTHER'S COMPOSITION
# ============================================================================

if (LIFT_A_TO_B) {
  message("\n=== Lifting ", CONDITION_A, " onto ", CONDITION_B, "'s cell-type universe ===")
  cellchat.a <- liftCellChat(cellchat.a, group.new = levels(cellchat.b@idents))
}
if (LIFT_B_TO_A) {
  message("\n=== Lifting ", CONDITION_B, " onto ", CONDITION_A, "'s cell-type universe ===")
  cellchat.b <- liftCellChat(cellchat.b, group.new = levels(cellchat.a@idents))
}

# ============================================================================
# 4. MERGE AND COMPARE
# ============================================================================

message("\n=== Merging and comparing ===")
object.list <- setNames(list(cellchat.a, cellchat.b), c(CONDITION_A, CONDITION_B))
cellchat    <- mergeCellChat(object.list,
                              add.names    = names(object.list),
                              cell.prefix  = (LIFT_A_TO_B || LIFT_B_TO_A))

# Overall comparison
gg1 <- compareInteractions(cellchat, show.legend = FALSE, group = c(1, 2))
gg2 <- compareInteractions(cellchat, show.legend = FALSE, group = c(1, 2),
                            measure = "weight")
ggsave(file.path(FIGURES_DIR, "comparison_overall_bars.pdf"),
       plot = gg1 + gg2, width = 8, height = 4)

# Differential network
pdf(file.path(FIGURES_DIR, "comparison_diff_interaction.pdf"),
    width = 10, height = 5)
par(mfrow = c(1, 2), xpd = TRUE)
netVisual_diffInteraction(cellchat, weight.scale = TRUE, measure = "count")
netVisual_diffInteraction(cellchat, weight.scale = TRUE, measure = "weight")
dev.off()

# Pathway ranking
gg3 <- rankNet(cellchat, mode = "comparison", measure = "weight",
               stacked = TRUE,  do.stat = TRUE)
gg4 <- rankNet(cellchat, mode = "comparison", measure = "weight",
               stacked = FALSE, do.stat = TRUE)
ggsave(file.path(FIGURES_DIR, "comparison_rankNet.pdf"),
       plot = gg3 + gg4, width = 14, height = 8)

# ============================================================================
# 5. DYSREGULATED L-R PAIRS
# ============================================================================

message("\n=== Dysregulated L-R pairs ===")
pos.dataset   <- CONDITION_B
features.name <- paste0(pos.dataset, ".merged")

cellchat <- identifyOverExpressedGenes(
  cellchat, group.dataset = "datasets",
  pos.dataset   = pos.dataset,
  features.name = features.name,
  only.pos      = FALSE,
  thresh.pc     = 0.1, thresh.fc = THRESH_FC, thresh.p = 0.05,
  group.DE.combined = FALSE
)
net <- netMappingDEG(cellchat, features.name = features.name, variable.all = TRUE)

net.up   <- subsetCommunication(cellchat, net = net, datasets = pos.dataset,
                                 ligand.logFC = THRESH_FC,    receptor.logFC = NULL)
net.down <- subsetCommunication(cellchat, net = net, datasets = CONDITION_A,
                                 ligand.logFC = -THRESH_FC,   receptor.logFC = NULL)
message("  Up:   ", nrow(net.up), " L-R pairs")
message("  Down: ", nrow(net.down), " L-R pairs")

write_tsv(net.up,   file.path(RESULTS_DIR, paste0("dme_LR_up_in_",   pos.dataset, ".tsv")))
write_tsv(net.down, file.path(RESULTS_DIR, paste0("dme_LR_down_in_", pos.dataset, ".tsv")))

# Bubble plots of dysregulated pairs
if (nrow(net.up) > 0) {
  pairLR.use.up <- net.up[, "interaction_name", drop = FALSE]
  gg <- netVisual_bubble(cellchat,
    pairLR.use     = pairLR.use.up,
    sources.use    = seq_len(length(levels(cellchat@idents))),
    targets.use    = seq_len(length(levels(cellchat@idents))),
    comparison     = c(1, 2),
    angle.x        = 90,
    remove.isolate = TRUE,
    title.name     = paste("Up-regulated in", pos.dataset)
  )
  ggsave(file.path(FIGURES_DIR, "dme_bubble_up.pdf"),
         plot = gg, width = 14, height = 10, limitsize = FALSE)
}
if (nrow(net.down) > 0) {
  pairLR.use.down <- net.down[, "interaction_name", drop = FALSE]
  gg <- netVisual_bubble(cellchat,
    pairLR.use     = pairLR.use.down,
    sources.use    = seq_len(length(levels(cellchat@idents))),
    targets.use    = seq_len(length(levels(cellchat@idents))),
    comparison     = c(1, 2),
    angle.x        = 90,
    remove.isolate = TRUE,
    title.name     = paste("Up-regulated in", CONDITION_A)
  )
  ggsave(file.path(FIGURES_DIR, "dme_bubble_down.pdf"),
         plot = gg, width = 14, height = 10, limitsize = FALSE)
}

# ============================================================================
# 6. PATHWAY MANIFOLD (functional + structural similarity)
# ============================================================================

message("\n=== Pathway manifold ===")
for (sim_type in c("functional", "structural")) {
  tryCatch({
    cellchat <- computeNetSimilarityPairwise(cellchat, type = sim_type)
    cellchat <- netEmbedding(cellchat, type = sim_type)
    cellchat <- netClustering(cellchat, type = sim_type)
    gg <- netVisual_embeddingPairwise(cellchat, type = sim_type, label.size = 3.5)
    ggsave(file.path(FIGURES_DIR, paste0("pathway_manifold_", sim_type, ".pdf")),
           plot = gg, width = 8, height = 6)
  }, error = function(e) {
    message("  ", sim_type, " manifold skipped: ", conditionMessage(e))
  })
}

# ============================================================================
# 7. SAVE
# ============================================================================

saveRDS(cellchat, file.path(RESULTS_DIR, "cellchat_merged.rds"))
message("\nDone.")
message("  Merged object → ", file.path(RESULTS_DIR, "cellchat_merged.rds"))
message("  Figures        → ", FIGURES_DIR, "/")
message("  Tables         → ", RESULTS_DIR, "/")
