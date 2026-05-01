# Operon Architecture

## Overview

Operon is a desktop IDE for researchers, built with Tauri 2 (Rust backend + React/TypeScript frontend). It wraps Claude Code into a professional multi-panel developer environment with an integrated terminal, file browser, Monaco code editor, and AI chat interface. Designed for scientists who work on HPC compute clusters — it runs Claude agents directly on remote compute nodes via SSH and tmux, with sessions that persist across app restarts.

## Tech Stack

| Component | Technology | Why |
|-----------|-----------|-----|
| App Shell | Tauri 2 | ~600KB bundle, 20-40MB RAM, native webview |
| Backend | Rust | Memory-safe, async (tokio), direct PTY/filesystem access |
| Frontend | React 18 + TypeScript + Vite 6 | Fast HMR, Monaco/xterm.js integrate natively |
| Terminal | xterm.js + portable-pty | Same stack as VS Code + WezTerm |
| Editor | Monaco Editor (@monaco-editor/react) | VS Code's editor engine |
| Layout | react-resizable-panels | Cursor-style multi-panel with draggable dividers |
| Styling | Tailwind CSS 3 + lucide-react icons | Utility-first dark theme |
| SSH | OpenSSH sidecar via portable-pty | Handles ProxyJump, agent forwarding, ~/.ssh/config |
| Auth | In-memory (production: macOS Keychain via keyring) | Secure API key storage |
| AI | Claude Code headless mode (stream-json) | Structured NDJSON streaming |

## Project Structure

```
operon/
├── src-tauri/                          # Rust backend
│   ├── src/
│   │   ├── main.rs                     # Entry point
│   │   ├── lib.rs                      # Tauri Builder: state managers, command registration, window cleanup
│   │   └── commands/
│   │       ├── mod.rs                  # Re-exports all commands
│   │       ├── terminal.rs             # TerminalManager + spawn/write/resize/kill PTY
│   │       ├── files.rs                # list_directory (symlink-aware), read_file, write_file, create, delete, rename
│   │       ├── claude.rs               # ClaudeManager + session management + dependency checking + setup wizard backend
│   │       ├── ssh.rs                  # SSHManager + profiles + spawn_ssh_terminal + remote file ops
│   │       └── settings.rs             # SettingsManager + get/update with JSON persistence + setup_completed flag
│   ├── Cargo.toml                      # Deps: tauri 2, portable-pty 0.8, serde, tokio, dirs, base64
│   └── tauri.conf.json                 # Window config, CSP, bundler settings
│
├── src/                                 # React frontend
│   ├── main.tsx                         # Entry point
│   ├── App.tsx                          # Root: setup wizard gate, ProjectProvider, Monaco theme init
│   ├── styles.css                       # Tailwind directives, custom scrollbar
│   │
│   ├── context/
│   │   └── ProjectContext.tsx            # Shared state: project path, editor tabs (open/close/save/diff)
│   │
│   ├── types/
│   │   └── chat.ts                      # ClaudeEvent types, ChatMessage, ContentBlock, ToolUseBlock, SessionMetadata, SessionFileStatus
│   │
│   ├── lib/
│   │   ├── terminal.ts                  # Typed wrappers: spawn/write/resize/kill + event listeners
│   │   ├── files.ts                     # Typed wrappers: listDirectory, readFile, writeFile, etc.
│   │   ├── claude.ts                    # Typed wrappers: checkInstalled, auth, startSession, etc.
│   │   ├── ssh.ts                       # Typed wrappers: saveProfile, listProfiles, spawnSSH
│   │   ├── settings.ts                  # AppSettings interface + defaults + get/update
│   │   └── theme.ts                     # Monaco theme registration (operon-dark)
│   │
│   ├── hooks/
│   │   └── useKeyboardShortcuts.ts      # Global keydown listener with meta/shift/alt matching
│   │
│   └── components/
│       ├── layout/
│       │   ├── AppShell.tsx              # Root layout: panels, visibility state, shortcuts, settings
│       │   ├── TopBar.tsx                # 40px bar: traffic light spacer, branding, status, settings button
│       │   ├── ActivityBar.tsx           # 48px vertical icon strip: Files/Search/SSH/Settings
│       │   ├── StatusBar.tsx             # 24px bottom bar: git branch, cursor pos, panels
│       │   └── CommandPalette.tsx        # Modal fuzzy search over commands
│       │
│       ├── sidebar/
│       │   ├── Sidebar.tsx               # Routes activeView; local explorer with Go-to-folder path bar
│       │   ├── SSHView.tsx               # Connection manager: add/edit/delete/connect profiles
│       │   └── RemoteExplorer.tsx        # Remote file tree with Go-to-folder, symlink-aware ls -L, cd-to-terminal
│       │
│       ├── editor/
│       │   ├── EditorArea.tsx            # Tab bar + Monaco/DiffViewer routing from ProjectContext
│       │   ├── CodeEditor.tsx            # Monaco wrapper: language detection, Cmd+S, theme
│       │   ├── DiffViewer.tsx            # Side-by-side/inline diff with accept/reject
│       │   └── FileViewer.tsx            # Image/PDF viewer with zoom, download (Blob+ObjectURL), expand
│       │
│       ├── terminal/
│       │   ├── TerminalArea.tsx           # Tab management: create/close/switch, exit detection
│       │   └── TerminalInstance.tsx       # xterm.js + IPC: fit, resize, WebGL, auto-copy on selection
│       │
│       ├── chat/
│       │   └── ChatPanel.tsx             # Claude chat: streaming, thinking blocks, tool display, session resume, plan detection
│       │
│       ├── settings/
│       │   └── SettingsPanel.tsx          # Modal: editor/terminal/claude/auth settings
│       │
│       └── setup/
│           └── SetupWizard.tsx           # First-time setup: dependency checks, Claude Code install, API key
│
├── package.json
├── vite.config.ts
├── tsconfig.json
├── tailwind.config.js
└── postcss.config.js
```

