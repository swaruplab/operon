#!/usr/bin/env Rscript
# hdwgcna_template.R — End-to-end hdWGCNA template.
#
# Edit the CONFIGURATION block, then run end-to-end:
#   - Build network for one cell type
#   - Compute MEs, hubs, UCell scores
#   - Differential MEs between two conditions
#   - Module-trait correlation
#   - Enrichr functional enrichment
#   - Save augmented Seurat .rds + diagnostic figures
#
# This is the "single-file" version. For long-running production analyses,
# split using the scripts in this protocol's scripts/ folder.

suppressPackageStartupMessages({
  library(Seurat)
  library(hdWGCNA)
  library(WGCNA)
  library(tidyverse)
  library(cowplot)
  library(patchwork)
  library(UCell)
  library(enrichR)
})

# ============================================================================
# CONFIGURATION — edit these
# ============================================================================

INPUT_RDS         <- "data/annotated_seurat.rds"   # your annotated Seurat object
OUTPUT_RDS        <- "results/seurat_hdwgcna.rds"
FIGURES_DIR       <- "figures"
RESULTS_DIR       <- "results"
TOM_DIR           <- "TOM"

# Network — change CELL_TYPE per analysis
CELL_TYPE         <- "INH"
CELL_TYPE_COL     <- "cell_type"
SAMPLE_COL        <- "Sample"
REDUCTION         <- "harmony"
GENE_SELECT       <- "fraction"
GENE_FRACTION     <- 0.05

# Metacells
METACELL_K        <- 25
MAX_SHARED        <- 10
MIN_CELLS         <- 100

# Network construction
NETWORK_TYPE      <- "signed"
MIN_MODULE_SIZE   <- 50
MERGE_CUT         <- 0.2
DEEP_SPLIT        <- 2
N_HUBS            <- 25

# Differential MEs
DME_GROUP_BY      <- "condition"
DME_GROUP1        <- "AD"
DME_GROUP2        <- "Ctrl"

# Module-trait correlation — must all be numeric
TRAITS            <- c("age", "is_disease")          # edit per analysis

# Enrichment
ENRICHR_DBS       <- c("GO_Biological_Process_2023", "GO_Cellular_Component_2023",
                       "GO_Molecular_Function_2023", "KEGG_2021_Human",
                       "Reactome_2022")
MAX_ENRICH_GENES  <- 100

# Performance
WGCNA_THREADS     <- 8

# ============================================================================
# SETUP
# ============================================================================

dir.create(FIGURES_DIR, recursive = TRUE, showWarnings = FALSE)
dir.create(RESULTS_DIR, recursive = TRUE, showWarnings = FALSE)
dir.create(TOM_DIR, recursive = TRUE, showWarnings = FALSE)

allowWGCNAThreads(nThreads = WGCNA_THREADS)
theme_set(theme_cowplot())
wgcna_name <- CELL_TYPE

# ============================================================================
# 1. LOAD + SET UP
# ============================================================================

message("\n=== 1. LOAD ===")
seurat_obj <- readRDS(INPUT_RDS)
message("Cells total: ", ncol(seurat_obj))
message("Cells in '", CELL_TYPE, "': ",
        sum(seurat_obj[[CELL_TYPE_COL]] == CELL_TYPE))

seurat_obj <- SetupForWGCNA(
  seurat_obj,
  gene_select = GENE_SELECT,
  fraction    = GENE_FRACTION,
  wgcna_name  = wgcna_name
)

# ============================================================================
# 2. METACELLS
# ============================================================================

message("\n=== 2. METACELLS ===")
seurat_obj <- MetacellsByGroups(
  seurat_obj,
  group.by    = c(CELL_TYPE_COL, SAMPLE_COL),
  reduction   = REDUCTION,
  k           = METACELL_K,
  max_shared  = MAX_SHARED,
  min_cells   = MIN_CELLS,
  ident.group = CELL_TYPE_COL
)
seurat_obj <- NormalizeMetacells(seurat_obj)

metacell_obj <- GetMetacellObject(seurat_obj)
n_metacells_ct <- sum(metacell_obj[[CELL_TYPE_COL]] == CELL_TYPE)
message("Metacells in '", CELL_TYPE, "': ", n_metacells_ct)
if (n_metacells_ct < 50) {
  warning("Fewer than 50 metacells — modules may be unstable.")
}

