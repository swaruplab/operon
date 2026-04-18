use base64::Engine;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::Emitter;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command as AsyncCommand;

/// Suppress console window creation on Windows for std::process::Command.
#[cfg(windows)]
fn hide_window(cmd: &mut std::process::Command) -> &mut std::process::Command {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(0x08000000)
}
#[cfg(not(windows))]
fn hide_window(cmd: &mut std::process::Command) -> &mut std::process::Command {
    cmd
}

/// Suppress console window creation on Windows for tokio::process::Command.
#[cfg(windows)]
fn hide_window_async(cmd: &mut AsyncCommand) -> &mut AsyncCommand {
    cmd.creation_flags(0x08000000)
}
#[cfg(not(windows))]
fn hide_window_async(cmd: &mut AsyncCommand) -> &mut AsyncCommand {
    cmd
}

/// Build the env vars to pass to Claude Code based on the AI provider setting.
///
/// - "anthropic" (default): sets `ANTHROPIC_API_KEY` from the in-memory key (if any).
/// - "custom": sets `ANTHROPIC_BASE_URL` + `ANTHROPIC_AUTH_TOKEN`, which Claude
///   Code respects for OpenAI-compatible proxies (LiteLLM, claude-code-proxy,
///   vLLM, Ollama w/ a shim, etc.). If no key was configured, a placeholder
///   token is still supplied so the SDK doesn't refuse to start.
fn ai_provider_env(
    settings_state: &tauri::State<'_, super::settings::SettingsManager>,
    fallback_key: &Option<String>,
) -> Vec<(String, String)> {
    let settings = match settings_state.settings.lock() {
        Ok(s) => s.clone(),
        Err(_) => return Vec::new(),
    };
    let base = settings.custom_base_url.trim();
    if settings.ai_provider == "custom" && !base.is_empty() {
        let token = if !settings.custom_api_key.is_empty() {
            settings.custom_api_key.clone()
        } else {
            // Many local endpoints (Ollama, LM Studio) accept any non-empty token.
            "local".to_string()
        };
        vec![
            ("ANTHROPIC_BASE_URL".to_string(), base.to_string()),
            ("ANTHROPIC_AUTH_TOKEN".to_string(), token),
        ]
    } else if let Some(k) = fallback_key {
        vec![("ANTHROPIC_API_KEY".to_string(), k.clone())]
    } else {
        Vec::new()
    }
}

/// Render the same env vars as a `export K='V'; ...` string for injection into
/// remote shell scripts.
fn ai_provider_env_exports(env: &[(String, String)]) -> String {
    let mut out = String::new();
    for (k, v) in env {
        out.push_str(&format!("export {}='{}'; ", k, v.replace('\'', "'\\''")));
    }
    out
}

/// The shell used to launch Claude sessions with `-l -c` flags.
/// On Windows, Git Bash is required because cmd.exe doesn't support `-l`/`-c`
/// and Claude Code itself needs a POSIX environment.
fn claude_shell() -> String {
    #[cfg(target_os = "windows")]
    {
        crate::platform::find_git_bash_path().unwrap_or_else(|| crate::platform::default_shell())
    }
    #[cfg(not(target_os = "windows"))]
    {
        crate::platform::default_shell()
    }
}

// --- Types ---

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ClaudeStatus {
    pub installed: bool,
    pub version: Option<String>,
    pub path: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AuthStatus {
    pub authenticated: bool,
    pub method: String, // "api_key", "oauth", "none"
}

/// Persistent metadata about a Claude session, saved to ~/.operon/sessions/
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SessionMetadata {
    pub session_id: String,                // Our frontend UUID
    pub claude_session_id: Option<String>, // Claude CLI's internal session ID (for --resume)
    pub project_path: String,              // Local or remote working directory
    pub profile_id: Option<String>,        // SSH profile ID if remote
    pub remote_path: Option<String>,       // Remote path if remote
    pub mode: String,                      // "agent", "plan", "ask"
    pub model: Option<String>,
    pub created_at: u64,             // Unix timestamp ms
    pub last_activity: u64,          // Unix timestamp ms
    pub status: String,              // "running", "completed", "failed"
    pub use_terminal: bool,          // Whether this used terminal mode
    pub terminal_id: Option<String>, // Terminal ID if terminal mode
    #[serde(default)]
    pub name: Option<String>, // Human-readable session name (from first prompt)
}

/// Status of a session's output files on the filesystem
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SessionFileStatus {
    pub session_id: String,
    pub output_exists: bool,
    pub done_exists: bool,
    pub is_running: bool,   // output exists but done doesn't
    pub is_completed: bool, // both exist
}

/// PATH prefix applied to every remote SSH command that invokes `claude`.
/// Covers all known install locations so `claude` is found regardless of shell or rc files.
const REMOTE_PATH_PREFIX: &str =
    r#"export PATH="$HOME/.claude/local/bin:$HOME/.local/bin:$HOME/.npm-global/bin:$PATH"; "#;

pub struct ClaudeSession {
    pub child: tokio::process::Child,
}

pub struct ClaudeManager {
    pub sessions: Mutex<HashMap<String, ClaudeSession>>,
    pub api_key: Mutex<Option<String>>,
}

impl ClaudeManager {
    pub fn new() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            api_key: Mutex::new(None),
        }
    }
}

// --- Session Metadata Persistence ---

fn sessions_dir() -> Result<std::path::PathBuf, String> {
    crate::platform::sessions_dir()
}

fn save_session_to_disk(meta: &SessionMetadata) -> Result<(), String> {
    let dir = sessions_dir()?;
    let path = dir.join(format!("{}.json", meta.session_id));
    let data = serde_json::to_string_pretty(meta).map_err(|e| e.to_string())?;
    std::fs::write(&path, data).map_err(|e| format!("Failed to save session: {}", e))
}

fn load_session_from_disk(session_id: &str) -> Result<Option<SessionMetadata>, String> {
    let dir = sessions_dir()?;
    let path = dir.join(format!("{}.json", session_id));
    if !path.exists() {
        return Ok(None);
    }
    let data = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let meta: SessionMetadata = serde_json::from_str(&data).map_err(|e| e.to_string())?;
    Ok(Some(meta))
}

fn load_all_sessions_from_disk() -> Vec<SessionMetadata> {
    let dir = match sessions_dir() {
        Ok(d) => d,
        Err(_) => return Vec::new(),
    };
    let mut sessions = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "json") {
                if let Ok(data) = std::fs::read_to_string(&path) {
                    if let Ok(meta) = serde_json::from_str::<SessionMetadata>(&data) {
                        sessions.push(meta);
                    }
                }
            }
        }
    }
    // Sort by last_activity descending (most recent first)
    sessions.sort_by(|a, b| b.last_activity.cmp(&a.last_activity));
    sessions
}

// --- Detection & Installation ---

/// Helper: run a command through the user's login shell to get proper PATH.
/// Delegates to the platform abstraction layer.
fn login_shell_cmd(command: &str) -> std::process::Command {
    crate::platform::shell_exec(command)
}

#[tauri::command]
pub async fn check_claude_installed() -> Result<ClaudeStatus, String> {
    // Use platform-aware tool discovery (where.exe on Windows, which on Unix)
    let tool_info = crate::platform::check_tool("claude");

    if tool_info.is_none() {
        return Ok(ClaudeStatus {
            installed: false,
            version: None,
            path: None,
        });
    }

    let (path, _) = tool_info.unwrap();

    let version_output = login_shell_cmd("claude --version").output().ok();

    let version = version_output
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());

    Ok(ClaudeStatus {
        installed: true,
        version,
        path: Some(path),
    })
}

/// Install Claude Code using platform-appropriate methods.
/// Delegates to the platform abstraction layer which handles:
/// - macOS: curl installer → npm fallback → Terminal.app fallback
/// - Windows: npm install
/// - Linux: curl installer → npm fallback
#[tauri::command]
pub async fn install_claude(_method: String) -> Result<(), String> {
    crate::platform::install_claude_platform()
}

// --- Dependency Checking for Setup Wizard ---

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DependencyStatus {
    pub xcode_cli: bool,
    pub node: bool,
    pub node_version: Option<String>,
    pub npm: bool,
    pub npm_version: Option<String>,
    pub claude_code: bool,
    pub claude_version: Option<String>,
    pub git_bash: bool,
}

/// Check all local dependencies needed for Claude Code
#[tauri::command]
pub async fn check_local_dependencies() -> Result<DependencyStatus, String> {
    // Delegate to the platform abstraction layer which handles
    // tool discovery paths and Xcode checks per-platform.
    Ok(crate::platform::check_dependencies())
}

/// Refresh the process PATH from the system registry (Windows) or shell profile.
/// Called by the frontend before re-checking dependencies after user installs something.
#[tauri::command]
pub async fn refresh_environment() -> Result<(), String> {
    crate::platform::refresh_path();
    crate::platform::persist_git_bash_env();
    Ok(())
}

/// Install Xcode CLI tools (macOS only, no-op on other platforms).
/// Delegates to the platform abstraction layer.
#[tauri::command]
pub async fn install_xcode_cli() -> Result<(), String> {
    crate::platform::install_xcode_cli_platform()
}

/// The Operon-managed Node.js installation directory.
fn operon_node_dir() -> std::path::PathBuf {
    crate::platform::operon_node_dir()
}

/// Get the path to the Operon-managed `node` binary (if it exists).
fn operon_node_bin() -> Option<String> {
    let bin = operon_node_dir().join("bin").join("node");
    if bin.exists() {
        Some(bin.to_string_lossy().to_string())
    } else {
        None
    }
}

/// Install Node.js using platform-appropriate methods.
/// Delegates to the platform abstraction layer which handles:
/// - macOS: Homebrew → tarball fallback
/// - Windows: winget → tarball
/// - Linux: apt → tarball fallback
#[tauri::command]
pub async fn install_node() -> Result<(), String> {
    crate::platform::install_node_platform()
}

// ── Phased Dependency Installation ──
// Split into 3 phases so the frontend can show separate pages:
//   Phase 1: Xcode CLI Tools (can take 20-30 min on slow internet)
//   Phase 2: Homebrew + Node.js + GitHub CLI
//   Phase 3: Claude Code
//
// Each phase emits `install-progress` events with step/status/message/percent.
// The frontend shows each phase as its own page, with fallback terminal commands on failure.

#[derive(Debug, Clone, Serialize)]
pub struct InstallProgress {
    pub step: String,   // e.g. "xcode", "homebrew", "node", "gh", "claude", "done"
    pub status: String, // "starting", "downloading", "installing", "waiting", "complete", "skipped", "error"
    pub message: String,
    pub percent: u8, // 0-100 within this phase
}

fn emit_install_progress(
    app: &tauri::AppHandle,
    step: &str,
    status: &str,
    message: &str,
    percent: u8,
) {
    use tauri::Emitter;
    let _ = app.emit(
        "install-progress",
        InstallProgress {
            step: step.to_string(),
            status: status.to_string(),
            message: message.to_string(),
            percent,
        },
    );
}

/// Phase 1: Xcode CLI Tools (macOS only).
/// Triggers the macOS installer dialog and polls until it completes.
/// On non-macOS platforms, this is a no-op that returns true.
#[tauri::command]
pub async fn install_phase_xcode(app: tauri::AppHandle) -> Result<bool, String> {
    if !crate::platform::requires_xcode() {
        emit_install_progress(
            &app,
            "xcode",
            "skipped",
            "Not required on this platform",
            100,
        );
        return Ok(true);
    }

    let already = login_shell_cmd("xcode-select -p")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if already {
        emit_install_progress(
            &app,
            "xcode",
            "skipped",
            "Xcode Command Line Tools already installed",
            100,
        );
        return Ok(true);
    }

    emit_install_progress(
        &app,
        "xcode",
        "starting",
        "Installing Xcode Command Line Tools...",
        5,
    );

    // Delegate to platform layer which calls xcode-select --install
    if let Err(e) = crate::platform::install_xcode_cli_platform() {
        emit_install_progress(
            &app,
            "xcode",
            "error",
            &format!("Xcode install failed: {}", e),
            100,
        );
        return Ok(false);
    }

    emit_install_progress(
        &app,
        "xcode",
        "waiting",
        "A macOS dialog will appear — click Install and wait for it to finish.",
        10,
    );

    // Poll for up to 40 minutes (slow internet scenario)
    for i in 0..480_u32 {
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        let check = login_shell_cmd("xcode-select -p")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if check {
            emit_install_progress(
                &app,
                "xcode",
                "complete",
                "Xcode Command Line Tools installed!",
                100,
            );
            return Ok(true);
        }
        let pct = 10 + std::cmp::min((i * 85 / 480) as u8, 85);
        emit_install_progress(
            &app,
            "xcode",
            "waiting",
            "Waiting for Xcode installer...",
            pct,
        );
    }

    emit_install_progress(
        &app,
        "xcode",
        "error",
        "Xcode install timed out — it may still be running in the background.",
        100,
    );
    Ok(false)
}

