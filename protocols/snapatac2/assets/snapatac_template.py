#!/usr/bin/env python3
"""
snapatac_template.py — End-to-end SnapATAC2 template.

Configure the CONFIGURATION block, then run end-to-end:
  1. Single-sample or multi-sample import (auto-detected)
  2. QC + feature selection + doublet filter
  3. Spectral embedding + (optional) batch correction (Harmony / MNN)
  4. UMAP + clustering
  5. Per-cluster MACS3 peak calling
  6. Gene activity matrix
  7. (Optional) SCANVI label transfer from a scRNA-seq reference
  8. Save annotated outputs + diagnostic figures
"""

import os
import sys
from pathlib import Path

import snapatac2 as snap

# ============================================================================
# CONFIGURATION — edit these
# ============================================================================

# Input — either a single fragment file OR a list of (name, path) tuples
INPUT_MODE = "single"   # "single" or "multi"
SINGLE_FRAGMENT = "/data/sample/fragments.tsv.gz"
MULTI_SAMPLES   = [
    # ("ctrl_d1", "/data/ctrl_d1_fragments.tsv.gz"),
    # ("ctrl_d2", "/data/ctrl_d2_fragments.tsv.gz"),
    # ("dis_d1",  "/data/dis_d1_fragments.tsv.gz"),
    # ("dis_d2",  "/data/dis_d2_fragments.tsv.gz"),
]

# Reference genome — must match the species/build of the fragments
GENOME = "hg38"          # hg38 | mm10 | hg19 | GRCh38 | mm9

# QC
MIN_TSSE    = 7
MIN_COUNTS  = 1000
MAX_COUNTS  = 100000
BIN_SIZE    = 5000
N_FEATURES  = 50000

# Dimensionality reduction + clustering
N_COMPS     = 50
N_NEIGHBORS = 50
RESOLUTION  = 1.0

# Batch correction (multi-sample only)
BATCH_CORRECT = "harmony"   # "harmony" | "mnn" | "none"

# Outputs
OUT_H5AD       = "results/atac.h5ad" if INPUT_MODE == "single" else "results/atac.h5ads"
PEAK_OUT       = "results/atac_peaks.h5ad"
GENE_OUT       = "results/atac_gene_activity.h5ad"
PER_SAMPLE_DIR = "results/per_sample"
FIG_DIR        = "figures"

# Optional SCANVI label transfer — set REFERENCE_H5AD to enable
REFERENCE_H5AD          = None            # path to annotated scRNA-seq .h5ad, e.g. "rna_ref.h5ad"
REFERENCE_LABEL_KEY     = "cell_type"     # column with cell-type labels in the reference

# ============================================================================
# SETUP
# ============================================================================

os.makedirs(Path(OUT_H5AD).parent or ".", exist_ok=True)
os.makedirs(PER_SAMPLE_DIR, exist_ok=True)
os.makedirs(FIG_DIR,        exist_ok=True)

genome = getattr(snap.genome, GENOME)

# ============================================================================
# 1. IMPORT
# ============================================================================

if INPUT_MODE == "single":
    print("=" * 80)
    print("SINGLE-SAMPLE MODE")
    print("=" * 80)
    print(f"Importing fragments from {SINGLE_FRAGMENT} …")
    data = snap.pp.import_fragments(
        SINGLE_FRAGMENT,
        chrom_sizes=genome,
        file=OUT_H5AD,
        sorted_by_barcode=False,
        min_num_fragments=200,
    )
    print(f"  Imported: {data.n_obs} barcodes")
else:
    print("=" * 80)
    print("MULTI-SAMPLE MODE")
    print("=" * 80)
    if not MULTI_SAMPLES:
        sys.exit("INPUT_MODE='multi' but MULTI_SAMPLES is empty. Edit the CONFIGURATION block.")
    names = [n for n, _ in MULTI_SAMPLES]
    paths = [p for _, p in MULTI_SAMPLES]
    print(f"Importing {len(MULTI_SAMPLES)} samples …")
    adatas = snap.pp.import_fragments(
        paths,
        file=[os.path.join(PER_SAMPLE_DIR, f"{n}.h5ad") for n in names],
        chrom_sizes=genome,
        min_num_fragments=MIN_COUNTS,
    )

# ============================================================================
# 2. QC + FEATURES + DOUBLETS
# ============================================================================

print("\n=== QC ===")
target = data if INPUT_MODE == "single" else adatas
snap.metrics.tsse(target, genome)
snap.pp.filter_cells(target, min_tsse=MIN_TSSE,
                     min_counts=MIN_COUNTS, max_counts=MAX_COUNTS)
snap.pp.add_tile_matrix(target, bin_size=BIN_SIZE)
snap.pp.select_features(target, n_features=N_FEATURES)
snap.pp.scrublet(target)
snap.pp.filter_doublets(target)

# ============================================================================
# 3. COMBINE (multi-sample only) + SPECTRAL
# ============================================================================

if INPUT_MODE == "multi":
    print("\n=== Combining samples ===")
    data = snap.AnnDataSet(
        adatas=list(zip(names, adatas)),
        filename=OUT_H5AD,
    )
    print(f"  Combined: {data.n_obs} cells × {data.n_vars} features")
    snap.pp.select_features(data, n_features=N_FEATURES)

print(f"\n=== Spectral ({N_COMPS} comps) ===")
snap.tl.spectral(data, n_comps=N_COMPS)

