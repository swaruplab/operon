# Recipe: ATAC-seq peak calling

The full chromatin-accessibility pipeline: trim → align → MACS2 → QC →
motif enrichment.

## What you'll build

- Trimmed and aligned paired-end ATAC-seq data (`<sample>.filtered.bam`)
- Called peaks (`<sample>_peaks.narrowPeak` and `_peaks.broadPeak`)
- A FRiP / TSS enrichment QC report
- A consensus peak set across samples
- A peak × sample count matrix (input for DiffBind / DESeq2 on ATAC)
- JASPAR motif enrichment for the consensus peaks

## Inputs

- Paired-end FASTQ files per sample
- A reference genome (e.g. GRCh38) with a Bowtie2 index pre-built — or
  point Operon at a download URL and it'll build the index

## Setup

Load **chromatin › ATAC-seq pipeline** + (optionally) **chromatin ›
JASPAR motif enrichment**.

This recipe assumes you're on HPC — ATAC alignment is CPU-heavy and
generally runs on a compute node. See [HPC mode](../hpc/index.md) for
SSH + tmux setup.

## Step 1 — Plan

**Plan** mode:

> *I have paired-end ATAC-seq FASTQs in `fastq/` (samples sample1_R1.fastq.gz,
> sample1_R2.fastq.gz, etc.). The Bowtie2 index for GRCh38 is at
> `/scratch/references/grch38/bowtie2/`. Run the full ATAC pipeline:
> Trim Galore for adapter trimming, Bowtie2 alignment with sensitive
> settings, filtering (remove duplicates, low-quality, mito, multi-mappers),
> MACS2 narrow + broad peak calling, FRiP + TSS enrichment QC, and
> consensus peaks across samples. Generate a Snakemake workflow so I can
> rerun parts of it.*

Plan should specify:

- **Adapter sequence** — auto-detect with Trim Galore, or explicit
  Nextera adapters
- **Bowtie2 flags** — `--very-sensitive -X 2000 --no-mixed --no-discordant`
- **Duplicate removal** — Picard MarkDuplicates
- **Mito removal** — drop reads on `chrM`
- **Quality filter** — `samtools view -q 30 -F 1804`
- **MACS2 params** — `--nomodel --shift -100 --extsize 200 -B --SPMR -g hs`
  for narrow; `--broad` flag for broad
- **FRiP threshold** — flag samples below 20%
- **TSS enrichment** — use ATAQV or deepTools computeMatrix

## Step 2 — Execute on the compute node

You're on an interactive SLURM session (see [SLURM](../hpc/slurm.md)).

**Agent** mode:

> *Plan looks good. Generate the Snakemake workflow and submit it as a
> SLURM cluster job (config.yaml with cluster-submit settings). Use
> 8 cores per sample, walltime 6h per sample for alignment, 1h for peak
> calling.*

Agent generates:

- `Snakefile` with rules for `trim`, `align`, `filter`, `peakcall_narrow`,
  `peakcall_broad`, `frip`, `tss`
- `config/cluster.yaml` with SLURM submission parameters per rule
- A submit script

Submit and Operon polls the queue.

## Step 3 — QC review

When alignment + peaks finish:

**Ask** mode:

> *Show me the FRiP and TSS enrichment scores for each sample. Flag any
> that look problematic.*

Claude reads the QC outputs, summarizes, and points out:

- Samples with FRiP < 20% (poor enrichment — possible failed library)
- TSS enrichment < 4 (suboptimal)
- Pct reads on `chrM` > 30% (mitochondrial contamination — common
  pre-Tn5 cleanup issue)
- Library complexity (NRF, PCR bottleneck coefficient)

Decide which samples to drop before consensus peak calling.

## Step 4 — Consensus peaks and counts matrix

**Agent**:

> *From the surviving samples, build a consensus peak set using IDR for
> narrow peaks (or merging for broad). Then count reads in each consensus
> peak × sample to make a counts matrix suitable for DiffBind / DESeq2.*

## Step 5 — Motif enrichment

If you loaded the motif protocol:

**Agent**:

> *Run JASPAR motif enrichment on the consensus narrow peaks vs a
> background of GC-matched random regions. Report top 20 enriched motifs.
> Plot the enrichment heatmap.*

## Variations

### scATAC-seq

Different protocol — **chromatin › scATAC-seq (ArchR)**. Single-cell
ATAC needs a totally different workflow (tile matrices, gene-score
inference) rather than bulk peak calling.

### CUT&RUN or CUT&Tag

Different protocol — **chromatin › CUT&RUN / CUT&Tag**. Lower input,
different peak callers (SEACR), spike-in normalization.

### Just narrow peaks, skip broad

Tell Plan: "Skip the broad peak calling — I'm focused on TF binding
proxies, not histone modifications."

### Different organism

Tell Plan: "Reference is mm10, not GRCh38. Use `-g mm` in MACS2 and
the mouse Bowtie2 index at `/scratch/references/mm10/`."

## Pitfalls

- **Adapter contamination** in Tn5 ATAC — always trim. The Tn5 transposase
  attaches Nextera adapters; if you skip trimming, ~5-10% of reads have
  adapter dimer.
- **`chrM` overrepresentation** — common with cells that have lots of
  mitochondria (cardiomyocytes, hepatocytes). Drop them after alignment.
- **PCR duplicates** — typical ATAC libraries have 20-40% duplicates.
  Above 60% suggests low complexity (overamplified library).
- **Insert size distribution** — should have a nucleosome ladder (sub-100bp
  for nucleosome-free, ~200bp mono-nucleosome, etc.). No ladder = failed
  library.
- **MACS2 `--nomodel` is required** — ATAC peaks don't have the same
  shape as ChIP, so MACS2's default modeling fails.
- **Reference contamination** — ATAC libraries sometimes have ~1% contam
  with another species (mycoplasma in cell culture). Aligners drop these
  silently. If your alignment rate is mysteriously low, check.

## Sanity checks

```bash
# Alignment rate — expect > 80% for good library
samtools flagstat <sample>.raw.bam

# Post-filter — typical retention is 30-50% of raw reads
samtools view -c <sample>.filtered.bam

# Fragment length distribution
samtools view <sample>.filtered.bam | awk '{print $9}' | abs | sort -n | uniq -c
# Should show: peak at 50-100bp (NFR), peak at 200bp (mono), shoulder at 400bp (di)

# FRiP
bedtools intersect -a peaks.narrowPeak -b reads.bed -c | awk '{sum+=$NF} END{print sum}'
# divide by total reads
```

## Next steps

- Differential accessibility with DiffBind or DESeq2 on the counts matrix
  ([bulk RNA-seq](bulk-rnaseq-deseq2.md) workflow translates directly)
- Protocol: **chromatin › ChIP-seq pipeline** for parallel ChIP samples
- Integrate with scRNA via gene scores (per-cell gene activity from ATAC
  peaks)
