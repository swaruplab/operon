#!/usr/bin/env bash
# run_cellbender.sh — single-sample CellBender wrapper.
#
# Usage:
#   bash run_cellbender.sh INPUT_H5 OUTPUT_DIR [--cpu] [--epochs N] [--fpr X]
#
# Examples:
#   bash run_cellbender.sh /data/sample1/raw_feature_bc_matrix.h5 results/sample1
#   bash run_cellbender.sh raw.h5 out --epochs 200 --fpr 0.05
#   bash run_cellbender.sh raw.h5 out --cpu                      # force CPU
#
# Checks for GPU availability and uses --cuda automatically unless --cpu is passed.

set -euo pipefail

if [[ $# -lt 2 ]]; then
  echo "Usage: $0 INPUT_H5 OUTPUT_DIR [--cpu] [--epochs N] [--fpr X] [--expected-cells N]" >&2
  exit 1
fi

INPUT="$1"
OUTDIR="$2"
shift 2

USE_CUDA=true
EPOCHS=150
FPR=0.01
EXTRA_ARGS=()

while [[ $# -gt 0 ]]; do
  case "$1" in
    --cpu)
      USE_CUDA=false
      shift ;;
    --epochs)
      EPOCHS="$2"
      shift 2 ;;
    --fpr)
      FPR="$2"
      shift 2 ;;
    --expected-cells|--total-droplets-included|--learning-rate|--z-dim|--posterior-batch-size)
      EXTRA_ARGS+=("$1" "$2")
      shift 2 ;;
    *)
      echo "Unknown arg: $1" >&2
      exit 1 ;;
  esac
done

if [[ ! -f "$INPUT" && ! -d "$INPUT" ]]; then
  echo "ERROR: input not found: $INPUT" >&2
  exit 1
fi

if ! command -v cellbender >/dev/null 2>&1; then
  echo "ERROR: 'cellbender' not on PATH. Install with: pip install cellbender" >&2
  exit 1
fi

mkdir -p "$OUTDIR"
SAMPLE_NAME=$(basename "$OUTDIR")
OUTPUT_H5="$OUTDIR/${SAMPLE_NAME}_cellbender.h5"

# GPU detection
CUDA_FLAG=""
if $USE_CUDA; then
  GPU_OK=$(python -c "import torch; print(torch.cuda.is_available())" 2>/dev/null || echo "False")
  if [[ "$GPU_OK" == "True" ]]; then
    CUDA_FLAG="--cuda"
    GPU_NAME=$(python -c "import torch; print(torch.cuda.get_device_name(0))")
    echo "GPU detected: $GPU_NAME — using --cuda"
  else
    echo "WARNING: --cuda requested but no GPU detected. Falling back to CPU (will be slow)."
  fi
else
  echo "Running on CPU (--cpu flag passed). Expect hours of runtime."
fi

echo "Input:  $INPUT"
echo "Output: $OUTPUT_H5"
echo "Epochs: $EPOCHS  |  FPR: $FPR"
echo ""

cellbender remove-background \
    $CUDA_FLAG \
    --input  "$INPUT" \
    --output "$OUTPUT_H5" \
    --epochs "$EPOCHS" \
    --fpr    "$FPR" \
    "${EXTRA_ARGS[@]}"

echo ""
echo "Done. Outputs in: $OUTDIR/"
echo ""
echo "Inspect the report before downstream:"
echo "  $OUTDIR/${SAMPLE_NAME}_cellbender_report.html"
echo ""
echo "Load the filtered matrix into scanpy:"
echo "  from cellbender.remove_background.downstream import anndata_from_h5"
echo "  adata = anndata_from_h5('$OUTDIR/${SAMPLE_NAME}_cellbender_filtered.h5')"
