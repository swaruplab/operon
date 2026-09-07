---
name: forge
display_name: FORGE — Flow Orchestrated Regulatory Genomics Engine
description: Install, validate, configure and run FORGE, the Swarup Lab Nextflow pipeline for end-to-end single-cell and single-nucleus multiome (RNA + ATAC) analysis, on a SLURM cluster with no root — covers the pinned Nextflow window and the NXF_VER trap, the 15-second pre-flight preview, the manifest CSV and dataset config (including the GTF params that must be set explicitly), the five Singularity containers and their bind mounts, minimal versus full (~600 GB) references, adapting slurm_* parameters and --gres syntax to a non-UCI cluster, submitting the Nextflow head job with sbatch, on-ramps, -resume, enabling expensive stages one at a time, and the failure modes that cost hours.
license: BSD-3-Clause
metadata:
---

# FORGE on a SLURM cluster

A practical guide to running FORGE (Flow Orchestrated Regulatory Genomics
Engine) on a shared SLURM cluster with **no root access**, driven from Operon
over SSH. Compiled 2026-09-06 against https://github.com/swaruplabUCI/FORGE
(README, `docs/`, `configs/`, `launch.sh`, `launch_tutorial.sh`,
`hpc_defs/BUILD_ON_HPC.sh`, `main.nf`, `nextflow.config`). Everything below
comes from those files; where the repository says something is "to be
provided" or unreleased, this guide says so rather than guessing.

Conventions: `$PROJECT` = the FORGE clone on **shared storage** (not
`$HOME`, see 3.1); `<account>`, `<partition>` = scheduler placeholders;
`/path/to/refs/` = wherever your references live; `my_study.config`,
`my_manifest.csv` = the two files you write.

## 1. What FORGE is and when to use it

FORGE is a Nextflow pipeline for end-to-end analysis of single-cell and
single-nucleus **multiome (RNA + ATAC)** data on HPC, from raw 10x (Cell
Ranger ARC) or BD Rhapsody output. Two files — a manifest CSV and a short
dataset config — drive ~108 processes in 13 workflow blocks: an **RNA arm**
(CellBender → QC → scVI → scANVI/CellTypist → MAST DE → hdWGCNA), an **ATAC
arm** (SnapATAC2 QC → peaks → scATAnno → Cicero CCANs → ChromVAR →
SCENIC+/pycisTopic → scPRINTER footprinting), **multiome integration**
(MOFA+ → MultiVI → MuData) and **communication/visualization** (CellChat →
enhancer footprinting recipes A/B/C → genome browser tracks). RNA-only or
ATAC-only runs are possible (`rna.run` / `atac.run`).

| Stage | Tool | Citation |
|---|---|---|
| Ambient RNA correction | CellBender | Fleming et al. 2023, *Nat Methods* |
| RNA QC, clustering | scanpy | Wolf et al. 2018, *Genome Biol* |
| RNA integration | scVI | Lopez et al. 2018, *Nat Methods* |
| RNA annotation | scANVI / CellTypist | Xu et al. 2021, *Mol Syst Biol*; Domínguez Conde et al. 2022, *Science* |
| ATAC QC, peaks, clustering, DA | SnapATAC2 | Zhang et al. 2024, *Nat Methods* |
| ATAC annotation | scATAnno | Jiang et al. 2025, *Genom Proteom Bioinform* |
| Co-accessibility | Cicero | Pliner et al. 2018, *Mol Cell* |
| TF motif enrichment | chromVAR | Schep et al. 2017, *Nat Methods* |
| GRN / topic models | SCENIC+ / pycisTopic | Bravo González-Blas et al. 2023, *Nat Methods* |
| TF footprinting | scPRINTER | Hu et al. 2025, *Nature* |
| Co-expression networks | hdWGCNA | Morabito et al. 2023, *Cell Rep Methods* |
| Cell–cell communication | CellChat | Jin et al. 2021, *Nat Commun* |
| Multi-modal integration | MOFA+ / MultiVI | Argelaguet et al. 2020, *Genome Biol*; Ashuach et al. 2023, *Nat Methods* |
| Differential expression | MAST | Finak et al. 2015, *Genome Biol* |

Footprinting scores against JASPAR 2022; motif enrichment uses CIS-BP.
**Validated genomes**: GRCh38/hg38 (Gencode v38), GRCm38/mm10 (Gencode vM10),
GRCm39/mm39 (Gencode vM37) — not a whitelist. **Shipped examples**:
`examples/nextflow_PBMC_Hs_10X.config` (public 10x *10k Human PBMCs, Multiome
v1.0, Chromium X*) and `examples/nextflow_AD_Mm_10X.config` (Alzheimer's
mouse, mm10; download link "to be provided"). BD Rhapsody configs in
`configs/datasets/` "require collaborator metadata not yet released".

## 2. Requirements

| Requirement | Value | Notes |
|---|---|---|
| Nextflow | **25.10.0** tested; supported `>=25.04.0, <26.0.0` | The README's "≥ 23.04" is stale: `nextflow.config` records `<= 24.10.5` fails to compile `main.nf`, `>= 26.04.6` fails to parse `nextflow.config`. |
| Java | 17 | `java -version`; if missing, `module load java/17`. |
| Singularity or Apptainer | ≥ 3.8 | On `$PATH` (`module load singularity` or `module load apptainer`). |
| SLURM | any | The only shipped scheduler profile. |
| GPU | **optional** | CPU-only end to end; see 2.1. |
| High-memory node | ≥ 256 GB RAM | SCENIC+ and large reference atlases. |
| Disk | ~600 GB references (full) + ~13–14 GB containers | A minimal RNA + ATAC run needs far less (§6). |

### 2.1 GPU tiers and the CPU-only reality

| Tier | Stages | Notes |
|---|---|---|
| **A30-class** (≥ 24 GB VRAM) | `TRAIN_SCVI`, `TRAIN_SCANVI` | The only genuine A30 requirement — full-atlas training exhausts smaller cards. |
| **V100-class** (16 GB VRAM) | `CELLBENDER`, `GPU_CHROMVAR`, `MULTIVI_*`, `MOFA_INTEGRATE` | Ample at these sizes. |
| **CPU-only** | everything else (~100 of ~108 processes) | No GPU code path at all. |

