# Browsing the catalog

Click the **Protocols** icon in the activity bar to open the catalog.

![Protocols catalog](../img/protocols-list.png){ width=600 }

## Layout

The catalog is a two-pane view:

- **Left pane** — categories, with counts per category. Click to filter.
- **Right pane** — protocol cards in the active category, with title,
  one-line description, and tool stack.

## Search

The search box at the top filters across **all** categories (not just the
active one) by name, description, and tool stack.

Examples:

| Search | What you'll find |
|---|---|
| `deseq2` | All protocols that use DESeq2 |
| `seurat` | Seurat-based pipelines |
| `motif` | JASPAR, HOMER, motif enrichment |
| `slurm` | Protocols with SLURM-aware templates |

## Categories

The full taxonomy (with rough counts — exact numbers shift with each
release; v0.7.2 catalog is 665 protocols total):

| Category | What's in it |
|---|---|
| **single-cell** | Scanpy, Seurat, integration, annotation, velocity, CellChat |
| **spatial** | Visium, Xenium, MERFISH, deconvolution, neighborhood analysis |
| **chromatin** | ATAC, ChIP, CUT&Tag, CUT&RUN, scATAC, Hi-C |
| **rna** | Bulk RNA-seq, DESeq2, edgeR, GSEA, alignment, quantification |
| **crispr** | Screen analysis, MAGeCK, CRISPResso, base/prime editing |
| **cytometry** | Flow, CyTOF, spectral, gating |
| **epigenetics** | Methylation, hydroxymethylation, allele-specific |
| **immunology** | TCR/BCR-seq, MHC binding, neoantigen prediction |
| **microbiome** | 16S, shotgun, DADA2, QIIME2 |
| **liquid-biopsy** | cfDNA, MRD, methylation panels |
| **population** | GWAS, eQTL, polygenic scores, variant calling |
| **copy-number** | CNVkit, ASCAT, somatic CNVs |
| **genome-assembly** | de novo, hybrid, polishing |
| **phylogenetics** | RAxML, IQ-TREE, BEAST, time-trees |
| **sequence-io** | FASTA / FASTQ / BAM / VCF utilities |
| **proteomics** | MaxQuant, FragPipe, structural |
| **drug-discovery** | Docking, ADMET, ChEMBL |
| **metabolomics** | LC-MS, NMR, pathway mapping |
| **systems-biology** | Network inference, ODE models |
| **medical-imaging** | Radiomics, DICOM, segmentation |
| **clinical** | Cohort QC, survival analysis, biomarker workflows |
| **lab-automation** | Opentrons, Hamilton, plate scheduling |
| **databases** | PubMed / GEO / KEGG / GTEx / UniProt / JASPAR agents |
| **bio-agents** | Higher-level biology agents (literature, design, synthesis) |
| **ml-compute** | PyTorch, JAX, deep-learning training utilities |
| **statistics** | Stats helpers (mixed models, multiple testing) |
| **visualization** | Plot helpers (volcano, UMAP, heatmap, lab styles) |
| **writing** | Methods writeup, figure legends, manuscript drafts |
| **research** | Generic literature / hypothesis-generation agents |

## Selecting a protocol

Click a card to load that protocol into the current chat session. Its
instructions appear as a system message and influence every subsequent
Claude response.

You can:

- **Stack multiple protocols** (e.g. `scRNA-seq Seurat` + `Enhanced volcano`)
  — Claude reads both
- **Deselect** with the X on the chip
- **Preview the full text** by clicking the protocol's "View source" link

## Working with the protocol files directly

All protocols are Markdown files in `~/.operon/protocols/<category>/<name>/SKILL.md`.
You can:

- Edit them in any text editor (they reload on next selection)
- Copy them to make a customized variant
- Version-control them with Git for lab sharing

Bundled protocols live read-only inside the app; user-created ones live in
your home directory.
