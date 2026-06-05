# Differential Kinetics — Multi-Regime Velocity

## The problem

The dynamical model fits **one** set of kinetic parameters `(α, β, γ)` per gene, applied globally across all cells. For a homogeneous population, that's fine. For a **branching lineage**, it's wrong: the same gene can be induced fast in branch A and slowly in branch B, or have different splicing rates between cell types.

A gene with two competing kinetic regimes gets a fit that's neither — the model splits the difference, producing inaccurate velocity arrows in both branches.

## What `differential_kinetic_test` does

A **likelihood-ratio test** that compares:
- Null model: one fit for all cells of the tested gene
- Alternative model: per-cluster fits

If the per-cluster fits are significantly better, the gene shows differential kinetics. The p-value follows an asymptotic χ² distribution.

```python
import scvelo as scv

# After recover_dynamics + velocity (dynamical mode)
var_names = ['Gene1', 'Gene2', 'Gene3']          # genes to test
scv.tl.differential_kinetic_test(
    adata,
    var_names = var_names,
    groupby   = 'clusters'                        # column with cluster labels
)

# Results in adata.var:
#   fit_diff_kinetics      — list of clusters with distinct kinetics
#   fit_pval_kinetics      — likelihood-ratio p-value
```

Typically run on the top likelihood genes from `recover_dynamics`:

```python
top_genes = adata.var['fit_likelihood'].sort_values(ascending=False).index[:100]
scv.tl.differential_kinetic_test(adata, var_names=list(top_genes), groupby='clusters')

# Significant differential-kinetic genes
sig = adata.var.dropna(subset=['fit_pval_kinetics'])
sig = sig[sig['fit_pval_kinetics'] < 0.01]
print(f"{len(sig)} of {len(top_genes)} top driver genes show differential kinetics")
```

## Visualizing differential-kinetic genes

```python
# Pick a gene with strong differential kinetics
sig.sort_values('fit_pval_kinetics').head(5)
target = 'Gene1'

# Phase portrait colored by cluster — branches should look distinct
scv.pl.velocity(adata, [target], color='clusters', ncols=1, add_outline='cluster_a, cluster_b')

# Compare global vs cluster-specific fits
scv.pl.scatter(adata, target, color=['clusters', 'fit_diff_kinetics'])
```

The phase portrait of a true differential-kinetic gene shows two (or more) **distinct slopes / trajectories** for different clusters along the spliced × unspliced plane — i.e. the steady-state line that the global fit drew through the middle doesn't match either cluster's actual ratio.

## Recomputing velocity with differential kinetics

Once differential-kinetic genes are identified, re-run velocity using cluster-specific rates for those genes:

```python
scv.tl.velocity(adata, diff_kinetics=True)
scv.tl.velocity_graph(adata)
```

Internally: for each cluster, scVelo uses the cluster-specific kinetic fit for genes flagged in `fit_diff_kinetics`. For other genes (and other clusters), the global fit is used.

## Before/after diagnostic

The whole point of doing this is to see arrows change in branching regions. Compare:

```python
# Save the global-fit velocity for comparison
adata.layers['velocity_global'] = adata.layers['velocity'].copy()

# Re-run with differential kinetics
scv.tl.velocity(adata, diff_kinetics=True)
scv.tl.velocity_graph(adata)

# Side-by-side streamline plots
import matplotlib.pyplot as plt
fig, axes = plt.subplots(1, 2, figsize=(16, 7))

# Restore global velocity, plot
adata.layers['velocity'] = adata.layers['velocity_global']
scv.tl.velocity_graph(adata)
scv.pl.velocity_embedding_stream(adata, basis='umap', color='clusters',
                                   ax=axes[0], title='Global kinetics', show=False)

# Re-run differential kinetics, plot
scv.tl.velocity(adata, diff_kinetics=True)
scv.tl.velocity_graph(adata)
scv.pl.velocity_embedding_stream(adata, basis='umap', color='clusters',
                                   ax=axes[1], title='Differential kinetics', show=False)

plt.tight_layout()
plt.savefig('figures/dk_comparison.pdf')
```

Differential-kinetic arrows should look subtly different in the branching region — typically more coherent within each branch, less smearing across the bifurcation.

## When to bother

Differential kinetics testing is the right tool when:
- ✅ Your trajectory has a clear bifurcation (one progenitor → multiple fates)
- ✅ The dynamical model's global velocity arrows look wrong at the bifurcation (e.g. arrows pointing back toward the progenitor in one branch)
- ✅ You have ≥ 100 cells per branch (less = unstable cluster fits)

It's overkill when:
- ❌ Linear trajectory with one lineage (the global fit is fine)
- ❌ All cells homogeneous (no biology to find)
- ❌ Already happy with the dynamical streamlines and just want the figure

## Limitations

- **Per-gene independent test**: a TF that affects 50 genes won't be flagged individually — it's the downstream genes that show differential kinetics.
- **Cluster-dependent**: results depend on your `groupby` granularity. Try resolution 0.5 vs 1.0 to see what's robust.
- **High compute**: testing 100 genes across 8 clusters takes minutes; testing 2000 genes takes hours. Limit to the top likelihood genes.
- **Doesn't generalize cross-dataset**: per-cluster kinetics are dataset-specific. Don't compare absolute rates across samples / conditions.

## Worked example — pancreatic endocrine

The canonical scVelo example uses pancreatic endocrine differentiation, which branches into α / β / δ / ε cells:

```python
adata = scv.datasets.pancreas()

# Standard pipeline
scv.pp.filter_and_normalize(adata, min_shared_counts=20, n_top_genes=2000)
scv.pp.moments(adata, n_pcs=30, n_neighbors=30)
scv.tl.recover_dynamics(adata, n_jobs=8)
scv.tl.velocity(adata, mode='dynamical')
scv.tl.velocity_graph(adata)

# Identify differential kinetic genes
top_genes = adata.var['fit_likelihood'].sort_values(ascending=False).index[:50]
scv.tl.differential_kinetic_test(adata, var_names=list(top_genes), groupby='clusters')

# Re-run velocity with cluster-specific kinetics
scv.tl.velocity(adata, diff_kinetics=True)
scv.tl.velocity_graph(adata)

scv.pl.velocity_embedding_stream(adata, basis='umap', color='clusters')
```

In the pancreatic example, several β-cell-lineage genes show distinct kinetics from α-cell-lineage versions of the same gene. After `diff_kinetics=True`, the arrows in the bifurcation region become more cleanly separated between the α and β branches.

## Common pitfalls

- **Running before `recover_dynamics`**: differential kinetics needs the dynamical fits as input. Run `recover_dynamics` first.
- **Too few cells per cluster**: clusters with < 50 cells produce unstable kinetic fits, leading to false positives in the test.
- **Forgetting `velocity_graph` after re-running `velocity`**: the projection won't update without it.
- **Testing on the noisy fits** (low likelihood): only test on top-likelihood genes; otherwise you're testing noise vs noise.
