#!/usr/bin/env Rscript
# archr_template.R — End-to-end ArchR pipeline.
#
# Edit the CONFIGURATION block, then run end-to-end:
#   1. Build Arrow files + ArchRProject
#   2. Doublet filter
#   3. LSI + Harmony + clusters + UMAP
#   4. Gene scores + plots
#   5. MACS2 peak calling + reproducible peak set
#   6. Marker peaks + motif enrichment + chromVAR
#   7. (Optional) scRNA-seq label transfer
#   8. Save augmented project

suppressPackageStartupMessages({
  library(ArchR)
})

# ============================================================================
# CONFIGURATION — edit these
# ============================================================================

# Inputs — TAB-separated list of (name, fragment_path)
SAMPLES <- list(
  list(name = "donor1", path = "/data/donor1/fragments.tsv.gz"),
  list(name = "donor2", path = "/data/donor2/fragments.tsv.gz")
)

GENOME           <- "hg38"           # hg38 | mm10 | hg19
OUTPUT_DIR       <- "ATAC_analysis"
THREADS          <- 16

# QC
FILTER_TSS       <- 4
FILTER_FRAGS     <- 1000

# Dimensionality reduction + clustering
RESOLUTION       <- 0.8
USE_HARMONY      <- TRUE
MOTIF_SET        <- "cisbp"          # cisbp | JASPAR2020 | homer
SPECIES          <- "Homo sapiens"

# Marker genes to plot on the UMAP (gene scores + imputed)
MARKER_GENES     <- c("CD3D", "CD3E", "MS4A1", "CD14", "LYZ", "NKG7", "FCGR3A")

# Optional — scRNA-seq label transfer
RNA_RDS              <- NULL                  # set to "rna_ref.rds" to enable
RNA_LABEL_COL        <- "cell_type"

# ============================================================================
# SETUP
# ============================================================================

set.seed(1)
addArchRThreads(threads = THREADS)
addArchRGenome(GENOME)

dir.create(OUTPUT_DIR, recursive = TRUE, showWarnings = FALSE)
setwd(OUTPUT_DIR)

inputFiles <- setNames(
  sapply(SAMPLES, function(s) s$path),
  sapply(SAMPLES, function(s) s$name)
)
stopifnot(all(file.exists(inputFiles)))

# ============================================================================
# 1. ARROW FILES + DOUBLETS
# ============================================================================

message("\n=== 1. Arrow files + doublet scores ===")
ArrowFiles <- createArrowFiles(
  inputFiles      = inputFiles,
  sampleNames     = names(inputFiles),
  filterTSS       = FILTER_TSS,
  filterFrags     = FILTER_FRAGS,
  addTileMat      = TRUE,
  addGeneScoreMat = TRUE,
  threads         = THREADS
)
doubScores <- addDoubletScores(
  input = ArrowFiles, k = 10, knnMethod = "UMAP", LSIMethod = 1, threads = THREADS
)

# ============================================================================
# 2. ARCHR PROJECT + FILTER DOUBLETS
# ============================================================================

message("\n=== 2. ArchRProject ===")
proj <- ArchRProject(
  ArrowFiles      = ArrowFiles,
  outputDirectory = ".",
  copyArrows      = TRUE,
  threads         = THREADS
)
n_before <- nCells(proj)
proj <- filterDoublets(ArchRProj = proj)
message("  Cells: ", n_before, " → ", nCells(proj))

# ============================================================================
# 3. LSI (+ HARMONY) + CLUSTERS + UMAP
# ============================================================================

message("\n=== 3. LSI + UMAP + clusters ===")
proj <- addIterativeLSI(
  ArchRProj  = proj, useMatrix = "TileMatrix", name = "IterativeLSI",
  iterations = 2, varFeatures = 25000, dimsToUse = 1:30, force = TRUE
)

reducedDims <- "IterativeLSI"
if (USE_HARMONY && length(inputFiles) > 1) {
  proj <- addHarmony(
    ArchRProj = proj, reducedDims = "IterativeLSI", name = "Harmony",
    groupBy = "Sample", force = TRUE
  )
  reducedDims <- "Harmony"
}
proj <- addClusters(input = proj, reducedDims = reducedDims, name = "Clusters",
                     resolution = RESOLUTION, force = TRUE)
proj <- addUMAP(ArchRProj = proj, reducedDims = reducedDims, name = "UMAP", force = TRUE)

p1 <- plotEmbedding(ArchRProj = proj, colorBy = "cellColData",
                     name = "Sample", embedding = "UMAP")
p2 <- plotEmbedding(ArchRProj = proj, colorBy = "cellColData",
                     name = "Clusters", embedding = "UMAP")