No stage *requires* a GPU: the tutorial completes all 94 tasks without one,
running CellBender, MOFA+ and MultiVI on CPU (`scvi_accelerator = 'cpu'`).
scVI/scANVI do **not** run in the tutorial — RNA annotation goes through
CellTypist, so `TRAIN_SCVI` and `TRAIN_SCANVI` appear nowhere in its
25-process graph — and their CPU training was verified separately by the
authors (`nextflow.config`'s `scvi_accelerator` note, 2026-08-06), not by the
tutorial. **ChromVAR is the exception**: `bin/gpu_chromvar_nf.py` imports
`cupy`/`rmm` at module scope with no CPU fallback (the tutorial sets
`chromvar { run = false }`). For local runs `docs/setup/cluster.md` says to
disable the GPU stages "rather than expecting a CPU fallback".

Disabling ChromVAR has consequences worth knowing before you plan around it.
`main.nf` gates the whole `ENHANCER_FOOTPRINTING_RECIPES` block on
`enhancer_footprinting.run && cicero.run && chromvar.run && scprinter.run`,
`differential_tf` on `chromvar.run`, and Cicero's ChromVAR-driven target
selection (`cicero.use_chromvar_targets`, default `true`) on the same flag.
Without a GPU you therefore cannot run TF motif enrichment, differential TF
accessibility, or any of §8.5 step 3 (multi-scale footprinting, strips,
TF–gene networks). Everything else — the RNA arm, ATAC QC/peaks/annotation,
Cicero, MOFA+/MultiVI, CellChat, hdWGCNA, pycisTopic/SCENIC+ — runs on CPU.

### 2.2 The `NXF_VER` trap

The `get.nextflow.io` installer always fetches the **newest** release, which
is outside the window. An out-of-window Nextflow aborts while parsing the
config with `Config parsing failed`, *before* FORGE's own version check can
print anything friendlier. `export NXF_VER=25.10.0` **before** installing is
the only real protection.

## 3. Install without sudo, then the 15-second pre-flight

### 3.1 Nextflow in a user directory, then clone

`$HOME` on most clusters is small and quota-limited; a full run will not fit,
so use a workspace on **shared storage visible from compute nodes** as
`$PROJECT`. No `sudo` is needed; many clusters have no `nextflow` module.

```bash
export NXF_VER=25.10.0                      # BEFORE the install; add to ~/.bashrc too
mkdir -p ~/bin && cd ~/bin
curl -s https://get.nextflow.io | bash      # writes ./nextflow here
export PATH="$HOME/bin:$PATH"               # add to ~/.bashrc to persist
nextflow -version                           # should report 25.10.0

cd /path/to/your/workspace                  # NOT ~/bin
git clone https://github.com/swaruplabUCI/FORGE.git
cd FORGE && export PROJECT="$PWD"
```

### 3.2 Prove the graph is sound (do this first, always)

FORGE ships a fixture (`test_data/`, eleven placeholder files of a few hundred
bytes each — the repository variously describes it as "7 KB" and "a few
hundred KB"; measured, it is ~1.6 KB of content) and
`configs/datasets/test_preview.config`, which enables every optional block.
No containers, references, GPU or cluster needed:

```bash
nextflow run main.nf -profile test -preview \
    -c configs/datasets/test_preview.config
```

Expected, in about fifteen seconds:

```text
WARN: Missing container files: [...]. Not required for -preview, but a real
      run needs them in singularity_cache/.

PRE-FLIGHT CHECKLIST PASSED (8 checks):
    [OK] Manifest schema (2 rows)
    [OK] Species/genome consistency
    [OK] GTF files (5 paths validated)
    [OK] scATAnno reference atlas (stub_atlas.h5ad)
    [OK] MOFA mode (high_memory)
    [OK] scPRINTER genome (hg38)
    [OK] CellTypist model: Immune_All_Low.pkl
    [OK] Resource tier (test)
  Warnings: 1 (see above)
```

**The container warning is expected** on a fresh clone: a preview launches no
task. For a real run the same check is an error (`Expected in
singularity_cache/. Run container build/pull first.`); with containers built
you see nine checks. **`-profile test`** strips every site-specific scheduler
assumption via `configs/resource_tiers/test.config` (1 CPU / 1 GB,
`clusterOptions = ''`), disables Singularity and redirects `outdir` to
`results_test/` — it is for `-preview` only, **never for a real analysis**.
`-preview` catches construction-time defects but nothing that only happens at
task runtime. Print the merged configuration at any time with `nextflow
config -profile cluster,singularity`.

## 4. The three core files

### 4.1 The manifest CSV (`params.metadata_file`)

The **only** file that describes your data. One row per sample; `rna_file`
and `fragment_file` are **filenames**, the directory goes in `data_dir`.

```csv
sample_id,batch,sample_type,original_lane_id,rna_file,fragment_file,condition_group,data_dir
my_sample,batch1,lane,L1,my_sample_raw_feature_bc_matrix.h5,my_sample_atac_fragments.tsv.gz,ConditionA,/data/my_study
```

| Column | Required | Meaning |
|---|---|---|
| `sample_id` | **yes** | Unique; joins RNA and ATAC; prefix on nearly every output. Blank rows silently skipped, duplicates a pre-flight error. |
| `sample_type` | **yes** | Write exactly `lane`, lowercase. Vestigial (not a sequencing lane). The validator accepts `lane` or `demux` — `demux` is a BD demux path that additionally requires `fragment_file` — and its message reads `Expected 'lane' or 'demux' (case-sensitive)`. `main.nf`'s routing layer (`normalizeSampleType`) does lowercase and accept `rna`/`atac` as aliases, but the validator still rejects them: do not rely on the aliases. Unrelated strings pass validation and then match no channel, silently dropping the row. |
| `batch` | yes in practice | Batch/group label (`docs/core/manifest.md`: "used for batch correction"); the lookup key into `params.batch_dirs` / `params.atac_batch_dirs` / `params.atac_coord_batch_dirs` when `data_dir` is absent, and carried in each sample's meta map. It is *not* the scVI batch axis — `TRAIN_SCVI` hard-codes `--batch_key sample`. |
| `rna_file` | for RNA runs | 10x `*_raw_feature_bc_matrix.h5`; BD `*_RSEC_MolsPerCell_MEX.zip`; or a MEX directory (`matrix.mtx.gz`, `barcodes.tsv.gz`, `features.tsv.gz`). |
| `fragment_file` | for ATAC runs | 10x `*_atac_fragments.tsv.gz`; BD `*_ATAC_Fragments.bed.gz`. No extension → `.bed.gz` appended. |
| `condition_group` | for differential | e.g. `WT`/`TG`. **Single-condition datasets still need one label on every row.** Empty warns and defaults to `Control`. |
| `data_dir` | see below | Absolute directory for the row's files; wins if present. Otherwise RNA rows resolve through `params.batch_dirs[batch]` and ATAC fragments through the separate `params.atac_batch_dirs[batch]`. |
| `original_lane_id` | optional | Lane subdirectory, only for batches listed in `params.batch_dirs_use_lane_subdir`. |
| `coord_data_dir` | ignored | Accepted by the manifest schema and documented in `docs/core/manifest.md`, but `resolveAtacCoordDir` has no caller anywhere in `main.nf` — the column has no effect. Leave it blank. |

Per-batch alternative (relocatable; the shipped test and tutorial manifests
use it) — set **both** maps, because RNA and ATAC resolve through different
ones: `params.batch_dirs = [june: '/data/june_run', july: '/data/july_run']`
and `params.atac_batch_dirs = [june: '/data/june_run', july:
'/data/july_run']` (`atac_batch_dirs` is not declared in `nextflow.config`, so
your config is the only place it comes from). A row with neither source stops
with `No directory configured for batch` (RNA) or `No ATAC directory
configured for batch 'june'. Set atac_batch_dirs.june in config.` (ATAC).
Pre-flight checks required columns, `sample_id` uniqueness, `rna_file`
existence, MEX completeness and config/manifest coherence; misspelled headers
are named (`did you mean: sample_ID?`) but **never remapped**. `fragment_file`
is *not* pre-flight checked; a bad one surfaces slightly later as `ATAC
fragment file not found`. (`docs/core/manifest.md` is authoritative;
`docs/setup/install.md` shows an older three-column example with full paths.)

### 4.2 The dataset config (`-c my_study.config`)

`nextflow.config` holds every default (~1,000 lines) and is the only place a
parameter is declared; you never edit it. Layers, later wins:
`nextflow.config → resource tier → -profile → -c my_study.config → --flag`.
`-c` **merges, it does not validate** — a misspelled key is silently ignored.
Start small, expensive stages off:

```groovy
params {
    species       = 'human'                  // or 'mouse' — required, no default
    metadata_file = '/path/to/my_manifest.csv'
    outdir        = 'results_my_study'
    resource_tier = 'small'                  // ALSO pass --resource_tier on the CLI (§8.2)

    // References (see §6)
    gtf_human_full = '/path/to/refs/gencode.v38.annotation.gtf'
    blacklist_bed  = '/path/to/refs/hg38-blacklist.v2.bed.gz'

    // These MUST be set explicitly, even though they repeat gtf_human_full
    // above, but for two different reasons. scprinter.gtf_human/gtf_mouse are
    // interpolated from gtf_*_full when nextflow.config is parsed, BEFORE your
    // dataset config merges, so they keep the OLD value (usually the literal
    // string 'null'). cicero.gtf_full / gtf_plot are plain null defaults with
    // no fallback to gtf_human_full at all. Pre-flight rejects every one of
    // them ("resolves to 'null'").
    cicero {
        gtf_full = '/path/to/refs/gencode.v38.annotation.gtf'
        gtf_plot = '/path/to/refs/gencode.v38.annotation.gtf'  // pre-flight requires it once cicero.target_genes is non-empty
        outdir   = "${params.outdir}/cicero"                   // re-declare AFTER outdir, as every shipped dataset config does
    }
    scprinter {
        gtf_human = '/path/to/refs/gencode.v38.annotation.gtf'
        gtf_mouse = '/path/to/refs/gencode.vM10.annotation.gtf'
    }

    // REQUIRED whenever atac.run = true: main.nf calls file() on this
    // unguarded, so null aborts with "Argument of `file()` function cannot
    // be null" -- and pre-flight does NOT catch it. Normally the same CSV.
    atac { sample_metadata = '/path/to/my_manifest.csv' }

    // Annotation. RNA uses CellTypist; ATAC requires a scATAnno atlas (or your
    // own atac.marker_file) — there is no atlas-free ATAC option.
    celltypist { model = 'Immune_All_Low.pkl' }
    scatanno   { reference_atlas = '/path/to/refs/scatanno_pbmc_atlas.h5ad' }

    // Leave the expensive arms off for the first pass. ChromVAR, scPRINTER,
    // enhancer footprinting and enhancer_viz all default to run = true and read
    // UCI-only paths (atac.cisbp_*, scprinter.cache_dir, scprinter.pfms all
    // default to /dfs7/swaruplab/lesolano/...) that pre-flight does NOT
    // existence-check (§6). Left on, the run passes -preview, spends hours on
    // CellBender/QC/integration, then dies in GPU_CHROMVAR /
    // SCPRINTER_BUILD_PRINTER on a missing file. Switch them off exactly as
    // configs/datasets/tutorial_pbmc.config does:
    scenicplus            { run = false }
    pycistopic            { run = false }
    dorc                  { run = false }
    chromvar              { run = false }   // GPU-only (cupy/rmm), no CPU fallback
    scprinter             { run = false }   // also wants the ~95 GB cache_dir + JASPAR pfms
    enhancer_footprinting { run = false; msfp_enabled = false }
    enhancer_viz          { run = false }
    cicero { use_chromvar_targets = false } // defaults true and needs ChromVAR output
    // When you later enable them, point at your own copies (the
    // configs/datasets/ad_mm_10x.config pattern):
    //   atac      { cisbp_human = '/path/to/refs/cisBP_2.00_human.meme' }
    //   scprinter { cache_dir = '/path/to/refs/scprinter'
    //               pfms      = '/path/to/refs/JASPAR2022_core_nonredundant.jaspar' }
}
```

The parse-time rule is also why `cicero.outdir` (re-declared in the block
above) would otherwise strand Cicero's output at `results/cicero`, and why
`pipeline_info/` lands under `results/` regardless (§9.2).

Worked configs live in `configs/datasets/`, but the published ones (and
`examples/*.config`) are UCI HPC3 instances carrying 15–32 `/dfs7/...` paths
each — read them for structure, never run them. `example_template.config` is
the portable starting point (no `/dfs7` paths at all), with two caveats: it
enables pycisTopic, SCENIC+, ChromVAR and scPRINTER with `null` reference
paths (turn them off for a first pass, as above), and its commented on-ramp
example lists `cistopic_obj_pkl` and `seurat_rds`, which pre-flight
**rejects** (§10).

Keys to revisit early. `celltypist.model` defaults to `Immune_All_Low.pkl`,
**wrong for most tissues** (mouse brain: `Mouse_Whole_Brain.pkl`; the
template also lists `Pan_Fetal_Human.pkl`, `Developing_Mouse_Brain.pkl`,
`Human_Lung_Atlas.pkl`, `Cells_Intestinal_Tract.pkl`,
`Healthy_COVID19_PBMC.pkl`). `atac.annotation_method` has one supported
value, `'scatanno'` (`'celltypist'` errors: `ATAC_CELLTYPIST on gene activity
has been removed`; `params.celltypist` governs RNA only); `atac.marker_file`
(the tutorial uses `configs/marker_genes.json`) overrides it and avoids the
atlas; `atac.tissue_type` is a free-form tissue label (the template shows
`'brain'`/`'pbmc'`, shipped configs also use `'kidney'`) — pre-flight only
uses it to *warn* when the scATAnno atlas filename or the CellTypist model
looks like a different tissue, and marker-based ATAC annotation receives it as
`--tissue_type`. `ref_dir_human_integrated` / `ref_dir_mouse_integrated` set to
an atlas directory enables GPU scANVI (Path A); `null` is CellTypist only
(Path B).

Only the footprint/figure **extras** default to off (`msfp_enabled`,
`msfp_strip`, `browser_viz`, `promoter_overlay`, `cis_rewiring`,
`shi_figures`, `cicero_per_ct`, `differential*`, `dorc`). The heavy analysis
blocks default **on** — `pycistopic.run`, `scenicplus.run`, `chromvar.run`,
`scprinter.run`, `enhancer_footprinting.run`, `enhancer_viz.run`,
`cicero.run`, `cellchat.run`, `hdwgcna.run`, `run_multiome_integration` — so a
first pass must switch them off explicitly (above). And **gates nest**:
`msfp_strip.enabled = true` does nothing unless
`enhancer_footprinting.msfp_enabled = true` (likewise `promoter_overlay`).
`shi_figures` is *not* nested under `msfp_enabled`: it has its own gates —
`shi_figures.enabled` for Tier A, plus `shi_figures.treatment`/`control` for
Tier B — and its bigwig tracks exist only when `enhancer_footprinting.run`,
`cicero.run`, `chromvar.run` and `scprinter.run` are all true.

### 4.3 `main.nf` and profiles

`main.nf` is one validation section (`validateStartupParams()`) plus thirteen
gated workflow blocks; you do not edit it to run your data. Entry points:
the default `workflow {}`, `-entry VIZ_ONLY`, `-entry SHI_FIGURES`.
Profiles are additive (comma-separated):

| Profile | Effect |
|---|---|
| `standard` | Local executor; parse checks and tiny tests. |
| `cluster` | SLURM via `configs/profiles/hpc3_cluster.config` (`errorStrategy = 'retry'`, `maxRetries = 2`, `submitRateLimit = '5 sec'`, `queueSize = 100`). |
| `gpu` | SLURM with GPU `clusterOptions` and `accelerator` directives; `launch.sh` uses `cluster,gpu,singularity` in production. |
| `singularity` / `docker` | Singularity/Apptainer with `--contain --home /tmp`, `/tmp`-redirected caches and the `/dfs7` bind (§5.2) — the profile string carries **no** `--nv`; that is added per GPU process by `containerOptions` in `nextflow.config`. Or Docker. |
| `test` / `tutorial` | Preview-only fixture (§3.2) / CPU-only local tier for the tutorial (§8.4). |

## 5. Containers

Five Singularity images. The **recipes** (`docs/defs/*.def`, ~40 KB) ship in
the repository; the built `.sif` images (1.8–4.7 GB each, ~14 GB total) are
gitignored, and pre-built downloads are **"[download link — to be
provided]"** in `docs/setup/install.md` — as of this writing, you build them.

| Image | Size | Role |
|---|---|---|
| `scgpu_extended.sif` | 3.7 G | Python/GPU: CellBender, scVI, scANVI, CellTypist, scrublet, MOFA+, muon |
| `snapatac_extended.sif` | 4.7 G | SnapATAC2, scATAnno, scPRINTER, MACS3, chromVAR (cupy/rmm), deeptools |
| `scenicplus.sif` | 1.8 G | SCENIC+, pycisTopic, pySCENIC, Mallet, graph-tool |
| `cicero.sif` | 1.9 G | R: Cicero (Monocle3), Bioconductor, rtracklayer, Gviz |
| `seurat_extended.sif` | 2.3 G (v3.6; `install.md`'s ~1.1 GB is the archived v3.4) | R: Seurat 5, hdWGCNA, CellChat, MAST, edgeR, WGCNA, zellkonverter |

### 5.1 Build on a compute node (not a login node)

Building needs root or `--fakeroot`.

```bash
srun -A <account> -p <partition> --time=04:00:00 --mem=24G --cpus-per-task=8 --pty bash
module load singularity          # or: module load apptainer
cd "$PROJECT"
bash hpc_defs/BUILD_ON_HPC.sh all                 # all five, in order
bash hpc_defs/BUILD_ON_HPC.sh seurat_extended     # one container; add --rebuild to force
bash hpc_defs/BUILD_ON_HPC.sh all --no-test       # skip %test blocks (faster, less safe)
```

`sbatch hpc_defs/BUILD_ON_HPC.sh all` also works (its `#SBATCH
--partition=free` is a UCI name — pass `-p <partition>`). The wrapper tries
`--fakeroot`, falls back to an unprivileged build (may fail on `apt-get`),
writes to `singularity_cache/` (override `FORGE_SIF_DIR=/some/path`), logs to
`singularity_cache/build_logs/`, skips existing images, and prints a size +
SHA256 table. Builds need network access to `ghcr.io`, `quay.io`, `docker.io`,
`cran.r-project.org`, `bioconductor.org`, `pypi.org`, `download.pytorch.org`,
`github.com`, `conda-forge`; observed 20–70 min per image on an Apple Silicon
Mac under Rosetta x86_64 emulation (`docs/setup/containers.md`) — native
x86_64 Linux builds are stated to be ~2–3× faster, so the 4 h `srun`
allocation above covers all five.

**No root or `--fakeroot`?** (1) Ask your administrators to run the build —
point them at `docs/defs/` and `hpc_defs/BUILD_ON_HPC.sh`. (2) Build where
you have privileges and copy the images over; *running* a `.sif` needs no
privileges, but the build host must match the cluster's CPU architecture
(`uname -m` on a compute node; in practice `x86-64` — a native `arm64` image
will not run). `docs/setup/containers.md` describes a `mac_build_containers.sh`
Lima-VM builder (Rosetta on Apple Silicon) writing to `./sif_output/`, but
**that script is not in the repository** as of this clone — do not tell a user
to run it. Off-cluster, build on any x86-64 Linux host where you have root or
`--fakeroot`, from the shipped recipes: `singularity build --fakeroot
<name>.sif docs/defs/<name>.def` (or `bash hpc_defs/BUILD_ON_HPC.sh all`),
then `scp` the images across.

### 5.2 Where the images live, and bind mounts

`nextflow.config` maps `scgpu`, `snapatac`, `scenicplus`, `r_cicero`,
`r_seurat`, `r_cellchat` to `${projectDir}/singularity_cache/*.sif`
(`r_seurat` and `r_cellchat` both use `seurat_extended.sif`); the directory
does not exist on a fresh clone (`mkdir -p singularity_cache`). If the images
live elsewhere, override the map in your dataset config — `params.containers
= [scgpu: '/shared/containers/scgpu_extended.sif', snapatac: ..., ...]`.

The `singularity` profile runs containers with `--contain --home /tmp --bind
/dfs7 --bind /tmp --bind /dev/shm` plus `--env` for the numba/matplotlib/
XDG/cupy caches under `/tmp`, `PYTHONNOUSERSITE=1` and
`HDF5_USE_FILE_LOCKING=FALSE`. `--contain` suppresses implicit mounts and
admin bind paths; `/dfs7` is the UCI HPC3 lab share. **Bind your own data and
reference filesystems** or the pipeline cannot see input or write output.

`docs/setup/cluster.md` and `docs/troubleshooting.md` show this as
`singularity.runOptions = '--nv -B /data -B /refs -B /scratch'`. **Do not copy
that literally.** `runOptions` is a single string, so assigning it *replaces*
the whole shipped default — dropping `--contain`, `--home /tmp`, `--bind
/tmp`, `--bind /dev/shm` and every `--env` cache redirection, i.e. exactly the
isolation the next paragraph tells you to keep (and `--nv` was never in it).
Two safe routes:

```groovy
// (a) Extend, don't replace: copy the shipped string and swap /dfs7 for your
//     filesystems, keeping everything else verbatim.
singularity.runOptions = "--contain --home /tmp --bind /data --bind /refs --bind /scratch --bind /tmp --bind /dev/shm --env NUMBA_CACHE_DIR=/tmp/numba_cache --env MPLCONFIGDIR=/tmp/matplotlib --env XDG_CACHE_HOME=/tmp/cache --env CUPY_CACHE_DIR=/tmp/cupy_cache --env PYTHONNOUSERSITE=1 --env HDF5_USE_FILE_LOCKING=FALSE"
```

```bash
# (b) Leave runOptions alone and add binds additively in the head-job script,
#     which is what both launchers do (launch.sh uses "/dfs7,/tmp"):
export SINGULARITY_BINDPATH="/data,/refs,/scratch,/tmp"
```

Keep `/tmp` bound (caches live there; `--home /tmp` keeps R/Python away from
your cluster home); budget ≥ 20 GB scratch per parallel task on large runs.
R processes add `--env R_LIBS_USER=/dev/null`, GPU processes `--nv`. Runtime
network: the sources disagree about CellTypist and you should assume the
pessimistic one. `docs/setup/containers.md` says the models are baked into
`scgpu_extended.sif` at build time "so the container works offline"; but the
containers run with `--home /tmp`, and the *measured* tutorial run fetched
`Immune_All_Low.pkl` (2.8 MB) at runtime from `celltypist.cog.sanger.ac.uk`
into `/tmp/.celltypist` on the executing node — `docs/tutorial.md` lists
network as **required**. scPRINTER dispersion models likewise download on
first use into `$XDG_CACHE_HOME` (`/tmp/cache`). On isolated nodes,
pre-populate a directory, bind it in, and point `CELLTYPIST_FOLDER` at it — or
set `params.celltypist.model` to the full path of a local `.pkl`, which
`bin/run_cell_typist.py` loads without any download (`docs/setup/references.md`
gives this as the route for models not bundled in the image). Smoke tests, sizes and SHA256 hashes: `docs/setup/containers.md`.

## 6. References

**Minimal versus full.** The full set is ~600 GB (both species, Allen and
SEA-AD atlases, two scPRINTER caches); ~360 GB is the figure *without SEA-AD
and with only one species' scPRINTER cache* — both species' GTFs, FASTAs and
cisTarget feathers and the 145.7 GB Allen mouse atlas are still inside it. A
**minimal RNA + ATAC run** needs only a GTF, a CellTypist model (baked into
`scgpu_extended.sif`, though `RUN_CELLTYPIST` re-fetches it at runtime — §5.2)
and a scATAnno atlas or `atac.marker_file` — **provided** `chromvar.run`,
`scprinter.run`, `enhancer_footprinting.run`, `enhancer_viz.run`,
`pycistopic.run` and `scenicplus.run` are all `false`, as
`configs/datasets/tutorial_pbmc.config` sets them. With the shipped defaults
(all `true`) the same run also needs `atac.cisbp_human`/`cisbp_mouse`,
`scprinter.pfms`, the ~95 GB `scprinter.cache_dir`, `pycistopic.gtf` +
`pycistopic.blacklist_bed`, the three cisTarget files and a GPU for ChromVAR.
Download in a batch job onto shared storage, not on a login node.

**Human PBMC (hg38)**

| Reference | Size | Source |
|---|---|---|
| `gencode.v38.annotation.gtf` | 1.46 GB | gencodegenes.org human release 38 |
| `hg38-blacklist.v2.bed.gz` | small | github.com/Boyle-Lab/Blacklist |
| `cisBP_2.00_human.meme` | small | cisBP v2.00; also distributed with scATAnno |
| `PBMC_reference_atlas_final.h5ad` (scATAnno) | 2.76 GB | **custom-built** by the Swarup Lab; build scripts live on UCI HPC3, "a user-facing build tutorial will be added in a future update" |
| `hg38_screen_v10_clust.regions_vs_motifs.rankings.feather` | 35.2 GB | resources.aertslab.org/cistarget |
| `hg38_screen_v10_clust.regions_vs_motifs.scores.feather` | 13.9 GB | resources.aertslab.org/cistarget |
| `motifs-v10nr_clust-nr.hgnc-m0.001-o0.0.tbl` | 98.7 MB | resources.aertslab.org/cistarget |
| `JASPAR2022_core_nonredundant.jaspar` | small | jaspar.elixir.no |
| scPRINTER cache (hg38) | ~95 GB | auto-populated on first run, or manual download |
| `hg38.fa` + `.fai` (SCENIC+ `fai`) | ~3 GB | Gencode / UCSC; mouse: `GRCm38.primary_assembly.genome.fa` + `.fai`, 2.77 GB |

**Mouse (mm10)**

| Reference | Size | Source |
|---|---|---|
| `gencode.vM10.annotation.gtf` | 802 MB | gencodegenes.org mouse release M10 |
| `mm10-blacklist.v2.bed.gz` | small | github.com/Boyle-Lab/Blacklist |
| `cisBP_2.00_mouse.meme` | small | cisBP v2.00 |
| `mouse_brain_reference_atlas.h5ad` (scATAnno) | 1.96 GB | **custom-built** from GEO GSE246791 (126 GB source tar) |
| `AllenRef_mouse10xv2.h5ad` | 145.7 GB | Allen Brain Cell Atlas; scANVI Path A only; subsampled to 50,000 cells at runtime by `PREPARE_REFERENCE` |
| `mm10_screen_v10_clust.regions_vs_motifs.rankings.feather` | 17.8 GB | resources.aertslab.org/cistarget |
| `mm10_screen_v10_clust.regions_vs_motifs.scores.feather` | 8.2 GB | resources.aertslab.org/cistarget |
| `motifs-v10nr_clust-nr.mgi-m0.001-o0.0.tbl` | 113.1 MB | resources.aertslab.org/cistarget |
| scPRINTER cache (mm10) | ~95 GB | as above |

**Mouse (mm39)**: Gencode vM37. There is **no upstream mm39 cisTarget
database**; the lab derived one by UCSC liftOver from mm10
(`docs/setup/cistarget_mm39_liftover.md`: ~5 CPU-hours, 256 GB RAM) — the mm10
database against mm39 peaks makes SCENIC+ "silently return near-empty motif
enrichment". **The liftover page's wiring snippet is wrong**: it sets
`scenicplus { rankings_db / scores_db / motif_annot }`, keys that do not exist
in `nextflow.config`, and `-c` silently ignores unknown keys — so SCENIC+ runs
against null databases. Point `scenicplus.ctx_rankings` and
`scenicplus.ctx_scores` at `mm39_region_based.rankings.feather` /
`mm39_region_based.scores.feather`, keep `scenicplus.motif_annotations` on the
unchanged MGI table `motifs-v10nr_clust-nr.mgi-m0.001-o0.0.tbl` (not
coordinate-based, no liftOver needed), and confirm with `nextflow -c
my_study.config config | grep -A6 scenicplus`. Human brain can use SEA-AD
(`SEAAD_MTG_RNAseq_final-nuclei.2024-02-13.h5ad`, 36.3 GB) for Path A.
Verbatim download commands are in `docs/setup/references.md`.

| `params` key | Reference | Tool |
|---|---|---|
| `gtf_human_full` / `gtf_mouse_full`, re-declared as `cicero.gtf_full` (`gtf_plot`), `scprinter.gtf_human`/`gtf_mouse`, `pycistopic.gtf`, `scenicplus.gtf` | Gencode GTF | Cicero, SCENIC+, scPRINTER, pycisTopic |
| `blacklist_bed` | ENCODE blacklist | Nothing reads it — pre-flight uses it for a species build-string check only |
| `pycistopic.blacklist_bed` | ENCODE blacklist | pycisTopic — set it explicitly; it does **not** inherit `blacklist_bed` |
| `scatanno.reference_atlas` | scATAnno `.h5ad` | scATAnno |
| `atac.cisbp_human` / `atac.cisbp_mouse` | cisBP `.meme` | ChromVAR |
| `scenicplus.ctx_rankings`, `ctx_scores`, `motif_annotations`, `fai` | cisTarget feathers, `.tbl`, `.fa.fai` | SCENIC+ (all three cisTarget files required) |
| `scprinter.cache_dir`, `scprinter.pfms`, `scprinter.genome` (`'hg38'`/`'mm10'` in the template; pre-flight accepts `hg38\|hg19\|grch38\|grch37` for human and `mm10\|mm39\|grcm38\|grcm39` for mouse, and errors only on a species mismatch) | cache dir, JASPAR | scPRINTER |
| `ref_dir_human_integrated` / `ref_dir_mouse_integrated` | SEA-AD / Allen directory | scANVI Path A |

**Not every path is checked.** Pre-flight existence-checks the manifest and
each lane row's `rna_file`, the GTFs actually in play (`scprinter.gtf_<species>`
whenever scPRINTER / enhancer footprinting / enhancer_viz / ChromVAR run;
`cicero.gtf_full`, plus `cicero.gtf_plot` when `cicero.target_genes` is set;
`pycistopic.gtf`; `scenicplus.gtf` if set), `scatanno.reference_atlas`,
`ref_dir_*_integrated` (which must contain `.h5ad` files) and the container
`.sif`s — each miss is one numbered error. It does **not** existence-check
`blacklist_bed` or `pycistopic.blacklist_bed` (species build-string sniff
only), `atac.cisbp_human`/`cisbp_mouse`, `scprinter.pfms`,
`scprinter.cache_dir`, or `scenicplus.ctx_rankings`/`ctx_scores`/
`motif_annotations`/`fai`. Worse, the cisTarget species sniff reads
`params.scenicplus.cistarget_rankings` — a key that does not exist (the real
one is `ctx_rankings`) — so it can never fire. A wrong or `/dfs7` path in any
of those passes `-preview` and fails hours later inside `GPU_CHROMVAR`,
`SCPRINTER_*`, `ATAC_FINAL_PIPELINE` or `SCENICPLUS_RUN`. Verify them yourself
with `ls -l` from a compute node before submitting.

## 7. Adapting to a non-UCI cluster

Only scheduler details are site-specific: `configs/profiles/hpc3_cluster.config`,
`configs/resource_tiers/{small,medium,large}.config` and the `profiles` block
of `nextflow.config` (find them with `grep -rn
"slurm_account\|slurm_partition\|slurm_qos\|gres=gpu" nextflow.config configs/`).
The identifiers are parameters, so redefine them in your dataset config:

```groovy
params {
    slurm_account                = 'my_lab'
    slurm_partition_cpu          = 'compute'
    slurm_account_gpu            = 'my_lab_gpu'
    slurm_partition_gpu          = 'gpu'
    slurm_partition_gpu_hugemem  = 'gpu-bigmem'
    slurm_qos_gpu_hugemem        = 'normal'
    slurm_gpu_type               = 'a100'   // must match your --gres names
    slurm_gpu_count              = 1
}
```

Defaults are UCI HPC3 values. Confirm with
`nextflow -c my_study.config config -profile cluster,singularity | grep slurm`.
GPUs are requested with an explicit string, `clusterOptions = "-A
${params.slurm_account_gpu} -p ${params.slurm_partition_gpu}
--gres=gpu:${params.slurm_gpu_type}:${params.slurm_gpu_count}"`; different
type names, or no type qualifier, get the submission rejected (`Invalid
partition`, `Invalid account`, or `sbatch: error:` with no `.command.log`).
Many literals are **not** parameterised, and no `slurm_*` value reaches them:

- `configs/profiles/hpc3_cluster.config` — the `hugemem` label hard-codes `-p
  highmem` (500 GB).
- `small.config` — `-p highmem` (ENHANCER_FOOTPRINTING, CROSS_MODAL_VALIDATION,
  SIGNAL_CHAIN_CORRELATION), `-p hugemem` (SCENICPLUS_RUN),
  `--gres=gpu:V100:1` (MULTIVI_INTEGRATE, MULTIVI_VISUALIZE,
  MULTIVI_DRIVER_FACTORS), `--gres=gpu:A30:1` (MULTIVI_MASKING_SWEEP_ONE) and
  one typeless `--gres=gpu:1`.
- `medium.config` — additionally the full UCI string `-p gpu-hugemem
  --qos=gpu-hugemem-vswarup --gres=gpu:A30:1` (MOFA_INTEGRATE,
  MULTIVI_INTEGRATE, MULTIVI_GAP_FILL), which bypasses
  `slurm_partition_gpu_hugemem`/`slurm_qos_gpu_hugemem` entirely, plus `-p
  highmem`/`-p hugemem` in eight more blocks.
- `large.config` — `-p hugemem`/`-p highmem` in twelve blocks, `-p maxmem`,
  and `--gres=gpu:A100:${params.slurm_gpu_count}` for GPU_CHROMVAR.

A `grep gres=gpu:` finds only the last kind, so sweep for all of them:

```bash
grep -nE '\-p [a-z]|--qos=[a-z]|gres=gpu:[A-Za-z0-9]' \
    configs/resource_tiers/*.config configs/profiles/*.config
```

Override each affected process in your dataset config (`process { withName:
'MULTIVI_INTEGRATE' { clusterOptions = "-A ${params.slurm_account} -p
<your-partition>" } }`). Note that MULTIVI_INTEGRATE pins `V100` even on the
default `small` tier, so a first minimal run with `run_multiome_integration =
true` (the default) is rejected by `sbatch` on any cluster without that GRES
name — after the RNA and ATAC arms have already run.

**Resource tiers** (`params.resource_tier`, lowercase and strict — `Medium`
is a pre-flight error):

| Tier | Intended scale |
|---|---|
| `small` / `auto` | ≤ 20k cells, 1–5 samples. Default. |
| `medium` | 20k–100k cells, 6–50 samples; > 250 GB spills to highmem, > 450 GB to hugemem. |
| `large` | > 100k cells, 50+ samples; hugemem nodes required — requests up to 2200 GB (`MOFA_INTEGRATE\|MULTIVI_INTEGRATE`) and 1400 GB on `-p maxmem`. |

`test.config`'s "the production tiers request up to 1200 GB" is stale and
understates every tier. Even the default `small` asks 1100 GB for
`TRAIN_SCVI|TRAIN_SCANVI|PREPARE_REFERENCE` (Path A only) and 2200 GB for
`MULTIVI_MASKING_SWEEP_ONE` (off by default); `medium` adds 1200 GB for
`MULTIVI_GAP_FILL`. Check before you size a partition: `grep -n "memory"
configs/resource_tiers/<tier>.config`.

Values are deliberately generous because HPC3 refunds unused walltime — **if
your site bills reserved resources they will overspend**; right-size from the
execution report (§9.2) with a per-process override in your dataset config
(`process { withName: 'SCENICPLUS_RUN' { memory = '512.GB' } }`), not a new
`withName:` block in a tier file (which busts `-resume` for every process
below it).

**Non-SLURM schedulers**: add a profile with `process.executor = 'sge'` (or
`'lsf'`, `'pbs'`, `'awsbatch'`, `'k8s'`) and `process.queue`, run with
`-profile my_cluster,singularity`, and replace or drop the tiers' SLURM-syntax
`clusterOptions` in favour of `cpus`/`memory`/`time`. **No scheduler**:
`nextflow run main.nf -profile standard,singularity -c my_study.config`
ignores partitions and QOS; disable GPU stages (§2.1).

**Clusters with no GPUs at all.** The production tiers submit `CELLBENDER`,
`TRAIN_SCVI`, `TRAIN_SCANVI`, `MOFA_INTEGRATE`, `MULTIVI_INTEGRATE` and
`GPU_CHROMVAR` with `containerOptions = '--nv'`, `accelerator = 1` and a
`--gres=gpu:...` `clusterOptions` string, so they go to a GPU partition
whatever your `slurm_*` values say. The only shipped CPU-only tier is
`tutorial` (local executor, sized for 1,000 cells). To run these on CPU, set
`scvi_accelerator = 'cpu'` and `chromvar { run = false }` in the dataset
config and strip the GPU directives the way
`configs/resource_tiers/tutorial.config` does:

```groovy
process {
    withName: '(.*:)?(TRAIN_SCVI|TRAIN_SCANVI|CELLBENDER|MOFA_INTEGRATE|MULTIVI_INTEGRATE)' {
        accelerator      = null
        containerOptions = ''
        clusterOptions   = "-A ${params.slurm_account} -p ${params.slurm_partition_cpu}"
    }
}
```

Expect CellBender and MultiVI to be much slower on CPU; the tutorial's
measured times (§8.4) are the only published reference.

## 8. Running

### 8.1 The production command

```bash
cd "$PROJECT"
nextflow run main.nf -profile cluster,singularity -c my_study.config \
    --resource_tier small -resume
```

Always `-resume`: Nextflow hashes each task's inputs, code and container, and
an interrupted run continues from where it died.

### 8.2 Why `--resource_tier` goes on the command line

`docs/core/config.md` says the tier is selected by `params.resource_tier`,
but the `tutorial` profile comment in `nextflow.config` records that a tier
set in a `-c` file **cannot** select it — the `includeConfig` chain runs while
`nextflow.config` is parsed, before `-c` merges (verified 2026-08-06).
`launch.sh` scrapes the value from the dataset config and re-passes it as
`--resource_tier`. Do not treat that as proven: the same comment says this CLI
path "is long-standing but was NOT re-verified here, so prefer a profile for
anything new" — a profile (`-profile tutorial`) is the verified mechanism. For
`medium`/`large`, confirm the tier actually landed rather than trusting it:
the first tasks' `cpus`/`memory` in `pipeline_info/trace.tsv` must match that
tier file's `withName:` values, not `small.config`'s (the tell-tale is
`MULTIVI_INTEGRATE`, 200 GB in `small.config`). Allocations silently not
landing is a documented failure mode.

### 8.3 The `-preview` then `-resume` trap

A `-preview` is written to Nextflow's run history; a later bare `-resume` can
latch onto that empty session and report nothing cached. Run previews from a
separate directory, or resume an explicit session:

```bash
nextflow log                              # list sessions
nextflow run main.nf -resume <session-id> -profile cluster,singularity -c my_study.config
```

### 8.4 First real run: the tutorial, not your data

The tutorial runs the real containers on a ~1,000-cell subset of the public
10x 10k PBMC multiome sample, ATAC restricted to chr21 + chr22 (~2.6% of
hg38) — a wiring demo, not biology (CellTypist assigns 45 labels to 1,000
cells). Needs: 36 MB download (78.9 MB unpacked, a GitHub release asset),
~15 GB free including `work/` (~4.5 GB; results ~320 MB), 9.1 GB RAM for the
heaviest task (`ATAC_FINAL_PIPELINE`), no GPU, network for one 2.8 MB
CellTypist model. Measured 42 min on 50 CPUs / 300 GB and 4.6 CPU-hours
(`docs/tutorial.md`). For 8 CPUs / 48 GB the repository quotes two different
numbers — ~2 h 20 min / 4.6 CPU-h in `docs/tutorial.md` (labelled "estimated"
in one table and "measured" in the next) and 1 h 43 min / 6.6 CPU-h in the
`configs/datasets/tutorial_pbmc.config` header — so budget 2–3 h inside the
8 h walltime `launch_tutorial.sh` requests.

```bash
cd "$PROJECT"
REL=https://github.com/swaruplabUCI/FORGE/releases/download/tutorial-data-v1
curl -LO $REL/forge_tutorial_pbmc_v1.tar.gz
curl -LO $REL/forge_tutorial_pbmc_v1.tar.gz.sha256
sha256sum -c forge_tutorial_pbmc_v1.tar.gz.sha256   # must print: OK
mkdir -p tutorial_data
tar -xzf forge_tutorial_pbmc_v1.tar.gz -C tutorial_data/

# Validate: expect PRE-FLIGHT CHECKLIST PASSED (7 checks) and "No warnings."
nextflow run main.nf -preview -profile tutorial,singularity \
    -c configs/datasets/tutorial_pbmc.config

# Run under SLURM (account/partition are deliberately not hardcoded)
sbatch -A <account> -p <partition> launch_tutorial.sh
```

`launch_tutorial.sh` requests `--cpus-per-task=8 --mem=48G --time=08:00:00`,
loads `singularity`/`apptainer`, pins `NXF_VER=25.10.0`, finds `nextflow` on
`PATH`, then `~/bin`, `~/.local/bin` or the repository root (or
`FORGE_NEXTFLOW=/path/to/nextflow`), and
runs the **whole pipeline inside one allocation with Nextflow's local
executor**; inside SLURM it adds `configs/tutorial_slurm.config`, which sizes
the task pool to the allocation (otherwise Nextflow reads the *machine's*
CPU/RAM and the job is OOM-killed with no useful error). Options: `--outdir
DIR`, `--tutorial_data DIR`, `--no-resume`, `--preview`. Use `-profile
tutorial`; `resource_tier = 'tutorial'` in the config does not select it (§8.2).

Structural results that must match: 944 of 1,000 cells pass ATAC initial QC
(`docs/tutorial.md`; `tutorial_pbmc.config` says 943 — treat the counts in
the `expected_results.json` release asset as authoritative when the two
disagree), peak matrix 817 cells × 12,085 peaks, Leiden 4/7/9, 924 RNA cells annotated,
767 cells in RNA ∩ ATAC, 3 MOFA+ factors, ~94 tasks. Then `curl -LO
$REL/checksums_data.txt` and `python3 bin/verify_tutorial_outputs.py --results
results_tutorial --checksums checksums_data.txt` (expect `168/168 matched`).
ChromVAR, footprinting, SCENIC+/pycisTopic and differential are off here.
`docs/verification.md` still carries "TODO: Confirm data is available for
Tier 2" — if the release asset is missing, say so and go from §3.2 to your
own data with expensive blocks off.

### 8.5 Enable one expensive block at a time

After a clean minimal run, turn on one block, re-run `-preview`, run with
`-resume`. Reasonable order:

1. `pycistopic.run` + `scenicplus.run` — GRN inference. Set every reference
   key explicitly; nothing here inherits `gtf_*_full` or `blacklist_bed`:
   `pycistopic { gtf = '/path/to/refs/gencode.v38.annotation.gtf';
   blacklist_bed = '/path/to/refs/hg38-blacklist.v2.bed.gz' }` and `scenicplus
   { ctx_rankings = ...; ctx_scores = ...; motif_annotations = ...; gtf = ...;
   fai = '/path/to/refs/hg38.fa.fai' }` (the block in
   `docs/guides/regulatory.md`). A missing `pycistopic.gtf` is a pre-flight
   error; a missing `pycistopic.blacklist_bed` aborts at DAG construction with
   `Argument of file() function cannot be null` and is **not** pre-flight
   checked; none of the cisTarget paths or `fai` is existence-checked before
   runtime (§6). `pycistopic.species` auto-derives from `params.species`.
   Needs all three cisTarget references and ≥ 256 GB; SCENIC+ requires
   `pycistopic.run = true`. `pycistopic.topics = '10,20,30'` is an LDA sweep,
   `selected_topics = null` lets pycisTopic choose.
2. `differential.run` / `differential_rna.run` — ≥ 2 `condition_group`
   values, and the manifest column itself becomes mandatory (pre-flight error
   without it) for *every* condition-aware workflow: `differential`,
   `differential_rna`, `differential_tf` in `differential` mode,
   `cicero.stratified`, `shi_figures` Tier B, and
   `enhancer_footprinting.disease_stratified`. `differential.run = true` also
   requires a non-empty `differential.comparisons` (e.g. `[['TG','WT']]`,
   treatment first) — otherwise pre-flight fails with `differential.run=true
   but differential.comparisons=[]` — plus `control_condition` /
   `treatment_condition`, which also label stratified Cicero and footprinting
   output. `differential_rna` needs `group_mapping` (not pre-flight checked).
   Enabling `differential.run` auto-activates stratified Cicero.
3. `enhancer_footprinting.msfp_enabled` — **the expensive one.**
   `ENHANCER_FOOTPRINTING_PER_CT` alone was **54% of all compute-hours**
   across the four published datasets (1,012 compute-hours, 4,984 tasks).
   First cap `chromvar.global_top_n` (0 = all TFs; the main cost lever), keep
   `enhancer_footprinting.use_per_ct = true` (~657 tasks → 10–33), and raise
   `qc.cell_type_resolution.min_cells` (default 50) — footprinting memory
   scales with regions, not cells, so a 7-cell type costs as much as a big one.

## 9. Submitting the Nextflow head job with `sbatch`, and monitoring

`launch.sh` submits the Nextflow head process itself as a SLURM job
(`sbatch launch.sh`); the head job then submits one job per task through the
`cluster` profile, keeping a multi-day Nextflow JVM off the login node.

**Never run the shipped `launch.sh` as-is.** It is a UCI instance script: a
hard-coded `/dfs7/...` `PROJECT_DIR` and log paths, `-A vswarup_lab`,
`--partition=standard`, the author's mail address, `ad_mm_10x.config`, a
`/dfs7` `PATH`, and a data check over twelve named AD/WT samples. Copy its
*pattern* (below) instead. The same applies to `sbatch launch.sh --dry-run`
(its `-preview`), which carries all the same hard-coded paths.

### 9.1 Template (modelled on `launch.sh` / `launch_tutorial.sh`)

Create the log directory **before** submitting — SLURM opens `--output` /
`--error` when the job starts, so a missing `logs/` fails the job before the
script's own `mkdir -p logs` can run (this is why `launch.sh` points at
absolute paths in a directory that already exists, and `launch_tutorial.sh`
writes into the submit directory):

