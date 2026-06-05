# tmux sessions

For remote work, **always run inside tmux** (or `screen`, but tmux is the
modern default). This is true for any long-running bioinformatics work,
not just Operon — Operon just leans into it harder than most tools.

## Why tmux is non-negotiable

If your SSH connection drops:

- Laptop goes to sleep
- Wi-Fi reconnects
- VPN renegotiates
- You walk between buildings

… a bare SSH session would **kill any running processes**. With tmux, your
session survives disconnections, and you reattach later from anywhere.

For Operon specifically: AI sessions running in Agent mode on a compute
node may take hours. The Operon chat panel will reconnect to the running
session on relaunch — but only if the underlying process is still alive,
which only tmux can guarantee on a flaky link.

## Starting (or attaching to) a session

```bash
# Attach if "operon" exists, create if it doesn't
tmux new-session -A -s operon
```

`-A` is the magic flag — idempotent attach-or-create.

## Reattaching after a disconnect

Reconnect via SSH in Operon, open a terminal, and:

```bash
tmux attach -t operon
```

Your session, its scrollback, and any running processes are intact.

## Listing sessions

```bash
tmux ls
# operon: 1 windows (created Mon Jun  3 14:22:17 2026) [232x67]
# scratch: 1 windows (created Tue Jun  4 09:08:00 2026)
```

## A starter `.tmux.conf`

If you don't already have one, drop this into `~/.tmux.conf` on the
remote host:

```tmux
# More readable status bar
set -g status-bg colour234
set -g status-fg white
set -g status-left "#[fg=green]#S #[fg=white]| "

# 256-color support
set -g default-terminal "screen-256color"

# More scrollback
set -g history-limit 50000

# Mouse mode (scroll, resize panes)
set -g mouse on

# Vim-style copy
setw -g mode-keys vi
```

Then `tmux kill-server` and reattach so the config loads.

## Multiple sessions for parallel work

Many bioinformatics workflows run a long alignment in one session while
you do exploratory analysis in another:

```bash
# In one terminal tab:
tmux new -s align
# start the long STAR/BWA job ...

# In another terminal tab:
tmux new -s explore
# poke at QC outputs, run a Jupyter notebook ...
```

Operon's terminal panel can have multiple tabs, each attached to a
different tmux session. Or open a single tmux session with multiple
windows and split panes — both work.

## Session resume

When you start an AI session on a remote host, Operon:

1. Creates (or attaches to) a tmux session
2. Writes the AI session metadata to `~/.operon/sessions/`
3. Tails the agent's NDJSON output from the login node

On app relaunch — same machine or a different one — Operon detects the
running session and offers to reconnect:

> *"Session `operon-a3f8` on `hpc.school.edu` is still running. Reconnect?"*

This is what lets you start an analysis on your Mac at the office, close
the laptop, and pick it up on your Linux desktop at home — the analysis
keeps running on the cluster while your laptop is gone.

## When to kill a session

Sessions accumulate. Periodically:

```bash
tmux ls
tmux kill-session -t old-session-name
# or nuclear option:
tmux kill-server
```

Operon's session metadata is independent — killing a tmux session won't
delete the metadata, but the next reconnect attempt will fail and Operon
will mark the session as ended.
