#!/usr/bin/env Rscript
# enhanced_volcano_template.R — Publication-quality volcano plot from a
# differential-expression table.
#
# Usage (CLI):
#   Rscript enhanced_volcano_template.R \
#     --input  results/treated_vs_control.tsv \
#     --output figures/volcano_treated_vs_control.pdf \
#     --p-cutoff 0.05 --fc-cutoff 1.0 --label-top-n 20
#
# Usage (edit-then-run):
#   Edit the CONFIGURATION block below, then:
#     Rscript enhanced_volcano_template.R
#
# Output file extension determines the format (.pdf or .png). PNG is rendered
# at 300 dpi.

# ============================================================================
# DEPENDENCY GUARD — install missing packages, then load
# ============================================================================

.ensure_cran <- function(pkg) {
  if (!requireNamespace(pkg, quietly = TRUE)) {
    install.packages(pkg, repos = "https://cloud.r-project.org")
  }
}

.ensure_bioc <- function(pkg) {
  if (!requireNamespace(pkg, quietly = TRUE)) {
    .ensure_cran("BiocManager")
    BiocManager::install(pkg, update = FALSE, ask = FALSE)
  }
}

.ensure_cran("optparse")
.ensure_cran("ggrepel")
.ensure_cran("magrittr")
.ensure_cran("ggplot2")
.ensure_bioc("EnhancedVolcano")

suppressPackageStartupMessages({
  library(optparse)
  library(ggplot2)
  library(ggrepel)
  library(magrittr)
  library(EnhancedVolcano)
})

# ============================================================================
# CONFIGURATION — defaults, overridden by CLI flags
# ============================================================================

INPUT            <- NULL                  # path to DE table; required
OUTPUT           <- NULL                  # path to plot (.pdf or .png); required

GENE_COL         <- "gene"
X_COL            <- "log2FoldChange"
Y_COL            <- "padj"

P_CUTOFF         <- 0.05
FC_CUTOFF        <- 1.0
LABEL_TOP_N      <- 15L

TITLE            <- NULL                  # default: filename stem
SUBTITLE         <- ""                    # default: DEG count caption

WIDTH            <- 10                    # inches
HEIGHT           <- 8                     # inches
POINT_SIZE       <- 2.0
LABEL_SIZE       <- 3.5
DRAW_CONNECTORS  <- TRUE
MAX_OVERLAPS     <- 15L

COLOUR_UP        <- "#E41A1C"             # red
COLOUR_DOWN      <- "#377EB8"             # blue
COLOUR_NS        <- "grey70"

# ============================================================================
# CLI PARSING
# ============================================================================

option_list <- list(
  make_option(c("-i", "--input"),         type = "character", default = INPUT,
              help = "Input DE table (.csv / .tsv / .txt)"),
  make_option(c("-o", "--output"),        type = "character", default = OUTPUT,
              help = "Output plot path (.pdf or .png)"),
  make_option("--gene-col",               type = "character", default = GENE_COL,
              help = "Gene-name column [default %default]"),
  make_option("--x-col",                  type = "character", default = X_COL,
              help = "Effect-size column on log2 scale [default %default]"),
  make_option("--y-col",                  type = "character", default = Y_COL,
              help = "Significance column (p-value or padj) [default %default]"),
  make_option("--p-cutoff",               type = "double",    default = P_CUTOFF,
              help = "Horizontal significance cutoff [default %default]"),
  make_option("--fc-cutoff",              type = "double",    default = FC_CUTOFF,
              help = "Symmetric fold-change cutoff [default %default]"),
  make_option("--label-top-n",            type = "integer",   default = LABEL_TOP_N,
              help = "Number of top-significant genes to label [default %default]"),
  make_option("--title",                  type = "character", default = TITLE,
              help = "Plot title [default: filename stem]"),
  make_option("--subtitle",               type = "character", default = SUBTITLE,
              help = "Plot subtitle [default: DEG count caption]"),
  make_option("--width",                  type = "double",    default = WIDTH,
              help = "Output width in inches [default %default]"),
  make_option("--height",                 type = "double",    default = HEIGHT,
              help = "Output height in inches [default %default]"),
  make_option("--point-size",             type = "double",    default = POINT_SIZE,
              help = "Point size [default %default]"),
  make_option("--label-size",             type = "double",    default = LABEL_SIZE,
              help = "Gene-label font size [default %default]"),
  make_option("--draw-connectors",        type = "logical",   default = DRAW_CONNECTORS,
              help = "Draw label-to-point connectors [default %default]"),
  make_option("--max-overlaps",           type = "integer",   default = MAX_OVERLAPS,
              help = "ggrepel max.overlaps budget [default %default]"),
  make_option("--colour-up",              type = "character", default = COLOUR_UP,
              help = "Colour for significant up-regulated points [default %default]"),
  make_option("--colour-down",            type = "character", default = COLOUR_DOWN,
              help = "Colour for significant down-regulated points [default %default]"),
  make_option("--colour-ns",              type = "character", default = COLOUR_NS,
              help = "Colour for non-significant points [default %default]")
)

opt <- parse_args(OptionParser(option_list = option_list))

