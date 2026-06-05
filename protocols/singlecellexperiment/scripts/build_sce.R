#!/usr/bin/env Rscript
# build_sce.R — turnkey SingleCellExperiment pipeline.
#
# Loads a 10X directory / h5 / Seurat .rds, runs the standard scater + scran
# pipeline (QC → doublets → normalization → HVG → PCA → UMAP → clustering →
# markers), writes the annotated SCE .rds.
#
# Usage:
#   Rscript build_sce.R --input data/filtered_feature_bc_matrix --out sce.rds
#   Rscript build_sce.R --input data/seurat.rds                 --out sce.rds
#
# Optional flags:
#   --species          human | mouse [default human]
#   --max-mt           Max percent.mt [default 10]
#   --no-doublets      Skip scDblFinder
#   --n-hvgs           Top HVGs [default 2000]
#   --n-pcs            PCA components [default 30]
#   --resolution       Louvain resolution [default 0.5]
#   --markers          Run findMarkers
#   --threads          BiocParallel workers [default 4]

suppressPackageStartupMessages({
  library(optparse)
  library(SingleCellExperiment)
  library(scater)
  library(scran)
  library(BiocSingular)
  library(BiocParallel)
})

option_list <- list(
  make_option("--input",       type = "character",                       help = "10X dir / .h5 / Seurat .rds (required)"),
  make_option("--out",         type = "character", default = "sce.rds",  help = "Output .rds [%default]"),
  make_option("--species",     type = "character", default = "human",    help = "human | mouse [%default]"),
  make_option("--max-mt",      type = "double",    default = 10,         help = "Max percent.mt [%default]"),
  make_option("--no-doublets", action = "store_true", default = FALSE,   help = "Skip scDblFinder"),
  make_option("--n-hvgs",      type = "integer",   default = 2000),
  make_option("--n-pcs",       type = "integer",   default = 30),
  make_option("--resolution",  type = "double",    default = 0.5),
  make_option("--markers",     action = "store_true", default = FALSE),
  make_option("--threads",     type = "integer",   default = 4),
  make_option("--fig-dir",     type = "character", default = "figures")
)
opt <- parse_args(OptionParser(option_list = option_list))
if (is.null(opt$input)) stop("--input is required")

set.seed(1)
dir.create(opt$`fig-dir`, recursive = TRUE, showWarnings = FALSE)
register(MulticoreParam(workers = opt$threads))

# ── 1. Load ─────────────────────────────────────────────────────────────────
message("[1/9] Loading ", opt$input, " …")

if (dir.exists(opt$input)) {
  if (!requireNamespace("DropletUtils", quietly = TRUE))
    BiocManager::install("DropletUtils", ask = FALSE, update = FALSE)
  library(DropletUtils)
  sce <- read10xCounts(opt$input)
  rownames(sce) <- uniquifyFeatureNames(rowData(sce)$ID, rowData(sce)$Symbol)
} else if (endsWith(opt$input, ".h5")) {
  library(DropletUtils)
  sce <- read10xCounts(opt$input)
  rownames(sce) <- uniquifyFeatureNames(rowData(sce)$ID, rowData(sce)$Symbol)
} else if (endsWith(opt$input, ".rds")) {
  obj <- readRDS(opt$input)
  if (inherits(obj, "Seurat")) {
    if (!requireNamespace("Seurat", quietly = TRUE))
      stop("Need Seurat to convert .rds — install.packages('Seurat')")
    sce <- Seurat::as.SingleCellExperiment(obj)
  } else if (inherits(obj, "SingleCellExperiment")) {
    sce <- obj
  } else {
    stop("Unsupported .rds class: ", paste(class(obj), collapse = ", "))
  }
  rm(obj)
} else {
  stop("Unsupported input: ", opt$input)
}
message("  Loaded: ", ncol(sce), " cells × ", nrow(sce), " genes")

# ── 2. QC ───────────────────────────────────────────────────────────────────
message("[2/9] QC …")
mt_pat <- if (opt$species == "human") "^MT-" else "^mt-"
is_mt <- grepl(mt_pat, rownames(sce))
sce <- addPerCellQC(sce, subsets = list(MT = is_mt))

