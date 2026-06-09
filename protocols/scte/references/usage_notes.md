# scTE — Usage Notes

## Picking the right barcode/UMI tags

scTE relies on per-read BAM tags to group reads by cell and deduplicate by UMI. The default flags assume STARsolo-style tags (`-CB CR -UMI UR`), but Cell Ranger writes its corrected tags as `CB:Z` and `UB:Z`. Passing the wrong pair does not error — scTE just iterates a BAM that has no matching tag and writes an empty matrix. Always run `samtools view inp.bam | head` first and confirm which tags are present before launching a long job. For barcode-less protocols (Smart-seq2, C1 Fluidigm) pass `-CB False -UMI False` so every read is treated as one cell defined by the BAM filename.

## Allocation modes

The `-m` flag determines how scTE resolves reads landing in regions where a TE annotation overlaps a gene exon/UTR. `exclusive` (default) assigns those reads only to the gene — this is the right choice when you want gene-level counts to match Cell Ranger. `inclusive` double-counts so that the TE locus also receives the read, which is useful when the biological question is TE expression from an exonized element. `nointron` excludes intronic reads entirely, mimicking poly-A-only quantification.

## Resource expectations

Memory scales roughly linearly with `-p`: budget ~10 GB per thread. A typical 10x dataset (~50 GB BAM, ~10k cells) finishes in 2–4 hours on 8 threads. Very large BAMs (>50 GB) benefit from sorting and indexing (`samtools sort`, `samtools index`) before running, but scTE itself does not require a `.bai`.

## Downstream integration

Pass `--hdf5 True` to get an AnnData `.h5ad` that loads directly with `scanpy.read_h5ad`. Genes and TE features share the `.var` index — filter by prefix (e.g. starts with `L1`, `Alu`, `ERV`) or by intersection with the bundled `TE_list_*.txt` to separate the two modalities. For Seurat, convert with `SeuratDisk::Convert("out.h5ad", dest = "h5seurat")` and then `LoadH5Seurat()`. CSV output is the same matrix layout but loses sparsity — only use it for small pilot runs.

## Custom genomes

If your species is not one of the bundled prebuilds (mm10, hg38, panTro6, macFas5, dm6, danRer11, xenTro9), build an index once with `scTE_build -te repeats.bed -gene genes.gtf -o myref -g myref`. The BED must be RepeatMasker-style with the TE name in column 4; the GTF must contain `gene_id` and `gene_name` attributes.
