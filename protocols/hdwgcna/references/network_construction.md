# Network Construction — Deep Dive

This is the meat of hdWGCNA: turning a Seurat object into a co-expression network. Most of the tuning lives here. Downstream analyses (DME, module-trait, enrichment) are cheap once the network is built.

## The hdWGCNA experiment slot

Every hdWGCNA call modifies one named "experiment" inside `seurat_obj@misc`. Multiple experiments can coexist — useful for running parallel networks across cell types:

```r
seurat_obj <- SetupForWGCNA(seurat_obj, ..., wgcna_name = "INH")
seurat_obj <- SetupForWGCNA(seurat_obj, ..., wgcna_name = "EXC")
seurat_obj <- SetupForWGCNA(seurat_obj, ..., wgcna_name = "MG")
# Three separate networks in seurat_obj@misc, each with its own modules / hubs / MEs.
```

Every subsequent call accepts `wgcna_name = ...` to target a specific experiment. Forget this and you'll overwrite the previous network silently.

## Step 1 — `SetupForWGCNA` and gene selection

```r
seurat_obj <- SetupForWGCNA(
  seurat_obj,
  gene_select = "fraction",     # "variable" | "fraction" | "custom"
  fraction    = 0.05,           # keep genes expressed in ≥ 5% of cells
  wgcna_name  = "INH"
)
```

| `gene_select` | What it does | When to use |
|---|---|---|
| `"variable"` | Uses `Seurat::VariableFeatures()` — typically ~2000 HVGs | Default for most analyses. Already in your Seurat pipeline. |
| `"fraction"` | Genes expressed in ≥ `fraction` of cells | Better when you want a wider net than HVGs. 0.05 is a reasonable default. |
| `"custom"` | Pass a custom gene list via `gene_list = c(...)` | Targeted analyses, panel-based platforms (Xenium / CosMx) |

**For panel-based platforms (~300-1000 genes):** use `gene_select = 'custom'` and pass the full panel. Variable-gene selection is meaningless on a curated panel.

## Step 2 — `MetacellsByGroups`

The core innovation. WGCNA's correlation structure breaks on sparse single-cell data — too many zero–zero pairs inflate correlation noise. Metacells fix this by aggregating k nearest neighbors per group.

```r
seurat_obj <- MetacellsByGroups(
  seurat_obj,
  group.by    = c("cell_type", "Sample"),     # CRITICAL: include sample
  reduction   = 'harmony',                     # use the integrated embedding
  k           = 25,                            # cells per metacell
  max_shared  = 10,                            # neighbor sharing cap
  min_cells   = 100,                           # drop groups with < 100 cells
  ident.group = 'cell_type'                    # which group becomes the metacell identity
)
seurat_obj <- NormalizeMetacells(seurat_obj)
```

**Why `group.by` must include sample/donor:** without it, k-NN finds nearest neighbors across donors and your metacells average over biological variability between samples — exactly the variability you usually want to *preserve* for downstream comparisons.

**Parameter intuition:**
- `k`: bigger = denoised metacells, but fewer of them and less variability for WGCNA. 25 is the sweet spot for ~10k cells per cell type.
- `max_shared`: each cell can appear in up to N metacells. Lower (5-8) → cleaner separation, fewer metacells. Higher (15-20) → more metacells but more redundancy.
- `min_cells`: groups (cell_type × Sample) smaller than this are dropped. 100 is a sane default — below it metacells are statistically meaningless.

### Inspect metacell yield

```r
metacell_obj <- GetMetacellObject(seurat_obj)
print(metacell_obj)
table(metacell_obj$cell_type)
# Want: ≥ 50 metacells per group of interest, ideally 100+
```

If you have < 50 metacells for your group of interest, either lower `k` or you genuinely don't have enough cells — WGCNA on too few metacells produces unstable modules.

### Optional: visualize metacell embedding

```r
seurat_obj <- ScaleMetacells(seurat_obj, features = VariableFeatures(seurat_obj))
seurat_obj <- RunPCAMetacells(seurat_obj, features = VariableFeatures(seurat_obj))
seurat_obj <- RunHarmonyMetacells(seurat_obj, group.by.vars = 'Sample')
seurat_obj <- RunUMAPMetacells(seurat_obj, reduction = 'harmony', dims = 1:15)

DimPlotMetacells(seurat_obj, group.by = 'cell_type') + ggtitle("Metacells")
```

If the metacell UMAP doesn't recapitulate your single-cell UMAP topology, something is wrong with `MetacellsByGroups` parameters.