/// Phase 2: Package manager + Node.js + GitHub CLI.
/// Uses platform abstraction for package manager detection and installation:
/// - macOS: Homebrew → brew install node/gh
/// - Windows: winget → winget install node/gh
/// - Linux: apt → apt install node/gh
#[tauri::command]
pub async fn install_phase_tools(app: tauri::AppHandle) -> Result<bool, String> {
    let mut all_ok = true;

    // ── Git Bash (Windows only, 0-10%) ──
    // Claude Code on Windows requires Git Bash. Install it first so that
    // Claude Code works after npm install.
    #[cfg(target_os = "windows")]
    {
        if crate::platform::find_git_bash_path().is_none() {
            emit_install_progress(
                &app,
                "git",
                "installing",
                "Downloading Git for Windows installer...",
                2,
            );

            match crate::platform::install_git_platform() {
                Ok(()) => {
                    // Shouldn't normally reach here — install_git returns Err sentinels
                    emit_install_progress(&app, "git", "complete", "Git installed!", 10);
                }
                Err(e) if e == "INSTALLER_LAUNCHED" => {
                    emit_install_progress(&app, "git", "error",
                        "Git installer launched — complete the setup wizard, then click Re-check below.", 10);
                    all_ok = false;
                }
                Err(e) if e == "BROWSER_OPENED" => {
                    emit_install_progress(
                        &app,
                        "git",
                        "error",
                        "Download page opened — install Git, then click Re-check below.",
                        10,
                    );
                    all_ok = false;
                }
                Err(e) => {
                    emit_install_progress(
                        &app,
                        "git",
                        "error",
                        &format!(
                            "Git install failed: {}. Download from https://git-scm.com",
                            e
                        ),
                        10,
                    );
                    all_ok = false;
                }
            }
        } else {
            // Git already installed — make sure env var is persisted
            crate::platform::persist_git_bash_env();
            emit_install_progress(&app, "git", "skipped", "Git already installed", 10);
        }

        // Refresh PATH after Git install so subsequent steps find git/node/npm
        crate::platform::refresh_path();
    }

    // ── Package Manager (10-50%) ──
    let mut pkg_mgr = crate::platform::find_package_manager();

    if pkg_mgr.is_none() {
        emit_install_progress(
            &app,
            "homebrew",
            "installing",
            "Installing package manager...",
            5,
        );

        match crate::platform::install_homebrew_platform() {
            Ok(path) => {
                pkg_mgr = Some(path);
                emit_install_progress(
                    &app,
                    "homebrew",
                    "complete",
                    "Package manager installed!",
                    45,
                );
            }
            Err(e) => {
                eprintln!("[Package Manager] Install failed: {}", e);
                emit_install_progress(
                    &app,
                    "homebrew",
                    "error",
                    &format!("Package manager install failed: {}", e),
                    45,
                );
                // Not fatal — Node.js can still be installed via tarball
            }
        }
    } else {
        emit_install_progress(
            &app,
            "homebrew",
            "skipped",
            "Package manager already installed",
            45,
        );
    }

    // ── Node.js (50-80%) ──
    let has_node = login_shell_cmd("node --version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
        || operon_node_bin().is_some();

    if !has_node {
        emit_install_progress(&app, "node", "installing", "Installing Node.js...", 55);

        match crate::platform::install_node_platform() {
            Ok(()) => {
                emit_install_progress(&app, "node", "complete", "Node.js installed!", 80);
            }
            Err(e) => {
                eprintln!("[Node] Install failed: {}", e);
                emit_install_progress(
                    &app,
                    "node",
                    "error",
                    "Node.js could not be installed automatically.",
                    80,
                );
                all_ok = false;
            }
        }
    } else {
        let ver = login_shell_cmd("node --version")
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default();
        emit_install_progress(
            &app,
            "node",
            "skipped",
            &format!("Node.js already installed ({})", ver),
            80,
        );
    }

    // ── GitHub CLI (80-100%) ──
    let has_gh = crate::platform::check_tool("gh").is_some();

    if !has_gh {
        emit_install_progress(&app, "gh", "installing", "Installing GitHub CLI...", 85);
        let mut gh_installed = false;

        // Strategy 1 (Windows): winget
        #[cfg(target_os = "windows")]
        {
            let winget = hide_window(std::process::Command::new("winget").args([
                "install",
                "--id",
                "GitHub.cli",
                "-e",
                "--accept-source-agreements",
                "--accept-package-agreements",
            ]))
            .output();
            if let Ok(o) = winget {
                let out_text = format!(
                    "{}{}",
                    String::from_utf8_lossy(&o.stdout),
                    String::from_utf8_lossy(&o.stderr)
                );
                if o.status.success() || out_text.contains("already installed") {
                    gh_installed = true;
                }
            }
        }

        // Strategy 2: Package manager (Homebrew on macOS, apt on Linux)
        if !gh_installed {
            if let Some(ref mgr) = pkg_mgr {
                let output =
                    hide_window(std::process::Command::new(mgr).args(["install", "gh"])).output();
                if let Ok(o) = output {
                    let stderr = String::from_utf8_lossy(&o.stderr);
                    if o.status.success() || stderr.contains("already installed") {
                        gh_installed = true;
                    } else {
                        eprintln!("[gh] {} install gh failed: {}", mgr, stderr);
                    }
                }
            }
        }

        if gh_installed {
            emit_install_progress(&app, "gh", "complete", "GitHub CLI installed!", 90);
        } else {
            eprintln!("[gh] All install strategies failed");
            emit_install_progress(
                &app,
                "gh",
                "error",
                "GitHub CLI could not be installed (optional — you can install it later).",
                90,
            );
            // gh is optional — don't fail the whole phase
        }
    } else {
        emit_install_progress(&app, "gh", "skipped", "GitHub CLI already installed", 90);
    }

    // ── Python (Windows only, 80-84%) ──
    // On macOS/Linux, Python is typically pre-installed or managed by the user.
    // On Windows, we install it automatically via winget.
    #[cfg(target_os = "windows")]
    {
        if crate::platform::find_python().is_none() {
            emit_install_progress(
                &app,
                "python",
                "installing",
                "Installing Python (required for PDF reports and research tools)...",
                81,
            );

            match crate::platform::install_python_platform() {
                Ok(()) => {
                    emit_install_progress(&app, "python", "complete", "Python installed!", 84);
                }
                Err(e) => {
                    eprintln!("[Python] Install failed: {}", e);
                    emit_install_progress(
                        &app,
                        "python",
                        "error",
                        "Python could not be installed. Install from https://python.org/downloads",
                        84,
                    );
                    // Not fatal — user can install manually
                }
            }
        } else {
            emit_install_progress(&app, "python", "skipped", "Python already installed", 84);
        }
    }

    // ── OpenSSH (Windows only, 84-87%) ──
    // macOS/Linux always have OpenSSH.
    #[cfg(target_os = "windows")]
    {
        // Check both native OpenSSH and Git Bash's ssh
        let has_ssh =
            crate::platform::has_openssh() || crate::platform::find_git_bash_path().is_some(); // Git Bash includes ssh

        if !has_ssh {
            emit_install_progress(
                &app,
                "openssh",
                "installing",
                "Installing OpenSSH client (for remote connections)...",
                85,
            );

            match crate::platform::install_openssh_platform() {
                Ok(()) => {
                    emit_install_progress(&app, "openssh", "complete", "OpenSSH installed!", 87);
                }
                Err(e) => {
                    eprintln!("[OpenSSH] Install failed: {}", e);
                    emit_install_progress(&app, "openssh", "error",
                        "OpenSSH could not be installed (optional — Git Bash provides SSH if needed).", 87);
                    // Not fatal — Git Bash includes ssh as a fallback
                }
            }
        } else {
            emit_install_progress(
                &app,
                "openssh",
                "skipped",
                "SSH available (via OpenSSH or Git Bash)",
                87,
            );
        }
    }

    // ── uv / uvx (Windows only, 87-90%) ──
    // On macOS/Linux, uv is typically installed via brew or curl by the user.
    // On Windows, we install it automatically.
    #[cfg(target_os = "windows")]
    {
        if !crate::platform::has_uv() {
            emit_install_progress(
                &app,
                "uv",
                "installing",
                "Installing uv (Python package manager for research tools)...",
                88,
            );

            match crate::platform::install_uv_platform() {
                Ok(()) => {
                    emit_install_progress(&app, "uv", "complete", "uv installed!", 90);
                }
                Err(e) => {
                    eprintln!("[uv] Install failed: {}", e);
                    emit_install_progress(
                        &app,
                        "uv",
                        "error",
                        "uv could not be installed. Install from https://docs.astral.sh/uv/",
                        90,
                    );
                }
            }
        } else {
            emit_install_progress(&app, "uv", "skipped", "uv already installed", 90);
        }
    }

    // ── Python reportlab for PDF reports (90-100%) ──
    let has_reportlab = crate::platform::has_reportlab();

    if !has_reportlab {
        emit_install_progress(
            &app,
            "reportlab",
            "installing",
            "Installing PDF report library (reportlab)...",
            92,
        );

        match crate::platform::install_reportlab_platform() {
            Ok(()) => {
                emit_install_progress(&app, "reportlab", "complete", "reportlab installed!", 100);
            }
            Err(_e) => {
                emit_install_progress(
                    &app,
                    "reportlab",
                    "error",
                    "reportlab could not be installed (Report mode will install it on first use).",
                    100,
                );
                // Don't fail the whole phase — report mode has its own fallback
            }
        }
    } else {
        emit_install_progress(
            &app,
            "reportlab",
            "skipped",
            "reportlab already installed",
            100,
        );
    }

    emit_install_progress(
        &app,
        "done",
        if all_ok { "complete" } else { "error" },
        if all_ok {
            "All tools installed!"
        } else {
            "Some items need attention"
        },
        100,
    );

    Ok(all_ok)
}

/// Phase 3: Claude Code.
/// Delegates to the platform abstraction layer which handles:
/// - macOS: curl installer → npm fallback → Terminal.app fallback
/// - Windows: curl installer via Git Bash → npm fallback
/// - Linux: curl installer → npm fallback
#[tauri::command]
pub async fn install_phase_claude(app: tauri::AppHandle) -> Result<bool, String> {
    // Refresh PATH so we can find npm/claude from tools installed in previous phases
    crate::platform::refresh_path();

    // On Windows, ensure CLAUDE_CODE_GIT_BASH_PATH is set before installing/running Claude
    crate::platform::persist_git_bash_env();

    // Check Git Bash is available on Windows before proceeding
    #[cfg(target_os = "windows")]
    {
        if crate::platform::find_git_bash_path().is_none() {
            emit_install_progress(&app, "claude", "error",
                "Git Bash is required but not installed. Please install Git from https://git-scm.com/downloads/win and restart Operon.", 100);
            return Ok(false);
        }
    }

    let has_claude = crate::platform::check_tool("claude").is_some();

    if has_claude {
        let ver = login_shell_cmd("claude --version")
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default();
        emit_install_progress(
            &app,
            "claude",
            "skipped",
            &format!("Claude Code already installed ({})", ver),
            100,
        );
        return Ok(true);
    }

    emit_install_progress(
        &app,
        "claude",
        "installing",
        "Installing Claude Code...",
        20,
    );

    match crate::platform::install_claude_platform() {
        Ok(()) => {
            emit_install_progress(&app, "claude", "complete", "Claude Code installed!", 100);
            Ok(true)
        }
        Err(e) => {
            eprintln!("[Claude] Platform install failed: {}", e);
            let hint = if cfg!(target_os = "windows") {
                "Claude Code could not be installed automatically. Try running: npm install -g @anthropic-ai/claude-code"
            } else {
                "Claude Code could not be installed automatically. Try running: curl -fsSL https://claude.ai/install.sh | bash"
            };
            emit_install_progress(&app, "claude", "error", hint, 100);
            Ok(false)
        }
    }
}

/// Legacy wrapper — calls all 3 phases sequentially.
/// Kept for backward compatibility if anything still calls it.
#[tauri::command]
pub async fn install_all_dependencies(app: tauri::AppHandle) -> Result<(), String> {
    install_phase_xcode(app.clone()).await?;
    install_phase_tools(app.clone()).await?;
    install_phase_claude(app).await?;
    Ok(())
}

/// Check if Claude Code is available on a remote server via SSH
#[tauri::command]
pub async fn check_remote_claude(
    ssh_state: tauri::State<'_, super::ssh::SSHManager>,
    profile_id: String,
) -> Result<DependencyStatus, String> {
    let profile = {
        let profiles = ssh_state.profiles.lock().map_err(|e| e.to_string())?;
        profiles
            .iter()
            .find(|p| p.id == profile_id)
            .cloned()
            .ok_or_else(|| format!("SSH profile {} not found", profile_id))?
    };

    // Check all deps in one SSH call for efficiency.
    // Check multiple locations: PATH, ~/.npm-global/bin, ~/.claude/local/bin
    let check_script = r#"
# Add common install locations to PATH
export PATH="$HOME/.npm-global/bin:$HOME/.claude/local/bin:$HOME/.local/bin:$PATH"

echo "NODE:$(node --version 2>/dev/null || echo MISSING)"
echo "NPM:$(npm --version 2>/dev/null || echo MISSING)"

# Check claude — look in PATH, official install dir, npm-global, and shell profiles
CLAUDE_VER="MISSING"
if command -v claude &>/dev/null; then
  CLAUDE_VER="$(claude --version 2>/dev/null || echo FOUND)"
elif [ -x "$HOME/.claude/local/bin/claude" ]; then
  CLAUDE_VER="$($HOME/.claude/local/bin/claude --version 2>/dev/null || echo FOUND)"
elif [ -x "$HOME/.npm-global/bin/claude" ]; then
  CLAUDE_VER="$($HOME/.npm-global/bin/claude --version 2>/dev/null || echo FOUND)"
elif [ -f ~/.bashrc ] || [ -f ~/.bash_profile ]; then
  export PS1=x
  shopt -s expand_aliases 2>/dev/null
  source ~/.bashrc 2>/dev/null
  source ~/.bash_profile 2>/dev/null
  if command -v claude &>/dev/null || alias claude &>/dev/null 2>&1; then
    CLAUDE_VER="$(claude --version 2>/dev/null || echo FOUND)"
  fi
fi
echo "CLAUDE:$CLAUDE_VER"
echo "REPORTLAB:$(python3 -c 'import reportlab; print(reportlab.Version)' 2>/dev/null || echo MISSING)"
"#;

    let result = super::ssh::ssh_exec(&profile, check_script)
        .map_err(|e| format!("SSH check failed: {}", e))?;

    let node_line = result
        .lines()
        .find(|l| l.starts_with("NODE:"))
        .unwrap_or("NODE:MISSING");
    let npm_line = result
        .lines()
        .find(|l| l.starts_with("NPM:"))
        .unwrap_or("NPM:MISSING");
    let claude_line = result
        .lines()
        .find(|l| l.starts_with("CLAUDE:"))
        .unwrap_or("CLAUDE:MISSING");
    let reportlab_line = result
        .lines()
        .find(|l| l.starts_with("REPORTLAB:"))
        .unwrap_or("REPORTLAB:MISSING");
    let _reportlab_ver = reportlab_line
        .strip_prefix("REPORTLAB:")
        .unwrap_or("MISSING");
    // reportlab status is logged but not yet surfaced in DependencyStatus

    let node_ver = node_line.strip_prefix("NODE:").unwrap_or("MISSING");
    let npm_ver = npm_line.strip_prefix("NPM:").unwrap_or("MISSING");
    let claude_ver = claude_line.strip_prefix("CLAUDE:").unwrap_or("MISSING");

    Ok(DependencyStatus {
        xcode_cli: true, // Not applicable for remote
        node: node_ver != "MISSING",
        node_version: if node_ver != "MISSING" {
            Some(node_ver.to_string())
        } else {
            None
        },
        npm: npm_ver != "MISSING",
        npm_version: if npm_ver != "MISSING" {
            Some(npm_ver.to_string())
        } else {
            None
        },
        claude_code: claude_ver != "MISSING",
        claude_version: if claude_ver != "MISSING" && claude_ver != "FOUND" {
            Some(claude_ver.to_string())
        } else {
            None
        },
        git_bash: true, // Not applicable for remote (Linux servers)
    })
}

/// Check if Claude Code on a remote server is authenticated.
/// First does a fast filesystem scan for credential files, then verifies
/// the credentials actually work by running a quick `claude -p 'ping'`.
/// Returns: "authenticated", "not_authenticated", or an error string.
#[tauri::command]
pub async fn check_remote_claude_auth(
    ssh_state: tauri::State<'_, super::ssh::SSHManager>,
    profile_id: String,
) -> Result<String, String> {
    let profile = {
        let profiles = ssh_state.profiles.lock().map_err(|e| e.to_string())?;
        profiles
            .iter()
            .find(|p| p.id == profile_id)
            .cloned()
            .ok_or_else(|| format!("SSH profile {} not found", profile_id))?
    };

    // Two-phase auth check:
    // Phase 1: Quick filesystem scan for credential files
    // Phase 2: If files found, verify they actually work with `claude -p 'ping'`
    let check_script = r#"
# Source shell profile so `claude` is in PATH
for rc in "$HOME/.bashrc" "$HOME/.bash_profile" "$HOME/.profile"; do
    [ -f "$rc" ] && . "$rc" 2>/dev/null
done
# Also check common install locations
export PATH="$HOME/.claude/local/bin:$HOME/.npm-global/bin:$HOME/.local/bin:$PATH"

CRED_FOUND=0

# Primary check: the known credential file location
if [ -s "$HOME/.claude/.credentials.json" ]; then
    CRED_FOUND=1
fi

# Fallback: check other possible credential locations
if [ "$CRED_FOUND" -eq 0 ]; then
    for f in \
        "$HOME/.claude/credentials.json" \
        "$HOME/.claude/.credentials" \
        "$HOME/.claude.json" \
        "$HOME/.config/claude/credentials.json" \
        "$HOME/.config/claude-code/credentials.json"
    do
        if [ -s "$f" ]; then
            CRED_FOUND=1
            break
        fi
    done
fi

# Fallback: scan all hidden json files in ~/.claude/
if [ "$CRED_FOUND" -eq 0 ]; then
    for f in "$HOME/.claude"/.*.json; do
        [ -s "$f" ] 2>/dev/null && { CRED_FOUND=1; break; }
    done
fi

# No credential files found at all
if [ "$CRED_FOUND" -eq 0 ]; then
    echo "AUTH:none"
    ls -la "$HOME/.claude/" 2>&1 | head -20 | while read line; do echo "DEBUG:$line"; done
    exit 0
fi

# Credential files exist — verify they actually work
# Use TERM=dumb to avoid TUI mode, timeout after 15s
if command -v claude >/dev/null 2>&1; then
    RESULT=$(TERM=dumb timeout 15 claude -p 'ping' --max-turns 1 --output-format json 2>/dev/null)
    EXIT_CODE=$?
    if [ "$EXIT_CODE" -eq 0 ] && [ -n "$RESULT" ]; then
        echo "AUTH:verified"
        exit 0
    else
        echo "AUTH:expired"
        echo "DEBUG:claude ping exit=$EXIT_CODE"
        exit 0
    fi
fi

# claude binary not in PATH but cred files exist — assume ok (may need PATH fix)
echo "AUTH:ok"
"#;

    let result = super::ssh::ssh_exec(&profile, check_script)
        .map_err(|e| format!("SSH auth check failed: {}", e))?;

    eprintln!("[Operon] Remote auth check result: {}", result.trim());

    if result.contains("AUTH:verified") || result.contains("AUTH:ok") {
        Ok("authenticated".to_string())
    } else if result.contains("AUTH:expired") {
        // Credential files exist but are expired/invalid
        Ok(format!(
            "not_authenticated:credentials_expired:{}",
            result.trim()
        ))
    } else {
        // No credentials found at all
        Ok(format!("not_authenticated:{}", result.trim()))
    }
}

/// Install Claude Code on a remote server via SSH.
/// On HPC servers users typically don't have sudo, so we configure npm
/// to use a user-local prefix (~/.npm-global) and install there.
#[tauri::command]
pub async fn install_remote_claude(
    ssh_state: tauri::State<'_, super::ssh::SSHManager>,
    profile_id: String,
) -> Result<(), String> {
    let profile = {
        let profiles = ssh_state.profiles.lock().map_err(|e| e.to_string())?;
        profiles
            .iter()
            .find(|p| p.id == profile_id)
            .cloned()
            .ok_or_else(|| format!("SSH profile {} not found", profile_id))?
    };

    // Use the official Claude Code installer (no Node.js dependency).
    // Falls back to npm if curl installer fails.
    let install_script = "
# Ensure common install locations are in PATH
export PATH=\"$HOME/.claude/local/bin:$HOME/.local/bin:$HOME/.npm-global/bin:$PATH\"

# Method 1: Official Claude Code installer (recommended, no Node.js needed)
echo '>>> Installing Claude Code via official installer...'
if command -v curl >/dev/null 2>&1; then
    curl -fsSL https://claude.ai/install.sh | bash 2>&1
    # Source updated profile so claude is in PATH
    [ -f $HOME/.bashrc ] && . $HOME/.bashrc 2>/dev/null
    [ -f $HOME/.bash_profile ] && . $HOME/.bash_profile 2>/dev/null
    [ -f $HOME/.profile ] && . $HOME/.profile 2>/dev/null
fi

# Check if it worked
if command -v claude >/dev/null 2>&1; then
    echo OPERON_INSTALL_SUCCESS
    claude --version 2>/dev/null || echo installed
    exit 0
fi

# Also check ~/.claude/local/bin (common install location)
if [ -x $HOME/.claude/local/bin/claude ]; then
    echo OPERON_INSTALL_SUCCESS
    $HOME/.claude/local/bin/claude --version 2>/dev/null || echo installed
    exit 0
fi

# Method 2: npm fallback (if Node.js is available)
if command -v npm >/dev/null 2>&1; then
    echo '>>> Curl installer did not work, trying npm fallback...'
    NPM_PREFIX=$HOME/.npm-global
    mkdir -p $NPM_PREFIX
    npm config set prefix $NPM_PREFIX 2>&1
    export PATH=$NPM_PREFIX/bin:$PATH
    npm install -g @anthropic-ai/claude-code 2>&1

    # Persist PATH
    LINE='export PATH=$HOME/.npm-global/bin:$PATH'
    for rc in $HOME/.bashrc $HOME/.bash_profile $HOME/.profile; do
        if [ -f $rc ]; then
            if ! grep -q .npm-global/bin $rc 2>/dev/null; then
                echo '' >> $rc
                echo '# Added by Operon - npm user-local bin' >> $rc
                echo $LINE >> $rc
            fi
        fi
    done

    if command -v claude >/dev/null 2>&1 || [ -x $NPM_PREFIX/bin/claude ]; then
        echo OPERON_INSTALL_SUCCESS
        claude --version 2>/dev/null || $NPM_PREFIX/bin/claude --version 2>/dev/null || echo installed
        exit 0
    fi
fi

echo OPERON_INSTALL_FAILED
";

    let result = super::ssh::ssh_exec(&profile, install_script)
        .map_err(|e| format!("Remote install failed: {}", e))?;

    if result.contains("OPERON_INSTALL_SUCCESS") {
        // Probe for the actual claude binary path and store it in server_config
        let probe_script = r#"
export PATH="$HOME/.claude/local/bin:$HOME/.local/bin:$HOME/.npm-global/bin:$PATH"
for p in "$HOME/.claude/local/bin/claude" "$HOME/.local/bin/claude" "$HOME/.npm-global/bin/claude"; do
    [ -x "$p" ] && { echo "CLAUDE_PATH:$p"; exit 0; }
done
# fallback: which claude
WHICH=$(command -v claude 2>/dev/null)
[ -n "$WHICH" ] && echo "CLAUDE_PATH:$WHICH" || echo "CLAUDE_PATH:claude"
"#;
        if let Ok(probe_result) = super::ssh::ssh_exec(&profile, probe_script) {
            if let Some(path_line) = probe_result.lines().find(|l| l.starts_with("CLAUDE_PATH:")) {
                let claude_path = path_line
                    .strip_prefix("CLAUDE_PATH:")
                    .unwrap_or("claude")
                    .trim()
                    .to_string();
                eprintln!("[operon] Detected remote claude path: {}", claude_path);
                // Store in server_config
                {
                    let mut profiles = ssh_state.profiles.lock().map_err(|e| e.to_string())?;
                    if let Some(prof) = profiles.iter_mut().find(|p| p.id == profile_id) {
                        prof.server_config
                            .insert("claude_path".to_string(), claude_path);
                        let _ = super::ssh::save_profiles_to_disk(&profiles);
                    }
                }
            }
        }

        // Ensure PATH is in all common shell rc files so `claude` works in interactive terminals
        let patch_rc_script = r#"
LINE='export PATH="$HOME/.claude/local/bin:$HOME/.local/bin:$HOME/.npm-global/bin:$PATH"'
MARKER='# Added by Operon'
for rc in "$HOME/.bashrc" "$HOME/.bash_profile" "$HOME/.profile" "$HOME/.zshrc"; do
    [ -f "$rc" ] || continue
    grep -q '.claude/local/bin\|.local/bin.*claude\|.npm-global/bin' "$rc" 2>/dev/null && continue
    printf '\n%s\n%s\n' "$MARKER" "$LINE" >> "$rc"
done
"#;
        let _ = super::ssh::ssh_exec(&profile, patch_rc_script);

        // Also install reportlab for PDF report generation on the remote server
        let reportlab_script = r#"
if python3 -c 'import reportlab' 2>/dev/null; then
    echo 'REPORTLAB_OK'
else
    echo '>>> Installing reportlab for PDF reports...'
    python3 -m pip install reportlab --user --quiet 2>/dev/null \
        || python3 -m pip install reportlab --quiet --break-system-packages 2>/dev/null \
        || pip3 install reportlab --user --quiet 2>/dev/null \
        || echo 'REPORTLAB_SKIP'
    if python3 -c 'import reportlab' 2>/dev/null; then
        echo 'REPORTLAB_OK'
    else
        echo 'REPORTLAB_SKIP'
    fi
fi
"#;
        // Best-effort: don't fail the whole install if reportlab can't be installed
        if let Ok(rl_result) = super::ssh::ssh_exec(&profile, reportlab_script) {
            if rl_result.contains("REPORTLAB_SKIP") {
                eprintln!("[operon] reportlab could not be installed on remote server — report mode will attempt at runtime");
            }
        }
        return Ok(());
    }

    // Provide a helpful error with manual install command
    Err(format!(
        "Automatic installation failed on this server.\n\n\
         You can install manually by running this in the terminal:\n  \
         curl -fsSL https://claude.ai/install.sh | bash\n\n\
         Then click Re-check in Operon.\n\n\
         Server output:\n{}",
        result.lines().take(20).collect::<Vec<_>>().join("\n")
    ))
}

// --- Remote OAuth Login ---

/// Run `claude login` on a remote server via SSH, extract the OAuth URL,
/// and return it so the frontend can display it as a clickable link.
/// After the user authenticates in their browser, they paste the code back,
/// which is sent via `remote_claude_login_code`.
#[tauri::command]
pub async fn remote_claude_login(
    ssh_state: tauri::State<'_, super::ssh::SSHManager>,
    profile_id: String,
) -> Result<String, String> {
    let profile = {
        let profiles = ssh_state.profiles.lock().map_err(|e| e.to_string())?;
        profiles
            .iter()
            .find(|p| p.id == profile_id)
            .cloned()
            .ok_or_else(|| format!("SSH profile {} not found", profile_id))?
    };

    // Resolve the claude binary path from server_config or fall back to common locations
    let claude_bin = {
        let profiles = ssh_state.profiles.lock().map_err(|e| e.to_string())?;
        profiles
            .iter()
            .find(|p| p.id == profile_id)
            .and_then(|p| p.server_config.get("claude_path").cloned())
            .unwrap_or_else(|| "claude".to_string())
    };

    // Run `claude login` on the remote server and extract the OAuth URL.
    //
    // `claude login` is interactive and needs a PTY to produce output. We use
    // the `script` command (available on all Linux/macOS) to allocate a pseudo-TTY.
    // The process runs in the background so it stays alive to receive the OAuth
    // callback after the user authenticates in their browser.
    //
    // Strategy:
    //   1. Use `script` to run `claude login` with a PTY, output to a temp file
    //   2. Background it so it stays alive for the OAuth callback
    //   3. Poll the temp file for up to 30s until the OAuth URL appears
    //   4. Strip ANSI escape codes when extracting the URL
    let login_script = format!(
        r#"{prefix}
LOGFILE="/tmp/.operon-login-$$.log"
rm -f "$LOGFILE"
touch "$LOGFILE"

# Use `script` to provide a pseudo-TTY for claude login, which needs
# interactive output to display the OAuth URL.
# Linux (util-linux) and macOS/BSD have different `script` flag syntax.
if script -V 2>&1 | grep -qi 'util-linux' || [ "$(uname)" = "Linux" ]; then
  # Linux: script -q -c 'cmd' outfile
  script -q -c 'TERM=dumb {claude_bin} login 2>&1' "$LOGFILE" </dev/null &
else
  # macOS / BSD: script -q outfile cmd...
  script -q "$LOGFILE" bash -c 'TERM=dumb {claude_bin} login 2>&1' </dev/null &
fi
LOGIN_PID=$!

# Poll for URL to appear (up to 30 seconds)
for i in $(seq 1 60); do
  # Strip ANSI escape codes and carriage returns, then look for https URL
  URL=$(sed 's/\x1b\[[0-9;]*[a-zA-Z]//g; s/\x1b\][^\x07]*\x07//g; s/\x1b[()][A-Z0-9]//g; s/\r//g' "$LOGFILE" 2>/dev/null | tr -d '\000' | grep -oE 'https://[^[:space:]"'"'"']+' | head -1)
  if [ -n "$URL" ]; then
    echo "URL:$URL"
    echo "PID:$LOGIN_PID"
    echo "LOG:$LOGFILE"
    exit 0
  fi
  sleep 0.5
done

# Timeout — dump whatever we got (strip ANSI for readability)
echo "TIMEOUT"
CLEANED=$(sed 's/\x1b\[[0-9;]*[a-zA-Z]//g; s/\x1b\][^\x07]*\x07//g; s/\x1b[()][A-Z0-9]//g; s/\r//g' "$LOGFILE" 2>/dev/null | tr -d '\000' | head -30)
if [ -n "$CLEANED" ]; then
  echo "$CLEANED"
else
  echo "(no output captured — script command may not be available)"
  # Last-resort fallback: try without PTY
  TERM=dumb timeout 10 {claude_bin} login 2>&1 | head -30 || echo "(direct login also failed)"
fi
"#,
        prefix = REMOTE_PATH_PREFIX,
        claude_bin = claude_bin,
    );

    let result = super::ssh::ssh_exec(&profile, &login_script)
        .map_err(|e| format!("Failed to run claude login: {}", e))?;

    eprintln!("[operon] remote_claude_login output: {}", result);

    // Extract the URL from our structured output
    let url = result.lines().find_map(|line| {
        let line = line.trim();
        line.strip_prefix("URL:").map(|s| s.to_string())
    });

    // If structured extraction failed, try broad URL scan as fallback
    let url = url.or_else(|| {
        result
            .lines()
            .filter_map(|line| {
                let line = line.trim();
                if let Some(start) = line.find("https://") {
                    let url_part = &line[start..];
                    let end = url_part
                        .find(|c: char| c.is_whitespace())
                        .unwrap_or(url_part.len());
                    Some(url_part[..end].to_string())
                } else {
                    None
                }
            })
            .find(|url| url.contains("claude.ai") || url.contains("anthropic.com"))
    });

    match url {
        Some(url) => Ok(url),
        None => {
            // Check for common failure reasons
            let hint = if result.contains("TIMEOUT") {
                "The login command timed out waiting for an OAuth URL — the server may be slow or `claude login` may require a different setup."
            } else if result.contains("command not found") || result.contains("not found") {
                "Claude Code may not be installed or not in PATH on this server."
            } else if result.contains("permission denied") || result.contains("Permission denied") {
                "Permission denied — check that you have access to run Claude on this server."
            } else if result.is_empty() || result.trim().is_empty() {
                "No output was returned — the SSH connection may have dropped."
            } else {
                "The login command did not produce an authentication URL."
            };
            Err(format!(
                "{}\n\nYou can log in manually by running 'claude login' in a terminal on the server.\n\nServer output:\n{}",
                hint,
                result.lines().take(10).collect::<Vec<_>>().join("\n")
            ))
        }
    }
}

// --- Authentication ---

#[tauri::command]
pub async fn store_api_key(
    state: tauri::State<'_, ClaudeManager>,
    key: String,
) -> Result<(), String> {
    let mut api_key = state.api_key.lock().map_err(|e| e.to_string())?;
    *api_key = Some(key);
    // In production, use keyring crate for macOS Keychain storage
    Ok(())
}

#[tauri::command]
pub async fn get_api_key(state: tauri::State<'_, ClaudeManager>) -> Result<Option<String>, String> {
    let api_key = state.api_key.lock().map_err(|e| e.to_string())?;
    Ok(api_key.clone())
}

#[tauri::command]
pub async fn delete_api_key(state: tauri::State<'_, ClaudeManager>) -> Result<(), String> {
    let mut api_key = state.api_key.lock().map_err(|e| e.to_string())?;
    *api_key = None;
    Ok(())
}

/// Check if the user has an active OAuth session via Claude CLI.
/// First does a fast filesystem scan of ~/.claude/ for any auth/credential
/// files. If nothing found, falls back to running `claude` through a login
/// shell to test if auth works.
#[tauri::command]
pub async fn check_oauth_status() -> Result<bool, String> {
    // Fast path: scan ~/.claude/ for any file that looks like credentials/auth
    {
        let claude_dir = crate::platform::home_dir()
            .unwrap_or_default()
            .join(".claude");
        if claude_dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&claude_dir) {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_lowercase();
                    // Look for any file with auth/credential/token/oauth in the name
                    if name.contains("credential")
                        || name.contains("auth")
                        || name.contains("token")
                        || name.contains("oauth")
                    {
                        if let Ok(content) = std::fs::read_to_string(entry.path()) {
                            let trimmed = content.trim();
                            if !trimmed.is_empty() && trimmed != "{}" && trimmed != "null" {
                                return Ok(true);
                            }
                        }
                    }
                }
            }
        }
    }

    // Slow path: actually run claude through a login shell to test auth
    let mut auth_cmd = crate::platform::shell_exec_async(
        "claude -p 'ping' --max-turns 1 --output-format json 2>/dev/null",
    );
    if let Some(git_bash_path) = crate::platform::find_git_bash_path() {
        auth_cmd.env("CLAUDE_CODE_GIT_BASH_PATH", &git_bash_path);
    }
    let output = auth_cmd.output().await.map_err(|e| e.to_string())?;

    // If claude exits 0 and produces output, auth is working
    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        if !stdout.trim().is_empty() {
            return Ok(true);
        }
    }

    Ok(false)
}

