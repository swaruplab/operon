# STELLAR Modules — Parquet Schemas

Each STELLAR module ingests precomputed results as parquet files. This reference gives the exact schema each module expects: column names, types, validation rules. Use it when writing exporters from your analysis tool (Seurat / scanpy / R / etc.) to STELLAR.

## `core` — always required

Reads from the input `.h5ad` / Seurat `.rds` directly. No external parquets needed.

**Required structure in the input matrix:**
- `X` or a named `layer` with log-normalized expression
- `obsm['X_umap']` with shape `(n_obs, 2)` (configurable key via `obsm_umap` in `stellar.yaml`)
- `obs` with at least one cell-type column (configurable list via `groupings`)
- Optional: `obs[donors_column]` for per-donor stratification

The ingest pipeline writes:
- `data/lance/<matrix>.lance/` — gene-major LanceDB (rows = genes, columns = cells)
- `data/lance/<matrix>_cells.lance/` — `row_idx`, `cell_id` lookup
- `data/parquet/cells.parquet` — all `obs` columns
- `data/parquet/genes.parquet` — gene metadata (names, IDs)
- `data/parquet/donors.parquet` — aggregated per-donor counts (if `donors_column` set)
- `data/atlas.duckdb` — embedded analytical store

## `de` — differential expression

Two parquet files in `source_dir`.

### `comparisons.parquet`

| Column | Type | Required | Notes |
|---|---|---|---|
| `comparison_id` | string | ✓ | Stable identifier, used in URL |
| `name` | string | ✓ | Display name (e.g. "AD vs Ctrl in Astrocytes") |
| `group_a` | string | ✓ | Label of group A |
| `group_b` | string | ✓ | Label of group B |
| `cell_type` | string | | Optional — for cell-type-stratified comparisons |
| `description` | string | | Optional, longer text shown in the comparison panel |

### `results.parquet`

| Column | Type | Required | Notes |
|---|---|---|---|
| `comparison_id` | string | ✓ | Foreign key into `comparisons.parquet` |
| `gene` | string | ✓ | Gene symbol |
| `logFC` | float | ✓ | log2 fold-change (A relative to B) |
| `p_val` | float | ✓ | Raw p-value |
| `p_val_adj` | float | ✓ | Adjusted p-value (BH / Bonferroni — your call) |
| `pct_a` | float | | Fraction expressing in group A |
| `pct_b` | float | | Fraction expressing in group B |

Typical exporter from a Seurat `FindMarkers` loop:

```r
# scripts/seurat_de_to_parquet.R
library(arrow)
all_de <- bind_rows(lapply(seq_along(comparisons), function(i) {
  cmp <- comparisons[[i]]
  de  <- FindMarkers(seurat_obj, ident.1 = cmp$group_a, ident.2 = cmp$group_b)
  data.frame(
    comparison_id = cmp$id,
    gene          = rownames(de),
    logFC         = de$avg_log2FC,
    p_val         = de$p_val,
    p_val_adj     = de$p_val_adj,
    pct_a         = de$pct.1,
    pct_b         = de$pct.2
  )
}))
write_parquet(all_de, "data/external/de/results.parquet")
```

## `hdwgcna` — co-expression modules

Three required parquet files + one optional, in `source_dir`.

### `modules.parquet`

| Column | Type | Required |
|---|---|---|
| `gene` | string | ✓ |
| `module` | string | ✓ |
| `color` | string | ✓ (WGCNA color name or hex) |
| `kME` | float | ✓ (kME within own module) |

### `hubs.parquet`

Top hub genes per module (typically top 25).

| Column | Type | Required |
|---|---|---|
| `module` | string | ✓ |
| `gene` | string | ✓ |
| `kME` | float | ✓ |
| `rank` | int | | Optional, 1 = top hub |

### `kme.parquet`

Full kME matrix in long form (one row per gene × module pair).

| Column | Type | Required |
|---|---|---|
| `gene` | string | ✓ |
| `module` | string | ✓ |
| `kME` | float | ✓ |

### `dme.parquet` (optional)

Differential MEs between groups.

| Column | Type | Required |
|---|---|---|
| `module` | string | ✓ |
| `group_a` | string | ✓ |
| `group_b` | string | ✓ |
| `avg_log2FC` | float | ✓ |
| `p_val_adj` | float | ✓ |

Exporter from a STELLAR-compatible hdWGCNA analysis (built using the `hdwgcna` Operon protocol):

```r
# scripts/hdwgcna_to_parquet.R — abbreviated
library(hdWGCNA); library(arrow)
seurat_obj <- readRDS('seurat_hdwgcna.rds')

modules <- GetModules(seurat_obj) %>% filter(module != 'grey') %>%
  transmute(gene = gene_name, module, color, kME = .data[[paste0('kME_', module)]])
write_parquet(modules, 'data/external/hdwgcna/modules.parquet')

hubs <- GetHubGenes(seurat_obj, n_hubs = 25) %>%
  group_by(module) %>% mutate(rank = row_number()) %>% ungroup() %>%
  rename(gene = gene_name)
write_parquet(hubs, 'data/external/hdwgcna/hubs.parquet')

# kme.parquet — pivot the wide kME matrix to long
modules_full <- GetModules(seurat_obj)
kme_long <- modules_full %>%
  pivot_longer(starts_with('kME_'), names_to = 'module', values_to = 'kME') %>%
  mutate(module = sub('^kME_', '', module)) %>%
  transmute(gene = gene_name, module, kME)
write_parquet(kme_long, 'data/external/hdwgcna/kme.parquet')
```

