#!/usr/bin/env Rscript
# integrate_seurat.R — multi-sample Seurat v5 integration via IntegrateLayers.
#
# Reads a TAB-separated samples list, builds a unified Seurat object with
# per-sample layers, runs preprocessing + IntegrateLayers + clustering + UMAP,
# writes the integrated .rds.
#
# Usage:
#   Rscript integrate_seurat.R --samples samples.txt --method harmony --out integrated.rds
#
# samples.txt (TAB-separated):
#   donor1<TAB>/data/donor1/filtered_feature_bc_matrix
#   donor2<TAB>/data/donor2/filtered_feature_bc_matrix.h5
#   donor3<TAB>/data/donor3.rds
#
# Method options:
#   harmony | cca | rpca | fastmnn | scvi

suppressPackageStartupMessages({
  library(optparse)
  library(Seurat)
})

option_list <- list(
  make_option("--samples",   type = "character",                       help = "TAB-separated samples list (required)"),
  make_option("--out",       type = "character", default = "integrated.rds", help = "Output .rds [%default]"),
  make_option("--method",    type = "character", default = "harmony",  help = "harmony | cca | rpca | fastmnn | scvi [%default]"),
  make_option("--sct",       action = "store_true", default = FALSE,   help = "Use SCTransform normalization"),
  make_option("--dims",      type = "integer",   default = 30,         help = "PCs to use [%default]"),
  make_option("--resolution", type = "double",   default = 1.0,        help = "Cluster resolution [%default]"),
  make_option("--nfeatures", type = "integer",   default = 2000,       help = "Variable features [%default]"),
  make_option("--fig-dir",   type = "character", default = "figures")
)
opt <- parse_args(OptionParser(option_list = option_list))
if (is.null(opt$samples)) stop("--samples is required")
set.seed(1)
dir.create(opt$`fig-dir`, recursive = TRUE, showWarnings = FALSE)

# ── Parse the samples file ─────────────────────────────────────────────────
lines <- readLines(opt$samples)
lines <- lines[lines != "" & !startsWith(lines, "#")]
parts <- strsplit(lines, "\t")
sample_names <- sapply(parts, `[`, 1)
sample_paths <- sapply(parts, `[`, 2)
message("Found ", length(sample_names), " samples: ",
        paste(sample_names, collapse = ", "))

# ── Load each sample as a Seurat object ────────────────────────────────────
load_one <- function(path) {
  if (dir.exists(path))           counts <- Read10X(path)
  else if (endsWith(path, ".h5")) counts <- Read10X_h5(path)
  else if (endsWith(path, ".rds")) {
    o <- readRDS(path)
    if (inherits(o, "Seurat")) return(o)
    counts <- as.matrix(o)
  } else {
    stop("Unsupported input: ", path)
  }
  CreateSeuratObject(counts = counts, min.cells = 3, min.features = 200)
}

per_sample <- lapply(sample_paths, load_one)
names(per_sample) <- sample_names

# Tag each sample's cells then merge
for (i in seq_along(per_sample)) {
  per_sample[[i]]$sample <- sample_names[i]
}

obj <- merge(per_sample[[1]], y = per_sample[-1],
             add.cell.ids = sample_names,
             project = "integrated")
message("Merged: ", ncol(obj), " cells × ", nrow(obj), " genes")

# Basic QC (per-sample column will help track downstream)
obj[["percent.mt"]] <- PercentageFeatureSet(obj, pattern = "^MT-")
obj <- subset(obj, subset = nFeature_RNA > 200 & percent.mt < 20)

# ── Split RNA assay into per-sample layers ────────────────────────────────
obj[["RNA"]] <- split(obj[["RNA"]], f = obj$sample)
message("Layers: ", paste(Layers(obj), collapse = ", "))

# ── Preprocessing on the layered object ───────────────────────────────────
if (opt$sct) {
  message("SCTransform per layer …")
  obj <- SCTransform(obj, verbose = FALSE)
} else {
  message("NormalizeData + FindVariableFeatures + ScaleData per layer …")
  obj <- NormalizeData(obj)
  obj <- FindVariableFeatures(obj, nfeatures = opt$nfeatures)
  obj <- ScaleData(obj)
}
obj <- RunPCA(obj)

# Plot unintegrated UMAP for the "before" picture
obj <- RunUMAP(obj, dims = 1:opt$dims, reduction = "pca",
                reduction.name = "umap.unintegrated")
ggplot2::ggsave(file.path(opt$`fig-dir`, "umap_before.pdf"),
                 DimPlot(obj, reduction = "umap.unintegrated", group.by = "sample"),
                 width = 7, height = 5)

# ── IntegrateLayers — pick a method ───────────────────────────────────────
method_map <- list(
  cca     = list(fn = CCAIntegration,     new_rep = "integrated.cca"),
  rpca    = list(fn = RPCAIntegration,    new_rep = "integrated.rpca"),
  harmony = list(fn = HarmonyIntegration, new_rep = "harmony"),
  fastmnn = list(fn = FastMNNIntegration, new_rep = "integrated.mnn"),
  scvi    = list(fn = scVIIntegration,    new_rep = "integrated.scvi")
)
m <- method_map[[tolower(opt$method)]]
if (is.null(m)) stop("Unknown --method: ", opt$method)

message("IntegrateLayers (method = ", opt$method, ") …")
extra_args <- list()
if (opt$sct) extra_args$normalization.method <- "SCT"

obj <- do.call(IntegrateLayers, c(list(
  object = obj,
  method = m$fn,
  orig.reduction = "pca",
  new.reduction = m$new_rep,
  verbose = FALSE
), extra_args))

# ── Re-cluster + UMAP on the integrated embedding ─────────────────────────
message("FindNeighbors + FindClusters + RunUMAP on '", m$new_rep, "' …")
obj <- FindNeighbors(obj, reduction = m$new_rep, dims = 1:opt$dims)
obj <- FindClusters(obj, resolution = opt$resolution)
obj <- RunUMAP(obj, reduction = m$new_rep, dims = 1:opt$dims)

# Plot integrated UMAPs
ggplot2::ggsave(file.path(opt$`fig-dir`, "umap_after_sample.pdf"),
                 DimPlot(obj, group.by = "sample") +
                   ggplot2::ggtitle(paste("After", opt$method)),
                 width = 7, height = 5)
ggplot2::ggsave(file.path(opt$`fig-dir`, "umap_after_clusters.pdf"),
                 DimPlot(obj, group.by = "seurat_clusters", label = TRUE) + NoLegend(),
                 width = 6, height = 5)

# ── JoinLayers for downstream DE ──────────────────────────────────────────
message("Rejoining layers …")
obj[["RNA"]] <- JoinLayers(obj[["RNA"]])

# For SCT, prepare for DE
if (opt$sct) {
  obj <- PrepSCTFindMarkers(obj)
}

# ── Save ──────────────────────────────────────────────────────────────────
message("Writing ", opt$out, " …")
saveRDS(obj, opt$out)
message("Done.")
message("  Clusters: ", length(unique(obj$seurat_clusters)))
message("  Use Reduction '", m$new_rep, "' for downstream analyses")
