---
name: archr
description: Single-cell ATAC-seq analysis with ArchR (R). The mature R-based scATAC pipeline — Arrow files, doublet inference, iterative LSI + Harmony, clustering, gene scores, MACS2 peak calling, motif enrichment, chromVAR deviations, footprinting, scRNA-seq integration (label transfer), trajectory analysis (built-in + Monocle3 + Slingshot), and ArchR's interactive genome browser. Sister protocol to snapatac2 — pick this for established R workflows.
license: MIT
metadata:
---

# ArchR: scATAC-seq Analysis in R

## Overview

[ArchR](https://www.archrproject.com/) is the mature R-based pipeline for scATAC-seq analysis. It uses an on-disk **Arrow file** format (HDF5-based, indexed by barcode) so it scales to hundreds of thousands of cells on modest hardware. The `ArchRProject` object is a thin reference to one or more Arrow files plus all derived data (matrices, embeddings, peaks, motif enrichments).

The full pipeline branches like this:

```
Fragments / BAM
   ↓
Arrow files  ←  doublet inference (LSI-based, knn-method)
   ↓
ArchRProject
   ↓
Iterative LSI (+ Harmony) → clusters → UMAP / tSNE
   ↓
Gene scores (TileMatrix + GeneScoreMatrix) → marker genes → label transfer from scRNA-seq
   ↓
Pseudo-bulk replicates → MACS2 peak calling per cluster → reproducible peak set → PeakMatrix
   ↓
Marker peaks → motif enrichment → chromVAR deviations → TF footprinting
   ↓
Trajectories (built-in / Monocle3 / Slingshot), Peak2GeneLinkage, positive TF regulators
```

ArchR overlaps heavily with [SnapATAC2](../snapatac2/SKILL.md) in scope. **When to choose ArchR**: established R-based labs, downstream ecosystem in R (Seurat, Bioconductor), need for built-in motif / chromVAR / footprinting in one package. **When to choose SnapATAC2**: Python ecosystem, atlas-scale (millions of cells), tighter scverse integration.

## When to Use This Skill

- Standard scATAC analysis: fragment → cells → clusters → peaks
- Integrating scATAC with scRNA-seq via cross-platform CCA (`addGeneIntegrationMatrix`)
- Motif enrichment + chromVAR deviations + TF footprinting all in one place
- Trajectory analysis on chromatin accessibility (built-in or Monocle3/Slingshot)
- Multiome (paired ATAC + RNA) analysis
- Bulk ATAC projection onto a single-cell embedding

**Not for**: extremely large atlases (≥ 1M cells — ArchR is heavy here; switch to SnapATAC2); CUT&Tag / CUT&RUN (use specialised tools); spatial ATAC (use spatial-transcriptomics protocol with ATAC adaptations).

## Prerequisites

- R ≥ 4.0 (4.2+ recommended)
- Bioconductor + many deps; budget for a long initial install
- ~64 GB RAM recommended for ~100k cells; ~256 GB for ~500k+
- Linux/macOS strongly preferred (Windows works for analysis but Arrow-file creation is slow)

```r
# Install from CRAN/BioC + ArchR's installer
install.packages("devtools")
devtools::install_github("GreenleafLab/ArchR", ref = "master", repos = BiocManager::repositories())
library(ArchR)
ArchR::installExtraPackages()      # MACS2, chromVAR, motifmatchr, etc.
```

For multiomic ATAC+RNA: also need `Seurat` and a working `cellranger-arc` (or 10X multiome) preprocessing pipeline.

## Quick Start — Full Pipeline

```r
library(ArchR)
set.seed(1)
addArchRThreads(threads = 16)        # adjust to your machine
addArchRGenome("hg38")                # or "mm10", "hg19"

# ── 1. Create Arrow files from fragment files ───────────────────────────────
# Fragment files: typically cellranger-atac output (fragments.tsv.gz + .tbi)
inputFiles <- c(
  "donor1" = "/data/donor1/fragments.tsv.gz",
  "donor2" = "/data/donor2/fragments.tsv.gz"
)

ArrowFiles <- createArrowFiles(
  inputFiles      = inputFiles,
  sampleNames     = names(inputFiles),
  filterTSS       = 4,               # TSS enrichment cutoff — raise for stringent QC
  filterFrags     = 1000,            # min fragments per cell
  addTileMat      = TRUE,            # build the bin matrix (default 500 bp)
  addGeneScoreMat = TRUE,            # build gene scores from accessibility
  threads         = 16
)

# ── 2. Doublet detection ────────────────────────────────────────────────────
doubScores <- addDoubletScores(
  input      = ArrowFiles,
  k          = 10,                   # KNN for doublet score
  knnMethod  = "UMAP",
  LSIMethod  = 1
)

# ── 3. Create the project ───────────────────────────────────────────────────
proj <- ArchRProject(
  ArrowFiles      = ArrowFiles,
  outputDirectory = "ATAC_analysis",
  copyArrows      = TRUE             # copies arrows into outputDirectory — safe
)
proj <- filterDoublets(ArchRProj = proj)

# Inspect what matrices exist
getAvailableMatrices(proj)
# "TileMatrix", "GeneScoreMatrix"

# ── 4. Dimensionality reduction + clustering ────────────────────────────────
proj <- addIterativeLSI(
  ArchRProj  = proj,
  useMatrix  = "TileMatrix",
  name       = "IterativeLSI",
  iterations = 2,
  clusterParams = list(resolution = c(0.2), sampleCells = 10000, n.start = 10),
  varFeatures = 25000,
  dimsToUse   = 1:30
)

# (Multi-sample: add Harmony batch correction)
proj <- addHarmony(
  ArchRProj   = proj,
  reducedDims = "IterativeLSI",
  name        = "Harmony",
  groupBy     = "Sample"
)

proj <- addClusters(input = proj, reducedDims = "Harmony", name = "Clusters",
                     resolution = 0.8)
proj <- addUMAP(ArchRProj = proj, reducedDims = "Harmony", name = "UMAP")

# Save & visualize
proj <- saveArchRProject(ArchRProj = proj)
p1 <- plotEmbedding(ArchRProj = proj, colorBy = "cellColData",
                     name = "Sample",   embedding = "UMAP")
p2 <- plotEmbedding(ArchRProj = proj, colorBy = "cellColData",
                     name = "Clusters", embedding = "UMAP")
plotPDF(p1, p2, name = "UMAP-Sample-Clusters.pdf",
        ArchRProj = proj, addDOC = FALSE, width = 5, height = 5)

# ── 5. Gene scores + marker genes ───────────────────────────────────────────
proj <- addImputeWeights(proj)                  # MAGIC-style smoothing for plots

markerGenes <- c("CD3D", "CD3E", "MS4A1", "CD14", "LYZ", "NKG7", "FCGR3A")
p <- plotEmbedding(
  ArchRProj      = proj,
  colorBy        = "GeneScoreMatrix",
  name           = markerGenes,
  embedding      = "UMAP",
  imputeWeights  = getImputeWeights(proj)
)
plotPDF(plotList = p, name = "UMAP-Marker-Genes-Imputed.pdf",
        ArchRProj = proj, addDOC = FALSE, width = 5, height = 5)

# ── 6. Browser-style track plots ────────────────────────────────────────────
p <- plotBrowserTrack(
  ArchRProj  = proj,
  groupBy    = "Clusters",
  geneSymbol = markerGenes,
  upstream   = 50000,
  downstream = 50000
)
plotPDF(plotList = p, name = "Tracks-Marker-Genes.pdf",
        ArchRProj = proj, addDOC = FALSE, width = 5, height = 5)

# Interactive (for exploratory sessions only)
# ArchRBrowser(ArchRProj = proj)

# ── 7. Peak calling (requires MACS2) ────────────────────────────────────────
# Pseudo-bulk replicates per cluster so MACS2 has enough reads
proj <- addGroupCoverages(ArchRProj = proj, groupBy = "Clusters")

# Reproducible peak set across replicates
proj <- addReproduciblePeakSet(
  ArchRProj = proj,
  groupBy   = "Clusters",
  pathToMacs2 = findMacs2(),
  threads   = 16
)

# Add peak matrix (cells × peaks)
proj <- addPeakMatrix(ArchRProj = proj)
getAvailableMatrices(proj)
# Now includes "PeakMatrix"

# ── 8. Marker peaks per cluster ─────────────────────────────────────────────
markersPeaks <- getMarkerFeatures(
  ArchRProj   = proj,
  useMatrix   = "PeakMatrix",
  groupBy     = "Clusters",
  bias        = c("TSSEnrichment", "log10(nFrags)"),
  testMethod  = "wilcoxon"
)

# Extract significant marker peaks per cluster
markerList <- getMarkers(markersPeaks, cutOff = "FDR <= 0.01 & Log2FC >= 1")
markerList$C1   # top peaks for cluster C1

# ── 9. Motif enrichment ─────────────────────────────────────────────────────
proj <- addMotifAnnotations(
  ArchRProj = proj,
  motifSet  = "cisbp",                          # or "JASPAR2020", "homer"
  name      = "Motif"
)

enrichMotifs <- peakAnnoEnrichment(
  seMarker  = markersPeaks,
  ArchRProj = proj,
  peakAnnotation = "Motif",
  cutOff    = "FDR <= 0.01 & Log2FC >= 1"
)

# Heatmap of motif enrichment across clusters
heatmapEM <- plotEnrichHeatmap(enrichMotifs, n = 7, transpose = TRUE)
plotPDF(heatmapEM, name = "Motifs-Enriched-Marker-Heatmap.pdf",
        ArchRProj = proj, addDOC = FALSE, width = 8, height = 6)

# ── 10. chromVAR deviations ─────────────────────────────────────────────────
# Per-cell motif activity scores
proj <- addBgdPeaks(proj)
proj <- addDeviationsMatrix(
  ArchRProj      = proj,
  peakAnnotation = "Motif",
  force          = TRUE
)

# Plot per-cell deviation z-scores for top TFs
p <- plotEmbedding(
  ArchRProj    = proj,
  colorBy      = "MotifMatrix",
  name         = c("z:GATA1_1", "z:CEBPA_4", "z:PAX5_1"),
  embedding    = "UMAP",
  imputeWeights = getImputeWeights(proj)
)
plotPDF(plotList = p, name = "TF-Deviation-Scores.pdf",
        ArchRProj = proj, addDOC = FALSE, width = 5, height = 5)

# Save
proj <- saveArchRProject(ArchRProj = proj)
```

Convenience: `Rscript scripts/build_archr.R --fragments samples.txt --genome hg38 --out ATAC_analysis`. See [references/peaks_motifs.md](references/peaks_motifs.md) for peak / motif / footprinting depth; [references/integration.md](references/integration.md) for scRNA-seq integration + multiome + trajectories.

---

## scRNA-seq Integration

If you have a paired or unpaired scRNA-seq dataset with cell-type labels, ArchR can transfer those labels to the ATAC cells using Seurat's CCA.

```r
# Load the scRNA-seq Seurat object
seRNA <- readRDS("rna_reference.rds")    # must contain cell type labels in @meta.data$cell_type

# Cross-platform alignment + label transfer
proj <- addGeneIntegrationMatrix(
  ArchRProj         = proj,
  useMatrix         = "GeneScoreMatrix",
  matrixName        = "GeneIntegrationMatrix",
  reducedDims       = "Harmony",
  seRNA             = seRNA,
  addToArrow        = TRUE,
  groupRNA          = "cell_type",
  nameCell          = "predictedCell",
  nameGroup         = "predictedGroup",
  nameScore         = "predictedScore",
  threads           = 16
)

# Plot the predicted cell types on UMAP
p <- plotEmbedding(
  ArchRProj = proj,
  colorBy   = "cellColData",
  name      = "predictedGroup",
  embedding = "UMAP"
)
plotPDF(p, name = "UMAP-Predicted-CellType.pdf",
        ArchRProj = proj, addDOC = FALSE, width = 6, height = 5)
```

`addGeneIntegrationMatrix` writes the integrated matrix into the Arrow files (`addToArrow = TRUE`) so it persists across sessions.

---

## Multiome (paired ATAC + RNA)

If using 10X multiome (one assay per cell), ArchR handles both modalities through the same project:

```r
proj <- ArchRProject(
  ArrowFiles      = ArrowFiles,         # from multiome ATAC fragments
  outputDirectory = "Multiome_analysis"
)

# Add the RNA matrix from cellranger-arc output
rna_se <- import10xFeatureMatrix(
  input = "filtered_feature_bc_matrix.h5",
  names = "donor1"
)
proj <- addGeneExpressionMatrix(
  input  = proj,
  seRNA  = rna_se,
  threads = 16
)

# Joint LSI: combine ATAC + RNA into one embedding
proj <- addIterativeLSI(
  ArchRProj  = proj,
  useMatrix  = "GeneExpressionMatrix",
  name       = "LSI_RNA",
  varFeatures = 2500,
  dimsToUse  = 1:30
)
proj <- addCombinedDims(
  proj,
  reducedDims = c("IterativeLSI", "LSI_RNA"),
  name        = "LSI_Combined"
)
proj <- addUMAP(proj, reducedDims = "LSI_Combined", name = "UMAP_Combined")
proj <- addClusters(proj, reducedDims = "LSI_Combined", name = "JointClusters",
                     resolution = 0.8)
```

---

## Trajectory Analysis

ArchR's built-in trajectory implementation, taking a list of cluster labels in order:

```r
trajectory <- c("Naive_T", "Memory_T", "Effector_T")   # cluster identities along the path
proj <- addTrajectory(
  ArchRProj  = proj,
  name       = "T_cell_trajectory",
  groupBy    = "Clusters",
  trajectory = trajectory,
  embedding  = "UMAP"
)

# Visualize
p <- plotTrajectory(
  ArchRProj = proj,
  trajectory = "T_cell_trajectory",
  colorBy    = "cellColData",
  name       = "T_cell_trajectory",
  embedding  = "UMAP"
)
plotPDF(p, name = "Trajectory.pdf",
        ArchRProj = proj, addDOC = FALSE, width = 6, height = 5)

# Gene scores or motif deviations along pseudotime
trajGSM <- getTrajectory(
  ArchRProj  = proj,
  name       = "T_cell_trajectory",
  useMatrix  = "GeneScoreMatrix",
  log2Norm   = TRUE
)
heatmap <- plotTrajectoryHeatmap(trajGSM, varCutOff = 0.95)
plotPDF(heatmap, name = "Trajectory-GeneScores.pdf",
        ArchRProj = proj, addDOC = FALSE, width = 6, height = 8)
```

Alternative trajectory backends: `addMonocleTrajectory()`, `addSlingShotTrajectories()`.

---

## Key Parameters to Adjust

### `createArrowFiles`
- `filterTSS` (default 4) — TSS enrichment cutoff. Raise to 7-10 for stringent (fresh PBMC); keep at 4 for nuclei / tissue.
- `filterFrags` (default 1000) — min fragments. Lower for shallow libraries.
- `minFrags` / `maxFrags` — hard bounds on per-cell fragment count.

### `addIterativeLSI`
- `iterations` (default 2) — number of LSI re-runs (each refines the variable feature set).
- `varFeatures` (default 25000) — variable bins kept after the first iteration.
- `dimsToUse` (default 1:30) — LSI components to retain. Drop dim 1 if it correlates with depth (`corCutOff = 0.75` auto-drops).

### `addHarmony`
- Always include `groupBy = "Sample"` (or the appropriate batch column).
- `corCutOff = 0.75` — drops components correlated with depth before correcting.

### Peak calling
- `addReproduciblePeakSet(reproducibility = "(n+1)/2")` — require peaks in majority of replicates per group.
- Increase `pseudoBulkN` (default 500 cells per pseudo-bulk) for clusters with > 5000 cells.

---

## Best Practices

- **Allocate enough threads.** `addArchRThreads(threads = 16)` early. Most ArchR functions parallelize per-Arrow-file.
- **Save the project often.** `saveArchRProject(proj)` between every major step. ArchR analyses are long; losing state hurts.
- **Use the disk-backed Arrow format.** Don't subset the project in-memory — call `subsetArchRProject()` to create a new project directory.
- **Inspect `getCellColData(proj)` after each major step.** All per-cell metadata accumulates there — useful for sanity-checking column additions.
- **`addImputeWeights` is for plots only.** Don't run downstream analyses on imputed values — they're a visualization smoothing trick.
- **Marker peaks need `bias = c("TSSEnrichment", "log10(nFrags)")`** — without it, marker peaks are confounded by per-cell depth.
- **chromVAR runs on the deviations matrix**, not the gene score matrix. Set up `addBgdPeaks` first.
- **For ≥ 200k cells, drop the TileMatrix after building gene scores** — it's huge and rarely needed downstream.

---

## End-to-End Template

`assets/archr_template.R` — single parameterized script. Edit the CONFIGURATION block (input fragments, genome, batch column, marker genes, optional RNA reference) and run end-to-end.

## Convenience Scripts

- `scripts/build_archr.R` — Arrow files → ArchRProject → LSI/Harmony/UMAP/leiden → save
- `scripts/downstream_archr.R` — peaks + motifs + chromVAR + (optional) RNA integration

---

## References

- [ArchR website](https://www.archrproject.com/) — Greenleaf Lab
- [Full ArchR manual / bookdown](https://www.archrproject.com/bookdown/index.html) — 24 chapters
- [Brief tutorial](https://www.archrproject.com/articles/Articles/tutorial.html)
- [Function reference](https://www.archrproject.com/reference/index.html)
- Granja et al. (2021), *ArchR is a scalable software package for integrative single-cell chromatin accessibility analysis*, *Nature Genetics*
