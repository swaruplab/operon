---
name: scte
display_name: scTE
description: Quantify transposable element expression from single-cell RNA/ATAC-seq BAM files at locus or family level.
license: MIT
metadata:
---

# scTE

## Overview

scTE allocates aligned reads from single-cell RNA-seq or ATAC-seq BAM files to a unified gene + transposable element (TE) reference, producing a per-cell count matrix that contains both protein-coding genes and individual TE loci (or TE family aggregates). It uses prebuilt or custom genome indices that encode gene/exon coordinates alongside RepeatMasker-style TE annotations, and resolves the overlap between TE elements and gene bodies via configurable allocation modes (`exclusive`, `inclusive`, `nointron`). Output is a CSV table by default, or an AnnData `.h5ad` ready for Scanpy/Seurat workflows.

## When to Use This Skill

- Analyzing 10x Genomics or STARsolo scRNA-seq BAM files for TE expression alongside gene counts.
- Studying individual TE loci vs. family-level abundance in single cells.
- Processing C1 Fluidigm or other barcode-less scRNA-seq formats with UMI disabled.

## Prerequisites

```bash
git clone https://github.com/JiekaiLab/scTE.git
cd scTE
python setup.py install

# Recommended companion tools
conda install -c bioconda samtools
pip install anndata h5py pysam numpy
```

Requires Python ≥ 3.6. Prebuilt indices for mm10, hg38, panTro6, macFas5, dm6, danRer11, and xenTro9 are bundled with the repository; custom genomes are built with `scTE_build -te <BED> -gene <GTF> -o <prefix> -g <genome>`.

## Input Format

- Aligned BAM/SAM file with cell barcodes in `CR:Z` or `CB:Z` tags and UMIs in `UR:Z` or `UB:Z` tags.
- Prebuilt genome index (`mm10.exclusive.idx`, `hg38.exclusive.idx`, etc.) or custom index from GTF genes and BED TEs.
- Optional: custom gene GTF and TE BED files for `scTE_build`.

## Quick Start

```bash
scTE -i inp.bam -o out -x mm10.exclusive.idx --hdf5 True -CB CB -UMI UB
```

## Parameters

| Name | Default | Description |
| --- | --- | --- |
| `-i` | (required) | Input aligned BAM/SAM file. |
| `-o` | (required) | Output file prefix. |
| `-x` | (required) | Path to prebuilt or custom genome index (`.idx`). |
| `-p` | `1` | Number of threads (~10 GB RAM per thread). |
| `--hdf5` | `False` | Emit `.h5ad` AnnData instead of CSV when `True`. |
| `-CB` | `CR` | Cell barcode BAM tag name, or `False` to disable. |
| `-UMI` | `UR` | UMI BAM tag name, or `False` to disable. |
| `-m` / `--mode` | `exclusive` | TE/gene overlap allocation: `exclusive`, `inclusive`, or `nointron`. |

## Output

- CSV (default) or HDF5 (`--hdf5 True`) count matrix with cells as rows and genes/TEs as columns.
- HDF5 output is an AnnData `.h5ad` file directly loadable in Scanpy (`sc.read_h5ad`) or convertible to Seurat via `SeuratDisk::Convert`.

## Sharp Edges

- Memory ~10 GB per thread; cap `-p` on modest systems.
- BAM must carry correct barcode/UMI tags: Cell Ranger uses `CB:Z`/`UB:Z`, STARsolo uses `CR:Z`/`UR:Z` — wrong flags silently produce empty matrices.
- By default, TEs inside exon/UTR regions are assigned only to the gene; use `-m inclusive` to count both.
- Prebuilt indices ship for mm10, hg38, panTro6, macFas5, dm6, danRer11, xenTro9; other genomes require `scTE_build`.
- Input BAM should be coordinate-sorted and indexed (`samtools sort` + `samtools index`) for best performance; very large BAMs (>50 GB) may take hours.

## References

He, J. et al. (2021). "scTE: identifying the activity of transposable elements at single-cell resolution." *Nature Communications* 12, 1456. DOI: 10.1038/s41467-021-21808-x.