plotPDF(p1, p2, name = "UMAP-Sample-Clusters.pdf",
        ArchRProj = proj, addDOC = FALSE, width = 5, height = 5)

# ============================================================================
# 4. GENE SCORES + MARKER PLOTS
# ============================================================================

message("\n=== 4. Gene scores + marker plots ===")
proj <- addImputeWeights(proj)
p <- plotEmbedding(
  ArchRProj     = proj, colorBy = "GeneScoreMatrix",
  name          = MARKER_GENES,
  embedding     = "UMAP",
  imputeWeights = getImputeWeights(proj)
)
plotPDF(plotList = p, name = "UMAP-Marker-Genes-Imputed.pdf",
        ArchRProj = proj, addDOC = FALSE, width = 5, height = 5)

# ============================================================================
# 5. PEAK CALLING (MACS2)
# ============================================================================

message("\n=== 5. Peak calling ===")
proj <- addGroupCoverages(ArchRProj = proj, groupBy = "Clusters", force = TRUE)
proj <- addReproduciblePeakSet(
  ArchRProj   = proj, groupBy = "Clusters",
  pathToMacs2 = findMacs2(), threads = THREADS, force = TRUE
)
proj <- addPeakMatrix(ArchRProj = proj, threads = THREADS, force = TRUE)

# ============================================================================
# 6. MARKER PEAKS + MOTIFS + CHROMVAR
# ============================================================================

message("\n=== 6. Marker peaks + motifs + chromVAR ===")
markersPeaks <- getMarkerFeatures(
  ArchRProj  = proj, useMatrix = "PeakMatrix", groupBy = "Clusters",
  bias       = c("TSSEnrichment", "log10(nFrags)"),
  testMethod = "wilcoxon", threads = THREADS
)
saveRDS(markersPeaks, "markersPeaks.rds")

heatmapPeaks <- markerHeatmap(seMarker = markersPeaks,
                               cutOff = "FDR <= 0.01 & Log2FC >= 1", transpose = TRUE)
plotPDF(heatmapPeaks, name = "Peak-Marker-Heatmap.pdf",
        ArchRProj = proj, addDOC = FALSE, width = 8, height = 6)

proj <- addMotifAnnotations(ArchRProj = proj, motifSet = MOTIF_SET, name = "Motif",
                             species = SPECIES, force = TRUE)
enrichMotifs <- peakAnnoEnrichment(seMarker = markersPeaks, ArchRProj = proj,
                                    peakAnnotation = "Motif",
                                    cutOff = "FDR <= 0.01 & Log2FC >= 1")
heatmapEM <- plotEnrichHeatmap(enrichMotifs, n = 7, transpose = TRUE)
plotPDF(heatmapEM, name = "Motifs-Enriched-Heatmap.pdf",
        ArchRProj = proj, addDOC = FALSE, width = 8, height = 6)

proj <- addBgdPeaks(proj, force = TRUE)
proj <- addDeviationsMatrix(ArchRProj = proj, peakAnnotation = "Motif",
                             matrixName = "MotifMatrix", force = TRUE,
                             threads = THREADS)
plotVarDev <- getVarDeviations(proj, plot = TRUE, name = "MotifMatrix")
plotPDF(plotVarDev, name = "Variable-Motif-Deviations.pdf",
        ArchRProj = proj, addDOC = FALSE, width = 8, height = 6)

# ============================================================================
# 7. (OPTIONAL) scRNA-seq LABEL TRANSFER
# ============================================================================

if (!is.null(RNA_RDS) && file.exists(RNA_RDS)) {
  message("\n=== 7. scRNA-seq label transfer ===")
  seRNA <- readRDS(RNA_RDS)
  proj <- addGeneIntegrationMatrix(
    ArchRProj   = proj, useMatrix = "GeneScoreMatrix",
    matrixName  = "GeneIntegrationMatrix",
    reducedDims = reducedDims,
    seRNA       = seRNA, addToArrow = TRUE,
    groupRNA    = RNA_LABEL_COL,
    nameCell    = "predictedCell", nameGroup = "predictedGroup",
    nameScore   = "predictedScore", threads = THREADS, force = TRUE
  )
  p <- plotEmbedding(ArchRProj = proj, colorBy = "cellColData",
                      name = "predictedGroup", embedding = "UMAP")
  plotPDF(p, name = "UMAP-Predicted-CellType.pdf",
          ArchRProj = proj, addDOC = FALSE, width = 6, height = 5)
}

# ============================================================================
# 8. SAVE
# ============================================================================

proj <- saveArchRProject(ArchRProj = proj)
message("\nDone. Project saved to ", getOutputDirectory(proj))
