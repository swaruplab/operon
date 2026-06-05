#!/usr/bin/env bash
# kallisto_template.sh — end-to-end kallisto-bustools quantification.
#
# Edit the CONFIGURATION block, then run end-to-end:
#   1. Build the reference index (or skip if it exists)
#   2. Quantify each sample
#   3. Print summary + downstream loading instructions

set -euo pipefail

# ============================================================================
# CONFIGURATION
# ============================================================================

# Reference — either prebuilt or build from FASTA+GTF
PREBUILT=""                                    # "human" | "mouse" | "" (build custom)
GENOME_FASTA="GRCh38.primary_assembly.genome.fa.gz"
GTF="gencode.v44.annotation.gtf.gz"
WORKFLOW="standard"                            # standard | nac | lamanno

REF_DIR="reference"
INDEX="$REF_DIR/index.idx"
T2G="$REF_DIR/t2g.txt"

# Samples — name → (R1, R2) pairs
declare -a SAMPLE_NAMES=("sample1" "sample2")
declare -a SAMPLE_FASTQS=(
  # one entry per sample, space-separated R1 R2 (or interleaved lanes: R1 R2 R1 R2 …)
  "fastq/sample1_R1.fastq.gz fastq/sample1_R2.fastq.gz"
  "fastq/sample2_R1.fastq.gz fastq/sample2_R2.fastq.gz"
)
TECH="10xv3"                                   # see `kb --list`

# Performance
THREADS=8
MEMORY="16G"

# Output
OUT_ROOT="results"

# ============================================================================
# SETUP
# ============================================================================

mkdir -p "$REF_DIR" "$OUT_ROOT"

if ! command -v kb >/dev/null 2>&1; then
  echo "ERROR: 'kb' not found. Install: pip install kb_python" >&2
  exit 1
fi

# ============================================================================
# 1. BUILD INDEX (if needed)
# ============================================================================

if [[ -f "$INDEX" && -f "$T2G" ]]; then
  echo "=== 1. Index already exists at $INDEX — skipping build ==="
elif [[ -n "$PREBUILT" ]]; then
  echo "=== 1. Downloading prebuilt index ($PREBUILT) ==="
  kb ref -d "$PREBUILT" -i "$INDEX" -g "$T2G"
else
  echo "=== 1. Building index from $GENOME_FASTA + $GTF (workflow=$WORKFLOW) ==="
  case "$WORKFLOW" in
    standard)
      kb ref -i "$INDEX" -g "$T2G" -f1 "$REF_DIR/cdna.fasta" \
              "$GENOME_FASTA" "$GTF"
      ;;
    nac)
      kb ref --workflow nac \
              -i "$INDEX" -g "$T2G" \
              -c1 "$REF_DIR/cdna.txt" -c2 "$REF_DIR/nascent.txt" \
              -f1 "$REF_DIR/cdna.fasta" -f2 "$REF_DIR/nascent.fasta" \
              "$GENOME_FASTA" "$GTF"
      ;;
    lamanno)
      kb ref --workflow lamanno \
              -i "$INDEX" -g "$T2G" \
              -c1 "$REF_DIR/cdna_t2c.txt" -c2 "$REF_DIR/intron_t2c.txt" \
              -f1 "$REF_DIR/cdna.fasta" -f2 "$REF_DIR/intron.fasta" \
              "$GENOME_FASTA" "$GTF"
      ;;
  esac
fi

# ============================================================================
# 2. QUANTIFY EACH SAMPLE
# ============================================================================

for i in "${!SAMPLE_NAMES[@]}"; do
  SAMPLE_NAME="${SAMPLE_NAMES[$i]}"
  FASTQS=(${SAMPLE_FASTQS[$i]})                # space-split into array
  SAMPLE_OUT="$OUT_ROOT/$SAMPLE_NAME"

  echo ""
  echo "=== 2. Quantifying $SAMPLE_NAME ==="
  echo "    FASTQs: ${FASTQS[*]}"
  echo "    Output: $SAMPLE_OUT"
  echo ""

  KB_ARGS=(
    -i "$INDEX" -g "$T2G"
    -x "$TECH"
    -o "$SAMPLE_OUT"
    -t "$THREADS" -m "$MEMORY"
    --h5ad
  )
  case "$WORKFLOW" in
    nac|lamanno) KB_ARGS+=(--workflow "$WORKFLOW") ;;
  esac

  kb count "${KB_ARGS[@]}" "${FASTQS[@]}"

  # Quick summary
  if [[ -f "$SAMPLE_OUT/run_info.json" ]]; then
    python3 -c "
import json
with open('$SAMPLE_OUT/run_info.json') as f: d = json.load(f)
print(f'    Reads: {d.get(\"n_processed\", 0):,}, pseudoaligned: {d.get(\"p_pseudoaligned\", 0):.2f}%')
" 2>/dev/null || true
  fi
done

# ============================================================================
# 3. SUMMARY
# ============================================================================

echo ""
echo "=== 3. All samples done ==="
for SAMPLE_NAME in "${SAMPLE_NAMES[@]}"; do
  echo "  $OUT_ROOT/$SAMPLE_NAME/counts_unfiltered/adata.h5ad"
done

echo ""
echo "Load all samples into one scanpy AnnData:"
cat <<'PYTHON'

import scanpy as sc
import anndata
import os

samples = ${SAMPLE_NAMES[@]}    # e.g. ("sample1" "sample2")
adatas = []
for s in samples:
    a = anndata.read_h5ad(f"$OUT_ROOT/{s}/counts_unfiltered/adata.h5ad")
    a.obs["sample"] = s
    adatas.append(a)

adata = anndata.concat(adatas, label="sample", keys=samples, index_unique="-")
print(adata)
# Now proceed with scanpy QC + clustering — see the scanpy protocol
PYTHON