# ============================================================================
# 4. BATCH CORRECTION (multi-sample only)
# ============================================================================

primary_rep = "X_spectral"
if INPUT_MODE == "multi" and BATCH_CORRECT != "none":
    print(f"\n=== Batch correction: {BATCH_CORRECT} ===")
    if BATCH_CORRECT == "harmony":
        snap.pp.harmony(data, batch="sample", max_iter_harmony=20)
        primary_rep = "X_spectral_harmony"
    elif BATCH_CORRECT == "mnn":
        snap.pp.mnc_correct(data, batch="sample")
        primary_rep = "X_spectral_mnn"

# ============================================================================
# 5. UMAP + CLUSTERING
# ============================================================================

print(f"\n=== UMAP + leiden (rep={primary_rep}, res={RESOLUTION}) ===")
snap.pp.knn(data, use_rep=primary_rep, n_neighbors=N_NEIGHBORS)
snap.tl.umap(data, use_rep=primary_rep)
snap.tl.leiden(data, resolution=RESOLUTION)
print(f"  Clusters found: {data.obs['leiden'].nunique()}")

try:
    snap.pl.umap(data, color="leiden",
                  out_file=f"{FIG_DIR}/umap_leiden.pdf", interactive=False)
    if INPUT_MODE == "multi":
        snap.pl.umap(data, color="sample",
                      out_file=f"{FIG_DIR}/umap_sample.pdf", interactive=False)
except Exception as e:
    print(f"  (UMAP plots skipped: {e})")

# ============================================================================
# 6. PEAK CALLING
# ============================================================================

print("\n=== Peak calling (MACS3) ===")
try:
    if INPUT_MODE == "multi":
        snap.tl.macs3(data, groupby="leiden", replicate="sample")
    else:
        snap.tl.macs3(data, groupby="leiden")
    merged_peaks = snap.tl.merge_peaks(data.uns["macs3"], chrom_sizes=genome)
    print(f"  Merged peaks: {len(merged_peaks)}")

    peak_mat = snap.pp.make_peak_matrix(data, use_rep=merged_peaks)
    peak_mat.write_h5ad(PEAK_OUT)
    print(f"  Peak matrix → {PEAK_OUT}")
except Exception as e:
    print(f"  WARNING: peak calling failed: {e}")
    peak_mat = None

# ============================================================================
# 7. GENE ACTIVITY MATRIX
# ============================================================================

print("\n=== Gene activity matrix ===")
try:
    gene_mat = snap.pp.make_gene_matrix(data, gene_anno=genome)
    gene_mat.write_h5ad(GENE_OUT)
    print(f"  Gene activity → {GENE_OUT}")
except Exception as e:
    print(f"  WARNING: gene activity failed: {e}")
    gene_mat = None

# ============================================================================
# 8. OPTIONAL — SCANVI LABEL TRANSFER FROM scRNA-seq REFERENCE
# ============================================================================

if REFERENCE_H5AD and gene_mat is not None:
    print("\n=== SCANVI label transfer ===")
    try:
        import scanpy as sc
        import anndata as ad
        import scvi

        gene_mat.obs["batch"]            = "ATAC"
        gene_mat.obs["celltype_scanvi"]  = "Unknown"

        reference = sc.read_h5ad(REFERENCE_H5AD)
        reference.obs["batch"]            = "RNA"
        reference.obs["celltype_scanvi"]  = reference.obs[REFERENCE_LABEL_KEY]

        combined = ad.concat([reference, gene_mat], join="inner", label="batch_origin")
        sc.pp.normalize_total(combined); sc.pp.log1p(combined)
        sc.pp.highly_variable_genes(combined, n_top_genes=4000, batch_key="batch",
                                      flavor="seurat_v3")
        combined = combined[:, combined.var["highly_variable"]].copy()

        scvi.model.SCVI.setup_anndata(combined, batch_key="batch",
                                        labels_key="celltype_scanvi")
        vae = scvi.model.SCVI(combined, n_layers=2, n_latent=30)
        vae.train(max_epochs=200, early_stopping=True)

        lvae = scvi.model.SCANVI.from_scvi_model(
            vae, adata=combined,
            labels_key="celltype_scanvi",
            unlabeled_category="Unknown",
        )
        lvae.train(max_epochs=20, n_samples_per_label=100)

        combined.obs["C_scANVI"] = lvae.predict(combined)
        atac_mask = combined.obs["batch"] == "ATAC"
        data.obs["celltype_predicted"] = combined.obs.loc[atac_mask, "C_scANVI"].values

        try:
            snap.pl.umap(data, color="celltype_predicted",
                          out_file=f"{FIG_DIR}/umap_celltype.pdf", interactive=False)
        except Exception:
            pass
        print("  Labels transferred — see data.obs['celltype_predicted']")
    except ImportError as e:
        print(f"  Skipped (missing dep): {e}")
        print("  Install with: pip install scvi-tools")
    except Exception as e:
        print(f"  WARNING: SCANVI step failed: {e}")

# ============================================================================
# DONE
# ============================================================================

data.close()
print("\nDone.")
print(f"  ATAC AnnData:    {OUT_H5AD}")
print(f"  Peak matrix:     {PEAK_OUT}")
print(f"  Gene activity:   {GENE_OUT}")
print(f"  Figures:         {FIG_DIR}/")