# Plot QC
p_mt <- plotColData(sce, x = "sum", y = "detected", colour_by = "subsets_MT_percent")
ggplot2::ggsave(file.path(opt$`fig-dir`, "qc_scatter.pdf"), p_mt, width = 6, height = 5)
ggplot2::ggsave(file.path(opt$`fig-dir`, "qc_mt_violin.pdf"),
                 plotColData(sce, y = "subsets_MT_percent"), width = 6, height = 5)

# Quick adaptive QC + the user's MT cap as a hard floor
qc <- quickPerCellQC(sce, sub.fields = "subsets_MT_percent")
qc$discard <- qc$discard | sce$subsets_MT_percent > opt$`max-mt`
n_before <- ncol(sce)
sce <- sce[, !qc$discard]
message("  Cells: ", n_before, " → ", ncol(sce))

# ── 3. Doublets ────────────────────────────────────────────────────────────
if (!opt$`no-doublets`) {
  message("[3/9] scDblFinder …")
  library(scDblFinder)
  sce <- scDblFinder(sce, BPPARAM = MulticoreParam(workers = opt$threads))
  n_before <- ncol(sce)
  sce <- sce[, sce$scDblFinder.class == "singlet"]
  message("  Singlets: ", n_before, " → ", ncol(sce))
} else {
  message("[3/9] Skipping scDblFinder (--no-doublets)")
}

# ── 4. Normalization ───────────────────────────────────────────────────────
message("[4/9] scran deconvolution normalization …")
clust <- quickCluster(sce, BPPARAM = MulticoreParam(workers = opt$threads))
sce <- computeSumFactors(sce, clusters = clust,
                         BPPARAM = MulticoreParam(workers = opt$threads))
sce <- logNormCounts(sce)

# ── 5. HVGs ─────────────────────────────────────────────────────────────────
message("[5/9] Modeling gene variance + selecting top ", opt$`n-hvgs`, " HVGs …")
dec  <- modelGeneVar(sce)
hvgs <- getTopHVGs(dec, n = opt$`n-hvgs`)

# ── 6. PCA + UMAP ──────────────────────────────────────────────────────────
message("[6/9] PCA (", opt$`n-pcs`, " comps) + UMAP …")
sce <- runPCA(sce, subset_row = hvgs, ncomponents = opt$`n-pcs`,
              BSPARAM = IrlbaParam())
sce <- runUMAP(sce, dimred = "PCA")

# ── 7. Clustering ──────────────────────────────────────────────────────────
message("[7/9] Louvain clustering (res=", opt$resolution, ") …")
library(bluster)
sce$cluster <- clusterCells(
  sce, use.dimred = "PCA",
  BLUSPARAM = NNGraphParam(cluster.fun = "louvain")
)
colLabels(sce) <- sce$cluster
message("  Clusters: ", length(unique(sce$cluster)))

p_umap <- plotUMAP(sce, colour_by = "cluster", text_by = "cluster")
ggplot2::ggsave(file.path(opt$`fig-dir`, "umap_clusters.pdf"), p_umap,
                 width = 6, height = 5)

# ── 8. Markers (optional) ──────────────────────────────────────────────────
if (opt$markers) {
  message("[8/9] findMarkers …")
  markers <- findMarkers(sce, groups = sce$cluster,
                          direction = "up", lfc = 0.25,
                          BPPARAM = MulticoreParam(workers = opt$threads))
  saveRDS(markers, sub("\\.rds$", "_markers.rds", opt$out))

  top10 <- lapply(markers, function(x) head(rownames(x), 10))
  message("  Top 10 markers per cluster — first cluster: ",
          paste(top10[[1]], collapse = ", "))

  # Marker heatmap
  all_top <- unique(unlist(top10))
  p_heat <- plotHeatmap(sce, features = all_top,
                          order_columns_by = "cluster", center = TRUE)
  ggplot2::ggsave(file.path(opt$`fig-dir`, "marker_heatmap.pdf"), p_heat,
                   width = 12, height = 14)
} else {
  message("[8/9] Skipping markers (--markers not set)")
}

# ── 9. Save ────────────────────────────────────────────────────────────────
message("[9/9] Saving → ", opt$out)
saveRDS(sce, opt$out)
message("Done.")
message("  Reload: sce <- readRDS('", opt$out, "')")
