# Operon — Sandboxed Read-Only Sanitized Data Mount

**Design specification for expert review**

| | |
|---|---|
| **Status** | Draft — for review |
| **Version** | 0.1 |
| **Date** | 2026-05-18 |
| **Applies to** | Operon ≥ v0.6.x (Tauri 2 desktop IDE wrapping Claude Code) |
| **Author** | Swarup Lab / Operon maintainers |
| **Audience** | HPC administrators, research-data security reviewers, bioinformatics leads |
| **Colloquial name** | "Air-gapped mode" (see §2 for why this term is imprecise) |

---

## 1. Executive summary

Faculty have raised a concern that agentic AI systems embedded in Operon can
read file metadata and the contents of real research data, and could therefore
take unintended actions on that data. The mitigation requested is a
**read-only, sanitized data mount**: the AI agent should operate against a
*derived, sanitized copy* of the data, exposed *read-only*, with the original
raw data **not present in the agent's filesystem at all**.

This document specifies how to implement that. The mechanism decomposes into
two independent problems:

1. **Sanitization (Part A)** — produce a derived copy of the dataset with
   sensitive content removed, reduced, or replaced by metadata stubs.
2. **Read-only mounting (Part B)** — expose *only* that sanitized copy to the
   agent process through an OS **container namespace**, mounted read-only,
   while the raw data directories are simply never bound into the namespace.

The central technical claim of this document:

> A *mount* is not the same as `chmod -w`. `chmod` makes a directory read-only
> for **every** process on the host. A mount inside a container namespace gives
> the **agent process its own filesystem view**, in which the sanitized data is
> read-only and the raw data **does not exist**. Namespace isolation — not file
> permissions — is the mechanism that satisfies the faculty requirement.

The recommended runtimes are **Singularity/Apptainer** on HPC (already the
cluster-standard, unprivileged container runtime) and **Docker/Podman** for
local desktop use. Operon already ships command modules for both
(`extensions.rs`: `singularity_*`, `docker_*`), so part of the plumbing exists.

This is a non-trivial engineering effort (estimated three sub-phases, §10).
No code has been written; this document is for design review.

---

## 2. Terminology

The phrase **"air gap"** has been used informally to describe this feature. It
is worth being precise, because reviewers from a security background will
object to the term:

- A true **air gap** is *physical network isolation* — a machine with no
  network interface to an untrusted network. That is **not** what this feature
  provides.
- What this feature provides is more accurately a **capability-restricted,
  namespace-isolated sandbox** with a **read-only sanitized data view**. The
  agent runs on the same host as the data; it simply cannot *see* or *modify*
  the real data because of how its filesystem namespace is constructed.

Throughout this document we use **"sandboxed mode"** for the feature and
reserve "air gap" for the colloquial framing. If true network isolation is
also required, that is a separate, composable control (see §8.3).

Other terms:

- **Raw data** — the original, sensitive dataset (e.g. snRNA-seq matrices,
  sample sheets with donor identifiers, clinical covariates, BAM/FASTQ files).
- **Sanitized data** — a derived artifact produced by the sanitization step;
  the only data the agent is permitted to see.
- **Bind mount** — exposing a host directory inside a container at a chosen
  path, optionally read-only (`:ro`).
- **The agent** — the Claude Code process (`claude -p ...`) that Operon
  launches in headless `stream-json` mode.

---

## 3. Motivation and scope

### 3.1 The concern, decomposed

The faculty concern contains several distinct worries. It helps to separate
them, because the proposed mechanism addresses some fully and others only
partially:

| # | Concern | Addressed by this design? |
|---|---------|---------------------------|
| C1 | Agent can **read raw data values** | **Yes** — raw data is not in the namespace |
| C2 | Agent can **read file metadata** (paths, sizes, schema) | **Partially** — it sees metadata of the *sanitized* tree only |
| C3 | Agent can **modify or delete raw data** | **Yes** — sanitized data is `:ro`; raw data absent |
| C4 | Agent can **execute code** with unintended effects | **Composable** — combine with the capability gate (§8.1) |
| C5 | Agent can **exfiltrate** what it does see, over the network | **Composable** — requires network isolation (§8.3) |
| C6 | Sanitization is **incomplete** (sensitive data leaks into the sanitized copy) | **No** — this is a human/process responsibility (§7.6) |

