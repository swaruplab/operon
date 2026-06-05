# Code editor

Operon uses the **Monaco Editor** — the same engine that powers VS Code.

## Features

- **30+ languages** with syntax highlighting: Python, R, Bash, Nextflow,
  Snakemake, WDL, JSON, YAML, TOML, Markdown, CSV, TSV, and more
- **Tabs** for multiple open files
- **Diff viewer** with accept/reject for AI-generated edits
- **Image and PDF viewer** with zoom, download, expand
- **Find and replace** (++cmd+f++ / ++ctrl+f++ for find, ++cmd+alt+f++ /
  ++ctrl+h++ for replace)
- **Multi-cursor** editing
- **Bracket / quote auto-pairing**
- **Save with `Cmd/Ctrl+S`**, auto-save optional

## Themes

Two bundled themes that follow the global [light/dark toggle](theme.md):

- `operon-dark` — calm zinc-based dark theme
- `operon-light` — high-contrast paper-white light theme

You can pick a third-party Monaco theme by setting it in Settings, but most
users stick with the bundled pair.

## Diff viewer

When Claude proposes a file edit, it appears as a side-by-side diff. You can:

- **Accept** the change (writes to disk)
- **Reject** the change (no write)
- **Accept and keep editing** to tweak before writing

Diffs preserve syntax highlighting in both panes.

## File viewer (images & PDFs)

Operon's editor area handles non-text files too:

| File type | What you can do |
|---|---|
| PNG, JPG, WEBP, SVG | Zoom, pan, expand to full window, download |
| PDF | Page navigation, zoom, download |

Useful for inspecting plots and figures without leaving the IDE.

## Supported file types

The editor recognizes any of these and auto-applies the right language mode:

| Category | Extensions |
|---|---|
| **Python** | `.py`, `.pyi`, `.pyx` |
| **R** | `.R`, `.r`, `.Rmd` |
| **Shell** | `.sh`, `.bash`, `.zsh` |
| **Pipelines** | `.nf` (Nextflow), `.smk` (Snakemake), `.wdl`, `.cwl` |
| **Config** | `.json`, `.yaml`, `.yml`, `.toml`, `.ini`, `.cfg`, `.env` |
| **Web** | `.ts`, `.tsx`, `.js`, `.jsx`, `.css`, `.html` |
| **Docs** | `.md`, `.rst`, `.txt` |
| **Data** | `.csv`, `.tsv`, `.parquet` (preview only) |
| **Notebooks** | `.ipynb` (read-only; edit via Claude with the notebook MCP) |

## Customization

Settings (++cmd++/`,` or ++ctrl++/`,`) → **Editor**:

| Setting | Default | What |
|---|---|---|
| Font size | 14 | Editor and diff font size |
| Font family | system monospace | Pick any installed mono font |
| Tab size | 4 | Spaces per tab (does not change file content; affects display) |
| Word wrap | off | Soft line wrap |
| Line numbers | on | Show line numbers |
| Minimap | on | The little overview strip on the right edge |
| Auto-save | off | Save after each change |