# Bind options back onto local names (optparse converts --foo-bar to foo_bar).
INPUT            <- opt$input
OUTPUT           <- opt$output
GENE_COL         <- opt$`gene-col`
X_COL            <- opt$`x-col`
Y_COL            <- opt$`y-col`
P_CUTOFF         <- opt$`p-cutoff`
FC_CUTOFF        <- opt$`fc-cutoff`
LABEL_TOP_N      <- opt$`label-top-n`
TITLE            <- opt$title
SUBTITLE         <- opt$subtitle
WIDTH            <- opt$width
HEIGHT           <- opt$height
POINT_SIZE       <- opt$`point-size`
LABEL_SIZE       <- opt$`label-size`
DRAW_CONNECTORS  <- opt$`draw-connectors`
MAX_OVERLAPS     <- opt$`max-overlaps`
COLOUR_UP        <- opt$`colour-up`
COLOUR_DOWN      <- opt$`colour-down`
COLOUR_NS        <- opt$`colour-ns`

if (is.null(INPUT) || is.null(OUTPUT)) {
  stop("--input and --output are required. Use --help for full usage.")
}
if (!file.exists(INPUT)) {
  stop(sprintf("Input file does not exist: %s", INPUT))
}

# ============================================================================
# LOAD INPUT — auto-detect by extension
# ============================================================================

read_de_table <- function(path) {
  ext <- tolower(tools::file_ext(path))
  if (ext == "csv") {
    read.csv(path, stringsAsFactors = FALSE, check.names = FALSE)
  } else if (ext %in% c("tsv", "txt", "tab")) {
    read.delim(path, stringsAsFactors = FALSE, check.names = FALSE)
  } else {
    # Fall back to whitespace/tab — read.delim is permissive.
    read.delim(path, stringsAsFactors = FALSE, check.names = FALSE)
  }
}

de <- read_de_table(INPUT)

missing_cols <- setdiff(c(GENE_COL, X_COL, Y_COL), colnames(de))
if (length(missing_cols) > 0) {
  stop(sprintf(
    "Input is missing required column(s): %s. Available columns: %s",
    paste(missing_cols, collapse = ", "),
    paste(colnames(de),  collapse = ", ")
  ))
}

# Drop rows with NA in the columns we plot.
de <- de[!is.na(de[[X_COL]]) & !is.na(de[[Y_COL]]) & !is.na(de[[GENE_COL]]), ,
         drop = FALSE]

x_vals <- de[[X_COL]]
y_vals <- de[[Y_COL]]
genes  <- as.character(de[[GENE_COL]])

# ============================================================================
# DEG COUNTS + LABEL SELECTION
# ============================================================================

is_sig    <- y_vals < P_CUTOFF
is_up     <- is_sig & x_vals  >  FC_CUTOFF
is_down   <- is_sig & x_vals  < -FC_CUTOFF
n_up      <- sum(is_up)
n_down    <- sum(is_down)
n_total   <- sum(is_up | is_down)
n_all     <- nrow(de)

# Top-N labels: most significant first.
top_n     <- min(LABEL_TOP_N, n_all)
top_idx   <- order(y_vals, decreasing = FALSE)[seq_len(top_n)]
top_genes <- genes[top_idx]

# ============================================================================
# DEFAULTS — title from filename, subtitle from DEG counts
# ============================================================================

if (is.null(TITLE) || !nzchar(TITLE)) {
  TITLE <- tools::file_path_sans_ext(basename(INPUT))
}
if (!nzchar(SUBTITLE)) {
  SUBTITLE <- sprintf("%d up, %d down (%d significant of %d)",
                      n_up, n_down, n_total, n_all)
}

# ============================================================================
# CUSTOM COLOUR VECTOR — match COLOUR_UP / DOWN / NS exactly
# ============================================================================

keyvals          <- rep(COLOUR_NS, n_all)
names(keyvals)   <- ifelse(is_up,   "Up-regulated",
                    ifelse(is_down, "Down-regulated", "n.s."))
keyvals[is_up]   <- COLOUR_UP
keyvals[is_down] <- COLOUR_DOWN

# ============================================================================
# BUILD PLOT
# ============================================================================

p <- EnhancedVolcano(
  de,
  lab             = genes,
  x               = X_COL,
  y               = Y_COL,
  selectLab       = top_genes,
  title           = TITLE,
  subtitle        = SUBTITLE,
  caption         = sprintf("|log2FC| > %.2f, %s < %g",
                            FC_CUTOFF, Y_COL, P_CUTOFF),
  pCutoff         = P_CUTOFF,
  FCcutoff        = FC_CUTOFF,
  pointSize       = POINT_SIZE,
  labSize         = LABEL_SIZE,
  colCustom       = keyvals,
  colAlpha        = 0.85,
  drawConnectors  = DRAW_CONNECTORS,
  widenConnectors = TRUE,
  max.overlaps    = MAX_OVERLAPS,
  legendPosition  = "right",
  legendLabSize   = 10,
  legendIconSize  = 3.0,
  xlab            = bquote(~Log[2]~ "fold change"),
  ylab            = bquote(~-Log[10]~ italic(.(Y_COL)))
)

# ============================================================================
# SAVE
# ============================================================================

dir.create(dirname(OUTPUT), recursive = TRUE, showWarnings = FALSE)

out_ext <- tolower(tools::file_ext(OUTPUT))
if (!(out_ext %in% c("pdf", "png"))) {
  stop(sprintf("Unsupported output extension: .%s — use .pdf or .png", out_ext))
}

ggsave(
  filename = OUTPUT,
  plot     = p,
  width    = WIDTH,
  height   = HEIGHT,
  units    = "in",
  dpi      = if (out_ext == "png") 300 else NA,
  device   = out_ext
)

message(sprintf(
  "Wrote %s: %d up, %d down, %d significant out of %d.",
  OUTPUT, n_up, n_down, n_total, n_all
))
