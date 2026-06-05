# Cell-Type Annotation — Deep Dive

ATAC clusters need cell-type labels to be useful. Two strategies, often combined:

1. **Manual** — compute gene activity → score canonical markers → assign clusters
2. **Automated** — train SCANVI on a matched RNA reference → transfer labels

The manual approach is faster and works with no reference, but doesn't scale well to many cell types. The automated approach scales but requires (a) a high-quality RNA reference covering the same tissue/conditions and (b) GPU + scvi-tools.

## Approach 1 — Manual via gene activity + scanpy

### Build gene activity matrix

```python
import snapatac2 as snap
import scanpy as sc

gene_mat = snap.pp.make_gene_matrix(data, gene_anno=snap.genome.hg38)
# gene_mat is a new AnnData with cells × genes
# Each value is the sum of accessibility signal at the gene's TSS + body + extension
```

The default extension is 2 kb upstream of TSS. To customise:

```python
gene_mat = snap.pp.make_gene_matrix(
    data,
    gene_anno=snap.genome.hg38,
    upstream=5000,                # extend further upstream
    use_x=False,                  # use bin matrix (default) vs peak matrix
)
```

Wider extensions catch more distal regulation but pull in noise. 2 kb is fine for most analyses.

### Normalize like RNA

```python
sc.pp.normalize_total(gene_mat, target_sum=1e4)
sc.pp.log1p(gene_mat)
# gene_mat is now drop-in compatible with all scanpy DE / scoring tools
```

### Score canonical markers

Define marker sets up front. Examples for blood:

```python
markers = {
    'T_CD4':    ['CD3D', 'CD3E', 'CD4', 'TRAC'],
    'T_CD8':    ['CD3D', 'CD3E', 'CD8A', 'CD8B'],
    'B':        ['MS4A1', 'CD79A', 'CD79B', 'IGHM'],
    'NK':       ['NKG7', 'KLRD1', 'GNLY', 'PRF1'],
    'Mono':     ['CD14', 'LYZ', 'S100A8', 'S100A9'],
    'cDC':      ['FCER1A', 'CST3', 'CLEC10A'],
    'pDC':      ['IL3RA', 'TCF4', 'LILRA4'],
    'NK_T':     ['CD3D', 'NKG7', 'KLRD1'],
}

for name, genes in markers.items():
    sc.tl.score_genes(gene_mat, gene_list=genes, score_name=f'{name}_score')
```

### Inspect per-cluster mean scores

```python
score_cols = [f'{name}_score' for name in markers]
per_cluster = gene_mat.obs.groupby('leiden')[score_cols].mean()
per_cluster
#         T_CD4  T_CD8     B    NK  Mono  cDC   pDC
# 0        0.42   0.08  -0.21 -0.31  0.02 -0.18 -0.41
# 1        0.05   0.51   0.02 -0.18 -0.13 -0.22 -0.39
# 2       -0.18  -0.21   0.78  0.12 -0.16 -0.05  0.11
# ...
```

The dominant score per row → cluster identity. Visualize as a heatmap for figures.

### Cluster-to-cell-type mapping

```python
# After inspecting per_cluster, write the assignment manually
cluster_to_celltype = {
    '0': 'T_CD4',
    '1': 'T_CD8',
    '2': 'B',
    '3': 'NK',
    '4': 'Mono_classical',
    '5': 'Mono_non_classical',
    '6': 'cDC',
    '7': 'pDC',
}

# Write back to the ATAC AnnData (not the gene matrix — the ATAC has the spectral / peaks)
data.obs['cell_type'] = data.obs['leiden'].astype(str).map(cluster_to_celltype)

# Sanity check
snap.pl.umap(data, color='cell_type')
```

### When manual annotation fails

- **A cluster has no dominant marker** → either it's a novel cell type, a doublet population, or a transition state. Inspect with `sc.pl.umap(gene_mat, color=score_cols, ncols=3)` to see whether the cluster is mid-way between two types.
- **Multiple clusters score high on the same marker set** → fine-grained sub-types. Use sub-clustering: `sc.tl.leiden(gene_mat, restrict_to=('leiden', ['cluster_id']), resolution=1.5)`.
- **The marker set doesn't fit your tissue** → custom marker sets from the literature. CellMarker 2024 and PanglaoDB are good starting points.

## Approach 2 — Automated via SCANVI label transfer

When you have a labelled scRNA-seq reference covering similar tissue/cells, this is much more thorough than manual marker scoring. The trade-off is compute (GPU + ~20-60 minutes for training).

### Prereqs

```bash
pip install scvi-tools
# GPU strongly recommended — set device='cuda' below
```

### Step 1 — Prepare both sides

```python
import snapatac2 as snap
import scanpy as sc
import anndata as ad
import scvi

# ATAC → gene activity matrix
query = snap.pp.make_gene_matrix(atac_data, gene_anno=snap.genome.hg38)
query.obs['batch']            = 'ATAC'
query.obs['celltype_scanvi']  = 'Unknown'           # placeholder label

# RNA reference — should already have cell-type labels
reference = sc.read_h5ad('rna_reference_annotated.h5ad')
reference.obs['batch']            = 'RNA'
reference.obs['celltype_scanvi']  = reference.obs['cell_type']   # rename to the SCANVI key
```

