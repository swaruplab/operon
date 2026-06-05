---
name: stellar-atlas
description: Build deployable interactive web atlases from single-cell RNA-seq data using STELLAR. Turns a .h5ad (or Seurat .rds) into a UMAP + gene-expression + DE + hdWGCNA + CellChat + Milo + enrichment + AI-chat browser SPA. Covers the four-step CLI (init → ingest → build-frontend → serve/deploy), all six built-in modules, stellar.yaml configuration, and the parquet schemas each module ingests.
license: MIT
metadata:
---

# STELLAR-Atlas: Deployable Interactive Single-Cell Browsers

## Overview

[STELLAR](https://github.com/swaruplab/stellar-atlas) packages a single-cell dataset into a self-contained web app: UMAP scatter, gene expression overlay, per-cell-type violins, plus optional modules for differential expression, hdWGCNA co-expression modules, CellChat communication, Milo neighbourhood differential abundance, EnrichR pass-through, and a Claude-powered chat (with optional PubMed lookup). The reference deployment is [swaruplab.bio.uci.edu/panad_atlas/](https://swaruplab.bio.uci.edu/panad_atlas/) (~3M cells).

Tech stack under the hood:
- **LanceDB** for the gene-major expression matrix (fast per-gene reads at scale)
- **DuckDB** for cell metadata + derived per-module tables
- **FastAPI** + **uvicorn** for the backend
- **Pre-built React SPA** mounted at `/<project_name>/`, talks to `/api/`

The `stellar.yaml` config file declares which modules are enabled, where their input data lives, the project branding, and (for the Copilot module) API keys.

## When to Use This Skill

- You have a finalized scRNA-seq / snRNA-seq dataset (annotated, with UMAP) and want a shareable browser instead of sending colleagues 50 GB `.h5ad` files.
- You want collaborators to explore expression, DE results, and module / pathway analyses without re-running them.
- You have downstream analyses (hdWGCNA, CellChat, Milo) and want them surfaced in one place rather than a folder of static PDFs.
- You want to ship an AI chat over your atlas (Copilot module, requires `ANTHROPIC_API_KEY`).

**Not for**: real-time interactive analysis (it's a viewer, not a re-clusterer); raw data exploration before QC; datasets that aren't yet annotated.

## Prerequisites

- Python 3.11+ (3.12 recommended)
- A finalized `.h5ad` with: UMAP in `obsm['X_umap']`, cell-type labels in `obs`, log-normalized counts in `X` or a layer
- For Seurat input: R installed locally with `SeuratDisk` (auto-converted on first ingest)
- For optional modules: precomputed inputs as parquet (DE results, hdWGCNA modules, CellChat exports, Milo results — see [references/modules.md](references/modules.md))

```bash
# Install
pip install 'stellar-atlas[full]'   # all modules
# OR pick what you need:
pip install 'stellar-atlas[de,hdwgcna,copilot]'
```

Module extras:

| Extra | What it adds |
|---|---|
| `[de]` | Differential expression viewer (volcano + sortable table) |
| `[hdwgcna]` | Co-expression modules + hub-gene radial network + optional DME |
| `[cellchat]` | Pathway heatmap + L-R table + group delta |
| `[milo]` | Beeswarm-on-UMAP of neighbourhood DA + table |
| `[enrichment]` | Live EnrichR pass-through |
| `[copilot]` | Claude chat with auto-discovered tools + optional PubMed |
| `[full]` | All modules + dev deps |

## Quick Start — 5 Commands

```bash
# 1. Scaffold a new atlas project
stellar init my_atlas

# 2. Edit stellar.yaml — point at your .h5ad, enable modules, add branding
# (See "Configuration" below.)

# 3. Build the data stores (LanceDB + DuckDB + parquet)
stellar ingest

# 4. Verify everything is wired correctly
stellar doctor
# Expected: "stellar doctor: 0 issues — your project is healthy."

# 5. Serve locally
stellar serve
# → http://127.0.0.1:18901/my_atlas/
```

For a working end-to-end on PBMC 3K (~3 min first run):

```bash
pip install 'stellar-atlas[dev]' scanpy
python examples/pbmc_3k/bootstrap.py                                # writes data/raw/pbmc_3k.h5ad
stellar ingest --config examples/pbmc_3k/stellar.yaml
stellar doctor --config examples/pbmc_3k/stellar.yaml
stellar serve  --config examples/pbmc_3k/stellar.yaml
```

## Configuration — `stellar.yaml`

`stellar init` writes a starter `stellar.yaml`. The structure (abbreviated):

```yaml
project:
  name: my_atlas             # used in the URL path
  display_name: "My Atlas"   # shown in the SPA header
  description: "Brain atlas — 1.2M cells across 18 donors"

input:
  matrix:
    type: h5ad               # h5ad | rds
    path: data/raw/dataset.h5ad
  obsm_umap: X_umap          # which obsm key holds the UMAP
  layer: X                   # which layer to serve as expression

  groupings:                 # cell-type / cluster columns shown in the SPA
    - cell_type
    - cluster

  donors_column: donor       # for per-donor stratification

modules:
  de:
    enabled: true
    source_dir: data/external/de        # comparisons.parquet + results.parquet

  hdwgcna:
    enabled: true
    source_dir: data/external/hdwgcna   # modules.parquet, hubs.parquet, kme.parquet

  cellchat:
    enabled: true
    source_dir: data/external/cellchat  # 4 parquets, extracted from cellchat .rds

  milo:
    enabled: false

  enrichment:
    enabled: true                       # No source — calls EnrichR live

  copilot:
    enabled: true
    api_key_env: ANTHROPIC_API_KEY      # required at runtime
    pubmed_email_env: NCBI_EMAIL        # optional, for PubMed lookup
```

The full schema is auto-generated from the Pydantic models in the package; run `stellar init --schema` to print it.

## Module-by-Module — What Goes In, What Shows Up

### `core` — always on

- **Input**: just the `.h5ad` + UMAP + a grouping column.
- **Shows**: UMAP (color-by cell type / donor / continuous obs), gene search with per-cell expression overlay, per-group violin plots.

### `de` — differential expression viewer

- **Input**: two parquet files in `source_dir`:
  - `comparisons.parquet` — one row per comparison (`comparison_id`, `group_a`, `group_b`, `description`)
  - `results.parquet` — `comparison_id`, `gene`, `logFC`, `p_val`, `p_val_adj`, `pct_a`, `pct_b`
- **Shows**: comparison dropdown, volcano plot, sortable/filterable gene table.

### `hdwgcna` — co-expression modules

- **Input** (three parquets):
  - `modules.parquet` — `gene`, `module`, `color`, `kME`
  - `hubs.parquet` — `module`, `gene`, `kME` (top hubs)
  - `kme.parquet` — `gene`, `module`, `kME` (full kME matrix)
  - Optional `dme.parquet` — `module`, `group1`, `group2`, `avg_log2FC`, `p_val_adj`
- **Shows**: module list with sizes, radial hub-gene network, kME ranked tables, DME tab.

### `cellchat` — communication

- **Input** (four parquets, extracted from your CellChat `.rds`):
  - `pathway_net.parquet` — pathway × source × target probability
  - `lr_pairs.parquet` — L-R level table with pathway annotation
  - `centrality.parquet` — per-cell-type-per-pathway centrality scores
  - `group_delta.parquet` — (optional) `pathway`, `source`, `target`, `weight_a`, `weight_b`, `delta`
- **Shows**: source × target pathway heatmap, L-R drill-down table, per-pathway sender/receiver chord, group-delta chord for two-condition comparison.

### `milo` — neighbourhood differential abundance

- **Input** (three parquets from milopy / miloR):
  - `neighborhoods.parquet` — `cell_id`, `nhood_id` (one row per cell, may belong to multiple)
  - `nhood_meta.parquet` — `nhood_id`, `index_cell`, `logFC`, `SpatialFDR`, `Nhood_size`
  - `embeddings.parquet` — `nhood_id`, `UMAP_1`, `UMAP_2`
- **Shows**: UMAP overlay with neighbourhood circles colored by logFC + Spatial FDR, sortable table.

### `enrichment` — live EnrichR

- **Input**: none. The SPA sends gene lists to EnrichR's REST API.
- **Shows**: paste-a-gene-list textbox, library selector (GO BP / KEGG / etc.), bar plots of top terms.

### `copilot` — Claude chat

- **Input**: `ANTHROPIC_API_KEY` in environment.
- **Shows**: chat panel with auto-discovered tool calls into the data stores (UMAP query, gene lookup, DE table query, hdWGCNA module query, etc.). Optional PubMed lookup if `NCBI_EMAIL` is set.

Full per-module spec including exact column types and validation rules: [references/modules.md](references/modules.md).

## Ingest, Doctor, Serve, Deploy

```bash
stellar ingest [--config stellar.yaml] [--matrix MATRIX_NAME]
```
- Reads `stellar.yaml`
- Converts Seurat `.rds` → `.h5ad` if needed (via SeuratDisk in R)
- Writes `data/lance/<matrix>.lance/` (gene-major), `data/lance/<matrix>_cells.lance/` (cell catalog), `data/parquet/*.parquet`, `data/atlas.duckdb`
- Idempotent: re-running drops and recreates the stores (cheap if input hasn't changed)

```bash
stellar doctor [--config stellar.yaml]
```
- Validates the YAML against the Pydantic schema
- Checks each enabled module's required parquet files exist with expected columns
- Returns exit code 0 on success, 1 if there are issues to fix

```bash
stellar serve [--config stellar.yaml] [--port 18901]
```
- Starts uvicorn on `127.0.0.1:18901` by default
- SPA mounted at `/<project_name>/`, API at `/api/`
- Auto-reloads on YAML changes

```bash
stellar build-frontend [--config stellar.yaml]
```
- Bakes a branded React SPA bundle (per-project name, color, logo)
- Output: `frontend/dist/` ready to copy anywhere
- Only needed if you want to host the static bundle behind a reverse proxy independently

```bash
stellar deploy --target /var/www/html/my_atlas [--config stellar.yaml]
```
- Copies the data + SPA bundle to the target directory
- Production usage typically: nginx in front, gunicorn/uvicorn behind, atlas mounted as a sub-path

See [references/deploy.md](references/deploy.md) for production deployment patterns (nginx, systemd, Docker).

## Converting Existing Analysis Outputs to STELLAR Parquets

The trickiest part of adopting STELLAR is exporting your existing analyses into the parquet shapes each module expects. Each script is small but specific to the source tool.

```bash
# Convert Seurat DE (FindMarkers output) → comparisons + results parquets
Rscript scripts/seurat_de_to_parquet.R --rds annotated.rds --out data/external/de/

# Convert hdWGCNA Seurat → 3 parquets
Rscript scripts/hdwgcna_to_parquet.R --rds seurat_hdwgcna.rds --out data/external/hdwgcna/

# Convert CellChat .rds → 4 parquets
Rscript scripts/cellchat_to_parquet.R --rds cellchat.rds --out data/external/cellchat/

# Convert milopy / miloR results → 3 parquets
Rscript scripts/milo_to_parquet.R --rds milo_results.rds --out data/external/milo/
```

These ship with the STELLAR repo under `scripts/converters/`. The hdwgcna and cellchat exporters consume outputs from the corresponding Operon protocols (`protocols/hdwgcna`, `protocols/cellchat`).

## Best Practices

- **Start with `core` + `de` only.** Get the SPA running with the minimum, then layer modules in. Each module is independent.
- **Validate before deploying.** `stellar doctor` catches column-name typos and missing files before they become 500 errors in production.
- **The h5ad is read-once.** STELLAR copies what it needs into LanceDB + DuckDB. The original `.h5ad` doesn't need to live on the server.
- **Don't precompute everything.** EnrichR is live; the Copilot is live. Save your time for the actually-static results (DE, hdWGCNA, CellChat).
- **For multi-million-cell atlases**, subsample for the UMAP scatter (the SPA can handle ~200K points smoothly; LanceDB still gives full per-gene reads at all scales).
- **Bake branding into the build, not into the config.** Logos and custom colors go through `stellar build-frontend`; the config carries only display_name + description.

## End-to-End Template

`assets/stellar_template.sh` is a shell script that sets up a new atlas project: downloads/locates the input `.h5ad`, runs the optional Seurat→h5ad conversion, writes a starter `stellar.yaml`, runs ingest + doctor + serve.

## Convenience Scripts

- `scripts/init_project.sh` — non-interactive project scaffolding with sensible defaults
- `scripts/converters/` (see "Converting Existing Analysis Outputs" above) — exporters from Seurat / hdWGCNA / CellChat / Milo to the parquet shapes STELLAR ingests

## References

- [STELLAR-atlas GitHub](https://github.com/swaruplab/stellar-atlas) — swaruplab
- [Landing](https://swaruplab.github.io/stellar-atlas/), [Modules](https://swaruplab.github.io/stellar-atlas/modules/), [Quickstart](https://swaruplab.github.io/stellar-atlas/quickstart/)
- Reference deployment: [swaruplab.bio.uci.edu/panad_atlas/](https://swaruplab.bio.uci.edu/panad_atlas/) (~3M cells)
- Version 1.0.0 / Beta (first public release)
