---
name: snapatac2
description: Single-cell ATAC-seq analysis with SnapATAC2 (scverse). Covers the full pipeline — fragment import, TSS enrichment QC, tile-matrix construction, doublet filtering, spectral embedding, UMAP/leiden, MACS3 peak calling, gene activity matrices, differentially accessible regions, multi-sample integration via Harmony / MNN-correct, and cell-type annotation via scRNA-seq reference (SCANVI label transfer) or marker-based gene activity. Built on AnnData; interoperates with scanpy.
license: BSD-3-Clause
metadata:
---

# SnapATAC2: Single-Cell ATAC-seq Analysis

## Overview

[SnapATAC2](https://scverse.org/SnapATAC2/) is the scverse-native Python rewrite of the original R SnapATAC package, designed for scalable scATAC-seq and snATAC-seq analysis. The compute-heavy paths are written in Rust, so it handles atlas-scale datasets (millions of cells) on modest hardware. It operates on AnnData objects and interoperates with scanpy, anndata, and the rest of the scverse stack.

The core pipeline: **fragments → cells → tile matrix → spectral → clusters → peaks → biology**. Five distinct downstream branches all share the same backbone:

1. **Standard analysis** (single sample) — QC, clustering, gene activity, marker peaks
2. **Multi-sample integration** — Harmony / MNN-correct across donors / batches
3. **Multi-modality** — joint analysis of paired ATAC + RNA (multiome)
4. **Atlas-scale** — millions of cells with chunked processing
5. **Annotation** — label transfer from a scRNA-seq reference via SCANVI, or manual annotation via gene activity scores

## When to Use This Skill

- Analyzing scATAC-seq or snATAC-seq fragment files (`.tsv.gz` from cellranger-atac, `.bed.gz`, etc.)
- Multi-sample integration of ATAC datasets (treatment vs control, multiple donors)
- Calling peaks at the cluster level with MACS3
- Identifying differentially accessible regions (DARs) between cell populations
- Annotating ATAC clusters using a matched scRNA-seq reference (label transfer)
- Going from a fragment file all the way to a final h5ad with cell-type labels + peaks + gene activity

**Not for**: bulk ATAC-seq (use vanilla MACS2/MACS3), CUT&RUN/CUT&Tag (specialised tools exist), differential motif analysis (use chromVAR or `pycisTopic`), trajectory analysis (use scVelo / PAGA on the spectral embedding).

## Prerequisites

- Python 3.9+ (3.10 / 3.11 recommended)
- Linux or macOS x86_64 (precompiled binaries available); Apple Silicon and Windows require source build
- 8 GB+ RAM for typical 10X experiments; 32 GB+ for multi-sample / atlas runs
- Fragment file(s) — sorted/bgzipped or unsorted (`sorted_by_barcode=False`)

### Installation

```bash
pip install snapatac2                    # core
pip install 'snapatac2[recommend]'       # + harmonypy, scanorama, xgboost (recommended)
```

If precompiled wheels aren't available for your platform (Apple Silicon, exotic Linux), install build deps first:
```bash
# Rust + cmake
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
brew install cmake hdf5                  # macOS — also: apt install cmake libhdf5-dev on Linux
pip install snapatac2
```

For SCANVI-based label transfer (annotation tutorial), additionally:
```bash
pip install 'scvi-tools[scanpy]'
```

---

## Pipeline 1 — Standard Single-Sample Analysis

This is the foundation. Every other pipeline starts here.

### 1. Import fragments and compute basic QC

```python
import snapatac2 as snap

data = snap.pp.import_fragments(
    "fragments.tsv.gz",
    chrom_sizes=snap.genome.hg38,        # or .mm10, .hg19, .GRCh38 etc.
    file="sample.h5ad",                   # backed mode — written to disk
    sorted_by_barcode=False,
    min_num_fragments=200,                # drop barcodes below this floor
)

# TSS enrichment — the standard scATAC quality metric.
# Higher = more cell-like signal (open promoters); lower = empty/dead droplets.
snap.metrics.tsse(data, snap.genome.hg38)

# Visualize the TSS-enrichment × fragments knee plot
snap.pl.tsse(data, interactive=False, out_file="figures/tsse_knee.pdf")
```

After this, `data.obs` has: `n_fragment`, `frac_dup`, `frac_mito`, `tsse`.

### 2. Filter cells

```python
snap.pp.filter_cells(
    data,
    min_counts=1000,
    min_tsse=7,                # canonical cut; raise to 10 for stringent data
    max_counts=100000,         # cap top end — likely doublets
)
```

The knee on `tsse` from the previous step tells you where to cut.

### 3. Tile matrix — cells × genome bins

ATAC data starts unstructured. SnapATAC2's tile matrix bins the genome into fixed windows (default 500 bp, often bumped to 5 kb for memory):

```python
snap.pp.add_tile_matrix(data, bin_size=5000)
```

After this, the AnnData `X` is a cell × bin sparse matrix. `data.var` has the bin coordinates.

### 4. Select informative features

```python
snap.pp.select_features(data, n_features=50000)
```

Picks the most variable / informative bins. The output is stored in `data.var['selected']`.

### 5. Doublet detection

```python
snap.pp.scrublet(data)
snap.pp.filter_doublets(data)
```

Scrublet is run on the binarized accessibility — same principle as scRNA but operates on bins. Adds `data.obs['doublet_score']` and `data.obs['is_doublet']`.

### 6. Spectral embedding

SnapATAC2's headline algorithm: a scalable spectral (Laplacian eigenmap) decomposition that scales to millions of cells via random projections.

```python
snap.tl.spectral(data, n_comps=50)
```

Result: `data.obsm['X_spectral']` is the cell × component matrix. This is the analog of PCA on RNA — every downstream step uses it.

### 7. KNN graph + UMAP + leiden

```python
snap.pp.knn(data, use_rep="X_spectral", n_neighbors=50)
snap.tl.umap(data, use_rep="X_spectral")
snap.tl.leiden(data, resolution=1.0)
```

Standard. Pass `use_rep="X_spectral_harmony"` after running Harmony (next pipeline).

### 8. Peak calling per cluster — MACS3

```python
snap.tl.macs3(
    data,
    groupby='leiden',                    # one peak set per cluster
    replicate=None,                       # set to the sample column for multi-sample
)

# Result lives in data.uns['macs3'] as a dict: {cluster_id: peak_dataframe}
```

### 9. Merge peaks across clusters

```python
merged_peaks = snap.tl.merge_peaks(data.uns['macs3'], chrom_sizes=snap.genome.hg38)
# merged_peaks is a polars DataFrame with the union of cluster-specific peaks
```

### 10. Cell × peak matrix

The tile matrix (step 3) is too coarse for biological interpretation. The cell × peak matrix uses the merged peak set:

```python
peak_mat = snap.pp.make_peak_matrix(data, use_rep=merged_peaks)
# peak_mat is a separate AnnData — cells × peaks, sparse
```

### 11. Differentially accessible regions (DARs)

```python
# Find peaks specific to one cluster vs the rest
dars = snap.tl.diff_test(
    peak_mat,
    cell_group1=peak_mat.obs.leiden == '5',
    cell_group2=peak_mat.obs.leiden != '5',
    direction='positive',                # 'positive', 'negative', or 'both'
)
# Returns a polars DataFrame: peak, log2(fc), p-value, FDR
```

### 12. Gene activity matrix — bridge to RNA-seq

```python
gene_mat = snap.pp.make_gene_matrix(data, gene_anno=snap.genome.hg38)
# gene_mat is a new AnnData — cells × genes
# X is the per-gene aggregated accessibility (TSS + gene body + extensions)
```

Once you have the gene matrix, you can use scanpy for marker analysis:

```python
import scanpy as sc
sc.pp.normalize_total(gene_mat); sc.pp.log1p(gene_mat)
sc.tl.rank_genes_groups(gene_mat, groupby='leiden', method='wilcoxon')
sc.pl.rank_genes_groups(gene_mat, n_genes=10)
```

Convenience: `python scripts/snapatac_standard.py --fragments fragments.tsv.gz --genome hg38 --out sample.h5ad`. See [references/multi_sample.md](references/multi_sample.md) for multi-sample, [references/annotation.md](references/annotation.md) for label transfer.

Source: [Standard pipeline tutorial](https://scverse.org/SnapATAC2/tutorials/pbmc.html) and [Identify DARs tutorial](https://scverse.org/SnapATAC2/tutorials/index.html).

---

## Pipeline 2 — Multi-Sample Integration

When you have multiple donors / conditions / batches and want a unified embedding. Two batch-correction methods supported: **Harmony** (more common) and **MNN-correct** (more robust on extreme batch shifts).

### 1. Import all fragments in one shot

```python
import snapatac2 as snap

files = snap.datasets.colon()             # demo: 5 colon snATAC samples

adatas = snap.pp.import_fragments(
    [fragment_path for _, fragment_path in files],
    file=[f"{name}.h5ad" for name, _ in files],   # one file per sample
    chrom_sizes=snap.genome.hg38,
    min_num_fragments=1000,
)
```

### 2. Run QC + features per sample (one call, vectorized)

```python
snap.metrics.tsse(adatas, snap.genome.hg38)
snap.pp.filter_cells(adatas, min_tsse=7)
snap.pp.add_tile_matrix(adatas, bin_size=5000)
snap.pp.select_features(adatas, n_features=50000)
snap.pp.scrublet(adatas)
snap.pp.filter_doublets(adatas)
```

Each call iterates internally across all samples; no Python loop needed.

### 3. Combine into an `AnnDataSet`

This is the SnapATAC2-specific container — a "view" over per-sample h5ad files that exposes them as one AnnData. Cells from different samples get prefixed barcodes so collisions don't matter.

```python
data = snap.AnnDataSet(
    adatas=[(name, ad) for (name, _), ad in zip(files, adatas)],
    filename="colon.h5ads",
)
# Result: AnnDataSet object with n_obs x n_vars = 41785 x 606219
#         (5 samples, 41785 cells total)
```

### 4. Spectral + UMAP (raw, pre-correction)

```python
snap.pp.select_features(data, n_features=50000)
snap.tl.spectral(data)
snap.tl.umap(data)
```

Inspect the UMAP — donor/batch usually dominates. That's what the next step fixes.

### 5. Batch correction — pick one

```python
# Option A: MNN-correct (mutual nearest neighbors — slower, more robust)
snap.pp.mnc_correct(data, batch="sample")
# Sets data.obsm['X_spectral_mnn']

# Option B: Harmony (faster, more common; needs harmonypy installed)
snap.pp.harmony(data, batch="sample", max_iter_harmony=20)
# Sets data.obsm['X_spectral_harmony']
```

You can run both and compare.

### 6. UMAP + clustering on the corrected embedding

```python
snap.tl.umap(data, use_rep="X_spectral_harmony")     # or X_spectral_mnn
snap.pp.knn(data, use_rep="X_spectral_harmony")
snap.tl.leiden(data, resolution=1.0)
```

### 7. Per-cluster peak calling, per-sample replicates

```python
snap.tl.macs3(data, groupby='leiden', replicate='sample')
merged_peaks = snap.tl.merge_peaks(data.uns['macs3'], chrom_sizes=snap.genome.hg38)
```

Specifying `replicate='sample'` tells MACS3 to treat each sample as a biological replicate within each cluster — yields more conservative, reproducible peaks.

### 8. Persist + reopen

```python
# The AnnDataSet persists automatically to colon.h5ads
# In a later session:
data = snap.read_dataset("colon.h5ads")
```

Convenience: `python scripts/snapatac_multi_sample.py --samples sample_paths.txt --genome hg38 --batch-correct harmony --out colon.h5ads`.

Source: [Integration tutorial](https://scverse.org/SnapATAC2/tutorials/integration.html).

---

## Pipeline 3 — Cell-Type Annotation

Two complementary approaches:

### 3a. Manual — gene activity + canonical markers

```python
# Build gene activity matrix from the standard pipeline output
gene_mat = snap.pp.make_gene_matrix(data, gene_anno=snap.genome.hg38)

import scanpy as sc
sc.pp.normalize_total(gene_mat); sc.pp.log1p(gene_mat)

# Score canonical markers (T cells, B cells, monocytes, etc.)
sc.tl.score_genes(gene_mat, gene_list=['CD3D', 'CD3E', 'CD3G', 'TRAC'], score_name='T_score')
sc.tl.score_genes(gene_mat, gene_list=['MS4A1', 'CD79A', 'CD79B'],      score_name='B_score')
sc.tl.score_genes(gene_mat, gene_list=['CD14', 'LYZ', 'S100A8'],        score_name='Mono_score')

# Per-cluster mean of each score
gene_mat.obs.groupby('leiden')[['T_score', 'B_score', 'Mono_score']].mean()

# Assign cluster → cell type from the scores, then write back to the ATAC object
data.obs['cell_type'] = data.obs['leiden'].map({
    '0': 'T_CD4', '1': 'T_CD8', '2': 'B', '3': 'Mono', ...
})
```

### 3b. Automated — SCANVI label transfer from scRNA-seq

When you have a matched scRNA-seq reference (with cell-type labels), train a joint embedding and predict labels for the ATAC cells.

```python
import scanpy as sc
import anndata as ad
import scvi

# Build ATAC gene-activity matrix
query = snap.pp.make_gene_matrix(atac, gene_anno=snap.genome.hg38)
query.obs['batch']   = 'ATAC'
query.obs['celltype_scanvi'] = 'Unknown'        # placeholder for unlabelled

# Load and tag the RNA reference
reference = sc.read_h5ad("rna_reference.h5ad")
reference.obs['batch'] = 'RNA'
# reference.obs['celltype_scanvi'] already has labels

# Merge on shared genes
data = ad.concat([reference, query], join='inner', label='batch_origin')

# Standard HVG / log-norm prep
sc.pp.normalize_total(data); sc.pp.log1p(data)
sc.pp.highly_variable_genes(data, n_top_genes=4000, batch_key='batch')

# Train scVI then SCANVI for label transfer
scvi.model.SCVI.setup_anndata(data, batch_key="batch")
vae  = scvi.model.SCVI(data, n_layers=2, n_latent=30)
vae.train()

lvae = scvi.model.SCANVI.from_scvi_model(
    vae, adata=data,
    labels_key="celltype_scanvi",
    unlabeled_category="Unknown",
)
lvae.train()
data.obs["C_scANVI"] = lvae.predict(data)

# Map predicted labels back to the ATAC AnnData
atac.obs['celltype_predicted'] = data[data.obs['batch'] == 'ATAC'].obs['C_scANVI'].values
```

See [references/annotation.md](references/annotation.md) for confidence scoring, conflict resolution between SCANVI predictions and leiden clusters, and how to handle ATAC cell types absent from the RNA reference (e.g. very rare populations).

Source: [Annotation tutorial](https://scverse.org/SnapATAC2/tutorials/annotation.html).

---

## Pipeline 4 — Differentially Accessible Regions (DARs)

After clustering and per-cluster peak calling, find peaks specific to one cell type:

```python
# Build the peak matrix (cells × peaks, sparse)
peak_mat = snap.pp.make_peak_matrix(data, use_rep=merged_peaks)

# DARs of cluster 5 (e.g. CD8 T cells) vs all other cells
dars_pos = snap.tl.diff_test(
    peak_mat,
    cell_group1=peak_mat.obs.leiden == '5',
    cell_group2=peak_mat.obs.leiden != '5',
    direction='positive',
)

# Filter by significance
sig_peaks = dars_pos.filter(
    (dars_pos['adjusted p-value'] < 0.01) & (dars_pos['log2(fc)'] > 1)
)

# Save to BED for downstream motif analysis (HOMER, MEME, etc.)
sig_peaks.to_pandas()[['chrom', 'start', 'end']].to_csv(
    'CD8_dars.bed', sep='\t', header=False, index=False
)
```

`direction='positive'` finds peaks ENRICHED in group1; `'negative'` finds peaks DEPLETED in group1; `'both'` returns all changing peaks.

For across-condition DARs (e.g. tumor vs adjacent normal within the same cell type), filter the peak matrix to one cluster first then run `diff_test` between conditions.

Source: [DAR tutorial](https://scverse.org/SnapATAC2/tutorials/index.html) (3rd tutorial in the list).

---

## Visualization Cookbook

```python
# QC plots
snap.pl.tsse(data, interactive=False)
snap.pl.frag_size_distr(data, interactive=False)

# UMAP — leiden clusters
snap.pl.umap(data, color='leiden')

# UMAP — sample (batch sanity check)
snap.pl.umap(data, color='sample')

# UMAP — gene activity of a marker
gene_mat = snap.pp.make_gene_matrix(data, gene_anno=snap.genome.hg38)
sc.pl.umap(gene_mat, color=['CD3D', 'MS4A1', 'CD14'])

# Peak coverage at a locus — like a Genome Browser snapshot
snap.pl.regions(data, groupby='leiden', regions=['chr1:1000000-1010000'])
```

---

## Key Parameters to Adjust

### Tile size — `add_tile_matrix(bin_size=...)`
- **500 bp** (default): finest resolution, biggest matrix
- **5,000 bp** (5 kb): standard for first-pass analysis; 10× faster
- **10,000 bp**: very large datasets only

### Spectral components — `tl.spectral(n_comps=...)`
- **50** (default): plenty for most datasets
- **30** for very small datasets (< 5k cells)
- **100** for atlas-scale (millions of cells)

### KNN neighbors — `pp.knn(n_neighbors=...)`
- **50** (default for ATAC): higher than RNA's typical 15, because ATAC noise is higher

### Leiden resolution — `tl.leiden(resolution=...)`
- **1.0** (default): coarse-grained
- **0.5**: fewer clusters (often what you want for cell-type assignment)
- **2.0+**: very fine sub-clusters (cell states / niches)

### MACS3 — `tl.macs3(replicate=...)`
- **None** for single-sample
- **sample column** for multi-sample — treats each sample as a replicate. Recommended for any multi-sample work.

---

## Best Practices

- **Use the backed h5ad mode.** Pass `file="sample.h5ad"` to `import_fragments` — keeps the working memory low and lets you resume from disk.
- **Inspect the TSS-enrichment knee.** It's the canonical scATAC QC. Cells with tsse < 7 are usually dead/empty. Higher data quality → higher cutoff (10-15 is reasonable for fresh PBMC).
- **5 kb bins by default; 500 bp only when you need it.** The default 500 bp matrix is huge and rarely changes downstream conclusions for clustering. Bump down to 500 bp only after you've decided which regions / cell types matter.
- **For peak calling, always provide `replicate=sample` in multi-sample.** Otherwise MACS3 treats all cells in a cluster as one pool, inflating power and getting non-reproducible peaks.
- **Don't bin-feature-select after batch correction.** Run `select_features` first, then spectral, then Harmony / MNN. Reversing this throws away the batch-correction benefit on the bin layer.
- **Gene activity is not gene expression.** It's a proxy. Trust it for high-level cell-type marker calls; don't trust it for fine-grained DEG-like analyses.

---

## End-to-End Template

`assets/snapatac_template.py` — single parameterized script. Set fragment file(s), genome, batch correction method, then run all of Pipelines 1+2+3.

## Convenience Scripts

- `scripts/snapatac_standard.py` — Pipeline 1 (single-sample)
- `scripts/snapatac_multi_sample.py` — Pipeline 2 (multi-sample integration with Harmony or MNN)

---

## References

- [SnapATAC2 docs](https://scverse.org/SnapATAC2/)
- [Installation](https://scverse.org/SnapATAC2/install.html)
- [Tutorials index](https://scverse.org/SnapATAC2/tutorials/index.html)
- [Integration](https://scverse.org/SnapATAC2/tutorials/integration.html)
- [Annotation](https://scverse.org/SnapATAC2/tutorials/annotation.html)
- Zhang et al. (2024), *Fast, scalable and versatile open-source analysis of single-cell ATAC-seq data with SnapATAC2*, *Nature Methods*
