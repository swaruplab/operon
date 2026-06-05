# Per-Platform Loading Guide

Every spatial platform stores its outputs differently. The squidpy / scanpy ecosystem covers all of the common ones, but each has gotchas. This guide is the single source of truth for "where does the data live, how do I load it, and what do I need to check after loading."

## Universal post-load checks

After loading, regardless of platform, verify:

```python
print(f"n_obs (cells/spots): {adata.n_obs}")
print(f"n_vars (genes):      {adata.n_vars}")
assert 'spatial' in adata.obsm, "Spatial coordinates missing — adata.obsm['spatial'] required"
assert adata.obsm['spatial'].shape == (adata.n_obs, 2), "spatial must be (n_obs, 2)"
print(f"Coord range: x ∈ [{adata.obsm['spatial'][:,0].min():.1f}, {adata.obsm['spatial'][:,0].max():.1f}], "
      f"y ∈ [{adata.obsm['spatial'][:,1].min():.1f}, {adata.obsm['spatial'][:,1].max():.1f}]")
```

If `adata.obsm['spatial']` is missing or shaped wrong, **stop** — every downstream squidpy call will silently behave wrong or crash.

## Visium (10X Genomics)

**Resolution:** 55 µm spots (~1-10 cells per spot)
**Panel:** Whole transcriptome (~36k genes)
**Image:** H&E always included; can be hi-res or low-res

```python
adata = sc.read_visium(
    path='path/to/spaceranger_outs/',
    count_file='filtered_feature_bc_matrix.h5',  # or 'raw_feature_bc_matrix.h5'
    library_id='sample_1',  # important if combining multiple slides
    source_image_path='path/to/tissue_image.png',  # for hi-res image
)
adata.var_names_make_unique()
```

**Expected directory layout:** `outs/` from `spaceranger count` containing `filtered_feature_bc_matrix.h5`, `spatial/tissue_positions_list.csv`, `spatial/tissue_hires_image.png`, `spatial/scalefactors_json.json`.

**Gotchas:**
- `filtered_*` keeps only barcodes inside tissue (almost always what you want). `raw_*` includes off-tissue spots.
- `library_id` becomes a key inside `adata.uns['spatial']`. If you don't pass one, scanpy invents one — make it stable across samples.
- Coordinate units in `adata.obsm['spatial']` are **pixel coords on the hi-res image**, not µm. Multiply by `scalefactor['tissue_hires_scalef']` to get a different image's coords.
- Fiducial frame artifacts are common at slide edges — inspect H&E with `sc.pl.spatial(adata, img_key='hires')` before downstream analysis.

## Xenium (10X Genomics)

**Resolution:** Subcellular (cells segmented from DAPI + boundary stains)
**Panel:** 300-500 genes (custom + curated panels)
**Image:** Morphology TIFF separate from count data

```python
adata = sq.read.xenium(
    path='path/to/xenium_output/',
    cells_boundaries=True,   # load segmentation polygons (used for plotting)
    cells_table=True,
    transcripts=False,       # set True only if you need individual transcripts (slow + large)
)
```

**Expected layout:** Xenium "output bundle" with `cells.csv.gz`, `cell_feature_matrix.h5`, `cell_boundaries.csv.gz`, `morphology_focus.ome.tif`.

