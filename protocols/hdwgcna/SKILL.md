---
name: hdwgcna
description: Co-expression network analysis for single-cell and spatial transcriptomics using hdWGCNA (R/Seurat). Covers the full pipeline — metacell construction, soft-power selection, network construction, module identification, module eigengenes (MEs/hMEs), hub gene scoring — plus three downstream analyses: differential module eigengenes between groups (FindDMEs / FindAllDMEs), module-trait correlation (ModuleTraitCorrelation), and enrichment via Enrichr / fgsea. Operates on Seurat objects.
license: GPL-3.0-or-later
metadata:
---

# hdWGCNA: High-Dimensional Co-Expression Network Analysis

## Overview

[hdWGCNA](https://smorabit.github.io/hdWGCNA/) is the modern adaptation of WGCNA for **high-dimensional** transcriptomics — single-cell RNA-seq, single-nucleus RNA-seq, and spatial transcriptomics. It solves WGCNA's two biggest problems with single-cell data:

1. **Sparsity** — single cells have many zeros, breaking WGCNA's correlation structure. hdWGCNA aggregates similar cells into "metacells" first.
2. **Batch effects** — module eigengenes (MEs) can be confounded by donor/sample. hdWGCNA computes batch-corrected harmonized MEs (hMEs) via Harmony.

The package operates on Seurat objects: every step adds data to `seurat_obj@misc[[wgcna_name]]`, so you can run multiple parallel analyses (e.g. one network per cell type) on the same Seurat object.

## When to Use This Skill

- Identifying co-expressed gene modules within a cell type (e.g. "neuronal modules in inhibitory neurons")
- Finding hub genes (genes most central to a module) for downstream wet-lab validation
- Testing module activity differences between conditions (case vs control, treated vs untreated)
- Correlating module eigengenes with continuous traits (age, disease severity, cell type proportion)
- Functional enrichment of modules (GO, KEGG, custom gene sets) via Enrichr or fgsea
- Module preservation testing across datasets (cross-condition / cross-species)

**Not for**: bulk RNA-seq (use vanilla WGCNA), trajectory analysis (use scVelo / Slingshot), differential expression at the gene level (use Seurat / DESeq2).

## Prerequisites

- R ≥ 4.2, Seurat ≥ 4.x, WGCNA, harmony, UCell, igraph
- A Seurat object with: cell type / cluster labels, sample/donor identifiers, PCA + Harmony reductions if you want hMEs
- Recommend ≥ 5,000 cells per group of interest after QC

```r
# Install (one-time)
install.packages("BiocManager")
BiocManager::install(c("WGCNA", "GeneOverlap", "UCell"))
devtools::install_github('smorabit/hdWGCNA', ref='dev')
```

## Quick Start — Full Pipeline

```r
library(Seurat)
library(hdWGCNA)
library(tidyverse)
library(cowplot)
library(patchwork)
library(WGCNA)
allowWGCNAThreads(nThreads = 8)

# Assumes you already have a normalized Seurat object with cell type labels.
seurat_obj <- readRDS('your_annotated_seurat.rds')

# ── 1. Set up hdWGCNA experiment ───────────────────────────────────────────
seurat_obj <- SetupForWGCNA(
  seurat_obj,
  gene_select = "fraction",     # keep genes expressed in ≥ fraction of cells
  fraction = 0.05,
  wgcna_name = "INH"             # name this analysis (you can run multiple)
)

# ── 2. Build metacells ─────────────────────────────────────────────────────
# k-NN aggregation within each (cell_type × Sample) bin → "metacells"
# reduces sparsity while keeping per-sample biology distinct.
seurat_obj <- MetacellsByGroups(
  seurat_obj,
  group.by = c("cell_type", "Sample"),
  reduction = 'harmony',         # use the integrated embedding
  k = 25,                        # cells per metacell
  max_shared = 10,               # max cells shared across metacells (prevents cliques)
  ident.group = 'cell_type'
)
seurat_obj <- NormalizeMetacells(seurat_obj)

# ── 3. Pick the cell type / group to model ─────────────────────────────────
seurat_obj <- SetDatExpr(
  seurat_obj,
  group_name = "INH",            # the cell type of interest
  group.by = 'cell_type',
  assay = 'RNA',
  layer = 'data'
)

# ── 4. Soft-power selection ────────────────────────────────────────────────
seurat_obj <- TestSoftPowers(seurat_obj, networkType = 'signed')
plot_list <- PlotSoftPowers(seurat_obj)
wrap_plots(plot_list, ncol = 2)
# Pick the lowest power giving SFT R² ≥ 0.8 (default auto-selects from GetPowerTable).

# ── 5. Construct network ───────────────────────────────────────────────────
seurat_obj <- ConstructNetwork(
  seurat_obj,
  tom_name = 'INH',              # the TOM file (gene×gene topological overlap) is written here
  overwrite_tom = TRUE
)
PlotDendrogram(seurat_obj, main = 'INH hdWGCNA Dendrogram')

# ── 6. Module eigengenes (with Harmony batch correction) ───────────────────
seurat_obj <- ScaleData(seurat_obj, features = VariableFeatures(seurat_obj))
seurat_obj <- ModuleEigengenes(
  seurat_obj,
  group.by.vars = "Sample"      # harmonize MEs across samples
)

# ── 7. Connectivity (kME) + hub gene scoring ───────────────────────────────
seurat_obj <- ModuleConnectivity(
  seurat_obj,
  group.by = 'cell_type',
  group_name = 'INH'
)
seurat_obj <- ResetModuleNames(seurat_obj, new_name = "INH-M")

# Top-10 hub genes per module
hub_df <- GetHubGenes(seurat_obj, n_hubs = 10)

# UCell signature score for the top-25 hubs per module — fast, per-cell
library(UCell)
seurat_obj <- ModuleExprScore(seurat_obj, n_genes = 25, method = 'UCell')

# Save the analysis to disk — the network construction step is the slow one
saveRDS(seurat_obj, 'seurat_obj_hdwgcna.rds')
```

The result object is the same Seurat object, with the entire hdWGCNA experiment stored in `seurat_obj@misc[["INH"]]`. Inspect the three key outputs:

```r
# Module assignments (one row per gene)
modules <- GetModules(seurat_obj) %>% subset(module != 'grey')
head(modules)
#   gene_name  module    color   kME_INH-M1   kME_INH-M2 ...
# 1 GRIN1      INH-M1    red     0.332        0.021
# 2 INTS11     INH-M2    blue    0.050        0.227

# Module eigengenes (cells × modules)
hMEs <- GetMEs(seurat_obj)             # harmonized
MEs  <- GetMEs(seurat_obj, harmonized = FALSE)   # raw

# Hub genes (top N per module by kME)
hub_df <- GetHubGenes(seurat_obj, n_hubs = 25)
```

Convenience runner: `Rscript scripts/build_network.R --rds annotated.rds --cell-type INH --out seurat_hdwgcna.rds`. See [references/network_construction.md](references/network_construction.md) for parameter tuning, metacell diagnostics, and gotchas.

---

## Downstream 1 — Differential Module Eigengenes (DMEs)

**The question:** does module-N activity differ between condition A and condition B (e.g. AD vs control, treated vs untreated)? This is the most common follow-up after network construction.

### Two-group test — `FindDMEs`

```r
# Pick the cells to compare. Often: same cell type, two conditions.
group1 <- seurat_obj@meta.data %>%
  filter(cell_type == 'INH', condition == 'AD')   %>% rownames()
group2 <- seurat_obj@meta.data %>%
  filter(cell_type == 'INH', condition == 'Ctrl') %>% rownames()

DMEs <- FindDMEs(
  seurat_obj,
  barcodes1 = group1,
  barcodes2 = group2,
  test.use  = 'wilcox',          # Mann-Whitney U
  pseudocount.use = 0.01,        # for log2FC stability
  wgcna_name = 'INH'
)
```

Returns a dataframe with one row per module: `p_val`, `avg_log2FC`, `pct.1`, `pct.2`, `p_val_adj`, `module`. The `pct.*` columns are the fraction of cells in each group whose ME is non-zero — useful when a module is "active" in one condition but silent in the other.

### Visualize

```r
PlotDMEsLollipop(seurat_obj, DMEs, group.by = 'cell_type', wgcna_name = 'INH')
PlotDMEsVolcano (seurat_obj, DMEs, plot_labels = TRUE,    wgcna_name = 'INH')
```

The lollipop is for a single comparison (great for figures). The volcano shows effect-size × significance at a glance — useful when you have many modules and want to spot the outliers.

### One-vs-all — `FindAllDMEs`

Across each level of a grouping variable, runs that level vs all other cells:

```r
DMEs_all <- FindAllDMEs(
  seurat_obj,
  group.by   = 'cell_type',
  wgcna_name = 'INH'
)
# adds a `group` column with the tested category
```

### Module signature scores instead of MEs

If your modules don't have clean eigengenes (e.g. small modules with noisy first PC), test on UCell-based hub-gene signatures instead:

```r
seurat_obj <- ModuleExprScore(seurat_obj, n_genes = 25, method = 'UCell')
DMEs_scores <- FindDMEs(
  seurat_obj,
  barcodes1 = group1, barcodes2 = group2,
  features  = 'ModuleScores',
  wgcna_name = 'INH'
)
```

Convenience: `Rscript scripts/downstream_analyses.R --rds seurat_hdwgcna.rds --task dme --group-by condition --group1 AD --group2 Ctrl --cell-type INH`

Source: [hdWGCNA Differential MEs tutorial](https://smorabit.github.io/hdWGCNA/articles/differential_MEs.html).

---

## Downstream 2 — Module–Trait Correlation

**The question:** which modules correlate with continuous or ordered phenotypes (age, Braak stage, disease severity, % tumor cells per spot)?

```r
cur_traits <- c('age', 'Braak_stage', 'sex_binary', 'cell_density')

seurat_obj <- ModuleTraitCorrelation(
  seurat_obj,
  traits   = cur_traits,
  group.by = 'cell_type',         # stratify the correlation by cell type
  wgcna_name = 'INH'
)

mt_cor <- GetModuleTraitCorrelation(seurat_obj)
# mt_cor is a list:
#   mt_cor$cor   — correlation coefficients (one matrix per group)
#   mt_cor$pval  — p-values
#   mt_cor$fdr   — FDR-corrected

PlotModuleTraitCorrelation(
  seurat_obj,
  label        = 'fdr',           # or 'pval' or 'cor'
  label_symbol = 'stars',         # *** < 0.001, ** < 0.01, * < 0.05
  text_size    = 2.5,
  high_color   = 'yellow',
  mid_color    = 'black',
  low_color    = 'purple',
  plot_max     = 0.2,             # cap colour scale at |r| = 0.2
  combine      = TRUE             # one figure with patchwork
)
```

### Trait type compatibility

| Trait type | Supported? | Notes |
|---|---|---|
| Continuous (numeric) | ✅ | Use as-is |
| Binary categorical | ✅ | Convert to 0/1 before passing |
| Ordered categorical | ✅ | Convert to factor with explicit `levels = c('low', 'mid', 'high')`, then `as.numeric()` |
| Unordered categorical (>2 levels) | ❌ | The correlation is meaningless. Either dummy-encode or skip. |

Sample ID, mouse strain, batch — these are unordered categorical; if you want to test for batch effects on modules, use ANOVA via `FindDMEs` between groups instead.

Convenience: `Rscript scripts/downstream_analyses.R --rds seurat_hdwgcna.rds --task trait --traits age,Braak_stage`

Source: [hdWGCNA Module-Trait Correlation tutorial](https://smorabit.github.io/hdWGCNA/articles/module_trait_correlation.html).

---

## Downstream 3 — Functional Enrichment

**The question:** what biological processes / pathways does each module represent?

### Enrichr (web-based, fast, comprehensive)

```r
library(enrichR)

dbs <- c(
  'GO_Biological_Process_2023',
  'GO_Cellular_Component_2023',
  'GO_Molecular_Function_2023',
  'KEGG_2021_Human',
  'Reactome_2022',
  'WikiPathway_2023_Human'
)

seurat_obj <- RunEnrichr(
  seurat_obj,
  dbs        = dbs,
  max_genes  = 100,                # genes per module sent to Enrichr (sorted by kME)
  wgcna_name = 'INH'
)

enrich_df <- GetEnrichrTable(seurat_obj)
# Columns: Term, Overlap, P.value, Adjusted.P.value, Odds.Ratio, Combined.Score, Genes, db, module
```

### Visualize

```r
# Per-module bar plots — writes one PDF per module to outdir/
EnrichrBarPlot(
  seurat_obj,
  outdir     = 'figures/enrichr/',
  n_terms    = 10,
  plot_size  = c(5, 7),
  logscale   = TRUE,
  wgcna_name = 'INH'
)

# Single dot plot across all modules (great for figures)
EnrichrDotPlot(
  seurat_obj,
  mods       = 'all',
  database   = 'GO_Biological_Process_2023',
  n_terms    = 2,                # top N per module
  term_size  = 8,
  p_adj      = FALSE,
  wgcna_name = 'INH'
)
```

### GSEA via fgsea (when you have a ranked gene list)

For a continuous test (not over-representation), rank genes by kME within a module and use fgsea against any MSigDB collection:

```r
library(fgsea)
library(msigdbr)

# Pull genes ranked by kME in module of interest
modules <- GetModules(seurat_obj) %>% filter(module != 'grey')
target_mod <- 'INH-M3'
ranks <- modules %>%
  arrange(desc(.data[[paste0('kME_', target_mod)]])) %>%
  pull(.data[[paste0('kME_', target_mod)]], name = gene_name)

# Get pathways from MSigDB
pathways <- msigdbr(species = 'Homo sapiens', category = 'H') %>%
  split(x = .$gene_symbol, f = .$gs_name)

gsea_res <- fgsea(pathways, ranks, minSize = 10, maxSize = 500)
head(gsea_res[order(padj)])
```

Convenience: `Rscript scripts/downstream_analyses.R --rds seurat_hdwgcna.rds --task enrich --dbs GO_Biological_Process_2023,KEGG_2021_Human`

Source: [hdWGCNA Enrichment tutorial](https://smorabit.github.io/hdWGCNA/articles/enrichment_analysis.html).

---

## Visualization Cookbook

### Module feature plot (UMAP × module)

```r
plot_list <- ModuleFeaturePlot(seurat_obj, features = 'hMEs', order = TRUE)
wrap_plots(plot_list, ncol = 6)
# Alternatives: features = 'scores' (UCell hub scores), 'MEs' (raw, no Harmony)
```

### Module dot plot across cell types

```r
hMEs <- GetMEs(seurat_obj, harmonized = TRUE)
modules <- GetModules(seurat_obj)
mods <- levels(modules$module); mods <- mods[mods != 'grey']
seurat_obj@meta.data <- cbind(seurat_obj@meta.data, hMEs)
DotPlot(seurat_obj, features = mods, group.by = 'cell_type') +
  RotatedAxis() +
  scale_color_gradient2(high = 'red', mid = 'grey95', low = 'blue')
```

### Hub gene network (top hubs from each module)

```r
ModuleNetworkPlot(seurat_obj, n_hubs = 5)              # one figure per module
HubGeneNetworkPlot(seurat_obj, n_hubs = 5, n_other = 10)   # multi-module network
```

### Module–module correlation

```r
ModuleCorrelogram(seurat_obj, features = 'hMEs')
```

### Module radar plot across cell subtypes

```r
ModuleRadarPlot(
  seurat_obj,
  group.by = 'cluster_label',
  barcodes = seurat_obj@meta.data %>% filter(cell_type == 'INH') %>% rownames(),
  axis.label.size = 4
)
```

More patterns + custom layouts in [references/plotting_guide.md](references/plotting_guide.md).

---

## Key Parameters to Adjust

### Metacell construction (`MetacellsByGroups`)
- `k` (default 25): cells per metacell. Lower → finer resolution but noisier. For small datasets (< 2000 cells/type), drop to 10–15.
- `max_shared` (default 10): caps cell sharing across metacells. Higher → fewer metacells but more redundancy.
- `group.by`: always include both cell type **and** sample — otherwise metacells will mix donors and you'll lose biology.

### Soft power
- The plot from `PlotSoftPowers` should show R² climbing past 0.8. Pick the **lowest** power that crosses this threshold.
- Default `networkType='signed'` (recommended). `'unsigned'` collapses positive and negative correlations — usually wrong.

### Network construction (`ConstructNetwork`)
- `minModuleSize` (default 50): smallest allowed module. Reduce for small datasets.
- `mergeCutHeight` (default 0.2): merges similar modules. Increase (0.3–0.4) to consolidate noisy modules.
- `deepSplit` (0–4): how aggressively to split. 2 is default; 4 produces many tiny modules.

### Module eigengenes (`ModuleEigengenes`)
- `group.by.vars`: pass your batch/donor variable. Without this you get raw MEs (`MEs`) — use harmonized (`hMEs`) for downstream stats.

### kME / hub genes
- `n_hubs` (default 25 for ModuleExprScore): how many top-kME genes count as "the module's signature". 25 is a good default; for sparse single-cell data, 50 makes the signature more stable.

---

## Best Practices

- **Run one network per major cell type, not one big network across all cells.** WGCNA's TOM is dominated by cell-type identity otherwise — the "modules" will just be marker-gene signatures for each cell type rather than meaningful within-cell-type co-expression.
- **Save the Seurat object after `ConstructNetwork`.** It's the slow step — minutes to hours depending on n cells and n genes. Everything downstream (MEs, kME, hubs, DMEs, enrichment) is fast.
- **The grey module is the dumping ground for unassigned genes.** Always `subset(module != 'grey')` before downstream analyses.
- **Inspect `PlotDendrogram` BEFORE downstream analysis.** If the dendrogram has one giant module and a tiny grey blob, your soft power is too low. If it has 30+ skinny modules, soft power is too high or `minModuleSize` is too small.
- **For module-trait correlation with categorical traits, dummy-encode unordered factors instead of relying on alphabetical order.** Otherwise the correlation is meaningless.
- **Cross-condition module preservation matters.** A module discovered in controls may not exist in cases. Test with `ModulePreservation` before trusting cross-condition DME results.

---

## End-to-End Template

`assets/hdwgcna_template.R` is a single parameterized script — set the variables at the top (input `.rds`, cell type, sample column, condition column) and run end-to-end through DME + module-trait correlation + Enrichr.

```bash
Rscript assets/hdwgcna_template.R
```

## Convenience Scripts

- `scripts/build_network.R` — network construction only (the slow step). Saves an annotated Seurat object.
- `scripts/downstream_analyses.R` — DME / module-trait / enrichment. Subtask selected via `--task dme|trait|enrich|all`.

---

## References

- [hdWGCNA documentation](https://smorabit.github.io/hdWGCNA/) — Morabito et al., main project site
- [Basic tutorial](https://smorabit.github.io/hdWGCNA/articles/basic_tutorial.html)
- [Differential Module Eigengenes](https://smorabit.github.io/hdWGCNA/articles/differential_MEs.html)
- [Module–Trait Correlation](https://smorabit.github.io/hdWGCNA/articles/module_trait_correlation.html)
- [Enrichment Analysis](https://smorabit.github.io/hdWGCNA/articles/enrichment_analysis.html)
- Original WGCNA: Langfelder & Horvath (2008), *BMC Bioinformatics*
- hdWGCNA paper: Morabito, Reese, Rahimzadeh et al. (2023), *Cell Reports Methods*