# ============================================================================
# 3. SET EXPRESSION + SOFT POWER
# ============================================================================

message("\n=== 3. SOFT POWER ===")
seurat_obj <- SetDatExpr(
  seurat_obj,
  group_name = CELL_TYPE,
  group.by   = CELL_TYPE_COL,
  assay      = "RNA",
  layer      = "data"
)
seurat_obj <- TestSoftPowers(seurat_obj, networkType = NETWORK_TYPE)
plot_list <- PlotSoftPowers(seurat_obj)
ggsave(file.path(FIGURES_DIR, paste0("soft_power_", CELL_TYPE, ".pdf")),
       plot = wrap_plots(plot_list, ncol = 2), width = 12, height = 8)

# ============================================================================
# 4. CONSTRUCT NETWORK (slow step)
# ============================================================================

message("\n=== 4. CONSTRUCT NETWORK ===")
seurat_obj <- ConstructNetwork(
  seurat_obj,
  setDatExpr      = FALSE,
  tom_name        = wgcna_name,
  tom_outdir      = TOM_DIR,
  minModuleSize   = MIN_MODULE_SIZE,
  mergeCutHeight  = MERGE_CUT,
  deepSplit       = DEEP_SPLIT,
  overwrite_tom   = TRUE,
  networkType     = NETWORK_TYPE
)
pdf(file.path(FIGURES_DIR, paste0("dendrogram_", CELL_TYPE, ".pdf")),
    width = 12, height = 6)
PlotDendrogram(seurat_obj, main = paste0(CELL_TYPE, " hdWGCNA Dendrogram"))
dev.off()

# ============================================================================
# 5. MODULE EIGENGENES + kME + HUB SCORING
# ============================================================================

message("\n=== 5. MODULE EIGENGENES + kME ===")
seurat_obj <- ScaleData(seurat_obj, features = VariableFeatures(seurat_obj))
seurat_obj <- ModuleEigengenes(seurat_obj, group.by.vars = SAMPLE_COL)
seurat_obj <- ModuleConnectivity(seurat_obj,
                                  group.by = CELL_TYPE_COL, group_name = CELL_TYPE)
seurat_obj <- ResetModuleNames(seurat_obj, new_name = paste0(CELL_TYPE, "-M"))
seurat_obj <- ModuleExprScore(seurat_obj, n_genes = N_HUBS, method = "UCell")

modules <- GetModules(seurat_obj) %>% subset(module != "grey")
hub_df  <- GetHubGenes(seurat_obj, n_hubs = N_HUBS)
message("Modules found: ", length(unique(modules$module)) - 1, " (excluding grey)")
write_tsv(modules, file.path(RESULTS_DIR, paste0("modules_", CELL_TYPE, ".tsv")))
write_tsv(hub_df,  file.path(RESULTS_DIR, paste0("hub_genes_", CELL_TYPE, ".tsv")))

# Module feature plots
plot_list <- ModuleFeaturePlot(seurat_obj, features = "hMEs", order = TRUE)
ggsave(file.path(FIGURES_DIR, paste0("module_feature_umap_", CELL_TYPE, ".pdf")),
       plot = wrap_plots(plot_list, ncol = 6), width = 18, height = 12)

# Module dot plot across cell types
hMEs <- GetMEs(seurat_obj, harmonized = TRUE)
mods <- levels(modules$module); mods <- mods[mods != "grey"]
seurat_obj@meta.data <- cbind(seurat_obj@meta.data, hMEs)
p_dot <- DotPlot(seurat_obj, features = mods, group.by = CELL_TYPE_COL) +
  RotatedAxis() +
  scale_color_gradient2(high = "red", mid = "grey95", low = "blue")
ggsave(file.path(FIGURES_DIR, paste0("module_dotplot_", CELL_TYPE, ".pdf")),
       plot = p_dot, width = 10, height = 6)

# ============================================================================
# 6. DIFFERENTIAL MEs
# ============================================================================

message("\n=== 6. DIFFERENTIAL MEs (", DME_GROUP1, " vs ", DME_GROUP2, ") ===")

meta_ct <- seurat_obj@meta.data %>% filter(.data[[CELL_TYPE_COL]] == CELL_TYPE)
group1 <- meta_ct %>% filter(.data[[DME_GROUP_BY]] == DME_GROUP1) %>% rownames()
group2 <- meta_ct %>% filter(.data[[DME_GROUP_BY]] == DME_GROUP2) %>% rownames()

