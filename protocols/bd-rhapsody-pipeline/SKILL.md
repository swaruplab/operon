---
name: bd-rhapsody-pipeline
display_name: BD Rhapsody Sequence Analysis Pipeline
description: Install and run the BD Rhapsody™ Sequence Analysis Pipeline (v3.0) on a shared cluster or remote Linux server with no root and no container runtime. Covers the self-contained install bundle, reference archives, FASTQ manifests, per-library YML generation, SLURM array execution, outputs, sample-tag demultiplexing, and the failure modes that cost hours — wrong Sample_Tags_Version on nuclei runs, uncapped Maximum_Threads, node-local scratch, and pinning a stale `latest` bundle.
license: MIT
metadata:
---

# BD Rhapsody™ Sequence Analysis Pipeline on a Remote Server

A practical guide to installing and running the pipeline on a shared cluster or
remote Linux server with **no root access and no container runtime**. Compiled
2026-08-13 against pipeline **v3.0** documentation and the shipped
`pipeline_inputs_template.yml`.

Everything here uses placeholders. Substitute your own paths, account names,
partitions, and library identifiers.

Conventions used below:
- `$PROJECT` = project root on shared storage
- `$PIPELINE` = extracted bundle directory
- `<library>` = one BD cartridge or one pipeline run unit
- `<account>`, `<partition>` = scheduler placeholders

---

## 1. Scope and the one design decision that matters

The pipeline is CWL. There are two ways to satisfy its dependencies: pull
container images at each step, or use the self-contained install bundle.

On a shared cluster the bundle is almost always the right answer. It ships its
own Python, R, and `cwltool` under `$PIPELINE/{external,rhapsody_internal}/bin`,
prepends them to `PATH`, and invokes `cwltool --no-container`. That means:

- No Docker daemon, no root, no Singularity or Apptainer image
- No conda environment, and no module loads beyond a working shell
- No version conflicts with whatever the cluster provides system-wide

The rest of this document assumes the bundle. A containerized route exists and is
documented by BD, but it introduces a dependency most shared systems do not
grant, and one historical pipeline step invoked a container from inside a
container, which defeats hand-rolled conversions.

---

## 2. Install

```bash
mkdir -p "$PROJECT/pipeline" && cd "$PROJECT/pipeline"

# ~1.3 GB
wget https://bd-rhapsody-public.s3.amazonaws.com/Rhapsody-Install-Bundle/rhapsodyPipeline-latest.tar.gz

tar -xvzf rhapsodyPipeline-latest.tar.gz
cd rhapsodyPipeline-*/
export PIPELINE="$PWD/rhapsody"
```

Officially tested: Ubuntu 16.04 / 20.04 / 22.04, Red Hat 7, CentOS 7 / 9. Linux
only. BD labels the bundle experimental, though it is now the route they
recommend first to sites without containers.

Wrapper signature, from the bundled README:

```
./rhapsody pipeline [cwltool options] pipeline_inputs.yml
```

**The YML must always be the last argument.** Parallel node execution is on by
default.

Bundled subcommands:

| Command | Purpose |
|---|---|
| `./rhapsody pipeline inputs.yml` | Main sequence analysis pipeline |
| `./rhapsody makeRhapReference inputs.yml` | Build a custom reference archive |
| `./rhapsody annotateCellLabelUmi inputs.yml` | Move cell label and UMI from R1 into the R2 header |
| `./rhapsody phiXContamination inputs.yml` | Estimate phiX fraction in a FASTQ |

### 2.1 Verify the install before anything else

```bash
"$PIPELINE" pipeline --outdir "$PROJECT/test_results" \
  "$(dirname "$PIPELINE")/test_files/test_smallDemo.yml"
```

The demo data is bundled. If this fails, the problem is the environment, not your
inputs, and you have saved yourself a multi-hour debugging cycle on a real
library.

### 2.2 Pin the version, do not trust `latest`

`rhapsodyPipeline-latest.tar.gz` is a copy, not a symlink, and it lags. As of
2026-08-13:

| Object | Last modified | Size (bytes) | ETag prefix |
|---|---|---|---|
| `rhapsodyPipeline-latest.tar.gz` | 2025-10-28 | 1,291,484,729 | `37558792` |
| `rhapsodyPipeline-3.0.tar.gz` | 2025-10-28 | 1,291,484,729 | `37558792` |
| `rhapsodyPipeline-3.0.post20260521.tar.gz` | 2026-05-21 | 1,369,307,019 | `6a83323d` |

Identical ETag and byte count means `latest` is byte-for-byte **3.0**, not the
newer post build. Check the bucket for what is actually newest, download by
explicit filename, and record the filename plus ETag in your run log.

Published bundles under
`https://bd-rhapsody-public.s3.amazonaws.com/Rhapsody-Install-Bundle/`:
`rhapsody-2.0`, `rhapsodyPipeline-2.1b1`, `2.1`, `2.2`, `2.2.post1`, `2.2.1_rc1`,
`2.2.1`, `2.2.1post1`, `2.3`, `2.3.post1`, `2.3.post2`, `2.3.post3`, `2.4b1`,
`2.4b2`, `2.4b3`, `2.4b3.post1`, `3.0`, `3.0.post20260521`, `latest`