### Step 2 — Merge on shared genes

```python
data = ad.concat(
    [reference, query],
    join='inner',                  # keep only genes present in both
    label='batch_origin',
)
print(f"Combined: {data.n_obs} cells × {data.n_vars} genes")
```

### Step 3 — HVG selection on the joint set, respecting batches

```python
sc.pp.normalize_total(data, target_sum=1e4)
sc.pp.log1p(data)
sc.pp.highly_variable_genes(
    data,
    n_top_genes=4000,
    batch_key='batch',           # finds HVGs robust across batches
    flavor='seurat_v3',          # recommended for scVI input
)
data = data[:, data.var['highly_variable']].copy()
```

### Step 4 — Train scVI (unsupervised joint embedding)

```python
scvi.model.SCVI.setup_anndata(
    data,
    batch_key='batch',
    labels_key='celltype_scanvi',     # 'Unknown' for ATAC, real labels for RNA
)

vae = scvi.model.SCVI(
    data,
    n_layers=2,
    n_latent=30,
    gene_likelihood='nb',
)
vae.train(max_epochs=200, early_stopping=True, accelerator='gpu', devices=1)
```

Inspect the latent space:
```python
data.obsm['X_scVI'] = vae.get_latent_representation()
sc.pp.neighbors(data, use_rep='X_scVI')
sc.tl.umap(data)
sc.pl.umap(data, color=['batch', 'celltype_scanvi'])
```

The two batches should mix cleanly. If they don't, more training epochs or a larger `n_latent`.

### Step 5 — Train SCANVI (label transfer)

```python
lvae = scvi.model.SCANVI.from_scvi_model(
    vae,
    adata=data,
    labels_key='celltype_scanvi',
    unlabeled_category='Unknown',
)
lvae.train(
    max_epochs=20,                   # SCANVI fine-tunes — fewer epochs
    n_samples_per_label=100,
    accelerator='gpu',
    devices=1,
)

# Predict for everyone
data.obs['C_scANVI'] = lvae.predict(data)

# Get prediction confidence
soft_predictions = lvae.predict(data, soft=True)
data.obs['C_scANVI_confidence'] = soft_predictions.max(axis=1)
```

### Step 6 — Map back to the ATAC AnnData

```python
# Pull the ATAC half of the combined object
atac_mask = data.obs['batch'] == 'ATAC'
atac_data.obs['celltype_predicted'] = data.obs.loc[atac_mask, 'C_scANVI'].values
atac_data.obs['confidence']         = data.obs.loc[atac_mask, 'C_scANVI_confidence'].values

# Visualize
snap.pl.umap(atac_data, color='celltype_predicted')
snap.pl.umap(atac_data, color='confidence')          # low-confidence cells flag novel populations
```

### Step 7 — Reconcile with leiden clusters

If SCANVI says cell X is T_CD4 but it sits in a leiden cluster labelled B from your earlier manual analysis, something's off. Inspect:

```python
import pandas as pd
confusion = pd.crosstab(
    atac_data.obs['leiden'],
    atac_data.obs['celltype_predicted'],
)
# Most cells should be in one (cluster, celltype) pair per row.
# If a leiden cluster splits across multiple celltypes, sub-cluster it.
# If two leiden clusters map to the same celltype, they're likely sub-states.
```

## When SCANVI doesn't work

| Symptom | Likely cause | Fix |
|---|---|---|
| Many cells predicted as the most-common reference type | The reference doesn't cover your ATAC cell types | Use manual marker scoring for unknown populations |
| Predictions disagree with leiden clusters everywhere | scVI didn't converge — RNA and ATAC don't mix in latent space | More HVGs (8000+), more epochs (500+), more `n_latent` (50) |
| Confidence is uniformly low | Reference is too dissimilar (different tissue, condition) | Find a better reference, or fall back to manual |
| Atlas-level reference predicts every minor subtype | Over-specific labels for your downstream needs | Coarsen the reference labels first: `reference.obs.cell_type = reference.obs.cell_type.map({...})` to broader categories |

## When to combine both approaches

Use SCANVI for the broad strokes (T cells, B cells, monocytes), then sub-cluster + manually annotate the subtypes (CD4 vs CD8, naive vs memory). This is a common workflow for tissues where a high-quality broad-type atlas exists but fine-grained labels don't.

```python
# After SCANVI broad-type labels are assigned:
for broad_type in ['T', 'B', 'Mono']:
    mask = atac_data.obs['celltype_predicted'] == broad_type
    sub = atac_data[mask].copy()
    sc.tl.leiden(sub, resolution=1.5, key_added=f'leiden_{broad_type}')
    sc.tl.rank_genes_groups(sub, groupby=f'leiden_{broad_type}', method='wilcoxon')
    # then manually annotate from the sub-cluster marker output
```
