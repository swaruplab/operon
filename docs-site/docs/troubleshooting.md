# Troubleshooting

## Installation

### "Operon can't be opened because Apple cannot check it for malicious software"

macOS Gatekeeper hasn't approved Operon yet (we're not a paying Apple
developer). Right-click **Operon** in Applications → **Open**, click
**Open** in the dialog. Once approved, future launches are normal.

If the right-click option doesn't appear: **System Settings → Privacy &
Security**, scroll to the "Operon was blocked" line, click **Open Anyway**.

### Windows SmartScreen blocks the installer

"Windows protected your PC — Unrecognized app". Click
**More info → Run anyway**. We're not yet signed with a Microsoft EV
certificate; SmartScreen reputation builds over time.

### Linux: "error while loading shared libraries: libwebkit2gtk-4.1.so"

Install webkit2gtk:

```bash
# Debian/Ubuntu
sudo apt install libwebkit2gtk-4.1-0

# Fedora
sudo dnf install webkit2gtk4.1
```

If `webkit2gtk-4.0` is installed but not `-4.1`, the AppImage often works
even when the .deb/.rpm doesn't — that's the recommended fallback for
older distros.

### Linux: AppImage refuses to launch on Wayland

Some compositors have issues with our default rendering. Try:

```bash
WEBKIT_DISABLE_DMABUF_RENDERER=1 ./Operon_*.AppImage
```

If that works, add `export WEBKIT_DISABLE_DMABUF_RENDERER=1` to your
shell profile so it sticks.

## Authentication

### "API Error: 400 Either x-portkey-config or x-portkey-provider header is required"

Your Anthropic-direct session is being routed to Portkey because a stale
`ANTHROPIC_BASE_URL` lives in your shell profile. Fixed in **v0.7.3+** —
upgrade.

If you're on an older version, edit `~/.zshrc` / `~/.bash_profile` and
remove `export ANTHROPIC_BASE_URL=...` and `export ANTHROPIC_AUTH_TOKEN=...`,
then restart Operon.

### Custom (Ollama / vLLM) session returns 401 / anonymous

A stale `ANTHROPIC_API_KEY` in your shell profile is being preferred over
the bearer token Operon sends. Fixed in **v0.7.3+** — upgrade. Or remove
the `export ANTHROPIC_API_KEY=...` from your shell profile.

### "Invalid API key" on a Max / Pro subscription, or "connectors are disabled"

Full message from the Claude Code CLI:

```
Invalid API key · Fix external API key
⚠ claude.ai connectors are disabled because ANTHROPIC_API_KEY or another
auth source is set and takes precedence over your claude.ai login ·
Unset it to load your organization's connectors
```

