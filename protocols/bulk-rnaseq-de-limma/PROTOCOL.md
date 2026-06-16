# Bulk RNA-seq Differential Expression with limma (FPKM input)

Differential expression pipeline for bulk RNA-seq using limma's empirical-Bayes
(`eBayes`) moderated t-statistics, starting from an **FPKM** expression matrix.
Because FPKM is already normalized for library size and gene length, this uses
the **limma-trend** approach on log2(FPKM) — NOT `voom` (voom models the
mean–variance relationship from raw counts and must not be applied to FPKM).

## Environment
- R >= 4.0 with BiocManager
- Required packages: limma, ggplot2, pheatmap, EnhancedVolcano, enrichR, dplyr, readr, RColorBrewer
- enrichR queries the Enrichr web service — the analysis node needs outbound HTTPS

## Input Requirements
- FPKM matrix (genes × samples); gene symbols as row names are ideal for enrichR
- Sample metadata with a condition/group column (and any covariates to adjust for)
- File formats: CSV, TSV, or a gene-by-sample expression table
- Two or more groups; record the smallest group size (used for filtering)

## Workflow Steps

1. **Load Data**: Read the FPKM matrix and metadata; align sample columns to metadata rows.
   ```r
   library(limma); library(dplyr)
   fpkm <- as.matrix(read.csv("fpkm.csv", row.names = 1))
   meta <- read.csv("metadata.csv", row.names = 1)
   fpkm <- fpkm[, rownames(meta)]                 # enforce column/row order
   group <- factor(meta$condition)
   ```

2. **Filter Lowly Expressed Genes**: Keep genes expressed above a small FPKM
   threshold in at least as many samples as the smallest group (avoids
   single-group artifacts and stabilizes the variance trend).
   ```r
   min_n <- min(table(group))
   keep  <- rowSums(fpkm > 1) >= min_n            # FPKM > 1 in >= smallest group
   fpkm  <- fpkm[keep, ]
   ```

3. **Log2 Transformation**: Stabilize variance with an offset to avoid log(0).
   ```r
   logFPKM <- log2(fpkm + 1)
   ```
   (Skip this step only if the input is already on a log scale — check the data range first.)

4. **Design Matrix**: Model groups (add covariates as `+ batch` etc. for adjustment).
   ```r
   design <- model.matrix(~ 0 + group)
   colnames(design) <- levels(group)
   ```

5. **Fit Linear Model**:
   ```r
   fit <- lmFit(logFPKM, design)
   ```

6. **Contrasts**: Define the comparison(s) of interest.
   ```r
   cm  <- makeContrasts(treated - control, levels = design)
   fit <- contrasts.fit(fit, cm)
   ```

7. **Empirical Bayes (limma-trend)**: `trend = TRUE` accounts for the
   mean–variance relationship of log-FPKM; `robust = TRUE` guards against outlier genes.
   ```r
   fit <- eBayes(fit, trend = TRUE, robust = TRUE)
   res <- topTable(fit, coef = 1, number = Inf, adjust.method = "BH", sort.by = "P")
   ```

8. **PCA**: On the log-FPKM matrix (samples as observations), colored by group.
   ```r
   pca <- prcomp(t(logFPKM), scale. = TRUE)
   var <- round(100 * pca$sdev^2 / sum(pca$sdev^2), 1)
   df  <- data.frame(PC1 = pca$x[,1], PC2 = pca$x[,2], group = group)
   ggplot2::ggplot(df, ggplot2::aes(PC1, PC2, color = group)) +
     ggplot2::geom_point(size = 3) +
     ggplot2::labs(x = paste0("PC1 (", var[1], "%)"), y = paste0("PC2 (", var[2], "%)"))
   ```

9. **Heatmap of Top DEGs**: Row-scaled (z-score) log-FPKM for the top genes.
   ```r
   top <- rownames(res)[res$adj.P.Val < 0.05 & abs(res$logFC) > 1]
   top <- head(top, 50)
   pheatmap::pheatmap(logFPKM[top, ], scale = "row",
                      annotation_col = data.frame(group, row.names = colnames(logFPKM)),
                      show_rownames = TRUE, show_colnames = TRUE)
   ```

10. **Volcano Plot**:
    ```r
    EnhancedVolcano::EnhancedVolcano(res, lab = rownames(res),
      x = "logFC", y = "adj.P.Val",
      pCutoff = 0.05, FCcutoff = 1, title = "treated vs control")
    ```

11. **Enrichment Analysis (enrichR)**: Query Enrichr with the significant gene
    symbols; run up- and down-regulated sets separately for directional biology.
    ```r
    library(enrichR)
    sig  <- subset(res, adj.P.Val < 0.05 & abs(logFC) > 1)
    up   <- rownames(sig)[sig$logFC > 0]
    down <- rownames(sig)[sig$logFC < 0]
    dbs  <- c("GO_Biological_Process_2023", "GO_Molecular_Function_2023",
              "KEGG_2021_Human", "Reactome_2022", "MSigDB_Hallmark_2020")
    enr_up   <- enrichr(up,   dbs)
    enr_down <- enrichr(down, dbs)
    plotEnrich(enr_up[["GO_Biological_Process_2023"]], showTerms = 20, numChar = 50,
               y = "Count", orderBy = "Adjusted.P.value")
    ```
    (Use `org.Mm.eg.db`/mouse Enrichr libraries, e.g. `KEGG_2019_Mouse`, for mouse data.)

12. **Export Results**: Save the full `topTable` (gene, logFC, AveExpr, t, P.Value,
    adj.P.Val) and each Enrichr table as CSV; include contrast + sample size in filenames.

## Thresholds
- Significant DEG: `adj.P.Val < 0.05` AND `|logFC| > 1`
- Filter: FPKM > 1 in ≥ smallest-group-size samples
- Enrichment significant: `Adjusted.P.value < 0.05`
- Report total DEGs and up/down counts separately

## Conventions
- FPKM is already library-size + gene-length normalized — use **limma-trend**
  (`eBayes(trend=TRUE)`), never `voom`, which requires raw counts.
- log2(FPKM + 1) before modeling; verify the data isn't already log-scaled.
- enrichR needs gene **symbols** — map IDs to symbols before step 11 if rows are Ensembl/Entrez.
- Run enrichR on up- and down-regulated genes separately, not the combined set.
- For count data, prefer the DESeq2 protocol or limma-voom; FPKM + limma-trend is
  appropriate when only FPKM is available.
- Save plots as PDF (publication) and PNG (review); include contrast and n in filenames.

## Related Skills

- bulk-rnaseq-deseq2 - Count-based DE (DESeq2); prefer it when raw counts are available rather than FPKM
- expression-matrix-normalization - Why FPKM/TPM aren't ideal for DE and the log2/log-CPM rationale; read before choosing limma-trend over voom
- enhanced-volcano-plot - Volcano plotting from the limma `topTable` (`logFC` / `adj.P.Val`); the visualization step in detail
- pathway-analysis-gsea - GSEA (rank-based) as a complement to the enrichR over-representation step
- proteomics-differential-abundance - The same limma `eBayes` moderated-t pattern applied to protein abundances