---

## 3. Reference archives

### 3.1 What is inside

**WTA only:**
```
BD_Rhapsody_Reference_Files/
    star_index/                  # STAR --runMode genomeGenerate output
    <annotation>.gtf
```

**WTA + ATAC-Seq** adds:
```
    mitochondrial_contigs.txt
    JASPAR2024_CORE_vertebrates_non-redundant_pfms_jaspar.pfm
    bwa-mem2_index/
```

The GTF in BD's prebuilt archives is pre-filtered to these gene types only:
`protein_coding, protein_coding_LOF, lncRNA, lincRNA, antisense, IG_LV_gene,
IG_V_gene, IG_V_pseudogene, IG_D_gene, IG_J_gene, IG_J_pseudogene, IG_C_gene,
IG_C_pseudogene, TR_V_gene, TR_V_pseudogene, TR_D_gene, TR_J_gene,
TR_J_pseudogene, TR_C_gene`

If you ever compare gene counts against a pipeline built on unfiltered GENCODE,
the difference is the annotation, not the chemistry. State which archive you used
in methods.

### 3.2 Published archives

Browse:
- WTA + ATAC: http://bd-rhapsody-public.s3-website-us-east-1.amazonaws.com/Rhapsody-WTA-ATAC/
- WTA only: http://bd-rhapsody-public.s3-website-us-east-1.amazonaws.com/Rhapsody-WTA/

Direct download pattern: `https://bd-rhapsody-public.s3.amazonaws.com/<key>`

Newest as of 2026-08-13 (sizes are decimal GB):

| Archive | Size |
|---|---|
| `Rhapsody-WTA-ATAC/Pipeline-version2.x_WTA_ATAC_references/RhapRef_Mouse_WTA-ATAC_2025-10.tar.gz` | 32.9 |
| `Rhapsody-WTA-ATAC/Pipeline-version2.x_WTA_ATAC_references/RhapRef_Human_WTA-ATAC_2025-10.tar.gz` | 36.9 |
| `Rhapsody-WTA/Pipeline-version2.x_WTA_references/RhapRef_Mouse_WTA_2025-10.tar.gz` | 24.4 |
| `Rhapsody-WTA/Pipeline-version2.x_WTA_references/RhapRef_Human_WTA_2025-10.tar.gz` | 27.4 |

Older dated builds (2023-08, 2023-09, 2023-11, 2025-03, 2025-09) and combined
human-mouse archives remain published alongside these. **Nothing marks the newest
one.** It is easy to pin a two-year-old archive by copying a URL from an old email
or an old script, so check the listing when you start a project and pin
deliberately.

Legacy v1.x references under `Rhapsody-WTA/Pipeline-version1.x_WTA_references/`
are not compatible with v2.x or v3.x pipelines.

### 3.3 Downloading, including the pre-signed URL case

References are 24 to 68 GB. Download them in a batch job, not on a login node.

If BD support sends a **pre-signed URL** rather than a public path, two things
bite:

- The URL typically expires in 48 hours. Submit promptly.
- Pre-signed URLs are usually GET-only. `HEAD` returns 403, so any tooling that
  probes with HEAD first will fail. Use plain GET.
- The URL contains query parameters with characters that shells and `curl` will
  try to interpret. Use `--globoff`.

```bash
#!/bin/bash
set -euo pipefail

DEST="$PROJECT/reference"
OUT="$DEST/<archive-name>.tar.gz"
URL=$(head -n1 "$PROJECT/download-links.txt")   # keep URLs out of the script

mkdir -p "$DEST"

# --globoff: do not treat [] {} in the query string as glob syntax
# -C -     : resume a partial transfer
# -fL      : fail on HTTP error, follow redirects
curl --globoff -fL -C - -o "$OUT" "$URL"

# Verify before trusting it. A truncated tarball fails here, not four hours
# into an alignment.
tar -tzf "$OUT" > "$DEST/tarball_contents.txt"
echo "size: $(stat -c%s "$OUT") bytes, entries: $(wc -l < "$DEST/tarball_contents.txt")"
```

Do not extract the archive yourself. The pipeline consumes the `.tar.gz` directly
and extracts it internally.

### 3.4 Building a custom reference

Needed for transgenes, a newer annotation, a non-standard build, or unfiltered
biotypes.

```bash
"$PIPELINE" makeRhapReference make_reference_inputs.yml
```

```yaml
Genome_fasta:
  - class: File
    location: "<genome>.primary_assembly.genome.fa.gz"
Gtf:
  - class: File
    location: "<annotation>.primary_assembly.annotation.gtf.gz"
Extra_sequences:
  - class: File
    location: "transgene.fasta"
WTA_Only: True
```

Omit `WTA_Only` to build the combined WTA+ATAC archive, which also needs a JASPAR
PFM file if you want transcription factor motif analysis.

GTF requirements, all easy to violate with a non-GENCODE annotation:

- Every gene and exon line needs `gene_id` and `gene_name`. `gene_name` becomes
  the bioproduct identifier in outputs. Non-unique names get suffixed with
  genomic location.
- `strand` must be `+` or `-`, never `.`.
- Lines need `gene_type` or `gene_biotype` or they are **silently dropped**.
  Disable the biotype whitelist with `Filtering_off`.
- For ATAC, `gene` features must have associated `transcript` features.
- Chromosome names must match exactly between FASTA and GTF.

Building a reference is itself a multi-hour, high-memory job. Submit it to the
scheduler with a similar footprint to a pipeline run.

---

## 4. Organizing inputs

### 4.1 FASTQ requirements

- Format is `.fastq.gz`. Filenames use letters, numbers, hyphens, and underscores
  only. Spaces or special characters cause failures.
- `Reads` takes R1/R2 pairs from WTA mRNA, Targeted mRNA, AbSeq, Sample
  Multiplexing (SMK), and VDJ libraries. As many pairs as you like.
- `Reads_ATAC` takes R1, R2, **and I2** from ATAC libraries.
- **I1 is never passed**, for any modality. It is the sample index and the
  pipeline does not want it.

Sequencing cores typically deliver names like:

```
<MODALITY>_<LIBRARY>_S<sample-index>_L<flowcell-lane>_<R1|R2|I1|I2>_001.fastq.gz
```

Note the collision hazard: `L` appears both in library identifiers and in the
flowcell lane field. Parse with an anchored regex rather than string splitting.

### 4.2 Build a manifest first

Do not hand-write YML files. Walk the FASTQ tree once, emit a machine-readable
manifest, then generate YML from the manifest. This makes the FASTQ-to-library
mapping auditable and reproducible.

```python
#!/usr/bin/env python3
"""Scan a FASTQ tree and emit a per-library manifest."""
from __future__ import annotations
import json, re
from collections import defaultdict
from pathlib import Path

DATA = Path("data")           # root of the delivered FASTQs
OUT_JSON = Path("manifest.json")
OUT_TSV = Path("manifest.tsv")

FASTQ_RE = re.compile(
    r"^(?P<modality>WTA|SMK|ATAC)_(?P<library>[A-Za-z0-9-]+)_"
    r"S(?P<sidx>\d+)_L(?P<lane>\d+)_(?P<read>R1|R2|I1|I2)_001\.fastq\.gz$"
)

libs: dict[str, dict[str, list[str]]] = defaultdict(lambda: defaultdict(list))
unmatched: list[str] = []

for f in sorted(DATA.rglob("*.fastq.gz")):
    m = FASTQ_RE.match(f.name)
    if not m:
        unmatched.append(str(f))
        continue
    libs[m["library"]][m["modality"]].append(str(f.resolve()))

OUT_JSON.write_text(json.dumps({k: dict(v) for k, v in libs.items()},
                               indent=2, sort_keys=True))

with OUT_TSV.open("w") as fh:
    fh.write("library\tmodality\tnum_files\tfastqs\n")
    for lib in sorted(libs):
        for mod in ("WTA", "SMK", "ATAC"):
            files = libs[lib].get(mod, [])
            if files:
                fh.write(f"{lib}\t{mod}\t{len(files)}\t{';'.join(files)}\n")

# Coverage table: eyeball this before generating any YML.
print(f"{'library':<12} {'WTA':>5} {'SMK':>5} {'ATAC':>5}")
for lib in sorted(libs):
    c = {m: len(libs[lib].get(m, [])) for m in ("WTA", "SMK", "ATAC")}
    print(f"{lib:<12} {c['WTA']:>5} {c['SMK']:>5} {c['ATAC']:>5}")

if unmatched:
    print(f"\nWARNING: {len(unmatched)} files did not match the naming pattern:")
    for u in unmatched[:10]:
        print("  ", u)
```

**Read the coverage table.** A library missing its SMK files, or carrying twice as
many WTA files as its neighbours, is a delivery problem you want to catch now
rather than after a failed demultiplexing.

---

## 5. Writing `pipeline_inputs.yml`

The authoritative template is `pipeline_inputs_template.yml`, shipped inside the
bundle. Diff against it after every version upgrade. Field names change between
minor versions.

### 5.1 Assay type is inferred, not declared

There is no "assay type" parameter. The pipeline infers it from which references
you supply. **Never provide both `Reference_Archive` and `Targeted_Reference`.**

Valid combinations:

| References supplied | Assay |
|---|---|
| WTA `Reference_Archive` | WTA only |
| WTA `Reference_Archive` + `AbSeq_Reference` | WTA + AbSeq |
| WTA `Reference_Archive` + `Supplemental_Reference` | WTA + transgenes |
| WTA `Reference_Archive` + `AbSeq_Reference` + `Supplemental_Reference` | WTA + AbSeq + transgenes |
| WTA+ATAC `Reference_Archive` | WTA + ATAC, or ATAC only |
| WTA+ATAC `Reference_Archive` + `Supplemental_Reference` | WTA + ATAC + transgenes |
| `Targeted_Reference` | Targeted only |
| `Targeted_Reference` + `AbSeq_Reference` | Targeted + AbSeq |
| `AbSeq_Reference` | AbSeq only |