## Step 3 — `SetDatExpr`

Picks **which cells** become the input to network construction. Almost always: one cell type at a time.

```r
seurat_obj <- SetDatExpr(
  seurat_obj,
  group_name = "INH",
  group.by   = 'cell_type',
  assay      = 'RNA',
  layer      = 'data',       # log-normalized expression
  use_metacells = TRUE       # default TRUE — use metacells, not single cells
)
```

The expression matrix passed to WGCNA lives at `GetDatExpr(seurat_obj)` after this call. Verify dimensions:

```r
datExpr <- GetDatExpr(seurat_obj)
dim(datExpr)
# Want: (n_metacells, n_genes_passing_filter)
# Rows = metacells, columns = genes — WGCNA's convention
```

**Multiple cell types in one network:** pass a vector to `group_name` (e.g. `c('INH', 'EXC')`). Generally discouraged — see [Best Practices](../SKILL.md#best-practices).

## Step 4 — Soft-power selection

WGCNA raises the correlation matrix to a power β to enforce scale-free topology. Higher β = more aggressive pruning of weak edges.

```r
seurat_obj <- TestSoftPowers(seurat_obj, networkType = 'signed', powers = 1:30)
plot_list <- PlotSoftPowers(seurat_obj)
wrap_plots(plot_list, ncol = 2)
```

The function tries powers 1–30 (or whatever you pass to `powers`) and computes the R² to a scale-free topology fit at each.

**Pick the lowest power giving SFT R² ≥ 0.8.** That's it. Most single-cell data ends up at β = 6–12.

Inspect the table directly:
```r
power_table <- GetPowerTable(seurat_obj)
power_table
# Power  SFT.R.sq  slope  truncated.R.sq  mean.k.  median.k.  max.k.
#     1    0.071    7.6           0.86    287.4     279.5    478.4
#     2    0.236   -1.4           0.95    115.8     108.0    266.7
# ...
```

Higher mean.k = denser network. Aim for mean.k ≈ 10–50 at the chosen power.

## Step 5 — `ConstructNetwork`

The slow step. Computes TOM, clusters genes, assigns module colors.

```r
seurat_obj <- ConstructNetwork(
  seurat_obj,
  soft_power      = NULL,        # NULL = auto-pick from TestSoftPowers
  setDatExpr      = FALSE,       # we already did SetDatExpr
  tom_name        = 'INH',
  tom_outdir      = 'TOM/',
  minModuleSize   = 50,
  mergeCutHeight  = 0.2,
  deepSplit       = 2,
  detectCutHeight = 0.995,
  overwrite_tom   = TRUE
)
PlotDendrogram(seurat_obj, main = 'INH hdWGCNA Dendrogram')
```

The TOM is written to disk as `TOM/INH_TOM.rda` (~50 MB–1 GB depending on n genes). Don't delete it — `ModuleConnectivity` reads it back.

**Tuning parameters when modules look wrong:**

| Symptom | Fix |
|---|---|
| One giant module + grey | Raise `soft_power` (more pruning) OR increase `deepSplit` (4 = aggressive) |
| Many tiny modules (< 30 genes) | Raise `minModuleSize` to 30-50, OR raise `mergeCutHeight` to 0.3-0.4 |
| Modules look like cell-type markers | You probably ran across multiple cell types. Subset to one cell type via `SetDatExpr`. |
| All genes in grey | `soft_power` too high, gene selection too narrow, or insufficient metacells |

Inspect the dendrogram after every parameter change — it's the fastest diagnostic.

## Step 6 — `ModuleEigengenes`

The first principal component of each module → a single value per cell.

```r
seurat_obj <- ScaleData(seurat_obj, features = VariableFeatures(seurat_obj))
seurat_obj <- ModuleEigengenes(
  seurat_obj,
  group.by.vars = "Sample",       # harmonize across this variable
  modules       = NULL,           # NULL = all modules
  vars.to.regress = NULL,         # optional Seurat ScaleData covariates
  scale_model.use = 'linear'
)
```

Two matrices result:
- `MEs`: raw module eigengenes
- `hMEs`: same but Harmony-corrected for `group.by.vars`. **Use this for downstream stats.**

Retrieve via:
```r
hMEs <- GetMEs(seurat_obj, harmonized = TRUE)
MEs  <- GetMEs(seurat_obj, harmonized = FALSE)
```

## Step 7 — `ModuleConnectivity` (kME)

For each gene, computes the correlation with every module's eigengene. Genes with high kME for module M are "hub" genes for M.

```r
seurat_obj <- ModuleConnectivity(
  seurat_obj,
  group.by   = 'cell_type',
  group_name = 'INH',
  corFnc     = 'cor',          # or 'bicor' for robust
  corOptions = "use='p'"
)
```

This adds kME columns to the modules dataframe — one per module:

```r
GetModules(seurat_obj) %>% head()
# gene_name | module  | color | kME_grey | kME_INH-M1 | kME_INH-M2 | ...
```

A gene is **assigned to its highest-kME non-grey module**. The `kME_<own_module>` is its connectivity within that module.

## Hub gene extraction

```r
# Top-N hubs per module by kME
hub_df <- GetHubGenes(seurat_obj, n_hubs = 25)

# Or filter the modules table manually
hubs_M1 <- GetModules(seurat_obj) %>%
  filter(module == 'INH-M1') %>%
  arrange(desc(`kME_INH-M1`)) %>%
  head(25)
```

## UCell hub gene signatures

A more robust per-cell "module activity" signal than raw MEs — useful when modules are small or eigengenes are noisy.

```r
library(UCell)
seurat_obj <- ModuleExprScore(
  seurat_obj,
  n_genes = 25,
  method  = 'UCell'      # or 'Seurat' for AddModuleScore
)
```

Adds one column per module to `seurat_obj@meta.data` with the per-cell score.

## Renaming modules

WGCNA assigns modules colors (`turquoise`, `blue`, `brown`, ...) and integer IDs. For readable plots, rename:

```r
seurat_obj <- ResetModuleNames(
  seurat_obj,
  new_name = "INH-M"        # → "INH-M1", "INH-M2", ...
)
```

Run this **before** any downstream analysis — `FindDMEs`, `ModuleTraitCorrelation`, and `RunEnrichr` use the module names as-is in their output.

## Saving and resuming

```r
# After ConstructNetwork — save the slow work
saveRDS(seurat_obj, 'seurat_obj_hdwgcna.rds')

# Resume in a new session
library(Seurat); library(hdWGCNA); library(WGCNA); library(tidyverse)
seurat_obj <- readRDS('seurat_obj_hdwgcna.rds')
# All downstream functions still work — the TOM is read from TOM/ when needed
```

The TOM file (`TOM/INH_TOM.rda`) is a separate artifact. Move it together with the `.rds` if you copy the analysis to another machine.

## Multi-cell-type networks (parallel pipelines)

```r
cell_types_to_model <- c('INH', 'EXC', 'MG', 'ASC')

for (ct in cell_types_to_model) {
  seurat_obj <- SetupForWGCNA(seurat_obj, gene_select = 'fraction',
                               fraction = 0.05, wgcna_name = ct)
  seurat_obj <- MetacellsByGroups(seurat_obj, group.by = c('cell_type', 'Sample'),
                                   reduction = 'harmony', k = 25, max_shared = 10,
                                   ident.group = 'cell_type')
  seurat_obj <- NormalizeMetacells(seurat_obj)
  seurat_obj <- SetDatExpr(seurat_obj, group_name = ct, group.by = 'cell_type')
  seurat_obj <- TestSoftPowers(seurat_obj, networkType = 'signed')
  seurat_obj <- ConstructNetwork(seurat_obj, tom_name = ct, overwrite_tom = TRUE)
  seurat_obj <- ModuleEigengenes(seurat_obj, group.by.vars = 'Sample')
  seurat_obj <- ModuleConnectivity(seurat_obj, group.by = 'cell_type', group_name = ct)
  seurat_obj <- ResetModuleNames(seurat_obj, new_name = paste0(ct, '-M'))
  saveRDS(seurat_obj, paste0('seurat_', ct, '_hdwgcna.rds'))
}
```

For long jobs (5+ cell types × hours each) — run as an sbatch array.

## Common pitfalls

- **Forgetting `wgcna_name` on downstream calls.** If you've set up multiple experiments, every downstream function needs `wgcna_name = '...'` to target the right one. Otherwise it operates on whichever was set last.
- **Not running `ResetModuleNames`.** Default names are `turquoise`, `blue`, `brown` etc. — fine for inspection, awful for figures with 20+ modules.
- **Trusting raw MEs cross-condition.** If your conditions are confounded with sample/donor, the raw MEs reflect batch as much as biology. Always use `hMEs` (Harmony-corrected) for cross-condition stats.
- **Running on too few metacells.** < 50 metacells per group → modules are noise. Add cells or pool groups.
- **Treating the grey module as biology.** It's the unassigned bucket; always filter it out.
