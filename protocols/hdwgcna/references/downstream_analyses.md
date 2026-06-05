# Downstream Analyses — DME, Module-Trait, Enrichment

The three analyses you'll run after `ConstructNetwork`. All operate on the Seurat object returned by the network-construction pipeline.

## 1. Differential Module Eigengenes (DME)

**Conceptually:** "is module M more active in condition A than condition B?" The test is a Wilcoxon (Mann-Whitney) rank-sum on the module eigengene values across the two cell groups.

### Two-group: `FindDMEs`

```r
group1 <- seurat_obj@meta.data %>%
  filter(cell_type == 'INH', condition == 'AD')   %>% rownames()
group2 <- seurat_obj@meta.data %>%
  filter(cell_type == 'INH', condition == 'Ctrl') %>% rownames()

DMEs <- FindDMEs(
  seurat_obj,
  barcodes1       = group1,
  barcodes2       = group2,
  test.use        = 'wilcox',         # only option implemented
  pseudocount.use = 0.01,             # for log2FC stability
  features        = NULL,             # NULL → all modules
  wgcna_name      = 'INH'
)
```

### Results table

```
        p_val  avg_log2FC  pct.1  pct.2  p_val_adj  module
INH-M1  1e-32    0.421     0.94   0.62   2e-31     INH-M1
INH-M2  2e-18   -0.156     0.71   0.83   4e-17     INH-M2
INH-M3  0.034    0.082     0.55   0.51   0.34      INH-M3
```

| Column | What it means |
|---|---|
| `p_val` | Raw Wilcoxon p-value |
| `avg_log2FC` | Mean hME in group1 minus group2, on log2 scale (with pseudocount) |
| `pct.1`/`pct.2` | Fraction of cells with non-zero ME in each group — useful for sparse modules |
| `p_val_adj` | Bonferroni-corrected (NOT FDR — hdWGCNA uses Bonferroni for module-level tests) |
| `module` | Module name |

`avg_log2FC > 0` → module is **up** in group1.

### One-vs-all: `FindAllDMEs`

For each level of `group.by`, runs that level vs all other cells:

```r
DMEs_all <- FindAllDMEs(
  seurat_obj,
  group.by   = 'cell_type',
  test.use   = 'wilcox',
  wgcna_name = 'INH'
)
# Adds a `group` column with the tested level
```

This is the right test for "which cell types is each module enriched in?".

### Visualizations

```r
# Lollipop — single comparison, ranked by effect size
PlotDMEsLollipop(
  seurat_obj, DMEs,
  group.by    = 'cell_type',
  comparison  = 'AD_vs_Ctrl',
  wgcna_name  = 'INH'
)

# Volcano — effect size × significance
PlotDMEsVolcano(
  seurat_obj, DMEs,
  plot_labels = TRUE,
  pval_cutoff = 0.05,
  wgcna_name  = 'INH'
)
```

### Testing on UCell module scores instead of MEs

For small / noisy modules where the eigengene PC1 doesn't capture the signal cleanly, test on the UCell signature of top hub genes:

```r
library(UCell)
seurat_obj <- ModuleExprScore(seurat_obj, n_genes = 25, method = 'UCell')

DMEs_scores <- FindDMEs(
  seurat_obj,
  barcodes1 = group1, barcodes2 = group2,
  features  = 'ModuleScores',
  wgcna_name = 'INH'
)
```

Results have the same shape but use the UCell scores. Often agrees with the ME-based test on the strong modules and disagrees on the borderline ones — that disagreement is itself informative.

### Handling multiple comparisons

For >2 conditions (e.g. control / mild / severe), either:

```r
# Pairwise — N(N-1)/2 tests
DMEs_mild_vs_ctrl   <- FindDMEs(seurat_obj, ..., barcodes1 = mild,   barcodes2 = ctrl)
DMEs_severe_vs_ctrl <- FindDMEs(seurat_obj, ..., barcodes1 = severe, barcodes2 = ctrl)
```

Or — better — use `ModuleTraitCorrelation` with the ordered factor as the trait (see next section).

---

## 2. Module–Trait Correlation

**Conceptually:** correlate each module's hME with one or more cell-level traits, stratified by cell type. The output is a heatmap of correlations + significance.

