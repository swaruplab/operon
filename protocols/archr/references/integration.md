# Integration, Multiome, Trajectories — Deep Dive

The "second-layer" analyses that build on the clustered ArchR project. Each section can be run independently once you have a project with clusters + UMAP + (for some) peaks.

## scRNA-seq Integration (label transfer)

When you have an annotated scRNA-seq Seurat object covering similar cells, transfer labels to the ATAC cells via CCA alignment of gene scores.

### Prereqs
- An annotated scRNA-seq Seurat object (`seRNA`) with cell-type labels in `seRNA@meta.data$cell_type`
- The ATAC project must have `GeneScoreMatrix` (built by `createArrowFiles` if `addGeneScoreMat = TRUE`)

```r
library(Seurat)
seRNA <- readRDS("rna_reference.rds")
table(seRNA$cell_type)             # sanity-check the labels

proj <- addGeneIntegrationMatrix(
  ArchRProj          = proj,
  useMatrix          = "GeneScoreMatrix",
  matrixName         = "GeneIntegrationMatrix",
  reducedDims        = "Harmony",
  seRNA              = seRNA,
  addToArrow         = TRUE,        # persist into Arrow files
  groupRNA           = "cell_type",  # column in seRNA@meta.data
  nameCell           = "predictedCell",
  nameGroup          = "predictedGroup",
  nameScore          = "predictedScore",
  groupATAC          = "Clusters",   # restrict to within-cluster matching
  k.anchor           = 5,
  k.score            = 30,
  reducedDims.score  = "Harmony",
  threads            = 16
)
```

After this, `getCellColData(proj)` has three new columns:
- `predictedCell` — the matched scRNA cell barcode
- `predictedGroup` — the predicted cell-type label
- `predictedScore` — confidence (0-1)

### Visualize predictions

```r
p <- plotEmbedding(
  ArchRProj = proj,
  colorBy   = "cellColData",
  name      = "predictedGroup",
  embedding = "UMAP"
)
plotPDF(p, name = "UMAP-Predicted-CellType.pdf",
        ArchRProj = proj, addDOC = FALSE, width = 6, height = 5)

# Per-cluster confidence
cluster_scores <- aggregate(
  getCellColData(proj)$predictedScore,
  by = list(Cluster = getCellColData(proj)$Clusters),
  FUN = mean
)
```

Clusters with mean confidence > 0.5 are reliably labeled; below that, the cluster may represent an ATAC-only cell type not in the RNA reference.

### Confusion-matrix inspection

```r
library(pheatmap)
cM <- as.matrix(table(
  getCellColData(proj)$Clusters,
  getCellColData(proj)$predictedGroup
))
cM <- cM / rowSums(cM)              # row-normalize to fractions
pheatmap(
  cM,
  border_color = "black",
  color = colorRampPalette(c("white", "blue"))(100),
  filename = "Cluster-to-CellType.pdf"
)
```

A clean diagonal → clusters map 1:1 to cell types. Multiple cell types per cluster → sub-cluster; one cell type spread across clusters → those are biologically related sub-states.

### After label transfer — relabel clusters

```r
remap <- c("C1" = "T_CD4", "C2" = "T_CD8", "C3" = "B", ...)
proj$CellType <- remap[proj$Clusters]
# Save and use CellType for all downstream group-level analyses
```

## Pseudo-scRNA-seq profiles per cell

`addGeneIntegrationMatrix` with `addToArrow = TRUE` also writes a synthetic gene-expression matrix for the ATAC cells (using the nearest scRNA cells in the integrated space). Useful for cross-comparisons that need a "shared expression space":

```r
proj <- addImputeWeights(proj)
markerGenes <- c("CD3D", "CD3E", "MS4A1", "CD14")
p <- plotEmbedding(
  ArchRProj      = proj,
  colorBy        = "GeneIntegrationMatrix",     # NOT GeneScoreMatrix — the imputed RNA
  name           = markerGenes,
  embedding      = "UMAP",
  imputeWeights  = getImputeWeights(proj)
)
```

Compare the same plot using `colorBy = "GeneScoreMatrix"` — the gene-score version is purely chromatin-derived, the integration-matrix version is RNA-imputed. Pseudo-RNA usually shows cleaner cell-type markers.

## Multiome (paired ATAC + RNA in the same cell)

For 10X Multiome / Chromium-X data where one droplet captures both modalities. The Arrow files come from `cellranger-arc` ATAC fragments, and the RNA comes from the same pipeline's `filtered_feature_bc_matrix.h5`.

```r
# Step 1: build the standard ATAC project
ArrowFiles <- createArrowFiles(
  inputFiles      = c("donor1" = "/data/multiome/donor1/atac_fragments.tsv.gz"),
  sampleNames     = "donor1",
  filterTSS       = 4, filterFrags = 1000,
  addTileMat      = TRUE, addGeneScoreMat = TRUE
)
proj <- ArchRProject(ArrowFiles, outputDirectory = "Multiome", copyArrows = TRUE)

# Step 2: import the matched RNA matrix
rna_se <- import10xFeatureMatrix(
  input = "/data/multiome/donor1/filtered_feature_bc_matrix.h5",
  names = "donor1"
)
proj <- addGeneExpressionMatrix(input = proj, seRNA = rna_se, threads = 16)

# Step 3: separate LSI per modality
proj <- addIterativeLSI(
  ArchRProj  = proj,
  useMatrix  = "TileMatrix",
  name       = "LSI_ATAC",
  varFeatures = 25000,
  dimsToUse  = 1:30
)
proj <- addIterativeLSI(
  ArchRProj   = proj,
  useMatrix   = "GeneExpressionMatrix",
  name        = "LSI_RNA",
  varFeatures = 2500,
  dimsToUse   = 1:30,
  firstSelection = "var",                  # different feature selection for RNA
  binarize    = FALSE                       # RNA isn't binary
)

# Step 4: combine into a joint embedding
proj <- addCombinedDims(
  proj,
  reducedDims = c("LSI_ATAC", "LSI_RNA"),
  name        = "LSI_Combined"
)

proj <- addUMAP   (proj, reducedDims = "LSI_Combined", name = "UMAP_Combined")
proj <- addClusters(proj, reducedDims = "LSI_Combined", name = "JointClusters",
                     resolution = 0.8)
```

