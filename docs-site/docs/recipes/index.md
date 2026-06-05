# Recipes

End-to-end walkthroughs of real analyses, structured as guided prompts you
can use as templates. Each recipe assumes you've gone through the
[Quickstart](../quickstart.md) and have Operon connected to your data.

## Available recipes

<div class="grid cards" markdown>

-   :material-dna:{ .lg } [__PBMC scRNA-seq__](scrna-pbmc.md)

    QC → normalization → clustering → annotation. The "hello world" of
    single-cell. Uses Scanpy on the 10x PBMC 3k dataset.

-   :material-chart-line:{ .lg } [__Bulk RNA-seq + DESeq2__](bulk-rnaseq-deseq2.md)

    Count matrix → DEG tables → volcano + heatmap. The canonical bulk
    workflow. Uses DESeq2 in R, with PyDESeq2 alternative.

-   :material-map:{ .lg } [__Spatial Visium__](spatial-visium.md)

    Space Ranger output → spot clustering → niches → cell-type
    deconvolution. End-to-end spatial transcriptomics.

-   :material-dna:{ .lg } [__ATAC-seq peak calling__](atacseq.md)

    Trimming → alignment → MACS2 peaks → FRiP + TSS QC → motif
    enrichment. The full chromatin-accessibility pipeline.

-   :material-magnify:{ .lg } [__PubMed literature review__](pubmed-review.md)

    Question → curated citations → synthesized writeup. Using Ask mode
    + the PubMed MCP to do a real literature scoping exercise.

</div>

## How to use a recipe

Each recipe is structured as:

1. **What you'll build** — the end output
2. **Inputs** — what data you need
3. **Prompts** — copy-paste prompts for each step, designed to be used in
   the order shown
4. **Variations** — common variations (different organism, different tool,
   etc.)
5. **Pitfalls** — what to watch for

The prompts are written to work with any of the [four AI modes](../ai/modes.md);
the recipe will say which mode to use for each step.

## Contribute a recipe

If you've run a workflow in Operon that worked well and would help others,
PR a new recipe. See [Contributing](../contributing.md).

Good recipes are:

- **Concrete** — use a specific public dataset so anyone can follow
- **Honest** — include the "this didn't work the first time" beats
- **End-to-end** — from raw data to a publishable figure or table