Note the shape difference: `Reference_Archive` is a **single mapping**, while
`Reads`, `Targeted_Reference`, `AbSeq_Reference`, and `Supplemental_Reference` are
**lists**.

```yaml
Reference_Archive:
  class: File
  location: "/path/to/RhapRef_<species>_WTA_<date>.tar.gz"
```

### 5.2 Sample multiplexing, including the nuclei case

| `Sample_Tags_Version` | When |
|---|---|
| `human` | Whole-cell human SMK |
| `mouse` | Whole-cell mouse SMK |
| `flex` | Flex kit, species and cell-type agnostic, up to 24 tags |
| `nuclei_includes_mrna` | **SMK + nuclei mRNA**, or SMK + multiomic WTA+ATAC |
| `nuclei_atac_only` | SMK + ATAC-only |

This is the single most commonly mis-set parameter. For a nuclei preparation with
sample tags, the correct value is `nuclei_includes_mrna` **even if the samples are
mouse**. The `mouse` option refers to the whole-cell mouse kit and uses different
tag sequences. Setting `mouse` on a nuclei run produces a run that completes with
exit code 0 and calls nearly every cell `Undetermined`, which is easy to misread
as a wet-lab failure.

The same value applies to a multiomic WTA+ATAC run with SMK. Only a run with ATAC
and no mRNA uses `nuclei_atac_only`.

```yaml
Sample_Tags_Version: nuclei_includes_mrna
Tag_Names: [1-sampleA, 2-sampleB, 5-sampleC]
```

`Tag_Names` is optional and cosmetic: it puts readable names in the metrics.
Format is tag number, hyphen, name. Forbidden characters: `&` `()` `[]` `{}` `<>`
`?` `|`. Note that every tag sequence in the kit is evaluated whether or not you
list it, so omitting a tag from `Tag_Names` does not exclude it from analysis.

### 5.3 Full parameter reference

**Cell calling**

| Parameter | Default | Notes |
|---|---|---|
| `Cell_Calling_Data` | inferred | `mRNA_and_ATAC` if both read types present, `ATAC` if only ATAC, else `mRNA`. Also accepts `AbSeq`, `VDJ` |
| `Cell_Calling_Bioproduct_Algorithm` | `Basic` | `Basic` or `Refined` |
| `Cell_Calling_ATAC_Algorithm` | `Basic` | `Basic` or `Refined` |
| `Expected_Cell_Count` | none | Guides the Basic algorithm when the second-derivative curve has multiple inflection points. Usually the number loaded into the cartridge |
| `Exact_Cell_Count` | none | Hard-forces N putative cells by top error-corrected read count. Use sparingly |

**Reads and alignment**

| Parameter | Default | Notes |
|---|---|---|
| `Exclude_Intronic_Reads` | `false` | Introns are counted by default. **Leave at false for nuclei**, where intronic reads carry a large share of the signal |
| `Long_Reads` | auto | Forces STARlong. Set true only for reads longer than 650 bp |
| `Custom_STAR_Params` | see below | Full override of STAR mapping parameters |
| `Custom_bwa_mem2_Params` | program defaults | Full override for ATAC alignment |

Pipeline default STAR parameters, for the record:
```
--outFilterScoreMinOverLread 0 --outFilterMatchNminOverLread 0
--outFilterMultimapScoreRange 0 --clip3pAdapterSeq A(x38)
--seedSearchStartLmax 50 --outFilterMatchNmin 25 --limitOutSJcollapsed 2000000
```
Long reads add `--seedPerReadNmax 10000`. The pipeline uses STAR 2.7.10b and
bwa-mem2 2.2.1. If you override, do not set non-mapping parameters
(`--genomeDir`, `--outSAMtype`, `--readFilesIn`, `--runThreadN`, and similar). The
pipeline manages those.

**Execution and output**

| Parameter | Default | Notes |
|---|---|---|
| `Maximum_Threads` | **all CPU cores on the node** | See 6.3. Set this on any shared system |
| `Generate_Bam` | `false` | Expensive in CPU and disk. Required if you plan alignment-level downstream work such as transposable element quantification or genotype-based demultiplexing |
| `Run_Name` | none | Output file base name. Letters, numbers, hyphens only |
| `Predefined_ATAC_Peaks` | none | BED file forcing a common peak set. Essential for cross-sample ATAC comparison |

### 5.4 Generate one YML per library

