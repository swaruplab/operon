---
name: enhanced-volcano-plot
description: Publication-quality volcano plots from DE results using EnhancedVolcano (R) or matplotlib (Python).
license: MIT
metadata:
---

# Enhanced Volcano Plot

## Overview

A volcano plot is the standard one-glance summary of a differential-expression
(DE) result: each gene is a point with the effect size (log2 fold-change) on the
x-axis and the statistical significance (-log10 of a p-value or adjusted
p-value) on the y-axis. The shape of the resulting "volcano" makes it trivial
to read off three things at once — how many genes move, how strong those moves
are, and how robust they are statistically. This skill wraps the canonical
Bioconductor package [EnhancedVolcano](https://github.com/kevinblighe/EnhancedVolcano)
into a single runnable Rscript template, and offers a Python/matplotlib
fallback for environments where R is unavailable.

## When to Use This Skill

- You have a DE table from DESeq2, edgeR, limma-voom, Seurat `FindMarkers`,
  scanpy `rank_genes_groups`, or any pipeline that produces a per-gene
  `log2FoldChange` + `p-value` / `padj` column pair.
- You want a publication-ready figure (PDF or 300-dpi PNG) with labelled top
  genes, threshold guide-lines, and a DEG-count caption.
- You want consistent styling across many comparisons (cluster vs cluster,
  treatment vs control, time-point vs baseline).

**Not for**: pathway/GSEA visualisations (use a barplot / dotplot of normalized
enrichment scores instead), MA-plots (different convention: log2FC vs mean
expression), or QC of raw counts (use a PCA / dispersion plot).

## Prerequisites

EnhancedVolcano lives on Bioconductor; `ggrepel` and `optparse` live on CRAN.
The template guards each `library()` call with a `requireNamespace()` check
and installs the missing pieces, so this block is documentation rather than a
hard prerequisite — but installing once up-front avoids the install overhead
during plotting:

```r
if (!requireNamespace("BiocManager", quietly = TRUE))
  install.packages("BiocManager")

if (!requireNamespace("EnhancedVolcano", quietly = TRUE))
  BiocManager::install("EnhancedVolcano", update = FALSE, ask = FALSE)

if (!requireNamespace("ggrepel", quietly = TRUE))
  install.packages("ggrepel")

if (!requireNamespace("optparse", quietly = TRUE))
  install.packages("optparse")

if (!requireNamespace("magrittr", quietly = TRUE))
  install.packages("magrittr")
```

R ≥ 4.1 is recommended (matches current Bioconductor). The template is
single-threaded and runs in seconds on tables of up to ~100k genes.

## Input Format

A delimited text file with at least three columns:

| role          | typical names                                              |
| ------------- | ---------------------------------------------------------- |
| gene name     | `gene`, `gene_name`, `symbol`, `Gene`, `rowname`           |
| effect size   | `log2FoldChange` (DESeq2), `logFC` (edgeR/limma), `avg_log2FC` (Seurat) |
| significance  | `padj` (DESeq2), `FDR` (edgeR), `adj.P.Val` (limma), `p_val_adj` (Seurat), or raw `pvalue` |

The template auto-detects the file type from the extension:

- `.csv` &rarr; `read.csv`
- `.tsv` / `.txt` &rarr; `read.delim`

You pick which columns to use via `--gene-col`, `--x-col`, `--y-col`.
The default is `gene` / `log2FoldChange` / `padj`, which matches a tidied
DESeq2 result.

## Quick Start — R

```bash
Rscript assets/enhanced_volcano_template.R \
  --input  results/treated_vs_control.tsv \
  --output figures/volcano_treated_vs_control.pdf \
  --gene-col gene \
  --x-col log2FoldChange \
  --y-col padj \
  --p-cutoff 0.05 \
  --fc-cutoff 1.0 \
  --label-top-n 20 \
  --title "Treated vs Control" \
  --subtitle "DESeq2, padj < 0.05, |log2FC| > 1"
```

Or, equivalently, edit the `CONFIGURATION` block at the top of
`assets/enhanced_volcano_template.R` and run with no flags:

```bash
Rscript assets/enhanced_volcano_template.R
```

## Quick Start — Python

The R path is the primary, recommended one — `EnhancedVolcano` handles label
collision avoidance and threshold annotation in a polished way that is hard
to match in matplotlib. If R is genuinely unavailable, build the plot directly
with `matplotlib` + [`adjustText`](https://github.com/Phlya/adjustText):

```python
import pandas as pd, numpy as np, matplotlib.pyplot as plt
from adjustText import adjust_text

df = pd.read_csv("results/treated_vs_control.tsv", sep="\t")
df["nlog10"] = -np.log10(df["padj"].clip(lower=1e-300))
sig_up   = (df["log2FoldChange"] >  1.0) & (df["padj"] < 0.05)
sig_down = (df["log2FoldChange"] < -1.0) & (df["padj"] < 0.05)
colour = np.where(sig_up, "#E41A1C", np.where(sig_down, "#377EB8", "grey70"))

fig, ax = plt.subplots(figsize=(10, 8), dpi=300)
ax.scatter(df["log2FoldChange"], df["nlog10"], c=colour, s=8, alpha=0.8)
ax.axhline(-np.log10(0.05), ls="--", c="grey50")
ax.axvline( 1.0, ls="--", c="grey50"); ax.axvline(-1.0, ls="--", c="grey50")
top = df.nsmallest(15, "padj")
texts = [ax.text(r.log2FoldChange, -np.log10(r.padj), r.gene, fontsize=8)
         for r in top.itertuples()]
adjust_text(texts, ax=ax, arrowprops=dict(arrowstyle="-", color="grey50", lw=0.5))
ax.set_xlabel("log2 fold-change"); ax.set_ylabel("-log10(padj)")
fig.savefig("volcano.pdf", bbox_inches="tight")
```

Use the R path unless you have a hard reason not to.

## Parameters

All flags map 1:1 onto variables of the same name in the
`CONFIGURATION` block of the R template.

| Flag              | Default            | Meaning                                                                 |
| ----------------- | ------------------ | ----------------------------------------------------------------------- |
| `--input`         | (required)         | DE table (`.csv`, `.tsv`, `.txt`)                                       |
| `--output`        | (required)         | Plot path; extension must be `.pdf` or `.png`                           |
| `--gene-col`      | `gene`             | Column holding gene names / row IDs                                     |
| `--x-col`         | `log2FoldChange`   | Effect-size column (already on log2 scale)                              |
| `--y-col`         | `padj`             | Significance column — raw p-value or adjusted p-value                   |
| `--p-cutoff`      | `0.05`             | Horizontal threshold on `--y-col`                                       |
| `--fc-cutoff`     | `1.0`              | Vertical thresholds on `--x-col` (symmetric: ±FC)                       |
| `--label-top-n`   | `15`               | How many of the most significant genes to label                         |
| `--title`         | (filename stem)    | Plot title                                                              |
| `--subtitle`      | DEG count caption  | Plot subtitle; if empty, falls back to "N up / N down / N significant"  |
| `--width`         | `10`               | Output width (inches)                                                   |
| `--height`        | `8`                | Output height (inches)                                                  |
| `--point-size`    | `2.0`              | Point size                                                              |
| `--label-size`    | `3.5`              | Gene-label font size                                                    |
| `--draw-connectors` | `TRUE`           | Draw label-to-point connectors (`TRUE`/`FALSE`)                         |
| `--max-overlaps`  | `15`               | `ggrepel` overlap budget                                                |
| `--colour-up`     | `#E41A1C` (red)    | Colour for significant up-regulated points                              |
| `--colour-down`   | `#377EB8` (blue)   | Colour for significant down-regulated points                            |
| `--colour-ns`     | `grey70`           | Colour for non-significant points                                       |

## Output

A single file at `--output`:

- `.pdf` &rarr; vector, infinite resolution, the right choice for papers.
- `.png` &rarr; rasterised at 300 dpi, the right choice for slides.

Both are rendered at `--width` × `--height` inches. The legend (top-right by
default) shows the four EnhancedVolcano categories overlaid with the custom
up / down / ns palette so colours match the DEG count caption.

The script also prints one summary line to stdout, e.g.:

```
Wrote figures/volcano_treated_vs_control.pdf: 412 up, 287 down, 699 significant out of 18432.
```

## Style Guidelines

- Label only the top **15-20** most significant genes — beyond that the plot
  becomes a textbox and the eye loses every label.
- Title 16pt, axis labels 12pt, gene labels 8-10pt (template defaults already
  fall in this range).
- Save **both PNG (300 dpi, for slides) and PDF (vector, for the paper)** for
  each comparison — run the template twice with different `--output`
  extensions.
- Always include the **DEG count annotation** ("N up / N down / N significant")
  either as the subtitle or in the figure caption — the eye cannot reliably
  estimate point density.
- Use the same fold-change and p-value cutoffs across every comparison in a
  figure-set so the volcanoes are visually comparable.
- Prefer **`padj` / `FDR` / `adj.P.Val` over raw p-values** for the y-axis —
  raw p-values inflate the visual significance of every gene and will mislead
  reviewers.

## References

- Blighe K, Rana S, Lewis M. EnhancedVolcano: Publication-ready volcano plots
  with enhanced colouring and labeling. Bioconductor.
  <https://bioconductor.org/packages/EnhancedVolcano/>
- Slowikowski K. *ggrepel*: Automatically position non-overlapping text labels
  with *ggplot2*. CRAN. <https://cran.r-project.org/package=ggrepel>
