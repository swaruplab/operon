---
name: kallisto-bustools
description: scRNA-seq quantification with kallisto + bustools via kb-python. Pseudoalignment-based — orders of magnitude faster than full alignment (STAR / cellranger) while producing comparable count matrices. Covers index generation (kb ref), per-sample quantification (kb count), supported chemistries (10X v1/v2/v3, Drop-seq, CEL-Seq, inDrops, SMART-Seq), the nac workflow for nascent/mature RNA (snRNA-seq + RNA velocity), and the handoff to scanpy / Seurat / SCE for downstream analysis.
license: BSD-2-Clause
metadata:
---

# kallisto | bustools: Pseudoalignment-Based scRNA-seq Quantification

## Overview

[kallisto + bustools](https://kallisto.readthedocs.io/) is a **pseudoalignment**-based quantification pipeline for single-cell RNA-seq. The standard tools (cellranger, STARsolo, alevin-fry) do full alignment of reads to the genome; kallisto skips that and only determines **which transcripts are compatible with each read**. The result: 5–20× faster than cellranger, comparable accuracy, much lower memory footprint.

The toolkit has three components:

| Tool | What it does |
|---|---|
| `kallisto` | The pseudoaligner (also handles non-single-cell quantification) |
| `bustools` | Processes BUS files — barcode correction, UMI dedup, count matrix generation |
| `kb-python` | Thin wrapper that runs kallisto + bustools end-to-end with one command |

For most users, `kb-python` is the only entry point you need. This protocol uses `kb` throughout.

## When to Use This Skill

- Generating count matrices from raw FASTQ files for any droplet-based chemistry (10X, Drop-seq, inDrops) or plate-based (SMART-Seq)
- Processing **many samples** where cellranger would take days — kb does it in hours
- snRNA-seq / RNA velocity workflows that need both **nascent** (pre-mRNA) and **mature** counts
- Custom seqspec-defined chemistries
- Pipelines where you want maximum reproducibility — kb is deterministic, while cellranger has subtle non-determinism

**Not for**:
- Cell calling / empty-drop detection — kb produces unfiltered matrices; use `DropletUtils::emptyDrops` or `CellBender` downstream
- Cell-type annotation, clustering, DE — handoff to scanpy / Seurat / SCE
- Spatial transcriptomics — use Space Ranger or the spatial-transcriptomics protocol

## Prerequisites

- Python 3.8+
- ~32 GB RAM for human/mouse index build; ~16 GB for `kb count` per sample
- Linux/macOS preferred (Windows works via WSL)
- Reference: a transcriptome FASTA + GTF (Ensembl, GENCODE, RefSeq)

### Installation

```bash
# pip (recommended)
pip install kb_python

# OR conda
conda install -c bioconda kb-python

# Verify
kb --version
kb --list                              # supported single-cell technologies
```

`kb-python` bundles the kallisto + bustools binaries — no separate install needed for them.

For very large indexes (whole genome with introns, atlas-scale references), you may need to build kallisto from source for the v0.50+ features:

```bash
git clone https://github.com/pachterlab/kallisto && cd kallisto && mkdir build && cd build
cmake .. -DUSE_HDF5=ON && make && make install
```

## Pipeline 1 — Build the Reference Index

You only do this once per genome. Reuse for all your samples.

### Standard transcriptome (most users)

```bash
# Inputs: genome FASTA + GTF (matching versions)
# Example with GENCODE human v44:
kb ref \
    -i  index.idx \
    -g  t2g.txt \
    -f1 cdna.fasta \
    GRCh38.primary_assembly.genome.fa.gz \
    gencode.v44.annotation.gtf.gz
```

**Output files**:
- `index.idx` — the kallisto pseudoalignment index (~3-5 GB for human)
- `t2g.txt` — transcript-to-gene mapping
- `cdna.fasta` — extracted transcript sequences (intermediate, keep if rebuilding)

This takes ~30-60 minutes for human/mouse on a typical workstation. Index files are reusable across all your samples.

### Pre-built indexes (skip the build)

For common references, kb-python can download pre-built indexes:

```bash
kb ref -d human  -i index.idx -g t2g.txt
kb ref -d mouse  -i index.idx -g t2g.txt
kb ref -d zebrafish -i index.idx -g t2g.txt
```

Faster than building, identical result. Recommended unless you have a custom genome.

### nac workflow — nascent + mature RNA (for snRNA-seq and RNA velocity)

Single-nucleus RNA-seq captures predominantly intronic reads (from pre-mRNA in the nucleus). Standard transcriptome indexes only handle exonic/mature mRNA, so you lose those reads. The `nac` workflow indexes both.

```bash
kb ref \
    --workflow nac \
    -i  index.idx \
    -g  t2g.txt \
    -c1 cdna.txt          \
    -c2 nascent.txt       \
    -f1 cdna.fasta        \
    -f2 nascent.fasta     \
    GRCh38.primary_assembly.genome.fa.gz \
    gencode.v44.annotation.gtf.gz
```

**Additional outputs** vs standard:
- `cdna.txt` — transcript IDs for mature (spliced) sequences
- `nascent.txt` — transcript IDs for nascent (intron-containing) sequences
- `nascent.fasta` — pre-mRNA sequences for the index

Required when:
- Running snRNA-seq (most nuclei reads land in introns)
- Doing RNA velocity (need separate spliced + unspliced counts)

For RNA velocity specifically, also consider the older `lamanno` workflow which produces the spliced/unspliced layout scVelo expects:

```bash
kb ref --workflow lamanno -i index.idx -g t2g.txt \
    -f1 cdna.fasta -f2 intron.fasta \
    -c1 cdna_t2c.txt -c2 intron_t2c.txt \
    GRCh38.primary_assembly.genome.fa.gz gencode.v44.annotation.gtf.gz
```

Convenience: `bash scripts/kb_ref.sh --genome GRCh38.fa --gtf annotation.gtf --out-dir reference/`. See [references/advanced.md](references/advanced.md) for nac vs lamanno trade-offs and how to choose.

Source: [Index generation docs](https://kallisto.readthedocs.io/en/latest/index/index_generation.html).

## Pipeline 2 — Quantify a Sample

```bash
# Standard 10X v3 (cellranger output style)
kb count \
    -i index.idx \
    -g t2g.txt \
    -x 10xv3 \
    -o output_dir \
    --h5ad \
    sample_R1.fastq.gz sample_R2.fastq.gz
```

### Key arguments

| Argument | Purpose | Notes |
|---|---|---|
| `-i index.idx` | Path to the index | From `kb ref` |
| `-g t2g.txt` | Transcript-to-gene mapping | From `kb ref` |
| `-x <tech>` | Chemistry / sequencing technology | See `kb --list` |
| `-o output_dir` | Output directory | Created if missing |
| `--workflow nac` | Use nascent+mature index | Match the `kb ref` workflow |
| `--h5ad` | Write `.h5ad` directly (scanpy-ready) | Recommended |
| `--filter bustools` | Apply knee-plot cell filtering | Optional — usually do this in scanpy |
| `-t N` | Threads | Default 8; bump on big machines |
| `-m 8G` | Max RAM | Default 4G; bump for big indexes |

### Supported chemistries (`-x` values)

| Chemistry | `-x` value | FASTQ layout |
|---|---|---|
| 10X Chromium v2 (3') | `10xv2` | R1 = barcode (16 bp) + UMI (10 bp); R2 = cDNA |
| 10X Chromium v3 (3' / multiome) | `10xv3` | R1 = barcode (16 bp) + UMI (12 bp); R2 = cDNA |
| 10X Chromium v1 (legacy) | `10xv1` | I1, R1, R2 — index file is the barcode |
| Drop-seq | `DROPSEQ` | R1 = barcode (12 bp) + UMI (8 bp); R2 = cDNA |
| inDrops v1 | `INDROPSV1` | (custom — see `kb --list`) |
| CEL-Seq | `CELSEQ` | UMI + barcode in R1 |
| SMART-Seq v2 / v3 | `SMARTSEQ2` / `SMARTSEQ3` | full-length, --parity paired |
| BD Rhapsody | `BDWTA` | barcode + UMI |
| Slide-seq, Visium HD, etc. | various | run `kb --list` |

Full list: `kb --list` after installation. For unsupported chemistries, define one in seqspec and pass via `-x <seqspec.yaml>`.

### Input FASTQ layout

For paired-end droplet chemistries, file order is **R1 first, R2 second**, repeated per lane:

```bash
kb count -i index.idx -g t2g.txt -x 10xv3 -o out --h5ad \
    L001_R1.fastq.gz L001_R2.fastq.gz \
    L002_R1.fastq.gz L002_R2.fastq.gz \
    L003_R1.fastq.gz L003_R2.fastq.gz
```

For 10X v1 (legacy), also include the I1 file: `I1 R1 R2 I1 R1 R2 ...`.

For SMART-Seq2 / full-length protocols where both reads are biological, use `--parity paired`:

```bash
kb count -i index.idx -g t2g.txt -x SMARTSEQ2 -o out --parity paired \
    SAMPLE1_1.fastq.gz SAMPLE1_2.fastq.gz \
    SAMPLE2_1.fastq.gz SAMPLE2_2.fastq.gz
```

### Output structure

```
output_dir/
├── counts_unfiltered/                 # cell × gene matrix BEFORE cell calling
│   ├── cells_x_genes.mtx               # sparse matrix (Matrix Market format)
│   ├── cells_x_genes.barcodes.txt     # cell barcodes (one per row)
│   ├── cells_x_genes.genes.txt        # gene IDs (Ensembl)
│   ├── cells_x_genes.genes.names.txt  # gene symbols
│   └── adata.h5ad                     # AnnData (if --h5ad was passed)
├── inspect.json                        # QC metrics from bustools
├── run_info.json                       # kallisto run info (alignment rate, etc.)
├── output.bus                          # raw BUS file
├── output.unfiltered.bus               # barcode-corrected
└── matrix.ec                           # equivalence classes
```

The `counts_unfiltered/` directory is your handoff point. **Empty droplets are NOT filtered out** at this stage — call cells with `DropletUtils::emptyDrops` or `CellBender` downstream.

Convenience: `bash scripts/kb_count.sh --index index.idx --t2g t2g.txt --tech 10xv3 --out-dir sample1_out fastq/R1.fastq.gz fastq/R2.fastq.gz`.

Source: [pseudoalignment + output files docs](https://kallisto.readthedocs.io/en/latest/sc/pseudoalignment.html).

## Pipeline 3 — Handoff to Downstream (scanpy / Seurat / SCE)

### scanpy / Python

```python
import scanpy as sc
import anndata

# Easiest: load the .h5ad written by kb count --h5ad
adata = anndata.read_h5ad('output_dir/counts_unfiltered/adata.h5ad')

# Or load the .mtx manually if --h5ad wasn't passed
adata = sc.read_mtx('output_dir/counts_unfiltered/cells_x_genes.mtx')
barcodes = open('output_dir/counts_unfiltered/cells_x_genes.barcodes.txt').read().splitlines()
genes    = open('output_dir/counts_unfiltered/cells_x_genes.genes.txt').read().splitlines()
gene_names = open('output_dir/counts_unfiltered/cells_x_genes.genes.names.txt').read().splitlines()
adata.obs_names = barcodes
adata.var_names = gene_names
adata.var['gene_id'] = genes

# Standard preprocessing (see scanpy protocol for the full pipeline)
sc.pp.filter_cells(adata, min_genes=200)
sc.pp.filter_genes(adata, min_cells=3)
sc.pp.normalize_total(adata, target_sum=1e4)
sc.pp.log1p(adata)
```

### Seurat / R

```r
library(Seurat)
library(Matrix)

mtx <- readMM("output_dir/counts_unfiltered/cells_x_genes.mtx")
barcodes <- readLines("output_dir/counts_unfiltered/cells_x_genes.barcodes.txt")
genes    <- readLines("output_dir/counts_unfiltered/cells_x_genes.genes.names.txt")

# kb writes cells × genes; Seurat expects genes × cells — transpose
mtx <- t(mtx)
rownames(mtx) <- genes
colnames(mtx) <- barcodes
mtx <- as(mtx, "CsparseMatrix")

obj <- CreateSeuratObject(counts = mtx, min.cells = 3, min.features = 200)
# Now standard Seurat from here — see seurat protocol
```

### SingleCellExperiment / R

```r
library(SingleCellExperiment)
library(Matrix)

mtx <- t(readMM("output_dir/counts_unfiltered/cells_x_genes.mtx"))
rownames(mtx) <- readLines("output_dir/counts_unfiltered/cells_x_genes.genes.names.txt")
colnames(mtx) <- readLines("output_dir/counts_unfiltered/cells_x_genes.barcodes.txt")

sce <- SingleCellExperiment(assays = list(counts = mtx))
# Now standard SCE pipeline — see singlecellexperiment protocol
```

Source: [kb → scanpy notebook](https://kallisto.readthedocs.io/en/latest/sc/notebooks/scanpy.html).

## nac Workflow — Loading Nascent + Mature Counts

When you ran `kb count --workflow nac`, the output has BOTH a mature (`cdna`) and nascent matrix:

```python
import scanpy as sc

# Each matrix is loaded separately
mature  = sc.read_mtx('output_dir/counts_unfiltered/cells_x_genes.mature.mtx')
nascent = sc.read_mtx('output_dir/counts_unfiltered/cells_x_genes.nascent.mtx')

# For RNA velocity, stash both as layers of one AnnData
import anndata
adata = anndata.AnnData(
    X      = mature.X,
    layers = {'spliced': mature.X, 'unspliced': nascent.X},
    obs    = mature.obs,
    var    = mature.var,
)
# Then pass to scVelo (see scvelo protocol)
```

For snRNA-seq (no velocity, just better gene counts), **sum spliced + unspliced** before downstream — nuclei reads come from both.

## Multi-Sample Batch Processing

```bash
# Create a batch file: SAMPLE_ID<TAB>R1_PATH<TAB>R2_PATH per row
cat > samples.txt <<EOF
sample1	/data/s1_R1.fastq.gz	/data/s1_R2.fastq.gz
sample2	/data/s2_R1.fastq.gz	/data/s2_R2.fastq.gz
sample3	/data/s3_R1.fastq.gz	/data/s3_R2.fastq.gz
EOF

# Run all samples in one kb count call — preserves sample identity
kb count -i index.idx -g t2g.txt -x 10xv3 -o multi_out --h5ad \
    --batch-barcodes \
    --tcc \
    batch.txt
```

Each cell's barcode gets prefixed with its sample ID, so you can split later.

## Key Parameters to Adjust

### `kb ref`
- `--workflow` — `standard` (default), `nac` (nascent+mature for snRNA-seq + velocity), `lamanno` (legacy velocity layout)
- Memory: index build for human/mouse needs ~16 GB; use a smaller `k-mer` (default 31) only for tiny references

### `kb count`
- `-x <tech>` — must match your library chemistry exactly; use `kb --list` to check
- `--workflow` — must match the `kb ref` workflow
- `--h5ad` — recommended; writes a ready-to-load AnnData
- `--filter bustools` — applies bustools knee-plot cell calling (optional)
- `-t` — threads; one per core, kb scales near-linearly to ~8 threads
- `-m 8G` — RAM cap; bump if you see OOM errors

### Memory considerations
- Index files for human/mouse: ~3-5 GB. Keep them on local SSD for `kb count`.
- `kb count` peak RAM is roughly 1.5× the index size.
- For atlas-scale runs (1000+ samples), build the index **once** and reuse across all `kb count` jobs.

## Best Practices

- **Build the index once, reuse forever.** It's deterministic — same FASTA + GTF + workflow gives the same index. Stash it on shared storage.
- **Use `--h5ad`.** Saves writing the awkward MTX → AnnData glue code. Works for `scanpy` and converts cleanly to Seurat / SCE.
- **Don't filter cells in `kb count`.** Skip `--filter bustools` for most analyses — use scanpy `min_genes=200` + `min_cells=3` downstream, or CellBender for ambient-aware filtering.
- **For snRNA-seq, ALWAYS use `--workflow nac`.** Without nascent indexing, you throw away 30-70% of reads (the introns).
- **Check `run_info.json` after every run.** Pseudoalignment rate < 70% means your reference doesn't match the data well — check species, gene model version.
- **Multi-sample: use `--batch-barcodes` + a samples file.** One `kb count` call across many samples is much faster than parallel jobs because the index loads once.
- **Reproducibility: pin kb-python + kallisto + bustools versions** with `kb --version`. Index files can subtly change across versions.

## Common Pitfalls

- **Mismatched `kb ref` and `kb count` workflows.** `kb ref --workflow nac` + `kb count` without `--workflow nac` silently produces wrong results.
- **Forgetting `-x` chemistry.** Default is 10x v3; using it on v2 / Drop-seq data fails silently (most barcodes won't correct).
- **Confusing R1 (barcode + UMI) with R2 (cDNA).** Most chemistries are R1=barcode, R2=cDNA. The order in the command matters.
- **Using filtered_feature_bc_matrix output style as input to `kb`.** kb takes raw FASTQs; it's the upstream step. cellranger's matrix outputs aren't compatible.
- **Index built on wrong gene model.** If counts look 30% lower than expected from cellranger, the GTF probably annotates fewer transcripts. Use the **same** GTF cellranger uses (typically GENCODE or Ensembl primary assembly) for fair comparison.

## End-to-End Template

`assets/kallisto_template.sh` — single parameterized script. Edit FASTQ paths, technology, and reference paths, then run end-to-end ref → count → handoff message.

## Convenience Scripts

- `scripts/kb_ref.sh` — index builder (standard or `nac` workflow)
- `scripts/kb_count.sh` — per-sample quantifier

For downstream analysis, switch to one of:
- [`scanpy`](../scanpy/SKILL.md) — Python downstream
- [`seurat`](../seurat/SKILL.md) — R / Seurat downstream
- [`singlecellexperiment`](../singlecellexperiment/SKILL.md) — R / Bioconductor downstream
- [`cellbender`](../cellbender/SKILL.md) — ambient-aware cell calling on the unfiltered matrix
- [`scvelo`](../scvelo/SKILL.md) — RNA velocity if you used `--workflow nac` or `lamanno`

## References

- [kallisto docs](https://kallisto.readthedocs.io/) — Pachter Lab
- [Introduction](https://kallisto.readthedocs.io/en/latest/overview/introduction.html)
- [Installation](https://kallisto.readthedocs.io/en/latest/quick_start/installation.html)
- [Index generation](https://kallisto.readthedocs.io/en/latest/index/index_generation.html)
- [Single-cell quantification](https://kallisto.readthedocs.io/en/latest/sc/pseudoalignment.html)
- [scanpy notebook tutorial](https://kallisto.readthedocs.io/en/latest/sc/notebooks/scanpy.html)
- Bray et al. (2016), *Near-optimal probabilistic RNA-seq quantification*, *Nature Biotechnology* (original kallisto paper)
- Melsted et al. (2021), *Modular, efficient and constant-memory single-cell RNA-seq preprocessing*, *Nature Biotechnology* (kb-python paper)
- Sullivan et al. (2024), *kallisto, bustools, and kb-python for quantifying bulk, single-cell, and single-nucleus RNA-seq*, *Nature Protocols*