```python
#!/usr/bin/env python3
"""Emit one pipeline_inputs YML per library from manifest.json."""
from __future__ import annotations
import json, os
from pathlib import Path

MANIFEST = Path("manifest.json")
OUTDIR = Path("pipeline_inputs")
REFERENCE = Path("reference/RhapRef_<species>_<assay>_<date>.tar.gz").resolve()

SAMPLE_TAGS_VERSION = "nuclei_includes_mrna"
GENERATE_BAM = False
MAX_THREADS = int(os.environ.get("MAX_THREADS", "16"))

R1R2 = ("_R1_001.fastq.gz", "_R2_001.fastq.gz")
R1R2I2 = R1R2 + ("_I2_001.fastq.gz",)


def file_list(paths):
    return "\n".join(f' - class: File\n   location: "{p}"' for p in paths)


def yaml_for(library, modalities):
    # WTA and SMK share the Reads key. R1/R2 only, never I1.
    reads = sorted(
        f for m in ("WTA", "SMK") for f in modalities.get(m, [])
        if f.endswith(R1R2)
    )
    # ATAC is separate. R1/R2/I2 only, never I1.
    atac = sorted(f for f in modalities.get("ATAC", []) if f.endswith(R1R2I2))

    if not reads and not atac:
        raise ValueError(f"{library}: no usable FASTQs")

    out = [f"# Auto-generated for {library}", ""]

    if reads:
        out += ["Reads:", file_list(reads), ""]
    # Omit the key entirely when empty. An empty `Reads_ATAC:` parses as null
    # and is rejected, which is a confusing failure for a library that simply
    # has no ATAC libraries.
    if atac:
        out += ["Reads_ATAC:", file_list(atac), ""]

    out += [
        "Reference_Archive:",
        "  class: File",
        f'  location: "{REFERENCE}"',
        "",
        f"Sample_Tags_Version: {SAMPLE_TAGS_VERSION}",
        f"Run_Name: {library}",
        f"Generate_Bam: {'true' if GENERATE_BAM else 'false'}",
        f"Maximum_Threads: {MAX_THREADS}",
    ]
    return "\n".join(out) + "\n"


def main():
    OUTDIR.mkdir(parents=True, exist_ok=True)
    libs = json.loads(MANIFEST.read_text())

    index = ["idx\tlibrary\tinput_yml"]
    for i, lib in enumerate(sorted(libs), start=1):
        path = OUTDIR / f"{lib}.yml"
        path.write_text(yaml_for(lib, libs[lib]))
        index.append(f"{i}\t{lib}\t{path.resolve()}")

    Path("array_index.tsv").write_text("\n".join(index) + "\n")
    print(f"Wrote {len(libs)} YML files and array_index.tsv")


if __name__ == "__main__":
    main()
```

The `array_index.tsv` it emits is the key to safe array jobs. See 6.2.

---

## 6. Running under a scheduler

Examples use SLURM directives because they are the most common. The concepts
translate directly to PBS, LSF, or SGE.

### 6.1 Size the job by measurement, not by guessing

Run one library first, as a single job, and record what it actually consumed. A
representative measured data point for a WTA + SMK run with a 57 GB R2 FASTQ, on
32 CPUs:

**4 h 32 m wall, 101 GB peak RSS.**

That is one point on one dataset, so treat it as an order of magnitude rather than
a specification. It is enough to say the common advice of "at least 96 GB for WTA"
is a floor and not a comfortable working number.

A workable rule: pick your largest library, run it once, then set the array's
memory to roughly 2 to 2.5x observed peak RSS and the walltime to roughly 3 to 5x
observed elapsed. Larger libraries in the same run scale mostly with input FASTQ
size.

Pull the numbers from your scheduler's accounting:

```bash
sacct -j <jobid> --format=JobID,JobName,Elapsed,AllocCPUS,MaxRSS,State
```

Two scheduler details worth knowing before you commit:

- **CPU charging often scales with memory.** Many sites bill CPUs in proportion to
  the node's memory-per-core ratio, so the same memory request can be charged more
  cores on a standard partition than on a high-memory one. The high-memory
  partition can be the cheaper landing zone for a memory-heavy job, not the more
  expensive one. Check your site's policy.
- **List multiple partitions rather than one.** Letting the scheduler place each
  task wherever it can start soonest usually beats queueing behind a single
  partition, especially when the standard partition has nodes that already meet
  your memory request.

### 6.2 Array jobs: map index to library through a file

Never compute the library identifier arithmetically from the array task ID. A
partial rebuild of the input set silently shifts the mapping, and you get a run
where outputs are labelled with the wrong library. Read the mapping from the
`array_index.tsv` generated alongside the YML files.

