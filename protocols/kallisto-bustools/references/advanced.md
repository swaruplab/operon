# Advanced kallisto-bustools Patterns

For scenarios beyond the standard single-sample 10X pipeline.

## nac vs lamanno — choosing the right velocity workflow

Two workflows both produce "nascent + mature" indexes, but they're not interchangeable.

| Property | `--workflow nac` (newer) | `--workflow lamanno` (legacy) |
|---|---|---|
| Index covers | Mature mRNA + intron-containing nascent | Mature mRNA + intron-only sequences |
| Output matrices | `cells_x_genes.mature.mtx` + `cells_x_genes.nascent.mtx` | `cells_x_genes.spliced.mtx` + `cells_x_genes.unspliced.mtx` |
| Read assignment | A read mapping to BOTH cdna and nascent counts toward BOTH (ambiguity-aware) | A read mapping to BOTH is split heuristically |
| Recommended for | snRNA-seq, modern RNA velocity (scVelo with `nascent` argument) | scVelo legacy mode (`spliced`/`unspliced` layers in scVelo's expected layout) |

**My recommendation**:
- New analyses: `--workflow nac`. It's the kb authors' current preferred workflow.
- Reproducing older papers: `--workflow lamanno` for compatibility with scVelo's legacy layout.

### Loading nac output into scVelo

scVelo expects `spliced` / `unspliced` layers in AnnData. The mapping:
- `nac` mature → `spliced` layer
- `nac` nascent → `unspliced` layer

```python
import anndata
import scanpy as sc

mature  = sc.read_mtx('out/counts_unfiltered/cells_x_genes.mature.mtx').T
nascent = sc.read_mtx('out/counts_unfiltered/cells_x_genes.nascent.mtx').T

# Same barcodes + genes for both
barcodes = open('out/counts_unfiltered/cells_x_genes.barcodes.txt').read().splitlines()
genes    = open('out/counts_unfiltered/cells_x_genes.genes.names.txt').read().splitlines()

adata = anndata.AnnData(
    X=mature.X,
    obs={'_': [''] * len(barcodes)},
    var={'_': [''] * len(genes)},
    layers={'spliced': mature.X, 'unspliced': nascent.X},
)
adata.obs_names = barcodes
adata.var_names = genes

# Pass to scVelo (see scvelo protocol)
import scvelo as scv
scv.pp.filter_and_normalize(adata)
scv.pp.moments(adata)
scv.tl.velocity(adata, mode='stochastic')
```

## Custom chemistries via seqspec

For non-standard layouts (in-house protocols, modified 10X, etc.), use seqspec to describe the barcode/UMI layout, then pass the seqspec file to `-x`:

```bash
# Write a seqspec.yaml describing your protocol
# (see github.com/pachterlab/seqspec for the schema)
kb count -i index.idx -g t2g.txt -x my_chemistry.yaml -o out --h5ad R1.fastq.gz R2.fastq.gz
```

This is how you handle, e.g., 10X with a custom barcode whitelist, BD Rhapsody panels, or in-house combinatorial-indexing protocols.

## Multi-sample with `--batch-barcodes`

For dozens-to-hundreds of samples, processing them in one kb call (rather than parallel cellranger jobs) is dramatically faster because the index loads once.

```bash
# Tab-separated samples file
cat > batch.txt <<EOF
sample1	/data/s1_L001_R1.fastq.gz	/data/s1_L001_R2.fastq.gz
sample1	/data/s1_L002_R1.fastq.gz	/data/s1_L002_R2.fastq.gz
sample2	/data/s2_L001_R1.fastq.gz	/data/s2_L001_R2.fastq.gz
sample3	/data/s3_L001_R1.fastq.gz	/data/s3_L001_R2.fastq.gz
EOF

kb count \
    -i index.idx -g t2g.txt -x 10xv3 \
    -o multi_out \
    --h5ad \
    --batch-barcodes \
    batch.txt
```

The output is one combined `.h5ad` where each cell's barcode is `<sample_id>_<original_barcode>`. Split downstream:

```python
adata = anndata.read_h5ad('multi_out/counts_unfiltered/adata.h5ad')
adata.obs['sample'] = adata.obs_names.str.split('_').str[0]

# Process all together or per-sample
for sample, sub in adata.obs.groupby('sample'):
    sub_ad = adata[sub.index].copy()
    # ...
```

## Alternative output formats

By default `kb count` writes `cells_x_genes.mtx`. Other flags:

```bash
kb count ... --h5ad                # write AnnData (.h5ad) — scanpy-ready
kb count ... --loom                # write Loom format
kb count ... --tcc                 # transcript compatibility counts (sub-isoform resolution)
```

`--tcc` is useful for isoform-level analyses (alternative splicing in scRNA-seq) — emits per-cell counts at the level of equivalence classes rather than collapsing to gene. Read the bustools docs before using; downstream tooling is sparse.

## Long-read kallisto (lr-kallisto)

For long-read (PacBio, ONT) scRNA-seq, use the lr-kallisto subcommand:

```bash
kb count \
    -i index.idx -g t2g.txt \
    -x 10xv3 \
    -o long_out --h5ad \
    --long-reads \
    long_reads.fastq.gz
```

This is a separate algorithm tuned for long reads. Use it for Pacbio MAS-seq / ONT R10.

## Equivalence class transcript compatibility (TCC)

For more advanced analyses where you want **sub-isoform** resolution, run with `--tcc`:

```bash
kb count -i index.idx -g t2g.txt -x 10xv3 -o out --tcc R1.fastq.gz R2.fastq.gz
```

Output is cells × equivalence-classes instead of cells × genes. Each equivalence class is a set of transcripts indistinguishable by the reads. Used by:

- DESeq2-TCC for isoform-aware DE
- Custom isoform-quantification pipelines
- BUStools' `bustools quant` for downstream collapsing

Most users don't need this — gene-level is fine for clustering / annotation.

## Atlas-scale: index reuse + parallel `kb count`

```bash
# Build the index once on a shared filesystem
kb ref -d human -i /shared/refs/human_v44/index.idx -g /shared/refs/human_v44/t2g.txt

# Process many samples in parallel jobs (SLURM array, etc.)
# Each job points at the shared index — no rebuild
for sample in s001 s002 s003 ... s500; do
    sbatch --wrap "kb count -i /shared/refs/human_v44/index.idx \
                            -g /shared/refs/human_v44/t2g.txt \
                            -x 10xv3 -o /output/$sample --h5ad \
                            /fastq/$sample_R1.fastq.gz /fastq/$sample_R2.fastq.gz"
done
```

The index file is read-only during `kb count`, so any number of parallel jobs can share it. Hosts shared NFS or local-SSD copies on each compute node — local SSD is dramatically faster for the I/O.

## Reference choice — Ensembl vs GENCODE vs RefSeq

Same genome (e.g. GRCh38), different annotations give noticeably different counts.

| Source | Why pick it |
|---|---|
| GENCODE (Comprehensive) | Most transcript isoforms; standard for cellranger comparisons |
| Ensembl | Same as GENCODE basic; smaller alternate-haplotype overhead |
| RefSeq | Stricter manual curation; fewer transcripts; better for canonical isoform |

If you're comparing to a published cellranger result, **use the GENCODE version they used** (check the cellranger reference release notes). Otherwise GENCODE v44+ for human, vM33+ for mouse is a sane default.

## Verifying the index is good

After `kb ref`, do a small sanity check:

```bash
# A test FASTQ pair from a known sample
kb count -i index.idx -g t2g.txt -x 10xv3 -o test_out test_R1.fastq.gz test_R2.fastq.gz

# Inspect alignment rate
cat test_out/run_info.json | python -c "import json,sys; d=json.load(sys.stdin); print(f'p_pseudoaligned: {d[\"p_pseudoaligned\"]:.1f}%')"
# Expect ≥ 70% for good library × correct index
```

Pseudoalignment rate < 70% usually means:
- Wrong genome (e.g. human FASTQ vs mouse index)
- Wrong gene model version (transcript IDs changed between releases)
- Heavy ribosomal/mitochondrial contamination (low-quality library, not an index issue)

## `kb count` output JSON files

Two JSONs worth reading after every run:

```python
import json
with open('out/run_info.json') as f: ri = json.load(f)
print(f"n_processed:       {ri['n_processed']:,}")
print(f"n_pseudoaligned:   {ri['n_pseudoaligned']:,}")
print(f"p_pseudoaligned:   {ri['p_pseudoaligned']:.2f}%")

with open('out/inspect.json') as f: ij = json.load(f)
print(f"n_barcodes:        {ij['numBarcodes']:,}")
print(f"n_reads:           {ij['numReads']:,}")
print(f"mean reads/barcode: {ij['meanReadsPerBarcode']:.1f}")
```

Use these for QC dashboards across many samples — far cheaper than running the full pipeline on each one.