/// Launch `claude login` and open the OAuth URL in the user's browser.
///
/// Runs `claude login` as a direct child process with CLAUDE_CODE_GIT_BASH_PATH
/// set (Windows). Captures stdout/stderr, extracts the OAuth URL, and opens it
/// in the default browser. This avoids the fragile "open external terminal" approach
/// which fails when PowerShell/wt aren't in PATH or when env vars don't propagate.
#[tauri::command]
pub async fn launch_claude_login() -> Result<String, String> {
    // Refresh PATH so we can find the claude binary
    crate::platform::refresh_path();
    // Ensure Git Bash env var is set
    crate::platform::persist_git_bash_env();

    // --- Strategy 1: Run `claude login` directly and capture the OAuth URL ---
    // Find the actual claude binary path for reliable execution
    let claude_path = crate::platform::check_tool("claude").map(|(path, _)| path);

    let augmented = crate::platform::augmented_path();

    // Build the command — try direct path first, fall back to shell
    let mut cmd = if let Some(ref path) = claude_path {
        let mut c = tokio::process::Command::new(path);
        c.arg("login");
        c
    } else {
        crate::platform::shell_exec_async("claude login")
    };

    // Set environment for Claude Code
    cmd.env("PATH", &augmented);
    if let Some(git_bash_path) = crate::platform::find_git_bash_path() {
        cmd.env("CLAUDE_CODE_GIT_BASH_PATH", &git_bash_path);
    }
    // Prevent claude from trying to open a browser itself (we'll do it)
    cmd.env("BROWSER", "echo");

    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    hide_window_async(&mut cmd);

    let child = cmd.spawn();

    if let Ok(mut child) = child {
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        let url_found = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let url_clone = url_found.clone();

        // Read stdout and stderr looking for the OAuth URL
        tokio::spawn(async move {
            let mut all_output = String::new();

            // Helper to extract OAuth URL from text
            fn extract_url(text: &str) -> Option<String> {
                for line in text.lines() {
                    let trimmed = line.trim();
                    // Direct URL line
                    if trimmed.starts_with("https://") {
                        let url = trimmed.split_whitespace().next().unwrap_or(trimmed);
                        if url.contains("oauth")
                            || url.contains("claude.ai")
                            || url.contains("anthropic")
                            || url.contains("login")
                        {
                            return Some(url.to_string());
                        }
                    }
                    // "Open this URL: https://..." or similar
                    if let Some(url_start) = trimmed.find("https://") {
                        let url_part = &trimmed[url_start..];
                        let url = url_part.split_whitespace().next().unwrap_or(url_part);
                        if url.contains("claude.ai")
                            || url.contains("oauth")
                            || url.contains("anthropic")
                            || url.contains("login")
                        {
                            return Some(url.to_string());
                        }
                    }
                }
                None
            }

            // Read stdout
            let stdout_task = async {
                let mut buf = String::new();
                if let Some(mut s) = stdout {
                    use tokio::io::AsyncReadExt;
                    let mut bytes = vec![0u8; 4096];
                    loop {
                        match s.read(&mut bytes).await {
                            Ok(0) => break,
                            Ok(n) => {
                                let chunk = String::from_utf8_lossy(&bytes[..n]);
                                buf.push_str(&chunk);
                                if let Some(url) = extract_url(&chunk) {
                                    return (buf, Some(url));
                                }
                            }
                            Err(_) => break,
                        }
                    }
                }
                (buf, Option::<String>::None)
            };

            // Read stderr
            let stderr_task = async {
                let mut buf = String::new();
                if let Some(mut s) = stderr {
                    use tokio::io::AsyncReadExt;
                    let mut bytes = vec![0u8; 4096];
                    loop {
                        match s.read(&mut bytes).await {
                            Ok(0) => break,
                            Ok(n) => {
                                let chunk = String::from_utf8_lossy(&bytes[..n]);
                                buf.push_str(&chunk);
                                if let Some(url) = extract_url(&chunk) {
                                    return (buf, Some(url));
                                }
                            }
                            Err(_) => break,
                        }
                    }
                }
                (buf, Option::<String>::None)
            };

            let ((out, out_url), (err, err_url)) = tokio::join!(stdout_task, stderr_task);

            all_output.push_str(&out);
            all_output.push_str(&err);

            // Use the first URL found from either stream
            let found_url: Option<String> =
                out_url.or(err_url).or_else(|| extract_url(&all_output));

            if let Some(ref url) = found_url {
                let _ = crate::platform::open_url(url);
                if let Ok(mut u) = url_clone.lock() {
                    *u = url.clone();
                }
            }

            eprintln!(
                "[Claude Login] Output: {}",
                all_output.chars().take(500).collect::<String>()
            );
        });

        // Give the process a moment to output the URL
        tokio::time::sleep(std::time::Duration::from_secs(6)).await;

        let found = url_found.lock().map(|u| !u.is_empty()).unwrap_or(false);
        if found {
            return Ok(
                "Login page opened in your browser. Complete the sign-in, then click Verify below."
                    .to_string(),
            );
        }
    }

    // --- Strategy 2: Open an external terminal with `claude login` ---
    eprintln!("[Claude Login] Direct approach failed, trying external terminal");
    let result = crate::platform::open_terminal_with_command("claude login");
    match result {
        Ok(()) => Ok(
            "Terminal opened with Claude login. Complete sign-in there, then click Verify below."
                .to_string(),
        ),
        Err(e) => Err(format!(
            "Failed to launch login: {}. Try running 'claude login' manually in PowerShell.",
            e
        )),
    }
}

