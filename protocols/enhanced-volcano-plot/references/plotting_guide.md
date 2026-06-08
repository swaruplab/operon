# Volcano Plot — Plotting Guide

## Log-transforming the y-axis

EnhancedVolcano (and every conventional volcano plot) plots `-log10(p)` on the
y-axis, **not** raw p-values. That is the only way to spread out the dense
range near `p = 0` so significant points actually leave the floor. Do **not**
log-transform the column yourself before passing it in — `EnhancedVolcano`
does it internally from `y = "padj"`. If you log-transform twice the y-axis
will collapse to a horizontal line and every gene will look identical.

If your significance column already contains `-log10(p)` (some tools export
this), undo it first (`10^(-x)`) or pass the raw column instead.

## Fold-change cutoff and p-value cutoff are independent

The vertical lines (`FCcutoff`) and the horizontal line (`pCutoff`) answer
two different questions:

- **`pCutoff`** — "Is this difference real?" — a statistical claim.
- **`FCcutoff`** — "Is this difference big enough to care about?" — a
  biological claim.

A gene with `padj = 1e-40` and `log2FC = 0.05` is statistically real but
biologically tiny. A gene with `log2FC = 4` and `padj = 0.3` is biologically
striking but probably noise. Pick the two cutoffs separately, based on the
experiment — don't reuse the same default everywhere.

For bulk RNA-seq: `padj < 0.05`, `|log2FC| > 1` is the conventional
starting point. For single-cell (where p-values are inflated by per-cell
pseudoreplication): `padj < 0.01`, `|log2FC| > 0.25` is more honest.

## Label connectors — when on, when off

`drawConnectors = TRUE` is right when you label ≤ 20 genes and want the
reader to trace each label back to its point. It gets noisy past ~30 labels;
in that regime, either reduce `LABEL_TOP_N`, raise `max.overlaps`, or drop
connectors and let `ggrepel` push labels freely.

## Common mistakes

1. **Mistaking `padj` for raw `pvalue`** — using the raw p-value (uncorrected
   for multiple testing) inflates apparent significance by orders of
   magnitude. Always prefer the adjusted column.
2. **Mislabelling axes** — `log2FC` of `1.0` means a 2-fold change, not a
   100% increase from baseline. Be explicit in the axis label.
3. **Over-labelling tiny effects** — labelling 100+ genes turns the plot into
   a wordcloud. Stop at the top 15-20.
4. **Asymmetric cutoffs** — using `log2FC > 1` for up but `< -0.5` for down
   is rarely justifiable and breaks visual comparability. Keep cutoffs
   symmetric unless you have a real biological reason.
