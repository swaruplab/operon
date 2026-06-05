#!/usr/bin/env Rscript
# compare_cellchat.R — compare two CellChat objects.
#
# Reads two per-condition CellChat .rds files (built by build_cellchat.R),
# optionally lifts one to match the other's cell-type universe, merges, and
# writes the merged object + diagnostic figures.
#
# Usage:
#   # Same cell types in both
#   Rscript compare_cellchat.R --a cellchat_ctrl.rds --b cellchat_disease.rds \
#                               --name-a Ctrl --name-b Disease --out merged.rds
#
#   # Different cell types — auto-lift A onto B's cell-type universe
#   Rscript compare_cellchat.R --a cellchat_E13.rds  --b cellchat_E14.rds \
#                               --name-a E13 --name-b E14 --lift a --out merged.rds
#
# Outputs:
#   --out                          merged CellChat object
#   figures/comparison_overall.pdf   compareInteractions + diffInteraction
#   figures/comparison_rankNet.pdf   pathway ranking
#   figures/comparison_heatmap.pdf   diff heatmap
#   results/dme_LR_up.tsv            up-regulated L-R pairs (in --b)
#   results/dme_LR_down.tsv          down-regulated L-R pairs

suppressPackageStartupMessages({
  library(optparse)
  library(CellChat)
  library(patchwork)
  library(tidyverse)
})

option_list <- list(
  make_option("--a",       type = "character",                        help = "Path to condition A CellChat .rds (required)"),
  make_option("--b",       type = "character",                        help = "Path to condition B CellChat .rds (required)"),
  make_option("--name-a",  type = "character", default = "A",         help = "Display name for A [%default]"),
  make_option("--name-b",  type = "character", default = "B",         help = "Display name for B [%default]"),
  make_option("--lift",    type = "character", default = "none",      help = "Lift which? a | b | none [%default]"),
  make_option("--species", type = "character", default = "human",     help = "human | mouse | zebrafish — for enrichment scoring [%default]"),
  make_option("--pos-dataset", type = "character", default = NULL,    help = "Which dataset is the perturbed / case (defaults to --name-b)"),
  make_option("--thresh-fc", type = "double",  default = 0.05,        help = "logFC threshold for DE on dysregulated L-R [%default]"),
  make_option("--out",     type = "character", default = "merged.rds", help = "Output merged .rds [%default]"),
  make_option("--fig-dir", type = "character", default = "figures",   help = "Figures directory [%default]"),
  make_option("--results-dir", type = "character", default = "results", help = "Results directory [%default]")
)
opt <- parse_args(OptionParser(option_list = option_list))
if (is.null(opt$a) || is.null(opt$b)) stop("--a and --b are required.")
if (is.null(opt$`pos-dataset`)) opt$`pos-dataset` <- opt$`name-b`

dir.create(opt$`fig-dir`,     recursive = TRUE, showWarnings = FALSE)
dir.create(opt$`results-dir`, recursive = TRUE, showWarnings = FALSE)

# ── 1. Load + optional lift ─────────────────────────────────────────────────
message("Loading A: ", opt$a)
cc.a <- readRDS(opt$a)
message("Loading B: ", opt$b)
cc.b <- readRDS(opt$b)

lift_choice <- tolower(opt$lift)
if (lift_choice == "a") {
  message("Lifting A onto B's cell-type universe …")
  cc.a <- liftCellChat(cc.a, group.new = levels(cc.b@idents))
} else if (lift_choice == "b") {
  message("Lifting B onto A's cell-type universe …")
  cc.b <- liftCellChat(cc.b, group.new = levels(cc.a@idents))
} else if (lift_choice != "none") {
  stop("--lift must be a | b | none")
}

# ── 2. Merge ────────────────────────────────────────────────────────────────
message("Merging …")
object.list <- setNames(list(cc.a, cc.b), c(opt$`name-a`, opt$`name-b`))
cellchat    <- mergeCellChat(object.list,
                              add.names    = names(object.list),
                              cell.prefix  = (lift_choice != "none"))

# ── 3. Overall comparison plots ─────────────────────────────────────────────
message("Plotting overall comparison …")
gg1 <- compareInteractions(cellchat, show.legend = FALSE, group = c(1, 2),
                            measure = "count")
gg2 <- compareInteractions(cellchat, show.legend = FALSE, group = c(1, 2),
                            measure = "weight")
