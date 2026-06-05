#!/usr/bin/env Rscript
# downstream_archr.R — peaks + motifs + chromVAR + (optional) scRNA integration.
#
# Reads a project built by build_archr.R, runs peak calling + motif enrichment +
# chromVAR deviations, and optionally transfers labels from an scRNA-seq Seurat
# object.
#
# Usage:
#   # Full downstream pipeline
#   Rscript downstream_archr.R --project ATAC_analysis --species "Homo sapiens"
#
#   # + label transfer from an annotated Seurat object
#   Rscript downstream_archr.R --project ATAC_analysis --rna-rds rna_ref.rds \
#                               --rna-label-col cell_type

suppressPackageStartupMessages({
  library(optparse)
  library(ArchR)
})

option_list <- list(
  make_option("--project",         type = "character",                       help = "ArchR project directory (required)"),
  make_option("--threads",         type = "integer",   default = 16,         help = "Threads [%default]"),
  make_option("--species",         type = "character", default = "Homo sapiens", help = "For motif annotations [%default]"),
  make_option("--motif-set",       type = "character", default = "cisbp",    help = "cisbp | JASPAR2020 | homer [%default]"),
  make_option("--group-by",        type = "character", default = "Clusters", help = "Grouping for peak calling [%default]"),
  make_option("--skip-peaks",      action = "store_true", default = FALSE,   help = "Skip peak calling (use existing)"),
  make_option("--skip-motifs",     action = "store_true", default = FALSE,   help = "Skip motif analysis"),
  make_option("--skip-chromvar",   action = "store_true", default = FALSE,   help = "Skip chromVAR deviations"),
  make_option("--rna-rds",         type = "character", default = NULL,       help = "Optional: scRNA-seq Seurat .rds for label transfer"),
  make_option("--rna-label-col",   type = "character", default = "cell_type", help = "Cell-type column in --rna-rds metadata [%default]")
)
opt <- parse_args(OptionParser(option_list = option_list))
if (is.null(opt$project)) stop("--project is required.")

set.seed(1)
addArchRThreads(threads = opt$threads)

message("Loading project from ", opt$project, " …")
proj <- loadArchRProject(path = opt$project)
addArchRGenome(getArchRGenome(proj))

# ── 1. Peak calling ────────────────────────────────────────────────────────
if (!opt$`skip-peaks`) {
  message("\n[1/4] Peak calling …")
  proj <- addGroupCoverages(ArchRProj = proj, groupBy = opt$`group-by`, force = TRUE)
  proj <- addReproduciblePeakSet(
    ArchRProj      = proj,
    groupBy        = opt$`group-by`,
    pathToMacs2    = findMacs2(),
    threads        = opt$threads,
    force          = TRUE
  )
  proj <- addPeakMatrix(ArchRProj = proj, threads = opt$threads, force = TRUE)
  message("  Available matrices: ",
          paste(getAvailableMatrices(proj), collapse = ", "))
} else {
  message("[1/4] Skipping peak calling (--skip-peaks)")
}

# ── 2. Marker peaks ────────────────────────────────────────────────────────
message("\n[2/4] Marker peaks …")
markersPeaks <- getMarkerFeatures(
  ArchRProj   = proj,
  useMatrix   = "PeakMatrix",
  groupBy     = opt$`group-by`,
  bias        = c("TSSEnrichment", "log10(nFrags)"),
  testMethod  = "wilcoxon",
  threads     = opt$threads
)
saveRDS(markersPeaks, file.path(getOutputDirectory(proj), "markersPeaks.rds"))

heatmapPeaks <- markerHeatmap(
  seMarker  = markersPeaks,
  cutOff    = "FDR <= 0.01 & Log2FC >= 1",
  transpose = TRUE
)
plotPDF(heatmapPeaks, name = "Peak-Marker-Heatmap.pdf",
        ArchRProj = proj, addDOC = FALSE, width = 8, height = 6)