This document focuses on C1–C3. C4 and C5 are addressed by separate, composable
controls and are described where they interact. C6 is explicitly *out of
scope for the mechanism* — no software can decide what is "sensitive" in a
research dataset; that requires a human policy (§7.6).

### 3.2 In scope

- Running the Operon agent against a read-only sanitized data view, on HPC
  (Singularity/Apptainer) and on local desktops (Docker/Podman).
- A sanitization pipeline with configurable strictness.
- Operon settings, commands, and UI to drive the above.
- Audit logging of what was exposed to the agent.

### 3.3 Out of scope

- True network air gap (separate control; see §8.3).
- Defending the *host* from a compromised agent — the sandbox protects the
  *data from the agent*, not the host from the agent. Host hardening is the
  cluster's responsibility.
- Correctness of the sanitization policy itself (§7.6).
- Multi-tenant isolation between different users' agents.

---

## 4. Background — how Operon runs the agent today

Reviewers need the current baseline to evaluate the delta. Operon launches the
Claude Code CLI in headless mode and parses its NDJSON output stream. There are
three execution paths:

### 4.1 Local mode

```
claude -p --verbose --output-format stream-json --dangerously-skip-permissions ...
```

The process runs directly on the user's macOS/Windows/Linux machine, as the
user, with the user's full filesystem and environment. Operon reads the
process stdout directly.

### 4.2 HPC terminal mode (primary mode for HPC users)

The agent runs **inside an existing tmux session on a compute node**. Operon
writes a command line into the terminal's PTY:

```
_o='/shared/path/.operon-SESSION.jsonl'; _d='/shared/path/.operon-SESSION.done'
cd '/shared/path' && claude ... > "$_o" 2>&1; echo $? > "$_d"
```

A *separate* SSH connection from the login node tails `.operon-SESSION.jsonl`
and streams it back to Operon. Output files live on the **shared filesystem**
(NFS/GPFS), never `/tmp` (which is node-local on HPC).

### 4.3 Remote non-terminal SSH mode

The agent is launched over SSH on the remote host without a tmux wrapper;
output is streamed similarly.

### 4.4 Existing safety mechanism — the capability gate

Operon already ships a deterministic safety layer: a Claude Code **`PreToolUse`
hook** (the "deletion guard") installed via the `--settings <path>` flag. The
hook is a shell script that inspects each proposed tool call and can block it
(`exit 2`) regardless of `--dangerously-skip-permissions`. This is important
context: **the sandboxed mount described here reuses the same launch-wrapping
machinery** and *composes with* the capability gate. It does not replace it.

### 4.5 Why the current model is the concern

In all three paths the agent inherits the **full filesystem view of the user**.
Its `Read`, `Glob`, and `Grep` tools can traverse anything the user can read;
its `Bash` tool can run anything the user can run. There is no boundary between
"the agent's data" and "the lab's real data." That is precisely what the
sandboxed mount introduces.

---

## 5. Design overview