#[tauri::command]
pub async fn check_auth_status(
    state: tauri::State<'_, ClaudeManager>,
) -> Result<AuthStatus, String> {
    // Check API key first
    let has_api_key = {
        let api_key = state.api_key.lock().map_err(|e| e.to_string())?;
        api_key.is_some()
    };

    if has_api_key {
        return Ok(AuthStatus {
            authenticated: true,
            method: "api_key".to_string(),
        });
    }

    // Check OAuth credentials
    if let Ok(true) = check_oauth_status().await {
        return Ok(AuthStatus {
            authenticated: true,
            method: "oauth".to_string(),
        });
    }

    Ok(AuthStatus {
        authenticated: false,
        method: "none".to_string(),
    })
}

// --- Claude Code Session ---

/// Optional SSH context for running Claude on a remote server
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RemoteContext {
    pub profile_id: String,
    pub remote_path: String,
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn start_claude_session(
    state: tauri::State<'_, ClaudeManager>,
    terminal_state: tauri::State<'_, super::terminal::TerminalManager>,
    ssh_state: tauri::State<'_, super::ssh::SSHManager>,
    settings_state: tauri::State<'_, super::settings::SettingsManager>,
    app: tauri::AppHandle,
    session_id: String,
    prompt: String,
    project_path: String,
    model: Option<String>,
    max_turns: Option<u32>,
    resume_session: Option<String>,
    mode: Option<String>,
    remote: Option<RemoteContext>,
    use_terminal: Option<bool>,
    terminal_id: Option<String>,
) -> Result<(), String> {
    // Get API key
    let api_key = {
        let key = state.api_key.lock().map_err(|e| e.to_string())?;
        key.clone()
    };

    let mode = mode.unwrap_or_else(|| "agent".to_string());
    eprintln!(
        "[operon] start_claude_session: mode='{}', resume={:?}, max_turns={:?}",
        mode, resume_session, max_turns
    );

    // --- Check for existing plan files in the target directory ---
    // This gives Claude context about previous planning sessions in this folder.
    let existing_plan = if let Some(ref ctx) = remote {
        // Remote: read implementation_plan.md via SSH
        let profile = {
            let profiles = ssh_state.profiles.lock().map_err(|e| e.to_string())?;
            profiles.iter().find(|p| p.id == ctx.profile_id).cloned()
        };
        if let Some(prof) = profile {
            let check_cmd = format!(
                "cat '{}'/implementation_plan.md 2>/dev/null || echo ''",
                ctx.remote_path.replace('\'', "'\\''")
            );
            super::ssh::ssh_exec(&prof, &check_cmd).unwrap_or_default()
        } else {
            String::new()
        }
    } else {
        // Local: read implementation_plan.md from project path
        let plan_path = std::path::Path::new(&project_path).join("implementation_plan.md");
        std::fs::read_to_string(&plan_path).unwrap_or_default()
    };
    let existing_plan = existing_plan.trim().to_string();

    // Build the claude command string
    let escaped_prompt = prompt.replace('\'', "'\\''");

    // Build permission flag based on settings
    let permission_mode = {
        let settings = settings_state.settings.lock().map_err(|e| e.to_string())?;
        settings.permission_mode.clone()
    };
    // Permission levels control how Claude Code handles tool approvals:
    //  full_auto  — skip all permission prompts (fastest, default)
    //  safe_mode  — allow only read-only tools without prompts; Claude will be instructed
    //              to avoid destructive operations and ask the user before modifying files
    //  supervised — no permission skip; Claude runs in standard interactive mode
    //              and prompts for each tool use (works via terminal passthrough)
    let permission_flag = match permission_mode.as_str() {
        "supervised" => "",
        "safe_mode" => "--dangerously-skip-permissions",
        _ => "--dangerously-skip-permissions", // full_auto
    };
    // For safe_mode, we prepend a safety instruction to every prompt
    let safety_prefix = if permission_mode == "safe_mode" {
        "IMPORTANT SAFETY CONSTRAINT: You are in SAFE MODE. You may freely read files, search, \
         and browse, but you MUST ask the user for explicit confirmation before: \
         (1) writing or editing any file, (2) running any bash command that modifies state \
         (installs, deletes, moves, or overwrites), (3) creating new files. \
         For any such action, describe what you plan to do and wait for the user to say 'yes' or 'go ahead' \
         before executing. Read-only commands (cat, ls, grep, find, head, etc.) are always safe to run.\n\n"
            .to_string()
    } else {
        String::new()
    };

    // If there's an existing plan, prepend it as context for agent/ask modes
    let context_prefix = {
        let plan_ctx = if !existing_plan.is_empty() && mode != "plan" {
            format!(
                "CONTEXT: There is an existing implementation_plan.md in this directory from a previous planning session. \
                 Here is its content:\n\n---\n{}\n---\n\n\
                 Use this plan as context for your work. If the user's request relates to this plan, follow it. \
                 If the request is unrelated, you can ignore the plan.\n\n",
                existing_plan
            )
        } else {
            String::new()
        };
        format!("{}{}", safety_prefix, plan_ctx)
    };

    // Generate a human-readable timestamp for plan sections
    let now_timestamp = {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        // Format as YYYY-MM-DD HH:MM (UTC)
        let days = secs / 86400;
        let time_of_day = secs % 86400;
        let hours = time_of_day / 3600;
        let minutes = (time_of_day % 3600) / 60;
        // Compute year/month/day from epoch days
        let mut y = 1970i64;
        let mut remaining = days as i64;
        loop {
            let days_in_year = if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 {
                366
            } else {
                365
            };
            if remaining < days_in_year {
                break;
            }
            remaining -= days_in_year;
            y += 1;
        }
        let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
        let month_days = [
            31,
            if leap { 29 } else { 28 },
            31,
            30,
            31,
            30,
            31,
            31,
            30,
            31,
            30,
            31,
        ];
        let mut m = 0usize;
        for &md in &month_days {
            if remaining < md as i64 {
                break;
            }
            remaining -= md as i64;
            m += 1;
        }
        format!(
            "{:04}-{:02}-{:02} {:02}:{:02} UTC",
            y,
            m + 1,
            remaining + 1,
            hours,
            minutes
        )
    };
    // Also compute a filename-safe version for archiving
    let now_filename = now_timestamp.replace(' ', "_").replace(':', "");

    // --- Plan mode: archive existing plan before writing a new one ---
    // This keeps implementation_plan.md clean (always ONE active plan) while
    // preserving full history in .operon/plan_history/ for reference.
    if mode == "plan" && !existing_plan.is_empty() {
        if let Some(ref ctx) = remote {
            // Remote: archive via SSH
            let profile = {
                let profiles = ssh_state.profiles.lock().map_err(|e| e.to_string())?;
                profiles.iter().find(|p| p.id == ctx.profile_id).cloned()
            };
            if let Some(prof) = profile {
                let archive_cmd = format!(
                    "mkdir -p '{base}/.operon/plan_history' && \
                     cp '{base}/implementation_plan.md' '{base}/.operon/plan_history/plan_{ts}.md' 2>/dev/null || true",
                    base = ctx.remote_path.replace('\'', "'\\''"),
                    ts = now_filename
                );
                let _ = super::ssh::ssh_exec(&prof, &archive_cmd);
            }
        } else {
            // Local: archive to .operon/plan_history/
            let history_dir = std::path::Path::new(&project_path)
                .join(".operon")
                .join("plan_history");
            let _ = std::fs::create_dir_all(&history_dir);
            let archive_name = format!("plan_{}.md", now_filename);
            let plan_path = std::path::Path::new(&project_path).join("implementation_plan.md");
            let _ = std::fs::copy(&plan_path, history_dir.join(&archive_name));
        }
    }

    let mut claude_cmd = match mode.as_str() {
        "plan" => {
            // Plan mode: write a FRESH implementation_plan.md
            // The previous plan (if any) was just archived to .operon/plan_history/
            // Give Claude the old plan as read-only context so it can build on it,
            // but instruct it to write a completely new file.
            let existing_plan_context = if !existing_plan.is_empty() {
                format!(
                    "\n\nCONTEXT: The previous implementation plan (now archived) is shown below for reference. \
                     Use it to understand what has already been planned or completed. \
                     You may reference, build upon, or supersede it — but write your plan as a \
                     fresh, self-contained document.\n\n\
                     <previous_plan>\n{}\n</previous_plan>",
                    existing_plan
                )
            } else {
                String::new()
            };

            let plan_prompt = format!(
                "{}You are in PLAN mode.\n\n\
                 CRITICAL INSTRUCTION: Your ONLY action is to write a file called 'implementation_plan.md'. \
                 Do NOT run bash commands. Do NOT read files. Do NOT search for anything. Do NOT check MCP configurations. \
                 Do NOT use any tools except the Write tool to create implementation_plan.md. \
                 You already have all the context you need in this prompt.\n\n\
                 Write the plan to 'implementation_plan.md' in the current directory. \
                 This should be a FRESH, self-contained plan.\
                 \n\nFORMATTING RULES:\
                 \n- Start with: # Implementation Plan: <short title>\
                 \n- Add: **Date:** {}\
                 \n- Then include: 1) Overview of the task, 2) Step-by-step implementation steps, \
                 3) Files to create or modify, 4) Dependencies needed, 5) Testing strategy, \
                 6) Potential risks or considerations.\
                 \n- Include a '## Status' section with each step marked as [ ] (pending) \
                 so that Agent mode can track progress.\
                 \n- If the previous plan had steps marked [x] (completed), you may note those as \
                 already done in your new plan so Agent mode knows not to redo them.{}\
                 \n\nREMEMBER: Do NOT run any bash/shell commands. Just write the plan file directly.\
                 \n\nThe user's request: {}",
                safety_prefix,
                now_timestamp,
                existing_plan_context,
                escaped_prompt
            );
            format!(
                "claude {} -p '{}' --verbose --output-format stream-json",
                permission_flag,
                plan_prompt.replace('\'', "'\\''")
            )
        }
        "report" => {
            // Report mode: Claude drafts a scientific report based on project files.
            // The frontend sends a structured prompt with inline file contents, methods info,
            // PubMed citations, and user instructions.
            //
            // IMPORTANT: The prompt can be 200KB+ (31 files × 8KB each). We CANNOT pass
            // this via -p '...' because shell argument escaping breaks on file contents
            // (single quotes, backticks, $variables, heredoc delimiters in CSV/code data).
            // Instead, write the prompt to a temp file and pipe it to Claude via stdin.
            let tool_instruction =
                "CRITICAL: All file contents are already provided inline in this prompt inside <file> tags. \
                 Do NOT use any tools — no Read, no Bash, no Glob, no Grep, no file operations whatsoever. \
                 You have exactly 1 turn. Write the entire report directly from the provided file contents and context. \
                 Any attempt to use tools will fail and waste your only turn.";
            let report_prompt = format!(
                "You are in REPORT mode — a scientific report generator for bioinformatics analyses. \
                 Your task is to produce a professional analysis report based on the project files and context provided.\n\n\
                 {}\n\n\
                 RULES:\n\
                 1. Write in formal scientific prose suitable for a research report.\n\
                 2. Every factual claim about biology must cite a PubMed reference using [N] notation.\n\
                 3. The Methods section must list tools with version numbers — omit infrastructure details (SLURM, conda envs, HPC configs).\n\
                 4. Interpret results biologically — don't just describe what the plots show, explain what they mean.\n\
                 5. The Discussion should connect findings to the broader literature.\n\
                 6. Use the implementation_plan.md (if available) to understand what analyses were performed.\n\n\
                 Output the report NOW as structured markdown sections (# Title, ## Abstract, ## Introduction, \
                 ## Results, ## Discussion, ## Methods, ## References). \
                 Write each section thoroughly — this will become a PDF.\n\n\
                 {}{}",
                tool_instruction,
                context_prefix,
                // Use the raw prompt here — no shell escaping needed since it goes to a file
                prompt
            );

            // Write prompt to a local temp file — this bypasses all shell escaping issues
            let prompt_file =
                std::env::temp_dir().join(format!("operon-report-prompt-{}.txt", session_id));
            let prompt_file_str = prompt_file.to_string_lossy().to_string();
            std::fs::write(&prompt_file, &report_prompt)
                .map_err(|e| format!("Failed to write report prompt file: {}", e))?;
            eprintln!(
                "[operon] Report prompt written to {} ({} bytes)",
                prompt_file_str,
                report_prompt.len()
            );

            // Pipe prompt from file via stdin. -p enables print mode (non-interactive),
            // and the positional prompt argument comes from stdin.
            // Use platform-appropriate file-to-stdin command
            #[cfg(target_os = "windows")]
            let pipe_cmd = format!(
                "type \"{}\" | claude {} -p --verbose --output-format stream-json",
                prompt_file_str, permission_flag
            );
            #[cfg(not(target_os = "windows"))]
            let pipe_cmd = format!(
                "cat '{}' | claude {} -p --verbose --output-format stream-json",
                prompt_file_str, permission_flag
            );
            pipe_cmd
        }
        "ask" => {
            // Ask mode: no tool use, answer questions with scientific rigor
            let ask_prompt = format!(
                "You are in ASK mode — a scientific Q&A assistant for bioinformatics researchers. \
                 Do NOT use any tools (no file reads, writes, or bash commands). \
                 Answer the user's question using your knowledge and any PubMed literature provided in the prompt. \
                 If PubMed articles are included in <pubmed_literature> tags, you MUST:\n\
                 1. Directly reference and cite the provided articles by number [1], [2], etc.\n\
                 2. Include PubMed URLs so the user can access the original papers.\n\
                 3. Base your answer primarily on the evidence in these articles.\n\
                 4. End your response with a formatted References section.\n\
                 If you need to look at files or run commands, tell the user to switch to Agent mode.\n\n{}\
                 {}",
                context_prefix,
                escaped_prompt
            );
            format!(
                "claude {} -p '{}' --verbose --output-format stream-json --max-turns 1",
                permission_flag,
                ask_prompt.replace('\'', "'\\''")
            )
        }
        _ => {
            // Agent mode (default): full tool use
            // If there's a plan, tell Claude to follow it and update status
            let agent_prompt = if !existing_plan.is_empty() {
                format!(
                    "{}IMPORTANT: As you complete steps from the implementation plan, \
                     update implementation_plan.md to mark completed steps with [x] \
                     so progress is tracked.\n\n{}",
                    context_prefix, escaped_prompt
                )
            } else {
                format!("{}{}", context_prefix, escaped_prompt)
            };
            format!(
                "claude {} -p '{}' --verbose --output-format stream-json",
                permission_flag,
                agent_prompt.replace('\'', "'\\''")
            )
        }
    };

    if let Some(m) = &model {
        claude_cmd.push_str(&format!(" --model {}", m));
    }
    if mode == "plan" {
        claude_cmd.push_str(" --max-turns 3");
    } else if mode == "report" {
        // Report mode: all file contents are pre-read and injected into the prompt.
        // 1 turn is all that's needed — block all tools to prevent wasted reads.
        let report_turns = max_turns.unwrap_or(1);
        claude_cmd.push_str(&format!(" --max-turns {}", report_turns));
        claude_cmd.push_str(" --disallowedTools Read,Bash,Glob,Grep");
    } else if let Some(turns) = max_turns {
        claude_cmd.push_str(&format!(" --max-turns {}", turns));
    } else {
        // Default max-turns for agent mode to prevent infinite loops.
        // 30 turns is enough for complex multi-step tasks while ensuring
        // the agent eventually stops if it gets stuck in a polling cycle.
        claude_cmd.push_str(" --max-turns 30");
    }
    if let Some(resume) = &resume_session {
        claude_cmd.push_str(&format!(" --resume {}", resume));
    }

    eprintln!(
        "[operon] Final claude command (first 200 chars): {}",
        &claude_cmd[..claude_cmd.len().min(200)]
    );

    // Sync MCP servers into Claude Code's native config so they're available
    // without relying on --mcp-config (which has known bugs in some Claude Code versions).
    let mcp_servers = {
        let settings = settings_state.settings.lock().map_err(|e| e.to_string())?;
        settings.mcp_servers.clone()
    };
    let _ = super::mcp::sync_mcp_servers_to_claude(&mcp_servers);

    // Also generate mcp-config.json and pass --mcp-config as fallback
    // (needed for remote/HPC sessions where Claude runs on a different host).
    if let Some(config_path) = super::mcp::generate_mcp_config(&mcp_servers)? {
        // Shell-escape the path in case it contains spaces
        claude_cmd.push_str(&format!(
            " --mcp-config '{}'",
            config_path.replace('\'', "'\\''")
        ));
    }

    let shell = claude_shell();

    let use_terminal = use_terminal.unwrap_or(false);

    // --- Persist session metadata so it survives app restarts ---
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    // Derive session name from first ~50 chars of prompt
    let session_name = {
        let trimmed = prompt.trim();
        if trimmed.len() > 50 {
            format!(
                "{}...",
                &trimmed[..trimmed
                    .char_indices()
                    .nth(50)
                    .map(|(i, _)| i)
                    .unwrap_or(trimmed.len())]
            )
        } else {
            trimmed.to_string()
        }
    };

    let meta = SessionMetadata {
        session_id: session_id.clone(),
        claude_session_id: resume_session.clone(),
        project_path: project_path.clone(),
        profile_id: remote.as_ref().map(|r| r.profile_id.clone()),
        remote_path: remote.as_ref().map(|r| r.remote_path.clone()),
        mode: mode.clone(),
        model: model.clone(),
        created_at: now,
        last_activity: now,
        status: "running".to_string(),
        use_terminal,
        terminal_id: terminal_id.clone(),
        name: Some(session_name),
    };
    let _ = save_session_to_disk(&meta);

    // --- TERMINAL MODE: run Claude inside the user's existing terminal session ---
    // This reuses their tmux/compute node/conda environment
    if use_terminal {
        if let (Some(ref ctx), Some(ref tid)) = (&remote, &terminal_id) {
            let profile = {
                let profiles = ssh_state.profiles.lock().map_err(|e| e.to_string())?;
                profiles
                    .iter()
                    .find(|p| p.id == ctx.profile_id)
                    .cloned()
                    .ok_or_else(|| format!("SSH profile {} not found", ctx.profile_id))?
            };

            // For HPC terminal mode, write MCP config to the remote shared filesystem
            // so the claude process on the compute node can access it.
            if let Some(mcp_json) = super::mcp::generate_mcp_config_json(&mcp_servers)? {
                let mcp_config_remote = format!("{}/.operon-mcp-config.json", ctx.remote_path);
                let encoded_json =
                    base64::engine::general_purpose::STANDARD.encode(mcp_json.as_bytes());
                let write_cmd = format!(
                    "echo '{}' | base64 -d > '{}'",
                    encoded_json,
                    mcp_config_remote.replace('\'', "'\\''")
                );
                let _ = super::ssh::ssh_exec(&profile, &write_cmd);
                // Replace the local config path in claude_cmd with the remote path
                if let Some(local_path) = super::mcp::generate_mcp_config(&mcp_servers)? {
                    claude_cmd = claude_cmd.replace(
                        &format!("--mcp-config '{}'", local_path),
                        &format!(
                            "--mcp-config '{}'",
                            mcp_config_remote.replace('\'', "'\\''")
                        ),
                    );
                }
            }

            // For report mode, upload the local prompt file to the remote shared filesystem
            // so the `cat prompt | claude` command works on the compute node.
            // Uses SCP (with ControlMaster reuse) — reliable for any file size, no encoding issues.
            if mode == "report" {
                let local_prompt_file = std::env::temp_dir()
                    .join(format!("operon-report-prompt-{}.txt", session_id))
                    .to_string_lossy()
                    .to_string();
                let remote_prompt_file = format!(
                    "{}/.operon-report-prompt-{}.txt",
                    ctx.remote_path, session_id
                );
                if std::path::Path::new(&local_prompt_file).exists() {
                    let host_str = format!("{}@{}", profile.user, profile.host);
                    let mut scp_args: Vec<String> = vec![
                        "-o".to_string(),
                        "BatchMode=yes".to_string(),
                        "-o".to_string(),
                        "ConnectTimeout=10".to_string(),
                    ];
                    // Reuse ControlMaster socket if available
                    let sock = crate::platform::ssh_socket_path(
                        &profile.host,
                        profile.port,
                        &profile.user,
                    );
                    if sock.exists() {
                        scp_args.push("-o".to_string());
                        scp_args.push(format!("ControlPath={}", sock.to_string_lossy()));
                    }
                    if profile.port != 22 {
                        scp_args.push("-P".to_string());
                        scp_args.push(profile.port.to_string());
                    }
                    if let Some(key) = &profile.key_file {
                        if std::path::Path::new(key).exists() {
                            scp_args.push("-i".to_string());
                            scp_args.push(key.clone());
                        }
                    }
                    scp_args.push(local_prompt_file.clone());
                    scp_args.push(format!("{}:{}", host_str, remote_prompt_file));

                    let scp_result =
                        hide_window(std::process::Command::new("scp").args(&scp_args)).output();
                    match scp_result {
                        Ok(output) if output.status.success() => {
                            let file_size = std::fs::metadata(&local_prompt_file)
                                .map(|m| m.len())
                                .unwrap_or(0);
                            eprintln!(
                                "[operon] SCP uploaded report prompt to remote: {} ({} bytes)",
                                remote_prompt_file, file_size
                            );
                        }
                        Ok(output) => {
                            let stderr = String::from_utf8_lossy(&output.stderr);
                            eprintln!("[operon] SCP upload failed: {}", stderr);
                        }
                        Err(e) => {
                            eprintln!("[operon] SCP command failed: {}", e);
                        }
                    }
                    // Replace the local path in claude_cmd with the remote path
                    claude_cmd = claude_cmd.replace(&local_prompt_file, &remote_prompt_file);
                }
            }

            // Create a unique output file path on the SHARED filesystem (not /tmp which is node-local).
            // On HPC systems, /tmp is local to each node — the compute node writes the file but
            // the tail SSH connects to the login node, which can't see compute-node /tmp.
            // Use the remote working directory which is on a shared NFS/GPFS filesystem.
            let output_file = format!("{}/.operon-{}.jsonl", ctx.remote_path, session_id);
            let done_file = format!("{}/.operon-{}.done", ctx.remote_path, session_id);

            // Write the claude command to a temp script, then `source` it.
            // This keeps the terminal clean (only "source /path/.cf-run.sh" is visible)
            // while preserving the user's shell aliases (unlike piping to `bash`).
            let script_file = format!("{}/.operon-run-{}.sh", ctx.remote_path, session_id);
            // Clean up the report prompt file after Claude finishes (if it exists)
            let prompt_cleanup = if mode == "report" {
                format!(
                    "; rm -f '{}/.operon-report-prompt-{}.txt'",
                    ctx.remote_path.replace('\'', "'\\''"),
                    session_id
                )
            } else {
                String::new()
            };
            // Export provider env vars in the script so Claude can authenticate
            // on the remote. In terminal mode the user's shell may not have
            // them set. For custom endpoints this emits ANTHROPIC_BASE_URL +
            // ANTHROPIC_AUTH_TOKEN instead of ANTHROPIC_API_KEY.
            let provider_env = ai_provider_env(&settings_state, &api_key);
            let api_key_line = ai_provider_env_exports(&provider_env);
            let script_content = format!(
                "{}{}cd '{}' && {} > '{}' 2>&1; echo $? > '{}'{}",
                REMOTE_PATH_PREFIX,
                api_key_line,
                ctx.remote_path.replace('\'', "'\\''"),
                claude_cmd,
                output_file.replace('\'', "'\\''"),
                done_file.replace('\'', "'\\''"),
                prompt_cleanup,
            );

            // Upload the script to the remote via ssh_exec + base64.
            // Uses chunked transfer to avoid ControlMaster socket message size
            // limits (~256KB). Each chunk is appended to a temp b64 file on the
            // remote, then decoded in one shot.
            {
                let b64_script =
                    base64::engine::general_purpose::STANDARD.encode(script_content.as_bytes());
                let escaped_script = script_file.replace('"', "\\\"");
                let tmp_b64 = format!("{}.__b64__", escaped_script);
                const CHUNK_SIZE: usize = 100_000;

                if b64_script.len() <= CHUNK_SIZE {
                    // Small script — single command
                    let write_cmd = format!(
                        "printf %s {} | base64 -d > \"{}\"",
                        b64_script, escaped_script,
                    );
                    crate::commands::ssh::ssh_exec(&profile, &write_cmd)
                        .map_err(|e| format!("Failed to create run script on remote: {}", e))?;
                } else {
                    // Large script — write base64 in chunks, then decode
                    let mut offset = 0;
                    let mut first = true;
                    while offset < b64_script.len() {
                        let end = std::cmp::min(offset + CHUNK_SIZE, b64_script.len());
                        let chunk = &b64_script[offset..end];
                        let redirect = if first { ">" } else { ">>" };
                        let cmd = format!("printf %s {} {} \"{}\"", chunk, redirect, tmp_b64,);
                        crate::commands::ssh::ssh_exec(&profile, &cmd).map_err(|e| {
                            format!("Failed to upload script chunk to remote: {}", e)
                        })?;
                        first = false;
                        offset = end;
                    }
                    // Decode the assembled base64 and clean up
                    let decode_cmd = format!(
                        "base64 -d \"{}\" > \"{}\" && rm -f \"{}\"",
                        tmp_b64, escaped_script, tmp_b64,
                    );
                    crate::commands::ssh::ssh_exec(&profile, &decode_cmd)
                        .map_err(|e| format!("Failed to decode run script on remote: {}", e))?;
                }
            }

            // Send a short source command to the terminal (the script is already on the remote)
            // The leading space prevents it from appearing in shell history.
            let terminal_cmd = format!(
                " clear; source '{}'; rm -f '{}'\n",
                script_file.replace('\'', "'\\''"),
                script_file.replace('\'', "'\\''"),
            );

            // Write the command into the existing terminal
            let encoded = terminal_cmd.as_bytes().to_vec();
            {
                let terminals = terminal_state.terminals.lock().map_err(|e| e.to_string())?;
                let handle = terminals
                    .get(tid)
                    .ok_or_else(|| format!("Terminal {} not found", tid))?;
                let mut writer = handle.writer.lock().map_err(|e| e.to_string())?;
                use std::io::Write;
                writer.write_all(&encoded).map_err(|e| e.to_string())?;
                writer.flush().map_err(|e| e.to_string())?;
            }

            // Now tail the output file via a separate SSH connection to stream results back.
            // Reuse ControlMaster socket if available — avoids re-authentication (critical
            // for HPC clusters with Duo MFA where a second auth would block/fail).
            let mut ssh_tail_args = format!(
                "ssh -o BatchMode=yes -o ConnectTimeout=10 -o ServerAliveInterval=15 -o ServerAliveCountMax=3 -o TCPKeepAlive=yes {}@{} -p {}",
                profile.user, profile.host, profile.port
            );
            // Reuse ControlMaster socket if one exists from the main terminal connection
            let ctrl_sock =
                crate::platform::ssh_socket_path(&profile.host, profile.port, &profile.user);
            if ctrl_sock.exists() {
                ssh_tail_args.push_str(&format!(
                    " -o \"ControlPath={}\"",
                    ctrl_sock.to_string_lossy()
                ));
            }
            if let Some(key) = &profile.key_file {
                ssh_tail_args.push_str(&format!(" -i {}", key));
            }
            // Wait for the output file to appear, then tail -f it.
            // Use base64 encoding to completely avoid all shell quoting/expansion issues
            // across the local shell → SSH → remote shell → bash -c chain.
            // The tail script streams the JSONL output file back to the local machine.
            // Key fixes for reliability:
            //   1. Use stdbuf/unbuffer to force line-buffered output through SSH pipe.
            //      Without this, SSH block-buffers stdout (4KB) so small JSON lines
            //      accumulate silently, causing the "thinking but not responding" symptom.
            //   2. Use tail --pid or a polling loop so tail exits promptly when done.
            //   3. Read any remaining lines after tail exits (tail -f may miss the last write).
            let tail_script = format!(
                "i=0; while [ ! -f '{}' ] && [ \"$i\" -lt 1500 ]; do sleep 0.2; i=$((i+1)); done; \
                 if [ ! -f '{}' ]; then echo '{{\"type\":\"error\",\"error\":{{\"message\":\"Output file did not appear after 5 minutes. The command may have failed to start — check the terminal.\"}}}}'; exit 1; fi; \
                 if command -v stdbuf >/dev/null 2>&1; then \
                   TAIL_CMD=\"stdbuf -oL tail -f '{}'\"; \
                 else \
                   TAIL_CMD=\"tail -f '{}'\"; \
                 fi; \
                 eval $TAIL_CMD & TAIL_PID=$!; \
                 while [ ! -f '{}' ]; do sleep 0.5; done; \
                 sleep 0.5; kill $TAIL_PID 2>/dev/null; wait $TAIL_PID 2>/dev/null; \
                 cat '{}'; \
                 rm -f '{}' '{}'",
                output_file, output_file,
                output_file.replace('\'', "'\\''"),
                output_file.replace('\'', "'\\''"),
                done_file,
                output_file.replace('\'', "'\\''"),
                output_file, done_file,
            );
            // Base64-encode the script and have the REMOTE shell decode+execute it.
            // This avoids ALL quoting issues: local shell sees only safe base64 chars.
            let b64_tail = base64::engine::general_purpose::STANDARD.encode(tail_script.as_bytes());
            // The remote command: echo <b64> | base64 -d | bash
            // We pass this directly to SSH (no -- bash -c wrapper needed).
            // SSH sends its args as a single command string to the remote shell.
            ssh_tail_args.push_str(&format!(" \"echo {} | base64 -d | bash\"", b64_tail));

            let mut tail_cmd = AsyncCommand::new(&shell);
            tail_cmd.arg("-l").arg("-c").arg(&ssh_tail_args);
            for (k, v) in ai_provider_env(&settings_state, &api_key) {
                tail_cmd.env(k, v);
            }
            tail_cmd.stdout(std::process::Stdio::piped());
            tail_cmd.stderr(std::process::Stdio::piped());
            hide_window_async(&mut tail_cmd);

            let mut child = tail_cmd
                .spawn()
                .map_err(|e| format!("Failed to start tail: {}", e))?;
            let stdout = child.stdout.take().ok_or("Failed to capture tail stdout")?;
            let stderr = child.stderr.take();

            // Store as a session so it can be stopped
            state
                .sessions
                .lock()
                .map_err(|e| e.to_string())?
                .insert(session_id.clone(), ClaudeSession { child });

            // Stream stdout (JSON lines from the output file)
            let app_handle = app.clone();
            let sid = session_id.clone();
            tokio::spawn(async move {
                let reader = BufReader::new(stdout);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    if line.trim().is_empty() {
                        continue;
                    }
                    let _ = app_handle.emit(
                        &format!("claude-event-{}", sid),
                        serde_json::json!({ "line": line }),
                    );
                }
                let _ = app_handle.emit(&format!("claude-done-{}", sid), serde_json::json!({}));
            });

            // Handle stderr (suppress SSH warnings)
            if let Some(stderr) = stderr {
                let app_handle2 = app.clone();
                let sid2 = session_id.clone();
                tokio::spawn(async move {
                    let reader = BufReader::new(stderr);
                    let mut lines = reader.lines();
                    let mut error_buf = String::new();
                    while let Ok(Some(line)) = lines.next_line().await {
                        if !line.trim().is_empty() {
                            error_buf.push_str(&line);
                            error_buf.push('\n');
                        }
                    }
                    let trimmed = error_buf.trim();
                    if !trimmed.is_empty() {
                        let is_just_warning = trimmed.lines().all(|l| {
                            let lt = l.trim().trim_start_matches('*').trim();
                            lt.is_empty()
                                || lt.contains("WARNING")
                                || lt.contains("Warning")
                                || lt.contains("warning")
                                || lt.contains("sntrup")
                                || lt.contains("mlkem")
                                || lt.contains("post-quantum")
                                || lt.contains("quantum")
                                || lt.contains("vulnerable")
                                || lt.contains("decrypt later")
                                || lt.contains("upgraded")
                                || lt.contains("openssh.com")
                                || lt.contains("store now")
                                || lt.contains("key exchange")
                                || lt.contains("no stdin data")
                                || lt.contains("redirect stdin")
                                || lt.contains("piping from")
                                || lt.contains("/dev/null")
                                || lt.contains("wait longer")
                                || lt.contains("proceeding without")
                                || lt.contains("Connection to")
                                || lt.contains("Killed by signal")
                                || lt.contains("Transferred:")
                                || lt.contains("kex_exchange")
                                || lt.contains("banner")
                                || lt.starts_with("debug")
                                || lt.contains("file truncated")
                                || lt.contains("tail:")
                        });
                        if !is_just_warning {
                            let _ = app_handle2.emit(
                                &format!("claude-event-{}", sid2),
                                serde_json::json!({
                                    "line": format!(
                                        "{{\"type\":\"error\",\"error\":{{\"message\":\"{}\"}}}}",
                                        trimmed.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n")
                                    )
                                }),
                            );
                        }
                    }
                });
            }

            return Ok(());
        } else {
            return Err(
                "Terminal mode requires a remote connection and an active terminal".to_string(),
            );
        }
    }

    // Decide: local or remote execution
    let mut cmd = if let Some(ref ctx) = remote {
        // --- REMOTE: run claude via SSH on the remote server ---
        let profile = {
            let profiles = ssh_state.profiles.lock().map_err(|e| e.to_string())?;
            profiles
                .iter()
                .find(|p| p.id == ctx.profile_id)
                .cloned()
                .ok_or_else(|| format!("SSH profile {} not found", ctx.profile_id))?
        };

        // Step 1: Figure out how to invoke claude on the remote server.
        // It might be: a binary in PATH, an alias (e.g. alias claude='npx @anthropic-ai/claude-code'),
        // or available via npx. We detect all cases and return the actual invocation command.
        let find_claude_cmd = r#"
            # 1. Check for a real binary at common install locations
            for p in \
                "$HOME/.local/bin/claude" \
                "$HOME/.npm-global/bin/claude" \
                "$HOME/.npm/bin/claude" \
                "$HOME/bin/claude" \
                "$HOME/.yarn/bin/claude" \
                "$HOME/.bun/bin/claude" \
                /usr/local/bin/claude; do
                [ -x "$p" ] && echo "$p" && exit 0
            done
            # Check NVM paths
            for p in "$HOME"/.nvm/versions/node/*/bin/claude; do
                [ -x "$p" ] && echo "$p" && exit 0
            done

            # 2. Source profile files to get aliases and full PATH
            # Set PS1 to trick .bashrc into thinking this is interactive
            # (most .bashrc files have: [ -z "$PS1" ] && return)
            # Also enable alias expansion so `alias` builtin works after sourcing
            export PS1=x
            shopt -s expand_aliases 2>/dev/null
            . "$HOME/.profile" 2>/dev/null
            . "$HOME/.bash_profile" 2>/dev/null
            . "$HOME/.bashrc" 2>/dev/null
            . "$HOME/.nvm/nvm.sh" 2>/dev/null

            # 3. Check if claude is a real binary via which
            w=$(which claude 2>/dev/null)
            if [ -n "$w" ] && [ -x "$w" ]; then
                echo "$w"
                exit 0
            fi

            # 4. Check if claude is an alias — extract the underlying command
            a=$(alias claude 2>/dev/null)
            if [ -n "$a" ]; then
                # alias output: alias claude='npx @anthropic-ai/claude-code'
                # Extract the command between quotes
                cmd=$(echo "$a" | sed "s/^[^']*'//;s/'[^']*$//")
                if [ -n "$cmd" ]; then
                    echo "ALIAS:$cmd"
                    exit 0
                fi
            fi

            # 5. Check if npx can run it directly
            npx_path=$(which npx 2>/dev/null)
            if [ -n "$npx_path" ]; then
                echo "ALIAS:$npx_path @anthropic-ai/claude-code"
                exit 0
            fi

            echo ""
        "#;
        let claude_resolve = super::ssh::ssh_exec(&profile, find_claude_cmd).unwrap_or_default();
        let claude_resolve = claude_resolve.trim().to_string();

        if claude_resolve.is_empty() || claude_resolve.contains("not found") {
            return Err("Claude CLI not found on the remote server. \
                        Install it with: curl -fsSL https://claude.ai/install.sh | bash"
                .to_string());
        }

        // Step 2: Replace `claude` with the resolved command
        // If it starts with "ALIAS:", it's a multi-word command (e.g. "npx @anthropic-ai/claude-code")
        // Otherwise it's an absolute binary path
        let claude_invoke = if let Some(alias_cmd) = claude_resolve.strip_prefix("ALIAS:") {
            alias_cmd.trim().to_string()
        } else {
            claude_resolve.clone()
        };

        // For report mode, upload the prompt file to the remote server via SCP
        if mode == "report" {
            let local_prompt_file = std::env::temp_dir()
                .join(format!("operon-report-prompt-{}.txt", session_id))
                .to_string_lossy()
                .to_string();
            let remote_prompt_file = format!(
                "{}/.operon-report-prompt-{}.txt",
                ctx.remote_path, session_id
            );
            if std::path::Path::new(&local_prompt_file).exists() {
                let host_str = format!("{}@{}", profile.user, profile.host);
                let mut scp_args: Vec<String> = vec![
                    "-o".to_string(),
                    "BatchMode=yes".to_string(),
                    "-o".to_string(),
                    "ConnectTimeout=10".to_string(),
                ];
                let sock =
                    crate::platform::ssh_socket_path(&profile.host, profile.port, &profile.user);
                if sock.exists() {
                    scp_args.push("-o".to_string());
                    scp_args.push(format!("ControlPath={}", sock.to_string_lossy()));
                }
                if profile.port != 22 {
                    scp_args.push("-P".to_string());
                    scp_args.push(profile.port.to_string());
                }
                if let Some(key) = &profile.key_file {
                    if std::path::Path::new(key).exists() {
                        scp_args.push("-i".to_string());
                        scp_args.push(key.clone());
                    }
                }
                scp_args.push(local_prompt_file.clone());
                scp_args.push(format!("{}:{}", host_str, remote_prompt_file));

                match hide_window(std::process::Command::new("scp").args(&scp_args)).output() {
                    Ok(output) if output.status.success() => {
                        let file_size = std::fs::metadata(&local_prompt_file)
                            .map(|m| m.len())
                            .unwrap_or(0);
                        eprintln!(
                            "[operon] SCP uploaded report prompt: {} ({} bytes)",
                            remote_prompt_file, file_size
                        );
                    }
                    Ok(output) => {
                        eprintln!(
                            "[operon] SCP upload failed: {}",
                            String::from_utf8_lossy(&output.stderr)
                        );
                    }
                    Err(e) => {
                        eprintln!("[operon] SCP command failed: {}", e);
                    }
                }
                claude_cmd = claude_cmd.replace(&local_prompt_file, &remote_prompt_file);
            }
        }

        let claude_cmd_abs = claude_cmd.replacen("claude ", &format!("{} ", claude_invoke), 1);

        // Step 3: Build the remote command — source profile for PATH (needed for npx/node)
        // then cd to the working directory and run claude
        // For report mode, the command is `cat file | claude ...` — don't redirect stdin from /dev/null.
        // For other modes, redirect stdin to prevent Claude from hanging waiting for input.
        let stdin_redirect = if mode == "report" { "" } else { " < /dev/null" };
        // Forward provider env vars to the remote command — SSH doesn't forward
        // env vars by default, and HPC servers rarely have AcceptEnv configured
        // for custom vars. For custom endpoints this forwards ANTHROPIC_BASE_URL
        // + ANTHROPIC_AUTH_TOKEN so the remote Claude hits the same proxy.
        let api_key_export = ai_provider_env_exports(&ai_provider_env(&settings_state, &api_key));
        let remote_cmd = format!(
            "export PS1=x; . \"$HOME/.profile\" 2>/dev/null; . \"$HOME/.bash_profile\" 2>/dev/null; . \"$HOME/.bashrc\" 2>/dev/null; . \"$HOME/.nvm/nvm.sh\" 2>/dev/null; {}cd '{}' && {}{}",
            api_key_export,
            ctx.remote_path.replace('\'', "'\\''"),
            claude_cmd_abs,
            stdin_redirect
        );

        // Base64-encode to avoid nested quoting issues
        let encoded_cmd = base64::engine::general_purpose::STANDARD.encode(remote_cmd.as_bytes());

        // No -tt flag! We need clean stdout for JSON parsing, not a PTY.
        let mut ssh_args = format!(
            "ssh -o BatchMode=yes -o ConnectTimeout=10 -o ServerAliveInterval=15 -o ServerAliveCountMax=3 -o TCPKeepAlive=yes {}@{} -p {}",
            profile.user, profile.host, profile.port
        );
        // Reuse ControlMaster socket if available (avoids re-auth on Duo MFA clusters)
        let ctrl_sock =
            crate::platform::ssh_socket_path(&profile.host, profile.port, &profile.user);
        if ctrl_sock.exists() {
            ssh_args.push_str(&format!(
                " -o \"ControlPath={}\"",
                ctrl_sock.to_string_lossy()
            ));
        }
        if let Some(key) = &profile.key_file {
            ssh_args.push_str(&format!(" -i {}", key));
        }
        // Decode and execute on the remote side
        ssh_args.push_str(&format!(
            " -- bash -c \"$(echo {} | base64 -d)\"",
            encoded_cmd
        ));

        let mut c = AsyncCommand::new(&shell);
        c.arg("-l").arg("-c").arg(&ssh_args);
        c
    } else {
        // --- LOCAL: run claude directly ---
        let mut c = AsyncCommand::new(&shell);
        c.arg("-l").arg("-c").arg(&claude_cmd);
        c.current_dir(&project_path);
        c
    };

    for (k, v) in ai_provider_env(&settings_state, &api_key) {
        cmd.env(k, v);
    }

    // On Windows, Claude Code requires Git Bash. Set the path so it can find it.
    if let Some(git_bash_path) = crate::platform::find_git_bash_path() {
        cmd.env("CLAUDE_CODE_GIT_BASH_PATH", &git_bash_path);
    }

    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    hide_window_async(&mut cmd);

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to start Claude: {}", e))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Failed to capture stdout".to_string())?;

    let stderr = child.stderr.take();

    // Store session
    state
        .sessions
        .lock()
        .map_err(|e| e.to_string())?
        .insert(session_id.clone(), ClaudeSession { child });

    // Spawn stdout reader task
    let app_handle = app.clone();
    let sid = session_id.clone();
    // Persist output to .jsonl file so sessions can be resumed/reconnected.
    // For local sessions this was previously missing — output was only streamed live.
    let output_jsonl_path = format!("{}/.operon-{}.jsonl", project_path, session_id);
    let done_marker_path = format!("{}/.operon-{}.done", project_path, session_id);

    tokio::spawn(async move {
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();

        // Open the output file for appending (create if needed)
        let mut output_file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&output_jsonl_path)
            .await
            .ok();

        while let Ok(Some(line)) = lines.next_line().await {
            if line.trim().is_empty() {
                continue;
            }

            // Emit the raw JSON line to frontend for parsing
            let _ = app_handle.emit(
                &format!("claude-event-{}", sid),
                serde_json::json!({ "line": line }),
            );

            // Persist to disk for session resume
            if let Some(ref mut f) = output_file {
                use tokio::io::AsyncWriteExt;
                let _ = f.write_all(line.as_bytes()).await;
                let _ = f.write_all(b"\n").await;
            }
        }

        // Stream ended — write done marker and emit event
        let _ = tokio::fs::write(&done_marker_path, "done").await;
        let _ = app_handle.emit(&format!("claude-done-{}", sid), serde_json::json!({}));
    });

    // Spawn stderr reader task — surface SSH/remote errors to the frontend
    if let Some(stderr) = stderr {
        let app_handle2 = app.clone();
        let sid2 = session_id.clone();

        tokio::spawn(async move {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            let mut error_buf = String::new();

            while let Ok(Some(line)) = lines.next_line().await {
                if !line.trim().is_empty() {
                    error_buf.push_str(&line);
                    error_buf.push('\n');
                }
            }

            // If there was meaningful stderr output, send it as an error event
            let trimmed = error_buf.trim();
            if !trimmed.is_empty() {
                // Filter out common SSH warnings (post-quantum key exchange, etc.)
                let is_just_warning = trimmed.lines().all(|l| {
                    let lt = l.trim().trim_start_matches('*').trim();
                    lt.is_empty()
                        || lt.contains("WARNING")
                        || lt.contains("Warning")
                        || lt.contains("warning")
                        || lt.contains("sntrup")
                        || lt.contains("mlkem")
                        || lt.contains("post-quantum")
                        || lt.contains("quantum")
                        || lt.contains("vulnerable")
                        || lt.contains("decrypt later")
                        || lt.contains("upgraded")
                        || lt.contains("openssh.com")
                        || lt.contains("store now")
                        || lt.contains("key exchange")
                        || lt.contains("no stdin data")
                        || lt.contains("redirect stdin")
                        || lt.contains("piping from")
                        || lt.contains("/dev/null")
                        || lt.contains("wait longer")
                        || lt.contains("proceeding without")
                        || lt.contains("file truncated")
                        || lt.contains("tail:")
                });

                if !is_just_warning {
                    let _ = app_handle2.emit(
                        &format!("claude-event-{}", sid2),
                        serde_json::json!({
                            "line": format!(
                                "{{\"type\":\"error\",\"error\":{{\"message\":\"{}\"}}}}",
                                trimmed.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n")
                            )
                        }),
                    );
                }
            }
        });
    }

    Ok(())
}

#[tauri::command]
pub async fn stop_claude_session(
    state: tauri::State<'_, ClaudeManager>,
    session_id: String,
) -> Result<(), String> {
    // Extract session from lock first, then await kill — never hold Mutex across .await
    let session = {
        let mut sessions = state.sessions.lock().map_err(|e| e.to_string())?;
        sessions.remove(&session_id)
    };

    if let Some(mut session) = session {
        let _ = session.child.kill().await;
    }

    Ok(())
}

/// Check if an implementation_plan.md exists in the given directory (local or remote).
/// Returns the plan content if found, or an empty string if not.
#[tauri::command]
pub async fn check_existing_plan(
    ssh_state: tauri::State<'_, super::ssh::SSHManager>,
    project_path: String,
    remote: Option<RemoteContext>,
) -> Result<String, String> {
    if let Some(ctx) = remote {
        // Remote: check via SSH
        let profile = {
            let profiles = ssh_state.profiles.lock().map_err(|e| e.to_string())?;
            profiles
                .iter()
                .find(|p| p.id == ctx.profile_id)
                .cloned()
                .ok_or_else(|| format!("SSH profile {} not found", ctx.profile_id))?
        };
        let check_cmd = format!(
            "cat '{}'/implementation_plan.md 2>/dev/null || echo ''",
            ctx.remote_path.replace('\'', "'\\''")
        );
        let content = super::ssh::ssh_exec(&profile, &check_cmd).unwrap_or_default();
        Ok(content.trim().to_string())
    } else {
        // Local
        let plan_path = std::path::Path::new(&project_path).join("implementation_plan.md");
        let content = std::fs::read_to_string(&plan_path).unwrap_or_default();
        Ok(content.trim().to_string())
    }
}

/// Archive the current implementation_plan.md to .operon/plan_history/ before a new plan is written.
/// Called by the frontend before starting a plan session, so archival happens regardless of
/// what mode string the backend receives.
/// Returns Ok(true) if a plan was archived, Ok(false) if there was no plan to archive.
#[tauri::command]
pub async fn archive_current_plan(
    ssh_state: tauri::State<'_, super::ssh::SSHManager>,
    project_path: String,
    remote: Option<RemoteContext>,
) -> Result<bool, String> {
    // Generate timestamp for the archive filename
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;
    let mut y = 1970i64;
    let mut remaining = days as i64;
    loop {
        let days_in_year = if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 {
            366
        } else {
            365
        };
        if remaining < days_in_year {
            break;
        }
        remaining -= days_in_year;
        y += 1;
    }
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let month_days = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut m = 0usize;
    for &md in &month_days {
        if remaining < md as i64 {
            break;
        }
        remaining -= md as i64;
        m += 1;
    }
    let ts = format!(
        "{:04}-{:02}-{:02}_{:02}{:02}{:02}_UTC",
        y,
        m + 1,
        remaining + 1,
        hours,
        minutes,
        seconds
    );

    if let Some(ctx) = remote {
        let profile = {
            let profiles = ssh_state.profiles.lock().map_err(|e| e.to_string())?;
            profiles.iter().find(|p| p.id == ctx.profile_id).cloned()
        };
        if let Some(prof) = profile {
            let base = ctx.remote_path.replace('\'', "'\\''");
            // Check if plan exists, archive it, then return
            let cmd = format!(
                "if [ -f '{base}/implementation_plan.md' ]; then \
                     mkdir -p '{base}/.operon/plan_history' && \
                     cp '{base}/implementation_plan.md' '{base}/.operon/plan_history/plan_{ts}.md' && \
                     echo 'ARCHIVED'; \
                 else echo 'NO_PLAN'; fi"
            );
            let result = super::ssh::ssh_exec(&prof, &cmd).unwrap_or_default();
            return Ok(result.contains("ARCHIVED"));
        }
        Ok(false)
    } else {
        let plan_path = std::path::Path::new(&project_path).join("implementation_plan.md");
        if plan_path.is_file() {
            let history_dir = std::path::Path::new(&project_path)
                .join(".operon")
                .join("plan_history");
            std::fs::create_dir_all(&history_dir)
                .map_err(|e| format!("Failed to create plan_history dir: {}", e))?;
            let archive_name = format!("plan_{}.md", ts);
            std::fs::copy(&plan_path, history_dir.join(&archive_name))
                .map_err(|e| format!("Failed to archive plan: {}", e))?;
            eprintln!(
                "[operon] Archived implementation_plan.md → .operon/plan_history/{}",
                archive_name
            );
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

/// Archived plan entry returned to the frontend.
#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct PlanHistoryEntry {
    pub filename: String,
    pub timestamp: String, // e.g. "2026-03-29 14:30:05"
    pub title: String,     // first heading or "Untitled Plan"
    pub lines: u64,
    pub path: String, // full path to the archived file
}

/// List all archived plans from .operon/plan_history/, newest first.
#[tauri::command]
pub async fn list_plan_history(project_path: String) -> Result<Vec<PlanHistoryEntry>, String> {
    let history_dir = std::path::Path::new(&project_path)
        .join(".operon")
        .join("plan_history");
    if !history_dir.is_dir() {
        return Ok(vec![]);
    }

    let mut entries: Vec<PlanHistoryEntry> = Vec::new();
    let dir = std::fs::read_dir(&history_dir).map_err(|e| e.to_string())?;
    for entry in dir.flatten() {
        let fname = entry.file_name().to_string_lossy().to_string();
        if !fname.starts_with("plan_") || !fname.ends_with(".md") {
            continue;
        }
        // Parse timestamp from filename: plan_YYYY-MM-DD_HHMMSS.md
        let ts_part = fname.trim_start_matches("plan_").trim_end_matches(".md");
        let timestamp = ts_part
            .replacen('_', " ", 1) // "2026-03-29 143005"
            .chars()
            .enumerate()
            .map(|(i, c)| {
                // Insert colons into HHMMSS → HH:MM:SS
                if i == 13 || i == 15 {
                    ':'
                } else {
                    c
                }
            })
            .collect::<String>();

        let full_path = entry.path();
        let content = std::fs::read_to_string(&full_path).unwrap_or_default();
        let line_count = content.lines().count() as u64;

        // Extract title from first heading
        let title = content
            .lines()
            .find(|l| l.starts_with("# "))
            .map(|l| l.trim_start_matches("# ").trim().to_string())
            .unwrap_or_else(|| "Untitled Plan".to_string());

        entries.push(PlanHistoryEntry {
            filename: fname,
            timestamp,
            title,
            lines: line_count,
            path: full_path.to_string_lossy().to_string(),
        });
    }

    // Sort newest first
    entries.sort_by(|a, b| b.filename.cmp(&a.filename));
    Ok(entries)
}

/// Read the content of a specific archived plan.
#[tauri::command]
pub async fn read_plan_history_entry(path: String) -> Result<String, String> {
    std::fs::read_to_string(&path).map_err(|e| format!("Failed to read plan: {}", e))
}

// --- Session Management Commands ---

/// Save session metadata to disk. Called by frontend after session starts or updates.
#[tauri::command]
pub async fn save_session_metadata(metadata: SessionMetadata) -> Result<(), String> {
    save_session_to_disk(&metadata)
}

/// Update the claude_session_id for an existing session (called when we capture it from stream).
#[tauri::command]
pub async fn update_session_claude_id(
    session_id: String,
    claude_session_id: String,
) -> Result<(), String> {
    if let Some(mut meta) = load_session_from_disk(&session_id)? {
        meta.claude_session_id = Some(claude_session_id);
        meta.last_activity = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        save_session_to_disk(&meta)
    } else {
        Err(format!("Session {} not found", session_id))
    }
}

/// Mark a session as completed or failed.
#[tauri::command]
pub async fn update_session_status(session_id: String, status: String) -> Result<(), String> {
    if let Some(mut meta) = load_session_from_disk(&session_id)? {
        meta.status = status;
        meta.last_activity = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        save_session_to_disk(&meta)
    } else {
        Err(format!("Session {} not found", session_id))
    }
}

/// List sessions for a given project path (local or remote).
/// Returns sessions sorted by most recent first.
#[tauri::command]
pub async fn list_sessions(
    project_path: Option<String>,
    profile_id: Option<String>,
) -> Result<Vec<SessionMetadata>, String> {
    let all = load_all_sessions_from_disk();
    let filtered: Vec<SessionMetadata> = all
        .into_iter()
        .filter(|s| {
            // Filter by project path or profile if provided
            let path_match = project_path.as_ref().is_none_or(|p| {
                s.project_path == *p || s.remote_path.as_deref() == Some(p.as_str())
            });
            let profile_match = profile_id
                .as_ref()
                .is_none_or(|pid| s.profile_id.as_deref() == Some(pid.as_str()));
            path_match && profile_match
        })
        .collect();
    Ok(filtered)
}

/// Check the status of a session's output files on the filesystem (local or remote).
#[tauri::command]
pub async fn check_session_files(
    ssh_state: tauri::State<'_, super::ssh::SSHManager>,
    session_id: String,
    remote: Option<RemoteContext>,
) -> Result<SessionFileStatus, String> {
    // Load session metadata to find the output file path
    let meta = load_session_from_disk(&session_id)?
        .ok_or_else(|| format!("Session {} not found", session_id))?;

    let base_path = meta.remote_path.as_deref().unwrap_or(&meta.project_path);
    let output_file = format!("{}/.operon-{}.jsonl", base_path, session_id);
    let done_file = format!("{}/.operon-{}.done", base_path, session_id);

    if let Some(ctx) = remote {
        // Remote: check via SSH
        let profile = {
            let profiles = ssh_state.profiles.lock().map_err(|e| e.to_string())?;
            profiles
                .iter()
                .find(|p| p.id == ctx.profile_id)
                .cloned()
                .ok_or_else(|| format!("SSH profile {} not found", ctx.profile_id))?
        };
        let check_cmd = format!(
            "echo -n \"output:\"; test -f '{}' && echo 'yes' || echo 'no'; \
             echo -n \"done:\"; test -f '{}' && echo 'yes' || echo 'no'",
            output_file.replace('\'', "'\\''"),
            done_file.replace('\'', "'\\''"),
        );
        let result = super::ssh::ssh_exec(&profile, &check_cmd).unwrap_or_default();
        let output_exists = result.contains("output:yes");
        let done_exists = result.contains("done:yes");
        Ok(SessionFileStatus {
            session_id,
            output_exists,
            done_exists,
            is_running: output_exists && !done_exists,
            is_completed: output_exists && done_exists,
        })
    } else {
        // Local
        let output_exists = std::path::Path::new(&output_file).exists();
        let done_exists = std::path::Path::new(&done_file).exists();
        Ok(SessionFileStatus {
            session_id,
            output_exists,
            done_exists,
            is_running: output_exists && !done_exists,
            is_completed: output_exists && done_exists,
        })
    }
}

/// Read the full output of a completed session (.jsonl file).
/// Returns the raw content for the frontend to parse into messages.
#[tauri::command]
pub async fn read_session_output(
    ssh_state: tauri::State<'_, super::ssh::SSHManager>,
    session_id: String,
    remote: Option<RemoteContext>,
) -> Result<String, String> {
    let meta = load_session_from_disk(&session_id)?
        .ok_or_else(|| format!("Session {} not found", session_id))?;

    let base_path = meta.remote_path.as_deref().unwrap_or(&meta.project_path);
    let output_file = format!("{}/.operon-{}.jsonl", base_path, session_id);

    if let Some(ctx) = remote {
        let profile = {
            let profiles = ssh_state.profiles.lock().map_err(|e| e.to_string())?;
            profiles
                .iter()
                .find(|p| p.id == ctx.profile_id)
                .cloned()
                .ok_or_else(|| format!("SSH profile {} not found", ctx.profile_id))?
        };
        let cat_cmd = format!("cat '{}'", output_file.replace('\'', "'\\''"));
        let content = super::ssh::ssh_exec(&profile, &cat_cmd)
            .map_err(|e| format!("Failed to read session output: {}", e))?;
        Ok(content)
    } else {
        std::fs::read_to_string(&output_file)
            .map_err(|e| format!("Failed to read session output: {}", e))
    }
}

/// Reconnect to a running session by tailing the .jsonl file.
/// This spawns a tail process and streams events back to the frontend.
#[tauri::command]
pub async fn reconnect_session(
    state: tauri::State<'_, ClaudeManager>,
    ssh_state: tauri::State<'_, super::ssh::SSHManager>,
    app: tauri::AppHandle,
    session_id: String,       // The old session's ID (to find the files)
    event_session_id: String, // The current frontend session ID (for event channels)
    remote: Option<RemoteContext>,
) -> Result<(), String> {
    let meta = load_session_from_disk(&session_id)?
        .ok_or_else(|| format!("Session {} not found", session_id))?;

    let base_path = meta.remote_path.as_deref().unwrap_or(&meta.project_path);
    let output_file = format!("{}/.operon-{}.jsonl", base_path, session_id);
    let done_file = format!("{}/.operon-{}.done", base_path, session_id);

    let shell = claude_shell();

    if let Some(ctx) = remote {
        let profile = {
            let profiles = ssh_state.profiles.lock().map_err(|e| e.to_string())?;
            profiles
                .iter()
                .find(|p| p.id == ctx.profile_id)
                .cloned()
                .ok_or_else(|| format!("SSH profile {} not found", ctx.profile_id))?
        };

        // Build SSH command to tail the output file
        let mut ssh_tail_args = format!(
            "ssh -o BatchMode=yes -o ConnectTimeout=10 -o ServerAliveInterval=15 -o ServerAliveCountMax=3 -o TCPKeepAlive=yes {}@{} -p {}",
            profile.user, profile.host, profile.port
        );
        if let Some(key) = &profile.key_file {
            ssh_tail_args.push_str(&format!(" -i {}", key));
        }

        // Tail script: first cat any existing content, then tail -f for new lines
        // If done file already exists, just cat and exit (session already finished)
        let tail_script = format!(
            "if [ -f '{}' ]; then cat '{}'; exit 0; fi; \
             if [ ! -f '{}' ]; then echo '{{\"type\":\"error\",\"error\":{{\"message\":\"Output file not found\"}}}}'; exit 1; fi; \
             cat '{}'; tail -f -n +$(wc -l < '{}' | tr -d ' ') '{}' & TAIL_PID=$!; \
             while [ ! -f '{}' ]; do sleep 1; done; \
             sleep 1; kill $TAIL_PID 2>/dev/null; wait $TAIL_PID 2>/dev/null",
            done_file, output_file,
            output_file,
            output_file, output_file, output_file,
            done_file,
        );
        let b64_tail = base64::engine::general_purpose::STANDARD.encode(tail_script.as_bytes());
        ssh_tail_args.push_str(&format!(" \"echo {} | base64 -d | bash\"", b64_tail));

        let mut tail_cmd = AsyncCommand::new(&shell);
        tail_cmd.arg("-l").arg("-c").arg(&ssh_tail_args);
        tail_cmd.stdout(std::process::Stdio::piped());
        tail_cmd.stderr(std::process::Stdio::piped());
        hide_window_async(&mut tail_cmd);

        let mut child = tail_cmd
            .spawn()
            .map_err(|e| format!("Failed to reconnect: {}", e))?;
        let stdout = child
            .stdout
            .take()
            .ok_or("Failed to capture reconnect stdout")?;

        // Store as a session so it can be stopped
        state
            .sessions
            .lock()
            .map_err(|e| e.to_string())?
            .insert(event_session_id.clone(), ClaudeSession { child });

        // Stream output to frontend using the CURRENT frontend session ID for events
        let app_handle = app.clone();
        let sid = event_session_id.clone();
        tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if line.trim().is_empty() {
                    continue;
                }
                let _ = app_handle.emit(
                    &format!("claude-event-{}", sid),
                    serde_json::json!({ "line": line }),
                );
            }
            let _ = app_handle.emit(&format!("claude-done-{}", sid), serde_json::json!({}));
        });

        Ok(())
    } else {
        // Local reconnect — just read the file
        let content = std::fs::read_to_string(&output_file)
            .map_err(|e| format!("Failed to read output: {}", e))?;
        for line in content.lines() {
            if !line.trim().is_empty() {
                let _ = app.emit(
                    &format!("claude-event-{}", event_session_id),
                    serde_json::json!({ "line": line }),
                );
            }
        }
        let _ = app.emit(
            &format!("claude-done-{}", event_session_id),
            serde_json::json!({}),
        );
        Ok(())
    }
}

/// Reconnect a stalled tail SSH connection without stopping the agent.
/// Kills the existing tail process for this session and spawns a fresh one
/// that cats existing output then tail -f's for new lines.
#[tauri::command]
pub async fn reconnect_tail(
    state: tauri::State<'_, ClaudeManager>,
    ssh_state: tauri::State<'_, super::ssh::SSHManager>,
    app: tauri::AppHandle,
    session_id: String,
    remote: Option<RemoteContext>,
) -> Result<(), String> {
    // 1. Kill the stalled tail process (but NOT the agent on the compute node)
    let old_session = {
        let mut sessions = state.sessions.lock().map_err(|e| e.to_string())?;
        sessions.remove(&session_id)
    };
    if let Some(mut old) = old_session {
        let _ = old.child.kill().await;
    }

    // 2. Figure out the file paths from session metadata or remote context
    let ctx = remote.ok_or("Reconnect tail is only supported for remote sessions")?;
    let profile = {
        let profiles = ssh_state.profiles.lock().map_err(|e| e.to_string())?;
        profiles
            .iter()
            .find(|p| p.id == ctx.profile_id)
            .cloned()
            .ok_or_else(|| format!("SSH profile {} not found", ctx.profile_id))?
    };

    let output_file = format!("{}/.operon-{}.jsonl", ctx.remote_path, session_id);
    let done_file = format!("{}/.operon-{}.done", ctx.remote_path, session_id);
    let shell = claude_shell();

    // 3. Build a fresh SSH tail command with tighter keepalives
    let mut ssh_tail_args = format!(
        "ssh -o BatchMode=yes -o ConnectTimeout=10 -o ServerAliveInterval=15 -o ServerAliveCountMax=3 -o TCPKeepAlive=yes {}@{} -p {}",
        profile.user, profile.host, profile.port
    );
    let ctrl_sock = crate::platform::ssh_socket_path(&profile.host, profile.port, &profile.user);
    if ctrl_sock.exists() {
        ssh_tail_args.push_str(&format!(
            " -o \"ControlPath={}\"",
            ctrl_sock.to_string_lossy()
        ));
    }
    if let Some(key) = &profile.key_file {
        ssh_tail_args.push_str(&format!(" -i {}", key));
    }

    // Tail script: cat existing content, then tail -f for new lines.
    // If the done file already exists the session finished while we were stalled — just cat.
    // Uses stdbuf -oL to force line-buffered output and prevent SSH pipe buffering.
    let tail_script = format!(
        "if [ -f '{}' ]; then cat '{}'; exit 0; fi; \
         if [ ! -f '{}' ]; then echo '{{\"type\":\"error\",\"error\":{{\"message\":\"Output file not found — the agent may have finished or the file was cleaned up.\"}}}}'; exit 1; fi; \
         if command -v stdbuf >/dev/null 2>&1; then \
           stdbuf -oL cat '{}'; stdbuf -oL tail -f -n +$(($(wc -l < '{}' | tr -d ' ') + 1)) '{}' & TAIL_PID=$!; \
         else \
           cat '{}'; tail -f -n +$(($(wc -l < '{}' | tr -d ' ') + 1)) '{}' & TAIL_PID=$!; \
         fi; \
         while [ ! -f '{}' ]; do sleep 0.5; done; \
         sleep 0.5; kill $TAIL_PID 2>/dev/null; wait $TAIL_PID 2>/dev/null; \
         cat '{}'",
        done_file, output_file,
        output_file,
        output_file, output_file, output_file,
        output_file, output_file, output_file,
        done_file,
        output_file,
    );
    let b64_tail = base64::engine::general_purpose::STANDARD.encode(tail_script.as_bytes());
    ssh_tail_args.push_str(&format!(" \"echo {} | base64 -d | bash\"", b64_tail));

    let mut tail_cmd = AsyncCommand::new(&shell);
    tail_cmd.arg("-l").arg("-c").arg(&ssh_tail_args);
    tail_cmd.stdout(std::process::Stdio::piped());
    tail_cmd.stderr(std::process::Stdio::piped());
    hide_window_async(&mut tail_cmd);

    let mut child = tail_cmd
        .spawn()
        .map_err(|e| format!("Failed to reconnect tail: {}", e))?;
    let stdout = child.stdout.take().ok_or("Failed to capture tail stdout")?;

    // 4. Store the new tail process as the session's child
    state
        .sessions
        .lock()
        .map_err(|e| e.to_string())?
        .insert(session_id.clone(), ClaudeSession { child });

    // 5. Stream output to frontend — the dedup logic in the frontend handles
    //    re-sent lines gracefully (same message ID = replace, not duplicate).
    let app_handle = app.clone();
    let sid = session_id.clone();
    tokio::spawn(async move {
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if line.trim().is_empty() {
                continue;
            }
            let _ = app_handle.emit(
                &format!("claude-event-{}", sid),
                serde_json::json!({ "line": line }),
            );
        }
        let _ = app_handle.emit(&format!("claude-done-{}", sid), serde_json::json!({}));
    });

    Ok(())
}

/// Rename a session (update its human-readable name).
#[tauri::command]
pub async fn rename_session(session_id: String, name: String) -> Result<(), String> {
    if let Some(mut meta) = load_session_from_disk(&session_id).map_err(|e| e.to_string())? {
        meta.name = Some(name);
        save_session_to_disk(&meta)?;
        Ok(())
    } else {
        Err(format!("Session {} not found", session_id))
    }
}

/// Delete a session's metadata and optionally its output files.
#[tauri::command]
pub async fn delete_session(
    ssh_state: tauri::State<'_, super::ssh::SSHManager>,
    session_id: String,
    remote: Option<RemoteContext>,
    delete_output: Option<bool>,
) -> Result<(), String> {
    // Delete metadata file
    let dir = sessions_dir()?;
    let path = dir.join(format!("{}.json", session_id));
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| format!("Failed to delete session: {}", e))?;
    }

    // Optionally delete output files
    if delete_output.unwrap_or(false) {
        if let Some(meta) = load_session_from_disk(&session_id).ok().flatten() {
            let base_path = meta.remote_path.as_deref().unwrap_or(&meta.project_path);
            let output_file = format!("{}/.operon-{}.jsonl", base_path, session_id);
            let done_file = format!("{}/.operon-{}.done", base_path, session_id);

            if let Some(ctx) = remote {
                let profile = {
                    let profiles = ssh_state.profiles.lock().map_err(|e| e.to_string())?;
                    profiles.iter().find(|p| p.id == ctx.profile_id).cloned()
                };
                if let Some(profile) = profile {
                    let rm_cmd = format!(
                        "rm -f '{}' '{}'",
                        output_file.replace('\'', "'\\''"),
                        done_file.replace('\'', "'\\''"),
                    );
                    let _ = super::ssh::ssh_exec(&profile, &rm_cmd);
                }
            } else {
                let _ = std::fs::remove_file(&output_file);
                let _ = std::fs::remove_file(&done_file);
            }
        }
    }

    Ok(())
}
