#!/usr/bin/env bash
set -euo pipefail

# ============================================================================
# CONFIGURATION — edit these or override via CLI flags
# ============================================================================
INPUT_BAM="${INPUT_BAM:-}"
OUTPUT_PREFIX="${OUTPUT_PREFIX:-scte_out}"
GENOME_INDEX="${GENOME_INDEX:-}"
N_THREADS="${N_THREADS:-1}"
CB_TAG="${CB_TAG:-CB}"
UMI_TAG="${UMI_TAG:-UB}"
MODE="${MODE:-exclusive}"
HDF5="${HDF5:-True}"
# ============================================================================

usage() {
  cat <<EOF
Usage: $(basename "$0") [options]

Required:
  -i, --input PATH        Aligned BAM/SAM file (cell barcode + UMI tags required)
  -x, --index PATH        scTE genome index (.idx)

Optional:
  -o, --output PREFIX     Output file prefix          (default: ${OUTPUT_PREFIX})
  -p, --threads N         Threads (~10 GB RAM each)   (default: ${N_THREADS})
      --cb TAG            Cell barcode BAM tag        (default: ${CB_TAG})
      --umi TAG           UMI BAM tag                 (default: ${UMI_TAG})
  -m, --mode MODE         exclusive|inclusive|nointron (default: ${MODE})
      --hdf5 BOOL         Emit .h5ad AnnData          (default: ${HDF5})
  -h, --help              Show this message
EOF
}

# getopts does not natively handle GNU-style long options; manual shift loop instead.
while [[ $# -gt 0 ]]; do
  case "$1" in
    -i|--input)   INPUT_BAM="$2"; shift 2 ;;
    -x|--index)   GENOME_INDEX="$2"; shift 2 ;;
    -o|--output)  OUTPUT_PREFIX="$2"; shift 2 ;;
    -p|--threads) N_THREADS="$2"; shift 2 ;;
    --cb)         CB_TAG="$2"; shift 2 ;;
    --umi)        UMI_TAG="$2"; shift 2 ;;
    -m|--mode)    MODE="$2"; shift 2 ;;
    --hdf5)       HDF5="$2"; shift 2 ;;
    -h|--help)    usage; exit 0 ;;
    *) echo "Error: unknown argument: $1" >&2; usage; exit 2 ;;
  esac
done

if [[ -z "${INPUT_BAM}" ]]; then
  echo "Error: --input BAM is required" >&2; usage; exit 2
fi
if [[ -z "${GENOME_INDEX}" ]]; then
  echo "Error: --index .idx file is required" >&2; usage; exit 2
fi
if [[ ! -f "${INPUT_BAM}" ]]; then
  echo "Error: input BAM not found: ${INPUT_BAM}" >&2; exit 2
fi
if [[ ! -f "${GENOME_INDEX}" ]]; then
  echo "Error: genome index not found: ${GENOME_INDEX}" >&2; exit 2
fi

if ! command -v scTE >/dev/null 2>&1; then
  echo "Error: scTE not on PATH. Install via: git clone https://github.com/JiekaiLab/scTE.git && cd scTE && python setup.py install" >&2
  exit 127
fi

case "${MODE}" in
  exclusive|inclusive|nointron) ;;
  *) echo "Error: --mode must be exclusive|inclusive|nointron (got: ${MODE})" >&2; exit 2 ;;
esac

OUTPUT_DIR="$(dirname -- "${OUTPUT_PREFIX}")"
mkdir -p "${OUTPUT_DIR}"

scTE \
  -i "${INPUT_BAM}" \
  -o "${OUTPUT_PREFIX}" \
  -x "${GENOME_INDEX}" \
  -p "${N_THREADS}" \
  -CB "${CB_TAG}" \
  -UMI "${UMI_TAG}" \
  -m "${MODE}" \
  --hdf5 "${HDF5}"

OUT_SUFFIX=$([[ "${HDF5}" == "True" ]] && echo ".h5ad" || echo ".csv")
echo "scTE complete: ${OUTPUT_PREFIX}${OUT_SUFFIX} (mode=${MODE}, threads=${N_THREADS}, CB=${CB_TAG}, UMI=${UMI_TAG})"
