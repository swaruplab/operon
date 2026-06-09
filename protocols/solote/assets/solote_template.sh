#!/usr/bin/env bash
set -euo pipefail

# ============================================================================
# CONFIGURATION — edit these (or override at runtime via CLI flags below)
# ============================================================================

SAMPLE_NAME="sample1"
INPUT_BAM="/path/to/cellranger/sample1_outs/possorted_genome_bam.bam"
TE_ANNOTATION="/path/to/hg38_rmsk.bed"
OUTPUT_DIR="results/solote/${SAMPLE_NAME}"
GENOME_BUILD="hg38"
N_THREADS=8

SOLOTE_DIR="${SOLOTE_DIR:-$HOME/SoloTE}"
CONDA_ENV="${CONDA_ENV:-solote}"

# ============================================================================

usage() {
  cat <<EOF
Usage: $0 [options]

Options:
  -b, --bam PATH           Input cell-barcoded BAM (default: \$INPUT_BAM)
  -t, --teannotation PATH  TE annotation BED (default: \$TE_ANNOTATION)
  -o, --output DIR         Output directory (default: \$OUTPUT_DIR)
  -p, --prefix NAME        Sample prefix (default: \$SAMPLE_NAME)
  -g, --genome BUILD       Genome build for BED auto-build if missing (default: \$GENOME_BUILD)
  -j, --threads N          Worker threads (default: \$N_THREADS)
  -h, --help               Show this help and exit
EOF
}

# getopts doesn't support GNU-style long flags portably; parse by hand.
while [[ $# -gt 0 ]]; do
  case "$1" in
    -b|--bam)          INPUT_BAM="$2"; shift 2 ;;
    -t|--teannotation) TE_ANNOTATION="$2"; shift 2 ;;
    -o|--output)       OUTPUT_DIR="$2"; shift 2 ;;
    -p|--prefix)       SAMPLE_NAME="$2"; shift 2 ;;
    -g|--genome)       GENOME_BUILD="$2"; shift 2 ;;
    -j|--threads)      N_THREADS="$2"; shift 2 ;;
    -h|--help)         usage; exit 0 ;;
    *) echo "Unknown argument: $1" >&2; usage; exit 2 ;;
  esac
done

# ============================================================================
# VALIDATION
# ============================================================================

if [[ -n "${CONDA_ENV}" ]] && command -v conda >/dev/null 2>&1; then
  # shellcheck disable=SC1091
  source "$(conda info --base)/etc/profile.d/conda.sh"
  conda activate "${CONDA_ENV}" || echo "WARNING: could not activate conda env '${CONDA_ENV}' — assuming tools are on PATH"
fi

for tool in samtools bedtools python; do
  command -v "$tool" >/dev/null 2>&1 || { echo "ERROR: '$tool' not found on PATH" >&2; exit 1; }
done

[[ -d "${SOLOTE_DIR}" ]] || { echo "ERROR: SoloTE checkout not found at '${SOLOTE_DIR}'. Set SOLOTE_DIR or clone https://github.com/bvaldebenitom/SoloTE.git" >&2; exit 1; }
[[ -f "${SOLOTE_DIR}/SoloTE_pipeline.py" ]] || { echo "ERROR: SoloTE_pipeline.py missing in '${SOLOTE_DIR}'" >&2; exit 1; }

[[ -f "${INPUT_BAM}" ]] || { echo "ERROR: input BAM not found: ${INPUT_BAM}" >&2; exit 1; }

# Confirm the BAM carries the CB tag — SoloTE produces an empty matrix without it.
if ! samtools view "${INPUT_BAM}" | head -n 200 | grep -q "CB:Z:"; then
  echo "ERROR: no CB:Z: tag found in the first 200 reads of ${INPUT_BAM}." >&2
  echo "       SoloTE requires a cell-barcoded BAM (Cell Ranger possorted_genome_bam.bam or STARsolo --soloFeatures output)." >&2
  exit 1
fi

if [[ ! -f "${TE_ANNOTATION}" ]]; then
  echo "TE annotation BED not found: ${TE_ANNOTATION}"
  echo "Attempting to build one for genome '${GENOME_BUILD}' …"
  python "${SOLOTE_DIR}/SoloTE_RepeatMasker_to_BED.py" -g "${GENOME_BUILD}"
  TE_ANNOTATION="${GENOME_BUILD}_rmsk.bed"
  [[ -f "${TE_ANNOTATION}" ]] || { echo "ERROR: failed to build ${TE_ANNOTATION}" >&2; exit 1; }
fi

mkdir -p "${OUTPUT_DIR}"

# ============================================================================
# RUN
# ============================================================================

python "${SOLOTE_DIR}/SoloTE_pipeline.py" \
  --threads "${N_THREADS}" \
  --bam "${INPUT_BAM}" \
  --teannotation "${TE_ANNOTATION}" \
  --outputprefix "${SAMPLE_NAME}" \
  --outputdir "${OUTPUT_DIR}"

MATRIX_DIR="${OUTPUT_DIR}/${SAMPLE_NAME}_SoloTE_output"
N_FEATURES=$(wc -l < "${MATRIX_DIR}/features.tsv" 2>/dev/null || echo "?")
N_BARCODES=$(wc -l < "${MATRIX_DIR}/barcodes.tsv" 2>/dev/null || echo "?")

echo "SoloTE done: ${SAMPLE_NAME} — ${N_FEATURES} features x ${N_BARCODES} barcodes written to ${MATRIX_DIR}"
