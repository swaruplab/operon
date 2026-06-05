# Peaks, Motifs, chromVAR, Footprinting — Deep Dive

The "biology" side of ArchR — after clustering and basic visualization, this is where you turn accessibility patterns into transcription-factor-driven hypotheses. Run order matters: peaks → marker peaks → motif enrichment → chromVAR deviations → footprints.

## 1. Pseudo-bulk replicates

MACS2 expects bulk-ATAC-style coverage. ArchR builds pseudo-bulk replicates per cluster from random subsets of cells so MACS2 has enough reads, and so peaks are reproducible across replicates rather than one big pool.

```r
proj <- addGroupCoverages(
  ArchRProj  = proj,
  groupBy    = "Clusters",
  minCells   = 40,           # min cells per pseudo-bulk
  maxCells   = 500,          # max cells per pseudo-bulk
  minReplicates = 2,          # at least 2 replicates per cluster
  maxReplicates = 5,
  sampleRatio   = 0.8,       # subsample 80% of cells per replicate
  threads    = 16
)
```

Inspect: `getGroupBW(proj)` returns BigWig file paths per cluster — useful for browser-track snapshots.

## 2. MACS2 peak calling

```r
pathToMacs2 <- findMacs2()                         # auto-detect MACS2
# If macs2 isn't on PATH, install separately:
#   pip install MACS2
# Then pass explicitly: pathToMacs2 = "/path/to/macs2"

proj <- addReproduciblePeakSet(
  ArchRProj      = proj,
  groupBy        = "Clusters",
  pathToMacs2    = pathToMacs2,
  reproducibility = "(n+1)/2",     # peak must be in >= half the replicates
  cutOff         = 0.1,             # MACS2 q-value
  extendSummits  = 250,             # extend each summit ± 250 bp → 501 bp peaks
  peaksPerCell   = 500,             # max peaks per cell across all clusters
  promoterRegion = c(2000, 100),    # ± from TSS — defines promoter-proximal
  threads        = 16
)

# Result: a fixed-width (501 bp) peak set, deduplicated across clusters
peaks <- getPeakSet(proj)
```

ArchR's **iterative overlap merging** strategy produces non-redundant peaks across clusters — important for downstream marker testing.

## 3. Peak matrix

```r
proj <- addPeakMatrix(ArchRProj = proj, threads = 16)
getAvailableMatrices(proj)
# Now includes "PeakMatrix"
```

This is the cell × peak sparse matrix everything downstream operates on.

## 4. Marker peaks per cluster

```r
markersPeaks <- getMarkerFeatures(
  ArchRProj   = proj,
  useMatrix   = "PeakMatrix",
  groupBy     = "Clusters",
  bias        = c("TSSEnrichment", "log10(nFrags)"),    # critical!
  testMethod  = "wilcoxon",
  threads     = 16
)

# Extract per-cluster significant peaks
markerList <- getMarkers(markersPeaks, cutOff = "FDR <= 0.01 & Log2FC >= 1")
markerList$C5      # top peaks specific to cluster C5

# Heatmap of marker peaks (top 50 per cluster)
heatmapPeaks <- markerHeatmap(
  seMarker = markersPeaks,
  cutOff   = "FDR <= 0.01 & Log2FC >= 1",
  transpose = TRUE
)
plotPDF(heatmapPeaks, name = "Peak-Marker-Heatmap.pdf",
        ArchRProj = proj, addDOC = FALSE, width = 8, height = 6)
```

**`bias = c("TSSEnrichment", "log10(nFrags)")` is critical** — without it, marker peaks are confounded by per-cell sequencing depth and TSS-richness, producing meaningless results.

## 5. Pairwise comparisons (between two specific clusters)

```r
markerTest <- getMarkerFeatures(
  ArchRProj   = proj,
  useMatrix   = "PeakMatrix",
  groupBy     = "Clusters",
  bias        = c("TSSEnrichment", "log10(nFrags)"),
  testMethod  = "wilcoxon",
  useGroups   = "C5",                    # numerator
  bgdGroups   = "C7"                     # denominator
)
```

For condition-vs-control DARs (e.g. tumor vs normal within the same cluster), pass condition to `useGroups`/`bgdGroups` with `groupBy = "Sample"` or a condition column.

## 6. Motif enrichment in marker peaks

Annotate which peaks contain which motifs:

```r
proj <- addMotifAnnotations(
  ArchRProj = proj,
  motifSet  = "cisbp",                   # or "JASPAR2020", "JASPAR2022", "homer", "encode"
  name      = "Motif",
  species   = "Homo sapiens",
  force     = FALSE
)
```

Enrichment test — which motifs are over-represented in each cluster's marker peaks?

```r
enrichMotifs <- peakAnnoEnrichment(
  seMarker       = markersPeaks,
  ArchRProj      = proj,
  peakAnnotation = "Motif",
  cutOff         = "FDR <= 0.01 & Log2FC >= 1"
)

# Get the enrichment data frame
enrichDF <- assays(enrichMotifs)[["mlog10Padj"]]    # -log10(adj p) per motif × cluster
head(enrichDF[order(rowSums(enrichDF), decreasing = TRUE), ])

# Heatmap of top motifs per cluster
heatmapEM <- plotEnrichHeatmap(enrichMotifs, n = 7, transpose = TRUE)
plotPDF(heatmapEM, name = "Motifs-Enriched-Heatmap.pdf",
        ArchRProj = proj, addDOC = FALSE, width = 8, height = 6)
```

