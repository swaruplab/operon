#!/usr/bin/env bash
# kb_ref.sh — build a kallisto index from genome FASTA + GTF.
#
# Usage:
#   bash kb_ref.sh --genome GRCh38.fa --gtf annotation.gtf --out-dir reference/
#   bash kb_ref.sh --genome GRCh38.fa --gtf annotation.gtf --out-dir reference/ --workflow nac
#   bash kb_ref.sh --prebuilt human --out-dir reference/
#
# Output (standard):
#   reference/index.idx
#   reference/t2g.txt
#   reference/cdna.fasta
# Output (nac):
#   + reference/cdna.txt, reference/nascent.txt, reference/nascent.fasta

set -euo pipefail

GENOME=""
GTF=""
OUT_DIR=""
WORKFLOW="standard"
PREBUILT=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --genome)    GENOME="$2";    shift 2 ;;
    --gtf)       GTF="$2";       shift 2 ;;
    --out-dir)   OUT_DIR="$2";   shift 2 ;;
    --workflow)  WORKFLOW="$2";  shift 2 ;;
    --prebuilt)  PREBUILT="$2";  shift 2 ;;
    -h|--help)
      sed -n '2,12p' "$0"
      exit 0
      ;;
    *)
      echo "Unknown arg: $1" >&2
      exit 1
      ;;
  esac
done

if [[ -z "$OUT_DIR" ]]; then
  echo "ERROR: --out-dir is required" >&2
  exit 1
fi
mkdir -p "$OUT_DIR"

if ! command -v kb >/dev/null 2>&1; then
  echo "ERROR: 'kb' not found. Install with: pip install kb_python" >&2
  exit 1
fi

# ── Prebuilt path ──────────────────────────────────────────────────────────
if [[ -n "$PREBUILT" ]]; then
  echo "Downloading prebuilt index for '$PREBUILT' …"
  kb ref -d "$PREBUILT" \
         -i "$OUT_DIR/index.idx" \
         -g "$OUT_DIR/t2g.txt"
  echo "Done. Index: $OUT_DIR/index.idx"
  exit 0
fi

# ── Custom build ───────────────────────────────────────────────────────────
if [[ -z "$GENOME" || -z "$GTF" ]]; then
  echo "ERROR: --genome and --gtf are required (or use --prebuilt)" >&2
  exit 1
fi
if [[ ! -f "$GENOME" ]]; then echo "ERROR: $GENOME not found" >&2; exit 1; fi
if [[ ! -f "$GTF"    ]]; then echo "ERROR: $GTF not found"    >&2; exit 1; fi

case "$WORKFLOW" in
  standard)
    echo "Building STANDARD index (mature transcripts only) …"
    kb ref \
        -i  "$OUT_DIR/index.idx" \
        -g  "$OUT_DIR/t2g.txt" \
        -f1 "$OUT_DIR/cdna.fasta" \
        "$GENOME" "$GTF"
    ;;
  nac)
    echo "Building NAC index (mature + nascent for snRNA-seq / velocity) …"
    kb ref \
        --workflow nac \
        -i  "$OUT_DIR/index.idx" \
        -g  "$OUT_DIR/t2g.txt" \
        -c1 "$OUT_DIR/cdna.txt" \
        -c2 "$OUT_DIR/nascent.txt" \
        -f1 "$OUT_DIR/cdna.fasta" \
        -f2 "$OUT_DIR/nascent.fasta" \
        "$GENOME" "$GTF"
    ;;
  lamanno)
    echo "Building LAMANNO index (legacy velocity layout) …"
    kb ref \
        --workflow lamanno \
        -i  "$OUT_DIR/index.idx" \
        -g  "$OUT_DIR/t2g.txt" \
        -c1 "$OUT_DIR/cdna_t2c.txt" \
        -c2 "$OUT_DIR/intron_t2c.txt" \
        -f1 "$OUT_DIR/cdna.fasta" \
        -f2 "$OUT_DIR/intron.fasta" \
        "$GENOME" "$GTF"
    ;;
  *)
    echo "ERROR: --workflow must be standard | nac | lamanno" >&2
    exit 1
    ;;
esac

echo ""
echo "Done."
echo "  Index:     $OUT_DIR/index.idx"
echo "  t2g:       $OUT_DIR/t2g.txt"
ls -lh "$OUT_DIR/"*.idx "$OUT_DIR/"*.txt 2>/dev/null
