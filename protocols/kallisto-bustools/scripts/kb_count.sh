#!/usr/bin/env bash
# kb_count.sh — quantify a single-cell sample with kb count.
#
# Usage:
#   bash kb_count.sh --index index.idx --t2g t2g.txt --tech 10xv3 \
#                     --out-dir sample1 sample1_R1.fastq.gz sample1_R2.fastq.gz
#
#   # nac workflow (snRNA-seq / velocity)
#   bash kb_count.sh --index index.idx --t2g t2g.txt --tech 10xv3 \
#                     --workflow nac --out-dir sample1 \
#                     sample1_R1.fastq.gz sample1_R2.fastq.gz
#
#   # Multiple lanes — R1 R2 R1 R2 ... interleaved per lane
#   bash kb_count.sh --index index.idx --t2g t2g.txt --tech 10xv3 \
#                     --out-dir sample1 \
#                     s1_L001_R1.fq.gz s1_L001_R2.fq.gz \
#                     s1_L002_R1.fq.gz s1_L002_R2.fq.gz

set -euo pipefail

INDEX=""
T2G=""
TECH=""
OUT_DIR=""
WORKFLOW="standard"
THREADS=8
MEMORY="8G"
FILTER=""

POS=()
while [[ $# -gt 0 ]]; do
  case "$1" in
    --index)     INDEX="$2";     shift 2 ;;
    --t2g)       T2G="$2";       shift 2 ;;
    --tech|-x)   TECH="$2";      shift 2 ;;
    --out-dir|-o) OUT_DIR="$2";  shift 2 ;;
    --workflow)  WORKFLOW="$2";  shift 2 ;;
    --threads|-t) THREADS="$2";  shift 2 ;;
    --memory|-m) MEMORY="$2";    shift 2 ;;
    --filter)    FILTER="--filter $2"; shift 2 ;;
    -h|--help)
      sed -n '2,17p' "$0"
      exit 0
      ;;
    *)
      POS+=("$1"); shift ;;
  esac
done

if [[ -z "$INDEX" || -z "$T2G" || -z "$TECH" || -z "$OUT_DIR" || ${#POS[@]} -eq 0 ]]; then
  echo "ERROR: --index, --t2g, --tech, --out-dir, and FASTQ paths are required" >&2
  echo "Run with -h for usage." >&2
  exit 1
fi
if [[ ! -f "$INDEX" ]]; then echo "ERROR: $INDEX not found" >&2; exit 1; fi
if [[ ! -f "$T2G"   ]]; then echo "ERROR: $T2G not found"   >&2; exit 1; fi

if ! command -v kb >/dev/null 2>&1; then
  echo "ERROR: 'kb' not found. Install: pip install kb_python" >&2
  exit 1
fi

mkdir -p "$OUT_DIR"

KB_ARGS=(
  -i "$INDEX"
  -g "$T2G"
  -x "$TECH"
  -o "$OUT_DIR"
  -t "$THREADS"
  -m "$MEMORY"
  --h5ad
)

case "$WORKFLOW" in
  standard) ;;
  nac|lamanno) KB_ARGS+=(--workflow "$WORKFLOW") ;;
  *)
    echo "ERROR: --workflow must be standard | nac | lamanno" >&2
    exit 1
    ;;
esac

if [[ -n "$FILTER" ]]; then
  KB_ARGS+=($FILTER)
fi

echo "Running: kb count ${KB_ARGS[*]} ${POS[*]}"
echo ""
kb count "${KB_ARGS[@]}" "${POS[@]}"

# Print summary
echo ""
echo "=== Run summary ==="
if [[ -f "$OUT_DIR/run_info.json" ]]; then
  python3 -c "
import json
with open('$OUT_DIR/run_info.json') as f: d = json.load(f)
print(f'  n_processed:     {d.get(\"n_processed\", 0):,}')
print(f'  n_pseudoaligned: {d.get(\"n_pseudoaligned\", 0):,}')
print(f'  p_pseudoaligned: {d.get(\"p_pseudoaligned\", 0):.2f}%')
" 2>/dev/null || cat "$OUT_DIR/run_info.json"
fi

echo ""
echo "Done. Outputs:"
echo "  $OUT_DIR/counts_unfiltered/cells_x_genes.mtx"
echo "  $OUT_DIR/counts_unfiltered/adata.h5ad  (scanpy-ready)"
echo ""
echo "Load into scanpy:"
echo "  import anndata; adata = anndata.read_h5ad('$OUT_DIR/counts_unfiltered/adata.h5ad')"