```bash
#!/bin/bash
#SBATCH --account=<account>
#SBATCH --partition=<partition-list>
#SBATCH --job-name=rhapsody
#SBATCH --array=1-<N>
#SBATCH --nodes=1
#SBATCH --ntasks=1
#SBATCH --cpus-per-task=32
#SBATCH --mem=256G
#SBATCH --time=24:00:00
#SBATCH --output=logs/%x_%A_%a.out
#SBATCH --error=logs/%x_%A_%a.err

set -euo pipefail

PROJECT=<project-root>
PIPELINE="$PROJECT/pipeline/rhapsodyPipeline-<version>/rhapsody"
INDEX="$PROJECT/config/array_index.tsv"

cd "$PROJECT"
mkdir -p logs

# Index -> library mapping comes from a file, not from arithmetic.
ROW=$(awk -F'\t' -v idx="$SLURM_ARRAY_TASK_ID" 'NR>1 && $1==idx {print; exit}' "$INDEX")
[[ -n "$ROW" ]] || { echo "No array_index.tsv row for idx=$SLURM_ARRAY_TASK_ID" >&2; exit 2; }
IFS=$'\t' read -r IDX LIBRARY INPUT_YML <<< "$ROW"

OUTDIR="$PROJECT/results/${LIBRARY}"
SCRATCH="$PROJECT/tmp/${LIBRARY}_${SLURM_JOB_ID}/"
mkdir -p "$OUTDIR" "$SCRATCH"

[[ -x "$PIPELINE"   ]] || { echo "Missing wrapper: $PIPELINE" >&2; exit 2; }
[[ -f "$INPUT_YML"  ]] || { echo "Missing YML: $INPUT_YML" >&2; exit 2; }

# Rerun safety: a finished library is skipped, not recomputed.
if [[ -f "$OUTDIR/${LIBRARY}_COMPLETE" ]]; then
  echo "[$(date -Iseconds)] $LIBRARY already complete, skipping."
  exit 0
fi

export TMPDIR="$SCRATCH"

echo "[$(date -Iseconds)] task=$IDX library=$LIBRARY cpus=${SLURM_CPUS_PER_TASK}"

# Capture the exit status explicitly. Under `set -e` a bare `rc=$?` after the
# command never runs on failure, so the failure branch would be dead code.
rc=0
"$PIPELINE" pipeline \
    --outdir "$OUTDIR" \
    --tmpdir-prefix "$SCRATCH" \
    --cachedir "$SCRATCH/cache" \
    "$INPUT_YML" || rc=$?

if [[ $rc -eq 0 ]]; then
  touch "$OUTDIR/${LIBRARY}_COMPLETE"
fi
echo "[$(date -Iseconds)] rhapsody exit=$rc"
exit $rc
```

### 6.3 Four settings that cause most cluster-specific failures

**Cap `Maximum_Threads`.** The default is every core the pipeline can see, which is
the whole physical node, not your allocation. On a shared node this oversubscribes
badly: your job thrashes, and you degrade everyone else on the node. Set
`Maximum_Threads` in the YML to match `cpus-per-task`. This governs the
read-processing steps (QualCLAlign, AlignmentAnalysis, VDJ assembly).

**Keep scratch off `/tmp`.** Node-local `/tmp` is small and will fill mid-run,
typically hours in. Set both `TMPDIR` and `--tmpdir-prefix` to a per-job directory
on shared storage with room for several times the input size. Setting only one of
the two is a common partial fix that still fails.

**Use `--cachedir`.** cwltool skips already-completed steps on rerun. On a pipeline
where a single library takes hours, this turns a walltime overrun from a full
restart into a resume.

**Watch your own array throttle.** A `%N` suffix on `--array` caps concurrent
tasks. If tasks sit `PENDING` with a reason like `JobArrayTaskLimit` while the
cluster has idle cores, you are blocked by your own throttle, not by the
scheduler. Unless your account has a group resource cap, the throttle buys
nothing. Lift it live:

```bash
scontrol update JobId=<arrayjobid> ArrayTaskThrottle=<N>
```

### 6.4 Directory layout that survives a rerun

```
$PROJECT/
  pipeline/rhapsodyPipeline-<version>/   # the bundle, version in the path
  reference/                             # .tar.gz archives, unextracted
  data/                                  # delivered FASTQs, read-only
  config/
    manifest.json
    array_index.tsv
  scripts/
    pipeline_inputs/<library>.yml
  results/<library>/                     # final outputs + <library>_COMPLETE
  tmp/<library>_<jobid>/                 # scratch, safe to purge
  logs/
```

Putting the version in the bundle path means an upgrade cannot silently change
what produced a given result.

---

## 7. Outputs

Written to `--outdir`, prefixed with `Run_Name`.

| Output | Content |
|---|---|
| Metrics Summary CSV | Top-level run metrics. First thing to read |
| Pipeline Report HTML | Metrics plus diagnostic plots, self-contained |
| Cell-by-feature data tables | Expression matrices, RSEC and DBEC counts |
| `.cellismo`, `.h5mu`, Seurat `.rds` | Ready-made inputs for downstream tools. Seurat output carries sample tag and bioproduct metadata |
| BAM + index | Only if `Generate_Bam: true`. `.bam.csi` index for chromosomes over 500 Mb |
| Bioproduct statistics | Per-feature stats |
| `<run>_Sample_Tag_Calls.csv` | Per-cell sample of origin |
| `<run>_Sample_Tag_Metrics.csv` | Sample determination algorithm metrics |
| `<run>_Sample_Tag<NN>.zip` | Per-sample data tables and metric summary |
| VDJ metrics, per-cell, AIRR contigs | TCR/BCR output |
| ATAC fragments, transposase sites, peaks, peak annotation, data tables, metrics | ATAC output |
| Dimensionality reduction | Coordinates, see note below |

Two counting schemes appear throughout: **RSEC** (recursive substitution error
correction) and **DBEC** (distribution-based error correction). Report which you
used.

Dimensionality reduction in v3.0 is thresholded: under 100,000 cells gives both
t-SNE and UMAP; 100,000 to 300,000 gives UMAP only; above 300,000 a 300,000-cell
subsample is used. If you are running a large cohort and find embeddings that do
not cover every cell, this is why. Recompute yourself downstream for large
atlases.

