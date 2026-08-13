# Methods

## Operon: An AI-Augmented Integrated Development Environment for Computational Biology on High-Performance Computing Clusters

### Software Architecture

Operon is a cross-platform desktop application built on the Tauri 2 framework, which pairs a Rust backend with a browser-based frontend rendered in the operating system's native webview. This architecture was chosen over Electron to minimize resource consumption: Operon's binary is approximately 600 KB and its runtime memory footprint is 20-40 MB, compared to the 150-300 MB typical of Electron-based editors. The Rust backend provides memory-safe concurrency, direct access to pseudoterminal (PTY) allocation, filesystem operations, and SSH session management through the tokio asynchronous runtime.

The frontend is implemented in React 18 with TypeScript, bundled by Vite 6. The user interface follows a multi-panel layout inspired by modern code editors, with a resizable file explorer, code editor, integrated terminal, and AI chat panel arranged using the react-resizable-panels library. The code editor is powered by Monaco Editor, the same editing engine used by Visual Studio Code, providing syntax highlighting for over 30 programming languages, IntelliSense code completion, and side-by-side diff comparison. The terminal emulator uses xterm.js with WebGL-accelerated rendering and the portable-pty crate for native PTY management on macOS, Linux, and Windows.

Communication between the frontend and backend follows Tauri's inter-process communication (IPC) model: synchronous request-response operations use Tauri commands (invoked via the `@tauri-apps/api` JavaScript bridge), while asynchronous data streams such as terminal output and AI model responses use Tauri's event emission system. This separation ensures that long-running operations such as AI inference or file transfers do not block the user interface.

### AI Agent Integration

Operon integrates large language models (LLMs) through Claude Code, Anthropic's command-line interface for agentic AI coding, operating in headless mode with structured NDJSON (newline-delimited JSON) output. When a user submits a prompt, the Rust backend spawns a Claude Code process with the `--output-format stream-json` flag, which emits a stream of typed events including assistant messages, tool invocations (file edits, shell commands, search operations), thinking traces, and cost accounting. The frontend parses this stream incrementally, rendering each event type with appropriate visual treatment: collapsible thinking blocks, syntax-highlighted code diffs for file edits, and inline terminal output for shell commands.

Session management enables conversation continuity across application restarts. Each AI session is persisted to disk as JSON metadata containing the Claude Code session identifier, project path, SSH profile reference, execution mode, and timestamps. Completed sessions can be resumed using Claude Code's `--resume` flag, which replays the full conversation context. Running sessions on remote servers are reconnected by re-establishing the SSH tail process that streams the NDJSON output file.

To support environments where Anthropic's API is unavailable or where data governance requirements prohibit sending data to external services, Operon includes a translation proxy sidecar. This Rust-based proxy (approximately 3 MB) translates between the Anthropic Messages API format that Claude Code expects and the OpenAI Chat Completions API format spoken by local inference servers such as Ollama, vLLM, and LM Studio. The proxy binds to a random available port on localhost and is managed by the application lifecycle: started lazily when a custom AI provider is configured and terminated on application exit.

### HPC Terminal Mode

The primary execution mode for HPC users runs AI agents inside existing terminal multiplexer (tmux) sessions on compute nodes. This design preserves the user's shell environment, including conda/mamba environments, module-loaded tools, shell aliases, and scheduler-specific configurations that would be lost if commands were piped through a separate bash process.

The execution flow operates as follows: (1) the Rust backend constructs a shell command that invokes Claude Code with the user's prompt and writes structured output to a JSONL file on the shared filesystem; (2) this command is injected into the user's existing tmux session via the PTY; (3) a separate SSH connection from Operon to the login node tails the output file and streams it back to the application via Tauri events; (4) a sentinel file (`.done`) signals completion, at which point the tail process exits and cleanup runs.

Output files are written to the project's working directory on a shared filesystem (NFS, GPFS, or Lustre) rather than `/tmp`, which is node-local on most HPC clusters and invisible from the login node. Shell commands sent via SSH are base64-encoded before transmission to avoid multi-layer quoting issues that arise from the local shell, SSH transport, remote shell, and tmux input chain. Large file transfers (over 100 KB) are chunked into segments to respect the Unix domain socket message size limit imposed by SSH ControlMaster multiplexing.

### Remote File Operations and SSH Management

