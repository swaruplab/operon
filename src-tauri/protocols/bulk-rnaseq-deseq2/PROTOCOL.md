# Bulk RNA-seq Differential Expression with DESeq2

Standard pipeline for bulk RNA-seq differential expression analysis using DESeq2 in R.

## Environment
- R >= 4.0 with BiocManager
- Required packages: DESeq2, ggplot2, EnhancedVolcano, pheatmap, dplyr, readr
- Optional: clusterProfiler for pathway analysis, org.Hs.eg.db or org.Mm.eg.db for annotations

## Input Requirements
- Raw count matrix (genes × samples), NOT normalized — DESeq2 handles normalization internally
- Sample metadata with condition/group columns
- File formats: CSV, TSV, or featureCounts output

## Workflow Steps

1. **Load Data**: Read count matrix and metadata into R
2. **Create DESeqDataSet**: `dds <- DESeqDataSetFromMatrix(countData, colData, design = ~ condition)`
3. **Pre-filtering**: Remove low-count genes: `keep <- rowSums(counts(dds)) >= 10; dds <- dds[keep,]`
4. **Run DESeq2**: `dds <- DESeq(dds)`
5. **Extract Results**: `res <- results(dds, contrast = c("condition", "treated", "control"), alpha = 0.05)`
6. **Shrink LFC**: `res <- lfcShrink(dds, coef = "condition_treated_vs_control", type = "apeglm")`
7. **Visualizations**:
   - MA plot: `plotMA(res)`
   - Volcano plot: `EnhancedVolcano(res, x='log2FoldChange', y='padj', lab=rownames(res))`
   - PCA: `plotPCA(vst(dds), intgroup='condition')`
   - Heatmap of top DEGs: `pheatmap(assay(vst(dds))[top_genes,])`
8. **Export Results**: Save full results table as CSV with gene names, log2FC, padj
9. **Pathway Analysis** (optional): GSEA with fgsea or over-representation with clusterProfiler

## Thresholds
- Significant: padj < 0.05 AND |log2FoldChange| > 1
- Use lfcShrink for reliable fold change estimates
- Report both total DEGs and up/down separately

## Conventions
- Always use raw counts, never FPKM/TPM for DESeq2
- Save plots as PDF for publication quality, PNG for quick review
- Include sample size and contrast in output filenames
- Check size factors (`sizeFactors(dds)`) for outliers before proceeding
