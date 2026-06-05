# Recipe: Bulk RNA-seq with DESeq2

The canonical bulk workflow. By the end you'll have a DEG table, a volcano
plot, and a top-genes heatmap from a real count matrix.

## What you'll build

- A processed `dds.rds` (R) or `dds.pkl` (Python) — the fitted DESeq2 object
- `deg_results.csv` — full DEG table sorted by adjusted p-value
- `volcano.png` — labeled volcano with thresholds
- `heatmap_top50.png` — top 50 DEGs heatmap
- Optional: a GSEA result table against MSigDB Hallmark

## Inputs

- A counts matrix — genes × samples. CSV / TSV / Matrix Market all fine.
- A sample metadata table — at minimum, sample → condition.

If you don't have data yet, grab a public test set:

```bash
# GTEx subset — 50 muscle vs 50 brain samples
wget https://raw.githubusercontent.com/COMBINE-lab/tximport/master/inst/extdata/refseqgenes/counts.csv
```

Or any GEO accession via the GEO MCP (see [MCP catalog](../ai/mcp.md#geo-expression-data)).

## Setup

Open the project folder in Operon. Load the **bulk-rna › DESeq2
differential expression** protocol.

## Step 1 — Plan

**Plan** mode:

> *I have `counts.csv` (genes × samples) and `metadata.csv` (sample,
> condition). Run a DESeq2 analysis comparing condition `treated` vs
> `control`. Apply LFC shrinkage with apeglm, save the full DEG table,
> and produce a volcano plot labeling the top 20 genes by adjusted p-value.
> Use R DESeq2 in a script (not notebook).*

Claude will return a plan. Review:

- **Design formula** — `~ condition` is fine for two groups. With batch
  effects, expect `~ batch + condition`.
- **Independent filtering** — DESeq2 does this by default; don't disable it.
- **LFC shrinkage method** — `apeglm` is the modern default; `normal` is
  the older one. Plan should pick apeglm.
- **alpha for the results** — default is 0.1; mention 0.05 in the prompt
  if that's what you want.

Iterate in Plan until the formula and contrasts match your study.

## Step 2 — Execute

**Agent** mode:

> *Plan looks good. Generate the R script and run it.*

Agent will:

1. Create `deseq2_analysis.R`
2. Load counts + metadata, sanity-check column names match
3. Construct the `DESeqDataSet`
4. Run `DESeq()`
5. Pull `results(dds, contrast=c("condition","treated","control"))`
6. Apply `lfcShrink(dds, coef=..., type="apeglm")`
7. Save the table + the dds object
8. Generate the volcano with EnhancedVolcano

If the design has issues (e.g. confounded variables), Agent should stop
and report — don't let it silently proceed.

## Step 3 — Top genes heatmap

**Agent**:

> *Take the top 50 DEGs by adjusted p-value, extract their normalized
> counts (vst-transformed), and make a clustered heatmap with sample
> annotations for condition. Save as heatmap_top50.png.*

Agent uses `vst()` for the transform and `pheatmap` (or `ComplexHeatmap`)
for the plot.

## Step 4 — GSEA (optional)

If you also loaded the **bulk-rna › GSEA enrichment** protocol:

**Agent**:

> *Run pre-ranked GSEA on the full DEG table, ranking by `log2FoldChange *
> -log10(pvalue)`, against the MSigDB Hallmark gene sets. Save the top 20
> enriched gene sets as a CSV plus a dot plot.*

Uses `fgsea` (R) or `gseapy` (Python).

## Variations

### Three or more groups

Tell Plan: "I have three conditions: A, B, C. I want both pairwise
contrasts (A vs B, B vs C, A vs C) and a likelihood-ratio test against
the full model." Claude will use `LRT` mode and run each contrast.

### Use PyDESeq2 instead

Change protocol to **bulk-rna › PyDESeq2**. Same workflow, Python
implementation, no R toolchain required (great for HPC where R envs
are painful).

### Paired samples (e.g. before / after treatment)

Plan prompt: "Design is `~ subject + condition` because samples are
paired by donor." DESeq2 handles this fine; the contrast still extracts
the condition effect.

### Limma-voom alternative

For very small studies (< 4 per group), limma-voom is more stable than
DESeq2. Load **bulk-rna › limma-voom** instead.

## Pitfalls

- **Column-name mismatch** — counts.csv columns must exactly match
  metadata.csv sample column. Agent will catch this and ask.
- **Genes with zero counts** — DESeq2 handles them, but `vst()` warns.
  Pre-filter with `rowSums(counts) > 10` to silence the warning.
- **No replicates** — DESeq2 will refuse to estimate dispersion with
  n=1 per group. There's no good fix; collect more samples.
- **Batch confounded with condition** — if batch 1 is all treated and
  batch 2 is all control, DESeq2 can't separate them. The design matrix
  becomes rank-deficient and DESeq fails. Reroll the experiment, or
  accept you can't disentangle them.
- **Reading `dds.rds` later in Python** — you can't directly. Save the
  `results(dds)` data frame as a CSV; that's portable.

## Sanity checks

Before trusting your DEG table:

```r
# Inspect normalization
plotMA(dds)            # M vs A: should look symmetric, no obvious slope
plotPCA(vst(dds))      # PCA on vst: samples should cluster by condition
hist(res$pvalue)       # p-value histogram: enriched at low values + flat elsewhere
```

If the p-value histogram is bimodal at both 0 and 1 → batch effect.
If it's a flat hill → no real signal; check the design.

## Next steps

- Use the top DEGs in a [PubMed literature review](pubmed-review.md) to
  ground them in known biology
- Load **bulk-rna › Enhanced volcano** protocol for publication-grade
  volcano styling
- Pseudobulk a single-cell experiment first, then come back here for the
  per-cluster DEG analysis
