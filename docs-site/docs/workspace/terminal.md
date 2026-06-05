# Integrated terminal

A fully-featured terminal pinned to the bottom of the workspace. Same stack
as VS Code and WezTerm — **xterm.js** in the frontend, **portable-pty** in
the Rust backend.

## Toggle / new tab

| Action | Shortcut |
|---|---|
| Toggle terminal panel | ++cmd+j++ / ++ctrl+j++ |
| New terminal tab | ++cmd+n++ / ++ctrl+n++ (when terminal is focused) |
| Close terminal tab | ++cmd+w++ / ++ctrl+w++ (when terminal is focused) |

## Shell integration

Operon uses your **actual login shell** (zsh, bash, fish) and sources your
profile (`~/.zshrc`, `~/.bash_profile`, `~/.bashrc`). That means:

- Your **conda** / **mamba** environments work
- Your **module load** commands work
- Your **aliases** (`claude → npx @anthropic-ai/claude-code`) work
- Your `PATH` extensions, `pyenv`, `nvm`, `rbenv`, etc. work
- Your shell prompt looks exactly like in any other terminal

This also matters in HPC mode — see [HPC architecture](../hpc/architecture.md)
for why running commands "inside the shell" (rather than `bash -c`) is
non-negotiable for alias-heavy clusters.

## Features

- **WebGL rendering** for crisp glyph rendering even at 4K
- **Auto-copy on selection** — selecting text immediately puts it on the
  clipboard, in the BSD / Linux convention
- **Paste with right-click** (Windows / Linux) or ++cmd+v++ (macOS)
- **Tab management** — multiple independent shells, named per-tab
- **Resize debounced** so dragging the divider doesn't flood the PTY with
  resize events
- **Hidden-tab buffer preserved** — when you switch tabs, the inactive
  terminal keeps running and its scrollback stays intact

## On Windows

The integrated terminal uses **bundled Git Bash** so you get a POSIX shell
out of the box — meaning all the HPC tutorials, Claude Code's bash scripts,
and conda activations Just Work. No WSL required.

## How Claude uses the terminal

When you're in Agent mode, Claude **runs commands in the same shell** you
type into. You can see what it executes, intervene, and pick up the prompt
yourself between Claude's invocations. This is by design — the terminal
is a shared workspace, not a hidden subprocess.

## Settings

++cmd++/`,` or ++ctrl++/`,` → **Terminal**:

| Setting | Default | What |
|---|---|---|
| Font size | 13 | Terminal font size |
| Cursor style | block | block / underline / bar |
| Scrollback buffer | 10000 lines | Max lines kept in history |
| Cursor blink | on | Blink the cursor |
| Bell | off | Audible bell on terminal `\a` |
