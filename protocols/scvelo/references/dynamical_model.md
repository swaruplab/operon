# Dynamical Model — Deep Dive

The dynamical mode fits the full ODE describing splicing kinetics per gene via expectation-maximization. It's the most accurate of the three scVelo modes and the recommended choice for publication-grade analyses — at the cost of more compute.

## The model

For each gene, the dynamical model fits:

```
du/dt = α(t) − β·u(t)        (unspliced mRNA: induction − splicing)
ds/dt = β·u(t) − γ·s(t)       (spliced mRNA: splicing − degradation)
```

with per-cell latent time `t` and four learned parameters per gene:

| Parameter | Meaning |
|---|---|
| `α` (alpha) | Transcription rate during induction |
| `β` (beta) | Splicing rate (constant) |
| `γ` (gamma) | Degradation rate (constant) |
| `t_` | Per-cell latent time on this gene's phase trajectory |

The EM alternates between fitting `(α, β, γ)` given the current per-cell `t` estimates, and fitting `t` for each cell given the rates. The output is a per-gene likelihood score (`fit_likelihood`) that tells you which genes the model fits well — those are your reliable velocity drivers.

## Running the model

```python
import scvelo as scv

# Must follow the standard preprocessing (filter_and_normalize + moments)
scv.tl.recover_dynamics(adata, n_jobs=8)
# Populates adata.var with: fit_alpha, fit_beta, fit_gamma, fit_likelihood,
#                            fit_t_, fit_scaling, fit_std_s, fit_std_u, fit_r2

scv.tl.velocity(adata, mode='dynamical')
scv.tl.velocity_graph(adata)
```

`recover_dynamics` is the slow step. Rough wall-clock estimates:
- 2000 HVGs × 5000 cells: ~5-15 min on 8 cores
- 2000 HVGs × 100k cells: ~30-90 min on 8 cores
- 4000 HVGs × 100k cells: 1-3 hours

Always `n_jobs > 1` — fitting is per-gene and trivially parallel.

## Latent time

The model's headline output: a per-cell time coordinate inferred from the joint kinetic fit across all genes.

```python
scv.tl.latent_time(adata)
# Adds adata.obs['latent_time'] ∈ [0, 1]

scv.pl.scatter(adata, color='latent_time', color_map='gnuplot', size=80)
```

Why this is better than `velocity_pseudotime`:
- Built from the kinetic ODE fits, not just graph traversal
- Naturally bounded `[0, 1]` (interpretable across datasets)
- Integrates per-gene timing — robust to noisy individual genes
- Aligns biologically meaningful events (cell birth, lineage commitment) across the dataset

When you have multiple branches, `latent_time` is per-cell-relative — a cell at `t = 0.5` in branch A is not directly comparable to `t = 0.5` in branch B.

## Heatmap of dynamical genes along latent time

The canonical scVelo "story" figure:

```python
top_genes = adata.var['fit_likelihood'].sort_values(ascending=False).index[:300]

scv.pl.heatmap(
    adata,
    var_names = top_genes,
    sortby    = 'latent_time',
    col_color = 'clusters',
    n_convolve = 100,                  # smoothing window
    yticklabels = True,
    figsize   = (8, 12),
)
```

Rows = genes (top by likelihood), columns = cells sorted by latent time, colored by cluster. Reveals the gene-program waves across the trajectory.

## Driver gene identification

### Globally top genes

```python
top_genes = adata.var['fit_likelihood'].sort_values(ascending=False).index
top_genes[:15]                          # top 15 driver genes overall

scv.pl.scatter(adata, basis=top_genes[:15], ncols=5, frameon=False)
# 15 phase portraits — visual confirmation of clean trajectories
```

### Per-cluster top genes

```python
scv.tl.rank_dynamical_genes(adata, groupby='clusters')
df = scv.get_df(adata, 'rank_dynamical_genes/names')
df.head()
#       cluster_a   cluster_b   cluster_c
# 0      Gene1       Gene4       Gene7
# 1      Gene2       Gene5       Gene8
# ...

# Visualize the top-3 per cluster
for cl in df.columns[:3]:
    scv.pl.velocity(adata, var_names=df[cl][:3].tolist(), color='clusters', ncols=3)
```

