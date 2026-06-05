# Changelog

The full release-by-release changelog lives in
[CHANGELOG.md](https://github.com/swaruplab/operon/blob/cross-platform/CHANGELOG.md)
on GitHub.

Quick reference for the most recent releases:

## v0.7.3 — 2026-06-05

Stops stale shell-profile env vars from hijacking provider routing.

- Anthropic-direct sessions no longer routed to Portkey by a leftover
  `ANTHROPIC_BASE_URL` in `~/.zshrc` / `~/.bash_profile`
- Symmetric fix for Custom (Ollama / vLLM) endpoints where a stale
  `ANTHROPIC_API_KEY` was beating the bearer token and causing 401s
- New `ai_provider_env_unset()` helper clears the right vars per provider
- All four Claude spawn paths emit `unset X Y;` before exports so the
  clearing survives `bash -l` profile re-sourcing

## v0.7.2 — 2026-06-05

Protocol catalog cleanup + COPYRIGHT-NOTICE parser fix.

- 706 → 665 protocols after removing non-bio imports (geomaster, astropy,
  matlab generic, sympy, etc.)
- Protocol cards now render real titles + descriptions from YAML
  frontmatter — fixes "COPYRIGHT NOTICE" appearing as the title for
  OpenClaw-derived protocols
- `detect_category` rewritten with bio-first taxonomy — "Other" went
  from ~400 protocols to 0
- Protocol category labels + icons in ProtocolsView.tsx updated to match

## v0.7.1 — 2026-06-05

Protocol catalog overhaul.

- +536 protocols imported from 3 skill repos (OpenClaw, bioSkills, SciAgent)
- 22 non-bio protocols removed
- Large data bundles inside imported skills purged (~12MB freed):
  `chroma_squidpy_db/`, `dataset/`, `datasets/`
- `.gitignore` rules to prevent future bundle reimports

## v0.7.0 — 2026-06-04

Light/dark theme + Intel-Mac proxy + 12 single-cell protocols.

- Runtime light/dark toggle in the top bar — propagates to UI, Monaco,
  xterm WebGL atlas, and macOS native title bar
- 12 new single-cell protocols (scVI, doublet detection, CellChat, etc.)
- Bundled `anthropic-proxy-rs` sidecar for Intel-Mac users (the Rust
  proxy fixes the Rosetta-translation bug on aarch64-hosted x86 Operons)
- Initial Portkey provider support (UCI ZotGPT etc.)

## v0.6.x and earlier

See the full [CHANGELOG.md](https://github.com/swaruplab/operon/blob/cross-platform/CHANGELOG.md).

Highlights:

- **v0.6** — Windows + Linux releases (cross-platform branch)
- **v0.5** — HPC tmux mode + session resume
- **v0.4** — SSH remote, Git integration
- **v0.3** — Claude Code integration, protocol catalog
- **v0.2** — Monaco editor, integrated terminal
- **v0.1** — Initial Tauri scaffolding