## `cellchat` — communication

Four parquets in `source_dir`, all extracted from a CellChat `.rds`.

### `pathway_net.parquet`

Pathway-level communication probability (the `@netP$prob` tensor flattened).

| Column | Type | Required |
|---|---|---|
| `pathway` | string | ✓ |
| `source` | string | ✓ |
| `target` | string | ✓ |
| `prob` | float | ✓ |

### `lr_pairs.parquet`

L-R-level table.

| Column | Type | Required |
|---|---|---|
| `interaction_name` | string | ✓ (e.g. `CXCL12_CXCR4`) |
| `ligand` | string | ✓ |
| `receptor` | string | ✓ |
| `pathway` | string | ✓ |
| `source` | string | ✓ |
| `target` | string | ✓ |
| `prob` | float | ✓ |
| `pval` | float | ✓ |

### `centrality.parquet`

| Column | Type | Required |
|---|---|---|
| `cell_type` | string | ✓ |
| `pathway` | string | ✓ |
| `outdeg` | float | |
| `indeg` | float | |
| `flow_betweenness` | float | |
| `information_centrality` | float | |

### `group_delta.parquet` (for two-condition comparison)

| Column | Type | Required |
|---|---|---|
| `pathway` | string | ✓ |
| `source` | string | ✓ |
| `target` | string | ✓ |
| `weight_a` | float | ✓ |
| `weight_b` | float | ✓ |
| `delta` | float | ✓ (weight_b - weight_a) |

Exporter sketch (after building a CellChat object via the `cellchat` Operon protocol):

```r
# scripts/cellchat_to_parquet.R — abbreviated
library(CellChat); library(arrow); library(reshape2)
cellchat <- readRDS('cellchat.rds')

# pathway_net.parquet
pn <- melt(cellchat@netP$prob, varnames = c('source', 'target', 'pathway'),
            value.name = 'prob') %>% filter(prob > 0)
write_parquet(pn, 'data/external/cellchat/pathway_net.parquet')

# lr_pairs.parquet
lr <- subsetCommunication(cellchat) %>%
  transmute(interaction_name, ligand, receptor, pathway = pathway_name,
            source, target, prob, pval)
write_parquet(lr, 'data/external/cellchat/lr_pairs.parquet')

# centrality.parquet — flatten cellchat@netP$centr
centr_long <- bind_rows(lapply(names(cellchat@netP$centr), function(pw) {
  m <- cellchat@netP$centr[[pw]]
  data.frame(cell_type = rownames(m), pathway = pw, as.data.frame(m))
}))
write_parquet(centr_long, 'data/external/cellchat/centrality.parquet')
```

## `milo` — neighbourhood differential abundance

Three parquets in `source_dir`, from a milopy or miloR run.

### `neighborhoods.parquet`

| Column | Type | Required |
|---|---|---|
| `cell_id` | string | ✓ |
| `nhood_id` | int | ✓ |

(One cell can belong to multiple neighbourhoods — multiple rows per `cell_id`.)

### `nhood_meta.parquet`

| Column | Type | Required |
|---|---|---|
| `nhood_id` | int | ✓ |
| `index_cell` | string | ✓ (cell at the centre of the neighbourhood) |
| `logFC` | float | ✓ |
| `SpatialFDR` | float | ✓ |
| `Nhood_size` | int | ✓ |
| `PValue` | float | |
| `FDR` | float | |

### `embeddings.parquet`

| Column | Type | Required |
|---|---|---|
| `nhood_id` | int | ✓ |
| `UMAP_1` | float | ✓ |
| `UMAP_2` | float | ✓ |

(These are usually the index-cell UMAP coords; `milopy.utils.build_nhood_graph` produces them in scanpy.)

## `enrichment` — no parquet, live API

No precomputed input. The SPA sends gene lists to EnrichR's REST API and displays the result.

## `copilot` — no parquet, runtime API

No precomputed input. Needs environment variables at serve time:

```bash
export ANTHROPIC_API_KEY="sk-..."
export NCBI_EMAIL="you@uci.edu"   # optional, for PubMed
stellar serve
```

The Copilot auto-discovers tools by introspecting the enabled modules — if `de` is enabled, the chat gets a DE query tool; if `hdwgcna` is enabled, it gets module / hub query tools.

## Validation — what `stellar doctor` checks

For each enabled module, `stellar doctor` verifies:

1. `source_dir` exists
2. Required parquet files are present
3. Required columns exist with compatible types
4. Foreign keys resolve (e.g. `results.comparison_id` ↔ `comparisons.comparison_id`)
5. Cell IDs in module parquets are subsets of cell IDs in the input matrix

A passing `stellar doctor` is a hard prereq for `stellar serve` / `stellar deploy`. If a module's input is malformed, disable it in `stellar.yaml` rather than ship a broken atlas.
