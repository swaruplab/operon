#!/usr/bin/env Rscript
# sce_template.R — end-to-end Bioconductor scRNA-seq pipeline.
#
# Configure the CONFIGURATION block and run end-to-end:
#   1. Load 10X / Seurat / h5ad
#   2. QC + scDblFinder
#   3. scran deconvolution normalization
#   4. HVG + PCA + UMAP
#   5. Louvain clustering
#   6. findMarkers
#   7. (Optional) cell-type annotation via SingleR
#   8. Save SCE + diagnostic figures

suppressPackageStartupMessages({
  library(SingleCellExperiment)
  library(scater)
  library(scran)
  library(scDblFinder)
  library(BiocSingular)
  library(BiocParallel)
  library(bluster)
})

# ============================================================================
# CONFIGURATION
# ============================================================================

# Input — pick one
INPUT          <- "data/filtered_feature_bc_matrix"   # 10X dir / .h5 / .rds (Seurat or SCE) / .h5ad

# Output
OUTPUT_RDS     <- "results/sce_processed.rds"
FIG_DIR        <- "figures"

# Species
SPECIES        <- "human"      # "human" or "mouse"

# QC
MAX_MT         <- 10
RUN_DOUBLETS   <- TRUE

# Normalization + HVG
N_HVGS         <- 2000

# DR + clustering
N_PCS          <- 30
RESOLUTION     <- 0.5
CLUSTER_METHOD <- "louvain"    # "louvain" | "leiden" | "walktrap"

# Markers
RUN_MARKERS    <- TRUE

# Optional SingleR annotation — set REFERENCE to NULL to skip
SINGLER_REF    <- NULL         # e.g. "celldex::HumanPrimaryCellAtlasData"

# Performance
THREADS        <- 4

# ============================================================================
# SETUP
# ============================================================================

dir.create(dirname(OUTPUT_RDS), recursive = TRUE, showWarnings = FALSE)
dir.create(FIG_DIR, recursive = TRUE, showWarnings = FALSE)
set.seed(1)
register(MulticoreParam(workers = THREADS))

mt_pat <- if (SPECIES == "human") "^MT-" else "^mt-"

# ============================================================================
# 1. LOAD
# ============================================================================

message("=== 1. Load ===")
load_sce <- function(input) {
  if (dir.exists(input) || endsWith(input, ".h5")) {
    library(DropletUtils)
    sce <- read10xCounts(input)
    rownames(sce) <- uniquifyFeatureNames(rowData(sce)$ID, rowData(sce)$Symbol)
    return(sce)
  }
  if (endsWith(input, ".rds")) {
    obj <- readRDS(input)
    if (inherits(obj, "Seurat")) {
      library(Seurat)
      return(Seurat::as.SingleCellExperiment(obj))
    } else if (inherits(obj, "SingleCellExperiment")) {
      return(obj)
    } else {
      stop("Unsupported .rds class: ", paste(class(obj), collapse = ","))
    }
  }
  if (endsWith(input, ".h5ad")) {
    library(zellkonverter)
    return(readH5AD(input))
  }
  stop("Unsupported input: ", input)
}

sce <- load_sce(INPUT)
message("Loaded: ", ncol(sce), " cells × ", nrow(sce), " genes")

# ============================================================================
# 2. QC + DOUBLETS
# ============================================================================

message("\n=== 2. QC ===")
is_mt <- grepl(mt_pat, rownames(sce))
sce <- addPerCellQC(sce, subsets = list(MT = is_mt))

ggplot2::ggsave(file.path(FIG_DIR, "qc_scatter.pdf"),
                 plotColData(sce, x = "sum", y = "detected",
                              colour_by = "subsets_MT_percent"),
                 width = 6, height = 5)

qc <- quickPerCellQC(sce, sub.fields = "subsets_MT_percent")
qc$discard <- qc$discard | sce$subsets_MT_percent > MAX_MT
n_before <- ncol(sce)
sce <- sce[, !qc$discard]
message("  Cells: ", n_before, " → ", ncol(sce))