This is the equivalent of `rank_genes_groups` for velocity — markers of dynamic regulation, not steady-state expression.

## Per-gene phase portrait

The most direct way to inspect a single gene's dynamics:

```python
scv.pl.velocity(adata, ['Gene1'], color='clusters', ncols=1)
```

Gives you four panels per gene:
1. Phase portrait (spliced × unspliced × cluster)
2. Velocity on UMAP for this gene
3. Expression on UMAP for this gene
4. The fit_t_ (latent time) on UMAP

What to look for:
- A clean loop above the steady-state line during induction, below during repression
- High `fit_likelihood` in `adata.var.loc['Gene1']`
- `velocity` arrows on the UMAP pointing in the same direction as the high-level streamline plot

If a gene's phase portrait looks bad (scattered points, no loop, low likelihood), exclude it from interpretation:

```python
# Filter to "well-fit" genes (likelihood > median)
adata = adata[:, adata.var['fit_likelihood'] > adata.var['fit_likelihood'].median()].copy()

# Re-run velocity on the filtered set
scv.tl.velocity(adata, mode='dynamical')
scv.tl.velocity_graph(adata)
```

## Kinetic parameter biology

Inspecting `adata.var` columns gives per-gene biology:

```python
# Genes with the fastest induction (high alpha)
adata.var.sort_values('fit_alpha', ascending=False).head(20)

# Genes with the fastest decay (high gamma)
adata.var.sort_values('fit_gamma', ascending=False).head(20)

# Ratio gamma / beta = "lifetime ratio" — degradation vs splicing
adata.var['lifetime_ratio'] = adata.var['fit_gamma'] / adata.var['fit_beta']

# Most stable transcripts (low gamma) are typically housekeeping
# Most unstable are typically TFs and immediate-early response genes
```

Units are arbitrary — these rates are relative within the fit, not absolute biological rates in min⁻¹.

## When `recover_dynamics` fails

Symptoms:
- `fit_likelihood` is NaN or very low for most genes
- Phase portraits show scattered, unstructured points
- Latent time looks random

Causes:
- **Insufficient unspliced reads** — check `scv.pl.proportions`. If unspliced < 10%, the model can't fit.
- **Static / steady-state population** — no actual dynamics to capture. Velocity isn't appropriate.
- **Wrong intron model upstream** — re-run `kb count --workflow nac` if you used `--workflow lamanno` or no workflow at all.
- **Convergence failure** — bump `max_iter` from 10 to 20 in `scv.tl.recover_dynamics(adata, max_iter=20)`.

## Save and resume

```python
# After the slow step
adata.write('adata_dynamical.h5ad')

# Resume — everything downstream is fast
adata = scv.read('adata_dynamical.h5ad')
scv.tl.velocity(adata, mode='dynamical')        # quick — kinetics already in .var
scv.tl.velocity_graph(adata)
scv.tl.latent_time(adata)
```

The expensive computation is `recover_dynamics`. Once that's done, you can iterate quickly on velocity modes, graph parameters, and plotting.

## Combining with differential kinetics

For multi-lineage systems (see [differential_kinetics.md](differential_kinetics.md)), the dynamical model's global per-gene fit is the wrong model for branch-specific kinetic regimes. Run `scv.tl.differential_kinetic_test` after `recover_dynamics` to identify genes where the global fit is inadequate, then re-run velocity with `diff_kinetics=True`.

```python
top_genes = adata.var['fit_likelihood'].sort_values(ascending=False).index[:50]
scv.tl.differential_kinetic_test(adata, var_names=list(top_genes), groupby='clusters')
scv.tl.velocity(adata, diff_kinetics=True)
scv.tl.velocity_graph(adata)
```

The differential-kinetics workflow is most useful on the dynamical model's top likelihood genes — those are the ones the model thinks it understands well, so finding lineage-specific deviations there is highly interpretable.
