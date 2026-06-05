#!/usr/bin/env python3
"""
run_scvelo.py — turnkey RNA velocity pipeline.

Reads an AnnData with `spliced` / `unspliced` layers, runs filter_and_normalize
+ moments + velocity + velocity_graph (and optional dynamical fitting + latent
time + differential kinetics), writes the augmented .h5ad + streamline plots.

Usage:
    # Stochastic (default, fast)
    python run_scvelo.py --in adata.h5ad --cluster-col clusters --out velocity.h5ad

    # Dynamical (slower, more accurate)
    python run_scvelo.py --in adata.h5ad --cluster-col clusters \
        --mode dynamical --recover --latent-time --out velocity.h5ad

    # + differential kinetics on the top-likelihood genes
    python run_scvelo.py --in adata.h5ad --cluster-col clusters \
        --mode dynamical --recover --latent-time --diff-kinetics --out velocity.h5ad
"""

import argparse
import sys
from pathlib import Path


def main() -> None:
    p = argparse.ArgumentParser(description="Run scVelo RNA velocity analysis")
    p.add_argument("--in",          dest="input", required=True,
                   help="Input .h5ad with adata.layers['spliced'] and ['unspliced']")
    p.add_argument("--out",         required=True,
                   help="Output .h5ad")
    p.add_argument("--cluster-col", required=True,
                   help="adata.obs column with cluster labels (for grouping plots)")
    p.add_argument("--basis",       default="umap",
                   help="Embedding for projection [%(default)s]")
    p.add_argument("--mode",        default="stochastic",
                   choices=["deterministic", "stochastic", "dynamical"],
                   help="Velocity mode [%(default)s]")
    p.add_argument("--recover",     action="store_true",
                   help="Run recover_dynamics (required for --mode dynamical)")
    p.add_argument("--latent-time", action="store_true",
                   help="Compute latent_time (requires dynamical mode)")
    p.add_argument("--diff-kinetics", action="store_true",
                   help="Run differential_kinetic_test + re-velocity (requires dynamical)")
    p.add_argument("--n-top-genes", type=int, default=2000)
    p.add_argument("--min-shared",  type=int, default=20)
    p.add_argument("--n-pcs",       type=int, default=30)
    p.add_argument("--n-neighbors", type=int, default=30)
    p.add_argument("--n-jobs",      type=int, default=8)
    p.add_argument("--n-dk-genes",  type=int, default=50,
                   help="Top genes (by likelihood) to test for differential kinetics")
    p.add_argument("--fig-dir",     default="figures")
    args = p.parse_args()

    try:
        import scvelo as scv
        import scanpy as sc
    except ImportError as e:
        sys.exit(f"Missing dependency: {e}. Install: pip install -U scvelo")

    Path(args.fig_dir).mkdir(parents=True, exist_ok=True)
    scv.settings.figdir = args.fig_dir

    # ── 1. Load ─────────────────────────────────────────────────────────────
    print(f"Loading {args.input} …")
    adata = scv.read(args.input)
    if "spliced" not in adata.layers or "unspliced" not in adata.layers:
        sys.exit("Input AnnData is missing 'spliced' and/or 'unspliced' layers.\n"
                 "Run upstream with kallisto's --workflow nac, velocyto, or alevin-fry USA.")
    if args.cluster_col not in adata.obs.columns:
        sys.exit(f"--cluster-col '{args.cluster_col}' not in adata.obs")
    print(f"  Loaded: {adata.n_obs} cells × {adata.n_vars} genes")

    # ── 2. Spliced/unspliced proportions diagnostic ─────────────────────────
    try:
        scv.pl.proportions(adata, save="proportions.pdf")
    except Exception as e:
        print(f"  (proportions plot skipped: {e})")

    # ── 3. Preprocess ───────────────────────────────────────────────────────
    print("Preprocessing (filter_and_normalize + moments) …")
    scv.pp.filter_and_normalize(adata,
                                 min_shared_counts=args.min_shared,
                                 n_top_genes=args.n_top_genes)
    scv.pp.moments(adata, n_pcs=args.n_pcs, n_neighbors=args.n_neighbors)

    # ── 4. Dynamical mode prep ──────────────────────────────────────────────
    if args.mode == "dynamical" and not args.recover:
        print("WARNING: --mode dynamical requires --recover. Setting --recover automatically.")
        args.recover = True

    if args.recover:
        print(f"Recovering dynamics (n_jobs={args.n_jobs}) — this is the slow step …")
        scv.tl.recover_dynamics(adata, n_jobs=args.n_jobs)

    # ── 5. Velocity + graph ─────────────────────────────────────────────────
    print(f"Computing velocity (mode={args.mode}) + velocity_graph …")
    scv.tl.velocity(adata, mode=args.mode)
    scv.tl.velocity_graph(adata)

    # ── 6. Streamline plot ──────────────────────────────────────────────────
    print(f"Plotting streamlines on {args.basis} …")
    scv.pl.velocity_embedding_stream(adata, basis=args.basis,
                                      color=args.cluster_col,
                                      save="velocity_stream.pdf")
    scv.pl.velocity_embedding_grid(adata, basis=args.basis,
                                    color=args.cluster_col,
                                    arrow_length=3, arrow_size=2,
                                    save="velocity_grid.pdf")

    # ── 7. Latent time (dynamical only) ─────────────────────────────────────
    if args.latent_time:
        if args.mode != "dynamical":
            print("WARNING: --latent-time requires dynamical mode. Skipping.")
        else:
            print("Computing latent_time …")
            scv.tl.latent_time(adata)
            scv.pl.scatter(adata, color="latent_time", color_map="gnuplot",
                            save="latent_time.pdf")

    # ── 8. Driver genes ────────────────────────────────────────────────────
    if args.mode == "dynamical":
        print("Ranking dynamical driver genes …")
        scv.tl.rank_dynamical_genes(adata, groupby=args.cluster_col)
        df = scv.get_df(adata, 'rank_dynamical_genes/names')
        print("  Top driver genes per cluster:")
        print(df.head().to_string())
    else:
        print("Ranking velocity driver genes …")
        scv.tl.rank_velocity_genes(adata, groupby=args.cluster_col, min_corr=0.3)
        df = scv.DataFrame(adata.uns['rank_velocity_genes']['names'])
        print(df.head().to_string())

    # ── 9. Differential kinetics (optional) ────────────────────────────────
    if args.diff_kinetics:
        if args.mode != "dynamical":
            print("WARNING: --diff-kinetics requires dynamical mode. Skipping.")
        else:
            print(f"Differential kinetic test on top-{args.n_dk_genes} likelihood genes …")
            top_genes = adata.var['fit_likelihood'].sort_values(ascending=False).index[:args.n_dk_genes]
            scv.tl.differential_kinetic_test(
                adata, var_names=list(top_genes), groupby=args.cluster_col
            )
            print("Re-computing velocity with diff_kinetics=True …")
            scv.tl.velocity(adata, diff_kinetics=True)
            scv.tl.velocity_graph(adata)
            scv.pl.velocity_embedding_stream(adata, basis=args.basis,
                                              color=args.cluster_col,
                                              save="velocity_stream_diffkin.pdf")

    # ── 10. Confidence ──────────────────────────────────────────────────────
    try:
        scv.tl.velocity_confidence(adata)
        scv.pl.scatter(adata, c=['velocity_length', 'velocity_confidence'],
                        cmap='coolwarm', perc=[5, 95],
                        save='velocity_confidence.pdf')
    except Exception as e:
        print(f"(velocity_confidence skipped: {e})")

    # ── 11. Save ────────────────────────────────────────────────────────────
    print(f"Writing {args.out} …")
    adata.write(args.out)
    print("Done.")
    print(f"  Figures: {args.fig_dir}/")


if __name__ == "__main__":
    main()
