---
name: solote
description: Locus-specific transposable element quantification from single-cell RNA-seq BAMs, producing a gene+TE 10x-style count matrix.
license: GPL-3.0
metadata:
---

# SoloTE: Single-Cell Transposable Element Quantification

## Overview

[SoloTE](https://github.com/bvaldebenitom/SoloTE) quantifies **transposable element (TE) expression in single-cell RNA-seq** by re-annotating aligned, cell-barcoded BAM reads against TE genomic coordinates. The output is a 10x-style cell-by-feature matrix that fuses gene counts with **locus-level** (and where ambiguous, subfamily-level) TE counts.

Standard scRNA-seq pipelines (Cell Ranger, STARsolo) discard or down-weight multi-mappers — the same reads that hold most TE signal. SoloTE re-uses an existing BAM (no realignment) and routes TE-overlapping reads through a locus-vs-subfamily decision so that uniquely-mappable TE loci are preserved at locus resolution while ambiguous reads collapse to the subfamily level.

## When to Use This Skill

- Single-cell TE quantification from 10x Genomics / Cell Ranger BAMs (cell-barcoded, UMI-tagged).
- Resolving locus-specific TE expression where multi-mappers usually get discarded by standard scRNA-seq pipelines.
- Adding TE features alongside genes for downstream Seurat / Scanpy clustering and differential expression.

**Not for**:
- FASTQ → BAM alignment (run Cell Ranger or STARsolo upstream first).
- Bulk RNA-seq TE quantification (use TEtranscripts / SQuIRE).
- BAMs without a `CB` cell-barcode tag (e.g. raw STAR output without `--soloFeatures`).

## Prerequisites

```bash
git clone https://github.com/bvaldebenitom/SoloTE.git
cd SoloTE

# System tools — install via conda or your HPC module system
conda install -c bioconda "samtools>=1.16" "bedtools>=2.29.2" "r-base>=4"

# Python deps
pip install "pysam" "pandas>=1.5.0"

python SoloTE_RepeatMasker_to_BED.py -g hg38
# Replace hg38 with the build you aligned to (mm10, mm39, GRCh38, etc.).
# Produces a BED with col4 = locus|Subfamily:Family:Class — required input below.
```

## Input Format

| Input | Description |
|---|---|
| Aligned BAM | Cell-barcoded, UMI-tagged BAM. Cell Ranger's `possorted_genome_bam.bam` works out of the box; STARsolo output works if it carries the `CB` tag. |
| TE annotation BED | 5-column BED: `chr  start  end  locus\|Subfamily:Family:Class  strand`. Always (re)generate with `SoloTE_RepeatMasker_to_BED.py -g <build>` — hand-editing this file breaks the locus/subfamily decision logic. |

## Quick Start

```bash
python SoloTE_pipeline.py \
  --threads 8 \
  --bam possorted_genome_bam.bam \
  --teannotation hg38_rmsk.bed \
  --outputprefix sample1 \
  --outputdir ./results
```

## Parameters

| Name | Default | Description |
|---|---|---|
| `--bam` | required | Aligned, cell-barcoded BAM (Cell Ranger / STARsolo). Must carry the `CB` tag. |
| `--teannotation` | required | TE annotation BED from `SoloTE_RepeatMasker_to_BED.py`. |
| `--outputprefix` | required | Sample prefix prepended to all output filenames. |
| `--outputdir` | required | Destination directory for the MTX output and intermediates. |
| `--threads` | 1 | Parallelism for the samtools / bedtools steps. Bump to the per-job CPU budget on HPC. |

Additional flags (e.g. read-length / locus-vs-subfamily thresholds) are not surfaced in the upstream README — consult `python SoloTE_pipeline.py --help` for the full list.

## Output

A 10x-style MTX directory written to `--outputdir`, containing genes + TE features in a cells × features matrix:

```
<outputprefix>_SoloTE_output/
  matrix.mtx
  barcodes.tsv
  features.tsv
```

Feature naming:
- **Genes** keep their Ensembl / symbol IDs.
- **TE features** are named either by locus — `SoloTE|chr:start-end|Subfamily:Family:Class` — or, where reads can't be assigned to a single locus, collapsed to the subfamily.

Loads directly into Seurat (`Read10X`) or Scanpy (`scanpy.read_mtx` + companion barcodes/features).

## Sharp Edges

- **`CB` tag is mandatory.** Input BAM must carry the cell-barcode tag — works out-of-the-box with Cell Ranger / STARsolo output; raw aligner BAMs without `CB` fail silently or produce empty matrices.
- **TE BED format is strict.** Column 4 must be `locus|Subfamily:Family:Class` — always (re)generate it with `SoloTE_RepeatMasker_to_BED.py`, never hand-edit.
- **Resource scaling is undocumented.** Memory and runtime scale with BAM size and `--threads`; expect multi-hour runs and tens of GB RAM for a typical 10x sample.
- **Linux / macOS only in practice** (samtools / bedtools chain) — no Windows-native support; use WSL or a remote HPC.
- **`/tmp` is node-local on HPC.** Point `--outputdir` at a shared filesystem path when running on compute nodes, or intermediates vanish between login and compute nodes.
- **No alignment step.** FASTQ → BAM (Cell Ranger / STARsolo) must be run upstream — SoloTE consumes BAMs, not reads.

## References

- Source: [github.com/bvaldebenitom/SoloTE](https://github.com/bvaldebenitom/SoloTE)
- Rodríguez-Quiroz R, Valdebenito-Maturana B. *SoloTE for improved analysis of transposable elements in single-cell RNA-seq data using locus-specific expression.* Communications Biology 5, 1063 (2022). DOI: [10.1038/s42003-022-04020-5](https://doi.org/10.1038/s42003-022-04020-5)
