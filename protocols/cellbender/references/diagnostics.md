# Interpreting the CellBender Report

After every run, CellBender writes a `*_report.html` and a `*_cellbender.pdf`. These are the difference between "ambient RNA removed" and "ambient RNA removed AND the result is trustworthy." This guide tells you what to look at and what each pattern means.

## The four key plots

CellBender's output PDF has (in order):

1. **ELBO learning curves** — training and test loss vs epoch
2. **UMI rank plot** — cells ordered by total UMI count, with cell probability overlay
3. **Counts removed per cell** — distribution of how much CellBender pulled out
4. **Latent space PCA** — PCs 1-2 of the learned cell representations, colored by cell probability

The HTML report adds:
5. **Warnings panel** — yellow/red flags from CellBender's automatic checks
6. **Top genes removed** — which genes contributed most to the ambient signal

## Plot 1 — ELBO curve

**What it shows**: The model's negative-log-likelihood at each epoch, training (blue) and test (orange).

**What good looks like**:
- Monotonically decreasing.
- Flattens by the end (last 30-50 epochs are near-flat).
- Train and test stay close together (no big gap).

**What's bad**:
- **Still descending steeply at epoch 150** → undertrained. Re-run with `--epochs 200` or `300`.
- **Train and test diverging** (train flat, test rising) → overfitting. Reduce `--epochs` or `--z-dim`. Rare in practice.
- **Plateau very early** (epoch 30-50) → either CellBender has nothing to learn (rare; means your data has almost no ambient signal — verify with the next plot) or the learning rate is too high. Lower `--learning-rate` from 1e-4 to 1e-5.

## Plot 2 — UMI rank with cell probability

**What it shows**: All barcodes ranked by total UMI count (high → low). The line is the cellranger-style knee curve; the colour gradient overlay is CellBender's `cell_probability` (1 = cell, 0 = empty droplet).

**What good looks like**:
- A clear **knee** in the rank curve around the expected number of cells.
- The colour gradient transitions sharply from "cells" (high probability) above the knee to "empties" (low probability) below.
- The transition happens at roughly the same rank where the cellranger knee sits.

**What's bad**:
- **No sharp transition** — the gradient is fuzzy across many ranks. Two causes:
  1. Real cell counts are too close to ambient counts (low-quality library). Bump `--fpr` to 0.05 and accept more aggressive cleanup.
  2. `--total-droplets-included` is wrong. Set it explicitly to ~10× your expected cell count.
- **Transition at the wrong rank** — e.g. you expect 5000 cells but the transition is at rank 1000 or 20000. Set `--expected-cells` and `--total-droplets-included` manually.
- **Long shoulder of "maybe-cells"** between the high-prob region and low-prob region. CellBender is unsure. Inspect: are these biologically real cell types with low expression (granulocytes? oligos?) or are they doublets / debris?

## Plot 3 — Counts removed per cell

**What it shows**: For each called cell, the fraction of its original counts that CellBender removed as ambient.

**What good looks like**:
- Most cells lose **5-20% of their counts**.
- The distribution is unimodal and centered on a reasonable fraction (15-25% for nuclei, 5-15% for PBMCs).

**What's bad**:
- **Many cells lose > 50% of their counts** → over-correction. Lower `--fpr` from 0.01 → 0.001.
- **Bimodal distribution** — one peak at ~5% and another at ~40% → there are two populations with very different background signatures. Check if those cells cluster separately by cell type; sometimes one cell type is genuinely more contaminated (e.g. neutrophils releasing granule content).
- **Almost no counts removed (< 1% for everyone)** → either the sample has minimal ambient (unlikely for fresh PBMCs/tissue) or CellBender didn't converge. Check the ELBO.

## Plot 4 — Latent PCA

**What it shows**: PCs 1-2 of the learned cell representations, colored by `cell_probability`.

**What good looks like**:
- Cells (red / high probability) cluster on one side.
- Empties (blue / low probability) cluster on the other side.
- A clear gradient or separation between them in PC space.

**What's bad**:
- Cells and empties overlap heavily — CellBender hasn't learned a clean separation. Often paired with a bad UMI-rank plot. Same fixes apply.

## The HTML warnings panel

CellBender automatically flags suspicious patterns. The common warnings and what they mean:

### "Expected cells differs from automatic detection by > 2×"
The auto-detected cell count is very different from what you (or the cellranger summary) expected. Verify:
- Open the cellranger web summary — what does it call?
- Check sample QC — has the library failed?
- If everything looks normal, re-run with `--expected-cells <number>` to override.

### "Total droplets included extends past low-UMI background"
CellBender was asked to model background using barcodes that have essentially zero UMI — adding noise without signal. Reduce `--total-droplets-included`.

### "Learning curve has not converged"
Same as ELBO plot diagnosis: bump `--epochs`.

### "Cells removed > 5% of total counts"
Over-correction. Lower `--fpr`.

### "Top removed gene is mitochondrial / ribosomal"
Could be legitimate (mt/rRNA from lysed cells IS often the dominant ambient signal). But if your downstream analysis later excludes mt% > 20%, you'll have already removed the same signal twice. Not a hard error — just noted for awareness.

## Validation: UMAP before vs after

The single most useful sanity check, not in CellBender's output. Do it manually:

```python
# Before: load raw
adata_raw = sc.read_10x_h5('raw_feature_bc_matrix.h5')
# Standard preprocessing
sc.pp.filter_cells(adata_raw, min_genes=200)
sc.pp.filter_genes(adata_raw, min_cells=3)
sc.pp.normalize_total(adata_raw); sc.pp.log1p(adata_raw)
sc.pp.highly_variable_genes(adata_raw, n_top_genes=2000)
sc.pp.scale(adata_raw, max_value=10)
sc.tl.pca(adata_raw); sc.pp.neighbors(adata_raw); sc.tl.umap(adata_raw)

# After: same on the CellBender output
from cellbender.remove_background.downstream import anndata_from_h5
adata_cb = anndata_from_h5('sample_cellbender_filtered.h5')
sc.pp.filter_genes(adata_cb, min_cells=3)
sc.pp.normalize_total(adata_cb); sc.pp.log1p(adata_cb)
sc.pp.highly_variable_genes(adata_cb, n_top_genes=2000)
sc.pp.scale(adata_cb, max_value=10)
sc.tl.pca(adata_cb); sc.pp.neighbors(adata_cb); sc.tl.umap(adata_cb)

# Compare side-by-side
sc.pl.umap(adata_raw, color=['CD3D', 'MS4A1', 'CD14'])
sc.pl.umap(adata_cb,  color=['CD3D', 'MS4A1', 'CD14'])
```

Expected: the same cluster structure, but with **less spillover** of cell-type-specific markers into other clusters. CD3D (T cells) should be confined to the T-cell cluster; if it was washing across many clusters in the raw and is now clean, CellBender did its job. If clusters look very different (new ones appear, real ones disappear), something is wrong — investigate before trusting the output.

## When NOT to trust CellBender's output

- ELBO never converged
- Cell probability transition is fuzzy
- UMAP changes drastically between raw and CellBender-cleaned
- Marker genes you trust (e.g. canonical immune markers in PBMC) get heavily removed

In any of those cases, either re-run with adjusted parameters or skip CellBender for that sample. CellBender is a denoiser, not magic — bad input → bad output.
