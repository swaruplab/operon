#!/usr/bin/env python3
"""
run_resolvi.py — turnkey ResolVI training + denoising.

Reads a spatial AnnData (with raw counts in .X and adata.obsm['spatial']),
builds the spatial neighbor graph (if not already present), trains a ResolVI
model, writes the trained weights + latent embedding + denoised counts.

Usage:
    python run_resolvi.py --in adata.h5ad --labels cell_type --out adata_resolvi.h5ad

Required:
    --in           Input AnnData .h5ad (raw counts in .X)
    --labels       Column in adata.obs with cell-type labels

Optional:
    --batch          Batch / sample column [default: none]
    --unlabeled      Label string for unlabeled cells [default: Unknown]
    --n-neighs       Spatial graph k [default: 20]
    --n-latent       Latent dimensions [default: 20]
    --epochs         Max epochs [default: 200]
    --model-dir      Where to save the trained model [default: models/resolvi]
    --device         cpu | gpu [default: gpu]
    --no-semisup     Disable semi-supervised mode (fully unsupervised)
"""

import argparse
import sys
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser(description="Train ResolVI on imaging-ST data")
    parser.add_argument("--in",         dest="input", required=True,
                        help="Input .h5ad (raw counts in .X, spatial coords in obsm)")
    parser.add_argument("--out",        required=True,
                        help="Output .h5ad with denoised layer + latent")
    parser.add_argument("--labels",     required=True,
                        help="adata.obs column with cell-type labels")
    parser.add_argument("--batch",      default=None,
                        help="Batch / sample column (optional)")
    parser.add_argument("--unlabeled",  default="Unknown",
                        help="String marking unlabeled cells [%(default)s]")
    parser.add_argument("--n-neighs",   type=int, default=20)
    parser.add_argument("--n-latent",   type=int, default=20)
    parser.add_argument("--n-hidden",   type=int, default=128)
    parser.add_argument("--n-layers",   type=int, default=2)
    parser.add_argument("--epochs",     type=int, default=200)
    parser.add_argument("--model-dir",  default="models/resolvi")
    parser.add_argument("--device",     choices=["cpu", "gpu"], default="gpu")
    parser.add_argument("--no-semisup", action="store_true",
                        help="Disable semi-supervised mode")
    args = parser.parse_args()

    try:
        import scanpy as sc
        import squidpy as sq
        import scvi
        from scvi.external import RESOLVI
        import torch
    except ImportError as e:
        sys.exit(f"Missing dependency: {e}. Install: pip install scvi-tools squidpy")

    if args.device == "gpu" and not torch.cuda.is_available():
        print("WARNING: --device gpu but no CUDA. Falling back to CPU (will be slow).")
        args.device = "cpu"
    accelerator = "gpu" if args.device == "gpu" else "cpu"

    print(f"Loading {args.input} …")
    adata = sc.read_h5ad(args.input)

    if "spatial" not in adata.obsm:
        sys.exit("Input is missing adata.obsm['spatial'] — required for ResolVI.")
    if args.labels not in adata.obs.columns:
        sys.exit(f"Labels column '{args.labels}' not in adata.obs")
    print(f"  Loaded: {adata.n_obs} cells × {adata.n_vars} genes")

    # Fill unlabeled NAs with the unlabeled-category marker
    label_col = adata.obs[args.labels].astype("object").fillna(args.unlabeled).astype("category")
    adata.obs["celltype_resolvi"] = label_col
    n_labeled = (adata.obs["celltype_resolvi"] != args.unlabeled).sum()
    print(f"  Labeled cells: {n_labeled:,} / {adata.n_obs:,}")

    # Build neighbor graph if not already present
    if "spatial_connectivities" not in adata.obsp:
        print(f"Building spatial neighbor graph (n_neighs={args.n_neighs}) …")
        sq.gr.spatial_neighbors(adata, coord_type="generic", n_neighs=args.n_neighs)

    # Setup
    print("Running RESOLVI.setup_anndata …")
    RESOLVI.setup_anndata(
        adata,
        labels_key=args.labels,
        batch_key=args.batch,
        unlabeled_category=args.unlabeled,
    )

    # Build + train
    print(f"Building model (n_latent={args.n_latent}, semisup={not args.no_semisup}) …")
    model = RESOLVI(
        adata,
        n_hidden=args.n_hidden,
        n_latent=args.n_latent,
        n_layers=args.n_layers,
        dropout_rate=0.1,
        semisupervised=not args.no_semisup,
    )

    print(f"Training on {accelerator.upper()} for up to {args.epochs} epochs …")
    model.train(
        max_epochs=args.epochs,
        accelerator=accelerator,
        devices=1,
        early_stopping=True,
    )

    # Save model
    Path(args.model_dir).parent.mkdir(parents=True, exist_ok=True)
    model.save(args.model_dir, save_anndata=False, overwrite=True)
    print(f"Model saved to {args.model_dir}")

    # Latent + denoised
    print("Computing latent representation + denoised expression …")
    adata.obsm["X_resolvi"] = model.get_latent_representation()

    try:
        denoised = model.get_normalized_expression(
            library_size=1e4, return_mean=True
        )
        # If it returns a DataFrame, convert to a layer
        import numpy as np
        if hasattr(denoised, "values"):
            adata.layers["resolvi_denoised"] = denoised.values.astype(np.float32)
        else:
            adata.layers["resolvi_denoised"] = denoised.astype(np.float32)
    except Exception as e:
        print(f"WARNING: get_normalized_expression failed: {e}")

    # Predictions (semi-supervised only)
    if not args.no_semisup:
        try:
            adata.obs["resolvi_predicted"] = model.predict()
            soft = model.predict(soft=True)
            adata.obs["resolvi_confidence"] = soft.max(axis=1)
        except Exception as e:
            print(f"WARNING: predict() failed: {e}")

    # Standard scanpy downstream
    print("Building UMAP + leiden from X_resolvi …")
    sc.pp.neighbors(adata, use_rep="X_resolvi")
    sc.tl.umap(adata)
    sc.tl.leiden(adata, resolution=0.5)

    print(f"Writing {args.out} …")
    adata.write_h5ad(args.out)
    print("Done.")


if __name__ == "__main__":
    main()