## Key Architectural Rules

1. **IPC Pattern**: Tauri commands for request/response, Tauri events for streaming. Never HTTP/WebSocket.
2. **Terminal Output**: Uses events (`pty-output-{id}`), not commands. Commands would block.
3. **PTY Reader**: Runs in `std::thread::spawn` (not `tokio::spawn`) because portable-pty's Read is synchronous.
4. **Resize**: Always `fitAddon.fit()` FIRST, then `invoke('resize_terminal')`. Reverse causes garbled output.
5. **Hidden Terminals**: Use `visibility: hidden` not `display: none` to preserve buffer state.
6. **Secrets**: In-memory for dev. Production uses macOS Keychain via `keyring` crate.
7. **SSH**: Spawned OpenSSH binary (not Rust SSH library) for ProxyJump, agent forwarding, ~/.ssh/config.
8. **Claude Code**: `claude -p --verbose --output-format stream-json` gives NDJSON. Parsed line-by-line.
9. **State Managers**: Each domain has its own `Mutex<HashMap>` struct registered via `.manage()`.
10. **Locking**: Never hold `std::sync::Mutex` across `.await`. Extract data, drop lock, then await.
11. **Terminal Mode**: Commands must run in the terminal's own shell (not piped to `bash`) to preserve user aliases like `claude → npx @anthropic-ai/claude-code`.
12. **HPC /tmp**: Node-local on HPC clusters. Use shared NFS/GPFS paths (like the working directory) for output files accessible from both login and compute nodes.
13. **Symlinks**: Use `std::fs::metadata()` (follows symlinks) for local files, `ls -L` for remote files.
14. **SSH Quoting**: Base64-encode complex scripts sent via SSH to avoid multi-layer shell expansion issues (local shell → SSH → remote shell → bash -c).

## HPC Terminal Mode Architecture

Terminal mode is the primary execution mode for HPC users. It runs Claude agents inside existing tmux sessions on compute nodes.

### Execution Flow