Operon manages remote connections through SSH profiles that store host, user, port, authentication type (password, key-based, or Duo MFA with push/passcode), and optional ControlMaster multiplexing settings. Rather than implementing SSH in Rust, Operon spawns the system's OpenSSH binary as a child process, inheriting the user's `~/.ssh/config` including ProxyJump chains, agent forwarding, and custom identity files. This approach ensures compatibility with institutional SSH configurations that often involve multi-hop bastion hosts.

Remote filesystem operations (directory listing, file read/write, create, delete, rename) are implemented as individual SSH command executions. Directory listings use `ls -L` to follow symlinks, which is common in HPC environments where project directories, software modules, and shared datasets are frequently symlinked. File writes encode content as base64 and transmit it as a shell command argument, decoded on the remote side; files exceeding 100 KB are chunked and reassembled to avoid SSH socket limits.

### Job Status Reporting

For HPC batch jobs, Operon reports scheduler state on demand rather than running any resident process on the cluster. A single SSH round-trip issues `squeue` for the user's live jobs and `sacct` over a bounded recent window for finished ones, merging the two into one view in which the live record takes precedence; queries are made only while the jobs panel is open. Job logs are read on request with a line- and byte-bounded `tail`. Operon maintains a purely local registry mapping scheduler job identifiers to the chat session that submitted them, so a completion can be attributed and surfaced in the interface, and this registry is aged out to bound its growth. Nothing is written to the cluster for tracking purposes.

This design is deliberate. An earlier version deployed a persistent polling daemon to the remote host, which is incompatible with the shared-resource policies of many HPC sites: administrators terminate long-lived processes on login nodes, and such a daemon is repeatedly killed and restarted. Because Operon must therefore not be running for a job to be tracked, notification that survives the application being closed is delegated to the scheduler itself — users may configure an address per server profile, which Operon injects as `--mail-user` and `--mail-type` directives into generated batch scripts. On clusters without SLURM accounting (`sacct`), completed jobs leave no queryable record and the interface states this explicitly rather than reporting an absence of results.

### Automated Report Generation

Operon includes a report generation pipeline that scans project files, extracts method signatures and documentation from source code using regex-based parsing, reads CSV data files for summary statistics, and compiles findings into structured PDF reports. The scanning phase operates on both local and remote filesystems, with batch file preview reading to minimize SSH round trips. Users can scope reports to specific directories, file types, or analysis protocols.

### Extension System

Operon supports VS Code-compatible extensions from the Open VSX registry, providing language servers (LSP), syntax themes, and code snippets. Extensions are installed locally or on remote servers and managed through a dedicated UI. Language server communication follows the Language Server Protocol over JSON-RPC, with the Rust backend managing server lifecycles and the frontend routing LSP messages through the Tauri IPC bridge. Docker and Singularity/Apptainer container management is integrated for environments where analysis tools are containerized.

### Protocol System

Operon includes a protocol management system for standardizing bioinformatics analysis workflows. Protocols are stored as structured documents in the application's data directory and can be generated from natural language descriptions using the integrated AI, generated from existing analysis scripts by extracting steps and parameters, or authored manually. The protocol system is designed to capture the decision chain of a computational analysis: input data, software versions, parameter choices, and expected outputs.

### Build and Distribution

Operon is built for macOS (Apple Silicon and Intel via universal binary), Windows (MSI and NSIS installers), and Linux (Debian packages and AppImage). Automated builds use GitHub Actions with platform-specific runners. The application includes a built-in auto-updater that checks a GitHub Releases endpoint for signed update manifests, downloads differential updates, and prompts the user to restart. Update signatures use the minisign scheme to verify artifact integrity.

### Software Availability

Operon is available at https://github.com/swaruplab/operon under [license]. The application requires macOS 10.15+, Windows 10+, or a Linux distribution with WebKitGTK 4.1 and GTK 3. Claude Code (installed separately) is required for AI features. An Anthropic API key or a locally running LLM server (Ollama, vLLM) is required for AI inference.

### Implementation Statistics

The Operon codebase comprises approximately 30 Rust source files implementing the backend (terminal management, AI session orchestration, SSH operations, filesystem access, extension management, job monitoring, and report generation) and approximately 57 TypeScript/React source files implementing the frontend (panel layout, Monaco editor integration, xterm.js terminal, AI chat with streaming NDJSON parsing, settings management, and setup wizard). The Rust backend depends on 15 crates including tauri 2, tokio, portable-pty, serde, reqwest, ssh2, and regex. The frontend depends on 12 npm packages including React 18, Monaco Editor, xterm.js, and Tailwind CSS 3. Over 200 Tauri commands are registered at the IPC boundary.