---

## 8. Sample tag demultiplexing

### 8.1 How calls are made

Kits provide up to 12 species-specific tags or up to 24 flex tags. Every tag
sequence in the kit is evaluated whether or not you used it or named it. The
pipeline appends tag barcodes to the reference automatically.

1. **High quality singlets first.** A putative cell where more than 75% of tag
   reads come from one tag. All other tag counts in that cell are recorded as
   noise.
2. **Per-tag thresholds.** A tag's minimum read count is the lowest count among
   its high quality singlets.
3. **Noise model.** Per-tag noise contribution is that tag's noise divided by
   total noise. A trend line of total noise against total tag count per cell
   predicts expected noise for any cell.
4. **Noise subtraction.** Expected per-cell noise is apportioned across tags by
   noise contribution and subtracted, recovering singlets that initially fell
   short.
5. **Final call.** Any tag still above its minimum is called. Two or more gives
   `Multiplet`. Too few counts gives `Undetermined`.

BD names two noise sources explicitly: barcode contamination from oligo
manufacturing, and incomplete washing leaving residual labeling.

The consequence worth internalizing: **the noise model is estimated from the data
itself.** A tag that is genuinely contaminated inflates its own noise share across
every cell in that cartridge, so the subtraction over-corrects that tag and
under-corrects the others. Threshold estimation and multiplet calling shift for
the whole cartridge, not just the affected sample.

### 8.2 Joining calls back to a sample sheet

`Sample_Tag_Calls.csv` has comment lines starting with `#`, so skip them on read.
Relevant columns are `Cell_Index` and `Sample_Tag`. Tag values look like
`SampleTag05_mm`, plus the literals `Undetermined` and `Multiplet`.

**Cell indices are per-run.** When merging libraries into one object, prefix them
(`<library>_<cell_index>`) or you will silently collide.

```python
import pandas as pd

calls = pd.read_csv(f"results/{lib}/{lib}_Sample_Tag_Calls.csv", comment="#")

def status(tag):
    if pd.isna(tag):                          return "absent"     # not in calls file
    if tag in ("Undetermined", "Multiplet"):  return tag
    return "singlet"

calls["demux_status"] = calls["Sample_Tag"].apply(status)
```

Four statuses, and the fourth is the one people forget:

| Status | Meaning |
|---|---|
| `singlet` | One tag above threshold |
| `Multiplet` | Two or more tags above threshold |
| `Undetermined` | No tag above threshold |
| `absent` | Cell is in the expression matrix but has no row in the calls file |

Join on `(library, tag_id)` against your sample sheet. After the join, cells called
as singlets whose `(library, tag)` pair is **not in the sheet** are not real
singlets. Give them their own status rather than letting them merge as nulls:

```python
noise = m["demux_status"].eq("singlet") & m["sample_id"].isna()
m.loc[noise, "demux_status"] = "unassigned_tag"
```

### 8.3 Diagnostics worth running every time

Before trusting any downstream assignment:

- **Per-library tag census.** Cross-tabulate library against called tag. A tag
  listed in the sample sheet with **zero** called cells, sitting next to an
  unlisted tag carrying a large population in the same library, is the signature
  of a swapped or mislabeled tube. Correct it in the sample sheet, document the
  correction, and keep the correction in version control rather than editing the
  spreadsheet in place.
- **Noise share across libraries.** Compare per-tag noise contribution in
  `Sample_Tag_Metrics.csv` across libraries. A tube-level contamination problem
  shows as one tag with an outlying noise share in every library that used that
  tube, and a normal share in libraries that did not. That contrast is what
  distinguishes a reagent problem from a single bad cartridge.
- **`Undetermined` and `Multiplet` rates.** Track these per library and check
  whether they correlate with the presence of a suspect tag.
- **Recovery count.** Report samples recovered against samples expected. A sample
  sheet with 46 animals that yields 44 is telling you something before any biology
  starts.
- **Independent cross-check.** Where pooled samples differ genetically,
  genotype-based demultiplexing (souporcell, vireo, demuxlet) is fully independent
  of the tag chemistry. This requires `Generate_Bam: true`, so decide before the
  run, not after.

---

## 9. Troubleshooting

| Symptom | Cause |
|---|---|
| Fails immediately parsing FASTQs | Spaces or special characters in filenames |
| `Reads_ATAC` rejected on a library with no ATAC | Empty YML key parses as null. Omit the key entirely |
| Reference rejected | Both `Reference_Archive` and `Targeted_Reference` supplied, or a manually extracted directory passed instead of the `.tar.gz` |
| Out of memory hours in | Under-provisioned. Measure with one library first, see 6.1 |
| Scratch fills mid-run | `TMPDIR` or `--tmpdir-prefix` still pointing at node-local `/tmp` |
| Job thrashes, node overloaded | `Maximum_Threads` unset, pipeline grabbed every core on the node |
| Nearly all cells `Undetermined` | Wrong `Sample_Tags_Version`. For nuclei runs use `nuclei_includes_mrna`, not the species option |
| One sample missing entirely | Tag mislabel in the sample sheet, or tube swap. See 8.3 |
| Gene counts differ from another pipeline | BD's prebuilt GTF is biotype-filtered. See 3.1 |
| `makeRhapReference` silently drops genes | Missing `gene_type`/`gene_biotype`, or `strand` set to `.` |
| Download fails with 403 on HEAD | Pre-signed URL is GET-only. See 3.3 |
| Array tasks stuck `PENDING` | Your own `%N` array throttle. See 6.3 |