On a subscription Operon deliberately supplies **no** credential — the
Claude Code CLI owns the login. But Operon spawns a *login* shell, so an
`export ANTHROPIC_API_KEY=...` in `~/.zshrc` / `~/.bash_profile` (or, for
remote sessions, the cluster's `~/.bashrc`) gets re-sourced and outranks
that login. The subscription is valid; it just never gets used.

Fixed in **v1.0.1+** — Operon now clears `ANTHROPIC_API_KEY`,
`ANTHROPIC_AUTH_TOKEN` and `ANTHROPIC_BASE_URL` on every path that isn't
explicitly supplying one: local chat, remote HPC sessions, `claude login`
in a terminal tab, the auth check, and the code reviewer.

If you're on an older version, remove the `export` line from the relevant
profile (local **and** remote) and fully quit and relaunch Operon — the
spawned shell re-sources the profile, so the variable comes back until both
are done. Verify with `echo $ANTHROPIC_API_KEY` in an Operon terminal.

One source Operon *cannot* clear for you is an `apiKeyHelper` entry in
`~/.claude/settings.json`. If the message persists after upgrading, check
that file and remove the helper.

### Portkey Bedrock route returns 400 about `requestMetadata`

Claude Code's default `metadata.user_id` JSON blob contains `{`, `}`, and
`"` characters that violate Bedrock's `requestMetadata` regex. Operon
routes non-Anthropic Bedrock models through the translation proxy
automatically (which drops the metadata).

If you're hitting this for an Anthropic-family Bedrock model: the proxy
isn't being used for those. Workaround until we wire it: switch provider
to Anthropic direct, or use a non-Bedrock Portkey route.

### OAuth login: "Could not verify code"

Common causes:

- The browser tab finished authorization but you pasted the wrong code.
  Try again with a fresh OAuth flow.
- Your system clock is off (more than ~5 minutes). Sync via NTP.
- A corporate proxy is mangling the request. Try from a non-managed
  network as a diagnostic.

## SSH / HPC

### Repeated Duo prompts

You don't have SSH ControlMaster enabled, so each operation re-handshakes.
Operon's SSH profiles enable ControlMaster by default — check **Settings →
SSH** that "Reuse connection (ControlMaster)" is on.

### "Permission denied (publickey)" with the correct key

Three usual causes:

1. **File permissions** — `chmod 600 ~/.ssh/id_ed25519` (and 700 on `~/.ssh`).
2. **Wrong key path** — Operon defaults to `id_ed25519` and `id_rsa`. If
   yours has a different name, set it explicitly in the profile.
3. **Passphrase not unlocked** — start `ssh-agent` and `ssh-add` the key
   before launching Operon, or set up a key without passphrase.

### `sntrup761x25519-sha512` warnings flood stderr

These are benign OpenSSH 9.x post-quantum key-exchange warnings. Operon
filters them from the chat panel. If you see them in a manual SSH session,
ignore.

### tmux session not found after reconnect

If `tmux ls` shows no sessions but you expected one, the most common cause
is that you connected with a slightly different username or the tmux
server was killed by an admin cron. Sessions don't survive `tmux
kill-server` or a node reboot.

The fix is operational: always use `tmux new-session -A -s <name>` so a
typo creates a fresh session rather than silently failing.

### Login node killed my SSH session

Cluster admins kill heavy processes on login nodes. **Never run analyses
on the login node.** Always:

```bash
ssh login.hpc.school.edu
srun --pty --mem=16G --time=4:00:00 bash    # SLURM example
tmux new-session -A -s operon
# now work
```

## AI sessions

### Agent mode "stops" without finishing

Two common causes:

- **Max turns hit** — check Settings → Claude → Max turns. Default is 30.
  Bump it for long pipelines, or interject in chat to keep Claude going.
- **Tool error Claude couldn't recover** — check the chat for the last
  tool call; if it's a non-zero exit, ask Claude to retry with a fix.

### "Empty or malformed response" from Portkey

If your Portkey base URL already includes `/v1`, Operon strips it before
passing to the SDK (so it doesn't construct `/v1/v1/messages`). Make sure
you saved a *clean* base URL (no trailing slash, no double v1). Upgrade
to v0.7.0+ if you're on an older release — this was fixed there.

### Claude can't see my conda env

Claude's commands run in the **integrated terminal's shell**. If `conda`
works in the terminal panel but Claude says it doesn't, the most likely
issue is that Operon's chat-spawned shell isn't sourcing the same
profile as the terminal panel. Restart Operon — both spawn the same way,
so a stale process is usually the cause.

### Plan mode generates `implementation_plan.md` but Agent ignores it

The plan file must be in the **same working directory** as the Agent
session. If you changed folders between Plan and Agent, the plan is
invisible to Agent. Quick fix: copy/move the plan to the current working
dir, or restart the session in the original folder.

## Performance

### Operon CPU pegs at 100% on idle

Usually a runaway terminal or an AI session in an infinite tool-call loop.
Check:

1. Hide the terminal panel and see if CPU drops — if so, a process inside
   the terminal is the cause.
2. Check the chat panel for an Agent session that's been retrying the
   same command. Click **Stop** if so.

### File explorer feels slow on a remote NFS

Large NFS mounts (millions of files) take a while to enumerate. Operon's
file explorer lazily loads each subdirectory on click, so it shouldn't
freeze — but the initial open of a deeply-populated folder can take
20-30s. Subsequent opens are cached.

### Monaco lags on large files

Files over ~10 MB strain Monaco. For these (count matrices, BAM dumps),
open in the terminal with `less` or `head` instead — much faster.

## Logs and debugging

Operon writes logs to:

| OS | Path |
|---|---|
| macOS | `~/Library/Logs/com.operon.app/` |
| Windows | `%LOCALAPPDATA%\com.operon.app\logs\` |
| Linux | `~/.local/state/com.operon.app/logs/` |

Per-session AI logs (the raw NDJSON streams) live in `~/.operon/sessions/`.

For bug reports, include:

- Your Operon version (Settings → About)
- OS and version
- The last ~50 lines of the relevant log
- A minimal reproducer if possible

File at [github.com/swaruplab/operon/issues](https://github.com/swaruplab/operon/issues).
