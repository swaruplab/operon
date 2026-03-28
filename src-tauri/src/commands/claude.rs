use base64::Engine;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::Emitter;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command as AsyncCommand;

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
    pub session_id: String,           // Our frontend UUID
    pub claude_session_id: Option<String>, // Claude CLI's internal session ID (for --resume)
    pub project_path: String,         // Local or remote working directory
    pub profile_id: Option<String>,   // SSH profile ID if remote
    pub remote_path: Option<String>,  // Remote path if remote
    pub mode: String,                 // "agent", "plan", "ask"
    pub model: Option<String>,
    pub created_at: u64,              // Unix timestamp ms
    pub last_activity: u64,           // Unix timestamp ms
    pub status: String,               // "running", "completed", "failed"
    pub use_terminal: bool,           // Whether this used terminal mode
    pub terminal_id: Option<String>,  // Terminal ID if terminal mode
    #[serde(default)]
    pub name: Option<String>,         // Human-readable session name (from first prompt)
}

/// Status of a session's output files on the filesystem
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SessionFileStatus {
    pub session_id: String,
    pub output_exists: bool,
    pub done_exists: bool,
    pub is_running: bool,       // output exists but done doesn't
    pub is_completed: bool,     // both exist
}

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
    let home = dirs::home_dir().ok_or("Could not determine home directory")?;
    let dir = home.join(".operon").join("sessions");
    if !dir.exists() {
        std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create sessions dir: {}", e))?;
    }
    Ok(dir)
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
            if path.extension().map_or(false, |ext| ext == "json") {
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

/// Helper: run a command through the user's login shell to get proper PATH
fn login_shell_cmd(command: &str) -> std::process::Command {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
    let mut cmd = std::process::Command::new(&shell);
    cmd.arg("-l").arg("-c").arg(command);
    cmd
}

#[tauri::command]
pub async fn check_claude_installed() -> Result<ClaudeStatus, String> {
    let which = match login_shell_cmd("which claude").output() {
        Ok(o) => o,
        Err(_) => {
            return Ok(ClaudeStatus {
                installed: false,
                version: None,
                path: None,
            });
        }
    };

    if !which.status.success() {
        return Ok(ClaudeStatus {
            installed: false,
            version: None,
            path: None,
        });
    }

    let path = String::from_utf8_lossy(&which.stdout).trim().to_string();

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

#[tauri::command]
pub async fn install_claude(method: String) -> Result<(), String> {
    // Already installed?
    let has_claude = login_shell_cmd("claude --version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if has_claude {
        return Ok(());
    }

    // Primary method: official curl installer (works regardless of `method` param)
    eprintln!("[Claude Code] Attempting install via official installer...");
    let output = login_shell_cmd("curl -fsSL https://claude.ai/install.sh | bash").output();

    match output {
        Ok(ref o) if o.status.success() => {
            eprintln!("[Claude Code] Installed successfully via curl installer");
            // Verify the binary is accessible
            let check = login_shell_cmd("claude --version").output();
            if check.map(|c| c.status.success()).unwrap_or(false) {
                return Ok(());
            }
            // Also check common install location directly
            if let Some(home) = dirs::home_dir() {
                if home.join(".claude/local/bin/claude").exists() {
                    return Ok(());
                }
            }
        }
        Ok(ref o) => {
            let stderr = String::from_utf8_lossy(&o.stderr).to_string();
            eprintln!("[Claude Code] Curl installer failed: {}", stderr);
        }
        Err(e) => {
            eprintln!("[Claude Code] Curl installer error: {}", e);
        }
    }

    // Fallback: npm install (for systems where curl installer doesn't work)
    eprintln!("[Claude Code] Falling back to npm install...");

    let npm_path = if std::path::Path::new("/opt/homebrew/bin/npm").exists() {
        "/opt/homebrew/bin/npm"
    } else if std::path::Path::new("/usr/local/bin/npm").exists() {
        "/usr/local/bin/npm"
    } else {
        "npm"
    };

    let shell_command = match method.as_str() {
        "brew" => {
            let brew_path = if std::path::Path::new("/opt/homebrew/bin/brew").exists() {
                "/opt/homebrew/bin/brew"
            } else if std::path::Path::new("/usr/local/bin/brew").exists() {
                "/usr/local/bin/brew"
            } else {
                "brew"
            };
            format!("{} install --cask claude-code", brew_path)
        }
        _ => format!("{} install -g @anthropic-ai/claude-code", npm_path),
    };

    let npm_output = login_shell_cmd(&shell_command).output();

    match npm_output {
        Ok(ref o) if o.status.success() => {
            eprintln!("[Claude Code] Installed successfully via fallback");
            return Ok(());
        }
        Ok(ref o) => {
            let stderr = String::from_utf8_lossy(&o.stderr).to_string();
            if stderr.contains("already installed") {
                return Ok(());
            }
            eprintln!("[Claude Code] Fallback install failed: {}", stderr);
        }
        Err(e) => {
            eprintln!("[Claude Code] Fallback install error: {}", e);
        }
    }

    // All automatic methods failed — open Terminal.app as last resort
    eprintln!("[Claude Code] Opening Terminal for installation...");

    let install_cmd = "curl -fsSL https://claude.ai/install.sh | bash";

    let script = format!(
        r#"
            clear
            echo "╔═══════════════════════════════════════════════════╗"
            echo "║  Operon — Installing Claude Code                  ║"
            echo "║                                                   ║"
            echo "║  When done, go back to Operon and click Re-check. ║"
            echo "╚═══════════════════════════════════════════════════╝"
            echo ""
            echo "▸ Installing Claude Code..."
            {}
            echo ""
            echo "✅ Done! Go back to Operon and click Re-check."
            echo ""
            echo "You can close this Terminal window."
        "#,
        install_cmd
    );

    let applescript = format!(
        r#"tell application "Terminal"
            activate
            do script "{}"
        end tell"#,
        script.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n")
    );

    let result = std::process::Command::new("osascript")
        .arg("-e")
        .arg(&applescript)
        .output()
        .map_err(|e| format!("Failed to open Terminal: {}", e))?;

    if !result.status.success() {
        // Fallback: write script to temp file and open in Terminal
        eprintln!("[Claude Code] osascript failed, trying fallback...");

        let script_path = "/tmp/operon_install_claude.sh";
        std::fs::write(script_path, format!("#!/bin/bash\n{}", script))
            .map_err(|e| format!("Failed to write install script: {}", e))?;

        let _ = std::process::Command::new("chmod")
            .args(["+x", script_path])
            .output();

        let _ = std::process::Command::new("open")
            .args(["-a", "Terminal", script_path])
            .output();
    }

    // Return OK — the frontend will poll via Re-check
    Ok(())
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
}

/// Check all local dependencies needed for Claude Code
#[tauri::command]
pub async fn check_local_dependencies() -> Result<DependencyStatus, String> {
    // Build an augmented PATH that includes Homebrew and Operon-managed Node locations.
    // This is necessary because after a fresh install, the GUI app's login shell
    // may not yet see the updated PATH.
    let operon_bin = operon_node_dir().join("bin").to_string_lossy().to_string();
    let extra_paths = format!("{}:/opt/homebrew/bin:/usr/local/bin", operon_bin);
    let current_path = std::env::var("PATH").unwrap_or_default();
    let augmented_path = format!("{}:{}", extra_paths, current_path);

    // Helper: run a command with augmented PATH via login shell
    let check_cmd = |cmd: &str| -> Option<std::process::Output> {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
        std::process::Command::new(&shell)
            .arg("-l")
            .arg("-c")
            .arg(cmd)
            .env("PATH", &augmented_path)
            .output()
            .ok()
    };

    // Check Xcode CLI tools
    let xcode = check_cmd("xcode-select -p")
        .map(|o| o.status.success())
        .unwrap_or(false);

    // Check Node.js — try login shell first, then check Homebrew paths directly
    let node_out = check_cmd("node --version");
    let mut node = node_out.as_ref().map_or(false, |o| o.status.success());
    let mut node_version = node_out
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());

    // Fallback: check Operon-managed and Homebrew node directly
    if !node {
        let operon_node = operon_node_dir().join("bin").join("node");
        let operon_node_str = operon_node.to_string_lossy().to_string();
        for node_path in &[operon_node_str.as_str(), "/opt/homebrew/bin/node", "/usr/local/bin/node"] {
            if std::path::Path::new(node_path).exists() {
                if let Ok(out) = std::process::Command::new(node_path).arg("--version").output() {
                    if out.status.success() {
                        node = true;
                        node_version = Some(String::from_utf8_lossy(&out.stdout).trim().to_string());
                        break;
                    }
                }
            }
        }
    }

    // Check npm
    let npm_out = check_cmd("npm --version");
    let mut npm = npm_out.as_ref().map_or(false, |o| o.status.success());
    let mut npm_version = npm_out
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());

    if !npm {
        let operon_npm = operon_node_dir().join("bin").join("npm");
        let operon_npm_str = operon_npm.to_string_lossy().to_string();
        for npm_path in &[operon_npm_str.as_str(), "/opt/homebrew/bin/npm", "/usr/local/bin/npm"] {
            if std::path::Path::new(npm_path).exists() {
                if let Ok(out) = std::process::Command::new(npm_path).arg("--version").output() {
                    if out.status.success() {
                        npm = true;
                        npm_version = Some(String::from_utf8_lossy(&out.stdout).trim().to_string());
                        break;
                    }
                }
            }
        }
    }

    // Check Claude Code
    let claude_out = check_cmd("claude --version");
    let claude_code = claude_out.as_ref().map_or(false, |o| o.status.success());
    let claude_version = claude_out
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());

    Ok(DependencyStatus {
        xcode_cli: xcode,
        node,
        node_version,
        npm,
        npm_version,
        claude_code,
        claude_version,
    })
}