if (length(group1) >= 30 && length(group2) >= 30) {
  DMEs <- FindDMEs(
    seurat_obj,
    barcodes1 = group1, barcodes2 = group2,
    test.use  = "wilcox", pseudocount.use = 0.01,
    wgcna_name = wgcna_name
  )
  write_tsv(DMEs, file.path(RESULTS_DIR,
                            paste0("DMEs_", DME_GROUP1, "_vs_", DME_GROUP2, ".tsv")))

  pdf(file.path(FIGURES_DIR, "DME_volcano.pdf"), width = 8, height = 6)
  print(PlotDMEsVolcano(seurat_obj, DMEs, plot_labels = TRUE, wgcna_name = wgcna_name))
  dev.off()

  pdf(file.path(FIGURES_DIR, "DME_lollipop.pdf"), width = 8, height = 6)
  print(PlotDMEsLollipop(seurat_obj, DMEs, group.by = CELL_TYPE_COL,
                          comparison = paste0(DME_GROUP1, "_vs_", DME_GROUP2),
                          wgcna_name = wgcna_name))
  dev.off()
} else {
  warning("DME skipped — not enough cells in one group (g1=",
          length(group1), " g2=", length(group2), ")")
}

# ============================================================================
# 7. MODULE-TRAIT CORRELATION
# ============================================================================

if (length(TRAITS) > 0) {
  message("\n=== 7. MODULE-TRAIT CORRELATION ===")
  non_numeric <- TRAITS[!sapply(TRAITS, function(t) is.numeric(seurat_obj@meta.data[[t]]))]
  if (length(non_numeric) > 0) {
    warning("Skipping non-numeric traits (encode them first): ",
            paste(non_numeric, collapse = ", "))
    TRAITS <- setdiff(TRAITS, non_numeric)
  }

  if (length(TRAITS) > 0) {
    seurat_obj <- ModuleTraitCorrelation(
      seurat_obj,
      traits     = TRAITS,
      group.by   = CELL_TYPE_COL,
      wgcna_name = wgcna_name
    )
    mt_cor <- GetModuleTraitCorrelation(seurat_obj)
    saveRDS(mt_cor, file.path(RESULTS_DIR, "module_trait_correlation.rds"))

    pdf(file.path(FIGURES_DIR, "module_trait_heatmap.pdf"), width = 8, height = 10)
    print(PlotModuleTraitCorrelation(
      seurat_obj, label = "fdr", label_symbol = "stars",
      text_size = 2.5, high_color = "yellow", mid_color = "black",
      low_color = "purple", plot_max = 0.2, combine = TRUE,
      wgcna_name = wgcna_name
    ))
    dev.off()
  }
}

# ============================================================================
# 8. FUNCTIONAL ENRICHMENT
# ============================================================================

message("\n=== 8. ENRICHMENT ===")
seurat_obj <- RunEnrichr(
  seurat_obj,
  dbs        = ENRICHR_DBS,
  max_genes  = MAX_ENRICH_GENES,
  wgcna_name = wgcna_name
)
enrich_df <- GetEnrichrTable(seurat_obj)
write_tsv(enrich_df, file.path(RESULTS_DIR, "enrichr_table.tsv"))

EnrichrBarPlot(
  seurat_obj,
  outdir     = file.path(FIGURES_DIR, "enrichr"),
  n_terms    = 10,
  plot_size  = c(5, 7),
  logscale   = TRUE,
  wgcna_name = wgcna_name
)

pdf(file.path(FIGURES_DIR, "enrichr_dotplot.pdf"), width = 10, height = 8)
print(EnrichrDotPlot(
  seurat_obj, mods = "all",
  database   = ENRICHR_DBS[1],
  n_terms    = 2, term_size = 8, p_adj = FALSE,
  wgcna_name = wgcna_name
))
dev.off()

# ============================================================================
# 9. SAVE
# ============================================================================

message("\n=== 9. SAVE ===")
saveRDS(seurat_obj, OUTPUT_RDS)
message("Done.")
message("  Augmented Seurat → ", OUTPUT_RDS)
message("  TOM matrix       → ", file.path(TOM_DIR, paste0(wgcna_name, "_TOM.rda")))
message("  Figures          → ", FIGURES_DIR, "/")
message("  Tables           → ", RESULTS_DIR, "/")
