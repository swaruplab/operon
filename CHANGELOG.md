# Changelog

All notable changes to Operon are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project
follows [Semantic Versioning](https://semver.org/).

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
