#!/usr/bin/env Rscript
# downstream_analyses.R — run DME / module-trait / enrichment on a hdWGCNA Seurat .rds.
#
# Operates on the output of build_network.R. Pick one task or run them all.
#
# Usage:
#   # Differential MEs between conditions
#   Rscript downstream_analyses.R --rds seurat_hdwgcna.rds --task dme \
#     --cell-type INH --group-by condition --group1 AD --group2 Ctrl
#
#   # Module-trait correlation
#   Rscript downstream_analyses.R --rds seurat_hdwgcna.rds --task trait \
#     --traits age,Braak_ordered,sex_binary
#
#   # Enrichr functional enrichment
#   Rscript downstream_analyses.R --rds seurat_hdwgcna.rds --task enrich \
#     --dbs GO_Biological_Process_2023,KEGG_2021_Human
#
#   # All three
#   Rscript downstream_analyses.R --rds seurat_hdwgcna.rds --task all \
#     --cell-type INH --group-by condition --group1 AD --group2 Ctrl \
#     --traits age,Braak_ordered --dbs GO_Biological_Process_2023

suppressPackageStartupMessages({
  library(optparse)
  library(Seurat)
  library(hdWGCNA)
  library(tidyverse)
  library(cowplot)
  library(patchwork)
})

option_list <- list(
  make_option("--rds",             type = "character",                          help = "Path to hdWGCNA-augmented Seurat .rds (required)"),
  make_option("--wgcna-name",      type = "character", default = NULL,          help = "hdWGCNA experiment name [default: most recently used in the object]"),
  make_option("--task",            type = "character", default = "all",         help = "dme | trait | enrich | all [default %default]"),
  make_option("--cell-type",       type = "character", default = NULL,          help = "Cell type for DME (subset cells to this)"),
  make_option("--cell-type-col",   type = "character", default = "cell_type",   help = "Metadata column with cell types [default %default]"),
  make_option("--group-by",        type = "character", default = NULL,          help = "Metadata column for DME comparison"),
  make_option("--group1",          type = "character", default = NULL,          help = "DME group 1 value"),
  make_option("--group2",          type = "character", default = NULL,          help = "DME group 2 value"),
  make_option("--traits",          type = "character", default = NULL,          help = "Comma-separated trait column names (must be numeric)"),
  make_option("--dbs",             type = "character",
              default = "GO_Biological_Process_2023,KEGG_2021_Human,Reactome_2022",
              help = "Comma-separated Enrichr databases"),
  make_option("--max-enrich-genes", type = "integer",  default = 100,           help = "Max genes per module sent to Enrichr [default %default]"),
  make_option("--n-perms",         type = "integer",   default = 1000,          help = "Reserved for future permutation tests"),
  make_option("--outdir",          type = "character", default = "results",     help = "Output directory [default %default]"),
  make_option("--fig-dir",         type = "character", default = "figures",     help = "Figures directory [default %default]")
)
opt <- parse_args(OptionParser(option_list = option_list))

if (is.null(opt$rds)) stop("--rds is required.")
dir.create(opt$outdir,  recursive = TRUE, showWarnings = FALSE)
dir.create(opt$`fig-dir`, recursive = TRUE, showWarnings = FALSE)

message("Loading ", opt$rds, " …")
seurat_obj <- readRDS(opt$rds)
wgcna_name <- opt$`wgcna-name`
if (is.null(wgcna_name)) {
  wgcna_name <- names(seurat_obj@misc)[1]
  message("Using wgcna_name = '", wgcna_name, "' (first in @misc)")
}

# ── Task: DME ───────────────────────────────────────────────────────────────
run_dme <- function() {
  if (is.null(opt$`group-by`) || is.null(opt$group1) || is.null(opt$group2)) {
    stop("--group-by, --group1, --group2 are required for the DME task.")
  }

  meta <- seurat_obj@meta.data
  if (!is.null(opt$`cell-type`)) {
    meta <- meta %>% filter(.data[[opt$`cell-type-col`]] == opt$`cell-type`)
  }
  group1 <- meta %>% filter(.data[[opt$`group-by`]] == opt$group1) %>% rownames()
  group2 <- meta %>% filter(.data[[opt$`group-by`]] == opt$group2) %>% rownames()

  message("DME: ", opt$group1, " (n=", length(group1), ") vs ",
          opt$group2, " (n=", length(group2), ")")
  if (length(group1) < 30 || length(group2) < 30) {
    warning("Fewer than 30 cells in one group — DME results will be unstable.")
  }

  DMEs <- FindDMEs(
    seurat_obj,
    barcodes1  = group1,
    barcodes2  = group2,
    test.use   = "wilcox",
    pseudocount.use = 0.01,
    wgcna_name = wgcna_name
  )
  message("Top DMEs (by |avg_log2FC|):")
  print(DMEs %>% arrange(desc(abs(avg_log2FC))) %>% head(10))

  write_tsv(DMEs,
            file.path(opt$outdir,
                      paste0("DMEs_", opt$group1, "_vs_", opt$group2, ".tsv")))

  pdf(file.path(opt$`fig-dir`,
                paste0("DME_volcano_", opt$group1, "_vs_", opt$group2, ".pdf")),
      width = 8, height = 6)
  print(PlotDMEsVolcano(seurat_obj, DMEs, plot_labels = TRUE, wgcna_name = wgcna_name) +
        ggtitle(paste0(opt$group1, " vs ", opt$group2)))
  dev.off()

  pdf(file.path(opt$`fig-dir`,
                paste0("DME_lollipop_", opt$group1, "_vs_", opt$group2, ".pdf")),
      width = 8, height = 6)
  print(PlotDMEsLollipop(
    seurat_obj, DMEs,
    group.by    = opt$`cell-type-col`,
    comparison  = paste0(opt$group1, "_vs_", opt$group2),
    wgcna_name  = wgcna_name
  ))
  dev.off()

  invisible(DMEs)
}

