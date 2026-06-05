# Operon

**AI-powered IDE for bioinformatics — built by biologists, for biologists.**

Operon is a cross-platform desktop application that brings together an AI
coding assistant (Claude), an integrated terminal, a code editor, a file
browser, and remote-server access into a single tool designed for
computational biologists.

Whether you're running RNA-seq pipelines on an HPC cluster from a Windows
laptop, analyzing single-cell data on a Linux workstation, or writing scripts
on a Mac — Operon gives you a professional development environment with AI
that understands your domain.

[Download :material-download:](https://github.com/swaruplab/operon/releases/latest){ .md-button .md-button--primary }
[Quickstart :material-rocket-launch:](quickstart.md){ .md-button }
[GitHub :fontawesome-brands-github:](https://github.com/swaruplab/operon){ .md-button }

![Operon workspace](img/main-workspace.png){ .center }

---

## Why Operon?

<div class="grid cards" markdown>

-   :material-dna: __Built for biology__

    Understands bioinformatics file formats (FASTA, FASTQ, VCF, BAM, GFF),
    common pipelines, and domain-specific best practices.

-   :material-robot: __Four AI modes__

    **Agent** executes. **Plan** designs. **Ask** answers (with optional
    PubMed). **Report** produces structured writeups.

-   :material-server-network: __Remote HPC__

    SSH into university clusters, run Claude directly on compute nodes,
    persistent tmux sessions. Your data never leaves the server.

-   :material-format-list-bulleted-square: __665 bundled protocols__

    Bio-first taxonomy covering single-cell, spatial, chromatin, CRISPR,
    proteomics, drug discovery, and more.

-   :material-cog-transfer: __Multi-provider AI__

    Anthropic direct, institutional Portkey gateways (e.g. UCI ZotGPT),
    or any OpenAI-compatible local backend (Ollama, vLLM, LM Studio).

-   :material-rocket-launch: __Native performance__

    Tauri 2 (Rust + React). Tiny bundle, 20–40 MB RAM. macOS, Windows,
    Linux.

</div>

---

## Two-minute tour

1. **Open a project** — folder with FASTQs, count matrices, scripts, or VCFs.
2. **Pick a protocol** — from the bio-first catalog. Its instructions become
   Claude's context.
3. **Plan first** — switch the chat to Plan mode and describe what you want.
   Iterate until the plan is right.
4. **Execute** — switch to Agent mode. Claude writes scripts, runs commands,
   and adapts to errors.
5. **Read** — Ask mode + PubMed answers questions about the output.

See the [quickstart](quickstart.md) for the full walkthrough, or pick a
[recipe](recipes/index.md) to jump straight into a real analysis.

---

## What's new

The [Changelog](changelog.md) tracks every release. Recent highlights:

- **v0.7.3** — cleared stale shell-env vars on Claude spawn (fixes Portkey
  routing leak and Ollama/vLLM 401s)
- **v0.7.2** — protocol catalog cleanup with bio-first taxonomy (665 protocols)
- **v0.7.1** — protocol import overhaul (+536 from 3 skill repos)
- **v0.7.0** — light/dark theme, Intel-Mac proxy sidecar, multi-provider AI
- **v0.6.x** — Windows + Linux releases

---

## Get help

| Resource | Where |
|---|---|
| Install issues | [Install guide](install/index.md) for your OS |
| Troubleshooting | [Troubleshooting](troubleshooting.md) |
| Frequently asked questions | [FAQ](faq.md) |
| Bug reports | [GitHub Issues](https://github.com/swaruplab/operon/issues) |
| Lab page | [Swarup Lab](https://swaruplab.bio.uci.edu/operon) |
