#!/usr/bin/env python3
"""
snapatac_standard.py — turnkey single-sample SnapATAC2 pipeline.

Takes a fragment file, runs the full standard pipeline (import → QC → tile
matrix → spectral → UMAP → leiden → MACS3 peaks → gene activity), writes an
annotated AnnData + peak matrix + gene activity matrix + diagnostic figures.

Usage:
    python snapatac_standard.py \
        --fragments /path/to/fragments.tsv.gz \
        --genome hg38 \
        --out sample.h5ad

Required:
    --fragments    Path to fragment file
    --out          Output AnnData (.h5ad)

Optional:
    --genome       hg38 | mm10 | hg19 | GRCh38 | mm9    [default hg38]
    --min-tsse     TSS-enrichment cutoff                [default 7]
    --min-counts   Min fragments per cell               [default 1000]
    --max-counts   Max fragments per cell (doublet cap) [default 100000]
    --bin-size     Tile matrix bin size                 [default 5000]
    --n-features   Number of selected bins              [default 50000]
    --n-comps      Spectral components                  [default 50]
    --n-neighbors  KNN neighbors                        [default 50]
    --resolution   Leiden resolution                    [default 1.0]
    --skip-peaks   Skip MACS3 peak calling              [flag]
    --skip-genes   Skip gene-activity matrix             [flag]
    --fig-dir      Where to write QC figures            [default figures]
"""

import argparse
import os
import sys
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser(description="Standard SnapATAC2 pipeline")
    parser.add_argument("--fragments", required=True, help="Fragment file path")
    parser.add_argument("--out",        required=True, help="Output .h5ad")
    parser.add_argument("--genome",     default="hg38",
                        choices=["hg38", "mm10", "hg19", "GRCh38", "mm9"])
    parser.add_argument("--min-tsse",     type=float, default=7)
    parser.add_argument("--min-counts",   type=int,   default=1000)
    parser.add_argument("--max-counts",   type=int,   default=100000)
    parser.add_argument("--bin-size",     type=int,   default=5000)
    parser.add_argument("--n-features",   type=int,   default=50000)
    parser.add_argument("--n-comps",      type=int,   default=50)
    parser.add_argument("--n-neighbors",  type=int,   default=50)
    parser.add_argument("--resolution",   type=float, default=1.0)
    parser.add_argument("--skip-peaks",   action="store_true")
    parser.add_argument("--skip-genes",   action="store_true")
    parser.add_argument("--fig-dir",      default="figures")
    parser.add_argument("--peak-out",     default=None,
                        help="Peak matrix h5ad output [default: derived from --out]")
    parser.add_argument("--gene-out",     default=None,
                        help="Gene activity h5ad output [default: derived from --out]")
    args = parser.parse_args()

    try:
        import snapatac2 as snap
    except ImportError:
        sys.exit("Missing dependency: snapatac2. Install with: pip install snapatac2")

    os.makedirs(args.fig_dir, exist_ok=True)
    out_stem = Path(args.out).stem
    peak_out = args.peak_out or str(Path(args.out).with_name(f"{out_stem}_peaks.h5ad"))
    gene_out = args.gene_out or str(Path(args.out).with_name(f"{out_stem}_gene_activity.h5ad"))

    genome = getattr(snap.genome, args.genome)

    # 1. Import fragments
    print(f"[1/8] Importing fragments from {args.fragments} (genome={args.genome}) …")
    data = snap.pp.import_fragments(
        args.fragments,
        chrom_sizes=genome,
        file=args.out,
        sorted_by_barcode=False,
        min_num_fragments=200,
    )
    print(f"      Imported: {data.n_obs} barcodes")

    # 2. TSS enrichment + QC plots
    print("[2/8] Computing TSS enrichment …")
    snap.metrics.tsse(data, genome)
    try:
        snap.pl.tsse(data, interactive=False, out_file=f"{args.fig_dir}/tsse.pdf")
        snap.pl.frag_size_distr(data, interactive=False,
                                 out_file=f"{args.fig_dir}/frag_size.pdf")
    except Exception as e:
        print(f"      (QC plots skipped: {e})")

    # 3. Filter cells
    print(f"[3/8] Filtering cells (min_tsse={args.min_tsse}, "
          f"min_counts={args.min_counts}, max_counts={args.max_counts}) …")
    snap.pp.filter_cells(
        data,
        min_tsse=args.min_tsse,
        min_counts=args.min_counts,
        max_counts=args.max_counts,
    )
    print(f"      After QC: {data.n_obs} cells")

    # 4. Tile matrix + feature selection
    print(f"[4/8] Building tile matrix (bin_size={args.bin_size}) and selecting features …")
    snap.pp.add_tile_matrix(data, bin_size=args.bin_size)
    snap.pp.select_features(data, n_features=args.n_features)

    # 5. Doublet detection
    print("[5/8] Detecting doublets …")
    snap.pp.scrublet(data)
    snap.pp.filter_doublets(data)
    print(f"      After doublet filter: {data.n_obs} cells")

    # 6. Spectral + UMAP + leiden
    print(f"[6/8] Spectral ({args.n_comps} comps), UMAP, leiden (res={args.resolution}) …")
    snap.tl.spectral(data, n_comps=args.n_comps)
    snap.pp.knn(data, use_rep="X_spectral", n_neighbors=args.n_neighbors)
    snap.tl.umap(data, use_rep="X_spectral")
    snap.tl.leiden(data, resolution=args.resolution)
    print(f"      Clusters found: {data.obs['leiden'].nunique()}")

    # 7. Peak calling
    if not args.skip_peaks:
        print("[7/8] MACS3 peak calling per leiden cluster …")
        try:
            snap.tl.macs3(data, groupby="leiden")
            merged = snap.tl.merge_peaks(data.uns["macs3"], chrom_sizes=genome)
            print(f"      Merged peaks: {len(merged)}")

            peak_mat = snap.pp.make_peak_matrix(data, use_rep=merged)
            peak_mat.write_h5ad(peak_out)
            print(f"      Peak matrix → {peak_out}")
        except Exception as e:
            print(f"      WARNING: peak calling failed: {e}")
    else:
        print("[7/8] Skipping peak calling (--skip-peaks)")

    # 8. Gene activity
    if not args.skip_genes:
        print("[8/8] Building gene activity matrix …")
        try:
            gene_mat = snap.pp.make_gene_matrix(data, gene_anno=genome)
            gene_mat.write_h5ad(gene_out)
            print(f"      Gene activity → {gene_out}")
        except Exception as e:
            print(f"      WARNING: gene activity failed: {e}")
    else:
        print("[8/8] Skipping gene activity (--skip-genes)")

    # Final UMAP plot
    try:
        snap.pl.umap(data, color="leiden",
                      out_file=f"{args.fig_dir}/umap_leiden.pdf",
                      interactive=False)
    except Exception:
        pass

    data.close()
    print(f"\nDone.")
    print(f"  ATAC AnnData:    {args.out}")
    if not args.skip_peaks:
        print(f"  Peak matrix:     {peak_out}")
    if not args.skip_genes:
        print(f"  Gene activity:   {gene_out}")
    print(f"  Figures:         {args.fig_dir}/")


if __name__ == "__main__":
    main()
