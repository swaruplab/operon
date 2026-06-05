---
name: cellbender
description: Remove ambient RNA from raw scRNA-seq count matrices using CellBender's remove-background. GPU-strongly-recommended (~30 min on GPU vs hours on CPU for typical data). Takes raw 10X h5 (or h5ad with all-barcodes-included), trains a variational model that separates real cell counts from background contamination, and writes a denoised h5 plus a diagnostic HTML report. Includes SLURM template for GPU clusters.
license: BSD-3-Clause
metadata:
---

# CellBender: Ambient-RNA Removal from scRNA-seq

## Overview

[CellBender](https://cellbender.readthedocs.io/) removes **ambient RNA contamination** from droplet-based scRNA-seq count matrices. Ambient RNA — transcripts released from lysed cells that get packaged into otherwise-empty droplets — inflates "expression" of cell-type-specific markers across all cells, leading to false-positive cluster markers and confused cell-type calls.

The `remove-background` module trains a deep variational autoencoder on **all barcodes** (including empty droplets) to learn what background looks like, then subtracts it from cell-containing barcodes. The output is a denoised count matrix that drops in as the input to standard scanpy / Seurat workflows.

GPU is strongly recommended: **~30 min on GPU** vs **multiple hours on CPU** for typical 10X datasets (~100k–1M barcodes including empties).

## When to Use This Skill

- You have **raw, unfiltered** 10X output (`raw_feature_bc_matrix.h5` or `raw_feature_bc_matrix/`) — CellBender needs the empty droplets to model background.
- Your clustering shows **marker leakage** across cell types (e.g. T-cell markers showing up in B cells; hemoglobin in non-RBCs from lysed RBCs in blood samples).
- You're doing **nuclei** RNA-seq, which is especially prone to ambient contamination from cytoplasmic transcripts.
- You're working on **immune-tissue samples** (PBMC, tumour) where lytic cells contribute heavy background.

**Not for**:
- Already-filtered matrices (`filtered_feature_bc_matrix.h5`) — CellBender needs empty barcodes too
- Doublet detection (use Scrublet / DoubletFinder; CellBender doesn't touch this)
- Cross-sample batch correction (use Harmony / scVI after CellBender)

## Prerequisites

### Hardware (GPU strongly recommended)

| Hardware | Runtime (~500k barcodes) | Notes |
|---|---|---|
| NVIDIA GPU (V100 / A100 / H100) | ~20-40 min | The intended path |
| NVIDIA GPU (T4 / RTX 30xx) | ~30-60 min | Fine for most datasets |
| CPU only | 4-12 hours | Functional but painful |

### Software

```bash
# Recommended: dedicated conda env (CellBender is finicky about PyTorch versions)
conda create -n cellbender python=3.10 -y
conda activate cellbender
pip install cellbender

# Verify GPU detection
python -c "import torch; print('CUDA:', torch.cuda.is_available()); \
                       print('Device:', torch.cuda.get_device_name(0))"
# Expected on a working GPU node:
#   CUDA: True
#   Device: Tesla V100-SXM2-32GB
```

If `torch.cuda.is_available()` is `False` on a GPU node, your PyTorch was installed CPU-only — reinstall with the CUDA wheel:

```bash
pip uninstall torch torchvision
# Match the CUDA version of the cluster's drivers (check with `nvidia-smi`)
pip install torch --index-url https://download.pytorch.org/whl/cu121
```

## Quick Start

### Step 1 — Locate raw input

CellBender needs the **raw** (not filtered) output from cellranger / starsolo:

```bash
# 10X cellranger output
ls /path/to/sample_outs/raw_feature_bc_matrix.h5

# Or the directory variant
ls /path/to/sample_outs/raw_feature_bc_matrix/
# barcodes.tsv.gz  features.tsv.gz  matrix.mtx.gz
```

Both work. CellBender also reads `.h5ad` if the original raw matrix was stored that way.

### Step 2 — Run remove-background

```bash
cellbender remove-background \
    --cuda \
    --input  /path/to/raw_feature_bc_matrix.h5 \
    --output sample_cellbender.h5 \
    --epochs 150 \
    --fpr    0.01
```

That's the minimum. Since CellBender v0.3.0, `--expected-cells` and `--total-droplets-included` are **auto-detected** — only override them if the report flags a problem.

### Step 3 — Check the report

CellBender writes a directory of output files. The two to look at first:

```bash
sample_cellbender_report.html    # interactive diagnostic — warnings + plots
sample_cellbender.pdf            # static plots: ELBO loss curves, UMI rank + cell prob, PCA
```

Open `report.html` in a browser. Look for:

- **ELBO loss curve** monotonically decreasing and flattening — good. If it's still descending steeply at epoch 150, raise `--epochs 200` and re-run.
- **Cell probability vs UMI rank** sharp drop at the expected cell count — good. A long shoulder or no drop at all → CellBender thinks more (or fewer) droplets are cells than reality. Set `--expected-cells` and `--total-droplets-included` manually.
- **No yellow / red warnings** in the warnings panel.

### Step 4 — Load the denoised matrix downstream

```python
# scanpy / Python
from cellbender.remove_background.downstream import anndata_from_h5
adata = anndata_from_h5('sample_cellbender.h5')
# Keep barcodes CellBender called as cells (post.cell_probability > 0.5)
adata = adata[adata.obs['cell_probability'] > 0.5].copy()
```

```r
# Seurat / R — needs a small h5 conversion first
# (CellBender writes a non-Seurat-compatible structure; ptrepack flattens it)
ptrepack --complevel 5 sample_cellbender_filtered.h5:/matrix \
                       sample_cellbender_seurat.h5:/matrix
```
```r
library(Seurat)
counts <- Read10X_h5('sample_cellbender_seurat.h5', use.names = TRUE)
seurat_obj <- CreateSeuratObject(counts = counts)
```

Convenience: `bash scripts/run_cellbender.sh /path/to/raw.h5 sample_out_dir` (single sample) or `sbatch scripts/cellbender_slurm.sh` (cluster batch).

## All Command-Line Options

| Flag | Default / Recommendation | What it does |
|---|---|---|
| `--input` | required | Path to raw 10X `.h5`, raw `.mtx` dir, or `.h5ad` |
| `--output` | required | Output `.h5` path (CellBender derives sibling files from this) |
| `--cuda` | **always include if you have a GPU** | Use CUDA-accelerated PyTorch |
| `--epochs` | 150 (max ~300) | Training iterations. Bump to 200-300 if ELBO is still falling |
| `--expected-cells` | auto (v0.3.0+) | Override if the rank plot shows a wrong elbow |
| `--total-droplets-included` | auto (v0.3.0+) | Total barcodes to consider — empties + cells. Override if `expected-cells` was overridden |
| `--fpr` | 0.01 (conservative) | "False positive rate" — fraction of real counts allowed to be removed. Bump to 0.05-0.1 for more aggressive cleanup |
| `--learning-rate` | 1e-4 | Adam optimizer step size; rarely need to change |
| `--posterior-batch-size` | 128 | Inference batch size; lower if out of memory |
| `--z-dim` | 100 | Latent dim; rarely change |
| `--checkpoint` | (none) | Re-load a prior `ckpt.tar.gz` to skip training |
| `--model` | full | Model variant; `simple` is faster but less robust on contaminated data |

## Output Files

```
sample_cellbender.h5                  # full denoised matrix (all barcodes, with cell prob)
sample_cellbender_filtered.h5         # cells only (cell_prob > 0.5) — ready for scanpy/Seurat
sample_cellbender_cell_barcodes.csv   # list of called cell barcodes
sample_cellbender_report.html         # interactive diagnostics — open in browser
sample_cellbender.pdf                 # ELBO + rank + PCA static plots
sample_cellbender.log                 # full run log (verbose)
sample_cellbender_metrics.csv         # per-cell metrics (counts removed, etc.)
sample_cellbender_posterior.h5        # noise probability matrix (rarely needed)
ckpt.tar.gz                            # model checkpoint — delete if disk-constrained
```

The two you'll routinely use: `*_filtered.h5` for downstream and `*_report.html` for QC. Everything else is for debugging.

## SLURM Template for GPU Clusters

```bash
#!/bin/bash
#SBATCH --job-name=cellbender
#SBATCH --partition=gpu                # or your cluster's GPU partition
#SBATCH --gres=gpu:1                    # one GPU
#SBATCH --cpus-per-task=4
#SBATCH --mem=32G
#SBATCH --time=02:00:00
#SBATCH --output=logs/cellbender_%j.out

source ~/.bashrc
conda activate cellbender

cd /path/to/output_dir
cellbender remove-background \
    --cuda \
    --input  /shared/scratch/sample/raw_feature_bc_matrix.h5 \
    --output sample_cellbender.h5 \
    --epochs 150 \
    --fpr    0.01
```

Per-sample sbatch job, scales trivially to a sample array (see `scripts/cellbender_slurm.sh` for the array variant).

## Tuning by Inspecting the Report

The default settings work for most clean datasets. If the report flags issues:

### "ELBO still descending steeply at last epoch"
→ Increase `--epochs` to 200 or 250.

### "Cell probability transition is gradual / no sharp drop"
The model can't tell cells from empties cleanly. Two reasons:
1. **You picked the wrong `total_droplets_included`** — override to a smaller value (e.g. 25000 if cellranger's barcoderank elbow sits around 10k).
2. **The sample has very few real cells with low UMI counts** — sometimes unavoidable for low-quality libraries. Increase `--fpr` to 0.05 to remove more background.

### "Many genes with > 50% counts removed"
The model is over-correcting. Lower `--fpr` from 0.01 → 0.001 to be more conservative.

### "Run failed with CUDA out-of-memory"
- Reduce `--posterior-batch-size` from 128 to 64 (or 32).
- Reduce `--total-droplets-included` (most empties carry no signal beyond a certain rank).
- Switch to a higher-memory GPU.

## Best Practices

- **Run CellBender first**, before any downstream QC. Cells filtered out by CellBender shouldn't be the ones you cluster anyway.
- **Default to `--fpr 0.01`** unless you have strong contamination signals. Raising it to 0.1 throws away real signal.
- **Run per-sample**, not per-pool of multiple samples. Background composition differs between samples.
- **Always check the report.** A "successful" CellBender run can still be wrong — the report's UMI-rank plot is the smoking gun.
- **Save `ckpt.tar.gz`** until you've validated the downstream — re-running from checkpoint is much faster than retraining from scratch.
- **Compare UMAPs with and without CellBender** for at least one sample to confirm the changes are biologically reasonable, not introducing artifacts. The CellBender paper recommends this validation step explicitly.

## Common Pitfalls

- **Passing filtered_feature_bc_matrix.h5 by mistake.** CellBender needs empties to model background — passing the filtered matrix fails or gives garbage results.
- **Running without `--cuda` on a GPU node.** Easy to forget; the run will silently fall back to CPU and take 10× longer.
- **Trying to chain multiple samples in one CellBender call.** Always per-sample.
- **Reading `*_cellbender.h5` (unfiltered) directly into Seurat.** Use the `_filtered.h5` and run `ptrepack` first (Seurat's `Read10X_h5` is strict about HDF5 layout).

## End-to-End Template

`assets/cellbender_template.sh` — configures one sample's input/output paths + flags, runs the cellbender call, opens the report.

## Convenience Scripts

- `scripts/run_cellbender.sh` — single-sample wrapper with GPU detection
- `scripts/cellbender_slurm.sh` — SLURM array template (one sample per task)
- `scripts/load_into_scanpy.py` — read `_filtered.h5` into scanpy + standard preprocessing

## References

- [CellBender documentation](https://cellbender.readthedocs.io/) — Broad Institute
- [Introduction](https://cellbender.readthedocs.io/en/stable/introduction/index.html)
- [Installation](https://cellbender.readthedocs.io/en/stable/installation/index.html)
- [Usage](https://cellbender.readthedocs.io/en/stable/usage/index.html)
- [Tutorial](https://cellbender.readthedocs.io/en/stable/tutorial/index.html)
- Fleming et al. (2023), *Unsupervised removal of systematic background noise from droplet-based single-cell experiments using CellBender*, *Nature Methods*
- Source: [github.com/broadinstitute/CellBender](https://github.com/broadinstitute/CellBender)
