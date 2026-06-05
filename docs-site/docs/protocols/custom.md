# Creating your own protocols

Two paths: let Claude write one for you, or write it yourself in Markdown.

## AI-generated (recommended for first-time authors)

1. Click the **+** button in the Protocols sidebar.
2. Pick **AI-generated**.
3. Describe what you need in plain English. The more detail, the better:

    > *"A protocol for 16S rRNA amplicon analysis using QIIME2 with DADA2
    > denoising. Should cover primer trimming, ASV inference, taxonomy
    > assignment with SILVA, alpha/beta diversity, and DESeq2-style
    > differential abundance. Include SLURM batch script template."*

4. Claude generates a full protocol with sections for environment setup,
   tool parameters, QC checkpoints, and reproducibility guidelines.
5. Review, edit any sections you want, and save.

![AI protocol generation](../img/protocol-generate.png){ width=500 }

The generated protocol is saved to `~/.operon/protocols/<category>/<name>/SKILL.md`.

## Manual editor

If you prefer to write your own — for example, because you already have a
lab SOP you want to formalize — pick **Manual** instead.

![Manual protocol editor](../img/protocol-manual.png){ width=500 }

You get a Markdown editor pre-populated with the recommended structure.

## Recommended structure

Every protocol benefits from these sections:

```markdown
---
name: my-protocol
description: One-line summary that shows in the catalog
category: single-cell
tools: [scanpy, anndata]
runtime: ~30min on 8 cores
---

# Protocol name

## When to use this

Plain-language description of what kind of analysis this is for and what
inputs it expects.

## Environment

- Python 3.11
- scanpy >= 1.10
- anndata >= 0.10
- See `requirements.txt` in this directory

## Inputs

- 10x Genomics filtered_feature_bc_matrix folder, or
- AnnData .h5ad file with raw counts in `.X`

## Outputs

- `<sample>.processed.h5ad` — fully processed AnnData
- `<sample>.umap.png` — UMAP overview plot
- `<sample>.markers.csv` — top markers per cluster

## Steps

1. **QC filtering** — filter cells with `n_genes_by_counts < 200` or
   `pct_counts_mt > 20`.
2. **Normalization** — `sc.pp.normalize_total` + `sc.pp.log1p`.
3. **HVG** — top 2000 highly variable genes.
4. **PCA** — 50 components.
5. **Neighbors + UMAP** — `n_neighbors=15`, default min_dist.
6. **Leiden clustering** — resolution 0.5 as default; tune per dataset.
7. **Marker genes** — `sc.tl.rank_genes_groups` Wilcoxon.

## QC checkpoints

- After filtering, report: n cells in, n cells out, median nUMI.
- After normalization, sanity check: row sums should be ~1e4.
- After clustering, expect ~10-30 clusters for a typical 5000-cell PBMC
  dataset.

## SLURM template (optional)

\`\`\`bash
#!/bin/bash
#SBATCH --mem=64G
#SBATCH --cpus-per-task=8
#SBATCH --time=02:00:00
...
\`\`\`

## References

- Scanpy: Wolf et al., 2018, Genome Biology — DOI: 10.1186/s13059-017-1382-0
```

## YAML frontmatter

The frontmatter (between the `---` markers at the top) drives the catalog
display:

| Field | What |
|---|---|
| `name` | Stable identifier (used in URLs, filenames) |
| `description` | One-liner shown in the catalog card |
| `category` | One of the 30 bio-first categories — see [Browse](browse.md) |
| `tools` | Array of tool names — drives the tool chips on the card |
| `runtime` | Free-text estimate — shown as a secondary detail |

If you omit `category`, the protocol shows up in "Other" — try not to.

## File location

User-created protocols live in:

| OS | Path |
|---|---|
| macOS / Linux | `~/.operon/protocols/<category>/<name>/SKILL.md` |
| Windows | `%USERPROFILE%\.operon\protocols\<category>\<name>\SKILL.md` |

Bundled (read-only) protocols live inside the app bundle and are loaded
automatically alongside user ones.

## Sharing protocols

Git-friendly by design:

```bash
cd ~/.operon/protocols
git init
git add .
git commit -m "Initial lab protocols"
git remote add origin git@github.com:mylab/operon-protocols.git
git push -u origin main
```

Colleagues clone into the same location and they're picked up on next
Operon launch.

## Editing existing user protocols

Open any user protocol's `SKILL.md` in the editor (right-click in catalog
→ Open source). Save with ++cmd+s++ / ++ctrl+s++. The next time you select
that protocol, the new version is used.

You can't edit bundled protocols in place — they're inside the read-only
app bundle. If you want to customize a bundled one:

1. Right-click → **Duplicate to user protocols**
2. Edit the copy under `~/.operon/protocols/...`