```bash
cd "$PROJECT" && mkdir -p logs work && sbatch forge_<study>.sh
```

```bash
#!/bin/bash
#SBATCH --job-name=forge_<study>
#SBATCH -A <account>
#SBATCH --partition=<partition>
#SBATCH --time=72:00:00
#SBATCH --mem=32GB
#SBATCH --cpus-per-task=4
#SBATCH --output=logs/out.forge_%j.log
#SBATCH --error=logs/err.forge_%j.log

set -euo pipefail

PROJECT_DIR=/path/to/FORGE                       # the clone, on shared storage
DATASET_CONFIG=configs/datasets/my_study.config
RESOURCE_TIER=small                              # small | medium | large
OUTDIR="${PROJECT_DIR}/results_my_study"

cd "${PROJECT_DIR}" || exit 1
mkdir -p logs work

module load singularity 2>/dev/null || module load apptainer 2>/dev/null || true
export PATH="$HOME/bin:$PATH"                    # user-directory Nextflow (§3.1)
export NXF_VER="${NXF_VER:-25.10.0}"

# Settings taken from launch.sh / launch_tutorial.sh
export NXF_WORK="${PROJECT_DIR}/work"            # task work dirs on shared storage
export TMPDIR=/tmp                               # node-local scratch for container caches
export NXF_TEMP="$TMPDIR"
export HDF5_USE_FILE_LOCKING="FALSE"
export SINGULARITY_BINDPATH="/path/to/shared/storage,$TMPDIR"

nextflow run main.nf \
  -c "${DATASET_CONFIG}" \
  -profile cluster,singularity \
  -resume \
  --resource_tier "${RESOURCE_TIER}" \
  --outdir "${OUTDIR}" \
  -with-report   "${OUTDIR}/pipeline_info/nextflow_report.html" \
  -with-timeline "${OUTDIR}/pipeline_info/nextflow_timeline.html" \
  -with-trace    "${OUTDIR}/pipeline_info/trace.tsv" \
  -with-dag      "${OUTDIR}/pipeline_info/nextflow_dag.pdf"
```