```
1. User sends prompt in ChatPanel
2. Backend writes command to existing terminal PTY:
     _o='/path/.operon-SESSION.jsonl'; _d='/path/.operon-SESSION.done'
     cd '/path' && claude ... > "$_o" 2>&1; echo $? > "$_d"
3. Command runs inside terminal's shell (preserving aliases, conda env, etc.)
4. Separate SSH connection tails the .jsonl output file from the login node:
     ssh user@login-node "echo BASE64_SCRIPT | base64 -d | bash"
5. Tail script waits for file, then streams it back via stdout
6. Backend emits each line as a Tauri event → frontend parses NDJSON
7. When .done file appears, tail exits and cleanup runs
```

### Key Design Decisions

- **Output on shared filesystem** (`{remote_path}/.operon-{id}.jsonl`), not `/tmp` which is node-local on HPC
- **Terminal command runs in user's shell** (not piped to `bash`) to preserve aliases
- **Tail command uses base64 encoding** to avoid quoting issues across the SSH chain
- **SSH stderr is filtered** for post-quantum warnings, connection messages, etc.
- **Session metadata persisted** to `~/.operon/sessions/{id}.json` for resume across app restarts

## Session Management

Sessions persist across app restarts:

- **On session start**: Metadata saved to `~/.operon/sessions/{session_id}.json` (Claude CLI session ID, project path, SSH profile, mode, timestamps)
- **During streaming**: Claude CLI session ID captured from `system` event and persisted for `--resume`
- **On session end**: Status updated to "completed"
- **On app reopen**: Frontend checks for previous sessions, shows resume banner if found
- **Resume flow**: For running sessions → reconnect tail to .jsonl file. For completed sessions → read full .jsonl, parse into messages, restore chat. Follow-up messages use `--resume SESSION_ID`.

## Plan Mode

- Generates `implementation_plan.md` in the working directory
- Auto-detected on session start: if `implementation_plan.md` exists, its content is injected as context
- Agent mode tracks progress by marking steps `[x]` in the plan file
- Plan mode limited to 3 turns; agent mode uses configurable `max_turns`

## First-Time Setup Wizard

On first launch (`setup_completed: false` in settings):

1. **Welcome screen** with branding
2. **Dependency check**: Xcode CLI tools, Claude Code
3. **Installation**: Individual or "Install All Missing" (Xcode via `xcode-select --install`, Claude via `curl -fsSL https://claude.ai/install.sh | bash`, falls back to npm if curl fails)
4. **Authentication**: API key input or skip for OAuth later
5. **Completion**: Sets `setup_completed: true` in settings — never shown again

Remote server dependency checking available via `check_remote_claude` / `install_remote_claude` commands.

## Stream-JSON Deduplication

Claude Code's `--output-format stream-json` sends cumulative content for each assistant message. Each `assistant` event contains the FULL content for that message so far.

- Track message IDs with a `seenMsgIds` ref
- **Same ID = replace** content (intra-message content update)
- **New ID = append** as new message (new turn in conversation)
- Prevents duplicate tool blocks and text

## Color Palette

```
Background:    #09090b (zinc-950)
Panel bg:      #18181b (zinc-900)
Borders:       #27272a (zinc-800)
Surface:       #3f3f46 (zinc-700)
Text primary:  #fafafa (zinc-50)
Text muted:    #71717a (zinc-500)
Accent:        #3b82f6 (blue-500)
Error:         #ef4444 (red-500)
Success:       #22c55e (green-500)
Warning:       #eab308 (yellow-500)
```

## Rust Crate Dependencies

```toml
tauri = "2"
tauri-plugin-shell = "2"
portable-pty = "0.8"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["full"] }
uuid = { version = "1", features = ["v4"] }
anyhow = "1"
thiserror = "1"
dirs = "5"
base64 = "0.22"
```

## Frontend NPM Dependencies

```
@tauri-apps/api ^2, @tauri-apps/plugin-shell ^2
@monaco-editor/react ^4.7, monaco-editor ^0.55
@xterm/xterm ^6, @xterm/addon-fit, @xterm/addon-webgl, @xterm/addon-web-links
react ^18.3, react-dom ^18.3
react-resizable-panels ^2.1
lucide-react ^0.468
tailwindcss ^3.4, vite ^6
```

## Build Phases

