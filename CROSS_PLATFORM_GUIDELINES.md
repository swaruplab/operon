# Operon Cross-Platform Development Guidelines

> **Audience**: Every developer contributing to Operon.
> **Scope**: Architecture, workflow, CI/CD, coding standards, and per-platform reference for macOS, Windows 11, and Linux.
> **Status**: Living document — update as the platform layer evolves.

---

## Table of Contents

1. [Guiding Principles](#1-guiding-principles)
2. [Architecture: The Platform Abstraction Layer](#2-architecture-the-platform-abstraction-layer)
3. [Directory Structure](#3-directory-structure)
4. [The Platform Trait API](#4-the-platform-trait-api)
5. [Branching Strategy & Workflow](#5-branching-strategy--workflow)
6. [CI/CD Pipeline](#6-cicd-pipeline)
7. [How to Add a New Feature](#7-how-to-add-a-new-feature)
8. [How to Add a Platform-Specific Feature](#8-how-to-add-a-platform-specific-feature)
9. [Frontend Cross-Platform Rules](#9-frontend-cross-platform-rules)
10. [Backend Coding Standards](#10-backend-coding-standards)
11. [Dependency Management](#11-dependency-management)
12. [Setup Wizard Per Platform](#12-setup-wizard-per-platform)
13. [Testing Strategy](#13-testing-strategy)
14. [Release & Distribution](#14-release--distribution)
15. [Platform Reference Tables](#15-platform-reference-tables)
16. [Migration Checklist](#16-migration-checklist-current-codebase)
17. [Troubleshooting Common Build Failures](#17-troubleshooting-common-build-failures)

---

## 1. Guiding Principles

**Rule 1: Platform code lives in `platform/`, nowhere else.**
If you are about to type `#[cfg(target_os = ...)]`, `osascript`, `/bin/zsh`, `cmd.exe`, `xdg-open`, or any OS-specific path inside a command file, stop. That code belongs in `platform/`. Command files should read like pseudocode.

**Rule 2: CI gates every platform on every PR.**
No code reaches `dev` unless it compiles and passes tests on macOS, Windows, and Linux. This is the single most important guarantee. A feature that compiles on your Mac but breaks on Windows is a bug, and CI should catch it before review.

**Rule 3: New features are platform-agnostic by default.**
If a feature genuinely needs OS-specific behavior (like dictation or native keychain), it goes through the platform trait with graceful degradation. The frontend queries capability flags and hides or adapts UI accordingly. A missing platform implementation is never a crash — it is a disabled button or a polite message.

**Rule 4: macOS is the primary development platform, not the only platform.**
You develop on your Mac. CI validates everywhere. If you break Windows or Linux, you fix it before merging — not "later."

**Rule 5: Shared filesystem conventions.**
All file paths manipulated in Rust must use `std::path::PathBuf` (never string concatenation with `/`). All user-facing paths displayed in the frontend must be normalized. All data storage must use the `dirs` crate, never hardcoded paths like `~/.operon/`.

---

## 2. Architecture: The Platform Abstraction Layer

### Why This Exists

Today, the codebase has **128+ platform-specific call sites** scattered across 6 backend files. Here is a sample of what they look like:

```rust
// claude.rs — appears 3 times
let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
let mut cmd = std::process::Command::new(&shell);
cmd.arg("-l").arg("-c").arg(command);

// claude.rs — macOS-only installer
let _ = std::process::Command::new("osascript").arg("-e").arg(&applescript).output();

// claude.rs — hardcoded macOS paths
let npm_path = if std::path::Path::new("/opt/homebrew/bin/npm").exists() {
    "/opt/homebrew/bin/npm"
} else if std::path::Path::new("/usr/local/bin/npm").exists() {
    "/usr/local/bin/npm"
} else { "npm" };

// ssh.rs — Unix domain sockets
let sock_dir = home.join(".operon").join("sockets");

// settings.rs — macOS Swift dictation
let _ = std::process::Command::new("swift").arg(&script_path).spawn();

// mod.rs — Python-based URL opening
std::process::Command::new("python3").args(["-c", &python_cmd]).spawn();
```

Every one of these is a Windows/Linux build failure or runtime crash. The platform layer eliminates this by providing a single, tested interface.

### How It Works

```
                    ┌─────────────────────────┐
                    │   Command Files          │
                    │   (terminal.rs, etc.)    │
                    │                          │
                    │   Only calls:            │
                    │   platform::shell_exec() │
                    │   platform::home_dir()   │
                    │   platform::install()    │
                    └──────────┬──────────────┘
                               │
                    ┌──────────▼──────────────┐
                    │   platform/mod.rs        │
                    │   (trait + dispatch)     │
                    └──────────┬──────────────┘
                               │
            ┌──────────────────┼──────────────────┐
            │                  │                   │
   ┌────────▼────────┐ ┌──────▼──────┐ ┌─────────▼────────┐
   │ platform/macos.rs│ │platform/    │ │platform/linux.rs  │
   │                  │ │windows.rs   │ │                   │
   │ /bin/zsh -l -c   │ │cmd /C       │ │/bin/bash -l -c    │
   │ osascript         │ │powershell   │ │xdg-open           │
   │ Keychain          │ │Credential   │ │Secret Service     │
   │ ControlMaster     │ │ssh-agent    │ │ControlMaster      │
   │ brew/curl         │ │winget/npm   │ │apt/curl           │
   └──────────────────┘ └─────────────┘ └───────────────────┘
```

---

## 3. Directory Structure

```
src-tauri/src/
├── main.rs                              # Entry point (no platform code)
├── lib.rs                               # Tauri builder (calls platform::build_menu)
│
├── platform/                            # ALL OS-specific code
│   ├── mod.rs                           # Public API: function signatures + cfg dispatch
│   ├── macos.rs                         # macOS implementations
│   ├── windows.rs                       # Windows implementations
│   ├── linux.rs                         # Linux implementations
│   └── common.rs                        # Shared utilities (path normalization, etc.)
│
├── commands/                            # Business logic (platform-agnostic)
│   ├── mod.rs                           # Re-exports
│   ├── terminal.rs                      # PTY spawning, resize, kill
│   ├── claude.rs                        # Session management, streaming, auth
│   ├── ssh.rs                           # SSH profiles, remote exec, file ops
│   ├── settings.rs                      # Settings CRUD, dictation wrapper
│   ├── git.rs                           # Git status, commit, push
│   ├── files.rs                         # Local file CRUD
│   ├── extensions.rs                    # Extension marketplace
│   ├── mcp.rs                           # MCP server management
│   ├── knowledge.rs                     # PubMed search
│   └── report.rs                        # PDF report generation
│
└── Cargo.toml
```

### What goes where

| If your code does this... | It goes in... |
|--------------------------|---------------|
| Runs a shell command | `platform/` |
| References a file path like `/opt/homebrew` or `%APPDATA%` | `platform/` |
| Calls `osascript`, `powershell`, `xdg-open` | `platform/` |
| Installs software (brew, apt, winget, npm, curl) | `platform/` |
| Manages Unix sockets or named pipes | `platform/` |
| Builds the native menu bar | `platform/` |
| Starts native speech recognition | `platform/` |
| Opens a URL in the default browser | `platform/` |
| Manages secure credential storage (Keychain, Credential Store) | `platform/` |
| Parses Claude NDJSON output | `commands/claude.rs` |
| Manages SSH profiles in memory | `commands/ssh.rs` |
| Handles editor tab state | Frontend `ProjectContext.tsx` |
| Everything else (business logic, data structures, IPC) | `commands/` |

---

## 4. The Platform Trait API

### `platform/mod.rs`

This is the public interface. Every function dispatches to the correct OS module via `#[cfg]`.

```rust
// src-tauri/src/platform/mod.rs

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "linux")]
mod linux;
mod common;

pub use common::*;

// ─── Shell Execution ─────────────────────────────────────────────

/// Run a command string through the OS's login shell.
///
/// macOS:   /bin/zsh -l -c "command"
/// Windows: cmd.exe /C "command"
/// Linux:   /bin/bash -l -c "command"
///
/// This is the ONLY way command files should execute shell commands.
/// Never construct a Command::new("/bin/zsh") directly.
pub fn shell_exec(command: &str) -> std::process::Command {
    #[cfg(target_os = "macos")]
    { macos::shell_exec(command) }
    #[cfg(target_os = "windows")]
    { windows::shell_exec(command) }
    #[cfg(target_os = "linux")]
    { linux::shell_exec(command) }
}

/// Async version for use in async Tauri commands.
pub fn shell_exec_async(command: &str) -> tokio::process::Command {
    #[cfg(target_os = "macos")]
    { macos::shell_exec_async(command) }
    #[cfg(target_os = "windows")]
    { windows::shell_exec_async(command) }
    #[cfg(target_os = "linux")]
    { linux::shell_exec_async(command) }
}

/// The default interactive shell for terminal spawning.
///
/// macOS:   $SHELL or /bin/zsh
/// Windows: $COMSPEC or cmd.exe
/// Linux:   $SHELL or /bin/bash
pub fn default_shell() -> String {
    #[cfg(target_os = "macos")]
    { macos::default_shell() }
    #[cfg(target_os = "windows")]
    { windows::default_shell() }
    #[cfg(target_os = "linux")]
    { linux::default_shell() }
}

// ─── Directories ─────────────────────────────────────────────────

/// The user's home directory.
///
/// macOS:   /Users/{user}
/// Windows: C:\Users\{user}
/// Linux:   /home/{user}
pub fn home_dir() -> Option<std::path::PathBuf> {
    dirs::home_dir()
}

/// Operon's application data directory (sessions, SSH profiles, caches).
///
/// macOS:   ~/Library/Application Support/operon/
/// Windows: C:\Users\{user}\AppData\Local\operon\
/// Linux:   ~/.local/share/operon/
pub fn data_dir() -> std::path::PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default())
        .join("operon")
}

/// Operon's configuration directory (settings.json).
///
/// macOS:   ~/Library/Application Support/operon/
/// Windows: C:\Users\{user}\AppData\Roaming\operon\
/// Linux:   ~/.config/operon/
pub fn config_dir() -> std::path::PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default())
        .join("operon")
}

/// Session metadata directory.
pub fn sessions_dir() -> Result<std::path::PathBuf, String> {
    let dir = data_dir().join("sessions");
    if !dir.exists() {
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("Failed to create sessions dir: {}", e))?;
    }
    Ok(dir)
}

/// SSH socket/multiplexing directory.
pub fn ssh_sockets_dir() -> std::path::PathBuf {
    let dir = data_dir().join("sockets");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Temp directory (for install scripts, etc.)
pub fn temp_dir() -> std::path::PathBuf {
    std::env::temp_dir()
}

// ─── Tool Discovery ──────────────────────────────────────────────

/// Check if a CLI tool is installed and return (path, version).
///
/// macOS/Linux: `which {name}` then `{name} --version`
/// Windows:     `where.exe {name}` then `{name} --version`
pub fn check_tool(name: &str) -> Option<(String, String)> {
    #[cfg(target_os = "macos")]
    { macos::check_tool(name) }
    #[cfg(target_os = "windows")]
    { windows::check_tool(name) }
    #[cfg(target_os = "linux")]
    { linux::check_tool(name) }
}

/// Additional directories to search for tools beyond $PATH.
///
/// macOS:   ~/.operon/node/bin, /opt/homebrew/bin, /usr/local/bin
/// Windows: %APPDATA%\npm, %ProgramFiles%\nodejs
/// Linux:   ~/.operon/node/bin, /usr/local/bin, ~/.local/bin
pub fn extra_tool_paths() -> Vec<std::path::PathBuf> {
    #[cfg(target_os = "macos")]
    { macos::extra_tool_paths() }
    #[cfg(target_os = "windows")]
    { windows::extra_tool_paths() }
    #[cfg(target_os = "linux")]
    { linux::extra_tool_paths() }
}

// ─── Installation ────────────────────────────────────────────────

/// Platform-specific dependency check for the setup wizard.
pub fn check_dependencies() -> super::commands::claude::DependencyStatus {
    #[cfg(target_os = "macos")]
    { macos::check_dependencies() }
    #[cfg(target_os = "windows")]
    { windows::check_dependencies() }
    #[cfg(target_os = "linux")]
    { linux::check_dependencies() }
}

/// Install Node.js using the best method for this platform.
///
/// macOS:   brew install node, fallback to tarball in ~/.operon/node/
/// Windows: winget install OpenJS.NodeJS.LTS, fallback to .zip extraction
/// Linux:   apt install nodejs (if sudo), fallback to tarball
pub fn install_node() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    { macos::install_node() }
    #[cfg(target_os = "windows")]
    { windows::install_node() }
    #[cfg(target_os = "linux")]
    { linux::install_node() }
}

/// Install Claude Code.
///
/// All platforms: `curl -fsSL https://claude.ai/install.sh | bash` (Unix)
///                or npm install -g @anthropic-ai/claude-code (all)
pub fn install_claude() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    { macos::install_claude() }
    #[cfg(target_os = "windows")]
    { windows::install_claude() }
    #[cfg(target_os = "linux")]
    { linux::install_claude() }
}

// ─── SSH ─────────────────────────────────────────────────────────

/// Returns SSH connection multiplexing arguments.
///
/// macOS/Linux: ControlMaster=auto ControlPath=... ControlPersist=4h
/// Windows:     empty (use ssh-agent service instead)
pub fn ssh_mux_args(
    host: &str, port: u16, user: &str, as_master: bool
) -> Vec<String> {
    #[cfg(target_os = "macos")]
    { macos::ssh_mux_args(host, port, user, as_master) }
    #[cfg(target_os = "windows")]
    { windows::ssh_mux_args(host, port, user, as_master) }
    #[cfg(target_os = "linux")]
    { linux::ssh_mux_args(host, port, user, as_master) }
}

/// Check if an SSH multiplexed connection is alive.
pub fn ssh_mux_check(host: &str, port: u16, user: &str) -> bool {
    #[cfg(target_os = "macos")]
    { macos::ssh_mux_check(host, port, user) }
    #[cfg(target_os = "windows")]
    { false } // ControlMaster not supported
    #[cfg(target_os = "linux")]
    { linux::ssh_mux_check(host, port, user) }
}

// ─── Browser & OS Integration ────────────────────────────────────

/// Open a URL in the user's default browser.
///
/// macOS:   `open URL`
/// Windows: `start "" URL`  (via cmd /C)
/// Linux:   `xdg-open URL`
pub fn open_url(url: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    { macos::open_url(url) }
    #[cfg(target_os = "windows")]
    { windows::open_url(url) }
    #[cfg(target_os = "linux")]
    { linux::open_url(url) }
}

/// Open a terminal emulator with a command running in it.
/// Used as a fallback when in-app installation fails.
///
/// macOS:   Terminal.app via osascript
/// Windows: PowerShell window via `start powershell`
/// Linux:   xterm/gnome-terminal/konsole (best effort)
pub fn open_terminal_with_command(command: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    { macos::open_terminal_with_command(command) }
    #[cfg(target_os = "windows")]
    { windows::open_terminal_with_command(command) }
    #[cfg(target_os = "linux")]
    { linux::open_terminal_with_command(command) }
}

// ─── Capabilities (feature flags) ────────────────────────────────

/// Whether native dictation is available on this platform.
pub fn supports_dictation() -> bool {
    #[cfg(target_os = "macos")]
    { true }
    #[cfg(target_os = "windows")]
    { true }
    #[cfg(target_os = "linux")]
    { false }
}

/// Whether SSH ControlMaster multiplexing is supported.
pub fn supports_ssh_mux() -> bool {
    #[cfg(target_os = "windows")]
    { false }
    #[cfg(not(target_os = "windows"))]
    { true }
}

/// Whether the platform has a native package manager we can invoke.
pub fn native_package_manager() -> Option<&'static str> {
    #[cfg(target_os = "macos")]
    { Some("brew") }
    #[cfg(target_os = "windows")]
    { Some("winget") }
    #[cfg(target_os = "linux")]
    { Some("apt") } // Simplification; real code should detect distro
}

// ─── Menu ────────────────────────────────────────────────────────

/// Build the native app menu bar.
///
/// macOS:   includes Services, Hide, Hide Others, Show All
/// Windows: simpler File/Edit/View/Help
/// Linux:   same as Windows
pub fn build_menu(
    app: &tauri::App
) -> Result<tauri::menu::Menu<tauri::Wry>, Box<dyn std::error::Error>> {
    #[cfg(target_os = "macos")]
    { macos::build_menu(app) }
    #[cfg(target_os = "windows")]
    { windows::build_menu(app) }
    #[cfg(target_os = "linux")]
    { linux::build_menu(app) }
}

// ─── Credential Storage ──────────────────────────────────────────

/// Store a secret securely using the OS credential store.
///
/// macOS:   macOS Keychain via `keyring` crate
/// Windows: Windows Credential Manager via `keyring` crate
/// Linux:   Secret Service (GNOME Keyring / KWallet) via `keyring` crate
///
/// Note: the `keyring` crate handles all three platforms.
/// This wrapper exists for consistent error handling and namespace.
pub fn store_secret(service: &str, key: &str, value: &str) -> Result<(), String> {
    // In production, use: keyring::Entry::new(service, key)?.set_password(value)?;
    // For now, in-memory storage is used during development.
    let _ = (service, key, value);
    Ok(())
}

pub fn get_secret(service: &str, key: &str) -> Result<Option<String>, String> {
    let _ = (service, key);
    Ok(None)
}
```

### Example: `platform/macos.rs`

```rust
// src-tauri/src/platform/macos.rs

pub fn shell_exec(command: &str) -> std::process::Command {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
    let mut cmd = std::process::Command::new(&shell);
    cmd.arg("-l").arg("-c").arg(command);
    cmd
}

pub fn shell_exec_async(command: &str) -> tokio::process::Command {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
    let mut cmd = tokio::process::Command::new(&shell);
    cmd.arg("-l").arg("-c").arg(command);
    cmd
}

pub fn default_shell() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string())
}

pub fn check_tool(name: &str) -> Option<(String, String)> {
    let which = shell_exec(&format!("which {}", name)).output().ok()?;
    if !which.status.success() { return None; }
    let path = String::from_utf8_lossy(&which.stdout).trim().to_string();
    let ver_out = shell_exec(&format!("{} --version", name)).output().ok()?;
    let version = String::from_utf8_lossy(&ver_out.stdout).trim().to_string();
    Some((path, version))
}

pub fn extra_tool_paths() -> Vec<std::path::PathBuf> {
    let home = dirs::home_dir().unwrap_or_default();
    vec![
        home.join(".operon/node/bin"),
        std::path::PathBuf::from("/opt/homebrew/bin"),
        std::path::PathBuf::from("/usr/local/bin"),
        home.join(".claude/local/bin"),
        home.join(".npm-global/bin"),
    ]
}

pub fn open_url(url: &str) -> Result<(), String> {
    std::process::Command::new("open")
        .arg(url)
        .spawn()
        .map_err(|e| format!("Failed to open URL: {}", e))?;
    Ok(())
}

pub fn open_terminal_with_command(command: &str) -> Result<(), String> {
    let script = format!(
        r#"tell application "Terminal"
            activate
            do script "{}"
        end tell"#,
        command.replace('\\', "\\\\").replace('"', "\\\"")
    );
    std::process::Command::new("osascript")
        .arg("-e").arg(&script)
        .output()
        .map_err(|e| format!("osascript failed: {}", e))?;
    Ok(())
}

pub fn ssh_mux_args(host: &str, port: u16, user: &str, _as_master: bool) -> Vec<String> {
    let sock = super::ssh_sockets_dir()
        .join(format!("ctrl_{}_{}_{}", host, port, user));
    vec![
        "-o".into(), format!("ControlMaster=auto"),
        "-o".into(), format!("ControlPath={}", sock.display()),
        "-o".into(), "ControlPersist=4h".into(),
    ]
}

pub fn ssh_mux_check(host: &str, port: u16, user: &str) -> bool {
    let sock = super::ssh_sockets_dir()
        .join(format!("ctrl_{}_{}_{}", host, port, user));
    let check = format!(
        "ssh -o ControlPath={} -O check {}@{} -p {} 2>/dev/null",
        sock.display(), user, host, port
    );
    shell_exec(&check).output().map(|o| o.status.success()).unwrap_or(false)
}

// ... install_node(), install_claude(), check_dependencies(), build_menu() ...
```

### Example: `platform/windows.rs`

```rust
// src-tauri/src/platform/windows.rs

pub fn shell_exec(command: &str) -> std::process::Command {
    let mut cmd = std::process::Command::new("cmd.exe");
    cmd.arg("/C").arg(command);
    cmd
}

pub fn shell_exec_async(command: &str) -> tokio::process::Command {
    let mut cmd = tokio::process::Command::new("cmd.exe");
    cmd.arg("/C").arg(command);
    cmd
}

pub fn default_shell() -> String {
    // Prefer PowerShell if available, fall back to cmd.exe
    std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string())
}

pub fn check_tool(name: &str) -> Option<(String, String)> {
    let where_out = std::process::Command::new("where.exe")
        .arg(name)
        .output().ok()?;
    if !where_out.status.success() { return None; }
    let path = String::from_utf8_lossy(&where_out.stdout)
        .lines().next()?.trim().to_string();
    let ver_out = std::process::Command::new(&path)
        .arg("--version").output().ok()?;
    let version = String::from_utf8_lossy(&ver_out.stdout).trim().to_string();
    Some((path, version))
}

pub fn extra_tool_paths() -> Vec<std::path::PathBuf> {
    let home = dirs::home_dir().unwrap_or_default();
    let appdata = std::env::var("APPDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| home.join("AppData").join("Roaming"));
    vec![
        home.join(".operon\\node"),
        appdata.join("npm"),
        std::path::PathBuf::from(r"C:\Program Files\nodejs"),
        home.join(".claude\\local\\bin"),
    ]
}

pub fn open_url(url: &str) -> Result<(), String> {
    // Use `start` via cmd.exe. The empty "" is the window title argument.
    std::process::Command::new("cmd.exe")
        .args(["/C", "start", "", url])
        .spawn()
        .map_err(|e| format!("Failed to open URL: {}", e))?;
    Ok(())
}

pub fn open_terminal_with_command(command: &str) -> Result<(), String> {
    std::process::Command::new("powershell.exe")
        .args(["-NoExit", "-Command", command])
        .spawn()
        .map_err(|e| format!("Failed to open PowerShell: {}", e))?;
    Ok(())
}

pub fn ssh_mux_args(
    _host: &str, _port: u16, _user: &str, _as_master: bool
) -> Vec<String> {
    // ControlMaster uses Unix domain sockets, which are unreliable on Windows.
    // Instead, Windows relies on the ssh-agent service for key caching.
    vec![]
}

pub fn install_node() -> Result<(), String> {
    // Strategy 1: winget (built into Windows 11)
    let winget = std::process::Command::new("winget")
        .args(["install", "--id", "OpenJS.NodeJS.LTS", "--accept-source-agreements", "--accept-package-agreements"])
        .output();

    if let Ok(o) = winget {
        if o.status.success() { return Ok(()); }
    }

    // Strategy 2: Download .zip to ~/.operon/node/
    let arch = if cfg!(target_arch = "x86_64") { "x64" } else { "arm64" };
    let url = format!(
        "https://nodejs.org/dist/v22.14.0/node-v22.14.0-win-{}.zip", arch
    );
    // Download with reqwest, extract with zip crate to data_dir()/node/
    // (implementation uses existing reqwest + zip dependencies)
    Err(format!("Automatic Node.js install failed. Please install Node.js from https://nodejs.org/ and restart Operon."))
}

pub fn install_claude() -> Result<(), String> {
    // npm is the primary method on Windows
    let result = shell_exec("npm install -g @anthropic-ai/claude-code").output();
    match result {
        Ok(o) if o.status.success() => Ok(()),
        _ => Err("npm install failed. Run: npm install -g @anthropic-ai/claude-code".to_string()),
    }
}

pub fn check_dependencies() -> super::super::commands::claude::DependencyStatus {
    super::super::commands::claude::DependencyStatus {
        xcode_cli: true, // Not applicable on Windows — always pass
        node: check_tool("node").is_some(),
        node_version: check_tool("node").map(|(_, v)| v),
        npm: check_tool("npm").is_some(),
        npm_version: check_tool("npm").map(|(_, v)| v),
        claude_code: check_tool("claude").is_some(),
        claude_version: check_tool("claude").map(|(_, v)| v),
    }
}

// pub fn build_menu(app: &tauri::App) -> ... { ... }
```

### Example: `platform/linux.rs`

```rust
// src-tauri/src/platform/linux.rs
// Linux is very close to macOS — most functions are identical
// except: bash instead of zsh, xdg-open instead of open,
// apt instead of brew, no Xcode, no osascript

pub fn shell_exec(command: &str) -> std::process::Command {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
    let mut cmd = std::process::Command::new(&shell);
    cmd.arg("-l").arg("-c").arg(command);
    cmd
}

pub fn default_shell() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string())
}

pub fn open_url(url: &str) -> Result<(), String> {
    std::process::Command::new("xdg-open")
        .arg(url)
        .spawn()
        .map_err(|e| format!("Failed to open URL: {}", e))?;
    Ok(())
}

pub fn open_terminal_with_command(command: &str) -> Result<(), String> {
    // Try common terminal emulators in order of preference
    for term in &["gnome-terminal", "konsole", "xfce4-terminal", "xterm"] {
        let result = match *term {
            "gnome-terminal" => std::process::Command::new(term)
                .args(["--", "bash", "-c", &format!("{}; exec bash", command)])
                .spawn(),
            "konsole" => std::process::Command::new(term)
                .args(["-e", "bash", "-c", &format!("{}; exec bash", command)])
                .spawn(),
            _ => std::process::Command::new(term)
                .args(["-e", &format!("bash -c '{}; exec bash'", command)])
                .spawn(),
        };
        if result.is_ok() { return Ok(()); }
    }
    Err("No terminal emulator found".to_string())
}

pub fn install_node() -> Result<(), String> {
    // Strategy 1: apt (if sudo is available)
    let has_sudo = std::process::Command::new("sudo")
        .args(["-n", "true"]) // Non-interactive check
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if has_sudo {
        let result = shell_exec("sudo apt-get install -y nodejs npm").output();
        if let Ok(o) = result {
            if o.status.success() { return Ok(()); }
        }
    }

    // Strategy 2: Tarball to ~/.operon/node/ (no sudo needed)
    let arch = if cfg!(target_arch = "x86_64") { "x64" }
              else if cfg!(target_arch = "aarch64") { "arm64" }
              else { "x64" };
    let url = format!(
        "https://nodejs.org/dist/v22.14.0/node-v22.14.0-linux-{}.tar.gz", arch
    );
    // Download + extract with tar (same pattern as macOS tarball)
    Err(format!("Please install Node.js: https://nodejs.org/"))
}

// ssh_mux_args — identical to macOS (ControlMaster works on Linux)
// check_dependencies — same as macOS minus xcode_cli (always true)
```

---

## 5. Branching Strategy & Workflow

### Branches

```
main ← Stable releases. Tagged. All 3 platforms pass CI.
  │
  └── dev ← Integration branch. All 3 platforms must pass before merge.
       │
       ├── feature/notebook-viewer    ← New feature work
       ├── fix/ssh-timeout-handling   ← Bug fixes
       ├── platform/windows-initial   ← Platform porting work (rare, only during initial port)
       └── chore/update-dependencies  ← Maintenance
```

### Flow

```
1. Create branch from `dev`
2. Develop on macOS (your laptop)
3. Push to GitHub
4. CI runs on macOS + Windows + Linux
5. Fix any platform failures (CI tells you exactly which)
6. Open PR to `dev`
7. Required: all 3 platform checks green + 1 code review
8. Merge to `dev`
9. Nightly builds from `dev` for QA testing
10. When stable: PR from `dev` → `main`
11. Tag release (e.g. v0.5.0)
12. CI builds installers for all 3 platforms and uploads to GitHub Releases
```

### Branch Protection Rules (GitHub)

**On `dev`:**
- Require status checks: `build (macos-latest)`, `build (windows-latest)`, `build (ubuntu-22.04)`
- Require 1 PR review
- Require branches to be up-to-date before merging
- No direct pushes

**On `main`:**
- Same as `dev` plus:
- Require 2 PR reviews
- Require linear history (squash or rebase merges)

---

## 6. CI/CD Pipeline

### `.github/workflows/ci.yml`

```yaml
name: CI

on:
  push:
    branches: [dev, main]
  pull_request:
    branches: [dev, main]

# Cancel in-progress runs for the same branch
concurrency:
  group: ci-${{ github.ref }}
  cancel-in-progress: true

env:
  RUST_BACKTRACE: 1
  CARGO_TERM_COLOR: always

jobs:
  # ── Lint (runs once, fast) ────────────────────────────────────
  lint:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy, rustfmt
      - uses: actions/setup-node@v4
        with:
          node-version: 22
          cache: npm
      - name: Install Linux deps
        run: |
          sudo apt-get update
          sudo apt-get install -y libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf libssl-dev
      - run: npm ci
      - name: Rust formatting
        run: cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
      - name: Rust clippy
        run: cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
      - name: TypeScript type check
        run: npx tsc --noEmit

  # ── Build Matrix (all 3 platforms) ────────────────────────────
  build:
    needs: lint
    strategy:
      fail-fast: false    # IMPORTANT: don't cancel other platforms if one fails
      matrix:
        include:
          - os: macos-latest
            rust-target: aarch64-apple-darwin
            tauri-bundle: dmg
            label: macOS ARM

          - os: macos-13
            rust-target: x86_64-apple-darwin
            tauri-bundle: dmg
            label: macOS Intel

          - os: windows-latest
            rust-target: x86_64-pc-windows-msvc
            tauri-bundle: msi,nsis
            label: Windows x64

          - os: ubuntu-22.04
            rust-target: x86_64-unknown-linux-gnu
            tauri-bundle: deb,appimage
            label: Linux x64

    name: Build (${{ matrix.label }})
    runs-on: ${{ matrix.os }}

    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.rust-target }}

      - name: Rust cache
        uses: Swatinem/rust-cache@v2
        with:
          workspaces: src-tauri
          key: ${{ matrix.rust-target }}

      - name: Install Node.js
        uses: actions/setup-node@v4
        with:
          node-version: 22
          cache: npm

      # Platform-specific system dependencies
      - name: Install Linux dependencies
        if: runner.os == 'Linux'
        run: |
          sudo apt-get update
          sudo apt-get install -y \
            libwebkit2gtk-4.1-dev \
            libappindicator3-dev \
            librsvg2-dev \
            patchelf \
            libssl-dev \
            libgtk-3-dev

      - name: Install frontend dependencies
        run: npm ci

      - name: Rust check
        run: cargo check --manifest-path src-tauri/Cargo.toml --target ${{ matrix.rust-target }}

      - name: Rust tests
        run: cargo test --manifest-path src-tauri/Cargo.toml --target ${{ matrix.rust-target }}

      - name: Build Tauri app
        uses: tauri-apps/tauri-action@v0
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        with:
          tauriScript: npx tauri
          args: --target ${{ matrix.rust-target }} --bundles ${{ matrix.tauri-bundle }}

      - name: Upload artifacts
        uses: actions/upload-artifact@v4
        with:
          name: operon-${{ matrix.rust-target }}
          path: |
            src-tauri/target/${{ matrix.rust-target }}/release/bundle/**/*.dmg
            src-tauri/target/${{ matrix.rust-target }}/release/bundle/**/*.msi
            src-tauri/target/${{ matrix.rust-target }}/release/bundle/**/*.exe
            src-tauri/target/${{ matrix.rust-target }}/release/bundle/**/*.deb
            src-tauri/target/${{ matrix.rust-target }}/release/bundle/**/*.AppImage
          retention-days: 14
```

### `.github/workflows/release.yml`

```yaml
name: Release

on:
  push:
    tags: ['v*']

permissions:
  contents: write

jobs:
  release:
    strategy:
      fail-fast: false
      matrix:
        include:
          - os: macos-latest
            rust-target: aarch64-apple-darwin
          - os: macos-13
            rust-target: x86_64-apple-darwin
          - os: windows-latest
            rust-target: x86_64-pc-windows-msvc
          - os: ubuntu-22.04
            rust-target: x86_64-unknown-linux-gnu

    runs-on: ${{ matrix.os }}

    steps:
      - uses: actions/checkout@v4

      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.rust-target }}

      - uses: actions/setup-node@v4
        with:
          node-version: 22
          cache: npm

      - name: Install Linux deps
        if: runner.os == 'Linux'
        run: |
          sudo apt-get update
          sudo apt-get install -y libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf libssl-dev libgtk-3-dev

      - run: npm ci

      - name: Build and publish release
        uses: tauri-apps/tauri-action@v0
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
          # macOS code signing (optional)
          # APPLE_CERTIFICATE: ${{ secrets.APPLE_CERTIFICATE }}
          # APPLE_CERTIFICATE_PASSWORD: ${{ secrets.APPLE_CERTIFICATE_PASSWORD }}
          # APPLE_SIGNING_IDENTITY: ${{ secrets.APPLE_SIGNING_IDENTITY }}
          # Windows code signing (optional)
          # TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_PRIVATE_KEY }}
        with:
          tagName: ${{ github.ref_name }}
          releaseName: 'Operon ${{ github.ref_name }}'
          releaseBody: 'See CHANGELOG.md for details.'
          releaseDraft: true
          prerelease: false
          tauriScript: npx tauri
          args: --target ${{ matrix.rust-target }}
```

---

## 7. How to Add a New Feature

This is the standard process every developer follows.

### Example: Adding a Jupyter Notebook Viewer

**Step 1: Branch**
```bash
git checkout dev && git pull
git checkout -b feature/notebook-viewer
```

**Step 2: Frontend (platform-agnostic, ~90% of effort)**

```
src/components/editor/NotebookViewer.tsx   ← New React component
src/types/notebook.ts                      ← .ipynb type definitions
```

No platform code needed. This works identically everywhere.

**Step 3: Backend (if needed)**

If you need to run a Jupyter kernel:

```rust
// commands/notebook.rs — NO platform code here

#[tauri::command]
pub async fn start_jupyter_kernel(notebook_path: String) -> Result<String, String> {
    // ✅ CORRECT: uses platform abstraction
    let python = platform::check_tool("python3")
        .or_else(|| platform::check_tool("python"))
        .ok_or("Python not found. Install Python to use notebooks.")?;

    let output = platform::shell_exec(
        &format!("{} -m jupyter_core --version", python.0)
    ).output().map_err(|e| e.to_string())?;

    if !output.status.success() {
        return Err("Jupyter is not installed. Run: pip install jupyter".to_string());
    }

    // ... kernel startup logic (same on all platforms)
    Ok("kernel_id_here".to_string())
}
```

What this looks like **wrong** (and what code review should catch):

```rust
// ❌ WRONG: Direct platform references in a command file
let python = if cfg!(target_os = "windows") {
    "python.exe"
} else {
    "/usr/bin/python3"
};

// ❌ WRONG: Shell-specific syntax in a command file
let output = std::process::Command::new("/bin/bash")
    .arg("-c")
    .arg("which python3")
    .output()?;
```

**Step 4: Push and CI validates**
```bash
git push -u origin feature/notebook-viewer
```

If Windows CI fails (e.g., a path issue), fix it locally and push again. The PR stays blocked until all three platforms pass.

**Step 5: PR → `dev` → eventually `main`**

---

## 8. How to Add a Platform-Specific Feature

Some features have no cross-platform equivalent. Handle them with **capability flags + graceful degradation**.

### Example: Dictation

**Step 1: Add capability flag to `platform/mod.rs`**

```rust
pub fn supports_dictation() -> bool {
    #[cfg(target_os = "macos")] { true }
    #[cfg(target_os = "windows")] { true }  // via Windows SAPI
    #[cfg(target_os = "linux")] { false }   // no native API
}
```

**Step 2: Implement per-platform in `platform/*.rs`**

```rust
// platform/macos.rs
pub fn start_dictation(app: &tauri::AppHandle) -> Result<(), String> {
    // SFSpeechRecognizer via Swift script (existing code)
}

// platform/windows.rs
pub fn start_dictation(app: &tauri::AppHandle) -> Result<(), String> {
    // Windows.Media.SpeechRecognition via PowerShell
    // OR: Web Speech API in the webview (simpler, cross-platform)
}

// platform/linux.rs
pub fn start_dictation(_app: &tauri::AppHandle) -> Result<(), String> {
    Err("Dictation is not available on Linux. Consider using a browser extension.".to_string())
}
```

**Step 3: Command file wraps it cleanly**

```rust
// commands/settings.rs
#[tauri::command]
pub async fn start_dictation(app: tauri::AppHandle) -> Result<(), String> {
    if !crate::platform::supports_dictation() {
        return Err("Dictation is not supported on this platform.".to_string());
    }
    crate::platform::start_dictation(&app)
}
```

**Step 4: Frontend checks capability**

```typescript
// ChatPanel.tsx
const [canDictate, setCanDictate] = useState(false);

useEffect(() => {
  invoke<boolean>('supports_dictation').then(setCanDictate);
}, []);

// Render button only if supported
{canDictate && (
  <button onClick={startDictation}>
    <Mic className="w-4 h-4" />
  </button>
)}
```

### Features requiring this pattern

| Feature | macOS | Windows | Linux |
|---------|-------|---------|-------|
| Dictation | SFSpeechRecognizer (Swift) | SAPI / Web Speech API | Stub |
| Keychain | macOS Keychain | Credential Manager | Secret Service |
| SSH multiplexing | ControlMaster | ssh-agent only | ControlMaster |
| Native installer | Homebrew / curl | winget / npm | apt / curl |
| Menu bar | macOS conventions | Windows conventions | freedesktop |
| Xcode CLI tools | Required | N/A (always pass) | N/A |
| Traffic-light spacer | 78px left padding | None | None |

---

## 9. Frontend Cross-Platform Rules

### 9.1 Keyboard Shortcuts

The existing `useKeyboardShortcuts.ts` already handles this well — it checks `metaKey || ctrlKey`. Make sure all new shortcuts follow this pattern:

```typescript
// ✅ CORRECT — works on both Cmd (macOS) and Ctrl (Windows/Linux)
const metaMatch = shortcut.meta ? (e.metaKey || e.ctrlKey) : !(e.metaKey || e.ctrlKey);
```

For display purposes, show the right modifier:

```typescript
const isMac = navigator.platform.toUpperCase().includes('MAC');
const modLabel = isMac ? '⌘' : 'Ctrl';
// "⌘+S" on macOS, "Ctrl+S" on Windows/Linux
```

### 9.2 File Paths in UI

Never display raw backend paths. Windows paths have backslashes, which look foreign to most users:

```typescript
// utils/path.ts
export function displayPath(rawPath: string): string {
  return rawPath.replace(/\\/g, '/');
}

export function basename(path: string): string {
  return path.split(/[/\\]/).pop() || path;
}
```

### 9.3 Traffic-Light Spacer

The 78px spacer in `TopBar.tsx` is macOS-only (for the close/minimize/maximize buttons). On Windows and Linux, the title bar has its own native buttons:

```tsx
// TopBar.tsx
import { platform } from '@tauri-apps/plugin-os';

const os = platform(); // 'darwin', 'win32', 'linux'

return (
  <div className="h-10 flex items-center bg-zinc-900 border-b border-zinc-800">
    {/* macOS traffic light spacer — only on macOS */}
    {os === 'darwin' && <div className="w-[78px] shrink-0" />}
    {/* ... rest of TopBar */}
  </div>
);
```

### 9.4 Window Decorations

On macOS, you might use a transparent/custom title bar. On Windows and Linux, keep native decorations for a familiar feel:

```json
// tauri.conf.json
"windows": [{
  "decorations": true,
  "transparent": false
}]
```

If you want a custom title bar on macOS only, handle it in `lib.rs` via `platform::configure_window()`.

### 9.5 Terminal TERM Variable

Already set correctly (`xterm-256color`). This works on all platforms. No change needed.

### 9.6 Hidden Files

macOS and Linux use a leading `.` to denote hidden files. Windows uses the Hidden file attribute:

```rust
// This is already handled in files.rs for the leading dot convention.
// For Windows, you may want to also check the Hidden attribute:

#[cfg(target_os = "windows")]
fn is_hidden(path: &std::path::Path) -> bool {
    use std::os::windows::fs::MetadataExt;
    if let Ok(meta) = std::fs::metadata(path) {
        // FILE_ATTRIBUTE_HIDDEN = 0x2
        meta.file_attributes() & 0x2 != 0
    } else {
        false
    }
}

#[cfg(not(target_os = "windows"))]
fn is_hidden(path: &std::path::Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map_or(false, |n| n.starts_with('.'))
}
```

---

## 10. Backend Coding Standards

### 10.1 Never Construct Shell Commands Directly

```rust
// ❌ WRONG (in any command file)
let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
std::process::Command::new(&shell).arg("-l").arg("-c").arg(cmd);

// ❌ WRONG
std::process::Command::new("osascript").arg("-e").arg(script);

// ❌ WRONG
std::process::Command::new("/opt/homebrew/bin/npm").arg("install");

// ✅ CORRECT
crate::platform::shell_exec(cmd);
crate::platform::open_terminal_with_command(script);
crate::platform::check_tool("npm");
```

### 10.2 Never Hardcode Paths

```rust
// ❌ WRONG
let dir = home.join(".operon").join("sessions");
let npm = "/opt/homebrew/bin/npm";
let script = "/tmp/operon_install.sh";

// ✅ CORRECT
let dir = crate::platform::sessions_dir()?;
let npm = crate::platform::check_tool("npm").map(|(path, _)| path);
let script = crate::platform::temp_dir().join("operon_install.sh");
```

### 10.3 Use PathBuf, Not String Concatenation

```rust
// ❌ WRONG
let path = format!("{}/sessions/{}.json", home, session_id);

// ✅ CORRECT
let path = crate::platform::data_dir()
    .join("sessions")
    .join(format!("{}.json", session_id));
```

### 10.4 Conditional Compilation Belongs in `platform/` Only

```rust
// ❌ WRONG (in commands/claude.rs)
#[cfg(target_os = "macos")]
fn install_homebrew() { ... }

#[cfg(target_os = "windows")]
fn install_via_winget() { ... }

// ✅ CORRECT (in platform/macos.rs and platform/windows.rs)
// command files just call: crate::platform::install_node()
```

### 10.5 Error Messages Must Not Reference Platform Internals

```rust
// ❌ WRONG
Err("Run `brew install node` in Terminal.app".to_string())

// ✅ CORRECT
Err("Node.js is not installed. Please install Node.js from https://nodejs.org/ and restart Operon.".to_string())
```

### 10.6 Remote Commands Are Already Cross-Platform

The SSH remote execution code (`ssh_exec`, `check_remote_claude`, `install_remote_claude`) sends bash scripts to a remote Linux server. These do not need platform adaptation — the scripts run on the remote server, not on the local machine. The only part that needs platform awareness is the local SSH invocation:

```rust
// ssh.rs — ssh_exec() should use platform::shell_exec()
// The remote_cmd string is bash and stays as-is (it runs on Linux HPC nodes)
pub fn ssh_exec(profile: &SSHProfile, remote_cmd: &str) -> Result<String, String> {
    let ssh_args = format!(
        "ssh -o BatchMode=yes {}@{} -p {} -- {}",
        profile.user, profile.host, profile.port, shell_escape(remote_cmd)
    );
    let output = crate::platform::shell_exec(&ssh_args).output()?;
    // ...
}
```

---

## 11. Dependency Management

### Rust Dependencies (Cargo.toml)

All current dependencies are cross-platform:

| Crate | Platform support | Notes |
|-------|-----------------|-------|
| `tauri 2` | macOS, Windows, Linux | Full support |
| `portable-pty 0.8` | macOS (PTY), Windows (ConPTY), Linux (PTY) | Works out of the box |
| `tokio` | All | Cross-platform async |
| `serde`, `serde_json` | All | Pure Rust |
| `dirs 5` | All | Returns correct OS-specific directories |
| `reqwest` (rustls-tls) | All | rustls avoids OpenSSL linking issues on Windows |
| `openssl-sys` (vendored) | All | Compiles OpenSSL from source via MSVC on Windows |
| `ssh2 0.9` | All | libssh2 bindings, works with vendored OpenSSL |
| `uuid`, `base64`, `zip` | All | Pure Rust |
| `anyhow`, `thiserror` | All | Pure Rust |

**Recommended addition for credential storage:**
```toml
keyring = "3"  # Cross-platform: macOS Keychain, Windows Credential Manager, Linux Secret Service
```

### Frontend Dependencies (package.json)

All frontend dependencies are platform-agnostic (they run in a WebView). No changes needed.

### Platform-Conditional Dependencies

If you ever need a platform-specific Rust crate:

```toml
[target.'cfg(target_os = "macos")'.dependencies]
cocoa = "0.26"  # Example: native macOS API access

[target.'cfg(target_os = "windows")'.dependencies]
windows = "0.58"  # Example: Windows API access

[target.'cfg(target_os = "linux")'.dependencies]
# linux-specific crate if needed
```

---

## 12. Setup Wizard Per Platform

The setup wizard must be platform-aware. The steps differ:

### macOS Setup Flow (existing)

```
Step 1: Xcode Command Line Tools  →  xcode-select --install
Step 2: Homebrew (optional)        →  Silent install via osascript
Step 3: Node.js                    →  brew install node / tarball fallback
Step 4: GitHub CLI                 →  brew install gh
Step 5: Claude Code                →  curl installer / npm fallback
Step 6: API Key / OAuth            →  Terminal.app + claude login
```

### Windows Setup Flow

```
Step 1: (skip Xcode — N/A)
Step 2: (skip Homebrew — N/A)
Step 3: Node.js                    →  winget install / .zip fallback
Step 4: GitHub CLI                 →  winget install GitHub.cli
Step 5: Claude Code                →  npm install -g / PowerShell fallback
Step 6: API Key / OAuth            →  PowerShell + claude login
```

### Linux Setup Flow

```
Step 1: System dependencies        →  apt install libwebkit2gtk-4.1-dev ... (inform user)
Step 2: (skip Homebrew — N/A)
Step 3: Node.js                    →  apt install nodejs / tarball fallback
Step 4: GitHub CLI                 →  apt install gh / brew
Step 5: Claude Code                →  curl installer / npm fallback
Step 6: API Key / OAuth            →  gnome-terminal + claude login
```

### Implementation

The frontend `SetupWizard.tsx` should call a unified Tauri command that returns platform-appropriate steps:

```rust
#[tauri::command]
pub async fn get_setup_steps() -> Result<Vec<SetupStep>, String> {
    Ok(crate::platform::setup_steps())
}
```

Each platform module returns its own list of steps with labels, descriptions, and install functions.

---

## 13. Testing Strategy

### 13.1 Rust Unit Tests

Every function in `platform/` must have a unit test:

```rust
// platform/mod.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_exec_runs_echo() {
        let output = shell_exec("echo hello").output().unwrap();
        assert!(output.status.success());
        assert!(String::from_utf8_lossy(&output.stdout).contains("hello"));
    }

    #[test]
    fn default_shell_is_not_empty() {
        let shell = default_shell();
        assert!(!shell.is_empty());
    }

    #[test]
    fn data_dir_is_writable() {
        let dir = data_dir();
        std::fs::create_dir_all(&dir).unwrap();
        let test_file = dir.join("_test_write");
        std::fs::write(&test_file, "test").unwrap();
        std::fs::remove_file(&test_file).unwrap();
    }

    #[test]
    fn home_dir_exists() {
        let home = home_dir().expect("home_dir should return Some");
        assert!(home.exists());
    }

    #[test]
    fn extra_tool_paths_are_absolute() {
        for path in extra_tool_paths() {
            assert!(path.is_absolute(), "Path should be absolute: {:?}", path);
        }
    }
}
```

These tests run on each CI runner, validating the actual OS behavior.

### 13.2 Integration Tests

Test Tauri commands end-to-end:

```rust
// tests/terminal_integration.rs
#[tokio::test]
async fn terminal_spawn_and_write() {
    // Spawn a terminal, write "echo test\n", read output, verify
}

#[tokio::test]
async fn file_crud_operations() {
    // Create, read, write, delete a file via Tauri commands
}
```

### 13.3 Frontend Tests

Use Vitest for React component tests:

```typescript
// __tests__/keyboard-shortcuts.test.ts
test('keyboard shortcuts use correct modifier per platform', () => {
  // Mock navigator.platform
  // Verify Cmd on macOS, Ctrl on Windows/Linux
});
```

### 13.4 E2E Tests (future)

Use Tauri's WebDriver support + Playwright:

```yaml
# In CI, after build step
- name: E2E tests
  if: matrix.os == 'ubuntu-22.04'  # Run on Linux where display can be virtualized
  run: |
    Xvfb :99 &
    export DISPLAY=:99
    npx playwright test
```

---

## 14. Release & Distribution

### Tauri Config for All Platforms

```json
// tauri.conf.json
{
  "bundle": {
    "active": true,
    "icon": [
      "icons/32x32.png",
      "icons/128x128.png",
      "icons/128x128@2x.png",
      "icons/icon.icns",
      "icons/icon.ico",
      "icons/icon.png"
    ],
    "targets": "all",
    "macOS": {
      "entitlements": null,
      "infoPlist": "Info.plist",
      "minimumSystemVersion": "10.15"
    },
    "windows": {
      "certificateThumbprint": null,
      "digestAlgorithm": "sha256",
      "timestampUrl": ""
    },
    "linux": {
      "deb": {
        "depends": [
          "libwebkit2gtk-4.1-0",
          "libgtk-3-0"
        ]
      },
      "appimage": {
        "bundleMediaFramework": true
      }
    }
  }
}
```

### Generate Windows Icon

```bash
npx @tauri-apps/cli icon src-tauri/icons/icon.png
# This generates icon.ico + all required PNG sizes
```

### Build Commands

```json
// package.json
{
  "scripts": {
    "dev": "vite",
    "build": "tsc && vite build",
    "tauri": "tauri",

    "build:mac:arm": "tauri build --target aarch64-apple-darwin",
    "build:mac:intel": "tauri build --target x86_64-apple-darwin",
    "build:mac:universal": "bash build-universal.sh",

    "build:win": "tauri build --target x86_64-pc-windows-msvc",
    "build:win:msi": "tauri build --target x86_64-pc-windows-msvc --bundles msi",
    "build:win:nsis": "tauri build --target x86_64-pc-windows-msvc --bundles nsis",

    "build:linux": "tauri build --target x86_64-unknown-linux-gnu",
    "build:linux:deb": "tauri build --target x86_64-unknown-linux-gnu --bundles deb",
    "build:linux:appimage": "tauri build --target x86_64-unknown-linux-gnu --bundles appimage"
  }
}
```

### Distribution Formats

| Platform | Format | How users install |
|----------|--------|-------------------|
| macOS ARM | `.dmg` | Drag to Applications |
| macOS Intel | `.dmg` | Drag to Applications |
| Windows | `.msi` | Double-click, standard installer |
| Windows | `.exe` (NSIS) | Double-click, modern installer with options |
| Linux (Debian/Ubuntu) | `.deb` | `sudo dpkg -i operon.deb` |
| Linux (any) | `.AppImage` | `chmod +x Operon.AppImage && ./Operon.AppImage` |

### Code Signing

| Platform | Mechanism | Secrets needed |
|----------|-----------|---------------|
| macOS | Apple Developer certificate | `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY` |
| macOS (notarization) | Apple notarization | `APPLE_API_KEY`, `APPLE_API_ISSUER` |
| Windows | EV Code Signing Certificate | `TAURI_SIGNING_PRIVATE_KEY` or `WINDOWS_CERTIFICATE` |
| Linux | Not required | — |

---

## 15. Platform Reference Tables

### Shell Equivalents

| Concept | macOS | Windows | Linux |
|---------|-------|---------|-------|
| Default shell | `/bin/zsh` | `cmd.exe` | `/bin/bash` |
| Shell env var | `$SHELL` | `%COMSPEC%` | `$SHELL` |
| Run command | `zsh -l -c "cmd"` | `cmd /C "cmd"` | `bash -l -c "cmd"` |
| Home dir env | `$HOME` | `%USERPROFILE%` | `$HOME` |
| Temp dir | `/tmp/` or `$TMPDIR` | `%TEMP%` | `/tmp/` |
| Find binary | `which name` | `where.exe name` | `which name` |
| Open URL | `open URL` | `start "" URL` | `xdg-open URL` |
| Open terminal | `osascript` (AppleScript) | `start powershell` | `gnome-terminal --` |
| File permissions | `chmod +x` | Not applicable | `chmod +x` |
| Package manager | `brew` | `winget` | `apt` / `dnf` / `pacman` |
| PATH separator | `:` | `;` | `:` |
| Path separator | `/` | `\` (but `/` often works) | `/` |

### Directory Mapping

| Purpose | macOS | Windows | Linux |
|---------|-------|---------|-------|
| App data | `~/Library/Application Support/operon/` | `%LOCALAPPDATA%\operon\` | `~/.local/share/operon/` |
| App config | `~/Library/Application Support/operon/` | `%APPDATA%\operon\` | `~/.config/operon/` |
| Sessions | `{data}/sessions/` | `{data}\sessions\` | `{data}/sessions/` |
| SSH profiles | `{data}/ssh_profiles.json` | `{data}\ssh_profiles.json` | `{data}/ssh_profiles.json` |
| SSH sockets | `{data}/sockets/` | N/A (no ControlMaster) | `{data}/sockets/` |
| Temp files | `$TMPDIR` | `%TEMP%` | `/tmp/` |
| npm global | `~/.npm-global/bin/` | `%APPDATA%\npm\` | `~/.npm-global/bin/` |
| Claude install | `~/.claude/local/bin/` | `%APPDATA%\claude\` | `~/.claude/local/bin/` |

### Feature Support Matrix

| Feature | macOS | Windows | Linux |
|---------|-------|---------|-------|
| Terminal (PTY) | ✅ native PTY | ✅ ConPTY | ✅ native PTY |
| File browser | ✅ | ✅ | ✅ |
| Monaco editor | ✅ | ✅ | ✅ |
| Claude chat (NDJSON) | ✅ | ✅ | ✅ |
| SSH terminal | ✅ OpenSSH | ✅ Windows OpenSSH | ✅ OpenSSH |
| SSH ControlMaster | ✅ Unix sockets | ❌ Use ssh-agent | ✅ Unix sockets |
| Dictation | ✅ SFSpeechRecognizer | ⚠️ SAPI (partial) | ❌ Stub |
| Keychain | ✅ macOS Keychain | ✅ Credential Manager | ✅ Secret Service |
| HPC Terminal Mode | ✅ | ✅ | ✅ |
| Session resume | ✅ | ✅ | ✅ |
| Extensions (Open VSX) | ✅ | ✅ | ✅ |
| Docker/Singularity | ✅ | ✅ | ✅ |

---

## 16. Migration Checklist (Current Codebase)

Track progress as you port each file. Order matters — do the platform layer first.

### Phase 0: Infrastructure
- [ ] Create `src-tauri/src/platform/mod.rs`
- [ ] Create `src-tauri/src/platform/macos.rs` (extract existing macOS code)
- [ ] Create `src-tauri/src/platform/windows.rs` (stubs that compile)
- [ ] Create `src-tauri/src/platform/linux.rs` (copy macOS, change zsh→bash)
- [ ] Create `src-tauri/src/platform/common.rs` (shared utilities)
- [ ] Add `mod platform;` to `main.rs`
- [ ] Update `tauri.conf.json` with `"targets": "all"` and Windows icon
- [ ] Add `build:win` and `build:linux` scripts to `package.json`
- [ ] Set up `.github/workflows/ci.yml`
- [ ] Set up `.github/workflows/release.yml`
- [ ] Verify: `cargo check --target x86_64-pc-windows-msvc` passes (cross-check on macOS)

### Phase 1: Shell Execution (highest impact)
- [ ] Replace `login_shell_cmd()` in `commands/claude.rs` → `platform::shell_exec()`
- [ ] Replace `login_shell_cmd()` in `lib.rs` (git commands) → `platform::shell_exec()`
- [ ] Replace shell detection in `commands/terminal.rs` → `platform::default_shell()`
- [ ] Replace shell detection in `commands/ssh.rs` (×3) → `platform::shell_exec()`
- [ ] Replace shell detection in `commands/git.rs` → `platform::shell_exec()`
- [ ] Replace `HOME` env var reads → `platform::home_dir()`
- [ ] Replace `open_url()` in `commands/mod.rs` → `platform::open_url()`

### Phase 2: Paths & Directories
- [ ] Replace `home.join(".operon")` in `claude.rs` → `platform::data_dir()`
- [ ] Replace `home.join(".operon")` in `ssh.rs` → `platform::data_dir()`
- [ ] Replace `home.join(".operon/sockets")` in `ssh.rs` → `platform::ssh_sockets_dir()`
- [ ] Replace `/tmp/` references → `platform::temp_dir()`
- [ ] Replace hardcoded npm paths (`/opt/homebrew/bin/npm`) → `platform::check_tool("npm")`
- [ ] Replace hardcoded node paths → `platform::check_tool("node")`

### Phase 3: Installation Logic
- [ ] Move `install_homebrew_silent()` to `platform/macos.rs`
- [ ] Move `install_node_tarball()` to `platform/macos.rs` (rewrite for Windows/Linux)
- [ ] Move `install_xcode_cli()` to `platform/macos.rs` (stub on others)
- [ ] Move `install_phase_xcode` to `platform/macos.rs`
- [ ] Make `install_phase_tools` call `platform::install_node()` etc.
- [ ] Make `install_phase_claude` call `platform::install_claude()`
- [ ] Move osascript/Terminal.app fallbacks to `platform/macos.rs`

### Phase 4: SSH
- [ ] Move ControlMaster logic to `platform/` (macOS+Linux: real, Windows: no-op)
- [ ] Move `control_socket_path()` → `platform::ssh_sockets_dir()`
- [ ] Move `control_master_args()` → `platform::ssh_mux_args()`
- [ ] Move `control_master_active()` → `platform::ssh_mux_check()`
- [ ] Update `spawn_ssh_terminal` to use `platform::default_shell()`

### Phase 5: Platform-Specific Features
- [ ] Move dictation (Swift script) to `platform/macos.rs`
- [ ] Add capability flag: `platform::supports_dictation()`
- [ ] Move menu construction to `platform::build_menu()`
- [ ] Add `platform::supports_ssh_mux()` capability flag
- [ ] Frontend: conditional traffic-light spacer
- [ ] Frontend: correct modifier key labels (⌘ vs Ctrl)

### Phase 6: Testing & Polish
- [ ] Write unit tests for every `platform/` function
- [ ] Run full build on actual Windows 11 machine
- [ ] Run full build on actual Linux machine (Ubuntu 22.04)
- [ ] Test: terminal spawning on all 3 platforms
- [ ] Test: Claude session streaming on all 3 platforms
- [ ] Test: SSH connection on all 3 platforms
- [ ] Test: file browser on all 3 platforms
- [ ] Test: setup wizard on all 3 platforms
- [ ] Test: extension marketplace on all 3 platforms

---

## 17. Troubleshooting Common Build Failures

### Windows: `openssl-sys` build fails

Already handled: `openssl-sys` has `features = ["vendored"]` in Cargo.toml, which compiles OpenSSL from source. If it still fails, ensure Visual Studio Build Tools are installed with the C++ workload.

### Windows: `portable-pty` ConPTY errors

ConPTY requires Windows 10 1809+. Windows 11 24H2 is fine. If ConPTY fails, ensure the `conpty` feature is not disabled in Cargo.toml.

### Linux: Missing WebKit/GTK headers

```bash
sudo apt-get install -y \
  libwebkit2gtk-4.1-dev \
  libappindicator3-dev \
  librsvg2-dev \
  patchelf \
  libssl-dev \
  libgtk-3-dev
```

### macOS: Xcode CLI tools not found in CI

GitHub's `macos-latest` runner has Xcode pre-installed. If you see `xcrun` errors, add:
```yaml
- name: Select Xcode
  run: sudo xcode-select -s /Applications/Xcode.app/Contents/Developer
```

### All platforms: `npm ci` fails

Ensure `package-lock.json` is committed. `npm ci` requires it (unlike `npm install`). If you see version conflicts, delete `node_modules` and `package-lock.json`, run `npm install`, and commit the new lockfile.

### Cross-compilation (building Windows from macOS)

Not recommended for Tauri — native compilation is much simpler. Use CI runners for cross-platform builds. If you must cross-compile:

```bash
# Install cross-compilation target
rustup target add x86_64-pc-windows-msvc

# This alone won't work — you need the Windows SDK linker.
# Use cross: https://github.com/cross-rs/cross
cargo install cross
cross build --target x86_64-pc-windows-msvc
```

But seriously, just let CI handle it.

---

## Summary

Three rules. Everything flows from them.

1. **Platform code in `platform/`, nowhere else.**
2. **CI gates all three platforms on every PR.**
3. **New features are platform-agnostic by default.**

If every developer follows these rules, Operon stays a single codebase that builds everywhere, and new features just work on all platforms without anyone thinking about it.