# ── 3. Motif enrichment ────────────────────────────────────────────────────
if (!opt$`skip-motifs`) {
  message("\n[3/4] Motif enrichment …")
  proj <- addMotifAnnotations(
    ArchRProj = proj,
    motifSet  = opt$`motif-set`,
    name      = "Motif",
    species   = opt$species,
    force     = TRUE
  )

  enrichMotifs <- peakAnnoEnrichment(
    seMarker       = markersPeaks,
    ArchRProj      = proj,
    peakAnnotation = "Motif",
    cutOff         = "FDR <= 0.01 & Log2FC >= 1"
  )
  saveRDS(enrichMotifs, file.path(getOutputDirectory(proj), "enrichMotifs.rds"))

  heatmapEM <- plotEnrichHeatmap(enrichMotifs, n = 7, transpose = TRUE)
  plotPDF(heatmapEM, name = "Motifs-Enriched-Heatmap.pdf",
          ArchRProj = proj, addDOC = FALSE, width = 8, height = 6)
} else {
  message("[3/4] Skipping motif enrichment (--skip-motifs)")
}

# ── 4. chromVAR deviations ─────────────────────────────────────────────────
if (!opt$`skip-chromvar`) {
  message("\n[4/4] chromVAR deviations …")
  proj <- addBgdPeaks(proj, force = TRUE)
  proj <- addDeviationsMatrix(
    ArchRProj      = proj,
    peakAnnotation = "Motif",
    matrixName     = "MotifMatrix",
    force          = TRUE,
    threads        = opt$threads
  )

  plotVarDev <- getVarDeviations(proj, plot = TRUE, name = "MotifMatrix")
  plotPDF(plotVarDev, name = "Variable-Motif-Deviations.pdf",
          ArchRProj = proj, addDOC = FALSE, width = 8, height = 6)
} else {
  message("[4/4] Skipping chromVAR (--skip-chromvar)")
}

# ── (Optional) scRNA-seq label transfer ────────────────────────────────────
if (!is.null(opt$`rna-rds`)) {
  message("\n[+] scRNA-seq label transfer from ", opt$`rna-rds`, " …")
  seRNA <- readRDS(opt$`rna-rds`)
  if (!opt$`rna-label-col` %in% colnames(seRNA@meta.data))
    stop("--rna-label-col '", opt$`rna-label-col`, "' not in seRNA@meta.data")

  proj <- addGeneIntegrationMatrix(
    ArchRProj         = proj,
    useMatrix         = "GeneScoreMatrix",
    matrixName        = "GeneIntegrationMatrix",
    reducedDims       = if ("Harmony" %in% getReducedDims(proj)) "Harmony" else "IterativeLSI",
    seRNA             = seRNA,
    addToArrow        = TRUE,
    groupRNA          = opt$`rna-label-col`,
    nameCell          = "predictedCell",
    nameGroup         = "predictedGroup",
    nameScore         = "predictedScore",
    threads           = opt$threads,
    force             = TRUE
  )

  p <- plotEmbedding(
    ArchRProj = proj,
    colorBy   = "cellColData",
    name      = "predictedGroup",
    embedding = "UMAP"
  )
  plotPDF(p, name = "UMAP-Predicted-CellType.pdf",
          ArchRProj = proj, addDOC = FALSE, width = 6, height = 5)

  # Per-cluster mean confidence
  conf <- aggregate(getCellColData(proj)$predictedScore,
                     by = list(Cluster = getCellColData(proj)$Clusters),
                     FUN = mean)
  message("\nPer-cluster mean prediction confidence:")
  print(conf)
} else {
  message("\n[+] No --rna-rds provided — skipping label transfer.")
}

# ── Save ───────────────────────────────────────────────────────────────────
proj <- saveArchRProject(ArchRProj = proj)
message("\nDone. Outputs in ", getOutputDirectory(proj))
