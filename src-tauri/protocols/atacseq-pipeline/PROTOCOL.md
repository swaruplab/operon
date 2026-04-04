# ATAC-seq Analysis Protocol

Standard pipeline for ATAC-seq data processing, peak calling, and differential accessibility analysis.

## Environment
- Tools: bowtie2, samtools, picard, macs2, bedtools, deeptools
- R packages: DiffBind, ChIPseeker, GenomicRanges, clusterProfiler
- Python: pysam, pyBigWig (optional for signal processing)

## Workflow Steps

1. **Quality Control**: FastQC on raw FASTQ files
2. **Adapter Trimming**: `trim_galore --paired -q 20 --nextera` (ATAC uses Nextera adapters)
3. **Alignment**: `bowtie2 -X 2000 --very-sensitive -x genome -1 R1.fq -2 R2.fq | samtools sort -o aligned.bam`
4. **Filtering**:
   - Remove duplicates: `picard MarkDuplicates REMOVE_DUPLICATES=true`
   - Remove mitochondrial reads: `samtools view -h file.bam | grep -v chrM | samtools sort -o filtered.bam`
   - Remove low MAPQ: `samtools view -q 30 -f 2 -F 1804`
5. **Shift Reads**: Tn5 inserts with +4/-5 offset — use `alignmentSieve --ATACshift` from deeptools
6. **Peak Calling**: `macs2 callpeak -t filtered.bam -f BAMPE -g hs --nomodel --shift -75 --extsize 150 -q 0.05`
7. **Signal Tracks**: `bamCoverage --normalizeUsing RPGC --effectiveGenomeSize 2913022398 --binSize 10 -o signal.bw`
8. **Peak Annotation**: ChIPseeker `annotatePeak()` for genomic feature distribution
9. **Differential Accessibility**: DiffBind with DESeq2 method
10. **Motif Analysis**: HOMER `findMotifsGenome.pl` on differential peaks

## SLURM Notes
- Alignment is the bottleneck — request 8+ cores for bowtie2
- Use `--mem=32G` for peak calling with large genomes
- Array jobs work well: one job per sample for steps 1-7

## Conventions
- Use BAMPE format for paired-end ATAC-seq in MACS2
- Always shift Tn5 insertion sites before peak calling
- Report FRiP (fraction of reads in peaks) as QC metric — target > 0.3
- Save bigWig files for IGV visualization
