# SoloTE Usage Notes

## Input gotchas

SoloTE will run end-to-end on a BAM with no `CB` tags and emit an essentially empty matrix without raising an error. The template script in `assets/` greps the first 200 reads for `CB:Z:` and aborts if absent — replicate that check if you wrap SoloTE in your own pipeline. The canonical input is Cell Ranger's `possorted_genome_bam.bam`. STARsolo BAMs work as long as you ran with `--soloFeatures Gene` (or richer) so cell barcodes land in the `CB` tag.

The TE annotation BED's column 4 — `locus|Subfamily:Family:Class` — drives SoloTE's locus-vs-subfamily decision. Always regenerate it with `SoloTE_RepeatMasker_to_BED.py -g <build>` instead of editing a BED by hand or pulling one from a different pipeline. The locus IDs are what tie features back to genomic coordinates downstream.

## Output interpretation

Features come out in two flavors:

- **Locus-level TEs** — named `SoloTE|chr:start-end|Subfamily:Family:Class`. These are the rows you keep for locus-specific differential expression and for plotting genomic-context-aware UMAPs.
- **Subfamily-level TEs** — bare subfamily names like `L1HS` or `AluY`. SoloTE falls back to subfamily aggregation when a read can't be assigned to a single locus. For a fast first-pass cluster sanity check, subset to subfamily features only — they're far fewer and noisier per row, but they're enough to spot TE-driven cell-type signals before you commit to the heavier locus-level analysis.

Gene features keep whatever IDs Cell Ranger / STARsolo wrote — usually Ensembl IDs. Mixing namespaces in `features.tsv` is intentional: it lets a Seurat / Scanpy object treat TEs and genes as a unified feature space for normalization, integration, and DE.

## Downstream integration

```python
import scanpy as sc
adata = sc.read_mtx("sample1_SoloTE_output/matrix.mtx").T
adata.var_names = open("sample1_SoloTE_output/features.tsv").read().splitlines()
adata.obs_names = open("sample1_SoloTE_output/barcodes.tsv").read().splitlines()
adata.var["is_te"] = adata.var_names.str.startswith("SoloTE|")
```

```r
library(Seurat)
counts <- Read10X("sample1_SoloTE_output/")
seu <- CreateSeuratObject(counts)
seu[["RNA"]]@meta.features$is_te <- grepl("^SoloTE\\|", rownames(seu))
```

Tag the TE rows with an `is_te` flag at load time — every downstream filter, normalization, and DE call benefits from being able to split or stratify by it.

## Resource expectations

Per-sample runtime on a typical 10x library (~5–10k cells, ~50 GB BAM) is in the multi-hour range with `--threads 8`, dominated by the bedtools intersect and the read-by-read TE assignment. RAM peaks in the tens of GB. On HPC, point `--outputdir` at a shared filesystem (Lustre / GPFS / NFS); the node-local `/tmp` will leave intermediates orphaned between login and compute nodes.