Every environment line appears in the repository's launchers. `NXF_WORK`
puts `work/` (every task's directory and the `-resume` cache) on shared
storage reachable from all compute nodes. `TMPDIR=/tmp` and `NXF_TEMP=/tmp`
are what both launchers set; containers bind `/tmp` and cache there
deliberately. `HDF5_USE_FILE_LOCKING=FALSE` is set by both launchers and in
the `singularity` profile's `runOptions`; the docs do not explain it further,
so keep it and do not extrapolate. `SINGULARITY_BINDPATH` in `launch.sh` is
`/dfs7,/tmp` — replace `/dfs7` with your shared filesystem (§5.2).
`launch.sh` also checks that `main.nf`, `nextflow.config`, the config, the
manifest, `modules/`, `bin/`, `configs/`, `singularity_cache/` and every
`.sif` exist, and offers `sbatch launch.sh --dry-run` (a `-preview`).

### 9.2 Monitoring and output layout

The head job's SLURM log (`logs/out.forge_<jobid>.log`) shows Nextflow's live
process table; `nextflow log` lists sessions; a failing task's `.command.log`
and `.command.err` are in its `work/` directory.
The execution report gives per-process runtime and **peak RSS** — the evidence
for right-sizing a tier — but **its name depends on how you launched**. With
the §9.1 template (and `launch.sh`), the explicit `-with-*` flags write
`${OUTDIR}/pipeline_info/nextflow_report.html`, `nextflow_timeline.html`,
`trace.tsv` and `nextflow_dag.pdf`; there is no `report.html`. Without those
flags you get the config-level `report.html` / `timeline.html` / `trace.tsv`
instead. Check `trace.tsv`'s `cpus`/`memory` columns against the tier, since
allocations silently not landing is a real failure mode. The docs sometimes
cite `logs/nextflow/report.html` and `logs/nextflow/trace.txt`;
`nextflow.config` actually writes `${params.outdir}/pipeline_info/{report.html,
trace.tsv,timeline.html}`, and because that path is interpolated when
`nextflow.config` is parsed, an `outdir` set in your `-c` dataset config (what
the tutorial's `tutorial_pbmc.config` does, and the normal case) still leaves
them under `results/pipeline_info/`. Passing `-with-report`/`-with-timeline`/
`-with-trace` with explicit paths, as §9.1 and `launch.sh` do, is the way to
put them where you expect.

| Path under `outdir` | Contents |
|---|---|
| `cellbender/`, `rna/qc/`, `rna/concatenated/`, `rna/scvi/`, `rna/post_integration_plots/`, `scanvi/`, `reference/`, `cell_annotation/` | Correction reports; per-sample QC; concatenated and scVI-integrated RNA h5ads; post-integration plots; scANVI and prepared reference (Path A); CellTypist / marker labels. (`main.nf`'s `onComplete` banner and `docs/quickstart.md` still print `rna_qc/` and `integration/` — no process publishes to those paths.) |
| `atac/initial_qc/`, `atac/final/` | `sample_thresholds.json` (the thresholds actually applied — `atac_pipeline_summary.json` reports argparse defaults `min_counts: 5000, min_tsse: 6` instead); `peak_matrix.h5ad`, `celltype_annotations.json` |
| `cicero/`, `chromvar/`, `scprinter/`, `enhancer_viz/` | Connections/CCANs, motif deviations, footprints and TF-gene networks, BigWig `tracks/` and `composites/` |
| `multiome/mudata/`, `multiome/mofa/`, `multiome/multivi/` | Joint `.h5mu`, factors, joint latent + UMAPs |
| `cellchat/`, `hdwgcna/`, `rna_differential/`, `differential/` | Communication, co-expression, MAST DE, differential accessibility |
| `pipeline_info/` | Trace, timeline, execution report |

## 10. On-ramps and resuming from checkpoints

`-resume` is Nextflow's task cache within one project; `params.onramp`
injects intermediates from *elsewhere* so FORGE skips the producing stage,
e.g. `params.onramp { rna_integrated_h5ad =
'/prior/results/integration/rna_integrated.h5ad' }`. What silently busts the
`-resume` cache: touching an upstream file's mtime (opening an h5ad in `r+`
is enough — open intermediates read-only), a new `withName:` block in a tier
file, editing any `bin/` script even in a comment, and a previous `-preview`
in the same directory (§8.3).

| Key | Skips | Rule |
|---|---|---|
| `rna_integrated_h5ad` | Whole RNA arm through integration | With `run_multiome_integration = true` you must also set `rna_per_sample_h5ads_dir` or `mudata_h5mu` — hard error otherwise |
| `atac_peak_matrix_h5ad` | ATAC QC, peaks, clustering | Warns without side keys `atac_individual_samples_dir` (scPRINTER) and `atac_anndataset` (enhancer footprinting) |
| `mudata_h5mu` | Multiome integration | |
| `printer_h5ad` | `SCPRINTER_BUILD_PRINTER` | |
| `cicero_connections` + `cicero_ccan` + `cicero_cds` | Cicero | **All-or-none triple** |
| `chromvar_deviations` + `chromvar_raw` | ChromVAR | **All-or-none pair** |
| `rna_cellchat_csv` | — | Side key for footprinting recipes |

Forward-declared keys with no consumer (`cistopic_obj_pkl`, `seurat_rds`,
`da_peaks_dir`, `cicero_connections_ctrl/_trt`, `cicero_ccan_ctrl/_trt`) are
**rejected**, not ignored. All rules are enforced by `-preview`. For figures
alone, `nextflow run main.nf -entry VIZ_ONLY -c my_study.config` with
`params.viz_only { peak_matrix_h5ad, cicero_connections, cicero_ccan,
cicero_cds, target_genes }` is cheaper than an on-ramped run.

## 11. Failure modes and troubleshooting

Before anything else: `nextflow run main.nf -preview -c my_study.config`.

| Symptom | Cause / fix |
|---|---|
| `Config parsing failed` at startup | Nextflow outside `>=25.04.0,<26.0.0`. `export NXF_VER=25.10.0` and reinstall (§2.2). `main.nf` failing to compile is the same problem on ≤ 24.10.5. |
| `PRE-FLIGHT CHECKLIST FAILED` with numbered errors | Read them all; each names the parameter. Placeholder paths are the usual cause — existence is checked, not presence. |
| `Manifest CSV not found` | A relative `metadata_file` resolves against the launch directory, not the config. |
| `missing required columns: [sample_id (did you mean: sample_ID?)]` | Fix the header exactly; near-misses are diagnosed, never remapped. |
| `atac.annotation_method='scatanno' requires params.scatanno.reference_atlas` | Set the atlas, or set `atac.marker_file`. No atlas-free ATAC option, no ATAC CellTypist fallback. |
| `Argument of \`file()\` function cannot be null` | A gate without its companion path: `atac.run = true` without `atac.sample_metadata`; `differential_rna.run = true` without `group_mapping`; or `pycistopic.run` / `scenicplus.run` / `dorc.run` (the first two default **true**) without `pycistopic.blacklist_bed`, which does not fall back to `params.blacklist_bed`. **Not caught by pre-flight.** |
| `RNA file not found for sample 'X'` | `rna_file` is a filename; a full path yields a doubled path. |
| `rna.run=true but the manifest contains no rows with a non-null rna_file` | An ATAC-only manifest: populate `rna_file`, or set `rna { run = false }`. |
| `Manifest CSV missing 'condition_group' column but a condition-aware workflow is enabled` | Add the column, or disable `differential`, `differential_rna`, `differential_tf` (differential mode), `cicero.stratified`, `shi_figures` Tier B and `enhancer_footprinting.disease_stratified`. |
| `differential.run=true but differential.comparisons=[]` | Set `differential.comparisons` to a list of `[treatment, control]` pairs, e.g. `[['TG','WT']]`, or `differential.run = false`. |
| Per-condition Cicero skips a cell type that passed the global floor | `cicero_per_ct.min_cells_per_stratum` (250) is a separate per-(cell type × condition) floor; skips are logged (exit 77). |
| `resource_tier` rejected | Lowercase `small`, `medium`, `large`, `auto`, `test`, `tutorial` only (`main.nf`'s `allowedTiers`; `Medium` fails). `test` and `tutorial` are meant to be selected by their profiles, not by a `-c` value (§8.2). |
| Nothing happened, no error | Inner gate without outer (`msfp_strip.enabled` without `msfp_enabled`); a misspelled key silently ignored by `-c` (`nextflow -c my_study.config config \| grep -A5 msfp_strip`); or a cell type below `max(min_cells, min_pct × total)` — skips are logged. |
| Exit code 137 | SLURM/cgroup kill. MultiVI needs far more *host* memory than its GPU footprint (`run_imputation = true` needs more); SCENIC+ wants ≥ 256 GB; footprinting on a tiny cell type — raise `qc.cell_type_resolution.min_cells`; some GPU partitions silently clip host memory. Size from the execution report (`nextflow_report.html` under the §9.1 template, else `report.html` — §9.2). |
| Run died, bare exit 1, no `Caused by:` | **Check filesystem quota first.** `df -h /path/to/workdir`; `dd if=/dev/zero of=/path/to/workdir/.probe bs=1M count=10 && rm /path/to/workdir/.probe`. |
| `FATAL: could not open image` | `params.containers` path wrong, or the `.sif` not readable from compute nodes. |
| Container cannot see your data | Missing bind mounts. Add them additively with `export SINGULARITY_BINDPATH="/data,/refs,/tmp"`, or copy the profile's full `runOptions` string and swap `/dfs7` — assigning a short `-B` list replaces the whole default (§5.2). |
| CUDA not available inside the container | `--nv` missing, or a non-GPU node. ChromVAR has no CPU fallback. |
| `QOSMaxSubmitJobPerUserLimit` | Add `maxForks` **and** an error-strategy retry; `maxForks` alone does not prevent submit-time rejections. |
| `/usr/bin/env: 'Rscript': No such file or directory` (exit 127) | A process ran without its container — a tier file missing a container assignment; only the shipped tiers carry them all. |
| `IndexError: index 20000 is out of bounds` in CellBender | `cellbender.total_droplets` equals the barcode count; must be strictly below (defaults 20000 droplets / 5000 `expected_cells`). |
| ATAC arm collapses to a handful of cells | `atac.min_counts` left `null` on a subset falls through to the script default of 5000. Set thresholds explicitly; never copy the tutorial's to whole-genome data. |
| `sample_id_regex did not match barcode sample id` | Listed in `docs/setup/install.md`, but `params.scprinter.sample_id_regex` is not declared in `nextflow.config` and its only consumer, `SCPRINTER_BUILD_MANIFEST` (`modules/scprint/manifest.nf`), is never included by `main.nf` — only `SCPRINTER_BARCODES` is. You cannot hit this error from the shipped pipeline. |
| Confident, plausible, wrong cell types; inverted differential direction | `celltypist.model` mismatched to tissue, or species/genome mismatch across `params.species`, GTF, blacklist, motifs. Positive `log2FC` = up in **treatment**; check `control_condition` / `treatment_condition`. |

When asking for help (https://github.com/swaruplabUCI/FORGE/issues) include
the `-preview` output, `nextflow -c my_study.config config`, the failing
task's `.command.log` and `.command.err`, and the relevant trace row.

## 12. Citation, contact, and how Operon should drive this

> Solano LE, Swarup V, et al. Flow Orchestrated Regulatory Genomics Engine
> (FORGE): A Configurable Nextflow Pipeline for End-to-End snMultiome
> Analysis. *[manuscript in preparation]*, 2026.

License BSD 3-Clause. Public contact: Luis Enrique Solano, lesolano@uci.edu,
Swarup Lab, UC Irvine (https://swaruplab.bio.uci.edu/). Docs: https://swaruplabUCI.github.io/FORGE/.

### 12.1 How Operon should drive this

1. **Validate before anything else.** Run §3.2 right after cloning, then
   `-preview` against the user's config, and again after **every** config
   edit — from a separate directory, or resume by explicit session id (§8.3).
2. **Never run the Nextflow head process on a login node.** Submit it as a
   SLURM batch job like §9.1 (the repository's own `launch.sh` pattern) with
   a walltime covering the whole run (`launch.sh` uses 72 h); container
   builds and reference downloads also go to batch jobs. Login nodes are for
   `-preview`, `nextflow config` and reading logs.
3. **Keep the clone, `work/` and results on shared storage**, not `$HOME`
   and not node-local `/tmp` (`NXF_WORK="$PROJECT/work"`). The launchers set
   `TMPDIR`/`NXF_TEMP` to `/tmp` for container caches deliberately — leave
   it, but budget ≥ 20 GB scratch per parallel task.
4. **Always `-resume`**, and always pass `--resource_tier` explicitly on the
   command line rather than trusting the `-c` value (§8.2).
5. **Start minimal, enable one block at a time** (§8.5). Never turn on
   `enhancer_footprinting.msfp_enabled` or SCENIC+ on a first pass, and never
   without reading the previous run's execution report from
   `pipeline_info/` (`nextflow_report.html` with the §9.1 flags, otherwise
   `report.html` — §9.2).
6. **Ask the user for site facts you cannot infer** (account, partitions, GPU
   `--gres` type names, the shared filesystem to bind, whether `--fakeroot`
   builds are allowed), put them in the dataset config (§7), and confirm with
   `nextflow -c my_study.config config -profile cluster,singularity | grep slurm`.
7. **Be honest about what is unreleased** (pre-built containers, the AD mouse
   dataset, BD example metadata, the scATAnno atlas build tutorial — without
   an atlas, use `atac.marker_file` and say why), and **record for
   reproducibility**: `nextflow -version`, the clone's git commit, `.sif`
   sizes/SHA256s, the resolved config, the manifest, the session id from
   `nextflow log`, and `pipeline_info/trace.tsv`.

## 13. Link index

- Repository: https://github.com/swaruplabUCI/FORGE · Docs: https://swaruplabUCI.github.io/FORGE/ · Issues: https://github.com/swaruplabUCI/FORGE/issues · Recipes: https://github.com/swaruplabUCI/FORGE/tree/main/docs/defs · Tutorial data: https://github.com/swaruplabUCI/FORGE/releases/download/tutorial-data-v1
- Nextflow: https://www.nextflow.io/ · installer: https://get.nextflow.io · Singularity: https://sylabs.io/singularity/ · Apptainer: https://apptainer.org/
- Gencode: https://www.gencodegenes.org/human/release_38.html · https://www.gencodegenes.org/mouse/release_M10.html · https://www.gencodegenes.org/mouse/release_M37.html
- ENCODE blacklists: https://github.com/Boyle-Lab/Blacklist · cisTarget: https://resources.aertslab.org/cistarget/ · JASPAR: https://jaspar.elixir.no/ · cisBP: https://cisbp.ccbr.utoronto.ca/
- CellTypist models: https://www.celltypist.org/models · scPRINTER: https://github.com/buenrostrolab/scPrinter · Allen Brain Cell Atlas: https://alleninstitute.github.io/abc_atlas_access/intro.html
- GEO GSE246791: https://www.ncbi.nlm.nih.gov/geo/query/acc.cgi?acc=GSE246791 · 10x datasets: https://www.10xgenomics.com/datasets · Swarup Lab: https://swaruplab.bio.uci.edu/
