#!/usr/bin/env Rscript
# build_seurat.R — single-sample Seurat pipeline through clustering + markers.
#
# Reads a 10X directory / h5 / counts matrix, runs QC + normalization + HVG +
# PCA + clustering + UMAP + marker discovery, writes the annotated Seurat .rds.
#
# Usage:
#   # 10X cellranger directory
#   Rscript build_seurat.R --input data/filtered_feature_bc_matrix --out pbmc.rds
#
#   # 10X .h5 file
#   Rscript build_seurat.R --input data/filtered_feature_bc_matrix.h5 --out pbmc.rds
#
#   # SCTransform path
#   Rscript build_seurat.R --input data/... --out pbmc.rds --sct
#
# Optional flags:
#   --project       Project name [default: scrna]
#   --species       human | mouse — controls mt pattern [default: human]
#   --min-features  Min features per cell [default: 200]
#   --max-features  Max features per cell [default: 2500]
#   --max-mt        Max percent.mt [default: 5]
#   --nfeatures     Variable features count [default: 2000]
#   --dims          PCs to use [default: 10]
#   --resolution    Leiden resolution [default: 0.5]
#   --sct           Use SCTransform instead of LogNormalize
#   --markers       Compute markers via FindAllMarkers
#   --threads       presto threads [default: 4]

suppressPackageStartupMessages({
  library(optparse)
  library(Seurat)
  library(dplyr)
})

option_list <- list(
  make_option("--input",        type = "character",                       help = "Path to 10X dir / .h5 / .csv (required)"),
  make_option("--out",          type = "character", default = "seurat.rds", help = "Output .rds [%default]"),
  make_option("--project",      type = "character", default = "scrna",    help = "Project name [%default]"),
  make_option("--species",      type = "character", default = "human",    help = "human | mouse [%default]"),
  make_option("--min-features", type = "integer",   default = 200),
  make_option("--max-features", type = "integer",   default = 2500),
  make_option("--max-mt",       type = "double",    default = 5),
  make_option("--min-cells",    type = "integer",   default = 3),
  make_option("--nfeatures",    type = "integer",   default = 2000),
  make_option("--dims",         type = "integer",   default = 10),
  make_option("--resolution",   type = "double",    default = 0.5),
  make_option("--sct",          action = "store_true", default = FALSE,    help = "Use SCTransform"),
  make_option("--markers",      action = "store_true", default = FALSE,    help = "Run FindAllMarkers"),
  make_option("--fig-dir",      type = "character", default = "figures")
)
opt <- parse_args(OptionParser(option_list = option_list))
if (is.null(opt$input)) stop("--input is required")
set.seed(1)

dir.create(opt$`fig-dir`, recursive = TRUE, showWarnings = FALSE)

# ── 1. Load ─────────────────────────────────────────────────────────────────
message("[1/8] Loading ", opt$input, " …")
if (dir.exists(opt$input)) {
  counts <- Read10X(data.dir = opt$input)
} else if (endsWith(opt$input, ".h5")) {
  counts <- Read10X_h5(opt$input)
} else if (endsWith(opt$input, ".csv") || endsWith(opt$input, ".tsv")) {
  counts <- as.matrix(read.csv(opt$input, row.names = 1))
} else if (endsWith(opt$input, ".rds")) {
  obj <- readRDS(opt$input)
  counts <- GetAssayData(obj, layer = "counts", assay = "RNA")
  rm(obj)
} else {
  stop("Unsupported input format: ", opt$input)
}

obj <- CreateSeuratObject(
  counts       = counts,
  project      = opt$project,
  min.cells    = opt$`min-cells`,
  min.features = opt$`min-features`
)
message("  Loaded: ", ncol(obj), " cells × ", nrow(obj), " genes")

# ── 2. QC ───────────────────────────────────────────────────────────────────
message("[2/8] QC …")
mt_pat <- if (opt$species == "human") "^MT-" else "^mt-"
obj[["percent.mt"]] <- PercentageFeatureSet(obj, pattern = mt_pat)

p_qc <- VlnPlot(obj, features = c("nFeature_RNA", "nCount_RNA", "percent.mt"),
                  ncol = 3, pt.size = 0)
ggplot2::ggsave(file.path(opt$`fig-dir`, "qc_violin.pdf"), p_qc, width = 10, height = 4)

n_before <- ncol(obj)
obj <- subset(obj,
              subset = nFeature_RNA > opt$`min-features` &
                       nFeature_RNA < opt$`max-features` &
                       percent.mt   < opt$`max-mt`)
message("  Cells: ", n_before, " → ", ncol(obj))

# ── 3. Normalization + HVG ──────────────────────────────────────────────────
if (opt$sct) {
  message("[3/8] SCTransform …")
  obj <- SCTransform(obj, vars.to.regress = "percent.mt", verbose = FALSE)
} else {
  message("[3/8] LogNormalize + FindVariableFeatures (", opt$nfeatures, ") …")
  obj <- NormalizeData(obj, normalization.method = "LogNormalize", scale.factor = 10000)
  obj <- FindVariableFeatures(obj, selection.method = "vst", nfeatures = opt$nfeatures)
  obj <- ScaleData(obj, features = rownames(obj))
}

# ── 4. PCA ──────────────────────────────────────────────────────────────────
message("[4/8] PCA …")
obj <- RunPCA(obj, features = VariableFeatures(obj))
ggplot2::ggsave(file.path(opt$`fig-dir`, "elbow.pdf"),
                ElbowPlot(obj, ndims = 50), width = 6, height = 4)

# ── 5. Clusters + UMAP ─────────────────────────────────────────────────────
message("[5/8] Neighbors + clusters (dims=1:", opt$dims, ", res=", opt$resolution, ") + UMAP …")
obj <- FindNeighbors(obj, dims = 1:opt$dims)
obj <- FindClusters (obj, resolution = opt$resolution)
obj <- RunUMAP     (obj, dims = 1:opt$dims)

p_umap <- DimPlot(obj, reduction = "umap", label = TRUE) + NoLegend()
ggplot2::ggsave(file.path(opt$`fig-dir`, "umap_clusters.pdf"), p_umap, width = 6, height = 5)
message("  Clusters: ", length(unique(obj$seurat_clusters)))

# ── 6. Marker genes (optional) ─────────────────────────────────────────────
if (opt$markers) {
  message("[6/8] FindAllMarkers (presto-accelerated if available) …")
  markers <- FindAllMarkers(obj, only.pos = TRUE,
                             min.pct = 0.25, logfc.threshold = 0.25)
  saveRDS(markers, sub("\\.rds$", "_markers.rds", opt$out))
  message("  Marker rows: ", nrow(markers))
  top10 <- markers %>% group_by(cluster) %>% slice_max(n = 10, order_by = avg_log2FC)
  p_heat <- DoHeatmap(obj, features = top10$gene) + NoLegend()
  ggplot2::ggsave(file.path(opt$`fig-dir`, "marker_heatmap.pdf"),
                   p_heat, width = 14, height = 10)
} else {
  message("[6/8] Skipping markers (--markers not set)")
}

# ── 7. Save ────────────────────────────────────────────────────────────────
message("[7/8] Saving Seurat object → ", opt$out)
saveRDS(obj, opt$out)

message("[8/8] Done.")
message("  Reload: obj <- readRDS('", opt$out, "')")
