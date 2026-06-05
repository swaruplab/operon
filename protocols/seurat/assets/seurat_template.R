#!/usr/bin/env Rscript
# seurat_template.R — end-to-end Seurat v5 pipeline.
#
# Edit the CONFIGURATION block and run end-to-end. Single-sample or multi-sample
# (via INPUT_MODE). Multi-sample uses IntegrateLayers with the selected method.

suppressPackageStartupMessages({
  library(Seurat)
  library(dplyr)
  library(patchwork)
})

# ============================================================================
# CONFIGURATION — edit these
# ============================================================================

INPUT_MODE       <- "single"          # "single" or "multi"

# Single mode
SINGLE_INPUT     <- "data/filtered_feature_bc_matrix"   # 10X dir / .h5 / .csv

# Multi mode — list of (name, path)
MULTI_SAMPLES <- list(
  list(name = "donor1", path = "data/donor1/filtered_feature_bc_matrix"),
  list(name = "donor2", path = "data/donor2/filtered_feature_bc_matrix")
)

# Output
OUTPUT_RDS       <- "results/seurat.rds"
FIG_DIR          <- "figures"

# Species + QC
SPECIES          <- "human"           # "human" or "mouse"
MIN_FEATURES     <- 200
MAX_FEATURES     <- 2500
MAX_MT           <- 5

# Normalization + clustering
USE_SCT          <- FALSE             # TRUE = SCTransform path
N_FEATURES       <- 2000
DIMS             <- 30
RESOLUTION       <- 0.5

# Integration (only used in multi mode)
INTEGRATION_METHOD <- "harmony"       # harmony | cca | rpca | fastmnn | scvi

# Markers
COMPUTE_MARKERS  <- TRUE

# ============================================================================
# SETUP
# ============================================================================

dir.create(dirname(OUTPUT_RDS), recursive = TRUE, showWarnings = FALSE)
dir.create(FIG_DIR, recursive = TRUE, showWarnings = FALSE)
set.seed(1)
mt_pat <- if (SPECIES == "human") "^MT-" else "^mt-"

load_one <- function(path) {
  if (dir.exists(path))            counts <- Read10X(path)
  else if (endsWith(path, ".h5"))  counts <- Read10X_h5(path)
  else if (endsWith(path, ".csv")) counts <- as.matrix(read.csv(path, row.names = 1))
  else                             stop("Unsupported input: ", path)
  counts
}

# ============================================================================
# 1. LOAD + (MULTI) MERGE
# ============================================================================

if (INPUT_MODE == "single") {
  message("\n=== SINGLE SAMPLE ===")
  counts <- load_one(SINGLE_INPUT)
  obj <- CreateSeuratObject(counts, min.cells = 3, min.features = MIN_FEATURES)
} else {
  message("\n=== MULTI SAMPLE (", length(MULTI_SAMPLES), " samples) ===")
  per_sample <- lapply(MULTI_SAMPLES, function(s) {
    counts <- load_one(s$path)
    o <- CreateSeuratObject(counts, min.cells = 3, min.features = MIN_FEATURES)
    o$sample <- s$name
    o
  })
  names(per_sample) <- sapply(MULTI_SAMPLES, function(s) s$name)
  obj <- merge(per_sample[[1]], y = per_sample[-1],
                add.cell.ids = names(per_sample), project = "integrated")
}
message("Loaded: ", ncol(obj), " cells × ", nrow(obj), " genes")

# ============================================================================
# 2. QC
# ============================================================================

message("\n=== QC ===")
obj[["percent.mt"]] <- PercentageFeatureSet(obj, pattern = mt_pat)

ggplot2::ggsave(file.path(FIG_DIR, "qc_violin.pdf"),
                 VlnPlot(obj, features = c("nFeature_RNA", "nCount_RNA", "percent.mt"),
                          ncol = 3, pt.size = 0),
                 width = 10, height = 4)

obj <- subset(obj, subset = nFeature_RNA > MIN_FEATURES &
                              nFeature_RNA < MAX_FEATURES &
                              percent.mt   < MAX_MT)
message("After QC: ", ncol(obj), " cells")

# ============================================================================
# 3. NORMALIZE + PREPROCESS
# ============================================================================

