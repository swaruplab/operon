# Architecture

Operon is a [Tauri 2](https://tauri.app) app — Rust backend, React +
TypeScript frontend, native webview on every OS. This page describes the
high-level architecture; for the HPC SSH-tail dance specifically, see
[HPC architecture](hpc/architecture.md).

## Tech stack

| Layer | Technology |
|---|---|
| **App shell** | Tauri 2 (Rust) — ~600KB bundle, 20-40MB RAM, native webview |
| **Backend** | Rust — memory-safe, async (tokio), direct PTY / filesystem access |
| **Frontend** | React 18 + TypeScript + Vite 6 — fast HMR, Monaco/xterm integrate natively |
| **Terminal** | xterm.js + portable-pty (same stack as VS Code + WezTerm) |
| **Editor** | Monaco Editor (`@monaco-editor/react`) — VS Code's editor engine |
| **Layout** | react-resizable-panels — Cursor-style multi-panel with draggable dividers |
| **Styling** | Tailwind CSS 3 + lucide-react icons; CSS-variable themes for light/dark |
| **SSH** | OpenSSH sidecar via portable-pty — ProxyJump, agent forwarding, `~/.ssh/config` |
| **Auth** | macOS Keychain · Windows Credential Manager · libsecret (Linux) |
| **AI engine** | Claude Code (headless `stream-json` NDJSON) + bundled `anthropic-proxy-rs` for OpenAI-compatible backends |
| **Provider routing** | Anthropic direct · Portkey gateways · Custom OpenAI-compatible |

## Project layout

```
operon/
├── src/                         # React/TypeScript frontend
│   ├── components/              # UI components
│   │   ├── chat/                # AI chat panel — streaming, tool display
│   │   ├── editor/              # Monaco editor, diff viewer, file viewer
│   │   ├── terminal/            # xterm.js terminal with tab management
│   │   ├── sidebar/             # File explorer, SSH, Git, Protocols, Help
│   │   ├── layout/              # AppShell, TopBar, ActivityBar, StatusBar
│   │   ├── settings/            # Settings panel
│   │   └── setup/               # First-time setup wizard
│   ├── context/                 # ProjectContext, ThemeContext
│   ├── hooks/                   # useKeyboardShortcuts
│   ├── lib/                     # Typed IPC wrappers
│   │   ├── claude.ts            # checkInstalled, startSession, ...
│   │   ├── files.ts             # list_directory, read_file, write_file, ...
│   │   ├── terminal.ts          # spawn, write, resize, kill
│   │   ├── ssh.ts               # saveProfile, listProfiles, spawnSSH
│   │   ├── portkey.ts           # virtual-key validation, slug heuristics
│   │   └── theme.ts             # Monaco theme registration
│   └── types/                   # TypeScript types (ClaudeEvent, ChatMessage, ...)
│
├── src-tauri/                   # Rust backend (Tauri 2)
│   ├── src/
│   │   ├── main.rs              # Entry point
│   │   ├── lib.rs               # Tauri builder: state managers, command registration
│   │   └── commands/            # IPC command handlers
│   │       ├── terminal.rs      # PTY spawn/write/resize/kill
│   │       ├── files.rs         # File ops (symlink-aware), protocol parsing
│   │       ├── claude.rs        # ClaudeManager, session management, provider env
│   │       ├── ssh.rs           # SSH profiles, remote spawn
│   │       ├── portkey.rs       # Catalog fetch, virtual-key handling
│   │       ├── proxy.rs         # anthropic-proxy sidecar manager
│   │       └── settings.rs      # JSON persistence, keychain wiring
│   ├── binaries/                # Bundled sidecars (anthropic-proxy-rs)
│   ├── protocols/               # Bundled bioinformatics protocols (read-only)
│   └── icons/                   # App icons (.icns, .png, .ico)
│
├── presets/                     # Hot-fetchable Portkey gateway presets
├── docs/                        # Legacy HTML landing pages + shared img/
├── docs-site/                   # This MkDocs documentation site
└── build-*.example.sh           # macOS signed-build script templates
```

## IPC model

| Direction | Mechanism | When |
|---|---|---|
| Frontend → Backend | Tauri **commands** | Request/response: read a file, start a session, save settings |
| Backend → Frontend | Tauri **events** | Streaming: PTY stdout, NDJSON tokens from Claude, file watch events |

The split matters — commands block while running, so streaming output uses
events. The PTY reader runs in a `std::thread::spawn` (not `tokio::spawn`)
because portable-pty's `Read` is synchronous.

## State managers

Each domain has a `Mutex<HashMap>`-backed manager registered via
`.manage()`:

| Manager | Owns |
|---|---|
| `TerminalManager` | Open PTYs by session ID |
| `ClaudeManager` | Running AI sessions (child processes, output channels) |
| `SSHManager` | Connection profiles, active ControlMaster sockets |
| `SettingsManager` | The single `settings.json` blob, in-memory + on-disk |
| `ProxyManager` | The anthropic-proxy sidecar lifecycle |

**Locking rule:** never hold `std::sync::Mutex` across `.await`. Extract,
drop, await.

## AI provider routing

`ai_provider_env` in `src-tauri/src/commands/claude.rs` is the single
source of truth for which env vars get passed to spawned Claude processes.
The provider setting decides which path:

| Provider | Env vars set |
|---|---|
| `anthropic` | `ANTHROPIC_API_KEY` |
| `portkey` (Anthropic model) | `ANTHROPIC_BASE_URL` → Portkey gateway, `ANTHROPIC_AUTH_TOKEN` → virtual key |
| `portkey` (non-Anthropic model) | `ANTHROPIC_BASE_URL` → local anthropic-proxy, `ANTHROPIC_AUTH_TOKEN` → placeholder |
| `custom` | `ANTHROPIC_BASE_URL` → local anthropic-proxy (with `UPSTREAM_BASE_URL` = user URL), `ANTHROPIC_AUTH_TOKEN` |

A parallel `ai_provider_env_unset` clears env vars from a different
provider that may have been re-exported by the user's shell profile.
(See v0.7.3 release notes.)

## anthropic-proxy sidecar

When Operon needs to talk to an OpenAI-compatible backend (Ollama, vLLM,
Portkey non-Anthropic routes), it spawns the bundled `anthropic-proxy-rs`
binary. The proxy:

- Listens on a random local port
- Receives Anthropic-format `/v1/messages` requests from Claude Code
- Translates them to OpenAI `/v1/chat/completions`
- Forwards to the configured `UPSTREAM_BASE_URL`
- Streams the OpenAI response back, re-encoded as Anthropic NDJSON

This is invisible to Claude Code (it thinks it's talking to Anthropic) and
invisible to the upstream (which thinks it's talking to OpenAI).

Binaries shipped:

| Target | File |
|---|---|
| macOS arm64 | `binaries/anthropic-proxy-aarch64-apple-darwin` |
| macOS x64 | `binaries/anthropic-proxy-x86_64-apple-darwin` |
| Windows x64 | `binaries/anthropic-proxy-x86_64-pc-windows-msvc.exe` |
| Linux x64 | `binaries/anthropic-proxy-x86_64-unknown-linux-gnu` |

## Session persistence

When Claude streams its first event, it includes a session ID. Operon
captures it and writes session metadata to:

```
~/.operon/sessions/<session-id>.json
{
  "session_id": "...",
  "project_path": "/Users/...",
  "ssh_profile": "lab-hpc",
  "mode": "agent",
  "status": "running" | "completed",
  "created_at": "...",
  "updated_at": "..."
}
```

On app reopen:

- **Running** sessions → Operon reconnects the SSH tail to the running
  `.jsonl` file
- **Completed** sessions → Operon reads the full `.jsonl`, parses into
  messages, restores the chat history
- **Follow-up messages** in a completed session pass `--resume <session-id>`
  so Claude itself picks up where it left off

## Cross-platform notes

| OS | Quirk |
|---|---|
| macOS | Apple Silicon + Intel both supported; universal binary build available; macOS Keychain for secrets |
| Windows | Git Bash bundled (required by Claude Code); WebView2 runtime; Credential Manager for secrets |
| Linux | webkit2gtk-4.1 required; libsecret / GNOME Keyring / KWallet for secrets; xdg-open for "Reveal in file manager" |

## Build phases (history)

The codebase grew through 12+ phases. See the [Changelog](changelog.md) for
the per-release details. High-level:

1. Scaffolding & layout
2. Integrated terminal (xterm + PTY)
3. File browser
4. Code editor (Monaco)
5. Claude Code integration (NDJSON streaming)
6. SSH + remote
7. Settings & polish
8. HPC terminal mode (tmux + tail + base64 SSH)
9. Session resume
10. Plan mode + `implementation_plan.md`
11. UX polish (theme, file viewer, setup wizard)
12. Robustness (symlinks, SSH stderr filters, unicode)
13. (planned) Native multi-provider agent loop with first-class GPT-5+ / o-series via `/v1/responses`