- **Phase 1** ✅ Scaffolding & Layout — Tauri 2 scaffolding, resizable panels, TopBar, ActivityBar, StatusBar, CommandPalette, keyboard shortcuts
- **Phase 2** ✅ Integrated Terminal — xterm.js + portable-pty, bidirectional IPC bridge, 100ms debounced resize, tab management, process cleanup on window close
- **Phase 3** ✅ File Browser — Rust file commands (list/read/write/create/delete/rename), real FileTree with lazy directory loading, search across files
- **Phase 4** ✅ Code Editor — Monaco with custom dark theme (operon-dark), 30+ language detection, tab management via ProjectContext, DiffViewer with accept/reject
- **Phase 5** ✅ Claude Code Integration — Detection & installation, API key auth, headless stream-json NDJSON parsing, chat UI with tool use display, model selector, cost tracking, session resume
- **Phase 6** ✅ SSH Remote — Connection profiles (CRUD), SSH terminal via spawned OpenSSH + PTY, connection manager UI with connect/edit/delete
- **Phase 7** ✅ Polish — Settings panel (editor/terminal/claude/auth), JSON config persistence, Cmd+comma shortcut, error handling
- **Phase 8** ✅ HPC Terminal Mode — Run Claude agents inside existing tmux/compute node sessions, output on shared filesystem, separate SSH tail for streaming, alias-aware command injection
- **Phase 9** ✅ Session Resume — Persist session metadata to disk, detect running/completed sessions on app reopen, reconnect tail for running sessions, hydrate messages for completed sessions, `--resume` for follow-up messages
- **Phase 10** ✅ Plan Mode — `implementation_plan.md` generation and auto-detection, context injection, progress tracking with `[x]` markers
- **Phase 11** ✅ UX Polish — Collapsible thinking blocks, stream deduplication, FileViewer (zoom/download/expand), Go-to-folder path bar (local + remote), auto-copy on terminal selection, settings button in TopBar, first-time setup wizard with dependency checks
- **Phase 12** ✅ Robustness — Symlink-aware file listing (local + remote), SSH stderr warning filters, `--dangerously-skip-permissions` for headless execution, Unicode fix for bullet characters in JSX

## Running the App

```bash
# Development
cd operon
npm install
cargo tauri dev

# Production build (macOS)
cargo tauri build

# Share with colleagues (no Xcode/signing needed)
# Output: src-tauri/target/release/bundle/macos/Operon.dmg
# Recipient: right-click → Open to bypass Gatekeeper
```

## Known Gotchas

- **`claude` is an alias on HPC**: Resolves to `npx @anthropic-ai/claude-code`. Terminal commands must run in the user's shell to access the alias. Never pipe through `| bash` or the alias is lost.
- **`/tmp` is node-local on HPC**: Output files written on a compute node are invisible from the login node's `/tmp`. Always use the shared working directory.
- **SSH post-quantum warnings**: OpenSSH 9.x emits warnings about sntrup/mlkem key exchange. These are filtered from stderr to avoid false error messages.
- **`--output-format stream-json` requires `--verbose`**: Without it, Claude Code exits with an error.
- **xterm.js Unicode in JSX**: `\u25CF` in JSX text is rendered literally. Must use `{'\u25CF'}` (JavaScript expression).
- **Lucide icon click interception**: SVG icons inside buttons intercept pointer events. Add `pointer-events-none` class to icons.
- **FileViewer download in Tauri**: Data URIs are blocked by Tauri webview CSP. Use Blob + `URL.createObjectURL()` instead.

## graphify

This project has a graphify knowledge graph at graphify-out/.

Rules:
- Before answering architecture or codebase questions, read graphify-out/GRAPH_REPORT.md for god nodes and community structure
- If graphify-out/wiki/index.md exists, navigate it instead of reading raw files
- For cross-module "how does X relate to Y" questions, prefer `graphify query "<question>"`, `graphify path "<A>" "<B>"`, or `graphify explain "<concept>"` over grep — these traverse the graph's EXTRACTED + INFERRED edges instead of scanning files
- After modifying code files in this session, run `graphify update .` to keep the graph current (AST-only, no API cost)
