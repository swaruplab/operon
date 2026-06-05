# Multi-Sample Integration — Deep Dive

When you have ≥ 2 samples (donors, conditions, batches) and want a unified embedding for joint clustering. This guide covers the full path: importing many fragment files, the `AnnDataSet` container, batch correction with Harmony or MNN-correct, and per-sample peak calling with replicates.

## The `AnnDataSet` container

SnapATAC2's solution to "many h5ad files, one analysis." It's a backed view: each sample stays in its own h5ad on disk, but you operate on them as if they were one AnnData.

```python
data = snap.AnnDataSet(
    adatas=[(name, anndata_obj), ...],
    filename="combined.h5ads",
)
```

| Property | What it does |
|---|---|
| Lazy loading | Only the chunks you query get pulled into memory |
| Barcode prefixing | Cell barcodes get prefixed with sample name — no collisions |
| Same API as AnnData | `data.obs`, `data.obsm`, `data.X` all work |
| Persistence | The `.h5ads` file is the canonical store — reopen with `snap.read_dataset()` |

Trade-off: some operations that scan the full matrix (e.g. computing a full bin × bin correlation) are slower than on a single backed AnnData. For typical pipelines this isn't a bottleneck.

## Step 1 — Import

```python
files = [
    ("ctrl_d1", "/data/ctrl_d1_fragments.tsv.gz"),
    ("ctrl_d2", "/data/ctrl_d2_fragments.tsv.gz"),
    ("dis_d1",  "/data/dis_d1_fragments.tsv.gz"),
    ("dis_d2",  "/data/dis_d2_fragments.tsv.gz"),
]

adatas = snap.pp.import_fragments(
    [fl for _, fl in files],
    file=[f"per_sample/{name}.h5ad" for name, _ in files],
    chrom_sizes=snap.genome.hg38,
    min_num_fragments=1000,
    sorted_by_barcode=False,
)
```

Each call to `import_fragments` with a list of fragment files writes one h5ad per sample. Pass a list of filenames to `file=` so each ends up on disk independently. This makes resumption trivial: if step 4 of 12 fails, you don't re-import.

## Step 2 — Per-sample QC + features (vectorized)

```python
snap.metrics.tsse(adatas, snap.genome.hg38)
snap.pp.filter_cells(adatas, min_tsse=7, min_counts=1000, max_counts=100000)
snap.pp.add_tile_matrix(adatas, bin_size=5000)
snap.pp.select_features(adatas, n_features=50000)
snap.pp.scrublet(adatas)
snap.pp.filter_doublets(adatas)
```

These all accept a list of AnnDatas (or an `AnnDataSet`) and iterate internally. Each step's effect is per-sample — important because doublet rates, QC distributions, and informative features differ by sample.

## Step 3 — Combine

```python
data = snap.AnnDataSet(
    adatas=[(name, ad) for (name, _), ad in zip(files, adatas)],
    filename="combined.h5ads",
)
# data.shape == (sum_of_cells, max_n_features)
```

Sanity check:
```python
print(data)
# AnnDataSet object with n_obs x n_vars = 41785 x 606219
print(data.obs.groupby('sample').size())
# ctrl_d1    9420
# ctrl_d2   10117
# dis_d1     8856
# dis_d2    13392
```

## Step 4 — Re-select features on the combined set

The per-sample feature selection in Step 2 is a starting point. After combining, run feature selection on the joint set so all samples share the same feature space:

```python
snap.pp.select_features(data, n_features=50000)
```

## Step 5 — Spectral embedding

```python
snap.tl.spectral(data, n_comps=50)
# data.obsm['X_spectral']
```

Inspect a raw UMAP before correction:
```python
snap.tl.umap(data, use_rep="X_spectral")
snap.pl.umap(data, color="sample")
```

If the samples mix cleanly already, you may not need batch correction. Usually they don't.

## Step 6 — Batch correction

Two methods. Run one or both.

### Harmony (faster, most common)

```python
snap.pp.harmony(
    data,
    batch="sample",                  # column in data.obs
    max_iter_harmony=20,             # default 10; bump for stubborn batches
    use_rep="X_spectral",            # input embedding
)
# data.obsm['X_spectral_harmony']
```

### MNN-correct (more robust on extreme batch shifts)

```python
snap.pp.mnc_correct(
    data,
    batch="sample",
    use_rep="X_spectral",
)
# data.obsm['X_spectral_mnn']
```

