# Analysis protocols

Protocols are reusable, domain-specific instructions that guide Claude's
behavior for particular types of analyses. Think of them as **expert
templates** — they encode best practices for tools, data formats, quality
control, and reproducibility.

![Protocols list](../img/protocols-list.png){ width=600 }

## What's in the box

Operon ships **665 protocols** across ~30 bio-first categories:

<div class="grid cards" markdown>

-   :material-dna: __Single-cell__

    Scanpy, Seurat, scVI, scvelo, doublet detection, cell-type annotation,
    CellChat communication

-   :material-map: __Spatial__

    Visium, Xenium, MERFISH, Slide-seq, cell2location, squidpy

-   :material-dna: __Chromatin__

    ATAC-seq, ChIP-seq, CUT&Tag, scATAC (ArchR), CUT&RUN, Hi-C, JASPAR motifs

-   :material-chart-line: __Bulk RNA-seq__

    DESeq2, edgeR, limma, STAR + Salmon, GSEA, PyDESeq2

-   :material-target: __CRISPR__

    MAGeCK, CRISPResso, screen analysis, base / prime editing

-   :material-microscope: __Imaging__

    CellPose, StarDist, DeepCell tissue segmentation, multiplexed IF

-   :material-account-group: __Population & variants__

    GWAS, eQTL, BWA + GATK, joint calling, structural variants

-   :material-cube: __Proteomics & structural__

    AlphaFold, ESM, PyMOL, MaxQuant, FragPipe, Biopython

-   :material-virus: __Microbiome__

    QIIME2, DADA2, shotgun metagenomics, mOTUs

-   :material-medical-bag: __Clinical & liquid biopsy__

    cfDNA, methylation, MRD, mutational signatures

-   :material-database: __Database agents__

    PubMed, GEO, KEGG, GTEx, UniProt, JASPAR, AlphaFold-DB, bioRxiv

-   :material-chart-bell-curve: __Visualization__

    EnhancedVolcano, UMAP, ComplexHeatmap, lab-branded matplotlib

</div>

## How a protocol works

When you select a protocol and start a session, its instructions are
injected into Claude's context. Claude then follows the protocol's
guidelines on:

- Which tools to use (and versions)
- Expected input formats
- QC checkpoints
- Output structure
- Reproducibility (conda env, container images, scheduler scripts)

A protocol is just a Markdown file with structured sections. You can read
any of them under `protocols/<category>/<name>/SKILL.md`.

## Pages in this section

- [Browsing the catalog](browse.md) — sidebar, search, filtering by category
- [Creating your own](custom.md) — AI-generated or manually written

## Don't see what you need?

Ask Claude. It can write any protocol on demand. See
[Creating your own](custom.md) for the workflow.