```
                    ┌──────────────────────────────────────────────┐
                    │                   HOST                       │
                    │  (login node / compute node / local machine)  │
                    │                                               │
   ┌─────────────┐  │   /data/raw/         <-- NEVER bound          │
   │  Raw data   │──┼──▶ /data/raw/cohort.h5ad   (invisible to      │
   │ (sensitive) │  │    /data/raw/samples.csv    the agent)        │
   └─────────────┘  │         │                                     │
          │         │         │  sanitization step (Part A)         │
          ▼         │         ▼                                     │
   ┌─────────────┐  │   /data/sanitized/   <-- bound READ-ONLY      │
   │ Sanitized   │──┼──▶ /data/sanitized/cohort.schema.json         │
   │   data      │  │    /data/sanitized/samples.redacted.csv       │
   └─────────────┘  │         │                                     │
                    │         │                                     │
                    │  ┌──────┼──────────────────────────────────┐  │
                    │  │      ▼      CONTAINER NAMESPACE          │  │
                    │  │   /data  (ro) ◀── bind sanitized only    │  │
                    │  │   /work  (rw) ◀── bind scratch workspace │  │
                    │  │   claude -p --output-format stream-json  │  │
                    │  │       │                                  │  │
                    │  │       └─▶ /work/.operon-SESSION.jsonl ────┼──┼──▶ Operon
                    │  └─────────────────────────────────────────┘  │   (tail/stream)
                    └──────────────────────────────────────────────┘
```

The agent's entire universe is `{/data (ro), /work (rw)}`. The raw data path is
not in the bind list, so from inside the namespace it does not exist — there is
nothing to deny, nothing to read, nothing to escape to.

---

## 6. Part B — the read-only mount

We describe Part B before Part A because the mount is the *enforcement*
mechanism and the simpler of the two; Part A (sanitization) only decides
*what* sits behind the read-only mount.

### 6.1 Why a container namespace and not file permissions

Several weaker alternatives will be proposed by reviewers; here is why they are
rejected:

| Alternative | Why it is insufficient |
|-------------|------------------------|
| `chmod -w` / `chmod 0444` on the data dir | Read-only for **all** processes including the user; coarse; the agent can still *read* raw values; trivially reverted; does not hide the data |
| `chflags uchg` (macOS immutable) | Same — read-only ≠ hidden; system-wide; not process-scoped |
| `PreToolUse` hook denying `Read` of data paths | LLM-tool-level, not kernel-level; depends on the hook enumerating every path/extension; a `Bash` call (`cat`, `head`) is a separate vector; brittle denylist |
| Prompt instruction ("don't read `/data/raw`") | Not enforced at all; the model can ignore it |
| A restricted Unix user for the agent | Helps, but the agent still shares the filesystem tree; managing a second account on HPC is impractical |

A **mount namespace** is the only mechanism that is simultaneously
(a) kernel-enforced, (b) process-scoped (the user's own shell is unaffected),
and (c) able to make the raw data **absent**, not merely unreadable. "Absent"
is strictly stronger than "denied": there is no path to attack.

### 6.2 HPC — Singularity / Apptainer

HPC clusters do not permit Docker (its daemon runs as root). The standard
unprivileged container runtime is **Singularity** or its Linux Foundation fork
**Apptainer** (command `apptainer`; flags compatible; env-var prefix
`APPTAINER_*` vs `SINGULARITY_*`). Operon already integrates Singularity
(`extensions.rs`: `singularity_action`, `singularity_list_images`,
`singularity_list_instances`).

#### 6.2.1 The wrapped launch

In sandboxed mode, instead of writing `claude ...` into the tmux PTY (§4.2),
Operon writes a wrapped command:

```bash
singularity exec \
  --contain \                                  # do not auto-bind $HOME, /tmp, CWD
  --no-home \                                  # belt-and-suspenders: never mount the user home
  --cleanenv \                                 # do not inherit the host environment
  --bind /data/project/sanitized:/data:ro \    # sanitized data — kernel-enforced read-only
  --bind /data/project/run-workspace:/work \   # writable scratch (scripts + the .jsonl stream)
  --pwd /work \                                # agent's working directory
  /data/project/images/operon-agent.sif \      # the agent image (§6.4)
  claude -p --verbose --output-format stream-json \
         --dangerously-skip-permissions \
         --settings /work/.operon/guard/settings.json \
         ...
```

#### 6.2.2 What each flag buys

- **`--contain`** — Singularity *by default* bind-mounts `$HOME`, `/tmp`,
  `/var/tmp`, and the current working directory into the container. `--contain`
  disables those auto-binds and gives an empty in-memory `/tmp` and `/home`.
  Without this flag the entire user home (which on HPC commonly contains or
  symlinks the raw data) would be visible. **This flag is mandatory.**
  Consider **`--containall` (`-C`)** to additionally isolate PID/IPC namespaces.
- **`--no-home`** — explicit guarantee the user home is never mounted, even if
  the launch CWD happens to be inside it.
- **`--cleanenv`** — the agent does not inherit host environment variables
  (which may contain paths, tokens, or `PATH` entries pointing at raw data
  tooling). Required env (e.g. the API key) is injected explicitly (§6.6).
- **`--bind src:dest:ro`** — a write to `/data` returns `EROFS` from the
  kernel. This holds *irrespective* of which tools the agent has or whether any
  hook fires. It is the strongest guarantee in the design.
- **`--bind src:dest`** (no `:ro`) — the single writable location, `/work`,
  for generated scripts and the NDJSON stream file.
- The container's **own root filesystem (the `.sif`) is read-only by default** —
  the agent cannot modify its own tooling.
- The process runs as the **invoking user's UID/GID** (Singularity does not
  escalate). Files the agent creates in `/work` are owned by the user — no
  root-owned output, which matters on HPC.