message("\n=== ", if (USE_SCT) "SCTransform" else "LogNormalize", " ===")

if (INPUT_MODE == "multi") {
  obj[["RNA"]] <- split(obj[["RNA"]], f = obj$sample)
  message("Split RNA into layers: ", paste(Layers(obj), collapse = ", "))
}

if (USE_SCT) {
  obj <- SCTransform(obj, verbose = FALSE)
} else {
  obj <- NormalizeData(obj)
  obj <- FindVariableFeatures(obj, nfeatures = N_FEATURES)
  obj <- ScaleData(obj)
}

obj <- RunPCA(obj)
ggplot2::ggsave(file.path(FIG_DIR, "elbow.pdf"),
                 ElbowPlot(obj, ndims = 50), width = 6, height = 4)

# ============================================================================
# 4. INTEGRATION (multi mode only)
# ============================================================================

primary_rep <- "pca"
if (INPUT_MODE == "multi") {
  message("\n=== Integration (", INTEGRATION_METHOD, ") ===")

  method_map <- list(
    cca     = list(fn = CCAIntegration,     new_rep = "integrated.cca"),
    rpca    = list(fn = RPCAIntegration,    new_rep = "integrated.rpca"),
    harmony = list(fn = HarmonyIntegration, new_rep = "harmony"),
    fastmnn = list(fn = FastMNNIntegration, new_rep = "integrated.mnn"),
    scvi    = list(fn = scVIIntegration,    new_rep = "integrated.scvi")
  )
  m <- method_map[[tolower(INTEGRATION_METHOD)]]

  extra_args <- list()
  if (USE_SCT) extra_args$normalization.method <- "SCT"

  obj <- do.call(IntegrateLayers, c(list(
    object         = obj,
    method         = m$fn,
    orig.reduction = "pca",
    new.reduction  = m$new_rep,
    verbose        = FALSE
  ), extra_args))

  primary_rep <- m$new_rep
}

# ============================================================================
# 5. CLUSTERS + UMAP
# ============================================================================

message("\n=== Clusters + UMAP (rep = ", primary_rep, ") ===")
obj <- FindNeighbors(obj, reduction = primary_rep, dims = 1:DIMS)
obj <- FindClusters (obj, resolution = RESOLUTION)
obj <- RunUMAP     (obj, reduction = primary_rep, dims = 1:DIMS)

p_clust <- DimPlot(obj, label = TRUE) + NoLegend()
ggplot2::ggsave(file.path(FIG_DIR, "umap_clusters.pdf"), p_clust, width = 6, height = 5)
if (INPUT_MODE == "multi") {
  ggplot2::ggsave(file.path(FIG_DIR, "umap_sample.pdf"),
                   DimPlot(obj, group.by = "sample"), width = 7, height = 5)
}
message("Clusters: ", length(unique(obj$seurat_clusters)))

# ============================================================================
# 6. JOIN LAYERS + (optional) MARKERS
# ============================================================================

if (INPUT_MODE == "multi") {
  message("\n=== Rejoining layers ===")
  obj[["RNA"]] <- JoinLayers(obj[["RNA"]])
  if (USE_SCT) obj <- PrepSCTFindMarkers(obj)
}

if (COMPUTE_MARKERS) {
  message("\n=== FindAllMarkers ===")
  markers <- FindAllMarkers(obj, only.pos = TRUE,
                             min.pct = 0.25, logfc.threshold = 0.25)
  saveRDS(markers, sub("\\.rds$", "_markers.rds", OUTPUT_RDS))
  message("  Marker rows: ", nrow(markers))

  top10 <- markers %>% group_by(cluster) %>% slice_max(n = 10, order_by = avg_log2FC)
  ggplot2::ggsave(file.path(FIG_DIR, "marker_heatmap.pdf"),
                   DoHeatmap(subset(obj, downsample = 100),
                              features = top10$gene) + NoLegend(),
                   width = 14, height = 12)
}

# ============================================================================
# 7. SAVE
# ============================================================================

message("\n=== Save ===")
saveRDS(obj, OUTPUT_RDS)
message("Output: ", OUTPUT_RDS)
message("Figures: ", FIG_DIR, "/")
