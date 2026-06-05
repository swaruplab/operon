#!/usr/bin/env Rscript
# build_cellchat.R — turnkey single-dataset CellChat analysis.
#
# Reads a Seurat .rds, runs the full pipeline (createCellChat → DB setup →
# identifyOverExpressed* → computeCommunProb → filter → pathway aggregation →
# centrality), and writes the resulting CellChat object to disk.
#
# Usage:
#   Rscript build_cellchat.R --rds seurat.rds --group-by cell_type \
#                             --species human --signaling 'Secreted Signaling' \
#                             --out cellchat.rds
#
# Required:
#   --rds        Path to a Seurat .rds with log-normalized data in the 'data' layer
#   --group-by   Metadata column with cell-type labels
#
# Optional:
#   --species    human | mouse | zebrafish [default human]
#   --signaling  'Secreted Signaling' | 'ECM-Receptor' | 'Cell-Cell Contact' | 'all'  [default all]
#   --min-cells  Filter threshold for filterCommunication [default 10]
#   --type       computeCommunProb type: triMean | truncatedMean | thresholdedMean [default triMean]
#   --threads    future::plan workers [default 4]
#   --out        Output .rds path [default cellchat.rds]

suppressPackageStartupMessages({
  library(optparse)
  library(Seurat)
  library(CellChat)
  library(tidyverse)
})

option_list <- list(
  make_option("--rds",        type = "character",                              help = "Seurat .rds (required)"),
  make_option("--group-by",   type = "character", default = "cell_type",       help = "Metadata column with cell types [%default]"),
  make_option("--species",    type = "character", default = "human",           help = "human | mouse | zebrafish [%default]"),
  make_option("--signaling",  type = "character", default = "all",
              help = "'Secreted Signaling' | 'ECM-Receptor' | 'Cell-Cell Contact' | 'all' [%default]"),
  make_option("--min-cells",  type = "integer",   default = 10,                help = "filterCommunication min cells [%default]"),
  make_option("--type",       type = "character", default = "triMean",         help = "computeCommunProb type [%default]"),
  make_option("--threads",    type = "integer",   default = 4,                 help = "future workers [%default]"),
  make_option("--out",        type = "character", default = "cellchat.rds",    help = "Output .rds [%default]")
)
opt <- parse_args(OptionParser(option_list = option_list))
if (is.null(opt$rds)) stop("--rds is required.")

options(stringsAsFactors = FALSE)
future::plan("multisession", workers = opt$threads)

# ── 1. Load + create ────────────────────────────────────────────────────────
message("Loading ", opt$rds, " …")
seurat_obj <- readRDS(opt$rds)
data.input <- GetAssayData(seurat_obj, layer = "data", assay = "RNA")
meta       <- seurat_obj@meta.data
if (!opt$`group-by` %in% colnames(meta))
  stop("--group-by '", opt$`group-by`, "' not in seurat_obj@meta.data")

message("Cells: ", ncol(data.input), " | Cell groups: ",
        length(unique(meta[[opt$`group-by`]])))
cellchat <- createCellChat(object = data.input, meta = meta, group.by = opt$`group-by`)

# ── 2. Database ─────────────────────────────────────────────────────────────
db <- switch(opt$species,
             human     = CellChatDB.human,
             mouse     = CellChatDB.mouse,
             zebrafish = CellChatDB.zebrafish,
             stop("--species must be human, mouse, or zebrafish"))
message("Using CellChatDB.", opt$species)

if (opt$signaling != "all") {
  message("Subsetting DB to '", opt$signaling, "'")
  db <- subsetDB(db, search = opt$signaling, key = "annotation")
}
cellchat@DB <- db

# ── 3. Identify over-expressed L-R pairs ────────────────────────────────────
message("Identifying over-expressed interactions …")
cellchat <- subsetData(cellchat)
cellchat <- identifyOverExpressedGenes(cellchat)
cellchat <- identifyOverExpressedInteractions(cellchat)

# ── 4. Communication probability ────────────────────────────────────────────
message("Computing communication probability (type = ", opt$type, ") …")
cellchat <- computeCommunProb(cellchat, type = opt$type)
cellchat <- filterCommunication(cellchat, min.cells = opt$`min-cells`)
cellchat <- computeCommunProbPathway(cellchat)
cellchat <- aggregateNet(cellchat)

# ── 5. Centrality ───────────────────────────────────────────────────────────
message("Computing centrality …")
cellchat <- netAnalysis_computeCentrality(cellchat, slot.name = "netP")

# ── 6. Summary ──────────────────────────────────────────────────────────────
n_lr   <- dim(cellchat@net$prob)[3]
n_path <- dim(cellchat@netP$prob)[3]
message("Done.")
message("  L-R pairs detected:  ", n_lr)
message("  Pathways detected:   ", n_path)
message("  Total significant communications: ", nrow(subsetCommunication(cellchat)))

saveRDS(cellchat, opt$out)
message("Saved to ", opt$out)
