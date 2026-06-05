# Changelog

All notable changes to Operon are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project
follows [Semantic Versioning](https://semver.org/).

## [0.7.3] — 2026-06-05

Stops stale shell-profile env vars from hijacking provider routing — the
"x-portkey-provider header required" 400 you'd see on an Anthropic-direct
session, and the silent 401/anonymous failures on Ollama / vLLM endpoints,
both came from the same root cause.

### Fixed

- **Anthropic-direct sessions hitting Portkey** (`src-tauri/src/commands/
  claude.rs`). When the user picked Provider = Anthropic, Operon set
  `ANTHROPIC_API_KEY` but never cleared `ANTHROPIC_BASE_URL` /
  `ANTHROPIC_AUTH_TOKEN`. If those were exported in `~/.zshrc` /
  `~/.bash_profile` (common after any prior Portkey or custom-endpoint
  tinkering), `bash -l` re-exported them inside the spawn and Claude
  Code's SDK routed `/v1/messages` to the wrong gateway. Portkey then
  rejected the call with `400 Either x-portkey-config or
  x-portkey-provider header is required`.
- **Custom / Ollama / vLLM endpoints returning 401** — symmetric bug.
  Picking Custom set `ANTHROPIC_BASE_URL` + `ANTHROPIC_AUTH_TOKEN` but
  left a stale `ANTHROPIC_API_KEY` in place; the SDK preferred
  `x-api-key` over `Authorization: Bearer`, and bearer-only proxies
  treated the request as anonymous.

### Changed

- New helper `ai_provider_env_unset()` returns the list of vars to clear
  for the active provider:
  - `anthropic` → `ANTHROPIC_BASE_URL`, `ANTHROPIC_AUTH_TOKEN`
  - `custom` / `portkey` → `ANTHROPIC_API_KEY`
- `ai_provider_env_exports()` now emits `unset X Y;` before the
  `export`s, so even profile-re-exported vars are cleared.
- All four spawn paths (local, remote terminal mode, remote SSH
  headless, remote tail) updated to also call `cmd.env_remove()` on the
  inherited env — belt-and-suspenders against profile-less subshells.

## [0.7.2] — 2026-06-05

Catalog cleanup + protocol-card display fix on top of the v0.7.1 import
batch. Net: 706 → 665 protocols, all bio/clinical/scientific-computing
oriented, with proper titles + descriptions rendered from YAML frontmatter.

### Removed

- **41 protocols** with no bioinformatics connection, including:
  - **geomaster** — geospatial / satellite imagery (Sentinel, Landsat, etc.)
  - **Non-bio scientific computing** imports: `aeon`, `astropy-astronomy`,
    `geopandas-geospatial`, `matlab-scientific-computing`, `pymatgen`
    (materials), `pymoo` (generic optim), `simpy-discrete-event-simulation`,
    `sympy-symbolic-math`, `uspto-database` (patents), `vaex-dataframes`
  - **Originals also pure-generic**: `geopandas`, `matlab`, `uspto-database`,
    `vaex`, `dask`, `polars`, `scikit-learn`, `networkx`, `pytorch-lightning`,
    `torch-geometric`, `transformers`, `shap`, `zarr-python`, `modal`,
    `pymc`, `statsmodels`, `pymoo`, `statistical-analysis`
  - **Generic search / academic**: `openalex-database` (all-disciplines
    scholarly DB — keep PubMed/bioRxiv instead), `perplexity-search`,
    `scholar-evaluation`
  - **Generic compute-host**: `modal`
- **All large data bundles** inside imported skills (~12MB freed):
  - `*/repo/src/db/chroma_squidpy_db/` (60MB vector DB shipped in two
    spatial-transcriptomics skills — purged from both copies)
  - `*/repo/dataset/` TREC corpora in `trialgpt-matching`
  - `*/data/GPTCellType/datasets/` in `cellagent-annotation`
  - `.gitignore` rule added so future protocol imports drop them automatically

### Fixed

- **Protocol cards rendering as "COPYRIGHT NOTICE"**
  (`src-tauri/src/commands/files.rs`). OpenClaw skills begin with an HTML
  comment block (`<!-- # COPYRIGHT NOTICE ... -->`) followed by YAML
  frontmatter (`---\nname: …\ndescription: …\n---`). The legacy parser
  picked the `# COPYRIGHT NOTICE` line *inside* the HTML comment as the
  protocol title, and the literal `<!--` as the description. New parser:
  - Strips leading HTML comments before scanning
  - Reads YAML frontmatter `name` + `description` when present
  - Falls back to first real `# heading` only after the preamble
- **`detect_category`** rewritten with a bio-first taxonomy (`src-tauri/
  src/commands/files.rs`) covering: single-cell, spatial, chromatin (ATAC/
  ChIP/Hi-C), bulk RNA, CRISPR, cytometry, epigenetics, immunology,
  microbiome, liquid biopsy, population/variants, copy number, genome
  assembly, phylogenetics, sequence I/O, proteomics/structural, drug
  discovery, metabolomics, systems biology, medical imaging, clinical,
  lab automation, databases, bio agents, ML compute, statistics,
  visualization, writing, research. "Other" went from ~400 protocols
  (most of v0.7.1's catalog) to 0.

### Changed

- **Protocol category labels + icons** in `ProtocolsView.tsx` updated to
  match the new bio-first taxonomy. 30 themed categories, best-first
  ordering, lucide icons appropriate to each domain (Dna, Microscope,
  Stethoscope, Atom, Pill, Bug, ScanLine, etc.).
- **Help panel** "Bundled bioinformatics protocols" item updated to
  reflect the new count (665) and curation criteria.

## [0.7.1] — 2026-06-05

Protocol catalog overhaul. Removed non-bioinformatics protocols that had
slipped in (finance, quantum, materials science) and merged curated bio
skills from three upstream skill repositories. Net: 192 → 706 bundled
protocols, all bio/clinical/scientific-computing oriented.

### Added

- **OpenClaw Medical Skills** (`FreedomIntelligence/OpenClaw-Medical-Skills`).
  150 clear-bio top-level skills imported as Operon protocols. Highlights:
  AlphaFold/Boltz/Chai structure prediction; antibody-design-agent;
  autonomous-oncology-agent; bindcraft / binder-design; bone-marrow-ai-agent;
  cell-free-expression; ChEMBL search; ChemCrow drug discovery;
  cellular-senescence-agent; clinical-diagnostic-reasoning; chatehr-clinician-assistant;
  many more. 395 fine-grained `bio-*` sub-skills (per-step ATAC/ChIP/alignment
  workflows) were intentionally skipped to keep the flat catalog navigable;
  the broader-scope skills cover the same territory.
- **bioSkills** (`GPTomics/bioSkills`). 345 skills across 49 categories
  Operon previously lacked, imported with a category prefix to keep names
  unambiguous and the catalog flat. New themes include `variant-calling-*`
  (13), `crispr-screens-*` (15), `comparative-genomics-*` (13),
  `chip-seq-*` (12), `clip-seq-*` (12), `hi-c-analysis-*` (9),
  `flow-cytometry-*` (8), `tcr-bcr-analysis-*` (5), `metagenomics-*` (7),
  `microbiome-*` (6), `ribo-seq-*` (5), `long-read-sequencing-*` (8),
  `methylation-analysis-*` (5), `epitranscriptomics-*` (5),
  `epidemiological-genomics-*` (5), `multi-omics-integration-*` (4),
  `liquid-biopsy-*` (6), `immunoinformatics-*` (5), `phasing-imputation-*` (4),
  `pathway-analysis-*` (6), `clinical-biostatistics-*` (12),
  `experimental-design-*` (5), `causal-genomics-*` (11),
  `temporal-genomics-*` (5), `population-genetics-*` (6),
  `primer-design-*` (3), `genome-engineering-*` (5),
  `restriction-analysis-*` (4), and many more.
- **SciAgent Skills** (`jaechang-hits/SciAgent-Skills`). 41 skills across
  the three categories Operon didn't cover: `lab-automation-*` (Benchling,
  Opentrons, ProtocolsIO, PyLabRobot integrations + Western-blot
  quantification), `medical-imaging-*` (histolab WSI, Imaging Data Commons,
  nnU-Net segmentation, OMERO, pathml, pydicom, scikit-image), and
  `scientific-computing-*` (29 numerical + ML skills: PyTorch Lightning,
  numpyro, jax, scipy, statsmodels, etc.).

### Removed

- **22 non-bioinformatics protocols** that had no clear connection to the
  Swarup-Lab use case. Finance (`alpha-vantage`, `edgartools`,
  `fred-economic-data`, `hedgefundmonitor`, `market-research-reports`,
  `usfiscaldata`), quantum computing (`cirq`, `qiskit`, `qutip`,
  `pennylane`), physical sciences (`astropy`, `fluidsim`, `pymatgen`,
  `rowan`), general RL/ML (`stable-baselines3`, `pufferlib`, `aeon`,
  `timesfm-forecasting`, `consciousness-council`), math/sim (`simpy`,
  `sympy`, `what-if-oracle`).

### Notes

- **Catalog size**: 192 → 706 protocols. The bundled-protocols help item
  has been updated to reflect new totals and the upstream skill sources.
- **Naming**: bioSkills + SciAgent skills are imported with their category
  as a prefix (e.g. `hi-c-analysis-cooler-loading`, `lab-automation-pylabrobot`)
  so the flat Operon model is preserved without name collisions.
- All imports respect skip-if-exists: any pre-existing Operon protocol with
  the same name was kept and the upstream version was not copied over.

## [0.7.0] — 2026-06-04

A big release. Three headline themes: a new **Portkey gateway provider** for
institutional and managed AI gateways (with UCI ZotGPT as the flagship
preset); a full **light / dark theme** with smooth runtime swap across every
panel including Monaco syntax, xterm palette, and the native macOS title
bar; and **twelve new bundled single-cell protocols** (now stackable up to
4 at once). Plus the proxy-sidecar registration fix that unblocked the
custom-provider path for Ollama/vLLM users, and an Intel-Mac binary for the
bundled translation proxy.

### Added

- **Portkey gateway provider** (`src-tauri/src/commands/portkey.rs`,
  `src/lib/portkey.ts`, `src/components/settings/SettingsPanel.tsx`). Third
  AI provider option alongside Anthropic Direct and Custom OpenAI-Compatible.
  Bundled presets for **UCI ZotGPT Gateway** (institutional, P3-compliant,
  IRB-friendly) and **Portkey Cloud** (pay-as-you-go). Preset manifest at
  `presets/portkey.json` is fetched from GitHub at launch with a 7-day cache,
  so new institutional presets propagate to existing installs without an
  Operon release. Add yours via PR.
- **Portkey model catalog auto-fetch + grouping**. Pasting a virtual key
  auto-calls `/v1/models` and renders results grouped by family
  (Anthropic / Google / Moonshot / Meta / Mistral), best-first within each
  family. Auto-picks the best Claude on first connect; falls back to preset
  suggestions if the gateway returns nothing. Workspace hint (e.g. "bedrock",
  "vertex") shown next to each model.
- **Smart routing for non-Anthropic Portkey models**. Anthropic-family
  models go direct to Portkey's `/v1/messages` passthrough. Non-Anthropic
  (Kimi, Gemini, Llama, …) is auto-routed through the bundled `anthropic-proxy`
  sidecar so Claude Code's Anthropic-format requests are translated to OpenAI
  Chat Completions transparently. Purple badge under the model dropdown
  confirms when the proxy is active.
- **Light / dark theme with smooth runtime swap**
  (`src/context/ThemeContext.tsx`, `src/styles.css`, `tailwind.config.js`).
  Sun/Moon toggle in the top bar; persists to `settings.theme`; supports
  `dark` / `light` / `system` (follows OS `prefers-color-scheme` live).
  Implementation: semantic palette (`bg-canvas` / `bg-panel` / `text-primary`
  / `border-default` …) wired to CSS variables that flip via a `light`
  class on `<html>`. 31 files swept from `zinc-*` to semantic classes; 144
  accent text colors promoted with `dark:` variants for readable light mode.
- **Monaco `operon-light` theme** (`src/components/editor/CodeEditor.tsx`).
  Darker syntax hues (violet-600 keywords, blue-700 functions, etc.) tuned
  for contrast against white. Live swap via `setTheme` on toggle. Applies to
  both `CodeEditor` and `DiffViewer`.
- **xterm.js theme swap with WebGL atlas refresh**
  (`src/components/terminal/TerminalInstance.tsx`). Light palette uses darker
  ANSI hues (red-700, green-700, blue-700, …) for readability on white. On
  theme toggle the WebGL addon is disposed + recreated so the new colors take
  immediately on every cell — `options.theme` alone doesn't invalidate the
  texture atlas.
- **Native macOS title bar follows theme**. `titleBarStyle: "Overlay"` +
  `hiddenTitle: true` in `tauri.conf.json` so the webview extends under the
  traffic-light area; the themed top-bar background fills the space.
  `getCurrentWindow().setTheme(resolved)` syncs `NSWindow.appearance` as a
  belt-and-suspenders.
- **Twelve new single-cell protocols**, all bundled (`protocols/`):
  ArchR, CellBender, CellChat, hdWGCNA, kallisto-bustools, MRVI, ResolVI,
  Seurat, SingleCellExperiment, snapATAC2, spatial-transcriptomics,
  STELLAR Atlas. scVelo extended with dynamical-model and
  differential-kinetics references. Brings the bundled total to 192.
- **Multi-protocol stacking — cap raised from 2 → 4**. Pick up to four
  protocols simultaneously and Operon stacks their context for the
  conversation. Useful for combined workflows (e.g. scanpy + scvelo +
  cellchat in a single agent run).
- **Intel-Mac binary** for the bundled translation proxy
  (`src-tauri/binaries/anthropic-proxy-x86_64-apple-darwin`, v1.2.0 from
  upstream m0n0x41d/anthropic-proxy-rs, sha256-verified). Production
  builds now work on both Apple Silicon and Intel Macs.
- **Phase 13 design note** added to `CLAUDE.md` — Native Multi-Provider
  Agent Loop. Sketches the path to first-class GPT-5+ / o-series and other
  non-Claude-Code-format models without dependency on the Anthropic protocol
  shim. Currently OpenAI is temporarily hidden from the Portkey UI; comes
  back when Phase 13 lands.

### Changed

- **Tauri sidecar registration**. Added `binaries/anthropic-proxy` to
  `externalBin` in `tauri.conf.json` and `shell:allow-execute` in
  `capabilities/default.json`. Previously the translation proxy was silently
  rejected at spawn time (the silent `.catch(() => {})` in the start path
  swallowed the permission error), so the Ollama/vLLM custom-provider
  path was effectively broken for everyone. Now works end-to-end.
- **Help panel** (`src/components/help/HelpPanel.tsx`). New items: Light
  and dark theme (Getting Started), Portkey gateway provider (AI Providers),
  Bundled single-cell protocols (Protocols). The AI Providers overview now
  documents all three provider types (Anthropic / Portkey / Custom).
- **Beta-header suppression for Bedrock-backed Portkey routes**. When the
  Portkey provider routes Claude models via Bedrock, Claude Code's
  `anthropic-beta` headers (prompt-caching, extended-thinking, etc.) trigger
  Bedrock's "invalid beta flag" 400. `ai_provider_env` now sets
  `DISABLE_PROMPT_CACHING=1`, `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1`,
  `MAX_THINKING_TOKENS=0`, `ANTHROPIC_BETAS=""` for the Portkey direct path
  to suppress them. Loses prompt-caching and extended-thinking for that
  path; both come back as soon as Portkey/Bedrock accepts the flags.
- **Settings panel for Portkey**. Real-time proxy lifecycle: starting a
  non-Anthropic Portkey model auto-spawns the translation proxy; switching
  to a Claude model auto-stops it. Surfaces proxy-start errors inline
  (previously swallowed silently). Surfaces a Windows warning when picking
  a non-Anthropic Portkey model (the proxy sidecar is Unix-only).

### Fixed

- **Translation proxy never started** — root cause for
  "Ollama models don't respond" reports. `start_translation_proxy` would
  fail with a Tauri permission error because `binaries/anthropic-proxy`
  wasn't allowlisted; the frontend swallowed it. Now allowlisted and the
  start error surfaces in the settings UI if it ever recurs.
- **Path doubling for Portkey base URLs ending in `/v1`**. The Anthropic
  SDK appends `/v1/messages` to `ANTHROPIC_BASE_URL`; a user-stored
  `https://api.portkey.ai/v1` was producing `…/v1/v1/messages` which Portkey
  accepted with 200 + a malformed body, causing Claude Code's
  "empty or malformed response" error. The Portkey env builder now strips
  trailing `/v1` before handing the URL to the SDK.
- **Bedrock `requestMetadata` 400 for non-Anthropic models** — the
  `anthropic-proxy-rs` sidecar drops Anthropic's `metadata.user_id` during
  translation (it would otherwise carry a JSON blob containing `{`, `}`,
  `"` characters that violate Bedrock's regex). Routing non-Anthropic
  Portkey models via the proxy bypasses this; a silent fallback to the
  direct path no longer fires when the proxy is missing.
- **xterm theme didn't switch** — `options.theme = X` alone doesn't
  invalidate the WebGL texture atlas, so cells kept rendering in the old
  palette. Now disposes + recreates the WebGL addon on theme change.
- **Three hardcoded `bg-[#09090b]` in terminal components** that escaped
  the theming sweep (arbitrary-value syntax, not `bg-zinc-*`). Now use
  `bg-canvas` and theme correctly in light mode.

### Known limitations

- **OpenAI Portkey models hidden in UI** until Phase 13 lands. GPT-5+/o-series
  need OpenAI's Responses API which the bundled translation proxy doesn't
  speak. Workaround: use LiteLLM as a router with direct Azure credentials
  via the Custom provider.
- **Non-Anthropic Portkey models in remote/HPC sessions** not yet wired —
  the proxy currently runs only on the laptop. Anthropic-family Portkey
  models do work remotely (direct passthrough, no proxy needed).
- **Windows + Portkey non-Anthropic** — the proxy sidecar depends on
  Unix-only daemonize; Windows users see an inline warning suggesting
  LiteLLM/OpenRouter via the Custom provider as the workaround.

## [0.6.0] — 2026-05-01

This release focuses on **HPC reliability** (no more "are you working?" check-ins
when SSH hiccups), **office-document previews** in-app, and a **one-click
disconnect** so users can switch between remote servers without restarting the
app. It also lands the first release of Operon's plan-mode data-audit harness.

### Added

- **HPC watchdog** — built-in monitor for long-running jobs. Tracks SLURM (or
  any user-defined) job counts and surfaces a `total / running / pending /
  failed` chip in the status bar. New `JobsView` sidebar panel with per-profile
  detail, a Rust `watchdog` command module, and a remote `operon-watchdog.sh`
  helper script.
- **XLSX viewer** (`src/components/editor/XlsxViewer.tsx`) — open spreadsheets
  in-app via SheetJS with sheet tabs and a download button. Read-only.
- **PPTX viewer** (`src/components/editor/PptxViewer.tsx`) — slide-list preview
  via `pptx-preview`, with a download fallback for unsupported decks.
- **SSH stream heartbeat + auto-reconnect** — the remote tail script now emits
  `{"type":"heartbeat"}` every 30 s in a parallel subshell so legitimate quiet
  periods don't trip the stall watchdog. When the SSH stream goes silent for
  >60 s, the chat panel auto-invokes `reconnect_tail` up to 3 times before
  surfacing a user-visible warning. Eliminates the "are you working?"
  follow-ups during transient network drops.
- **Disconnect / switch server** — one-click teardown of all remote state
  (SSH ControlMaster, terminals, explorer, chat session, cached listings).
  Three entry points: (1) a green-dot **Unplug** button on the connected
  profile in the SSH view, (2) the **✕** next to the remote chip in the chat
  header, (3) a global remote-status chip with a disconnect icon in the bottom
  status bar. Centralized in `src/lib/disconnect.ts` via a
  `disconnect-remote` Tauri event that the sidebar, terminal area, and chat
  panel all listen for.
- **Remote attachment auto-upload** — pasted screenshots and picker-selected
  files are now SCP'd to `<remote_workdir>/.operon-attachments/` before the
  prompt is sent in remote mode. The agent's `Read` tool sees the file at a
  path it can actually access on the HPC server.
- **Plan-mode data audit harness** — `scripts/audit/run-audit.sh` plus
  `scripts/audit/mitm-addon.py` to drive mitmproxy with a canary-scanning
  add-on. Pair it with the seed dataset and a fresh Plan-mode session to
  produce a `canaries.tsv` summary of exactly which fields of the synthetic
  dataset crossed the wire to the Anthropic Messages API. Documented in
  `docs/audit/plan-mode-data-audit.md`.
- **Documentation site refresh** — new pages: Download, Guide, HPC,
  Protocols, MCP, Private LLM, Workshop. Index and Tutorials pages
  rewritten. New shared `docs/assets/site.{css,js}`.

### Changed

- **Status bar** now shows a global remote-connection chip whenever an SSH
  profile is active, with an inline disconnect affordance.
- **SSH view** — connected profile rows display a green status dot, a green
  background tint, and an always-visible (not hover-only) **Unplug** button
  for predictable disconnect.
- **Editor file icons** — new color and icon mappings for `.xlsx`/`.xls`/
  `.xlsm` (Sheet) and `.pptx`/`.ppt`/`.pptm` (Presentation).
- **`BinaryFileType`** union extended to `'image' | 'pdf' | 'html' | 'xlsx'
  | 'pptx' | null` so the file viewer can route binary previews uniformly.

### Fixed

- Pasted screenshots no longer fail in remote mode — paths are now rewritten
  to a remote location after SCP upload.
- Disconnecting a remote profile while a streaming Claude session is running
  now stops the session cleanly (`stop_claude_session` is invoked from the
  chat panel's `disconnect-remote` listener) instead of leaking the
  background tail process.

### Internal

- Version bumped from `0.5.3` (source) / `v0.5.10` (last tag) to `0.6.0`
  across `package.json`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`,
  and `src-tauri/Cargo.lock`.
- `.gitignore` now excludes regenerable artifacts (`graphify-out/`),
  Claude Code session state (`.claude/`, `memory/`), and shell dotfiles that
  occasionally land here from Dropbox sync.

## [0.5.10] — previous release

Terminal WebGL renderer toggle + atlas hardening. See git history for the full
0.5.x series.
