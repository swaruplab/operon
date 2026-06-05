#!/usr/bin/env Rscript
# build_network.R — turnkey hdWGCNA network construction from a Seurat object.
#
# Reads an annotated Seurat .rds, runs the full pipeline (metacells → soft power
# → ConstructNetwork → ModuleEigengenes → ModuleConnectivity), and saves the
# augmented Seurat object back to disk. Network construction is the slow step;
# downstream analyses (DME, module-trait, enrichment) are fast on the output.
#
# Usage:
#   Rscript build_network.R --rds annotated.rds --cell-type INH --sample-col Sample \
#                            --reduction harmony --out seurat_hdwgcna.rds
#
# Required columns in seurat_obj@meta.data:
#   - the value passed to --cell-type-col (default: cell_type) with the target group
#   - the value passed to --sample-col   (default: Sample) — donor / batch identifier
#
# Output: an .rds containing the same Seurat object with the hdWGCNA experiment
# in seurat_obj@misc[[wgcna_name]]. The TOM matrix is written separately to
# TOM/<wgcna_name>_TOM.rda — keep it next to the .rds.

suppressPackageStartupMessages({
  library(optparse)
  library(Seurat)
  library(hdWGCNA)
  library(WGCNA)
  library(tidyverse)
  library(cowplot)
  library(patchwork)
  library(UCell)
})

# ── CLI ────────────────────────────────────────────────────────────────────
option_list <- list(
  make_option("--rds",             type = "character",                       help = "Path to annotated Seurat .rds (required)"),
  make_option("--cell-type",       type = "character",                       help = "Cell type group to build the network on (required)"),
  make_option("--cell-type-col",   type = "character", default = "cell_type", help = "Metadata column with cell types [default %default]"),
  make_option("--sample-col",      type = "character", default = "Sample",   help = "Metadata column with sample/donor IDs [default %default]"),
  make_option("--reduction",       type = "character", default = "harmony",  help = "Reduction used for metacell k-NN [default %default]"),
  make_option("--gene-select",     type = "character", default = "fraction", help = "fraction | variable | custom [default %default]"),
  make_option("--fraction",        type = "double",    default = 0.05,       help = "Expression fraction cutoff if gene-select=fraction [default %default]"),
  make_option("--metacell-k",      type = "integer",   default = 25,         help = "Cells per metacell [default %default]"),
  make_option("--max-shared",      type = "integer",   default = 10,         help = "Max neighbor sharing across metacells [default %default]"),
  make_option("--min-cells",       type = "integer",   default = 100,        help = "Drop (cell_type x sample) bins with fewer cells [default %default]"),
  make_option("--network-type",    type = "character", default = "signed",   help = "signed | unsigned [default %default]"),
  make_option("--min-module-size", type = "integer",   default = 50,         help = "ConstructNetwork: smallest allowed module [default %default]"),
  make_option("--merge-cut",       type = "double",    default = 0.2,        help = "ConstructNetwork: mergeCutHeight [default %default]"),
  make_option("--deep-split",      type = "integer",   default = 2,          help = "ConstructNetwork: deepSplit (0-4) [default %default]"),
  make_option("--n-hubs",          type = "integer",   default = 25,         help = "Top-N hubs per module for UCell scoring [default %default]"),
  make_option("--threads",         type = "integer",   default = 8,          help = "WGCNA threads [default %default]"),
  make_option("--out",             type = "character", default = "seurat_hdwgcna.rds", help = "Output .rds path [default %default]"),
  make_option("--fig-dir",         type = "character", default = "figures",  help = "Where to write diagnostic figures [default %default]")
)
opt <- parse_args(OptionParser(option_list = option_list))

if (is.null(opt$rds) || is.null(opt$`cell-type`)) {
  stop("--rds and --cell-type are required. See --help.")
}
dir.create(opt$`fig-dir`, recursive = TRUE, showWarnings = FALSE)
dir.create("TOM", showWarnings = FALSE)

# ── 1. Load + set up ────────────────────────────────────────────────────────
allowWGCNAThreads(nThreads = opt$threads)
message("Loading ", opt$rds, " …")
seurat_obj <- readRDS(opt$rds)

ct <- opt$`cell-type`
wgcna_name <- ct

message("Cells in target group: ",
        sum(seurat_obj[[opt$`cell-type-col`]] == ct))

seurat_obj <- SetupForWGCNA(
  seurat_obj,
  gene_select = opt$`gene-select`,
  fraction    = opt$fraction,
  wgcna_name  = wgcna_name
)

