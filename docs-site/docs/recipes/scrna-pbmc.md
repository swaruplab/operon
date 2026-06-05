# Recipe: PBMC scRNA-seq with Scanpy

The "hello world" of single-cell. By the end you'll have a clustered,
annotated UMAP of the 10x PBMC 3k dataset.

## What you'll build

- A processed `pbmc.h5ad` with QC-filtered cells, normalized expression,
  PCA, neighbors, UMAP, Leiden clustering, and a marker table
- A UMAP figure colored by cluster
- A dot plot of canonical immune-cell markers
- A `clusters_annotated.csv` mapping cluster ID → cell type

## Inputs

- The 10x PBMC 3k filtered matrix:
  [pbmc3k_filtered_gene_bc_matrices.tar.gz](https://cf.10xgenomics.com/samples/cell-exp/1.1.0/pbmc3k/pbmc3k_filtered_gene_bc_matrices.tar.gz)
  (download once, untar into a project subfolder)
- A working Python environment with `scanpy`, `anndata`, `leidenalg`
  (Operon's Agent mode will install these if needed)

## Setup

Open Operon, `File → Open Folder`, and point it at a fresh project
directory. Drop the untarred 10x folder inside.

Click the **Protocols** icon and load **single-cell › Scanpy end-to-end**.

## Step 1 — Plan the analysis

Switch chat to **Plan** mode and prompt:

> *I have the 10x PBMC 3k filtered matrix in `filtered_gene_bc_matrices/hg19/`.
> I want a complete Scanpy analysis: QC, normalization, HVG, PCA,
> neighbors, UMAP, Leiden clustering, and marker genes. End deliverables
> should be `pbmc.h5ad`, `umap.png`, `dotplot.png`, and a marker CSV.
> Use a Jupyter notebook so I can re-run cells.*

Read the plan. Push back on anything that looks off:

- Filter thresholds (defaults: `min_genes=200`, `max_pct_mt=20`)
- HVG count (2000 is fine; 3000 if you have ≥10k cells)
- Leiden resolution (start at 0.5, refine)

## Step 2 — Execute

Switch to **Agent** mode and say:

> *Looks good. Generate the notebook and run it end-to-end. Stop and show
> me the QC violin plot before any filtering happens.*

Agent will:

1. Create `scrna_pbmc.ipynb`
2. Run the import + load cells
3. Compute QC metrics (`pct_counts_mt`, `n_genes_by_counts`)
4. Show you the pre-filter violins (stop point)

When the violins appear, review and confirm thresholds. Then:

> *Looks good. Apply the thresholds and continue.*

Agent runs through normalization → PCA → neighbors → UMAP → Leiden →
marker genes. When it finishes, you have a fully populated notebook plus
the output files.

## Step 3 — Annotate clusters

Switch to **Ask** mode for the annotation step:

> *Here are the top 10 marker genes per cluster (paste the table). What
> cell types are these clusters? Use canonical PBMC markers — CD3D/CD3E/CD8A
> for T cells, MS4A1/CD79A for B, GNLY/NKG7 for NK, CD14/LYZ for monocytes,
> FCER1A/CST3 for DCs.*

Claude will return a cluster → cell-type table.

Back in Agent mode:

> *Apply this annotation, save as `pbmc.h5ad` with the cell-type label
> in `.obs["cell_type"]`, regenerate the UMAP colored by cell_type, and
> make a dot plot of one canonical marker per cell type.*

## Step 4 — Quality check

Open the UMAP and dotplot in the editor's image viewer. Look for:

- **T-cell cluster splits** — large clusters often hide CD4 vs CD8
  subdivisions. Re-cluster at higher resolution if you care.
- **Outlier cells** — isolated dots far from any cluster may be doublets.
- **Cluster size sanity** — a typical 5000-cell PBMC has ~10 clusters at
  resolution 0.5, ~15-20 at resolution 1.0.

## Variations

### Use Seurat instead of Scanpy

Change the protocol to **single-cell › Seurat integration** and rephrase
the Step 1 prompt to ask for an R/Seurat workflow. The structure is the
same; the tool versions differ.

### Multiple samples with batch effects

Load the **single-cell › Seurat integration** or **scVI** protocol. Tell
Claude in Plan mode:

> *I have 4 PBMC samples from different donors and want to integrate them.
> Use [Harmony / scVI / SCT integration].*

### Add doublet detection upfront

Add **single-cell › Doublet detection** as a second loaded protocol.
Tell Claude to run Scrublet per-sample before merging.

## Pitfalls

- **`leidenalg` not installed** — Scanpy doesn't ship Leiden by default.
  Agent mode will install it the first time. Don't fight the install.
- **`%matplotlib inline` in notebook** — make sure Claude includes this
  in the first cell, or plots won't render in Jupyter (they'll still go
  to disk).
- **Negative values after `log1p` on transformed data** — if you're given
  data that's already log-normalized, don't double-normalize. Tell Claude
  in Plan: "the data is already normalized; start from PCA".
- **Cluster IDs are not stable across re-runs** — `random_state` matters.
  Set `random_state=42` everywhere if you want reproducible cluster numbers.

## Next steps

- [Spatial Visium](spatial-visium.md) — same Scanpy patterns applied to
  spatial data
- [Bulk RNA-seq + DESeq2](bulk-rnaseq-deseq2.md) — when you have pseudobulk
  per cluster
- Protocol: **single-cell › CellChat communication** — ligand-receptor
  inference across the clusters you just made
