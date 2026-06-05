#!/usr/bin/env Rscript
# build_archr.R — turnkey scATAC-seq pipeline through clustering.
#
# Reads a TAB-separated sample list, builds Arrow files, runs doublet inference,
# creates an ArchRProject, runs LSI → Harmony → clusters → UMAP, saves the project
# and diagnostic plots.
#
# Usage:
#   Rscript build_archr.R --samples samples.txt --genome hg38 --out ATAC_analysis
#
# samples.txt (TAB-separated, one per line):
#   donor1<TAB>/data/donor1/fragments.tsv.gz
#   donor2<TAB>/data/donor2/fragments.tsv.gz
#
# Required:
#   --samples    TAB-separated NAME<TAB>FRAGMENT_PATH per line
#   --genome     hg38 | mm10 | hg19
#   --out        Output directory (project will be created here)
#
# Optional:
#   --threads        Number of threads [default 16]
#   --filter-tss     TSS enrichment cutoff [default 4]
#   --filter-frags   Min fragments per cell [default 1000]
#   --no-harmony     Skip Harmony batch correction
#   --resolution     Leiden resolution [default 0.8]

suppressPackageStartupMessages({
  library(optparse)
  library(ArchR)
})

option_list <- list(
  make_option("--samples",      type = "character",                          help = "TAB-separated NAME<TAB>FRAGMENT_PATH file (required)"),
  make_option("--genome",       type = "character", default = "hg38",        help = "hg38 | mm10 | hg19 [%default]"),
  make_option("--out",          type = "character", default = "ATAC_analysis", help = "Output directory [%default]"),
  make_option("--threads",      type = "integer",   default = 16,            help = "Threads [%default]"),
  make_option("--filter-tss",   type = "double",    default = 4,             help = "TSS enrichment cutoff [%default]"),
  make_option("--filter-frags", type = "integer",   default = 1000,          help = "Min fragments per cell [%default]"),
  make_option("--no-harmony",   action = "store_true", default = FALSE,      help = "Skip Harmony batch correction"),
  make_option("--resolution",   type = "double",    default = 0.8,           help = "Cluster resolution [%default]")
)
opt <- parse_args(OptionParser(option_list = option_list))
if (is.null(opt$samples)) stop("--samples is required.")

# ── Parse the samples file ─────────────────────────────────────────────────
lines <- readLines(opt$samples)
lines <- lines[lines != "" & !startsWith(lines, "#")]
parts <- strsplit(lines, "\t")
sample_names <- sapply(parts, `[`, 1)
sample_paths <- sapply(parts, `[`, 2)
stopifnot(all(file.exists(sample_paths)))
inputFiles <- setNames(sample_paths, sample_names)
message("Found ", length(inputFiles), " samples: ", paste(sample_names, collapse = ", "))

# ── Setup ──────────────────────────────────────────────────────────────────
set.seed(1)
addArchRThreads(threads = opt$threads)
addArchRGenome(opt$genome)
dir.create(opt$out, recursive = TRUE, showWarnings = FALSE)
setwd(opt$out)

# ── 1. Arrow files ─────────────────────────────────────────────────────────
message("\n[1/6] Creating Arrow files (this is the slow step) …")
ArrowFiles <- createArrowFiles(
  inputFiles      = inputFiles,
  sampleNames     = names(inputFiles),
  filterTSS       = opt$`filter-tss`,
  filterFrags     = opt$`filter-frags`,
  addTileMat      = TRUE,
  addGeneScoreMat = TRUE,
  threads         = opt$threads
)

# ── 2. Doublet inference ───────────────────────────────────────────────────
message("\n[2/6] Inferring doublets …")
doubScores <- addDoubletScores(
  input     = ArrowFiles,
  k         = 10,
  knnMethod = "UMAP",
  LSIMethod = 1,
  threads   = opt$threads
)

# ── 3. Project + filter doublets ───────────────────────────────────────────
message("\n[3/6] Creating ArchRProject + filtering doublets …")
proj <- ArchRProject(
  ArrowFiles      = ArrowFiles,
  outputDirectory = ".",
  copyArrows      = TRUE,
  threads         = opt$threads
)
n_before <- nCells(proj)
proj <- filterDoublets(ArchRProj = proj)
message("  Cells: ", n_before, " → ", nCells(proj),
        " (", nCells(proj) - n_before, " doublets removed)")

# ── 4. Iterative LSI + (optional) Harmony ──────────────────────────────────
message("\n[4/6] Iterative LSI …")
proj <- addIterativeLSI(
  ArchRProj    = proj,
  useMatrix    = "TileMatrix",
  name         = "IterativeLSI",
  iterations   = 2,
  varFeatures  = 25000,
  dimsToUse    = 1:30,
  force        = TRUE
)

reducedDims <- "IterativeLSI"
if (!opt$`no-harmony` && length(inputFiles) > 1) {
  message("[4/6] Harmony batch correction …")
  proj <- addHarmony(
    ArchRProj   = proj,
    reducedDims = "IterativeLSI",
    name        = "Harmony",
    groupBy     = "Sample",
    force       = TRUE
  )
  reducedDims <- "Harmony"
}

# ── 5. Clusters + UMAP ─────────────────────────────────────────────────────
message("\n[5/6] Clusters + UMAP (rep = ", reducedDims, ") …")
proj <- addClusters(input = proj, reducedDims = reducedDims,
                     name = "Clusters", resolution = opt$resolution,
                     force = TRUE)
proj <- addUMAP(ArchRProj = proj, reducedDims = reducedDims,
                 name = "UMAP", force = TRUE)
message("  Clusters found: ", length(unique(proj$Clusters)))

# ── 6. Plots + save ────────────────────────────────────────────────────────
message("\n[6/6] Plots + save …")
p1 <- plotEmbedding(ArchRProj = proj, colorBy = "cellColData",
                     name = "Sample", embedding = "UMAP")
p2 <- plotEmbedding(ArchRProj = proj, colorBy = "cellColData",
                     name = "Clusters", embedding = "UMAP")
plotPDF(p1, p2, name = "UMAP-Sample-Clusters.pdf",
        ArchRProj = proj, addDOC = FALSE, width = 5, height = 5)

proj <- saveArchRProject(ArchRProj = proj)
message("\nDone. Project saved to: ", getOutputDirectory(proj))
message("Reopen later with: proj <- loadArchRProject(path = '", getOutputDirectory(proj), "')")