### Setting up traits

The trait vector must be numeric. Categorical traits need encoding:

```r
# Binary
seurat_obj$sex_binary <- as.numeric(seurat_obj$sex == 'F')

# Ordered (low/mid/high or stage 0/1/2/3)
seurat_obj$Braak_ordered <- as.numeric(factor(seurat_obj$Braak_stage,
                                               levels = c('I', 'II', 'III', 'IV', 'V', 'VI')))

# Continuous — use as-is
# seurat_obj$age is already numeric — leave it alone

# Disease status (control = 0, disease = 1)
seurat_obj$is_disease <- as.numeric(seurat_obj$condition == 'AD')

cur_traits <- c('age', 'Braak_ordered', 'sex_binary', 'is_disease')
```

**Never pass unordered multi-level factors** (`sample_id`, `mouse_strain` with 5 levels, etc.) — Pearson correlation requires numeric values with meaningful order. Without ordering, the correlation is meaningless.

### Running the analysis

```r
seurat_obj <- ModuleTraitCorrelation(
  seurat_obj,
  traits     = cur_traits,
  group.by   = 'cell_type',         # stratify by cell type
  cor_method = 'pearson',           # or 'kendall' / 'spearman'
  subset_by  = NULL,                # subset cells before correlation
  wgcna_name = 'INH'
)

mt_cor <- GetModuleTraitCorrelation(seurat_obj)
# mt_cor is a nested list:
#   mt_cor$cor  — list of correlation matrices (one per group)
#   mt_cor$pval — same shape, raw p-values
#   mt_cor$fdr  — same shape, FDR-adjusted
```

The result is **one matrix per cell type** (the levels of `group.by`), with modules on rows and traits on columns.

### Visualization

```r
PlotModuleTraitCorrelation(
  seurat_obj,
  label        = 'fdr',           # which value to display in cells
  label_symbol = 'stars',         # 'stars' = *** ** *; 'numeric' = "0.034"
  text_size    = 2.5,
  text_digits  = 2,
  text_color   = 'white',
  high_color   = 'yellow',
  mid_color    = 'black',
  low_color    = 'purple',
  plot_max     = 0.2,             # cap |cor| at this for the colour scale
  combine      = TRUE,            # one plot via patchwork
  wgcna_name   = 'INH'
)
```

Stars: `***` < 0.001, `**` < 0.01, `*` < 0.05. The exact thresholds are configurable via `stars_cutoffs = c(0.001, 0.01, 0.05)`.

### Reading the heatmap

- **Strong colour + stars**: module correlates with that trait
- **Strong colour, no stars**: correlation is large but unstable (small group, high variance)
- **Weak colour, stars**: significant but biologically marginal — easy to find with thousands of cells; don't over-interpret

Always look at both colour intensity AND significance.

### When correlation is the wrong test

- **Binary trait with extreme class imbalance** (e.g. 5% cases vs 95% controls): use `FindDMEs` instead. Pearson is dominated by the majority class.
- **Cyclical traits** (time-of-day, cell-cycle phase): linear correlation is wrong. Use a circular statistic or fit a periodic model directly on the MEs.
- **Categorical traits where ordering is uncertain**: don't force an ordering — use `FindDMEs` between adjacent pairs.

---

## 3. Functional Enrichment

**Conceptually:** for each module, send its top-N genes (by kME) to Enrichr (or a similar tool) and ask which gene sets are over-represented.

### Enrichr

