# Quickstart

From zero to your first AI-driven analysis in about ten minutes.

## 1. Install

Pick your platform:

=== "macOS"

    Download the appropriate `.dmg` from [GitHub Releases](https://github.com/swaruplab/operon/releases/latest):

    - **Apple Silicon (M1/M2/M3/M4):** `Operon_<version>_aarch64.dmg`
    - **Intel:** `Operon_<version>_x64.dmg`

    Open the DMG, drag **Operon** into **Applications**. First launch:
    right-click → **Open** to bypass Gatekeeper.

=== "Windows"

    Download `Operon_<version>_x64-setup.exe` (or the MSI for IT rollouts)
    from [GitHub Releases](https://github.com/swaruplab/operon/releases/latest).

    SmartScreen may show "Unrecognized app" — click **More info → Run anyway**.

=== "Linux"

    Pick the format that matches your distro:

    ```bash
    # Debian / Ubuntu
    sudo apt install ./Operon_<version>_amd64.deb

    # Fedora / RHEL / Rocky / Alma
    sudo dnf install ./Operon-<version>-1.x86_64.rpm

    # Any distro (no install)
    chmod +x Operon_<version>_amd64.AppImage
    ./Operon_<version>_amd64.AppImage
    ```

Detailed per-OS notes: [Install](install/index.md).

## 2. First launch & setup wizard

On first launch, Operon checks your system and offers to install missing
dependencies — **Claude Code** is the main one. Accept the prompts and you'll
land on the workspace.

![Setup welcome](img/setup-welcome.png){ width=600 }

If you'd rather configure things yourself, see
[Build from source](install/build-from-source.md).

## 3. Pick an AI provider

Open **Settings → Auth → Provider** and choose:

| Provider | When to use |
|---|---|
| **Anthropic** (default) | You have a Claude API key or want to log in with your Anthropic account |
| **Portkey** | Your institution runs a Portkey gateway (e.g. UCI ZotGPT) — paste the virtual key |
| **Custom** | You want to use a local model — Ollama, vLLM, LM Studio, or any OpenAI-compatible endpoint |

Full provider docs: [AI providers](ai/providers.md).

## 4. Open your data

`File → Open Folder` (or ++cmd+o++ / ++ctrl+o++). Point Operon at a directory
that contains your data — FASTQs, count matrices, scripts, anything. This is
the working directory Claude will operate in.

## 5. Pick a protocol (optional but recommended)

Click the **Protocols** icon in the activity bar and pick one that matches
your workflow — e.g. *DESeq2 Differential Expression* or *scRNA-seq (Scanpy)*.
Its instructions become Claude's context for this session.

Browse the catalog: [Protocols](protocols/browse.md). Operon ships with 665
bio-first protocols.

## 6. Plan first

Switch the chat-panel dropdown to **Plan** mode and describe what you want:

> *"I have paired-end RNA-seq FASTQs in this folder. Run QC, align to GRCh38,
> and do differential expression with DESeq2."*

Claude returns a step-by-step plan (saved to `implementation_plan.md`). Read
it. Push back on anything off, ask for changes, iterate. **Do not rush to
Agent mode.** A few extra Plan-mode minutes saves hours of debugging.

## 7. Execute in Agent mode

Once the plan is right, switch to **Agent** mode and tell Claude to proceed.
It will:

- Write scripts and notebooks into your project
- Run commands in the terminal
- Read outputs and adapt
- Track its progress by checking off steps in `implementation_plan.md`

You can interrupt with the **Stop** button at any time.

## 8. Read the output with Ask mode

When the analysis finishes, switch to **Ask** mode for questions:

> *"What does this volcano plot tell me?"*
> *"Why did DESeq2 shrink these log-fold changes?"*

Toggle **PubMed** in Ask mode to ground Claude's answers in real biomedical
literature with cited DOIs.

---

## Next steps

- [The four AI modes](ai/modes.md) — when to use each
- [Recipes](recipes/index.md) — end-to-end walkthroughs (PBMC, bulk RNA, spatial, ATAC)
- [HPC mode](hpc/index.md) — run Claude on your cluster, not your laptop
- [Private LLM stack](ai/private-llm.md) — Ollama, vLLM, LM Studio for embargoed data
- [MCP catalog](ai/mcp.md) — typed tool access to PubMed, GEO, KEGG, AlphaFold, …