Log files land under the output directory and the cwltool scratch. Keep scratch
with `--leave-tmpdir` when debugging a specific node failure.

Docs:
- Access log files: https://bd-rhapsody-bioinfo-docs.genomics.bd.com/troubleshooting/tr_log_files.html
- Output metrics and associated problems: https://bd-rhapsody-bioinfo-docs.genomics.bd.com/troubleshooting/tr_metrics_solutions.html
- Skim sequencing for library QC: https://bd-rhapsody-bioinfo-docs.genomics.bd.com/troubleshooting/tr_skim.html

---

## 10. Reproducibility checklist

Record these alongside the results. Every one of them has changed underneath
someone at some point.

- [ ] Bundle filename and ETag, not "latest"
- [ ] Reference archive filename with its date stamp
- [ ] The generated `manifest.json` and `array_index.tsv`
- [ ] One `pipeline_inputs.yml` per library, in version control
- [ ] `Sample_Tags_Version` used, and why
- [ ] `Exclude_Intronic_Reads` setting
- [ ] RSEC or DBEC for any reported count
- [ ] Sample sheet corrections, as code rather than spreadsheet edits
- [ ] Scheduler job IDs, elapsed time, and peak RSS

---

## 11. Link index

**Documentation**
- Docs home (v3.0): https://bd-rhapsody-bioinfo-docs.genomics.bd.com/
- Release notes: https://bd-rhapsody-bioinfo-docs.genomics.bd.com/release_notes.html
- Reference files: https://bd-rhapsody-bioinfo-docs.genomics.bd.com/setup/input/reference_files.html
- Pipeline parameters: https://bd-rhapsody-bioinfo-docs.genomics.bd.com/setup/input/parameters.html
- FASTQ requirements: https://bd-rhapsody-bioinfo-docs.genomics.bd.com/setup/input/fastq_files.html
- Local server setup: https://bd-rhapsody-bioinfo-docs.genomics.bd.com/setup/local/top_local_setup.html
- Input specification file: https://bd-rhapsody-bioinfo-docs.genomics.bd.com/setup/local/local_input_file.html
- Docker-free install bundle: https://bd-rhapsody-bioinfo-docs.genomics.bd.com/resources/pipeline_install_bundle.html
- Extra utilities: https://bd-rhapsody-bioinfo-docs.genomics.bd.com/resources/extra_utilities.html
- Sample tag analysis: https://bd-rhapsody-bioinfo-docs.genomics.bd.com/steps/steps_sample_tag.html
- Putative cell calling: https://bd-rhapsody-bioinfo-docs.genomics.bd.com/steps/steps_putative_cells.html
- Error correction (RSEC/DBEC): https://bd-rhapsody-bioinfo-docs.genomics.bd.com/steps/steps_error_correction.html
- ATAC-Seq analysis: https://bd-rhapsody-bioinfo-docs.genomics.bd.com/steps/steps_atac.html
- Output files index: https://bd-rhapsody-bioinfo-docs.genomics.bd.com/outputs/top_outputs.html
- Tool citations: https://bd-rhapsody-bioinfo-docs.genomics.bd.com/references.html
- User's Guide PDF: https://www.bdbiosciences.com/content/dam/bdb/marketing-documents/products-pdf-folder/software-informatics/rhapsody-sequence-analysis-pipeline/Rhapsody-Sequence-Analysis-Pipeline-UG.pdf

**Downloads**
- Install bundles: https://bd-rhapsody-public.s3.amazonaws.com/Rhapsody-Install-Bundle/
- WTA + ATAC references: http://bd-rhapsody-public.s3-website-us-east-1.amazonaws.com/Rhapsody-WTA-ATAC/
- WTA-only references: http://bd-rhapsody-public.s3-website-us-east-1.amazonaws.com/Rhapsody-WTA/
- AbSeq references: http://bd-rhapsody-public.s3-website-us-east-1.amazonaws.com/AbSeq-references/
- AbSeq panel generator: https://abseq-ref-gen.genomics.bd.com/
- CWL repository: https://bitbucket.org/CRSwDev/cwl
- JASPAR PFM downloads: https://jaspar.elixir.no/downloads/
- GENCODE: https://www.gencodegenes.org/

**Support**
- BD single-cell multiomics support: scomix@bdscomix.bd.com
- scomix portal, primary analysis: https://scomix.bd.com/hc/en-us/articles/360023293991-Bioinformatics-Primary-Analysis
- scomix news and updates: https://scomix.bd.com/hc/en-us/articles/30601672679053-News-and-latest-updates
