#!/usr/bin/env python3
"""
run_mrvi.py — turnkey MrVI training + analysis.

Reads a multi-sample AnnData (raw counts), trains a MrVI model, writes the
trained weights + U latent + Z latent + cohort-mean sample-distance matrix.
Optional: --group-by + --group1 + --group2 to run differential abundance.

Usage:
    python run_mrvi.py --in cohort.h5ad --sample-key sample_id --out cohort_mrvi.h5ad

    # With sample-group comparison:
    python run_mrvi.py --in cohort.h5ad --sample-key sample_id \
        --group-col group --group1 Disease --group2 Control \
        --out cohort_mrvi.h5ad
"""

import argparse
import sys
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser(description="Train MrVI on multi-sample scRNA-seq")
    parser.add_argument("--in",       dest="input", required=True)
    parser.add_argument("--out",      required=True)
    parser.add_argument("--sample-key", required=True,
                        help="Column with biological sample identity (e.g. donor)")
    parser.add_argument("--batch-key", default=None,
                        help="Optional technical batch column (different from sample)")
    parser.add_argument("--labels-key", default=None,
                        help="Optional cell-type column")
    parser.add_argument("--n-latent",  type=int, default=20,
                        help="n_latent_u = n_latent_z [%(default)s]")
    parser.add_argument("--n-hidden",  type=int, default=128)
    parser.add_argument("--n-layers",  type=int, default=2)
    parser.add_argument("--epochs",    type=int, default=400)
    parser.add_argument("--n-hvg",     type=int, default=4000,
                        help="HVG count to keep before training [%(default)s]")
    parser.add_argument("--model-dir", default="models/mrvi")
    parser.add_argument("--device",    choices=["cpu", "gpu"], default="gpu")
    # Optional sample-group comparison
    parser.add_argument("--group-col", default=None,
                        help="Sample-level grouping column for DE/DA")
    parser.add_argument("--group1",    default=None)
    parser.add_argument("--group2",    default=None)
    parser.add_argument("--results-dir", default="results",
                        help="Where to write DE/DA tables")
    args = parser.parse_args()

    try:
        import scanpy as sc
        import scvi
        from scvi.external import MRVI
        import torch
        import numpy as np
    except ImportError as e:
        sys.exit(f"Missing dependency: {e}. Install: pip install scvi-tools")

    if args.device == "gpu" and not torch.cuda.is_available():
        print("WARNING: --device gpu but no CUDA. Falling back to CPU.")
        args.device = "cpu"
    accelerator = "gpu" if args.device == "gpu" else "cpu"

    Path(args.results_dir).mkdir(parents=True, exist_ok=True)

    # 1. Load + basic filter
    print(f"Loading {args.input} …")
    adata = sc.read_h5ad(args.input)
    print(f"  {adata.n_obs} cells × {adata.n_vars} genes")
    if args.sample_key not in adata.obs.columns:
        sys.exit(f"Sample column '{args.sample_key}' not in adata.obs")
    print(f"  Samples: {adata.obs[args.sample_key].nunique()}")

    sc.pp.filter_cells(adata, min_genes=200)
    sc.pp.filter_genes(adata, min_cells=3)

    # 2. HVG selection
    print(f"Selecting top {args.n_hvg} HVGs …")
    sc.pp.highly_variable_genes(
        adata,
        n_top_genes=args.n_hvg,
        flavor="seurat_v3",
        batch_key=args.sample_key,
    )
    adata = adata[:, adata.var["highly_variable"]].copy()

    # 3. Setup + train
    print("Running MRVI.setup_anndata …")
    setup_kwargs = {"sample_key": args.sample_key}
    if args.batch_key:
        setup_kwargs["batch_key"] = args.batch_key
    if args.labels_key:
        setup_kwargs["labels_key"] = args.labels_key
    MRVI.setup_anndata(adata, **setup_kwargs)

    print(f"Building model (n_latent={args.n_latent}) …")
    model = MRVI(
        adata,
        n_hidden=args.n_hidden,
        n_latent_u=args.n_latent,
        n_latent_z=args.n_latent,
        n_layers=args.n_layers,
    )

    print(f"Training on {accelerator.upper()} for ≤ {args.epochs} epochs …")
    model.train(
        max_epochs=args.epochs,
        accelerator=accelerator,
        devices=1,
        early_stopping=True,
    )

    Path(args.model_dir).parent.mkdir(parents=True, exist_ok=True)
    model.save(args.model_dir, save_anndata=False, overwrite=True)
    print(f"Model → {args.model_dir}")

    # 4. Latents
    print("Computing U + Z latents …")
    adata.obsm["U_mrvi"] = model.get_latent_representation(give_z=False)
    adata.obsm["Z_mrvi"] = model.get_latent_representation(give_z=True)

    # 5. UMAP + leiden on U
    print("UMAP + leiden on U …")
    sc.pp.neighbors(adata, use_rep="U_mrvi")
    sc.tl.umap(adata)
    sc.tl.leiden(adata, resolution=0.5)

    # 6. Cohort-mean sample-distance matrix
    print("Computing cohort-mean sample distances …")
    try:
        mean_dist = model.get_local_sample_distances(
            adata=adata, keep_cell=False
        )
        # Save as a numpy file alongside the AnnData
        np.save(
            Path(args.results_dir) / "sample_distance_matrix.npy",
            np.asarray(mean_dist),
        )
        print(f"  → {args.results_dir}/sample_distance_matrix.npy "
              f"({mean_dist.shape[0]} × {mean_dist.shape[1]})")
    except Exception as e:
        print(f"  WARNING: sample-distance computation failed: {e}")

    # 7. (Optional) DA + DE between sample groups
    if args.group_col and args.group1 and args.group2:
        if args.group_col not in adata.obs.columns:
            print(f"WARNING: --group-col '{args.group_col}' not in obs — skipping DA/DE")
        else:
            print(f"Running differential abundance ({args.group1} vs {args.group2}) …")
            try:
                da_df = model.differential_abundance(
                    adata=adata,
                    sample_cov_keys=[args.group_col],
                    group1=args.group1,
                    group2=args.group2,
                )
                da_path = Path(args.results_dir) / f"da_{args.group1}_vs_{args.group2}.tsv"
                if hasattr(da_df, "to_csv"):
                    da_df.to_csv(da_path, sep="\t")
                print(f"  DA → {da_path}")
            except Exception as e:
                print(f"  WARNING: DA failed: {e}")

            print("Running differential expression …")
            try:
                de_df = model.differential_expression(
                    adata=adata,
                    sample_cov_keys=[args.group_col],
                    group1=args.group1,
                    group2=args.group2,
                )
                de_path = Path(args.results_dir) / f"de_{args.group1}_vs_{args.group2}.tsv"
                if hasattr(de_df, "to_csv"):
                    de_df.to_csv(de_path, sep="\t")
                print(f"  DE → {de_path}")
            except Exception as e:
                print(f"  WARNING: DE failed: {e}")

    # 8. Save
    print(f"\nWriting {args.out} …")
    adata.write_h5ad(args.out)
    print("Done.")


if __name__ == "__main__":
    main()