ggsave(file.path(opt$`fig-dir`, "comparison_overall_bars.pdf"),
       plot = gg1 + gg2, width = 8, height = 4)

pdf(file.path(opt$`fig-dir`, "comparison_diff_interaction.pdf"),
    width = 10, height = 5)
par(mfrow = c(1, 2), xpd = TRUE)
netVisual_diffInteraction(cellchat, weight.scale = TRUE, measure = "count")
netVisual_diffInteraction(cellchat, weight.scale = TRUE, measure = "weight")
dev.off()

# ── 4. Pathway ranking ──────────────────────────────────────────────────────
message("Ranking pathways …")
gg3 <- rankNet(cellchat, mode = "comparison", measure = "weight",
               stacked = TRUE, do.stat = TRUE)
gg4 <- rankNet(cellchat, mode = "comparison", measure = "weight",
               stacked = FALSE, do.stat = TRUE)
ggsave(file.path(opt$`fig-dir`, "comparison_rankNet.pdf"),
       plot = gg3 + gg4, width = 12, height = 8)

# ── 5. Heatmap ──────────────────────────────────────────────────────────────
pdf(file.path(opt$`fig-dir`, "comparison_heatmap.pdf"),
    width = 10, height = 5)
gh1 <- netVisual_heatmap(cellchat)
gh2 <- netVisual_heatmap(cellchat, measure = "weight")
ComplexHeatmap::draw(gh1 + gh2, ht_gap = grid::unit(0.5, "cm"))
dev.off()

# ── 6. Dysregulated L-R pairs ───────────────────────────────────────────────
pos.dataset   <- opt$`pos-dataset`
features.name <- paste0(pos.dataset, ".merged")

message("Identifying dysregulated L-R pairs (pos.dataset = ", pos.dataset, ") …")
cellchat <- identifyOverExpressedGenes(
  cellchat, group.dataset = "datasets",
  pos.dataset   = pos.dataset,
  features.name = features.name,
  only.pos      = FALSE,
  thresh.pc     = 0.1,
  thresh.fc     = opt$`thresh-fc`,
  thresh.p      = 0.05,
  group.DE.combined = FALSE
)
net <- netMappingDEG(cellchat, features.name = features.name, variable.all = TRUE)

other.dataset <- setdiff(c(opt$`name-a`, opt$`name-b`), pos.dataset)
net.up <- subsetCommunication(cellchat, net = net,
                               datasets       = pos.dataset,
                               ligand.logFC   = opt$`thresh-fc`,
                               receptor.logFC = NULL)
net.down <- subsetCommunication(cellchat, net = net,
                                 datasets       = other.dataset,
                                 ligand.logFC   = -opt$`thresh-fc`,
                                 receptor.logFC = NULL)

write_tsv(net.up,   file.path(opt$`results-dir`,
                                paste0("dme_LR_up_in_", pos.dataset, ".tsv")))
write_tsv(net.down, file.path(opt$`results-dir`,
                                paste0("dme_LR_down_in_", pos.dataset, ".tsv")))

message("  L-R pairs up   in ", pos.dataset,   ": ", nrow(net.up))
message("  L-R pairs down in ", pos.dataset,   ": ", nrow(net.down))

# ── 7. Enrichment (optional, only if hits) ──────────────────────────────────
if (nrow(net.up) > 0) {
  tryCatch({
    enr.up <- computeEnrichmentScore(net.up, species = opt$species,
                                      variable.both = TRUE)
    write_tsv(enr.up, file.path(opt$`results-dir`, "enrichment_up.tsv"))
  }, error = function(e) message("Enrichment (up) skipped: ", conditionMessage(e)))
}
if (nrow(net.down) > 0) {
  tryCatch({
    enr.down <- computeEnrichmentScore(net.down, species = opt$species,
                                        variable.both = TRUE)
    write_tsv(enr.down, file.path(opt$`results-dir`, "enrichment_down.tsv"))
  }, error = function(e) message("Enrichment (down) skipped: ", conditionMessage(e)))
}

# ── 8. Save ─────────────────────────────────────────────────────────────────
saveRDS(cellchat, opt$out)
message("Saved merged object to ", opt$out)
message("\nFigures:  ", opt$`fig-dir`, "/")
message("Tables:   ",   opt$`results-dir`, "/")
