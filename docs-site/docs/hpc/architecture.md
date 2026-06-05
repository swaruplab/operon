# How HPC mode actually works

Three processes, one cluster. The full flow — no hand-waving.

## The picture

```
Your laptop                    Login node              Compute node
+---------------+              +-----------+           +-----------+
|  Operon       |  SSH #1 ---->|           |  inside   |           |
|  chat panel   |              |  bash     |  tmux --->|  claude   |
|               |  SSH #2 ---->|  tail -f  |           |  writes   |
|  NDJSON       |<----stdout---|  *.jsonl  |<--shared--|  *.jsonl  |
|  parser       |              |           |  FS       |           |
+---------------+              +-----------+           +-----------+
```

Two SSH connections, one shared filesystem.

## Step by step

### 1. You send a prompt

Operon writes a command into your existing tmux session — preserving
aliases and conda envs:

```bash
cd /scratch/project && claude -p "$PROMPT" \
  --verbose --output-format stream-json \
  > .operon-SESSION.jsonl 2>&1; echo $? > .operon-SESSION.done
```

Output goes to **`.operon-SESSION.jsonl`** on the **shared filesystem** —
not `/tmp`, which is node-local. The login node has to be able to see this
file.

The command runs inside your shell, not `bash -c`, so:

- `claude` resolves to whatever alias your shell has (often
  `npx @anthropic-ai/claude-code`)
- Your `PATH`, conda env, and module loads are active
- Your `$HOME`, `$SCRATCH`, etc. are set

### 2. A separate SSH tails the output

A second SSH session, opened to the **login node**, tails the JSONL stream
and forwards it to Operon:

```bash
ssh login-node "tail -f /scratch/project/.operon-SESSION.jsonl"
```

The login-node tail is base64-encoded and decoded remotely, to avoid the
hell of multi-layer shell quoting (local shell → SSH → remote shell →
bash -c).

When the `.done` file appears, the tail script exits and Operon marks the
session as completed.

### 3. Operon parses + renders

Each NDJSON line is parsed into a typed event:

- `assistant` — text or tool-use content
- `system` — session metadata (e.g. Claude CLI session ID)
- `result` — final usage stats

… and rendered into the chat panel with proper formatting for tool calls,
thinking blocks, code blocks, etc.

### 4. Session metadata persists

Claude's session ID, working directory, SSH profile, and timestamps are
saved to `~/.operon/sessions/<session-id>.json`. On app reopen, Operon
detects the file and offers to reconnect.

If the tmux session is still alive on the cluster, Operon resumes by
re-attaching the tail. If only the `.jsonl` file exists (session ended),
Operon hydrates the messages from the file so the chat history is
preserved.

For follow-up messages in a completed session, Operon passes
`--resume <session-id>` to Claude — picking up Claude's own session
context, not just Operon's view of it.

## Design decisions

| Choice | Why |
|---|---|
| Output on shared FS, not `/tmp` | Login and compute nodes need to see the same file |
| Command runs in user's shell | Preserves aliases (`claude → npx …`), conda env, modules |
| tail via separate SSH | Login node tails, compute node writes — decouples the two |
| Base64-encoded tail script | Avoids multi-layer shell quoting issues |
| SSH stderr filtered | Suppresses benign post-quantum and ControlMaster warnings |
| Session metadata to disk | Survives app restart, machine switch |

## What's NOT happening

- No SSH agent on the compute node
- No SSH from compute → laptop
- No SSH multiplexing across the two SSH connections (separate sockets)
- No proxy daemon on the cluster

## Why two SSH connections instead of one

The interactive tmux session needs to be exclusively yours so you can type
into it. The tail needs to be non-blocking and independent — if you
disconnect, the tail can resume on its own. Multiplexing them onto a
single SSH channel made the disconnect-resume behavior fragile.

Two channels also gives you a place to inject scheduling commands
(`squeue -u $USER`) from the chat without yanking control away from the
tmux session.

## See also

- [SSH connections](ssh.md) — setting up profiles
- [tmux sessions](tmux.md) — the persistence layer
- [HPC gotchas](gotchas.md) — `/tmp` is node-local, aliases, etc.
