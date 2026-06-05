#!/usr/bin/env python3
"""
snapatac_multi_sample.py — SnapATAC2 multi-sample integration pipeline.

Reads a sample list (one sample per line, TAB-separated: NAME<TAB>FRAGMENT_PATH),
runs per-sample QC, builds an AnnDataSet, applies batch correction (Harmony or
MNN-correct), and writes the integrated dataset + cluster-level peaks.

Usage:
    python snapatac_multi_sample.py \
        --samples samples.txt \
        --genome  hg38 \
        --batch-correct harmony \
        --out     combined.h5ads

samples.txt format (one per line, TAB-separated):
    ctrl_d1<TAB>/data/ctrl_d1_fragments.tsv.gz
    ctrl_d2<TAB>/data/ctrl_d2_fragments.tsv.gz
    dis_d1<TAB>/data/dis_d1_fragments.tsv.gz
    dis_d2<TAB>/data/dis_d2_fragments.tsv.gz
"""

import argparse
import os
import sys
from pathlib import Path


def parse_samples_file(path: str) -> list[tuple[str, str]]:
    """Parse the TAB-separated samples list. Returns [(name, fragment_path), ...]."""
    out: list[tuple[str, str]] = []
    with open(path) as f:
        for line_no, line in enumerate(f, 1):
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            parts = line.split("\t")
            if len(parts) != 2:
                sys.exit(f"{path}:{line_no}: expected 'NAME<TAB>PATH', got: {line!r}")
            name, frag = parts
            if not Path(frag).exists():
                sys.exit(f"{path}:{line_no}: fragment file not found: {frag}")
            out.append((name, frag))
    if not out:
        sys.exit(f"{path}: no samples found")
    return out


