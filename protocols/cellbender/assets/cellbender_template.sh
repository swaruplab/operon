#!/usr/bin/env bash
# cellbender_template.sh — end-to-end CellBender run for a single sample.
#
# Set the paths and parameters below, then run:
#   bash cellbender_template.sh
#
# Optimized for GPU; falls back to CPU with a warning if no GPU detected.

set -euo pipefail

# ============================================================================
# CONFIGURATION — edit these
# ============================================================================

SAMPLE_NAME="sample1"
INPUT_H5="/path/to/cellranger/sample1_outs/raw_feature_bc_matrix.h5"
OUTPUT_DIR="results/cellbender/${SAMPLE_NAME}"

# Training
EPOCHS=150
FPR=0.01                          # 0.01 = conservative, 0.05-0.1 = aggressive

# Optional overrides — uncomment and set if --expected-cells auto-detection fails
# EXPECTED_CELLS=8000
# TOTAL_DROPLETS_INCLUDED=20000

# Conda env (set to "" if cellbender is on PATH directly)
CONDA_ENV="cellbender"

# Downstream — set to true to auto-run scanpy preprocessing on the filtered output
RUN_DOWNSTREAM=true

# ============================================================================
# SETUP
# ============================================================================

mkdir -p "$OUTPUT_DIR"
cd "$OUTPUT_DIR"

if [[ -n "$CONDA_ENV" ]]; then
  source ~/.bashrc
  conda activate "$CONDA_ENV"
fi

# Detect GPU
CUDA_FLAG=""
GPU_OK=$(python -c "import torch; print(torch.cuda.is_available())" 2>/dev/null || echo "False")
if [[ "$GPU_OK" == "True" ]]; then
  CUDA_FLAG="--cuda"
  GPU_NAME=$(python -c "import torch; print(torch.cuda.get_device_name(0))")
  echo "[$(date)] GPU detected: $GPU_NAME"
else
  echo "[$(date)] WARNING: no GPU detected — will run on CPU (slow)."
fi

# ============================================================================
# RUN CELLBENDER
# ============================================================================

OUTPUT_H5="${SAMPLE_NAME}_cellbender.h5"

CB_ARGS=(
  $CUDA_FLAG
  --input  "$INPUT_H5"
  --output "$OUTPUT_H5"
  --epochs "$EPOCHS"
  --fpr    "$FPR"
)

# Add optional overrides if set
if [[ "${EXPECTED_CELLS:-}" != "" ]]; then
  CB_ARGS+=(--expected-cells "$EXPECTED_CELLS")
fi
if [[ "${TOTAL_DROPLETS_INCLUDED:-}" != "" ]]; then
  CB_ARGS+=(--total-droplets-included "$TOTAL_DROPLETS_INCLUDED")
fi

echo "[$(date)] Starting CellBender on $SAMPLE_NAME …"
echo "  Input:  $INPUT_H5"
echo "  Output: $(pwd)/$OUTPUT_H5"
echo ""

cellbender remove-background "${CB_ARGS[@]}"

echo ""
echo "[$(date)] CellBender done."
echo "Inspect the report:"
echo "  $(pwd)/${SAMPLE_NAME}_cellbender_report.html"
echo ""

# ============================================================================
# DOWNSTREAM (optional)
# ============================================================================

if $RUN_DOWNSTREAM; then
  echo "[$(date)] Running standard scanpy preprocessing on the filtered output …"
  FILTERED_H5="${SAMPLE_NAME}_cellbender_filtered.h5"
  OUTPUT_H5AD="${SAMPLE_NAME}_processed.h5ad"

  # Resolve the path to load_into_scanpy.py from this protocol's scripts/ dir.
  # Adjust if your layout differs.
  LOADER="$(dirname "${BASH_SOURCE[0]}")/../scripts/load_into_scanpy.py"
  if [[ ! -f "$LOADER" ]]; then
    echo "WARNING: $LOADER not found — skipping downstream. Run manually:"
    echo "  python load_into_scanpy.py $FILTERED_H5 --output $OUTPUT_H5AD"
  else
    python "$LOADER" "$FILTERED_H5" --output "$OUTPUT_H5AD"
    echo ""
    echo "Done. Processed AnnData: $(pwd)/$OUTPUT_H5AD"
  fi
fi

echo ""
echo "Next steps:"
echo "  1. Open $(pwd)/${SAMPLE_NAME}_cellbender_report.html to QC the run"
echo "  2. Validate by comparing UMAPs with and without CellBender"
echo "  3. If everything looks good, this sample's denoised matrix is ready for downstream"