**Choosing between them**:
- Harmony scales better (linear in #cells). Default choice for ≥ 100k cells.
- MNN-correct preserves cell-cell similarity better for small populations. Better when one sample has a rare cell type missing from others.
- They sometimes disagree on rare populations. Cross-check: if a population shows up in `X_spectral_harmony` UMAP but disappears in `X_spectral_mnn`, inspect whether it's biology or batch artifact.

## Step 7 — Downstream uses the corrected embedding

```python
snap.tl.umap(data, use_rep="X_spectral_harmony")
snap.pp.knn(data, use_rep="X_spectral_harmony", n_neighbors=50)
snap.tl.leiden(data, resolution=1.0)
```

Plot:
```python
snap.pl.umap(data, color=['leiden', 'sample'])
```

Goal: clusters that mix samples (good) without smearing real cell types together (bad).

## Step 8 — Per-cluster peak calling with sample replicates

This is where multi-sample really pays off. MACS3 can use sample identity as a biological replicate within each cluster, producing reproducible peaks instead of one big pool:

```python
snap.tl.macs3(
    data,
    groupby='leiden',
    replicate='sample',              # KEY for multi-sample
    qvalue=0.05,
)
```

Without `replicate`, peaks are called on the union of all cells in a cluster — high power, but no reproducibility guarantee, and a peak driven entirely by one sample looks just as significant.

With `replicate='sample'`, MACS3 requires the peak to appear consistently across samples. This is the right setting for any multi-sample analysis.

## Step 9 — Merge peaks

```python
merged_peaks = snap.tl.merge_peaks(data.uns['macs3'], chrom_sizes=snap.genome.hg38)
# polars DataFrame: chrom, start, end, plus a 'name' column with which clusters supported it
```

## Step 10 — Cell × peak matrix

```python
peak_mat = snap.pp.make_peak_matrix(data, use_rep=merged_peaks)
# peak_mat.shape == (n_cells_total, n_peaks_in_merged_set)
```

This is the "publication-ready" matrix. From here, DARs, gene activity, and motif analysis all proceed as in the single-sample pipeline.

## Step 11 — Persistence

```python
# Closes the h5ads handle but the file remains on disk
data.close()

# Reopen in a later session
data = snap.read_dataset("combined.h5ads")
```

The `.h5ads` is the canonical store. Each `per_sample/*.h5ad` is referenced from it; **don't delete them** unless you're done with the analysis. Moving the `.h5ads` requires moving the `per_sample/` directory alongside.

## Diagnostics for batch correction

After Harmony / MNN-correct:

```python
# 1. Samples should mix in the UMAP, not segregate
snap.pl.umap(data, color='sample')

# 2. Per-cluster sample composition — should be balanced unless biology says otherwise
data.obs.groupby(['leiden', 'sample']).size().unstack(fill_value=0)

# 3. Per-sample marker activity should agree
gene_mat = snap.pp.make_gene_matrix(data, gene_anno=snap.genome.hg38)
import scanpy as sc
sc.pl.umap(gene_mat, color=['CD3D', 'MS4A1'], groups='sample')   # marker by sample
```

Bad outcomes:

| Symptom | Likely cause | Fix |
|---|---|---|
| Sample-specific clusters that don't make biological sense | Under-corrected | Bump `max_iter_harmony` to 30-50; try MNN |
| A real cell type from one sample is absent from the UMAP | Over-corrected | Try MNN instead; lower Harmony's `theta` parameter |
| Big peak count discrepancies between samples | Wildly different sequencing depth | Equalize via `min_num_fragments` thresholding at import time |

## Memory considerations

For ≥ 1M cells across all samples, the bin matrix can hit 100s of GB. Strategies:

- Use `bin_size=5000` minimum (10000 for atlas-scale)
- Run feature selection per sample first, then combine on the union of selected features — drastically reduces `n_vars` in the combined object
- Use `low_memory=True` on `tl.spectral` (slower but bounded RAM)
- For Harmony, `low_memory=True` on `pp.harmony` is also available

Convenience: `python scripts/snapatac_multi_sample.py --samples sample_paths.txt --batch-correct harmony --out combined.h5ads`. The script handles per-sample QC, AnnDataSet creation, batch correction, peak calling — all from a single file listing one fragment path per line.