def main() -> None:
    parser = argparse.ArgumentParser(description="Multi-sample SnapATAC2 integration")
    parser.add_argument("--samples", required=True,
                        help="TAB-separated NAME<TAB>FRAGMENT_PATH per line")
    parser.add_argument("--out",     required=True,
                        help="Combined .h5ads output path")
    parser.add_argument("--genome", default="hg38",
                        choices=["hg38", "mm10", "hg19", "GRCh38", "mm9"])
    parser.add_argument("--batch-correct",
                        choices=["harmony", "mnn", "both", "none"],
                        default="harmony")
    parser.add_argument("--per-sample-dir", default="per_sample",
                        help="Directory for per-sample .h5ad files")
    parser.add_argument("--min-tsse",   type=float, default=7)
    parser.add_argument("--min-counts", type=int,   default=1000)
    parser.add_argument("--bin-size",   type=int,   default=5000)
    parser.add_argument("--n-features", type=int,   default=50000)
    parser.add_argument("--n-comps",    type=int,   default=50)
    parser.add_argument("--resolution", type=float, default=1.0)
    parser.add_argument("--skip-peaks", action="store_true")
    parser.add_argument("--fig-dir",    default="figures")
    args = parser.parse_args()

    try:
        import snapatac2 as snap
    except ImportError:
        sys.exit("Missing dependency: snapatac2. Install with: pip install snapatac2")

    os.makedirs(args.per_sample_dir, exist_ok=True)
    os.makedirs(args.fig_dir,        exist_ok=True)
    genome = getattr(snap.genome, args.genome)

    # ── 1. Parse the sample list ─────────────────────────────────────────────
    samples = parse_samples_file(args.samples)
    names = [n for n, _ in samples]
    paths = [p for _, p in samples]
    print(f"Found {len(samples)} samples: {', '.join(names)}")

    # ── 2. Import all fragment files ─────────────────────────────────────────
    print("\n[1/7] Importing all fragment files …")
    adatas = snap.pp.import_fragments(
        paths,
        file=[os.path.join(args.per_sample_dir, f"{n}.h5ad") for n in names],
        chrom_sizes=genome,
        min_num_fragments=args.min_counts,
    )

    # ── 3. Per-sample QC + feature processing ───────────────────────────────
    print("[2/7] Per-sample QC + tile matrix + features + doublets …")
    snap.metrics.tsse(adatas, genome)
    snap.pp.filter_cells(adatas, min_tsse=args.min_tsse, min_counts=args.min_counts)
    snap.pp.add_tile_matrix(adatas, bin_size=args.bin_size)
    snap.pp.select_features(adatas, n_features=args.n_features)
    snap.pp.scrublet(adatas)
    snap.pp.filter_doublets(adatas)
    for name, ad in zip(names, adatas):
        print(f"      {name}: {ad.n_obs} cells after QC + doublet filter")

    # ── 4. Combine into AnnDataSet ──────────────────────────────────────────
    print(f"[3/7] Building combined AnnDataSet → {args.out}")
    data = snap.AnnDataSet(
        adatas=list(zip(names, adatas)),
        filename=args.out,
    )
    print(f"      Combined: {data.n_obs} cells × {data.n_vars} features")

    # ── 5. Joint feature selection + spectral ───────────────────────────────
    print(f"[4/7] Joint feature selection + spectral ({args.n_comps} comps) …")
    snap.pp.select_features(data, n_features=args.n_features)
    snap.tl.spectral(data, n_comps=args.n_comps)

    # ── 6. Batch correction (one or both methods) ───────────────────────────
    use_reps = []  # which corrected embeddings to use downstream

    if args.batch_correct in ("harmony", "both"):
        print("[5/7] Harmony batch correction …")
        try:
            snap.pp.harmony(data, batch="sample", max_iter_harmony=20)
            use_reps.append("X_spectral_harmony")
        except Exception as e:
            print(f"      WARNING: harmony failed: {e}")

    if args.batch_correct in ("mnn", "both"):
        print("[5/7] MNN-correct batch correction …")
        try:
            snap.pp.mnc_correct(data, batch="sample")
            use_reps.append("X_spectral_mnn")
        except Exception as e:
            print(f"      WARNING: mnc_correct failed: {e}")

    if not use_reps:
        # No correction requested or all failed → use raw spectral
        print("[5/7] No batch correction — using raw X_spectral")
        use_reps.append("X_spectral")

    primary_rep = use_reps[0]
    print(f"      Primary embedding for downstream: {primary_rep}")

    # ── 7. UMAP + clustering on the corrected embedding ─────────────────────
    print(f"[6/7] UMAP + KNN + leiden (res={args.resolution}) on {primary_rep} …")
    snap.tl.umap(data, use_rep=primary_rep)
    snap.pp.knn(data, use_rep=primary_rep, n_neighbors=50)
    snap.tl.leiden(data, resolution=args.resolution)
    print(f"      Clusters found: {data.obs['leiden'].nunique()}")

    # Plot UMAPs colored by sample and by leiden
    try:
        snap.pl.umap(data, color="sample",
                      out_file=f"{args.fig_dir}/umap_sample.pdf", interactive=False)
        snap.pl.umap(data, color="leiden",
                      out_file=f"{args.fig_dir}/umap_leiden.pdf", interactive=False)
    except Exception as e:
        print(f"      (UMAP plots skipped: {e})")

    # ── 8. Per-cluster peak calling with sample replicates ──────────────────
    if not args.skip_peaks:
        print("[7/7] MACS3 peak calling (replicate=sample) …")
        try:
            snap.tl.macs3(data, groupby="leiden", replicate="sample")
            merged = snap.tl.merge_peaks(data.uns["macs3"], chrom_sizes=genome)
            print(f"      Merged peaks: {len(merged)}")
            # Write merged peaks for downstream use
            try:
                import polars as pl
                merged.write_csv(f"{args.fig_dir}/../merged_peaks.bed", separator="\t",
                                  include_header=False)
                print(f"      Peaks BED → {args.fig_dir}/../merged_peaks.bed")
            except Exception:
                pass
        except Exception as e:
            print(f"      WARNING: peak calling failed: {e}")
    else:
        print("[7/7] Skipping peak calling (--skip-peaks)")

    data.close()
    print(f"\nDone.")
    print(f"  Combined AnnDataSet: {args.out}")
    print(f"  Per-sample h5ads:    {args.per_sample_dir}/")
    print(f"  Figures:             {args.fig_dir}/")
    print(f"\nReopen later with:")
    print(f"  data = snap.read_dataset('{args.out}')")


if __name__ == "__main__":
    main()