# ── 2. Metacells ────────────────────────────────────────────────────────────
message("Building metacells (k=", opt$`metacell-k`,
        ", max_shared=", opt$`max-shared`, ") …")
seurat_obj <- MetacellsByGroups(
  seurat_obj,
  group.by    = c(opt$`cell-type-col`, opt$`sample-col`),
  reduction   = opt$reduction,
  k           = opt$`metacell-k`,
  max_shared  = opt$`max-shared`,
  min_cells   = opt$`min-cells`,
  ident.group = opt$`cell-type-col`
)
seurat_obj <- NormalizeMetacells(seurat_obj)

metacell_obj <- GetMetacellObject(seurat_obj)
ct_count <- sum(metacell_obj[[opt$`cell-type-col`]] == ct)
message("Metacells in '", ct, "': ", ct_count)
if (ct_count < 50) {
  warning("Fewer than 50 metacells for '", ct, "' — network may be unstable. ",
          "Consider lowering --metacell-k or pooling related cell types.")
}

# ── 3. Target the cell type ─────────────────────────────────────────────────
seurat_obj <- SetDatExpr(
  seurat_obj,
  group_name = ct,
  group.by   = opt$`cell-type-col`,
  assay      = "RNA",
  layer      = "data"
)

# ── 4. Soft-power ───────────────────────────────────────────────────────────
message("Testing soft powers …")
seurat_obj <- TestSoftPowers(seurat_obj, networkType = opt$`network-type`)

plot_list <- PlotSoftPowers(seurat_obj)
ggsave(file.path(opt$`fig-dir`, paste0("soft_power_", ct, ".pdf")),
       plot = wrap_plots(plot_list, ncol = 2),
       width = 12, height = 8)

power_table <- GetPowerTable(seurat_obj)
message("\nPower table:")
print(power_table)
message("Selected power: ", power_table$Power[which(power_table$SFT.R.sq >= 0.8)[1]])

# ── 5. Construct network ────────────────────────────────────────────────────
message("\nConstructing network — this is the slow step …")
seurat_obj <- ConstructNetwork(
  seurat_obj,
  setDatExpr      = FALSE,
  tom_name        = wgcna_name,
  tom_outdir      = "TOM/",
  minModuleSize   = opt$`min-module-size`,
  mergeCutHeight  = opt$`merge-cut`,
  deepSplit       = opt$`deep-split`,
  overwrite_tom   = TRUE,
  networkType     = opt$`network-type`
)

pdf(file.path(opt$`fig-dir`, paste0("dendrogram_", ct, ".pdf")),
    width = 12, height = 6)
PlotDendrogram(seurat_obj, main = paste0(ct, " hdWGCNA Dendrogram"))
dev.off()

# ── 6. Module eigengenes (Harmony-corrected) ────────────────────────────────
message("Computing module eigengenes (harmonized over ", opt$`sample-col`, ") …")
seurat_obj <- ScaleData(seurat_obj, features = VariableFeatures(seurat_obj))
seurat_obj <- ModuleEigengenes(seurat_obj, group.by.vars = opt$`sample-col`)

# ── 7. kME + hub scoring ────────────────────────────────────────────────────
seurat_obj <- ModuleConnectivity(
  seurat_obj,
  group.by   = opt$`cell-type-col`,
  group_name = ct
)
seurat_obj <- ResetModuleNames(seurat_obj, new_name = paste0(ct, "-M"))

seurat_obj <- ModuleExprScore(
  seurat_obj,
  n_genes = opt$`n-hubs`,
  method  = "UCell"
)

# ── 8. Summary outputs ──────────────────────────────────────────────────────
modules <- GetModules(seurat_obj) %>% subset(module != "grey")
hub_df  <- GetHubGenes(seurat_obj, n_hubs = opt$`n-hubs`)

message("\nModule sizes:")
print(table(modules$module))

message("\nTop 10 hubs (head):")
print(head(hub_df, 10))

# Save kME distribution plot
p_kmes <- PlotKMEs(seurat_obj, ncol = 5)
ggsave(file.path(opt$`fig-dir`, paste0("kme_distributions_", ct, ".pdf")),
       plot = p_kmes, width = 14, height = 8)

# ── 9. Save ─────────────────────────────────────────────────────────────────
message("\nWriting ", opt$out, " …")
saveRDS(seurat_obj, opt$out)

write_tsv(modules,
          file.path(opt$`fig-dir`, paste0("modules_", ct, ".tsv")))
write_tsv(hub_df,
          file.path(opt$`fig-dir`, paste0("hub_genes_", ct, ".tsv")))

message("Done. TOM is at TOM/", wgcna_name, "_TOM.rda — keep it next to the .rds.")
