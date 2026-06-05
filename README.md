<div align="center">

# Operon

**AI-powered IDE for bioinformatics — built by biologists, for biologists.**

[![MIT License](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Version](https://img.shields.io/github/v/release/swaruplab/operon?color=purple)](https://github.com/swaruplab/operon/releases/latest)
[![macOS](https://img.shields.io/badge/macOS-12%2B-black.svg?logo=apple)](https://github.com/swaruplab/operon/releases/latest)
[![Windows](https://img.shields.io/badge/Windows-10%2F11-0078D6.svg?logo=windows)](https://github.com/swaruplab/operon/releases/latest)
[![Linux](https://img.shields.io/badge/Linux-deb%20%2F%20rpm%20%2F%20AppImage-FCC624.svg?logo=linux&logoColor=black)](https://github.com/swaruplab/operon/releases/latest)
[![Tauri](https://img.shields.io/badge/Tauri_2-Rust_%2B_React-orange.svg)](https://tauri.app)
[![Protocols](https://img.shields.io/badge/protocols-665-green.svg)](#analysis-protocols)

Operon is a cross-platform desktop application that brings together an AI
coding assistant (Claude), integrated terminal, code editor, file browser, and
remote-server access into a single tool designed for computational biologists.
Whether you're running RNA-seq pipelines on an HPC cluster from your Windows
laptop, analyzing single-cell data on a Linux workstation, or writing scripts
on a Mac, Operon gives you a professional development environment with AI that
understands your domain.

[**Download**](https://github.com/swaruplab/operon/releases/latest) •
[**Documentation**](https://swaruplab.bio.uci.edu/operon) •
[**Release notes**](https://github.com/swaruplab/operon/blob/cross-platform/CHANGELOG.md)

<img src="docs/img/main-workspace.png" alt="Operon workspace" width="800">

</div>

---

## Table of Contents

- [Why Operon?](#why-operon)
- [Features](#features)
- [System Requirements](#system-requirements)
- [Installation](#installation)
  - [Download (pre-built installers)](#download-recommended)
  - [Build from Source](#build-from-source)
- [Getting Started](#getting-started)
  - [Setup Wizard](#setup-wizard)
  - [Authentication & AI Providers](#authentication--ai-providers)
- [Using Operon](#using-operon)
  - [Workspace Overview](#workspace-overview)
  - [File Explorer](#file-explorer)
  - [Code Editor](#code-editor)
  - [Integrated Terminal](#integrated-terminal)
  - [AI Chat — Four Modes](#ai-chat--four-modes)
  - [Analysis Protocols](#analysis-protocols)
  - [PubMed Integration](#pubmed-integration)
  - [Light & Dark Theme](#light--dark-theme)
- [Remote Server & HPC](#remote-server--hpc)
  - [SSH Connections](#ssh-connections)
  - [Running AI on Remote Clusters](#running-ai-on-remote-clusters)
- [Git Integration](#git-integration)
- [Keyboard Shortcuts](#keyboard-shortcuts)
- [Settings & Customization](#settings--customization)
- [Building for Distribution](#building-for-distribution)
- [Project Architecture](#project-architecture)
- [Contributing](#contributing)
- [Help & Support](#help--support)
- [License](#license)

---

## Why Operon?

Most IDEs are built for software engineers. Operon is built for **you** — the
biologist who writes Python scripts to process sequencing data, runs pipelines
on a shared HPC cluster, and needs to search PubMed while debugging a Scanpy
workflow. We built Operon because we needed it ourselves.

<table>
<tr>
<td width="33%" valign="top">

### Built for Biology
Understands bioinformatics file formats (FASTA, FASTQ, VCF, BAM, GFF),
common pipelines, and domain-specific best practices out of the box.

</td>
<td width="33%" valign="top">

### Four AI Modes
**Agent** executes multi-step tasks. **Plan** architects solutions.
**Ask** answers questions — with optional PubMed search.
**Report** produces a structured scientific writeup.

</td>
<td width="33%" valign="top">

### Remote HPC
SSH into university clusters, browse remote files, run AI agents directly
on compute nodes. Your data never leaves the server.

</td>
</tr>
<tr>
<td width="33%" valign="top">

### 665 Bundled Protocols
Curated bio-first analysis protocols spanning scRNA-seq, spatial,
chromatin, CRISPR, proteomics, drug discovery, and more. Create your own
with AI or write them in Markdown.

</td>
<td width="33%" valign="top">

### Multi-Provider AI
Anthropic direct, institutional **Portkey** gateways (e.g. UCI ZotGPT),
or any OpenAI-compatible local backend (Ollama, vLLM, LM Studio) via a
bundled translation proxy.

</td>
<td width="33%" valign="top">

### Native Performance
Built with Tauri 2 (Rust + React). Tiny bundle, 20–40 MB RAM. Uses your
system's native webview — not Electron. Ships for macOS, Windows, and
Linux.

</td>
</tr>
</table>

---

## System Requirements

| Platform | Minimum |
|---|---|
| **macOS** | macOS 12 (Monterey) or later · Apple Silicon (M1+) or Intel |
| **Windows** | Windows 10 (1809+) or Windows 11 · x64 · WebView2 runtime (pre-installed on Windows 11; auto-installed on 10) |
| **Linux** | x64 · glibc 2.31+ (Ubuntu 22.04+ / Fedora 36+ / Debian 12+) · `webkit2gtk-4.1` |
| **Disk** | ~500 MB including dependencies |
| **RAM** | 4 GB minimum, 8 GB recommended |
| **Internet** | Required for AI features and initial setup |

> **Note:** Developer dependencies (Claude Code, build toolchains where needed)
> are installed automatically by the setup wizard on first launch.

---

## Installation

### Download (Recommended)

The latest signed installers for every platform are on the
[**GitHub Releases**](https://github.com/swaruplab/operon/releases/latest) page.
A pre-built macOS DMG with notarization is also mirrored at
[swaruplab.bio.uci.edu/operon](https://swaruplab.bio.uci.edu/operon).

| Platform | Installer | Notes |
|---|---|---|
| **macOS — Apple Silicon** | `Operon_<version>_aarch64.dmg` | Right-click → Open the first time to bypass Gatekeeper |
| **macOS — Intel** | `Operon_<version>_x64.dmg` | Same Gatekeeper note |
| **Windows** | `Operon_<version>_x64-setup.exe` (NSIS) or `Operon_<version>_x64_en-US.msi` | SmartScreen may flag the binary as "Unrecognized app" — click "More info → Run anyway" |
| **Linux — Debian / Ubuntu** | `Operon_<version>_amd64.deb` | `sudo apt install ./Operon_*.deb` |
| **Linux — Fedora / RHEL** | `Operon-<version>-1.x86_64.rpm` | `sudo dnf install ./Operon-*.rpm` |
| **Linux — distro-agnostic** | `Operon_<version>_amd64.AppImage` | `chmod +x` then run directly |

### Build from Source

<details>
<summary><strong>Prerequisites by platform (click to expand)</strong></summary>

All platforms need **Node.js 18+**, **npm**, and **Rust** (via [rustup](https://rustup.rs/)).
Additional native toolchain per OS:

| OS | Native toolchain |
|---|---|
| **macOS** | Xcode Command Line Tools — `xcode-select --install` |
| **Windows** | [Visual Studio 2022 Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) with "Desktop development with C++"; [WebView2 runtime](https://developer.microsoft.com/microsoft-edge/webview2/) (pre-installed on Win 11) |
| **Linux (Debian/Ubuntu)** | `sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev` |
| **Linux (Fedora)** | `sudo dnf install webkit2gtk4.1-devel openssl-devel curl wget file libappindicator-gtk3-devel librsvg2-devel @"C Development Tools and Libraries"` |

</details>

```bash
# 1. Clone the repository
git clone https://github.com/swaruplab/operon.git
cd operon

# 2. Install frontend dependencies
npm install

# 3. Run in development mode (hot-reload)
npm run tauri dev

# 4. Build a production installer for your OS
npm run tauri build
# macOS  → src-tauri/target/release/bundle/{dmg,macos}/
# Win    → src-tauri/target/release/bundle/{msi,nsis}/
# Linux  → src-tauri/target/release/bundle/{deb,appimage,rpm}/
```

<details>
<summary><strong>All development & build commands</strong></summary>

| Command | What it does |
|---------|--------------|
| `npm run tauri dev` | Start dev server with hot-reload |
| `npm run build` | Build frontend only (TypeScript + Vite) |
| `npm run tauri build` | Full production build for the current host OS |
| `npm run build:mac:arm` | macOS Apple Silicon DMG |
| `npm run build:mac:intel` | macOS Intel DMG |
| `npm run build:mac:universal` | macOS universal binary (both arches) |
| `npm run build:win` | Windows installers (NSIS + MSI) |
| `npm run build:win:msi` | Windows MSI only |
| `npm run build:win:nsis` | Windows NSIS (.exe) only |
| `npm run build:linux` | Linux bundles (deb, AppImage, rpm) |
| `npm run build:linux:deb` | Linux .deb only |
| `npm run build:linux:appimage` | Linux AppImage only |

</details>

---

## Getting Started

### Setup Wizard

On first launch, Operon walks you through installing all dependencies automatically:

| Step | What's installed |
|------|------------------|
| 1 | Native toolchain check (Xcode CLT on macOS, WebView2 on Windows, system libs on Linux) |
| 2 | Claude Code (the AI engine) |
| 3 | Optional: GitHub CLI, conda, common bioinformatics CLIs |

<p align="center">
<img src="docs/img/setup-welcome.png" alt="Setup wizard welcome" width="500">
</p>

### Authentication & AI Providers

Operon supports three provider backends, picked from **Settings → Auth → Provider**:

| Provider | Use it for | How to authenticate |
|---|---|---|
| **Anthropic** (default) | Anthropic OAuth or a personal API key — talks directly to `api.anthropic.com` | "Log in with Claude" (browser OAuth) or paste an `sk-ant-...` key |
| **Portkey** | Institutional gateways (UCI ZotGPT, Bedrock-backed Claude, cost-tracked routing). Anthropic-family models pass through Portkey's `/v1/messages`; non-Anthropic models (Moonshot Kimi, GPT, Gemini) are auto-routed through Operon's bundled translation proxy. | Paste a Portkey virtual key; pick a model from the auto-loaded catalog |
| **Custom** | Any OpenAI-compatible local backend — Ollama, vLLM, LM Studio, llama.cpp server. Anthropic-format requests are translated to OpenAI Chat Completions by the bundled `anthropic-proxy` sidecar. | Enter the base URL (e.g. `http://localhost:11434/v1`); API key optional for most local backends |

<p align="center">
<img src="docs/img/auth-selection.png" alt="Provider selection in Settings → Auth" width="500">
</p>

---

## Using Operon

### Workspace Overview

The workspace has 5 main areas:

| Area | Description |
|------|-------------|
| **Activity Bar** (left edge) | Switch between File Explorer, SSH, Git, Protocols, Help |
| **Sidebar** | Context-sensitive panel for the active view |
| **Editor** (center) | Monaco-based code editor with 30+ language support |
| **Terminal** (bottom) | Integrated terminal with tab management |
| **AI Chat** (right) | Claude conversation panel with streaming responses |

<p align="center">
<img src="docs/img/tour-workspace.png" alt="Workspace overview" width="800">
</p>

### File Explorer

- Tree view with lazy directory loading
- Create, rename, delete files and folders
- Go-to-folder path bar (`Cmd/Ctrl+G`)
- Symlink-aware (local and remote)

<p align="center">
<img src="docs/img/file-explorer.png" alt="File explorer" width="300">
</p>

### Code Editor

- **Monaco Editor** — same engine as VS Code
- 30+ languages with syntax highlighting
- Custom `operon-dark` and `operon-light` themes
- Side-by-side diff viewer with accept/reject
- Image and PDF viewer with zoom & download

### Integrated Terminal

- Full terminal emulator powered by **xterm.js**
- Multiple tabs with independent sessions
- Auto-copy on selection
- WebGL rendering for performance
- Preserves your shell aliases and conda environments
- On Windows, Claude Code runs through bundled **Git Bash** so HPC-style
  Bash scripts work unchanged

### AI Chat — Four Modes

| Mode | Purpose | Best For |
|---|---|---|
| **Agent** | Executes multi-step tasks autonomously | Writing scripts, running pipelines, debugging |
| **Plan** | Generates an `implementation_plan.md` without executing; subsequent Agent runs track progress with `[x]` markers | Designing analysis workflows, architecture |
| **Ask** | Conversational Q&A with optional PubMed search | Literature review, explaining concepts |
| **Report** | Tool-restricted writeup mode that produces a structured methods/results section grounded in your project files | End-of-analysis reports, methods sections for papers |

<p align="center">
<img src="docs/img/tour-ai-modes.png" alt="AI modes" width="500">
</p>

### Analysis Protocols

Operon ships with **665 built-in protocols** organized into ~30 bio-first
categories:

<table>
<tr>
<td>

- Single-cell (Scanpy, Seurat, scVI)
- Spatial transcriptomics (Visium, MERFISH, Xenium)
- Chromatin (ATAC, ChIP, CUT&Tag, Hi-C)
- Bulk RNA-seq (DESeq2, edgeR, limma)
- CRISPR screens (MAGeCK, CRISPResso)
- Cytometry & immunology

</td>
<td>

- Population genetics & variants (GWAS, eQTL)
- Genome assembly & annotation
- Proteomics & structural (AlphaFold, ESM)
- Microbiome (16S, shotgun)
- Liquid biopsy & cfDNA
- Drug discovery & cheminformatics

</td>
<td>

- Database agents: PubMed, GEO, KEGG,
  GTEx, UniProt, JASPAR, AlphaFold,
  bioRxiv, ClinicalTrials
- Medical imaging & radiomics
- Lab automation
- Bio-aware coding agents

</td>
</tr>
</table>

When you select a protocol, its instructions are injected into Claude's
context. Claude then follows domain-specific best practices for that
analysis type.

**Create your own protocols:**
- **AI-Generated** — describe what you need in plain English and Claude writes
  the full protocol
- **Manual** — write Markdown in the built-in editor
- Stored in `~/.operon/protocols/` — shareable and version-controllable

<p align="center">
<img src="docs/img/protocols-list.png" alt="Protocols list" width="500">
</p>

### PubMed Integration

Toggle PubMed search in Ask mode. Claude searches NCBI's PubMed database,
retrieves relevant papers, and incorporates findings with proper citations.
No API key required — powered by NCBI E-utilities.

<p align="center">
<img src="docs/img/pubmed-toggle.png" alt="PubMed toggle" width="300">
</p>

### Light & Dark Theme

Toggle between **light** and **dark** themes from the top bar at any time.
The switch propagates everywhere — UI chrome, Monaco editor, xterm terminal
palette (WebGL atlas refresh included), and the native macOS title bar.

<p align="center">
<img src="docs/img/theme-toggle.png" alt="Light/dark theme toggle" width="500">
</p>

---

## Remote Server & HPC

### SSH Connections

Connect to remote servers and HPC clusters directly from Operon:

- Password and SSH key authentication
- **Duo/MFA support** for university clusters
- SSH connection multiplexing (fast, fewer auth prompts)
- Browse remote files, edit code, run commands
- Set up SSH keys directly from the app

<p align="center">
<img src="docs/img/ssh-connect.png" alt="SSH connection" width="500">
</p>

### Running AI on Remote Clusters

Operon can install Claude Code on your remote server and run AI sessions
directly on HPC infrastructure:

- **Agent mode works over SSH** — executes commands, writes scripts, submits
  SLURM/PBS jobs on the remote machine
- **Data never leaves the server** — only terminal I/O travels over SSH
- Runs inside **tmux sessions** that persist across app restarts
- Output files written to shared filesystem (not node-local `/tmp`)

> **This is the killer feature for computational biologists:** run Claude
> directly on your university's HPC cluster with access to your data,
> your conda environments, and your SLURM queue.

<p align="center">
<img src="docs/img/tour-hpc.png" alt="HPC remote workflow" width="500">
</p>

---

## Git Integration

Built-in Git panel in the sidebar:

- View changed files with staged/unstaged diffs
- Stage, unstage, commit with messages
- Push, pull, fetch from remote
- Create and publish new repositories
- Full GitHub workflow without leaving the app

<p align="center">
<img src="docs/img/git-panel.png" alt="Git panel" width="300">
</p>

---

## Keyboard Shortcuts

Modifier shorthand: `Cmd` on macOS, `Ctrl` on Windows/Linux.

| Shortcut | Action |
|----------|--------|
| `Cmd/Ctrl+,` | Open Settings |
| `Cmd/Ctrl+Shift+P` | Command Palette |
| `Cmd/Ctrl+B` | Toggle Sidebar |
| `Cmd/Ctrl+J` | Toggle Terminal |
| `Cmd/Ctrl+Shift+J` | Toggle AI Chat |
| `Cmd/Ctrl+N` | New Terminal Tab |
| `Cmd/Ctrl+W` | Close Tab |
| `Cmd/Ctrl+S` | Save File |
| `Cmd/Ctrl+G` | Go to Folder |

---

## Settings & Customization

Access via `Cmd/Ctrl+,` or the gear icon in the top bar.

| Category | Options |
|----------|---------|
| **Editor** | Font size, tab size, word wrap, minimap, theme (`operon-dark` / `operon-light`) |
| **Terminal** | Font size, cursor style, scrollback buffer |
| **Claude** | Provider (Anthropic / Portkey / Custom), model selection, max turns, plan-mode toggle |
| **Auth** | Anthropic OAuth or API key, Portkey virtual key, custom endpoint URL |
| **Appearance** | Light / dark theme toggle (also accessible from the top bar) |

<p align="center">
<img src="docs/img/settings-panel.png" alt="Settings" width="500">
</p>

---

## Building for Distribution

The same `npm run build:<target>` scripts used in development produce signed
artifacts when the appropriate signing credentials are present.

### macOS — signed & notarized DMGs

```bash
cp build-signed.example.sh build-signed.sh
# Edit build-signed.sh with your Apple Developer credentials
bash build-signed.sh
```

Three macOS templates are provided:

| Template | Target |
|---|---|
| `build-signed.example.sh` | Apple Silicon DMG |
| `build-intel.example.sh` | Intel DMG |
| `build-universal.example.sh` | Universal binary (both architectures) |

### Windows & Linux

The GitHub Actions [`Release` workflow](.github/workflows/release.yml) builds
signed Windows installers (MSI + NSIS) and Linux bundles (`.deb`, `.rpm`,
`.AppImage`) automatically on every `v*` tag push. Locally:

```bash
npm run build:win      # Windows: .msi + .exe
npm run build:linux    # Linux: .deb + .rpm + .AppImage
```

---

## Project Architecture

```
operon/
├── src/                    # React/TypeScript frontend
│   ├── components/         # UI components
│   │   ├── chat/           # AI chat panel with streaming, tool display
│   │   ├── editor/         # Monaco editor, diff viewer, file viewer
│   │   ├── terminal/       # xterm.js terminal with tab management
│   │   ├── sidebar/        # File explorer, SSH, Git, Protocols, Help
│   │   ├── layout/         # AppShell, TopBar, ActivityBar, StatusBar
│   │   ├── settings/       # Settings panel
│   │   └── setup/          # First-time setup wizard
│   ├── context/            # React context (project state, editor tabs, theme)
│   ├── hooks/              # Custom hooks (keyboard shortcuts)
│   ├── lib/                # Typed IPC wrappers (claude, files, terminal, ssh, portkey)
│   └── types/              # TypeScript type definitions
├── src-tauri/              # Rust backend (Tauri 2)
│   ├── src/
│   │   ├── main.rs         # Entry point
│   │   ├── lib.rs          # Tauri builder, state managers, command registration
│   │   └── commands/       # IPC command handlers (terminal, files, claude, ssh, settings, portkey, proxy)
│   ├── binaries/           # Bundled sidecars (anthropic-proxy translation layer)
│   └── icons/              # App icons (icns, png, ico)
├── protocols/              # 665 bundled bioinformatics protocols (Markdown)
├── presets/                # Hot-fetchable Portkey gateway presets
├── docs/                   # Documentation website + images
└── build-*.example.sh      # macOS signed-build script templates (no credentials)
```

### Tech Stack

| Layer | Technology |
|-------|-----------|
| **App Shell** | Tauri 2 (Rust) — small bundle, native webview |
| **Frontend** | React 18 + TypeScript + Vite 6 |
| **Terminal** | xterm.js + portable-pty (same stack as VS Code & WezTerm) |
| **Editor** | Monaco Editor (VS Code's engine) |
| **Layout** | react-resizable-panels |
| **Styling** | Tailwind CSS 3 + lucide-react icons; CSS-variable themes |
| **SSH** | OpenSSH sidecar via PTY (ProxyJump, agent forwarding, `~/.ssh/config`) |
| **AI Engine** | Claude Code (headless NDJSON streaming) + bundled `anthropic-proxy-rs` for OpenAI-compatible backends |
| **Provider Routing** | Anthropic direct · Portkey gateways · Custom OpenAI-compatible (Ollama / vLLM / LM Studio) |

---

## Contributing

We welcome contributions from the bioinformatics community!

**Branch model:**
- **`cross-platform`** — active development and the default branch (PRs go here)
- Release tags (`v0.7.x`) are cut directly from `cross-platform` and trigger
  the multi-OS Release workflow

1. Fork the repository
2. Create a feature branch off `cross-platform` (`git checkout -b feature/my-feature cross-platform`)
3. Make your changes
4. Run the dev server to test (`npm run tauri dev`)
5. Commit and push
6. Open a Pull Request targeting `cross-platform`

---

## Help & Support

| Resource | Link |
|----------|------|
| **In-app Help** | Click the Help icon in the activity bar |
| **Documentation** | [swaruplab.bio.uci.edu/operon](https://swaruplab.bio.uci.edu/operon) |
| **Bug Reports** | [GitHub Issues](https://github.com/swaruplab/operon/issues) |
| **Latest Downloads** | [GitHub Releases](https://github.com/swaruplab/operon/releases/latest) |
| **Release Notes** | [CHANGELOG.md](./CHANGELOG.md) |

<p align="center">
<img src="docs/img/help-panel.png" alt="Help panel" width="500">
</p>

---

## License

MIT License — see [LICENSE](LICENSE) for details.

<div align="center">

Built with care by [Swarup Lab](https://github.com/swaruplab) at UC Irvine

</div>
