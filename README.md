<div align="center">

# Operon

**AI-powered IDE for bioinformatics — built by biologists, for biologists.**

[![MIT License](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Version](https://img.shields.io/github/v/release/swaruplab/operon?color=purple)](https://github.com/swaruplab/operon/releases/latest)
[![macOS](https://img.shields.io/badge/macOS-12%2B-black.svg?logo=apple)](https://swaruplab.bio.uci.edu/operon)
[![Tauri](https://img.shields.io/badge/Tauri_2-Rust_%2B_React-orange.svg)](https://tauri.app)
[![Protocols](https://img.shields.io/badge/protocols-180%2B-green.svg)](#analysis-protocols)

Operon is a native macOS desktop application that brings together an AI coding
assistant (Claude), integrated terminal, code editor, file browser, and remote
server access into a single tool designed for computational biologists. Whether
you're running RNA-seq pipelines on an HPC cluster or analyzing single-cell
data on your laptop, Operon gives you a professional development environment
with AI that understands your domain.

[**Download**](https://swaruplab.bio.uci.edu/operon) •
[**Documentation**](https://swaruplab.bio.uci.edu/operon) •
[**GitHub Releases**](https://github.com/swaruplab/operon/releases)

<img src="docs/img/main-workspace.png" alt="Operon workspace" width="800">

</div>

---

## Table of Contents

- [Why Operon?](#why-operon)
- [Features](#features)
- [System Requirements](#system-requirements)
- [Installation](#installation)
  - [Download (pre-built DMG)](#download-recommended)
  - [Build from Source](#build-from-source)
- [Getting Started](#getting-started)
  - [Setup Wizard](#setup-wizard)
  - [Authentication](#authentication)
- [Using Operon](#using-operon)
  - [Workspace Overview](#workspace-overview)
  - [File Explorer](#file-explorer)
  - [Code Editor](#code-editor)
  - [Integrated Terminal](#integrated-terminal)
  - [AI Chat — Three Modes](#ai-chat--three-modes)
  - [Analysis Protocols](#analysis-protocols)
  - [PubMed Integration](#pubmed-integration)
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

### Three AI Modes
**Agent** executes multi-step tasks. **Plan** architects solutions.
**Ask** answers questions — with optional PubMed literature search.

</td>
<td width="33%" valign="top">

### Remote HPC
SSH into university clusters, browse remote files, run AI agents directly
on compute nodes. Your data never leaves the server.

</td>
</tr>
<tr>
<td width="33%" valign="top">

### 180+ Protocols
Curated analysis protocols for RNA-seq, scRNA-seq, ATAC-seq, proteomics,
and more. Create your own with AI or write them in Markdown.

</td>
<td width="33%" valign="top">

### Git Integration
Full Git and GitHub workflow built into the sidebar — stage, commit, push,
and publish repositories without leaving the app.

</td>
<td width="33%" valign="top">

### Native Performance
Built with Tauri 2 (Rust + React). ~600KB bundle, 20-40MB RAM. Uses your
system's native webview — not Electron.

</td>
</tr>
</table>

---

## System Requirements

| Requirement | Details |
|-------------|---------|
| **OS** | macOS 12 (Monterey) or later |
| **Architecture** | Apple Silicon (M1/M2/M3/M4) or Intel |
| **Disk Space** | ~500 MB (including dependencies) |
| **RAM** | 4 GB minimum, 8 GB recommended |
| **Internet** | Required for AI features and initial setup |

> **Note:** All developer dependencies (Xcode CLT, Homebrew, Node.js,
> Claude Code) are installed automatically by the setup wizard on first launch.

---

## Installation

### Download (Recommended)

**The latest signed & notarized DMGs are always available at:
https://swaruplab.bio.uci.edu/operon**

Past versions and release notes are on [GitHub Releases](https://github.com/swaruplab/operon/releases).

| Platform | Download |
|----------|----------|
| **Apple Silicon** (M1/M2/M3/M4) | [Operon_0.3.2_aarch64.dmg](https://swaruplab.bio.uci.edu/operon) |
| **Intel Mac** | [Operon_0.3.2_x64.dmg](https://swaruplab.bio.uci.edu/operon) |

<details>
<summary><strong>Installation steps (click to expand)</strong></summary>

1. Download the `.dmg` for your Mac architecture
2. Open the `.dmg` and drag **Operon** into your **Applications** folder
3. On first launch, right-click → **Open** to bypass macOS Gatekeeper

<p align="center">
<img src="docs/img/install-dmg.png" alt="Drag to Applications" width="500">
</p>
<p align="center">
<img src="docs/img/install-security.png" alt="Gatekeeper dialog" width="400">
</p>

</details>

### Build from Source

<details>
<summary><strong>Prerequisites (click to expand)</strong></summary>

For building from source, you need these installed manually:

| Tool | Install Command | Why |
|------|----------------|-----|
| **Xcode Command Line Tools** | `xcode-select --install` | C/C++ compiler, macOS SDK |
| **Rust** (rustup) | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` | Compiles the Tauri/Rust backend |
| **Node.js** (v18+) | `brew install node` or via [nvm](https://github.com/nvm-sh/nvm) | Builds the React frontend |
| **npm** | Included with Node.js | Package manager |

</details>

```bash
# 1. Clone the repository
git clone https://github.com/swaruplab/operon.git
cd operon

# 2. Install frontend dependencies
npm install

# 3. Run in development mode (hot-reload)
npm run tauri dev

# 4. Build a production .app bundle
npm run tauri build
# Output: src-tauri/target/release/bundle/macos/Operon.app
```

<details>
<summary><strong>All development commands</strong></summary>

| Command | What it does |
|---------|-------------|
| `npm run tauri dev` | Start dev server with hot-reload |
| `npm run build` | Build frontend only (TypeScript + Vite) |
| `npm run tauri build` | Full production build (.app + .dmg) |
| `npm run tauri build -- --target x86_64-apple-darwin` | Cross-compile for Intel |

</details>

---

## Getting Started

### Setup Wizard

On first launch, Operon walks you through installing all dependencies automatically:

| Step | What's installed |
|------|-----------------|
| 1 | Xcode Command Line Tools |
| 2 | Developer tools (Homebrew, Node.js, GitHub CLI) |
| 3 | Claude Code (the AI engine) |

<p align="center">
<img src="docs/img/setup-welcome.png" alt="Setup wizard welcome" width="500">
</p>

<details>
<summary><strong>Setup wizard screenshots</strong></summary>

<p align="center">
<img src="docs/img/setup-xcode.png" alt="Xcode CLT" width="500">
<br><em>Step 1: Xcode Command Line Tools</em>
</p>
<p align="center">
<img src="docs/img/setup-tools.png" alt="Developer tools" width="500">
<br><em>Step 2: Developer tools</em>
</p>
<p align="center">
<img src="docs/img/setup-claude.png" alt="Claude Code" width="500">
<br><em>Step 3: Claude Code</em>
</p>

</details>

### Authentication

Two options to authenticate with Claude:

| Method | How |
|--------|-----|
| **OAuth Login** (Recommended) | Click "Log in with Claude," authorize in browser, paste the code back |
| **API Key** | Enter an Anthropic API key directly in Settings |

<p align="center">
<img src="docs/img/auth-selection.png" alt="Auth options" width="500">
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
- Go-to-folder path bar (`Cmd+G`)
- Symlink-aware (local and remote)

<p align="center">
<img src="docs/img/file-explorer.png" alt="File explorer" width="300">
</p>

### Code Editor

- **Monaco Editor** — same engine as VS Code
- 30+ languages with syntax highlighting
- Custom dark theme (`operon-dark`)
- Side-by-side diff viewer with accept/reject
- Image and PDF viewer with zoom & download

### Integrated Terminal

- Full terminal emulator powered by **xterm.js**
- Multiple tabs with independent sessions
- Auto-copy on selection
- WebGL rendering for performance
- Preserves your shell aliases and conda environments

### AI Chat — Three Modes

| Mode | Purpose | Best For |
|------|---------|----------|
| **Agent** | Executes multi-step tasks autonomously | Writing scripts, running pipelines, debugging |
| **Plan** | Creates implementation plans without executing | Designing analysis workflows, architecture |
| **Ask** | Answers questions (with optional PubMed) | Literature review, explaining concepts |

<p align="center">
<img src="docs/img/tour-ai-modes.png" alt="AI modes" width="500">
</p>

### Analysis Protocols

Operon ships with **180+ built-in protocols** covering:

<table>
<tr>
<td>

- RNA-seq, scRNA-seq, bulk RNA-seq
- DESeq2, Scanpy, Seurat
- ATAC-seq, ChIP-seq, CUT&Tag
- WGS, WES, variant calling

</td>
<td>

- Spatial transcriptomics (Visium, MERFISH)
- Proteomics, metabolomics
- Metagenomics, 16S rRNA
- GWAS, eQTL, molecular dynamics

</td>
<td>

- Database queries:
  PubMed, GEO, KEGG,
  GTEx, UniProt, JASPAR,
  AlphaFold

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

| Shortcut | Action |
|----------|--------|
| `Cmd+,` | Open Settings |
| `Cmd+Shift+P` | Command Palette |
| `Cmd+B` | Toggle Sidebar |
| `Cmd+J` | Toggle Terminal |
| `Cmd+Shift+J` | Toggle AI Chat |
| `Cmd+N` | New Terminal Tab |
| `Cmd+W` | Close Tab |
| `Cmd+S` | Save File |
| `Cmd+G` | Go to Folder |

---

## Settings & Customization

Access via `Cmd+,` or the gear icon in the top bar.

| Category | Options |
|----------|---------|
| **Editor** | Font size, tab size, word wrap, minimap |
| **Terminal** | Font size, cursor style, scrollback buffer |
| **Claude** | Model selection, max turns, API key |
| **Auth** | OAuth vs. API key, manage credentials |

<p align="center">
<img src="docs/img/settings-panel.png" alt="Settings" width="500">
</p>

---

## Building for Distribution

To create signed & notarized DMGs for distribution:

```bash
cp build-signed.example.sh build-signed.sh
# Edit build-signed.sh with your Apple Developer credentials
bash build-signed.sh
```

Three build script templates are provided:

| Template | Target |
|----------|--------|
| `build-signed.example.sh` | Apple Silicon DMG |
| `build-intel.example.sh` | Intel DMG |
| `build-universal.example.sh` | Universal binary (both architectures) |

---

## Project Architecture

```
operon/
├── src/                    # React/TypeScript frontend
│   ├── components/         # UI components (chat, editor, terminal, sidebar, etc.)
│   │   ├── chat/           # AI chat panel with streaming, tool display
│   │   ├── editor/         # Monaco editor, diff viewer, file viewer
│   │   ├── terminal/       # xterm.js terminal with tab management
│   │   ├── sidebar/        # File explorer, SSH, Git, Protocols, Help
│   │   ├── layout/         # AppShell, TopBar, ActivityBar, StatusBar
│   │   ├── settings/       # Settings panel
│   │   └── setup/          # First-time setup wizard
│   ├── context/            # React context (project state, editor tabs)
│   ├── hooks/              # Custom hooks (keyboard shortcuts)
│   ├── lib/                # Typed IPC wrappers (claude, files, terminal, ssh)
│   └── types/              # TypeScript type definitions
├── src-tauri/              # Rust backend (Tauri 2)
│   ├── src/
│   │   ├── main.rs         # Entry point
│   │   ├── lib.rs          # Tauri builder, state managers, command registration
│   │   └── commands/       # IPC command handlers (terminal, files, claude, ssh, settings)
│   ├── protocols/          # 180+ bundled analysis protocols
│   └── icons/              # App icons (icns, png)
├── protocols/              # Protocol definitions (Markdown)
├── docs/                   # Documentation website + images
└── build-*.example.sh      # Build script templates (no credentials)
```

### Tech Stack

| Layer | Technology |
|-------|-----------|
| **App Shell** | Tauri 2 (Rust) — ~600KB bundle, 20-40MB RAM |
| **Frontend** | React 18 + TypeScript + Vite 6 |
| **Terminal** | xterm.js + portable-pty (same stack as VS Code) |
| **Editor** | Monaco Editor (VS Code's engine) |
| **Layout** | react-resizable-panels |
| **Styling** | Tailwind CSS 3 + lucide-react icons |
| **SSH** | OpenSSH sidecar via PTY |
| **AI Engine** | Claude Code (headless NDJSON streaming) |

---

## Contributing

We welcome contributions from the bioinformatics community!

**Branch model:**
- **`dev`** — active development (default branch, PRs go here)
- **`main`** — stable releases only (merged from `dev` when ready)

1. Fork the repository
2. Create a feature branch off `dev` (`git checkout -b feature/my-feature dev`)
3. Make your changes
4. Run the dev server to test (`npm run tauri dev`)
5. Commit and push
6. Open a Pull Request targeting `dev`

---

## Help & Support

| Resource | Link |
|----------|------|
| **In-app Help** | Click the Help icon in the activity bar |
| **Documentation** | [swaruplab.bio.uci.edu/operon](https://swaruplab.bio.uci.edu/operon) |
| **Bug Reports** | [GitHub Issues](https://github.com/swaruplab/operon/issues) |
| **Latest Downloads** | [swaruplab.bio.uci.edu/operon](https://swaruplab.bio.uci.edu/operon) |

<p align="center">
<img src="docs/img/help-panel.png" alt="Help panel" width="500">
</p>

---

## License

MIT License — see [LICENSE](LICENSE) for details.

<div align="center">

Built with care by [Swarup Lab](https://github.com/swaruplab) at UC Irvine

</div>
