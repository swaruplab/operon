# Frequently asked questions

## What is Claude Code and why does Operon need it?

[Claude Code](https://docs.anthropic.com/claude/code) is Anthropic's
command-line AI agent. Operon uses it as the AI backend — it's what allows
Claude to read your files, run commands, and execute multi-step analyses.

Operon provides the visual interface; Claude Code provides the engine. The
setup wizard installs Claude Code automatically; you can also install it
yourself with `curl -fsSL https://claude.ai/install.sh | bash`.

## Does my data leave my computer?

It depends on the [provider](ai/providers.md) you pick:

- **Anthropic direct** — your prompts and any file contents Claude reads
  are sent to `api.anthropic.com`. Your data goes to Anthropic.
- **Portkey** — same as above, plus Portkey sees the requests. Your
  institution's Portkey deployment may add logging / cost tracking on its
  side.
- **Custom (Ollama / vLLM / LM Studio)** — nothing leaves your network.
  Inference happens on your machine or your cluster.

For HPC workloads on a remote server: the data stays on the server.
Only the terminal I/O (the commands Claude runs and their output) travels
over SSH to your laptop. The actual file contents only leave the cluster
if you explicitly send them to a cloud LLM via Claude's prompt.

Operon itself collects **zero telemetry** — no session counts, no crash
breadcrumbs, no pings. Source is on [GitHub](https://github.com/swaruplab/operon)
— verify yourself.

## Can I use Operon offline?

Editor, file explorer, terminal, and Git all work offline.

AI features need internet access **to the LLM provider you chose**. If
that's Anthropic or Portkey, you need internet. If it's a local Ollama /
vLLM / LM Studio, you can be fully offline.

## What HPC schedulers are supported?

Protocols include templates for **SLURM, PBS/Torque, and SGE**. Claude
can generate and submit job scripts for any of these when working in
Agent mode on a remote cluster. See [SLURM / PBS / SGE](hpc/slurm.md).

## Can I use my own conda environments?

Yes. The integrated terminal inherits your shell configuration, including
conda / mamba initialization. Your environments are available in both the
terminal and when Claude executes commands in Agent mode.

## How do I update Operon?

Operon checks GitHub Releases at launch. When a newer version is available,
a banner appears with a one-click upgrade. You can also grab the latest
installer manually from the [Releases page](https://github.com/swaruplab/operon/releases/latest)
and overwrite the existing install:

- **macOS** — drag the new `.app` into Applications, overwrite when prompted
- **Windows** — run the new `.exe` / `.msi`
- **Linux** — re-run `apt`/`dnf`, or replace the AppImage

Your settings, sessions, and custom protocols live in `~/.operon/` and are
preserved.

## Is Operon open source?

Yes — MIT licensed. Source on [GitHub](https://github.com/swaruplab/operon).
Contributions welcome — see [Contributing](contributing.md).

## What models can I use?

Anything Claude Code can talk to:

- **Claude family** (Opus / Sonnet / Haiku, all current versions) — direct,
  via Portkey, or via Bedrock-routed Portkey
- **Non-Anthropic models** via Portkey — Moonshot Kimi, GPT, Gemini,
  DeepSeek, etc. (routed through Operon's translation proxy)
- **Local OpenAI-compatible models** — Llama 3, Qwen, Mistral, anything
  Ollama/vLLM/LM Studio can serve

Quality varies a lot — frontier models (Claude Opus, GPT-4o, Llama-3.3-70B)
are strong at agent / plan; smaller models (8B-class) are fine for
simple Ask queries but struggle with multi-step Agent work.

## What's the difference between Plan mode and Agent mode?

Plan mode **designs**, Agent mode **executes**.

- **Plan** — generates a step-by-step plan in `implementation_plan.md`,
  does NOT touch your files or run commands. Iterate with Claude until
  the plan is right.
- **Agent** — runs commands, writes files, debugs. If `implementation_plan.md`
  exists, Agent picks it up and tracks progress with `[x]` markers.

The recommended workflow: **Plan first, Agent second**. Especially on a
remote cluster where mistakes waste queued compute. See [Four modes](ai/modes.md).

## Can I run multiple Claude sessions in parallel?

Yes. Each chat tab is an independent session. Background sessions keep
streaming when you switch tabs. On a remote host, each session runs in
its own tmux session so they don't interfere.

There's no hard cap, but each Agent session uses cluster resources — be
nice to your cluster admins.

## Where is my data stored?

| What | Where |
|---|---|
| Settings, sessions, custom protocols | `~/.operon/` |
| Tauri state (window position, etc.) | `~/Library/Application Support/com.operon.app/` (macOS) · `%APPDATA%\com.operon.app\` (Windows) · `~/.config/com.operon.app/` (Linux) |
| Credentials | OS keychain (Keychain / Credential Manager / libsecret) — never plain-text |
| Logs | OS-standard log dir (see [Troubleshooting](troubleshooting.md#logs-and-debugging)) |

## Is the AI doing the science, or am I?

You are. Claude is a tool — a very capable assistant for the mechanical
parts (boilerplate, parameter sweeps, common-pattern debugging) — but the
hypothesis, the data, the interpretation, and the responsibility are still
yours.

Operon is opinionated about this: it ships in **Plan-first** mode for new
sessions, the chat panel always shows what command will run before it
runs, and Agent always asks for approval on destructive operations. The
goal is to make you faster without taking the scientist out of the loop.

## Can I use Operon for non-bio work?

Yes. Operon's protocol catalog is biology-focused, but the IDE itself is
just an AI-powered editor — Python, R, Rust, TypeScript, anything Claude
Code can work with. If you don't use the protocols, you don't have to.

## How do I report a bug?

[GitHub Issues](https://github.com/swaruplab/operon/issues). Include:

- Your Operon version (Settings → About)
- OS + version
- The last ~50 log lines (see [Troubleshooting](troubleshooting.md#logs-and-debugging))
- A minimal reproducer if possible

We triage on a weekly cadence and try to acknowledge within a few days.

## Can I contribute?

Yes please. See [Contributing](contributing.md). The codebase is well-doc'd
and the dev loop (`npm run tauri dev`) is fast.

## Who builds this?

[Swarup Lab](https://swaruplab.bio.uci.edu) at UC Irvine. Computational
neuroscience, single-cell genomics, microglia in Alzheimer's. We built
Operon for ourselves and released it because other labs kept asking.

Tech stack credit: Tauri 2, React, Rust, Monaco, xterm.js, Claude Code.
Stand on the shoulders of giants.
