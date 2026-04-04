# Enhanced Volcano Plot Protocol

Create publication-quality volcano plots from differential expression results.

## Environment
- Python: matplotlib, seaborn, adjustText, pandas, numpy
- R: EnhancedVolcano (Bioconductor), ggplot2, ggrepel

## Input Format
- CSV/TSV with columns: gene name, log2FoldChange, padj (or pvalue)
- Common column names: gene, logFC, log2FoldChange, FDR, padj, p_val_adj

## Python Implementation

1. Load DE results with pandas
2. Add significance categories: up (log2FC > 1, padj < 0.05), down (log2FC < -1, padj < 0.05), ns
3. Create scatter plot with -log10(padj) on y-axis, log2FC on x-axis
4. Color by category: red for up, blue for down, grey for ns
5. Add threshold lines: horizontal at -log10(0.05), vertical at ±1
6. Label top N significant genes using adjustText to avoid overlaps
7. Use `figsize=(10, 8)`, `dpi=300` for publication quality

## R Implementation (EnhancedVolcano)

```r
EnhancedVolcano(res,
  lab = rownames(res),
  x = 'log2FoldChange',
  y = 'padj',
  pCutoff = 0.05,
  FCcutoff = 1.0,
  pointSize = 2.0,
  labSize = 3.5,
  title = 'Treated vs Control',
  subtitle = 'DESeq2 results')
```

## Style Guidelines
- Always include: title with comparison, axis labels, legend
- Label top 15-20 genes by significance (smallest padj)
- Font size: title 16pt, axis labels 12pt, gene labels 8-10pt
- Save as both PNG (300 DPI) and PDF
- Add gene count annotations: "N up, N down, N total DEGs"