/// Install Xcode CLI tools (triggers macOS native installer dialog)
#[tauri::command]
pub async fn install_xcode_cli() -> Result<(), String> {
    // First check if already installed
    let check = login_shell_cmd("xcode-select -p")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if check {
        return Ok(());
    }

    let output = std::process::Command::new("xcode-select")
        .arg("--install")
        .output()
        .map_err(|e| {
            format!("Could not launch Xcode CLI installer: {}. Please run 'xcode-select --install' in Terminal.", e)
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        // "already installed" is not a real error
        if stderr.contains("already installed") {
            return Ok(());
        }
        // "install requested" means the native dialog popped up — that's success
        if stderr.contains("install requested") {
            return Ok(());
        }
        return Err(format!("Failed to start Xcode CLI install: {}", stderr));
    }
    Ok(())
}

/// The Operon-managed Node.js installation directory.
/// We install Node here so no sudo/admin/Homebrew is ever needed.
fn operon_node_dir() -> std::path::PathBuf {
    dirs::home_dir().unwrap_or_default().join(".operon").join("node")
}

/// Get the path to the Operon-managed `node` binary (if it exists).
fn operon_node_bin() -> Option<String> {
    let bin = operon_node_dir().join("bin").join("node");
    if bin.exists() { Some(bin.to_string_lossy().to_string()) } else { None }
}

/// Get the path to the Operon-managed `npm` binary (if it exists).
fn operon_npm_bin() -> Option<String> {
    let bin = operon_node_dir().join("bin").join("npm");
    if bin.exists() { Some(bin.to_string_lossy().to_string()) } else { None }
}

/// Download a Node.js tar.gz, extract to ~/.operon/node/, and add to PATH.
/// Zero admin privileges needed — everything goes in the user's home directory.
fn install_node_tarball() -> Result<(), String> {
    let arch = if cfg!(target_arch = "aarch64") { "arm64" } else { "x64" };
    let node_version = "v22.14.0"; // LTS
    let tarball_url = format!(
        "https://nodejs.org/dist/{}/node-{}-darwin-{}.tar.gz",
        node_version, node_version, arch
    );

    let dest = operon_node_dir();
    let tmp_tar = "/tmp/operon_node.tar.gz";

    // Download
    eprintln!("[Node] Downloading {} ...", tarball_url);
    let dl = std::process::Command::new("curl")
        .args(["-fSL", "--progress-bar", "-o", tmp_tar, &tarball_url])
        .output()
        .map_err(|e| format!("curl failed: {}", e))?;

    if !dl.status.success() {
        let stderr = String::from_utf8_lossy(&dl.stderr);
        return Err(format!("Download failed: {}", stderr));
    }

    // Clean any previous install
    if dest.exists() {
        let _ = std::fs::remove_dir_all(&dest);
    }
    std::fs::create_dir_all(&dest)
        .map_err(|e| format!("Failed to create {}: {}", dest.display(), e))?;

    // Extract — the tarball has a top-level directory like node-v22.14.0-darwin-arm64/
    // We strip that with --strip-components=1 so files go directly into ~/.operon/node/
    eprintln!("[Node] Extracting to {} ...", dest.display());
    let extract = std::process::Command::new("tar")
        .args(["xzf", tmp_tar, "--strip-components=1", "-C"])
        .arg(&dest)
        .output()
        .map_err(|e| format!("tar failed: {}", e))?;

    if !extract.status.success() {
        let stderr = String::from_utf8_lossy(&extract.stderr);
        return Err(format!("Extract failed: {}", stderr));
    }

    // Clean up tarball
    let _ = std::fs::remove_file(tmp_tar);

    // Verify node binary works
    let node_bin = dest.join("bin").join("node");
    if !node_bin.exists() {
        return Err("Node binary not found after extraction".to_string());
    }

    let check = std::process::Command::new(&node_bin)
        .arg("--version")
        .output();

    match check {
        Ok(o) if o.status.success() => {
            let ver = String::from_utf8_lossy(&o.stdout);
            eprintln!("[Node] Installed: {}", ver.trim());
        }
        _ => {
            return Err("Node binary exists but won't run".to_string());
        }
    }

    // Add ~/.operon/node/bin to PATH in shell profile so it's found in future shells
    let home = dirs::home_dir().unwrap_or_default();
    let bin_dir = dest.join("bin");
    let path_line = format!("\nexport PATH=\"{}:$PATH\"\n", bin_dir.to_string_lossy());

    for profile_name in &[".zprofile", ".bash_profile"] {
        let profile_path = home.join(profile_name);
        if profile_path.exists() || *profile_name == ".zprofile" {
            if let Ok(existing) = std::fs::read_to_string(&profile_path) {
                if !existing.contains(".operon/node") {
                    let _ = std::fs::write(&profile_path, format!("{}{}", existing, path_line));
                }
            } else {
                let _ = std::fs::write(&profile_path, &path_line);
            }
            break; // Only write to first matching profile
        }
    }

    Ok(())
}

/// Install Node.js — uses Homebrew if available, otherwise extracts tarball to ~/.operon/node/
#[tauri::command]
pub async fn install_node() -> Result<(), String> {
    // Already installed?
    let has_node = login_shell_cmd("node --version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if has_node {
        return Ok(());
    }

    // Also check our own managed install
    if operon_node_bin().is_some() {
        return Ok(());
    }

    // Try Homebrew if it happens to be installed already
    let brew_path = if std::path::Path::new("/opt/homebrew/bin/brew").exists() {
        Some("/opt/homebrew/bin/brew")
    } else if std::path::Path::new("/usr/local/bin/brew").exists() {
        Some("/usr/local/bin/brew")
    } else {
        None
    };

    if let Some(brew) = brew_path {
        eprintln!("[Node] Trying Homebrew...");
        let output = login_shell_cmd(&format!("{} install node", brew)).output();
        if let Ok(o) = output {
            if o.status.success() { return Ok(()); }
        }
    }

    // Primary strategy: download tar.gz → extract to ~/.operon/node/ (zero sudo)
    install_node_tarball()
}

/// Silently install Homebrew by bypassing the official install script.
///
/// The official script always calls `have_sudo_access()` and aborts without it on macOS.
/// Instead, we do it ourselves:
///
///   Phase 1 (one macOS password dialog):
///     Use `osascript "with administrator privileges"` to create /opt/homebrew
///     with all subdirectories and chown to the current user.
///
///   Phase 2 (zero sudo — Homebrew is just a git repo):
///     `git clone --depth=1 https://github.com/Homebrew/brew /opt/homebrew/Homebrew`
///     Then symlink `bin/brew` and run `brew update --force --quiet`.
///
/// Returns Ok(path_to_brew) on success.
fn install_homebrew_silent() -> Result<String, String> {
    // Already installed?
    if std::path::Path::new("/opt/homebrew/bin/brew").exists() {
        return Ok("/opt/homebrew/bin/brew".to_string());
    }
    if std::path::Path::new("/usr/local/bin/brew").exists() {
        return Ok("/usr/local/bin/brew".to_string());
    }

    let is_arm = cfg!(target_arch = "aarch64");
    let prefix = if is_arm { "/opt/homebrew" } else { "/usr/local" };
    let _repo_dir = if is_arm { "/opt/homebrew" } else { "/usr/local/Homebrew" };

    // Get current username
    let current_user = std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .unwrap_or_else(|_| {
            String::from_utf8_lossy(
                &std::process::Command::new("id").arg("-un")
                    .output().map(|o| o.stdout).unwrap_or_default()
            ).trim().to_string()
        });

    eprintln!("[Homebrew] User: {}, Prefix: {}", current_user, prefix);

    // ── Phase 1: Create ALL directories Homebrew needs (one password dialog) ──
    let subdirs = [
        "bin", "etc", "include", "lib", "sbin", "share", "var", "opt",
        "Cellar", "Caskroom", "Frameworks",
        "etc/bash_completion.d",
        "lib/pkgconfig",
        "share/aclocal", "share/doc", "share/info", "share/locale", "share/man",
        "share/man/man1", "share/man/man2", "share/man/man3", "share/man/man4",
        "share/man/man5", "share/man/man6", "share/man/man7", "share/man/man8",
        "share/zsh", "share/zsh/site-functions",
        "var/homebrew", "var/homebrew/linked", "var/log",
    ];

    let mkdir_list: Vec<String> = subdirs.iter()
        .map(|s| format!("{}/{}", prefix, s))
        .collect();

    let admin_script = format!(
        "mkdir -p {} {} && chown -R {}:admin {} && chmod -R 755 {} && chmod go-w {}/share/zsh {}/share/zsh/site-functions",
        prefix,
        mkdir_list.join(" "),
        current_user, prefix, prefix,
        prefix, prefix,
    );

    let osascript_cmd = format!(
        r#"do shell script "{}" with administrator privileges"#,
        admin_script.replace('\\', "\\\\").replace('"', "\\\"")
    );

    eprintln!("[Homebrew] Phase 1: Creating directories with admin privileges...");
    let mkdir_result = std::process::Command::new("osascript")
        .arg("-e")
        .arg(&osascript_cmd)
        .output()
        .map_err(|e| format!("osascript failed: {}", e))?;

    if !mkdir_result.status.success() {
        let stderr = String::from_utf8_lossy(&mkdir_result.stderr);
        if stderr.contains("cancel") || stderr.contains("-128") {
            return Err("Password dialog was cancelled.".to_string());
        }
        return Err(format!("Failed to create Homebrew directories: {}", stderr));
    }
    eprintln!("[Homebrew] Phase 1 complete — directories owned by {}", current_user);

    // Ensure cache directory exists (user-writable, no sudo)
    let home = dirs::home_dir().unwrap_or_default();
    let _ = std::fs::create_dir_all(home.join("Library/Caches/Homebrew"));

    // ── Phase 2: Clone Homebrew repo (zero sudo) ──
    // Clone to a temp dir first, then merge into the prefix.
    // This avoids git clone failing because the prefix dir already has subdirs we created.
    eprintln!("[Homebrew] Phase 2: Cloning Homebrew repository...");

    let tmp_clone = format!("{}/homebrew-clone-tmp", std::env::temp_dir().display());
    // Clean up any leftover temp dir
    let _ = std::fs::remove_dir_all(&tmp_clone);

    let clone_result = std::process::Command::new("git")
        .args(["clone", "--depth=1", "https://github.com/Homebrew/brew", &tmp_clone])
        .output()
        .map_err(|e| format!("git clone failed: {}", e))?;

    if !clone_result.status.success() {
        let stderr = String::from_utf8_lossy(&clone_result.stderr);
        let _ = std::fs::remove_dir_all(&tmp_clone);
        return Err(format!("git clone failed: {}", stderr));
    }

    // Move clone contents into the prefix using rsync (preserves existing dirs)
    eprintln!("[Homebrew] Moving cloned files into {}...", prefix);
    let rsync_result = std::process::Command::new("rsync")
        .args(["-a", &format!("{}/", tmp_clone), &format!("{}/", prefix)])
        .output()
        .map_err(|e| format!("rsync failed: {}", e))?;

    if !rsync_result.status.success() {
        // Fallback: try cp -a
        eprintln!("[Homebrew] rsync failed, trying cp...");
        let _ = std::process::Command::new("/bin/bash")
            .args(["-c", &format!("cp -a {}/* {}/", tmp_clone, prefix)])
            .output();
        // Also copy hidden dirs like .git
        let _ = std::process::Command::new("/bin/bash")
            .args(["-c", &format!("cp -a {}/.[!.]* {}/", tmp_clone, prefix)])
            .output();
    }

    // Clean up temp dir
    let _ = std::fs::remove_dir_all(&tmp_clone);

    let brew_bin = format!("{}/bin/brew", prefix);
    eprintln!("[Homebrew] Checking for brew at: {}", brew_bin);
    if !std::path::Path::new(&brew_bin).exists() {
        // Debug: list what's in prefix/bin
        if let Ok(entries) = std::fs::read_dir(format!("{}/bin", prefix)) {
            let files: Vec<_> = entries.flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect();
            eprintln!("[Homebrew] Files in {}/bin/: {:?}", prefix, files);
        }
        return Err(format!("brew binary not found at {} after clone", brew_bin));
    }

    // Run `brew update --force --quiet` to set up taps and complete installation
    eprintln!("[Homebrew] Running brew update --force --quiet...");
    let _ = std::process::Command::new(&brew_bin)
        .args(["update", "--force", "--quiet"])
        .env("HOMEBREW_NO_ANALYTICS", "1")
        .env("HOMEBREW_NO_AUTO_UPDATE", "1")
        .output();

    // Add to shell profile
    let zprofile = home.join(".zprofile");
    let shellenv_line = format!("\neval \"$({} shellenv)\"\n", brew_bin);
    if let Ok(existing) = std::fs::read_to_string(&zprofile) {
        if !existing.contains("brew shellenv") {
            let _ = std::fs::write(&zprofile, format!("{}{}", existing, shellenv_line));
        }
    } else {
        let _ = std::fs::write(&zprofile, &shellenv_line);
    }

    eprintln!("[Homebrew] Installed at {}", brew_bin);
    Ok(brew_bin)
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
    pub step: String,       // e.g. "xcode", "homebrew", "node", "gh", "claude", "done"
    pub status: String,     // "starting", "downloading", "installing", "waiting", "complete", "skipped", "error"
    pub message: String,
    pub percent: u8,        // 0-100 within this phase
}

fn emit_install_progress(app: &tauri::AppHandle, step: &str, status: &str, message: &str, percent: u8) {
    use tauri::Emitter;
    let _ = app.emit("install-progress", InstallProgress {
        step: step.to_string(),
        status: status.to_string(),
        message: message.to_string(),
        percent,
    });
}

/// Phase 1: Xcode CLI Tools.
/// Triggers the macOS installer dialog and polls until it completes.
/// This can take 20-30 min on slow internet — the frontend should let
/// the user confirm when it's done rather than blocking.
#[tauri::command]
pub async fn install_phase_xcode(app: tauri::AppHandle) -> Result<bool, String> {
    let already = login_shell_cmd("xcode-select -p")
        .output().map(|o| o.status.success()).unwrap_or(false);

    if already {
        emit_install_progress(&app, "xcode", "skipped", "Xcode Command Line Tools already installed", 100);
        return Ok(true);
    }

    emit_install_progress(&app, "xcode", "starting", "Installing Xcode Command Line Tools...", 5);

    let _ = std::process::Command::new("xcode-select")
        .arg("--install")
        .output();

    emit_install_progress(&app, "xcode", "waiting",
        "A macOS dialog will appear — click Install and wait for it to finish.", 10);

    // Poll for up to 40 minutes (slow internet scenario)
    for i in 0..480_u32 {
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        let check = login_shell_cmd("xcode-select -p")
            .output().map(|o| o.status.success()).unwrap_or(false);
        if check {
            emit_install_progress(&app, "xcode", "complete", "Xcode Command Line Tools installed!", 100);
            return Ok(true);
        }
        let pct = 10 + std::cmp::min((i * 85 / 480) as u8, 85);
        emit_install_progress(&app, "xcode", "waiting", "Waiting for Xcode installer...", pct);
    }

    emit_install_progress(&app, "xcode", "error",
        "Xcode install timed out — it may still be running in the background.", 100);
    Ok(false)
}

/// Phase 2: Homebrew + Node.js + GitHub CLI.
/// Homebrew: pre-create /opt/homebrew with one admin dialog → git clone (no install script).
/// Node.js: `brew install node`, fallback to tar.gz in ~/.operon/node/.
/// GitHub CLI: `brew install gh`.
#[tauri::command]
pub async fn install_phase_tools(app: tauri::AppHandle) -> Result<bool, String> {
    let mut all_ok = true;

    // ── Homebrew (0-50%) ──
    let mut brew_path: Option<String> = if std::path::Path::new("/opt/homebrew/bin/brew").exists() {
        Some("/opt/homebrew/bin/brew".into())
    } else if std::path::Path::new("/usr/local/bin/brew").exists() {
        Some("/usr/local/bin/brew".into())
    } else {
        None
    };

    if brew_path.is_none() {
        emit_install_progress(&app, "homebrew", "installing",
            "Installing Homebrew (you'll be asked for your Mac password once)...", 5);

        match install_homebrew_silent() {
            Ok(path) => {
                brew_path = Some(path);
                emit_install_progress(&app, "homebrew", "complete", "Homebrew installed!", 45);
            }
            Err(e) => {
                eprintln!("[Homebrew] Install failed: {}", e);
                emit_install_progress(&app, "homebrew", "error",
                    &format!("Homebrew install failed: {}", e), 45);
                all_ok = false;
            }
        }
    } else {
        emit_install_progress(&app, "homebrew", "skipped", "Homebrew already installed", 45);
    }

    // ── Node.js (50-80%) ──
    let has_node = login_shell_cmd("node --version")
        .output().map(|o| o.status.success()).unwrap_or(false)
        || operon_node_bin().is_some();

    if !has_node {
        let mut node_installed = false;

        if let Some(brew) = &brew_path {
            emit_install_progress(&app, "node", "installing", "Installing Node.js via Homebrew...", 55);
            let output = std::process::Command::new(brew).args(["install", "node"]).output();
            if let Ok(o) = output {
                if o.status.success() { node_installed = true; }
                else {
                    let stderr = String::from_utf8_lossy(&o.stderr);
                    if stderr.contains("already installed") { node_installed = true; }
                    else { eprintln!("[Node] brew install node failed: {}", stderr); }
                }
            }
        }

        // Fallback: tar.gz to ~/.operon/node/ (zero sudo, no Homebrew needed)
        if !node_installed {
            emit_install_progress(&app, "node", "downloading", "Downloading Node.js (no admin needed)...", 55);
            match install_node_tarball() {
                Ok(()) => { node_installed = true; }
                Err(e) => { eprintln!("[Node] Tarball fallback failed: {}", e); }
            }
        }

        if node_installed {
            emit_install_progress(&app, "node", "complete", "Node.js installed!", 80);
        } else {
            emit_install_progress(&app, "node", "error",
                "Node.js could not be installed automatically.", 80);
            all_ok = false;
        }
    } else {
        let ver = login_shell_cmd("node --version").output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
        emit_install_progress(&app, "node", "skipped",
            &format!("Node.js already installed ({})", ver), 80);
    }

    // ── GitHub CLI (80-100%) ──
    let has_gh = login_shell_cmd("which gh").output()
        .map(|o| o.status.success()).unwrap_or(false);

    if !has_gh {
        if let Some(brew) = &brew_path {
            emit_install_progress(&app, "gh", "installing", "Installing GitHub CLI...", 85);
            let output = std::process::Command::new(brew).args(["install", "gh"]).output();
            if let Ok(o) = output {
                if o.status.success() {
                    emit_install_progress(&app, "gh", "complete", "GitHub CLI installed!", 100);
                } else {
                    let stderr = String::from_utf8_lossy(&o.stderr);
                    if stderr.contains("already installed") {
                        emit_install_progress(&app, "gh", "complete", "GitHub CLI already installed!", 100);
                    } else {
                        eprintln!("[gh] brew install gh failed: {}", stderr);
                        emit_install_progress(&app, "gh", "error",
                            "GitHub CLI could not be installed.", 100);
                        all_ok = false;
                    }
                }
            }
        } else {
            emit_install_progress(&app, "gh", "error",
                "Cannot install GitHub CLI — Homebrew is required.", 100);
            all_ok = false;
        }
    } else {
        emit_install_progress(&app, "gh", "skipped", "GitHub CLI already installed", 100);
    }

    emit_install_progress(&app, "done",
        if all_ok { "complete" } else { "error" },
        if all_ok { "All tools installed!" } else { "Some items need attention" },
        100);

    Ok(all_ok)
}

/// Phase 3: Claude Code.
/// Uses the official installer (curl -fsSL https://claude.ai/install.sh | bash).
/// Falls back to npm if curl installer fails.
#[tauri::command]
pub async fn install_phase_claude(app: tauri::AppHandle) -> Result<bool, String> {
    let has_claude = login_shell_cmd("which claude").output()
        .map(|o| o.status.success()).unwrap_or(false);

    if has_claude {
        let ver = login_shell_cmd("claude --version").output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
        emit_install_progress(&app, "claude", "skipped",
            &format!("Claude Code already installed ({})", ver), 100);
        return Ok(true);
    }

    // Method 1: Official Claude Code installer (recommended, no Node.js dependency)
    emit_install_progress(&app, "claude", "installing",
        "Installing Claude Code via official installer...", 20);
    eprintln!("[Claude] Attempting install via curl installer...");

    let curl_output = login_shell_cmd("curl -fsSL https://claude.ai/install.sh | bash").output();

    let mut claude_installed = false;

    match curl_output {
        Ok(o) if o.status.success() => {
            eprintln!("[Claude] Curl installer succeeded");
            // Source updated profile so `claude` is in PATH for subsequent checks
            let check = login_shell_cmd("claude --version").output();
            if let Ok(c) = check {
                if c.status.success() {
                    claude_installed = true;
                } else {
                    // Also check common install location directly
                    let home = dirs::home_dir().unwrap_or_default();
                    let claude_bin = home.join(".claude/local/bin/claude");
                    if claude_bin.exists() {
                        claude_installed = true;
                    }
                }
            }
        }
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr).to_string();
            eprintln!("[Claude] Curl installer failed (exit {}): {}", o.status, stderr);
        }
        Err(e) => {
            eprintln!("[Claude] Curl installer error: {}", e);
        }
    }

    // Method 2: npm fallback (if curl installer didn't work and npm is available)
    if !claude_installed {
        emit_install_progress(&app, "claude", "installing",
            "Curl installer didn't work, trying npm fallback...", 50);
        eprintln!("[Claude] Trying npm fallback...");

        let npm_cmd = operon_npm_bin()
            .or_else(|| {
                if std::path::Path::new("/opt/homebrew/bin/npm").exists() {
                    Some("/opt/homebrew/bin/npm".to_string())
                } else if std::path::Path::new("/usr/local/bin/npm").exists() {
                    Some("/usr/local/bin/npm".to_string())
                } else {
                    login_shell_cmd("which npm").output().ok()
                        .filter(|o| o.status.success())
                        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                }
            });

        if let Some(npm) = npm_cmd {
            eprintln!("[Claude] Using npm at: {}", npm);
            let install_cmd = format!("{} install -g @anthropic-ai/claude-code", npm);
            let output = login_shell_cmd(&install_cmd).output();

            match output {
                Ok(o) if o.status.success() => { claude_installed = true; }
                Ok(o) => {
                    let stderr = String::from_utf8_lossy(&o.stderr).to_string();
                    eprintln!("[Claude] npm install failed: {}", stderr);

                    // EACCES workaround for system npm
                    if stderr.contains("EACCES") || stderr.contains("permission") {
                        emit_install_progress(&app, "claude", "installing",
                            "Fixing npm permissions and retrying...", 70);

                        let home = dirs::home_dir().unwrap_or_default();
                        let npm_global = home.join(".npm-global");
                        let _ = std::fs::create_dir_all(&npm_global);
                        let _ = login_shell_cmd(&format!("{} config set prefix {}", npm,
                            npm_global.to_string_lossy())).output();

                        let zprofile = home.join(".zprofile");
                        let path_line = format!("\nexport PATH=\"{}:$PATH\"\n",
                            npm_global.join("bin").to_string_lossy());
                        if let Ok(existing) = std::fs::read_to_string(&zprofile) {
                            if !existing.contains(".npm-global") {
                                let _ = std::fs::write(&zprofile, format!("{}{}", existing, path_line));
                            }
                        } else {
                            let _ = std::fs::write(&zprofile, path_line);
                        }

                        let retry = login_shell_cmd(&format!(
                            "export PATH={}:$PATH && {} install -g @anthropic-ai/claude-code",
                            npm_global.join("bin").to_string_lossy(), npm
                        )).output();
                        if let Ok(r) = retry {
                            if r.status.success() { claude_installed = true; }
                        }
                    }
                }
                Err(e) => { eprintln!("[Claude] npm command failed: {}", e); }
            }
        } else {
            eprintln!("[Claude] npm not available for fallback");
        }
    }

    if claude_installed {
        emit_install_progress(&app, "claude", "complete", "Claude Code installed!", 100);
        Ok(true)
    } else {
        emit_install_progress(&app, "claude", "error",
            "Claude Code could not be installed automatically. Try running: curl -fsSL https://claude.ai/install.sh | bash", 100);
        Ok(false)
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
"#;

    let result = super::ssh::ssh_exec(&profile, check_script)
        .map_err(|e| format!("SSH check failed: {}", e))?;

    let node_line = result.lines().find(|l| l.starts_with("NODE:")).unwrap_or("NODE:MISSING");
    let npm_line = result.lines().find(|l| l.starts_with("NPM:")).unwrap_or("NPM:MISSING");
    let claude_line = result.lines().find(|l| l.starts_with("CLAUDE:")).unwrap_or("CLAUDE:MISSING");

    let node_ver = node_line.strip_prefix("NODE:").unwrap_or("MISSING");
    let npm_ver = npm_line.strip_prefix("NPM:").unwrap_or("MISSING");
    let claude_ver = claude_line.strip_prefix("CLAUDE:").unwrap_or("MISSING");

    Ok(DependencyStatus {
        xcode_cli: true, // Not applicable for remote
        node: node_ver != "MISSING",
        node_version: if node_ver != "MISSING" { Some(node_ver.to_string()) } else { None },
        npm: npm_ver != "MISSING",
        npm_version: if npm_ver != "MISSING" { Some(npm_ver.to_string()) } else { None },
        claude_code: claude_ver != "MISSING",
        claude_version: if claude_ver != "MISSING" && claude_ver != "FOUND" { Some(claude_ver.to_string()) } else { None },
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
        Ok(format!("not_authenticated:credentials_expired:{}", result.trim()))
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
        return Ok(());
    }

    // Provide a helpful error with manual install command
    return Err(format!(
        "Automatic installation failed on this server.\n\n\
         You can install manually by running this in the terminal:\n  \
         curl -fsSL https://claude.ai/install.sh | bash\n\n\
         Then click Re-check in Operon.\n\n\
         Server output:\n{}",
        result.lines().take(20).collect::<Vec<_>>().join("\n")
    ))
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
pub async fn get_api_key(
    state: tauri::State<'_, ClaudeManager>,
) -> Result<Option<String>, String> {
    let api_key = state.api_key.lock().map_err(|e| e.to_string())?;
    Ok(api_key.clone())
}

#[tauri::command]
pub async fn delete_api_key(
    state: tauri::State<'_, ClaudeManager>,
) -> Result<(), String> {
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
    if let Some(home) = dirs::home_dir() {
        let claude_dir = home.join(".claude");
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
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());

    let output = tokio::process::Command::new(&shell)
        .arg("-l")
        .arg("-c")
        .arg("claude -p 'ping' --max-turns 1 --output-format json 2>/dev/null")
        .output()
        .await
        .map_err(|e| e.to_string())?;

    // If claude exits 0 and produces output, auth is working
    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        if !stdout.trim().is_empty() {
            return Ok(true);
        }
    }

    Ok(false)
}

/// Open the system Terminal.app with `claude login` running in it.
/// Uses AppleScript on macOS for a native, reliable experience.
#[tauri::command]
pub async fn launch_claude_login() -> Result<String, String> {
    // Use osascript to open Terminal.app and run `claude login`
    let output = std::process::Command::new("osascript")
        .arg("-e")
        .arg(r#"tell application "Terminal"
    activate
    do script "claude login"
end tell"#)
        .output()
        .map_err(|e| format!("Failed to open Terminal: {}", e))?;

    if output.status.success() {
        Ok("Terminal opened — complete login there, then come back and click Verify.".to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        Err(format!("Failed to open Terminal: {}", stderr))
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

    // --- Check for existing plan files in the target directory ---
    // This gives Claude context about previous planning sessions in this folder.
    let existing_plan = if let Some(ref ctx) = remote {
        // Remote: read implementation_plan.md via SSH
        let profile = {
            let profiles = ssh_state.profiles.lock().map_err(|e| e.to_string())?;
            profiles
                .iter()
                .find(|p| p.id == ctx.profile_id)
                .cloned()
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

    // If there's an existing plan, prepend it as context for agent/ask modes
    let context_prefix = if !existing_plan.is_empty() && mode != "plan" {
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
            let days_in_year = if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 { 366 } else { 365 };
            if remaining < days_in_year { break; }
            remaining -= days_in_year;
            y += 1;
        }
        let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
        let month_days = [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
        let mut m = 0usize;
        for &md in &month_days {
            if remaining < md as i64 { break; }
            remaining -= md as i64;
            m += 1;
        }
        format!("{:04}-{:02}-{:02} {:02}:{:02} UTC", y, m + 1, remaining + 1, hours, minutes)
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
            let history_dir = std::path::Path::new(&project_path).join(".operon").join("plan_history");
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
                "You are in PLAN mode. Do NOT execute any code or make any changes. \
                 Instead, analyze the request and create a detailed implementation plan. \
                 Write the plan to a file called 'implementation_plan.md' in the current directory. \
                 This should be a FRESH, self-contained plan (the previous plan, if any, has been \
                 automatically archived to .operon/plan_history/).\
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
                 \n\nThe user's request: {}",
                now_timestamp,
                existing_plan_context,
                escaped_prompt
            );
            format!("claude --dangerously-skip-permissions -p '{}' --verbose --output-format stream-json", plan_prompt.replace('\'', "'\\''"))
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
            format!("claude --dangerously-skip-permissions -p '{}' --verbose --output-format stream-json --max-turns 1", ask_prompt.replace('\'', "'\\''"))
        }
        _ => {
            // Agent mode (default): full tool use
            // If there's a plan, tell Claude to follow it and update status
            let agent_prompt = if !existing_plan.is_empty() {
                format!(
                    "{}IMPORTANT: As you complete steps from the implementation plan, \
                     update implementation_plan.md to mark completed steps with [x] \
                     so progress is tracked.\n\n{}",
                    context_prefix,
                    escaped_prompt
                )
            } else {
                format!("{}{}", context_prefix, escaped_prompt)
            };
            format!("claude --dangerously-skip-permissions -p '{}' --verbose --output-format stream-json", agent_prompt.replace('\'', "'\\''"))
        }
    };

    if let Some(m) = &model {
        claude_cmd.push_str(&format!(" --model {}", m));
    }
    if mode == "plan" {
        claude_cmd.push_str(" --max-turns 3");
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

    // Inject --mcp-config if any MCP servers are enabled
    let mcp_servers = {
        let settings = settings_state.settings.lock().map_err(|e| e.to_string())?;
        settings.mcp_servers.clone()
    };
    if let Some(config_path) = super::mcp::generate_mcp_config(&mcp_servers)? {
        // Shell-escape the path in case it contains spaces
        claude_cmd.push_str(&format!(" --mcp-config '{}'", config_path.replace('\'', "'\\''")));
    }

    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());

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
            format!("{}...", &trimmed[..trimmed.char_indices().nth(50).map(|(i,_)|i).unwrap_or(trimmed.len())])
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
                let encoded_json = base64::engine::general_purpose::STANDARD.encode(mcp_json.as_bytes());
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
                        &format!("--mcp-config '{}'", mcp_config_remote.replace('\'', "'\\''")),
                    );
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
            let script_content = format!(
                "cd '{}' && {} > '{}' 2>&1; echo $? > '{}'",
                ctx.remote_path.replace('\'', "'\\''"),
                claude_cmd,
                output_file.replace('\'', "'\\''"),
                done_file.replace('\'', "'\\''"),
            );

            // Write the script file, source it, then clean up — all in one terminal command.
            // The leading space prevents it from appearing in shell history.
            let terminal_cmd = format!(
                " cat > '{}' << 'CFEOF'\n{}\nCFEOF\nclear; source '{}'; rm -f '{}'\n",
                script_file.replace('\'', "'\\''"),
                script_content,
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

            // Now tail the output file via a separate SSH connection to stream results back
            let mut ssh_tail_args = format!(
                "ssh -o BatchMode=yes -o ConnectTimeout=10 -o ServerAliveInterval=30 -o ServerAliveCountMax=6 {}@{} -p {}",
                profile.user, profile.host, profile.port
            );
            if let Some(key) = &profile.key_file {
                ssh_tail_args.push_str(&format!(" -i {}", key));
            }
            // Wait for the output file to appear, then tail -f it.
            // Use base64 encoding to completely avoid all shell quoting/expansion issues
            // across the local shell → SSH → remote shell → bash -c chain.
            let tail_script = format!(
                "i=0; while [ ! -f '{}' ] && [ \"$i\" -lt 150 ]; do sleep 0.2; i=$((i+1)); done; \
                 if [ ! -f '{}' ]; then exit 1; fi; \
                 tail -f '{}' & TAIL_PID=$!; \
                 while [ ! -f '{}' ]; do sleep 1; done; \
                 sleep 1; kill $TAIL_PID 2>/dev/null; wait $TAIL_PID 2>/dev/null; \
                 rm -f '{}' '{}'",
                output_file, output_file, output_file,
                done_file, output_file, done_file,
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
            if let Some(key) = &api_key {
                tail_cmd.env("ANTHROPIC_API_KEY", key);
            }
            tail_cmd.stdout(std::process::Stdio::piped());
            tail_cmd.stderr(std::process::Stdio::piped());

            let mut child = tail_cmd.spawn().map_err(|e| format!("Failed to start tail: {}", e))?;
            let stdout = child.stdout.take().ok_or("Failed to capture tail stdout")?;
            let stderr = child.stderr.take();

            // Store as a session so it can be stopped
            state.sessions.lock().map_err(|e| e.to_string())?
                .insert(session_id.clone(), ClaudeSession { child });

            // Stream stdout (JSON lines from the output file)
            let app_handle = app.clone();
            let sid = session_id.clone();
            tokio::spawn(async move {
                let reader = BufReader::new(stdout);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    if line.trim().is_empty() { continue; }
                    let _ = app_handle.emit(
                        &format!("claude-event-{}", sid),
                        serde_json::json!({ "line": line }),
                    );
                }
                let _ = app_handle.emit(
                    &format!("claude-done-{}", sid),
                    serde_json::json!({}),
                );
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
                            lt.is_empty() || lt.contains("WARNING") || lt.contains("Warning") ||
                            lt.contains("warning") || lt.contains("sntrup") || lt.contains("mlkem") ||
                            lt.contains("post-quantum") || lt.contains("quantum") ||
                            lt.contains("vulnerable") || lt.contains("decrypt later") ||
                            lt.contains("upgraded") || lt.contains("openssh.com") ||
                            lt.contains("store now") || lt.contains("key exchange") ||
                            lt.contains("no stdin data") || lt.contains("redirect stdin") ||
                            lt.contains("piping from") || lt.contains("/dev/null") ||
                            lt.contains("wait longer") || lt.contains("proceeding without") ||
                            lt.contains("Connection to") || lt.contains("Killed by signal") ||
                            lt.contains("Transferred:") || lt.contains("kex_exchange") ||
                            lt.contains("banner") || lt.starts_with("debug")
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
            return Err("Terminal mode requires a remote connection and an active terminal".to_string());
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
        let claude_resolve = super::ssh::ssh_exec(&profile, find_claude_cmd)
            .unwrap_or_default();
        let claude_resolve = claude_resolve.trim().to_string();

        if claude_resolve.is_empty() || claude_resolve.contains("not found") {
            return Err("Claude CLI not found on the remote server. \
                        Install it with: curl -fsSL https://claude.ai/install.sh | bash".to_string());
        }

        // Step 2: Replace `claude` with the resolved command
        // If it starts with "ALIAS:", it's a multi-word command (e.g. "npx @anthropic-ai/claude-code")
        // Otherwise it's an absolute binary path
        let claude_invoke = if let Some(alias_cmd) = claude_resolve.strip_prefix("ALIAS:") {
            alias_cmd.trim().to_string()
        } else {
            claude_resolve.clone()
        };

        let claude_cmd_abs = claude_cmd.replacen("claude ", &format!("{} ", claude_invoke), 1);

        // Step 3: Build the remote command — source profile for PATH (needed for npx/node)
        // then cd to the working directory and run claude
        let remote_cmd = format!(
            "export PS1=x; . \"$HOME/.profile\" 2>/dev/null; . \"$HOME/.bash_profile\" 2>/dev/null; . \"$HOME/.bashrc\" 2>/dev/null; . \"$HOME/.nvm/nvm.sh\" 2>/dev/null; cd '{}' && {} < /dev/null",
            ctx.remote_path.replace('\'', "'\\''"),
            claude_cmd_abs
        );

        // Base64-encode to avoid nested quoting issues
        let encoded_cmd = base64::engine::general_purpose::STANDARD.encode(remote_cmd.as_bytes());

        // No -tt flag! We need clean stdout for JSON parsing, not a PTY.
        let mut ssh_args = format!(
            "ssh -o BatchMode=yes -o ConnectTimeout=10 -o ServerAliveInterval=30 -o ServerAliveCountMax=6 {}@{} -p {}",
            profile.user, profile.host, profile.port
        );
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

    if let Some(key) = &api_key {
        cmd.env("ANTHROPIC_API_KEY", key);
    }

    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| format!("Failed to start Claude: {}", e))?;

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

    tokio::spawn(async move {
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();

        while let Ok(Some(line)) = lines.next_line().await {
            if line.trim().is_empty() {
                continue;
            }

            // Emit the raw JSON line to frontend for parsing
            let _ = app_handle.emit(
                &format!("claude-event-{}", sid),
                serde_json::json!({ "line": line }),
            );
        }

        // Stream ended
        let _ = app_handle.emit(
            &format!("claude-done-{}", sid),
            serde_json::json!({}),
        );
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
                    lt.is_empty() ||
                    lt.contains("WARNING") || lt.contains("Warning") || lt.contains("warning") ||
                    lt.contains("sntrup") || lt.contains("mlkem") ||
                    lt.contains("post-quantum") || lt.contains("quantum") ||
                    lt.contains("vulnerable") || lt.contains("decrypt later") ||
                    lt.contains("upgraded") || lt.contains("openssh.com") ||
                    lt.contains("store now") || lt.contains("key exchange") ||
                    lt.contains("no stdin data") || lt.contains("redirect stdin") ||
                    lt.contains("piping from") || lt.contains("/dev/null") ||
                    lt.contains("wait longer") || lt.contains("proceeding without")
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
pub async fn update_session_status(
    session_id: String,
    status: String,
) -> Result<(), String> {
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
            let path_match = project_path.as_ref().map_or(true, |p| {
                s.project_path == *p || s.remote_path.as_deref() == Some(p.as_str())
            });
            let profile_match = profile_id.as_ref().map_or(true, |pid| {
                s.profile_id.as_deref() == Some(pid.as_str())
            });
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
    session_id: String,           // The old session's ID (to find the files)
    event_session_id: String,     // The current frontend session ID (for event channels)
    remote: Option<RemoteContext>,
) -> Result<(), String> {
    let meta = load_session_from_disk(&session_id)?
        .ok_or_else(|| format!("Session {} not found", session_id))?;

    let base_path = meta.remote_path.as_deref().unwrap_or(&meta.project_path);
    let output_file = format!("{}/.operon-{}.jsonl", base_path, session_id);
    let done_file = format!("{}/.operon-{}.done", base_path, session_id);

    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());

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
            "ssh -o BatchMode=yes -o ConnectTimeout=10 -o ServerAliveInterval=30 -o ServerAliveCountMax=6 {}@{} -p {}",
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

        let mut child = tail_cmd.spawn().map_err(|e| format!("Failed to reconnect: {}", e))?;
        let stdout = child.stdout.take().ok_or("Failed to capture reconnect stdout")?;

        // Store as a session so it can be stopped
        state.sessions.lock().map_err(|e| e.to_string())?
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
            let _ = app_handle.emit(
                &format!("claude-done-{}", sid),
                serde_json::json!({}),
            );
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

/// Rename a session (update its human-readable name).
#[tauri::command]
pub async fn rename_session(
    session_id: String,
    name: String,
) -> Result<(), String> {
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