Now `JointClusters` reflects shared ATAC+RNA structure. Compare with per-modality clusters:

```r
proj <- addUMAP(proj, reducedDims = "LSI_ATAC", name = "UMAP_ATAC")
proj <- addUMAP(proj, reducedDims = "LSI_RNA",  name = "UMAP_RNA")

p1 <- plotEmbedding(proj, colorBy = "cellColData", name = "JointClusters", embedding = "UMAP_ATAC")
p2 <- plotEmbedding(proj, colorBy = "cellColData", name = "JointClusters", embedding = "UMAP_RNA")
p3 <- plotEmbedding(proj, colorBy = "cellColData", name = "JointClusters", embedding = "UMAP_Combined")
plotPDF(p1, p2, p3, name = "Multiome-Joint-Clusters.pdf",
        ArchRProj = proj, addDOC = FALSE, width = 5, height = 5)
```

### Peak2Gene on multiome — high-confidence enhancer-gene pairs

With both modalities per-cell, peak-to-gene linking is much stronger than the synthetic version from unpaired integration:

```r
proj <- addPeak2GeneLinks(
  ArchRProj   = proj,
  reducedDims = "LSI_Combined",
  useMatrix   = "GeneExpressionMatrix"
)
p2g <- getPeak2GeneLinks(proj, corCutOff = 0.45, FDRCutOff = 1e-04)
```

These are typically tighter, higher-confidence than gene-score-derived links.

## Trajectories

### Built-in supervised trajectories

Define the cluster ordering manually based on biology, then ArchR computes pseudotime + per-cell ordering:

```r
# Example: HSC → CMP → GMP → Mono differentiation
trajectory <- c("HSC", "CMP", "GMP", "Mono")        # values in $Clusters (or another grouping)

proj <- addTrajectory(
  ArchRProj  = proj,
  name       = "Myeloid_traj",
  groupBy    = "Clusters",
  trajectory = trajectory,
  embedding  = "UMAP",
  force      = TRUE
)

# The trajectory column is now in cellColData
head(getCellColData(proj)$Myeloid_traj)            # NA for cells not on the trajectory
```

### Visualize

```r
p <- plotTrajectory(
  ArchRProj  = proj,
  trajectory = "Myeloid_traj",
  colorBy    = "cellColData",
  name       = "Myeloid_traj",
  embedding  = "UMAP"
)
plotPDF(p, name = "Trajectory-Myeloid.pdf",
        ArchRProj = proj, addDOC = FALSE, width = 6, height = 5)

# Gene-score dynamics along the trajectory
trajGSM <- getTrajectory(
  ArchRProj  = proj,
  name       = "Myeloid_traj",
  useMatrix  = "GeneScoreMatrix",
  log2Norm   = TRUE
)
heatmapGSM <- plotTrajectoryHeatmap(trajGSM, varCutOff = 0.95)

# Motif deviation dynamics
trajMM <- getTrajectory(
  ArchRProj  = proj,
  name       = "Myeloid_traj",
  useMatrix  = "MotifMatrix",
  log2Norm   = FALSE                                # already z-scored
)
heatmapMM <- plotTrajectoryHeatmap(trajMM, varCutOff = 0.95)

plotPDF(heatmapGSM, heatmapMM,
        name = "Trajectory-Dynamics.pdf",
        ArchRProj = proj, addDOC = FALSE, width = 6, height = 8)
```

### Monocle3 / Slingshot backends

When the cluster ordering isn't obvious or you want unsupervised trajectory inference:

```r
# Monocle3
proj <- addMonocleTrajectory(
  ArchRProj  = proj,
  reducedDims = "Harmony",
  groupBy    = "Clusters",
  useGroups  = NULL,                       # NULL = use all clusters
  name       = "Monocle_traj"
)

# Slingshot
proj <- addSlingShotTrajectories(
  ArchRProj  = proj,
  reducedDims = "Harmony",
  groupBy    = "Clusters",
  useGroups  = c("HSC", "CMP", "GMP", "Mono"),
  name       = "Slingshot_traj"
)
```

The output trajectory column has the same shape — pseudotime per cell — and all trajectory-heatmap functions work identically.

## When integration / multiome / trajectories disagree

- **Joint clusters fragment a labeled cell type**: the RNA and ATAC are revealing sub-states. Inspect by adding a `_RNA`-only and `_ATAC`-only UMAP next to the joint one.
- **Built-in trajectory disagrees with Monocle3**: the built-in is supervised (uses your provided cluster order); Monocle3 is unsupervised. Trust the data-driven one if your ordering was guesswork; trust the supervised one if you have strong prior biology.
- **Label transfer gives low scores everywhere**: your RNA reference doesn't cover these cells. Either find a better reference or fall back to manual annotation via gene scores.