[Enrichr](https://maayanlab.cloud/Enrichr/) hosts hundreds of gene-set libraries. hdWGCNA wraps the [enrichR](https://cran.r-project.org/web/packages/enrichR/) R interface.

```r
library(enrichR)

dbs <- c(
  'GO_Biological_Process_2023',
  'GO_Cellular_Component_2023',
  'GO_Molecular_Function_2023',
  'KEGG_2021_Human',
  'Reactome_2022',
  'WikiPathway_2023_Human',
  'MSigDB_Hallmark_2020',
  'TF_Perturbations_Followed_by_Expression',
  'CellMarker_2024'
)

seurat_obj <- RunEnrichr(
  seurat_obj,
  dbs        = dbs,
  max_genes  = 100,             # top 100 hubs per module (by kME)
  wgcna_name = 'INH'
)

enrich_df <- GetEnrichrTable(seurat_obj)
```

The full results table:

```
Term                    Overlap  P.value     Adjusted.P.value  Odds.Ratio  Combined.Score  Genes               db                              module
Synaptic vesicle...     12/85    1.2e-08     3.4e-06           4.5         85.3            SYT1;STX1A;...     GO_Biological_Process_2023      INH-M1
Voltage-gated...        8/52     3.4e-06     1.2e-04           3.8         52.7            CACNA1B;...        GO_Biological_Process_2023      INH-M1
...
```

| Column | Use it for |
|---|---|
| `Overlap` | Quick sanity: "X out of Y genes in the term" |
| `P.value` | Raw — usually want `Adjusted.P.value` |
| `Adjusted.P.value` | Benjamini-Hochberg |
| `Combined.Score` | Enrichr's proprietary rank: `log(P.value) * Z.score` — useful for sorting |
| `Genes` | Semicolon-separated list of hits |

### Visualizations

```r
# Bar plots — one PDF per module, top-N terms by combined score
EnrichrBarPlot(
  seurat_obj,
  outdir     = 'figures/enrichr/',
  n_terms    = 10,
  plot_size  = c(5, 7),
  logscale   = TRUE,
  wgcna_name = 'INH'
)

# Dot plot — multiple modules, one database
EnrichrDotPlot(
  seurat_obj,
  mods       = 'all',         # or a subset c('INH-M1', 'INH-M3')
  database   = 'GO_Biological_Process_2023',
  n_terms    = 2,             # top N per module
  term_size  = 8,
  p_adj      = FALSE,         # use raw p in the colour scale
  wgcna_name = 'INH'
)
```

### GSEA via fgsea (continuous, not over-representation)

Over-representation analysis (ORA — what Enrichr does) takes a binary "in module / not in module" cut. **GSEA** uses the full ranking, which can reveal terms enriched at module margins.

```r
library(fgsea)
library(msigdbr)

# Pull genes ranked by kME within one module
modules <- GetModules(seurat_obj) %>% filter(module != 'grey')
target_mod <- 'INH-M3'
kme_col <- paste0('kME_', target_mod)
ranks <- modules %>%
  filter(module == target_mod) %>%
  arrange(desc(.data[[kme_col]])) %>%
  pull(.data[[kme_col]], name = gene_name)

# MSigDB pathways
pathways <- msigdbr(species = 'Homo sapiens', category = 'H') %>%
  split(x = .$gene_symbol, f = .$gs_name)

gsea_res <- fgsea(pathways, ranks, minSize = 10, maxSize = 500, eps = 0)
head(gsea_res[order(padj)])

plotEnrichment(pathways[['HALLMARK_INFLAMMATORY_RESPONSE']], ranks) +
  ggtitle('INH-M3: HALLMARK_INFLAMMATORY_RESPONSE')
```

### Interpreting enrichment

- **The grey module is meaningless** — never enrich it.
- **Treat the bar plots as exploratory.** With 9+ databases × 20+ modules, you'll find spurious hits. Pre-register a hypothesis or use a stricter FDR (0.01 instead of 0.05).
- **Small modules (< 30 genes)** have unstable enrichment — the top term changes if you swap a few hub genes. Pool with adjacent modules or skip.
- **Confirm hub gene biology manually.** If an enrichment claims "GO: synaptic transmission", manually verify 3-5 of the top hubs are actually synaptic. Enrichr databases are noisy.

---

## Putting it together

A common publication-ready figure: one heatmap of module-trait correlation, one volcano of DMEs, one dot plot of GO enrichment. The script `scripts/downstream_analyses.R` produces all three given a hdWGCNA-augmented Seurat object.

```bash
Rscript scripts/downstream_analyses.R \
  --rds       seurat_hdwgcna.rds \
  --task      all \
  --cell-type INH \
  --group-by  condition --group1 AD --group2 Ctrl \
  --traits    age,Braak_ordered,sex_binary,is_disease \
  --dbs       GO_Biological_Process_2023,KEGG_2021_Human \
  --outdir    results/
```
