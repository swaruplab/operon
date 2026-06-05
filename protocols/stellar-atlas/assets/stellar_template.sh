#!/usr/bin/env bash
# stellar_template.sh — end-to-end STELLAR atlas builder.
#
# Edits the CONFIGURATION block, then runs the full pipeline:
#   1. (Optional) Convert Seurat .rds → .h5ad
#   2. Scaffold project directory + stellar.yaml
#   3. (Optional) Export DE / hdWGCNA / CellChat / Milo results to parquet
#   4. stellar ingest + doctor
#   5. Serve locally

set -euo pipefail

# ============================================================================
# CONFIGURATION
# ============================================================================

PROJECT_NAME="my_atlas"
PROJECT_DESCRIPTION="Single-cell atlas — disease vs control"
DISPLAY_NAME="My Atlas"

# Input — either a .h5ad (preferred) or a Seurat .rds (auto-converted)
INPUT_MATRIX="data/annotated.h5ad"           # .h5ad or .rds
CELL_TYPE_COLS=("cell_type" "cluster")        # obs columns shown as groupings
DONORS_COL="donor"                            # set to "" if not applicable
UMAP_KEY="X_umap"                             # obs/obsm key for UMAP

# Modules — set to true for each you want enabled
ENABLE_DE=false
ENABLE_HDWGCNA=false
ENABLE_CELLCHAT=false
ENABLE_MILO=false
ENABLE_ENRICHMENT=true
ENABLE_COPILOT=false

# Optional: paths to source files for each module (used by the exporters below)
SEURAT_RDS_FOR_DE="annotated.rds"             # for the DE exporter
HDWGCNA_RDS="seurat_hdwgcna.rds"              # output from the hdwgcna protocol
CELLCHAT_RDS="cellchat.rds"                   # output from the cellchat protocol
MILO_RDS="milo_results.rds"

# Local serve
SERVE_PORT=18901

# ============================================================================
# SETUP
# ============================================================================

mkdir -p "$PROJECT_NAME"/data/raw \
          "$PROJECT_NAME"/data/external/{de,hdwgcna,cellchat,milo}
cd "$PROJECT_NAME"

# ── 1. Convert Seurat → h5ad if needed ──────────────────────────────────────
H5AD_PATH="$INPUT_MATRIX"
if [[ "$INPUT_MATRIX" == *.rds ]]; then
  echo "Converting Seurat .rds → .h5ad …"
  Rscript - <<RSCRIPT
suppressPackageStartupMessages({
  library(Seurat); library(SeuratDisk)
})
obj <- readRDS("$INPUT_MATRIX")
SaveH5Seurat(obj, filename = "data/raw/converted.h5Seurat", overwrite = TRUE)
Convert("data/raw/converted.h5Seurat", dest = "h5ad", overwrite = TRUE)
RSCRIPT
  H5AD_PATH="data/raw/converted.h5ad"
else
  # Symlink the original .h5ad into the project dir
  if [[ ! -e "data/raw/$(basename "$INPUT_MATRIX")" ]]; then
    ln -s "$(realpath "../$INPUT_MATRIX")" "data/raw/$(basename "$INPUT_MATRIX")"
  fi
  H5AD_PATH="data/raw/$(basename "$INPUT_MATRIX")"
fi

# ── 2. Write stellar.yaml ───────────────────────────────────────────────────
GROUPING_LINES=""
for ct in "${CELL_TYPE_COLS[@]}"; do
  GROUPING_LINES+="    - $ct"$'\n'
done

cat > stellar.yaml <<YAML
project:
  name: ${PROJECT_NAME}
  display_name: "${DISPLAY_NAME}"
  description: "${PROJECT_DESCRIPTION}"

input:
  matrix:
    type: h5ad
    path: ${H5AD_PATH}
  obsm_umap: ${UMAP_KEY}
  layer: X
  groupings:
${GROUPING_LINES}$([[ -n "$DONORS_COL" ]] && echo "  donors_column: $DONORS_COL")

modules:
  de:
    enabled: ${ENABLE_DE}
    source_dir: data/external/de
  hdwgcna:
    enabled: ${ENABLE_HDWGCNA}
    source_dir: data/external/hdwgcna
  cellchat:
    enabled: ${ENABLE_CELLCHAT}
    source_dir: data/external/cellchat
  milo:
    enabled: ${ENABLE_MILO}
    source_dir: data/external/milo
  enrichment:
    enabled: ${ENABLE_ENRICHMENT}
  copilot:
    enabled: ${ENABLE_COPILOT}
    api_key_env: ANTHROPIC_API_KEY
    pubmed_email_env: NCBI_EMAIL
YAML

echo "Wrote stellar.yaml"

# ── 3. Export module inputs (only when needed) ──────────────────────────────
# These exporters are pointers — adapt to your actual analysis outputs.

if $ENABLE_HDWGCNA && [[ -f "../$HDWGCNA_RDS" ]]; then
  echo "Exporting hdWGCNA → parquet …"
  Rscript - <<RSCRIPT
suppressPackageStartupMessages({
  library(hdWGCNA); library(arrow); library(tidyverse)
})
seurat_obj <- readRDS("../$HDWGCNA_RDS")

modules <- GetModules(seurat_obj) %>% filter(module != 'grey')
# Pull each row's kME of its own module
modules\$kME <- mapply(function(g, m) {
  modules\$kME[[paste0('kME_', m)]][modules\$gene_name == g]
}, modules\$gene_name, modules\$module)
write_parquet(modules %>% transmute(gene = gene_name, module, color, kME),
              "data/external/hdwgcna/modules.parquet")

hubs <- GetHubGenes(seurat_obj, n_hubs = 25) %>%
  group_by(module) %>% mutate(rank = row_number()) %>% ungroup() %>%
  transmute(module, gene = gene_name, kME, rank)
write_parquet(hubs, "data/external/hdwgcna/hubs.parquet")

kme_long <- GetModules(seurat_obj) %>%
  pivot_longer(starts_with('kME_'), names_to = 'module',
                values_to = 'kME') %>%
  mutate(module = sub('^kME_', '', module)) %>%
  transmute(gene = gene_name, module, kME)
write_parquet(kme_long, "data/external/hdwgcna/kme.parquet")
RSCRIPT
fi

if $ENABLE_CELLCHAT && [[ -f "../$CELLCHAT_RDS" ]]; then
  echo "Exporting CellChat → parquet …"
  Rscript - <<RSCRIPT
suppressPackageStartupMessages({
  library(CellChat); library(arrow); library(reshape2); library(tidyverse)
})
cellchat <- readRDS("../$CELLCHAT_RDS")

pn <- melt(cellchat@netP\$prob,
            varnames = c('source', 'target', 'pathway'),
            value.name = 'prob') %>% filter(prob > 0)
write_parquet(pn, "data/external/cellchat/pathway_net.parquet")

lr <- subsetCommunication(cellchat) %>%
  transmute(interaction_name, ligand, receptor,
            pathway = pathway_name, source, target, prob, pval)
write_parquet(lr, "data/external/cellchat/lr_pairs.parquet")
RSCRIPT
fi

# Add similar Rscript blocks for DE and Milo when ready.

# ── 4. Ingest + doctor ─────────────────────────────────────────────────────
echo "Running stellar ingest …"
stellar ingest

echo "Running stellar doctor …"
stellar doctor

# ── 5. Serve ────────────────────────────────────────────────────────────────
echo ""
echo "All set. To launch the atlas locally:"
echo "  cd $(pwd) && stellar serve --port $SERVE_PORT"
echo ""
echo "Then open: http://localhost:${SERVE_PORT}/${PROJECT_NAME}/"