# ── Task: module-trait ──────────────────────────────────────────────────────
run_trait <- function() {
  if (is.null(opt$traits)) stop("--traits required for the trait task.")
  cur_traits <- strsplit(opt$traits, ",")[[1]] %>% trimws()
  missing <- setdiff(cur_traits, colnames(seurat_obj@meta.data))
  if (length(missing) > 0) {
    stop("Traits not in metadata: ", paste(missing, collapse = ", "))
  }
  # Verify all are numeric
  non_numeric <- cur_traits[!sapply(cur_traits, function(t) is.numeric(seurat_obj@meta.data[[t]]))]
  if (length(non_numeric) > 0) {
    stop("These traits are not numeric (encode before passing): ",
         paste(non_numeric, collapse = ", "))
  }

  message("Module-trait correlation across ", opt$`cell-type-col`, " for traits: ",
          paste(cur_traits, collapse = ", "))
  seurat_obj <<- ModuleTraitCorrelation(
    seurat_obj,
    traits     = cur_traits,
    group.by   = opt$`cell-type-col`,
    wgcna_name = wgcna_name
  )

  mt_cor <- GetModuleTraitCorrelation(seurat_obj)
  saveRDS(mt_cor,
          file.path(opt$outdir, "module_trait_correlation.rds"))

  pdf(file.path(opt$`fig-dir`, "module_trait_heatmap.pdf"),
      width = 8, height = 10)
  print(PlotModuleTraitCorrelation(
    seurat_obj,
    label        = "fdr",
    label_symbol = "stars",
    text_size    = 2.5,
    high_color   = "yellow",
    mid_color    = "black",
    low_color    = "purple",
    plot_max     = 0.2,
    combine      = TRUE,
    wgcna_name   = wgcna_name
  ))
  dev.off()

  invisible(mt_cor)
}

# ── Task: enrichment ────────────────────────────────────────────────────────
run_enrich <- function() {
  dbs <- strsplit(opt$dbs, ",")[[1]] %>% trimws()
  message("Enrichr: ", length(dbs), " database(s) — ", paste(dbs, collapse = ", "))
  seurat_obj <<- RunEnrichr(
    seurat_obj,
    dbs        = dbs,
    max_genes  = opt$`max-enrich-genes`,
    wgcna_name = wgcna_name
  )

  enrich_df <- GetEnrichrTable(seurat_obj)
  write_tsv(enrich_df, file.path(opt$outdir, "enrichr_table.tsv"))

  message("Top enrichments (head):")
  print(enrich_df %>% arrange(P.value) %>% head(20))

  # Per-module bar plots
  EnrichrBarPlot(
    seurat_obj,
    outdir     = file.path(opt$`fig-dir`, "enrichr"),
    n_terms    = 10,
    plot_size  = c(5, 7),
    logscale   = TRUE,
    wgcna_name = wgcna_name
  )

  # Dot plot for first database
  pdf(file.path(opt$`fig-dir`, "enrichr_dotplot.pdf"), width = 10, height = 8)
  print(EnrichrDotPlot(
    seurat_obj,
    mods       = "all",
    database   = dbs[1],
    n_terms    = 2,
    term_size  = 8,
    p_adj      = FALSE,
    wgcna_name = wgcna_name
  ))
  dev.off()

  invisible(enrich_df)
}

# ── Dispatch ────────────────────────────────────────────────────────────────
task <- tolower(opt$task)
if (task %in% c("dme", "all"))    run_dme()
if (task %in% c("trait", "all"))  run_trait()
if (task %in% c("enrich", "all")) run_enrich()
if (!task %in% c("dme", "trait", "enrich", "all")) {
  stop("--task must be one of dme | trait | enrich | all")
}

# Always save the (possibly mutated by ModuleTraitCorrelation / RunEnrichr) Seurat
saveRDS(seurat_obj, opt$rds)
message("Saved (possibly augmented) Seurat back to ", opt$rds)
message("Done.")
