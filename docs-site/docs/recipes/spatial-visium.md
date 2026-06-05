# Recipe: 10x Visium spatial transcriptomics

End-to-end spatial workflow: Space Ranger outputs → spot clustering →
niches → cell-type deconvolution.

## What you'll build

- A processed `visium.h5ad` with QC, normalization, PCA, neighbors,
  UMAP, spatial clustering, and Moran's I gene-spatial autocorrelation
- A spatial cluster map (tissue overlay) and a UMAP of the same
  clusters
- A spatial niche map showing 5-8 niches based on cell-type composition
- Optional: `cell2location_abundance.csv` — per-spot deconvolved cell-type
  proportions

## Inputs

- A 10x **Visium** sample processed through Space Ranger 2.x+. You need:

    - `filtered_feature_bc_matrix.h5` — gene × spot matrix
    - `spatial/` directory — tissue image, scalefactors, spot positions

- (For deconvolution) A scRNA-seq reference of the same tissue. Either:

    - Your own annotated `.h5ad`, or
    - A public reference from CellRef / Tabula Sapiens

If you don't have a sample, the [10x public datasets page](https://www.10xgenomics.com/datasets)
has free Visium samples (brain, breast cancer, lung).

## Setup

Open the project, load **spatial › Visium / 10x spatial** and optionally
**spatial › cell2location deconvolution**.

## Step 1 — Plan

**Plan** mode:

> *I have a 10x Visium sample in `space_ranger_output/`. Run the standard
> Scanpy + Squidpy spatial workflow: QC, normalization, HVG, PCA, neighbors,
> UMAP, Leiden clustering, spatial autocorrelation (Moran's I) on the top
> HVGs, and spatial neighborhood enrichment. End deliverables: visium.h5ad
> plus a spatial scatter colored by cluster overlaid on the tissue.*

The plan should call out:

- QC thresholds (Visium spots are looser than scRNA — typical:
  `pct_counts_mt < 30`, `n_genes_by_counts > 200`)
- Normalization (sc.pp.normalize_total + log1p; same as scRNA-seq)
- HVG count (3000 for Visium is reasonable)
- Spatial clustering method — Leiden on the gene-expression neighbor
  graph is the simplest; mention Banksy or STAGATE if you want
  spatial-aware clustering
- Whether to use the lo-res or hi-res tissue image for plotting

## Step 2 — Execute the spatial QC + clustering

**Agent**:

> *Plan looks good. Run it. Stop after generating the QC summary so I
> can inspect.*

Agent creates a notebook, loads with `sc.read_visium()`, runs the QC,
and stops. You inspect:

- Pct mitochondrial reads per spot — Visium often has higher mito than
  scRNA because tissue prep differs.
- Genes per spot — Visium spots typically capture 1k-5k unique genes;
  much higher than scRNA droplets.
- Tissue alignment — the spatial scatter should overlay the H&E image
  cleanly.

If QC looks good, tell Agent to continue:

> *Filters look reasonable. Continue with the rest of the workflow.*

You end up with a clustered spatial dataset.

## Step 3 — Spatial niches

**Agent**:

> *Compute the cell-type composition for each cluster using my scRNA-seq
> reference at `pbmc_ref.h5ad`. Then define niches as spatial regions
> where multiple clusters mix. Use Squidpy's neighborhood enrichment
> analysis. Plot the resulting niches on the tissue.*

## Step 4 — Cell-type deconvolution with cell2location

If you loaded the cell2location protocol:

**Agent**:

> *Run cell2location with my scRNA-seq reference at `scrna_ref.h5ad`.
> Use the default Bayesian priors. Run for 30k iterations on GPU if
> available. Output per-spot cell-type abundance as a CSV.*

This is the slow step (30k iterations is ~1h on an A100, ~6h on CPU).
If you're on HPC, this is the kind of step you submit as a SLURM batch
job. Tell Agent:

> *Generate a SLURM script with `--gres=gpu:1 --mem=64G --time=8:00:00`
> and submit with sbatch. Don't wait for completion in this session;
> just give me the job ID.*

Operon can poll the job later with `squeue` and notify you when it
finishes.

## Variations

### Xenium or MERFISH

Different protocol — **spatial › Xenium / MERFISH**. Workflow is
similar but ingestion uses subcellular-resolution segmentation, and QC
focuses on per-cell rather than per-spot metrics.

### Multiple sections from the same tissue

Tell Agent: "I have 3 Visium sections. Concatenate them with batch
labels and use Harmony for batch correction before clustering."

### Spatial deconvolution without cell2location

Lighter-weight alternatives: RCTD, SPOTlight, or stereoscope. Load
the relevant protocol. cell2location is the most accurate but the
slowest.

## Pitfalls

- **Lo-res vs hi-res image confusion** — Space Ranger ships both; lo-res
  is fine for spatial scatter plots in publications. Hi-res only if
  you're zooming into specific tissue regions.
- **`tissue_lowres_image.png` not found** — older Space Ranger versions
  named it differently. `sc.read_visium()` may fail; symlink or pass
  the path explicitly.
- **Out-of-tissue spots** — `filtered_feature_bc_matrix.h5` already
  excludes them, but check that `adata.obs["in_tissue"]` is all 1.
- **Coordinate confusion** — Visium stores both pixel coordinates (for
  plotting) and array coordinates (for graph neighbors). Most tools
  use the right one automatically; if your spatial plot looks rotated
  or flipped, you have the wrong one.
- **cell2location memory** — needs ~32GB RAM minimum for a Visium
  sample. GPU helps speed but doesn't reduce RAM.

## Sanity checks

```python
# Spot count
print(adata.shape)   # ~3000 spots in tissue for a typical Visium

# QC
sc.pl.spatial(adata, color=["n_genes_by_counts", "pct_counts_mt"])
# Should look smooth across tissue, not splotchy

# After clustering — overlay clusters on tissue
sc.pl.spatial(adata, color="leiden")
# Clusters should form coherent regions, not salt-and-pepper
```

If clusters are salt-and-pepper, you under-clustered or the normalization
is broken. If clusters are one giant blob, you over-clustered.

## Next steps

- [PBMC scRNA-seq](scrna-pbmc.md) — make a scRNA reference if you don't have one
- Protocol: **spatial › CellChat communication** — ligand-receptor
  inference across spatial cell types
- For longitudinal / multi-sample Visium, see the spatial integration
  protocols
