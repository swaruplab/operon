#!/usr/bin/env bash
# init_project.sh — non-interactive STELLAR atlas scaffold.
#
# Usage:
#   bash init_project.sh PROJECT_NAME H5AD_PATH [CELL_TYPE_COL]
#
# Example:
#   bash init_project.sh my_atlas data/annotated.h5ad cell_type
#
# Creates ./PROJECT_NAME/ with a starter stellar.yaml pointing at H5AD_PATH,
# runs `stellar ingest` + `stellar doctor`, and prints the next-step command.

set -euo pipefail

PROJECT_NAME="${1:-}"
H5AD_PATH="${2:-}"
CELL_TYPE_COL="${3:-cell_type}"

if [[ -z "$PROJECT_NAME" || -z "$H5AD_PATH" ]]; then
  echo "Usage: bash init_project.sh PROJECT_NAME H5AD_PATH [CELL_TYPE_COL]" >&2
  echo "  PROJECT_NAME    short identifier, used in URL path (e.g. my_atlas)" >&2
  echo "  H5AD_PATH       path to your annotated .h5ad" >&2
  echo "  CELL_TYPE_COL   obs column with cell-type labels (default: cell_type)" >&2
  exit 1
fi

if [[ ! -f "$H5AD_PATH" ]]; then
  echo "ERROR: $H5AD_PATH does not exist" >&2
  exit 1
fi

if ! command -v stellar >/dev/null 2>&1; then
  echo "ERROR: 'stellar' CLI not found. Install with:" >&2
  echo "  pip install 'stellar-atlas[full]'" >&2
  exit 1
fi

mkdir -p "$PROJECT_NAME"/data/raw "$PROJECT_NAME"/data/external/de \
         "$PROJECT_NAME"/data/external/hdwgcna "$PROJECT_NAME"/data/external/cellchat \
         "$PROJECT_NAME"/data/external/milo

# Copy or symlink the .h5ad into the project's data/raw
H5AD_DEST="$PROJECT_NAME/data/raw/$(basename "$H5AD_PATH")"
if [[ ! -e "$H5AD_DEST" ]]; then
  ln -s "$(realpath "$H5AD_PATH")" "$H5AD_DEST"
  echo "Linked: $H5AD_DEST -> $(realpath "$H5AD_PATH")"
fi

# Write a starter stellar.yaml
cat > "$PROJECT_NAME/stellar.yaml" <<YAML
project:
  name: ${PROJECT_NAME}
  display_name: "$(echo "$PROJECT_NAME" | sed 's/_/ /g' | awk '{for(i=1;i<=NF;i++) $i=toupper(substr($i,1,1)) substr($i,2)}1')"
  description: "Single-cell atlas"

input:
  matrix:
    type: h5ad
    path: data/raw/$(basename "$H5AD_PATH")
  obsm_umap: X_umap
  layer: X

  groupings:
    - ${CELL_TYPE_COL}

modules:
  de:
    enabled: false
    source_dir: data/external/de

  hdwgcna:
    enabled: false
    source_dir: data/external/hdwgcna

  cellchat:
    enabled: false
    source_dir: data/external/cellchat

  milo:
    enabled: false
    source_dir: data/external/milo

  enrichment:
    enabled: true

  copilot:
    enabled: false
    api_key_env: ANTHROPIC_API_KEY
    pubmed_email_env: NCBI_EMAIL
YAML

echo "Wrote $PROJECT_NAME/stellar.yaml"

# Ingest
echo "Running stellar ingest …"
(cd "$PROJECT_NAME" && stellar ingest)

# Doctor
echo "Running stellar doctor …"
(cd "$PROJECT_NAME" && stellar doctor)

echo ""
echo "Done. To serve locally:"
echo "  cd $PROJECT_NAME && stellar serve"
echo ""
echo "Then open http://localhost:18901/${PROJECT_NAME}/"
echo ""
echo "To enable additional modules, edit ${PROJECT_NAME}/stellar.yaml and re-run:"
echo "  stellar ingest && stellar doctor"