#### 6.2.3 Default-bind hazard

Cluster administrators frequently configure **site-wide default binds** in
`singularity.conf` / `apptainer.conf` (e.g. `/scratch`, `/dfs`, `/pub`,
project filesystems). These are bound **even with `--contain`**. This is a
critical review item:

> **Reviewer action required:** obtain the `bind path` entries and
> `user bind control` setting from the target cluster's `singularity.conf`.
> If the cluster force-binds the filesystem that holds the raw data, the
> sandbox is defeated. Mitigations: place the run-workspace and sanitized
> copy on a filesystem *not* in the default-bind list, or request an
> admin-provided execution profile, or use `--no-mount` to suppress specific
> binds (newer Singularity/Apptainer only).

#### 6.2.4 Network

Unprivileged Singularity **shares the host network namespace** by default;
`--net --network none` requires setuid-mode Singularity or admin configuration.
Therefore, on HPC, **do not rely on the container for network isolation**.
If C5 (exfiltration) is in scope, use the capability gate (§8.1, §8.3)
to remove the agent's network-capable tools.

### 6.3 Local desktop — Docker / Podman

On a local macOS/Windows/Linux machine, use a desktop container runtime.
Operon already integrates Docker (`extensions.rs`: `docker_container_action`,
`docker_list_containers`, `docker_list_images`, `docker_list_volumes`).
**Podman** is preferred where available because it is rootless by default.

```bash
docker run --rm \
  --network none \                              # full network isolation (works well here)
  --user "$(id -u):$(id -g)" \                  # do not write root-owned files
  --read-only \                                 # container root FS read-only
  --tmpfs /tmp \                                # writable scratch in memory
  --cap-drop ALL \
  --security-opt no-new-privileges \
  -v /Users/me/project/sanitized:/data:ro \
  -v /Users/me/project/run-workspace:/work \
  -w /work \
  operon-agent:latest \
  claude -p --verbose --output-format stream-json --dangerously-skip-permissions ...
```

Notes and honest caveats:

- **`--network none`** gives true network isolation locally — a genuine
  advantage over the HPC path.
- On **macOS**, Docker Desktop runs the container inside a Linux VM; bind
  mounts cross a virtualization layer (virtiofs/gRPC-FUSE). Read-only scanning
  of data is acceptable but not native speed.
- **There is no way on macOS to give a single process a private read-only
  mount without a container runtime.** Local sandboxed mode therefore has a
  *hard dependency* on Docker/Podman being installed. Operon must detect this
  and surface a clear message if absent.
- `--read-only` + `--tmpfs /tmp` makes everything except `/work` and `/tmp`
  immutable.

### 6.4 The agent container image

The image (`operon-agent.sif` / `operon-agent:latest`) must contain a working
Claude Code CLI. Building it once is a prerequisite.

Contents:

- A minimal base (e.g. `debian:stable-slim` or a distroless Node base).
- **Node.js LTS** + **Claude Code** installed at build time
  (`npm install -g @anthropic-ai/claude-code`, or the standalone binary from
  `claude.ai/install.sh`).
- `ripgrep` (the agent's `Grep` tool uses it; Operon already bundles `rg`).
- Any read-only analysis tooling the agent is allowed to *suggest* with (it
  does not need to *run* it if combined with the no-execute gate, but having
  `python`/`R` available lets the agent self-check syntax if execution against
  sanitized data is permitted — see §8.2).

Critical subtlety — the **npx-alias problem**: on HPC, `claude` is frequently a
shell alias resolving to `npx @anthropic-ai/claude-code` (documented in the
project's "Known Gotchas"). Inside `--cleanenv --contain`, that alias and the
user's shell configuration **do not exist**. The image must therefore contain
`claude` as a *real executable on `PATH`*. Do not rely on the host alias.

Image distribution:

- **HPC:** the `.sif` is a single file; place it on the shared filesystem.
  Some clusters require admin-vetted images — confirm policy (§11).
- **Local:** pull/built `operon-agent:latest` from a registry or built locally.
- **Versioning:** the image tag should be pinned and recorded in the audit log
  (§9) so a run is reproducible.

### 6.5 Streaming integration

Operon's value depends on streaming the agent's NDJSON output back to the UI.
This must keep working through the container boundary.

- **HPC terminal mode:** the agent's stdout is redirected to
  `/work/.operon-SESSION.jsonl`. Because `/work` is a bind to a directory on
  the **shared filesystem**, the existing login-node SSH tail reads that file
  exactly as it does today. **No change to the tail/streaming code.** The
  `.done` sentinel file is likewise written into `/work`.
- **Local mode:** `docker run` / `singularity exec` stdout is the agent's
  stdout; Operon captures it directly as it does for a bare process.
- The `--settings` guard file and any plan file (`implementation_plan.md`)
  must live under `/work` so they are both writable and visible to the agent.

### 6.6 Credentials inside the container

The agent needs an Anthropic API key or OAuth token. With `--cleanenv` the host
environment is dropped, so the credential must be injected explicitly:

- **Preferred:** pass `ANTHROPIC_API_KEY` via `--env ANTHROPIC_API_KEY=...`
  (Singularity) / `--env` (Docker) at exec time, sourced from Operon's existing
  secret store (in-memory dev / macOS Keychain prod). The key is never baked
  into the image and never written to the sanitized or raw filesystems.
- **Alternative:** bind a credentials file read-only at a fixed path. This
  leaves the token on disk inside `/work`; less preferred.
- The credential is visible to the agent process (it must be, to call the API).
  This is unavoidable and not a regression versus today.

---

## 7. Part A — sanitization

The mount (Part B) enforces *read-only* and *absence of raw data*. It does not
decide *what* the agent sees. Part A produces the sanitized tree that sits
behind the read-only bind.

### 7.1 Principle

Sanitization is **domain-specific** and is ultimately a **human policy
decision**. Operon supplies the *mechanism* and sensible defaults; the lab
supplies the *rules* (which columns are identifiers, which must be dropped,
how much data the agent needs). See §7.6.

### 7.2 Sanitization levels

The project chooses one level (or per-file overrides):

| Level | What the agent sees | When to use |
|-------|---------------------|-------------|
| **L0 — Schema only** | Data files are *replaced* by metadata stubs: column names, dtypes, shapes, row counts, value ranges for non-sensitive columns. No actual values. | Strictest. The agent authors scripts from structure alone. |
| **L1 — Redacted** | Real data, but configured *sensitive columns* are dropped or pseudonymized (salted hash); identifiers masked; quasi-identifiers generalized (age→band, date→year). | The agent needs realistic structure/values for non-sensitive fields. |
| **L2 — Subsampled / synthetic** | A small representative subsample (post-redaction) **or** fully synthetic data matching the schema. | The agent should be able to smoke-test scripts (requires §8.2). |

L0 is the recommended default for the faculty concern as stated. L1/L2 trade
strictness for agent usefulness and should be opt-in per project.

### 7.3 File-type handlers

Large binary data is **never copied**. The handler extracts a metadata sidecar.

| File type | L0 handler (schema stub) | L1/L2 handler |
|-----------|--------------------------|---------------|
| `.csv` / `.tsv` (sample sheets, metadata) | Emit header + dtypes + row count + per-column cardinality | Drop/hash configured columns; optionally cap rows |
| `.h5ad` / `.h5` / `.loom` (AnnData/scanpy) | Emit `obs`/`var` column names + dtypes, `X` shape, `obsm`/`layers` keys | Subsample cells; strip sensitive `obs` columns (donor ID, diagnosis, age, sex, PMI) |
| `.rds` / Seurat objects | Emit object class + assay names + dimensions + metadata column names (requires R) | Subsample; strip `meta.data` columns |
| `.bam` / `.cram` | `samtools view -H` header only; note `@RG` may carry sample names — scrub | Header only; not subsampled |
| `.fastq(.gz)` | Read count + read-length distribution; **no reads** | First N reads with read-name identifiers stripped |
| `.vcf(.gz)` | `##` header + `#CHROM` line + variant count; **no genotypes** | Drop sample genotype columns |
| Generic binary | Filename, size, mtime, magic-byte type only | Same |
| Source code / notebooks / `.md` | Passed through unchanged (not "data") | Passed through |

The distinction *code vs. data* matters: the agent generally *should* see the
project's scripts and READMEs (that is how it helps); it should not see the
data values. The handler table encodes that split.

### 7.4 Sanitization configuration schema

Stored per project (proposed `data_policy` block in Operon settings):

```jsonc
{
  "data_policy": {
    "enabled": true,
    "raw_data_dir": "/data/project/raw",
    "sanitized_dir": "/data/project/sanitized",
    "level": "L0",                       // L0 | L1 | L2
    "sensitive_columns": [               // matched case-insensitively
      "donor_id", "subject_id", "patient", "mrn", "name",
      "diagnosis", "age", "sex", "pmi", "date_of_*"
    ],
    "id_strategy": "hash",               // hash | drop | keep   (L1/L2)
    "id_salt_ref": "keychain://operon/data-policy-salt",
    "row_cap": 500,                      // L2 subsample size
    "passthrough_globs": ["**/*.py", "**/*.R", "**/*.md", "**/*.ipynb"],
    "data_globs": ["**/*.csv", "**/*.h5ad", "**/*.bam", "**/*.fastq*"]
  }
}
```

### 7.5 The `prepare` step is a job, not an instant copy

For realistic datasets, sanitization is a batch job:

- L0 schema extraction over many large `.h5ad`/`.bam` files is I/O-bound and
  can take minutes to hours.
- It should be runnable as a **SLURM job** on HPC (Operon already constructs
  SLURM submissions) and as a background task locally.
- Output: the `sanitized_dir` tree **plus** a `MANIFEST.json` recording, for
  each source file, its path, size, sha256, the handler applied, the level, and
  the timestamp. The manifest is the audit record of *what was exposed*.
- The sanitized snapshot should be **content-hashed and versioned** so a given
  agent session can be tied to an exact sanitized state (reproducibility, §11).

### 7.6 Sanitization correctness is a human responsibility — explicit non-guarantee

> **The mechanism cannot guarantee the sanitized data is free of sensitive
> content.** Operon does not know that a free-text `notes` column contains a
> patient name, or that a barcode encodes a collection date. Operon provides:
> default sensitive-column patterns, a dry-run diff/preview before the agent is
> ever launched, and the manifest. The **lab must review and sign off** on the
> sanitization policy and the preview. This is concern **C6** and it is
> deliberately out of scope for the automated mechanism. Reviewers should treat
> the sign-off as a required process control, not a software feature.

---

## 8. Composition with other controls

The mount addresses C1–C3. The remaining concerns are handled by *composable*
controls that stack on top.

### 8.1 The capability gate (C4 — execution)

Operon already has the `PreToolUse`-hook + `--disallowedTools` machinery (the
deletion guard). In sandboxed mode it is extended to optionally:

- `--disallowedTools "Bash WebFetch WebSearch"` — remove the execute and
  network-fetch tools entirely.
- A `PreToolUse` hook on `Bash` that always denies (defense in depth).

This makes the agent **advisory**: it reads the sanitized data and the code,
and *authors* scripts (via `Write`/`Edit` into `/work`), but does not run them.
The human reviews and runs — see §8.4.

### 8.2 Optional: permit execution *inside* the sandbox (C4 relaxed)

Because the mount makes raw data absent and read-only, **executing code inside
the container against sanitized data is safe by construction** — the blast
radius is `/work` only. If the lab wants the agent to smoke-test the scripts it
writes, sandboxed mode can *re-enable* `Bash` *inside the container*. This is a
strictly more flexible position than the pure capability gate, and is a
genuine advantage of the mount-based approach. It pairs naturally with
sanitization level L2 (subsampled/synthetic data the agent can actually run on).

### 8.3 Network isolation (C5 — exfiltration)

- **Local (Docker/Podman):** `--network none` — solved.
- **HPC (Singularity):** unprivileged Singularity cannot easily drop the
  network namespace. Mitigate at the capability layer: remove `WebFetch`/
  `WebSearch`; with `Bash` also removed (§8.1) the agent has no general network
  egress. If stronger guarantees are required, request an admin network
  profile or run on a node with egress firewalling.

### 8.4 Human-in-the-loop execution

The agent writes `/work/proposed/analysis.sh`. To keep friction low, Operon
adds a **"Review & Run"** affordance: the *human* inspects the script (Operon's
existing DiffViewer / editor) and clicks to run it — outside the sandbox, in a
normal terminal, against the real data. AI authors; human executes. The air-gap
intent is preserved while the workflow stays one click long.

---

## 9. Operon integration

### 9.1 Settings

- New `data_policy` block (§7.4), per project.
- A global toggle and a per-session **mode** value: alongside the existing
  terminal / plan / agent modes, add **`sandboxed`**.

### 9.2 New Rust commands (proposed)

| Command | Responsibility |
|---------|----------------|
| `prepare_sanitized_mount` | Run Part A: scan `raw_data_dir`, apply handlers, populate `sanitized_dir`, write `MANIFEST.json`. Local or over the SSH exec channel. May submit a SLURM job. |
| `preview_sanitization` | Dry run: report, per file, what handler would apply and what would be dropped/hashed — for the human sign-off (§7.6). |
| `check_container_runtime` | Detect Singularity/Apptainer (remote) or Docker/Podman (local) and version; surface a clear error if absent. |
| `verify_sandbox_binds` | Pre-flight: confirm the cluster's default binds (§6.2.3) do not expose `raw_data_dir`. |

### 9.3 Launch path — `claude.rs`

`start_claude_session` gains a `sandboxed` branch. Instead of emitting
`claude ...`, it emits the `singularity exec ...` / `docker run ...` wrapper
(§6.2.1 / §6.3) with the computed bind list. The change is *localized to
command construction*; the streaming, session-metadata, and resume logic are
unchanged because `/work` is on the shared filesystem (§6.5). The existing
guard `--settings` plumbing is reused, with the guard files written under
`/work`.

### 9.4 UI

- A **mode badge** ("Sandboxed — read-only sanitized data") visible whenever
  the mode is active, so it is never silently off.
- A panel showing the **exact bind list** and the sanitized-snapshot version/
  hash for the current session — the user can see precisely what was exposed.
- The sanitization **preview/sign-off** dialog before first launch.
- The **"Review & Run"** control (§8.4).

### 9.5 Audit log

Every sandboxed session records: timestamp, user, host, image tag, the full
bind list, the sanitized-snapshot hash, the `MANIFEST.json` reference, the
mode/flags, and any capability-gate denials. Written to `~/.operon/audit/`.
This is the artifact a faculty reviewer or IRB would ask for.

---

## 10. Implementation phasing

Proposed as **Phase 13** of the Operon roadmap, in three sub-phases:

- **Phase 13a — Mount MVP.** Container image build; `check_container_runtime`;
  `verify_sandbox_binds`; the wrapped launch path in `claude.rs`; streaming
  verified through the container. The user *manually* prepares a sanitized
  directory; Operon only does the read-only-mount launch. This alone satisfies
  C1–C3 for users willing to sanitize by hand.
- **Phase 13b — Sanitization pipeline.** `prepare_sanitized_mount` +
  `preview_sanitization`; file-type handlers (§7.3); the `data_policy` schema;
  SLURM-job mode for large datasets; `MANIFEST.json`.
- **Phase 13c — UX & governance.** Mode badge, bind-list panel, sign-off
  dialog, "Review & Run", audit log, snapshot versioning.

Each sub-phase is independently shippable.

---

## 11. Open questions for reviewers

1. **Container runtime availability.** Which clusters are in scope (HPC3/RCIC,
   others)? Is Singularity or Apptainer installed, and at what version?
   (`--no-mount` and some isolation flags require recent versions.)
2. **Default binds.** What does the target cluster's `singularity.conf` /
   `apptainer.conf` force-bind, and does any of it overlap the raw-data
   filesystem? (§6.2.3 — this can defeat the design.)
3. **Image policy.** Does the cluster permit user-supplied `.sif` images, or
   only admin-vetted ones? If the latter, the `operon-agent.sif` must be
   submitted for vetting.
4. **Network.** Is C5 (exfiltration) in scope? If so, is the capability-gate
   mitigation acceptable, or is a true network-isolated execution profile
   required from the cluster?
5. **Sanitization sufficiency.** Is L0 (schema-only) too restrictive for the
   agent to be useful in practice? Should the default be L1? This needs a
   pilot evaluation.
6. **Sanitization ownership.** Who in the lab owns and signs off on the
   sanitization policy and the preview (§7.6)? Is there an IRB/data-use
   agreement that constrains even the *sanitized* copy?
7. **Reproducibility.** Should every agent session pin the sanitized-snapshot
   hash and image tag, and should snapshots be retained for audit?
8. **Quota and storage.** The sanitized copy and run-workspace consume shared
   storage. Where do they live, under whose quota, and what is the retention
   policy?
9. **Credential handling.** Is injecting `ANTHROPIC_API_KEY` via `--env`
   acceptable to the security reviewers, or is a mounted credential file (or a
   broker) required?
10. **Threat-model boundary.** This design protects *data from the agent*. It
    does not protect the *host from the agent* or address a compromised host.
    Is that boundary acceptable, or is host-level hardening also expected?

---

## 12. Summary of guarantees

| Property | Guarantee | Enforced by |
|----------|-----------|-------------|
| Agent cannot **modify** sanitized data | Strong (kernel) | `:ro` bind mount |
| Agent cannot **see** raw data | Strong (kernel) | Raw path not in the namespace |
| Agent cannot **delete** raw data | Strong (kernel) | Raw path absent |
| Agent cannot **execute** code | Optional/strong | `--disallowedTools` + `PreToolUse` hook |
| Agent cannot reach the **network** | Strong locally, conditional on HPC | `--network none` / capability gate |
| Sanitized data is **free of sensitive content** | **No software guarantee** | Human policy + sign-off (§7.6) |
| Host is protected from a compromised agent | **Out of scope** | Cluster responsibility |

The design satisfies the faculty's stated requirement — a read-only sanitized
data mount in which the agent cannot see or alter the real data — using
kernel-enforced namespace isolation, while being honest about the two things
it deliberately does not guarantee: sanitization correctness, and host
security.

---

*End of specification. Comments and review feedback welcome — this is a draft.*