**Gotchas:**
- `adata.obsm['spatial']` is in **microns** (Xenium's native unit), not pixels.
- Cells flagged in `cells.csv.gz` as `total_counts == 0` are sometimes empty segmentation polygons — filter immediately.
- Transcripts table is huge (often >10 GB). Don't load it unless doing transcript-level analysis (e.g. subcellular localization).
- Morphology TIFF is a separate file — load with `tifffile.imread()` if you want it as a NumPy array for custom overlays.

## CosMx (Nanostring)

**Resolution:** Single-cell (FOV-based imaging)
**Panel:** ~1000 genes (whole-transcriptome panel exists but most users have RNA panels)
**Image:** Optional IF channels (DAPI + 3-4 markers)

```python
adata = sq.read.nanostring(
    path='path/to/cosmx_output/',
    counts_file='exprMat_file.csv',          # transcript counts per cell
    meta_file='metadata_file.csv',           # cell metadata, including coords + FOV
    fov_file='fov_positions_file.csv',       # FOV layout in slide space
)
```

**Expected layout:** Output from CosMx's AtoMx pipeline. Files: `exprMat_file.csv`, `metadata_file.csv`, `fov_positions_file.csv`, optional `*_fov_*.tif` IF images per FOV.

**Gotchas:**
- Coordinates are in **pixels** within each FOV. The `fov_file` provides the FOV layout, and `sq.read.nanostring` stitches them into global slide coordinates.
- Cells at FOV boundaries can be double-counted (same cell in two adjacent FOVs). The Nanostring pipeline usually deduplicates, but verify by checking for near-duplicate centroids near FOV edges.
- The default panel includes 20 "negative probes" (`NegPrb*` or `SystemControl*`). Filter these out before HVG / clustering: `adata = adata[:, ~adata.var_names.str.startswith(('NegPrb', 'SystemControl'))]`.

## MERFISH (Vizgen / academic)

**Resolution:** Subcellular (cells segmented post-imaging)
**Panel:** 100-1000 genes depending on probe set
**Image:** DAPI + cell-boundary stain TIFFs (per FOV)

```python
# Vizgen MERSCOPE output:
adata = sq.read.vizgen(
    path='path/to/vizgen_output/',
    counts_file='cell_by_gene.csv',
    meta_file='cell_metadata.csv',
    transformation_file='micron_to_mosaic_pixel_transform.csv',  # optional
)
```

For **academic MERFISH** (not Vizgen): there's no canonical format. Load the count matrix + metadata manually:

```python
counts = pd.read_csv('cell_by_gene.csv', index_col=0)
meta = pd.read_csv('cell_metadata.csv', index_col=0)
import anndata as ad
adata = ad.AnnData(X=counts.values, obs=meta, var=pd.DataFrame(index=counts.columns))
# Build spatial coords from metadata
adata.obsm['spatial'] = meta[['center_x', 'center_y']].values  # adjust column names
```

**Gotchas:**
- Coordinates are usually in **microns** but check — older MERFISH papers use arbitrary pixel units.
- Cell segmentation quality is often the limiting factor in MERFISH analysis. Inspect segmentation maps; cells with `volume_um3 == 0` are bad polygons.
- "Blank" / "control" probes need filtering same as CosMx (`Blank-*` typically).

## Slide-seq / Slide-seqV2 (academic — Curio Bio)

**Resolution:** ~10 µm beads (sub-cellular but not cell-resolved)
**Panel:** Whole transcriptome
**Image:** None (bead positions known from barcoding chemistry)

```python
# Slide-seq doesn't have a unified loader in squidpy — typically you receive an h5ad
adata = sc.read_h5ad('path/to/slideseq.h5ad')

# If it's a count matrix + bead-position CSV:
counts = sc.read_csv('counts.csv').T  # genes × beads → beads × genes
positions = pd.read_csv('positions.csv', index_col=0)  # columns: x, y
adata = counts
adata.obsm['spatial'] = positions[['x', 'y']].loc[adata.obs_names].values
```

**Gotchas:**
- Beads ≠ cells. Without further cell-typing (e.g. RCTD, cell2location), Slide-seq beads represent transcript pileups from whatever cell(s) overlap the bead area.
- Many beads in any Slide-seq run are off-tissue. Filter by total counts and visual inspection of the bead positions.

## GeoMx (Nanostring) — region-of-interest profiling

**Resolution:** User-defined ROIs (often hundreds of cells per ROI)
**Panel:** WTA (~18k genes) or CTA (~1800 genes)
**Image:** IF images per ROI

```python
# GeoMx output is typically an Excel / CSV from the analysis suite
counts = pd.read_csv('path/to/geomx_counts.csv', index_col=0)  # ROIs × genes (or transpose)
meta = pd.read_csv('path/to/geomx_metadata.csv', index_col=0)
import anndata as ad
adata = ad.AnnData(X=counts.values, obs=meta,
                   var=pd.DataFrame(index=counts.columns))
# ROI centroids → adata.obsm['spatial']
adata.obsm['spatial'] = meta[['ROI_X', 'ROI_Y']].values  # adjust column names
```

**Gotchas:**
- GeoMx is **not single-cell**. Squidpy's neighborhood-enrichment / co-occurrence analyses don't apply meaningfully to ROIs. Use Moran's I and standard differential expression instead.
- Normalization is different: GeoMx provides Q3 or background-subtracted normalized data. Often you should use the normalized matrix from the GeoMx DSP suite, not raw counts.
- ROIs from different slides/cores are independent — don't treat them as one continuous spatial field.

## Adding a new platform

If you have a custom or in-house spatial assay:

1. Load the count matrix into an AnnData object (any standard scanpy reader works).
2. Set `adata.obsm['spatial']` = `(n_obs, 2)` array of x/y centroids.
3. Optionally populate `adata.uns['spatial'][library_id]` with images + scalefactors if you want H&E-style overlay.

Once `obsm['spatial']` is set, squidpy doesn't care which platform produced the data.

## Sanity-check loading

After any platform's loader, run this block:

```python
print(adata)
print(f"Has spatial: {'spatial' in adata.obsm}")
print(f"Coord ranges: x={adata.obsm['spatial'][:,0].min():.0f}-{adata.obsm['spatial'][:,0].max():.0f}, "
      f"y={adata.obsm['spatial'][:,1].min():.0f}-{adata.obsm['spatial'][:,1].max():.0f}")
print(f"Counts per cell median: {np.median(np.asarray(adata.X.sum(axis=1))):.0f}")
print(f"Genes detected per cell median: {np.median(np.asarray((adata.X > 0).sum(axis=1))):.0f}")

# Quick visual confirmation that the layout makes sense
import matplotlib.pyplot as plt
plt.figure(figsize=(6, 6))
plt.scatter(adata.obsm['spatial'][:, 0], adata.obsm['spatial'][:, 1],
            s=1, alpha=0.5)
plt.axis('equal')
plt.title(f"Loaded: {adata.n_obs} cells/spots, {adata.n_vars} genes")
plt.savefig('figures/loading_sanity_check.png', dpi=150)
```

If the scatter plot doesn't look like the tissue you expected, the spatial coords are wrong — fix that **before** any QC or analysis.