## 7. chromVAR deviations — per-cell TF activity

Motif enrichment tells you which TFs matter per cluster; chromVAR tells you which TFs are active **in each cell**. The deviations matrix is cells × TFs.

```r
# Background peaks needed for the deviation calculation
proj <- addBgdPeaks(proj)

# Compute deviations
proj <- addDeviationsMatrix(
  ArchRProj      = proj,
  peakAnnotation = "Motif",
  matrixName     = "MotifMatrix",
  force          = TRUE,
  threads        = 16
)

getAvailableMatrices(proj)
# Now includes "MotifMatrix"
```

Visualize per-cell motif deviations on UMAP:

```r
# Get the top variable motifs
plotVarDev <- getVarDeviations(proj, plot = TRUE, name = "MotifMatrix")
plotPDF(plotVarDev, name = "Variable-Motif-Deviations.pdf",
        ArchRProj = proj, addDOC = FALSE, width = 8, height = 6)

# Plot specific TFs on UMAP — "z:" prefix accesses the z-score (deviation) values
markerTFs <- c("z:GATA1_1", "z:CEBPA_4", "z:PAX5_1", "z:STAT1_1")
p <- plotEmbedding(
  ArchRProj     = proj,
  colorBy       = "MotifMatrix",
  name          = markerTFs,
  embedding     = "UMAP",
  imputeWeights = getImputeWeights(proj)
)
plotPDF(plotList = p, name = "TF-Deviations-UMAP.pdf",
        ArchRProj = proj, addDOC = FALSE, width = 5, height = 5)
```

The `z:` prefix returns z-scores (per-cell deviations); use no prefix for raw deviation values. For interpretation, z-scores are usually what you want.

## 8. Positive TF regulators

The intersection of: motif enrichment in marker peaks AND high motif deviation in that cluster's cells. These are TFs that are **both** active and have available binding sites in the regulated peaks — the strongest candidate drivers.

```r
seTF <- correlateMatrices(
  ArchRProj  = proj,
  useMatrix1 = "MotifMatrix",      # chromVAR deviations
  useMatrix2 = "GeneScoreMatrix",  # gene activity (proxy for TF expression)
  reducedDims = "Harmony"
)

# TFs where deviation correlates with the TF's own gene activity → "positive regulators"
seTF <- seTF[order(seTF$Correlation, decreasing = TRUE), ]
seTF[seTF$Correlation > 0.5, ]
```

The TFs at the top are positive regulators in your dataset — strong candidates for follow-up wet-lab work.

## 9. TF footprinting

For one or a few specific TFs, footprinting plots show the Tn5 cut-site distribution around predicted binding sites. A "footprint" = a notch in coverage right over the motif center (because the bound TF protects DNA from Tn5).

```r
motifPositions <- getPositions(proj)            # genome-wide motif positions

motifs <- c("GATA1_1", "CEBPA_4")
seFoot <- getFootprints(
  ArchRProj     = proj,
  positions     = motifPositions[motifs],
  groupBy       = "Clusters",
  flank         = 250                            # bp around each motif center
)

plotFootprints(
  seFoot          = seFoot,
  ArchRProj       = proj,
  normMethod      = "Subtract",                  # subtract Tn5 bias signal
  plotName        = "Footprints-Subtract-Bias",
  addDOC          = FALSE,
  smoothWindow    = 5
)
```

`normMethod` options:
- `"Subtract"` — subtract Tn5 bias track (most interpretable footprints)
- `"Divide"` — divide by Tn5 bias (less common)
- `"None"` — raw signal (rarely useful)

## 10. Peak2GeneLinkage

Find peaks correlated with gene expression across the dataset — putative enhancer-gene links.

```r
proj <- addPeak2GeneLinks(
  ArchRProj   = proj,
  reducedDims = "Harmony",
  useMatrix   = "GeneExpressionMatrix"          # from multiome OR addGeneIntegrationMatrix
)

p2g <- getPeak2GeneLinks(
  ArchRProj  = proj,
  corCutOff  = 0.45,
  resolution = 1,
  returnLoops = FALSE
)
head(p2g)
# DataFrame: idxATAC, idxRNA, Correlation, FDR
```

Visualize as browser tracks with linked arcs:

```r
p <- plotBrowserTrack(
  ArchRProj  = proj,
  groupBy    = "Clusters",
  geneSymbol = "GATA1",
  upstream   = 100000,
  downstream = 100000,
  loops      = getPeak2GeneLinks(proj)
)
plotPDF(plotList = p, name = "P2G-Tracks-GATA1.pdf",
        ArchRProj = proj, addDOC = FALSE, width = 6, height = 6)
```

## Common pitfalls

- **Skipping `addGroupCoverages` before peak calling.** Without pseudo-bulks, `addReproduciblePeakSet` fails or produces nonsense.
- **Forgetting `bias = c("TSSEnrichment", "log10(nFrags)")`** in `getMarkerFeatures`. Marker peaks become depth-confounded.
- **`addDeviationsMatrix` without `addBgdPeaks` first.** The function errors out — easy to miss in long pipelines.
- **Using `addMotifAnnotations` without specifying `species`** on mixed-genome projects. Mouse motifs end up annotated on human peaks (or vice versa) silently.
- **Running motif enrichment on the full peak set, not marker peaks.** That tests "which TFs bind any open chromatin" — uninteresting. Always pass `seMarker = markersPeaks`.