if (RUN_DOUBLETS) {
  message("\n=== 3. scDblFinder ===")
  sce <- scDblFinder(sce, BPPARAM = MulticoreParam(workers = THREADS))
  n_before <- ncol(sce)
  sce <- sce[, sce$scDblFinder.class == "singlet"]
  message("  Singlets: ", n_before, " → ", ncol(sce))
}

# ============================================================================
# 4. NORMALIZATION
# ============================================================================

message("\n=== 4. Normalization (scran deconvolution) ===")
clust <- quickCluster(sce, BPPARAM = MulticoreParam(workers = THREADS))
sce <- computeSumFactors(sce, clusters = clust,
                          BPPARAM = MulticoreParam(workers = THREADS))
sce <- logNormCounts(sce)

# ============================================================================
# 5. HVG + PCA + UMAP
# ============================================================================

message("\n=== 5. HVG + PCA + UMAP ===")
dec  <- modelGeneVar(sce)
hvgs <- getTopHVGs(dec, n = N_HVGS)

sce <- runPCA(sce, subset_row = hvgs, ncomponents = N_PCS,
              BSPARAM = IrlbaParam())
sce <- runUMAP(sce, dimred = "PCA")

# ============================================================================
# 6. CLUSTERING
# ============================================================================

message("\n=== 6. Clustering (", CLUSTER_METHOD, ") ===")
sce$cluster <- clusterCells(
  sce, use.dimred = "PCA",
  BLUSPARAM = NNGraphParam(cluster.fun = CLUSTER_METHOD)
)
colLabels(sce) <- sce$cluster

ggplot2::ggsave(file.path(FIG_DIR, "umap_clusters.pdf"),
                 plotUMAP(sce, colour_by = "cluster", text_by = "cluster"),
                 width = 6, height = 5)
message("  Clusters: ", length(unique(sce$cluster)))

# ============================================================================
# 7. MARKERS
# ============================================================================

if (RUN_MARKERS) {
  message("\n=== 7. findMarkers ===")
  markers <- findMarkers(sce, groups = sce$cluster,
                          direction = "up", lfc = 0.25,
                          BPPARAM = MulticoreParam(workers = THREADS))
  saveRDS(markers, sub("\\.rds$", "_markers.rds", OUTPUT_RDS))

  top10 <- unique(unlist(lapply(markers, function(x) head(rownames(x), 10))))
  ggplot2::ggsave(file.path(FIG_DIR, "marker_heatmap.pdf"),
                   plotHeatmap(sce, features = top10,
                                order_columns_by = "cluster", center = TRUE),
                   width = 12, height = 14)
}

# ============================================================================
# 8. SINGLER ANNOTATION (optional)
# ============================================================================

if (!is.null(SINGLER_REF)) {
  message("\n=== 8. SingleR cell-type annotation (", SINGLER_REF, ") ===")
  if (!requireNamespace("SingleR", quietly = TRUE) ||
      !requireNamespace("celldex",  quietly = TRUE)) {
    message("  WARNING: install SingleR + celldex first (BiocManager::install).")
  } else {
    library(SingleR); library(celldex)
    ref <- eval(parse(text = paste0(SINGLER_REF, "()")))
    pred <- SingleR(test = sce, ref = ref, labels = ref$label.main)
    sce$singleR_label <- pred$labels
    ggplot2::ggsave(file.path(FIG_DIR, "umap_singler.pdf"),
                     plotUMAP(sce, colour_by = "singleR_label", text_by = "singleR_label"),
                     width = 8, height = 6)
  }
}

# ============================================================================
# 9. SAVE
# ============================================================================

message("\n=== 9. Save ===")
saveRDS(sce, OUTPUT_RDS)
message("Output: ", OUTPUT_RDS)
message("Figures: ", FIG_DIR, "/")
