#!/bin/bash
# cellbender_slurm.sh — SLURM array template for batch CellBender across samples.
#
# Each array task processes one sample. Configure the sample list and paths below,
# then submit with:
#
#   sbatch --array=0-$(($(wc -l < samples.txt) - 1)) cellbender_slurm.sh
#
# Where samples.txt contains one raw-matrix path per line:
#   /path/to/sample1/raw_feature_bc_matrix.h5
#   /path/to/sample2/raw_feature_bc_matrix.h5
#   ...

#SBATCH --job-name=cellbender
#SBATCH --partition=gpu                # change to your cluster's GPU partition
#SBATCH --gres=gpu:1                    # 1 GPU per task
#SBATCH --cpus-per-task=4
#SBATCH --mem=32G
#SBATCH --time=02:00:00                 # 30 min typical, 2h for headroom
#SBATCH --output=logs/cellbender_%A_%a.out
#SBATCH --error=logs/cellbender_%A_%a.err

set -euo pipefail

# ── CONFIGURATION ──────────────────────────────────────────────────────────
SAMPLES_FILE="samples.txt"              # one input path per line
OUTPUT_ROOT="results/cellbender"        # one subdir per sample
EPOCHS=150
FPR=0.01
CONDA_ENV="cellbender"

# ── SETUP ──────────────────────────────────────────────────────────────────
mkdir -p logs "$OUTPUT_ROOT"

source ~/.bashrc
conda activate "$CONDA_ENV"

# Pick the input for this array task
if [[ ! -f "$SAMPLES_FILE" ]]; then
  echo "ERROR: $SAMPLES_FILE not found in $(pwd)" >&2
  exit 1
fi
INPUT=$(sed -n "$((SLURM_ARRAY_TASK_ID + 1))p" "$SAMPLES_FILE")
if [[ -z "$INPUT" ]]; then
  echo "ERROR: no sample at index $SLURM_ARRAY_TASK_ID in $SAMPLES_FILE" >&2
  exit 1
fi

# Derive sample name from the path
SAMPLE_NAME=$(basename "$(dirname "$INPUT")")
OUTDIR="$OUTPUT_ROOT/$SAMPLE_NAME"
mkdir -p "$OUTDIR"
cd "$OUTDIR"

echo "[$(date)] Task $SLURM_ARRAY_TASK_ID — sample: $SAMPLE_NAME"
echo "Input:  $INPUT"
echo "Output: $OUTDIR/${SAMPLE_NAME}_cellbender.h5"

# Sanity-check GPU
python -c "import torch; assert torch.cuda.is_available(), 'CUDA not available!'; \
            print('Device:', torch.cuda.get_device_name(0))"

# ── RUN ────────────────────────────────────────────────────────────────────
cellbender remove-background \
    --cuda \
    --input  "$INPUT" \
    --output "${SAMPLE_NAME}_cellbender.h5" \
    --epochs "$EPOCHS" \
    --fpr    "$FPR"

echo "[$(date)] Done — $SAMPLE_NAME"
echo "Report: $OUTDIR/${SAMPLE_NAME}_cellbender_report.html"
