use base64::Engine;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::Emitter;

use crate::commands::files::FileEntry;

// ── Profile Model ──

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AuthType {
    /// Simple password auth (no MFA)
    Password,
    /// Key-based auth (key already installed)
    Key,
    /// Keyboard-interactive / Duo MFA (password + push/passcode)
    DuoMfa,
}

impl Default for AuthType {
    fn default() -> Self {
        AuthType::Password
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SSHProfile {
    pub id: String,
    pub name: String,
    pub host: String,
    pub user: String,
    pub port: u16,
    pub key_file: Option<String>,
    pub use_agent: bool,
    /// What kind of auth this server uses
    #[serde(default)]
    pub auth_type: AuthType,
    /// For Duo MFA: preferred method ("push", "phone", "passcode")
    #[serde(default)]
    pub mfa_method: Option<String>,
    /// Whether to use ControlMaster multiplexing for this connection
    #[serde(default = "default_true")]
    pub use_control_master: bool,
    /// Server-level configuration: SLURM accounts, partitions, conda envs, etc.
    /// Keys are lowercase identifiers (e.g. "slurm_account", "gpu_partition").
    /// These are available to every protocol/script running on this server.
    #[serde(default)]
    pub server_config: HashMap<String, String>,
}

fn default_true() -> bool {
    true
}

// ── Persistence ──

/// Returns the path to ~/.operon/ssh_profiles.json
fn profiles_path() -> Result<std::path::PathBuf, String> {
    let home = dirs::home_dir().ok_or("Could not determine home directory")?;
    let dir = home.join(".operon");
    if !dir.exists() {
        std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create config dir: {}", e))?;
    }
    Ok(dir.join("ssh_profiles.json"))
}

fn load_profiles_from_disk() -> Vec<SSHProfile> {
    let path = match profiles_path() {
        Ok(p) => p,
        Err(_) => return Vec::new(),
    };
    if !path.exists() {
        return Vec::new();
    }
    let data = match std::fs::read_to_string(&path) {
        Ok(d) => d,
        Err(_) => return Vec::new(),
    };
    serde_json::from_str(&data).unwrap_or_default()
}

fn save_profiles_to_disk(profiles: &[SSHProfile]) -> Result<(), String> {
    let path = profiles_path()?;
    let json = serde_json::to_string_pretty(profiles)
        .map_err(|e| format!("Failed to serialize profiles: {}", e))?;
    std::fs::write(&path, json).map_err(|e| format!("Failed to write profiles: {}", e))?;
    Ok(())
}

// ── ControlMaster Helpers ──

/// Returns the ControlMaster socket path for a given profile.
/// Socket is at ~/.operon/sockets/ctrl_%h_%p_%r
fn control_socket_path(profile: &SSHProfile) -> String {
    let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("/tmp"));
    let sock_dir = home.join(".operon").join("sockets");
    // Ensure socket directory exists
    let _ = std::fs::create_dir_all(&sock_dir);
    let sock = sock_dir.join(format!("ctrl_{}_{}_{}", profile.host, profile.port, profile.user));
    sock.to_string_lossy().to_string()
}

/// Check if a ControlMaster socket is active for this profile.
fn control_master_active(profile: &SSHProfile) -> bool {
    let sock = control_socket_path(profile);
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
    let check_cmd = format!(
        "ssh -o ControlPath={} -O check {}@{} -p {} 2>/dev/null",
        sock, profile.user, profile.host, profile.port
    );
    let output = std::process::Command::new(&shell)
        .arg("-l").arg("-c").arg(&check_cmd)
        .output();
    matches!(output, Ok(o) if o.status.success())
}

/// Build common SSH args including ControlMaster and ControlPath.
/// If `as_master` is true, starts a new ControlMaster. Otherwise, reuses existing.
fn control_master_args(profile: &SSHProfile, as_master: bool) -> String {
    if !profile.use_control_master {
        return String::new();
    }
    let sock = control_socket_path(profile);
    if as_master {
        format!(
            " -o ControlMaster=auto -o ControlPath={} -o ControlPersist=4h",
            sock
        )
    } else {
        format!(
            " -o ControlMaster=auto -o ControlPath={} -o ControlPersist=4h",
            sock
        )
    }
}

// ── Manager State ──

pub struct SSHManager {
    pub profiles: Mutex<Vec<SSHProfile>>,
    pub active_connections: Mutex<HashMap<String, String>>, // profile_id -> terminal_id
}

impl SSHManager {
    pub fn new() -> Self {
        let profiles = load_profiles_from_disk();
        // Ensure socket directory exists at startup
        if let Some(home) = dirs::home_dir() {
            let _ = std::fs::create_dir_all(home.join(".operon").join("sockets"));
        }
        Self {
            profiles: Mutex::new(profiles),
            active_connections: Mutex::new(HashMap::new()),
        }
    }
}

// ── Profile CRUD Commands ──

#[tauri::command]
pub async fn save_ssh_profile(
    state: tauri::State<'_, SSHManager>,
    profile: SSHProfile,
) -> Result<(), String> {
    let mut profiles = state.profiles.lock().map_err(|e| e.to_string())?;
    if let Some(existing) = profiles.iter_mut().find(|p| p.id == profile.id) {
        *existing = profile;
    } else {
        profiles.push(profile);
    }
    save_profiles_to_disk(&profiles)?;
    Ok(())
}

#[tauri::command]
pub async fn list_ssh_profiles(
    state: tauri::State<'_, SSHManager>,
) -> Result<Vec<SSHProfile>, String> {
    let profiles = state.profiles.lock().map_err(|e| e.to_string())?;
    Ok(profiles.clone())
}

/// Get server configuration for a specific profile.
/// Returns the server_config HashMap which protocols/chat can use
/// to inject SLURM accounts, conda envs, paths, etc. into scripts.
#[tauri::command]
pub async fn get_server_config(
    state: tauri::State<'_, SSHManager>,
    profile_id: String,
) -> Result<HashMap<String, String>, String> {
    let profiles = state.profiles.lock().map_err(|e| e.to_string())?;
    let profile = profiles
        .iter()
        .find(|p| p.id == profile_id)
        .ok_or_else(|| format!("Profile {} not found", profile_id))?;
    Ok(profile.server_config.clone())
}

#[tauri::command]
pub async fn delete_ssh_profile(
    state: tauri::State<'_, SSHManager>,
    profile_id: String,
) -> Result<(), String> {
    let mut profiles = state.profiles.lock().map_err(|e| e.to_string())?;
    profiles.retain(|p| p.id != profile_id);
    save_profiles_to_disk(&profiles)?;
    Ok(())
}

// ── SSH Terminal Spawning ──

#[tauri::command]
pub async fn spawn_ssh_terminal(
    terminal_state: tauri::State<'_, crate::commands::terminal::TerminalManager>,
    ssh_state: tauri::State<'_, SSHManager>,
    app: tauri::AppHandle,
    terminal_id: String,
    profile_id: String,
) -> Result<(), String> {
    use portable_pty::{native_pty_system, CommandBuilder, PtySize};
    use crate::commands::terminal::TerminalHandle;
    use std::io::Read;
    use std::sync::Arc;

    let profile = {
        let profiles = ssh_state.profiles.lock().map_err(|e| e.to_string())?;
        profiles
            .iter()
            .find(|p| p.id == profile_id)
            .cloned()
            .ok_or_else(|| format!("SSH profile {} not found", profile_id))?
    };

    let pty_system = native_pty_system();
    let size = PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
    };

    let pair = pty_system.openpty(size).map_err(|e| e.to_string())?;

    // Build the SSH command with ControlMaster support
    let mut ssh_cmd = format!("ssh {}@{} -p {}", profile.user, profile.host, profile.port);
    ssh_cmd.push_str(" -o ServerAliveInterval=30");
    // Add ControlMaster args — the first terminal becomes the master,
    // subsequent ones multiplex through it (no re-auth / no Duo)
    ssh_cmd.push_str(&control_master_args(&profile, true));
    if let Some(key) = &profile.key_file {
        ssh_cmd.push_str(&format!(" -i {}", key));
    }

    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
    let mut cmd = CommandBuilder::new(&shell);
    cmd.arg("-l");
    cmd.arg("-c");
    cmd.arg(&ssh_cmd);
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");

    if let Some(home) = dirs::home_dir() {
        cmd.env("HOME", home.to_string_lossy().as_ref());
        cmd.cwd(&home);
    }

    let child = pair.slave.spawn_command(cmd).map_err(|e| e.to_string())?;

    let reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
    let writer = pair.master.take_writer().map_err(|e| e.to_string())?;

    // Drop slave so the PTY reader gets EOF when child exits
    drop(pair.slave);

    let handle = TerminalHandle {
        id: terminal_id.clone(),
        master: Arc::new(std::sync::Mutex::new(pair.master)),
        writer: Arc::new(std::sync::Mutex::new(writer)),
        child: Arc::new(std::sync::Mutex::new(child)),
    };

    terminal_state
        .terminals
        .lock()
        .map_err(|e| e.to_string())?
        .insert(terminal_id.clone(), handle);

    // Track active connection
    ssh_state
        .active_connections
        .lock()
        .map_err(|e| e.to_string())?
        .insert(profile_id, terminal_id.clone());

    // Spawn reader thread
    let event_name = format!("pty-output-{}", terminal_id);
    let app_handle = app.clone();
    let tid = terminal_id.clone();

    std::thread::spawn(move || {
        let mut reader = reader;
        let mut buf = vec![0u8; 8192];

        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let output = String::from_utf8_lossy(&buf[..n]).to_string();
                    let _ = app_handle.emit(
                        &event_name,
                        serde_json::json!({ "output": output }),
                    );
                }
                Err(_) => break,
            }
        }

        let _ = app_handle.emit(
            &format!("pty-exit-{}", tid),
            serde_json::json!({}),
        );
    });

    Ok(())
}

// ── Remote Command Execution (uses ControlMaster when available) ──

/// Run a command on a remote server via a quick non-interactive SSH subprocess.
/// Automatically uses ControlMaster socket if one is active, bypassing re-auth.
pub(crate) fn ssh_exec(profile: &SSHProfile, remote_cmd: &str) -> Result<String, String> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());

    let mut ssh_args = format!(
        "ssh -o BatchMode=yes -o ConnectTimeout=5 -o ServerAliveInterval=30 {}@{} -p {}",
        profile.user, profile.host, profile.port
    );
    // Always include ControlPath so we ride the existing master connection
    ssh_args.push_str(&control_master_args(profile, false));
    if let Some(key) = &profile.key_file {
        if std::path::Path::new(key).exists() {
            ssh_args.push_str(&format!(" -i {}", key));
        }
    }
    ssh_args.push_str(&format!(" -- {}", shell_escape(remote_cmd)));

    let output = std::process::Command::new(&shell)
        .arg("-l")
        .arg("-c")
        .arg(&ssh_args)
        .output()
        .map_err(|e| format!("Failed to run SSH: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();

    if !output.status.success() && stdout.trim().is_empty() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("SSH command failed: {}", stderr));
    }

    Ok(stdout)
}

fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

fn shell_escape_inner(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\\\""))
}

// ── Remote File Operations ──

#[tauri::command]
pub async fn list_remote_directory(
    state: tauri::State<'_, SSHManager>,
    profile_id: String,
    path: String,
    show_hidden: Option<bool>,
) -> Result<Vec<FileEntry>, String> {
    let show_hidden = show_hidden.unwrap_or(false);
    let profile = {
        let profiles = state.profiles.lock().map_err(|e| e.to_string())?;
        profiles
            .iter()
            .find(|p| p.id == profile_id)
            .cloned()
            .ok_or_else(|| format!("SSH profile {} not found", profile_id))?
    };

    let ls_flag = if show_hidden { "-1aFL" } else { "-1FL" };
    let la_flag = if show_hidden { "-laL" } else { "-lL" };
    let cmd = format!(
        "ls {} {} 2>/dev/null && echo '---SEPARATOR---' && ls {} {} 2>/dev/null",
        ls_flag, shell_escape_inner(&path),
        la_flag, shell_escape_inner(&path)
    );

    let output = ssh_exec(&profile, &cmd)?;

    let parts: Vec<&str> = output.splitn(2, "---SEPARATOR---").collect();
    let names_output = parts.first().unwrap_or(&"");
    let long_output = parts.get(1).unwrap_or(&"");

    let mut size_map: HashMap<String, u64> = HashMap::new();
    for line in long_output.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() >= 9 {
            if let Ok(size) = fields[4].parse::<u64>() {
                let name = fields[8..].join(" ");
                size_map.insert(name, size);
            }
        }
    }

    let mut entries: Vec<FileEntry> = Vec::new();
    let base_path = if path.ends_with('/') {
        path.clone()
    } else {
        format!("{}/", path)
    };

    for line in names_output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let clean = line.trim_end_matches(|c| c == '/' || c == '*' || c == '@' || c == '=' || c == '|');
        if clean == "." || clean == ".." {
            continue;
        }

        let is_dir = line.ends_with('/');
        let name = clean.to_string();
        let full_path = format!("{}{}", base_path, name);

        let extension = if !is_dir {
            name.rsplit('.')
                .next()
                .and_then(|e| if e != name { Some(e.to_string()) } else { None })
        } else {
            None
        };

        let size = size_map.get(&name).copied().unwrap_or(0);

        entries.push(FileEntry {
            name,
            path: full_path,
            is_dir,
            size,
            extension,
        });
    }

    entries.sort_by(|a, b| {
        b.is_dir.cmp(&a.is_dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    Ok(entries)
}

#[tauri::command]
pub async fn get_remote_home(
    state: tauri::State<'_, SSHManager>,
    profile_id: String,
) -> Result<String, String> {
    let profile = {
        let profiles = state.profiles.lock().map_err(|e| e.to_string())?;
        profiles
            .iter()
            .find(|p| p.id == profile_id)
            .cloned()
            .ok_or_else(|| format!("SSH profile {} not found", profile_id))?
    };

    let output = ssh_exec(&profile, "echo $HOME")?;
    Ok(output.trim().to_string())
}

#[tauri::command]
pub async fn read_remote_file(
    state: tauri::State<'_, SSHManager>,
    profile_id: String,
    path: String,
) -> Result<String, String> {
    let profile = {
        let profiles = state.profiles.lock().map_err(|e| e.to_string())?;
        profiles
            .iter()
            .find(|p| p.id == profile_id)
            .cloned()
            .ok_or_else(|| format!("SSH profile {} not found", profile_id))?
    };

    ssh_exec(&profile, &format!("cat {}", shell_escape_inner(&path)))
}

#[tauri::command]
pub async fn read_remote_file_base64(
    state: tauri::State<'_, SSHManager>,
    profile_id: String,
    path: String,
) -> Result<String, String> {
    let profile = {
        let profiles = state.profiles.lock().map_err(|e| e.to_string())?;
        profiles
            .iter()
            .find(|p| p.id == profile_id)
            .cloned()
            .ok_or_else(|| format!("SSH profile {} not found", profile_id))?
    };

    let output = ssh_exec(&profile, &format!("base64 {}", shell_escape_inner(&path)))?;
    Ok(output.chars().filter(|c| !c.is_whitespace()).collect())
}

/// Create a directory on a remote server via SSH
#[tauri::command]
pub async fn create_remote_directory(
    state: tauri::State<'_, SSHManager>,
    profile_id: String,
    path: String,
) -> Result<(), String> {
    let profile = {
        let profiles = state.profiles.lock().map_err(|e| e.to_string())?;
        profiles
            .iter()
            .find(|p| p.id == profile_id)
            .cloned()
            .ok_or_else(|| format!("SSH profile {} not found", profile_id))?
    };

    let cmd = format!("mkdir -p {}", shell_escape_inner(&path));
    ssh_exec(&profile, &cmd)?;
    Ok(())
}

/// Write a file to the remote server via SSH.
/// For text files, pipes content through base64 to avoid quoting issues.
/// For binary files (like PDFs), use scp_to_remote instead.
#[tauri::command]
pub async fn write_remote_file(
    state: tauri::State<'_, SSHManager>,
    profile_id: String,
    path: String,
    content: String,
) -> Result<(), String> {
    let profile = {
        let profiles = state.profiles.lock().map_err(|e| e.to_string())?;
        profiles
            .iter()
            .find(|p| p.id == profile_id)
            .cloned()
            .ok_or_else(|| format!("SSH profile {} not found", profile_id))?
    };

    // Ensure parent directory exists
    if let Some(parent) = std::path::Path::new(&path).parent() {
        let mkdir_cmd = format!("mkdir -p {}", shell_escape_inner(&parent.to_string_lossy()));
        let _ = ssh_exec(&profile, &mkdir_cmd);
    }

    // Encode content as base64 and decode on the remote side to avoid quoting issues
    let b64 = base64::engine::general_purpose::STANDARD.encode(content.as_bytes());
    let cmd = format!("echo {} | base64 -d > {}", b64, shell_escape_inner(&path));
    ssh_exec(&profile, &cmd)?;
    Ok(())
}

/// Copy a local file to the remote server via SCP.
/// Uses ControlMaster socket if available.
#[tauri::command]
pub async fn scp_to_remote(
    state: tauri::State<'_, SSHManager>,
    profile_id: String,
    local_path: String,
    remote_path: String,
) -> Result<(), String> {
    let profile = {
        let profiles = state.profiles.lock().map_err(|e| e.to_string())?;
        profiles
            .iter()
            .find(|p| p.id == profile_id)
            .cloned()
            .ok_or_else(|| format!("SSH profile {} not found", profile_id))?
    };

    // Ensure remote parent directory exists
    if let Some(parent) = std::path::Path::new(&remote_path).parent() {
        let mkdir_cmd = format!("mkdir -p {}", shell_escape_inner(&parent.to_string_lossy()));
        let _ = ssh_exec(&profile, &mkdir_cmd);
    }

    let host_str = format!("{}@{}", profile.user, profile.host);
    let mut scp_args: Vec<String> = vec![
        "-o".to_string(), "BatchMode=yes".to_string(),
        "-o".to_string(), "ConnectTimeout=10".to_string(),
    ];

    // Use ControlMaster socket if available
    let ctrl_dir = std::env::temp_dir().join("operon-ssh");
    let sock = ctrl_dir.join(format!("{}_{}_{}", profile.user, profile.host, profile.port));
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

    scp_args.push(local_path);
    scp_args.push(format!("{}:{}", host_str, remote_path));

    let output = std::process::Command::new("scp")
        .args(&scp_args)
        .output()
        .map_err(|e| format!("Failed to run scp: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("SCP failed: {}", stderr));
    }

    Ok(())
}

// ── SSH Key Setup: PTY-Based with Duo/MFA Support ──
//
// Instead of using the ssh2 crate (which only supports simple password auth),
// we spawn a real `ssh` process in a PTY and drive it with a state machine that
// handles:
//   1. Simple password-only servers  (password prompt → done)
//   2. Duo MFA servers               (password prompt → Duo prompt → approval → done)
//
// Once the key is installed, all future connections use key auth and skip MFA entirely.

/// Progress events emitted during key setup so the frontend can show status.
#[derive(Debug, Clone, Serialize)]
pub struct KeySetupProgress {
    pub stage: String,   // "connecting", "password", "mfa_waiting", "installing", "verifying", "done", "error"
    pub message: String,
}

/// Generate an SSH key pair, connect to remote via PTY (handling password + optional Duo MFA),
/// install the public key, and update the profile. Returns the key file path on success.
///
/// Emits `ssh-key-setup-progress-{profile_id}` events for frontend status updates.
#[tauri::command]
pub async fn setup_ssh_key(
    state: tauri::State<'_, SSHManager>,
    app: tauri::AppHandle,
    profile_id: String,
    password: String,
    mfa_method: Option<String>, // "push" (default), "phone", "passcode", or a specific passcode
) -> Result<String, String> {
    use portable_pty::{native_pty_system, CommandBuilder, PtySize};
    use std::io::{Read, Write};

    let profile = {
        let profiles = state.profiles.lock().map_err(|e| e.to_string())?;
        profiles
            .iter()
            .find(|p| p.id == profile_id)
            .cloned()
            .ok_or_else(|| format!("SSH profile {} not found", profile_id))?
    };

    let event_name = format!("ssh-key-setup-progress-{}", profile_id);
    let emit_progress = |app: &tauri::AppHandle, stage: &str, msg: &str| {
        let _ = app.emit(
            &event_name,
            KeySetupProgress {
                stage: stage.to_string(),
                message: msg.to_string(),
            },
        );
    };

    emit_progress(&app, "connecting", "Generating SSH key...");

    // 1. Generate SSH key pair locally
    let home = dirs::home_dir().ok_or("Could not determine home directory")?;
    let ssh_dir = home.join(".ssh");
    if !ssh_dir.exists() {
        std::fs::create_dir_all(&ssh_dir).map_err(|e| format!("Failed to create .ssh dir: {}", e))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&ssh_dir, std::fs::Permissions::from_mode(0o700))
                .map_err(|e| format!("Failed to set .ssh permissions: {}", e))?;
        }
    }

    let safe_host = profile.host.replace('.', "_").replace(':', "_");
    let key_name = format!("operon_{}", safe_host);
    let private_key_path = ssh_dir.join(&key_name);
    let public_key_path = ssh_dir.join(format!("{}.pub", key_name));

    if !private_key_path.exists() {
        let output = std::process::Command::new("ssh-keygen")
            .args([
                "-t", "ed25519",
                "-f", &private_key_path.to_string_lossy(),
                "-N", "",
                "-C", &format!("operon@{}", profile.host),
            ])
            .output()
            .map_err(|e| format!("Failed to run ssh-keygen: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("ssh-keygen failed: {}", stderr));
        }
    }

    let pub_key = std::fs::read_to_string(&public_key_path)
        .map_err(|e| format!("Failed to read public key: {}", e))?;
    let pub_key = pub_key.trim().to_string();

    // Cleanup helper — remove generated keys if setup fails
    let cleanup_keys = |priv_path: &std::path::Path, pub_path: &std::path::Path| {
        let _ = std::fs::remove_file(priv_path);
        let _ = std::fs::remove_file(pub_path);
    };

    // 2. Connect via PTY-based SSH and handle password + MFA
    emit_progress(&app, "connecting", &format!("Connecting to {}...", profile.host));

    let pty_system = native_pty_system();
    let size = PtySize { rows: 24, cols: 80, pixel_width: 0, pixel_height: 0 };
    let pair = pty_system.openpty(size).map_err(|e| e.to_string())?;

    // Build SSH command that will install the key after login
    // We use a single-shot command: login, install key, exit
    let install_script = format!(
        "mkdir -p ~/.ssh && chmod 700 ~/.ssh && \
         touch ~/.ssh/authorized_keys && chmod 600 ~/.ssh/authorized_keys && \
         grep -qxF '{}' ~/.ssh/authorized_keys 2>/dev/null || echo '{}' >> ~/.ssh/authorized_keys && \
         echo 'OPERON_KEY_INSTALLED_OK'",
        pub_key, pub_key
    );

    let ssh_cmd = format!(
        "ssh -o StrictHostKeyChecking=accept-new -o ConnectTimeout=15 -p {} {}@{} {}",
        profile.port, profile.user, profile.host, shell_escape(&install_script)
    );

    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
    let mut cmd = CommandBuilder::new(&shell);
    cmd.arg("-l");
    cmd.arg("-c");
    cmd.arg(&ssh_cmd);
    cmd.env("TERM", "xterm-256color");
    if let Some(h) = dirs::home_dir() {
        cmd.env("HOME", h.to_string_lossy().as_ref());
        cmd.cwd(&h);
    }

    let child = pair.slave.spawn_command(cmd).map_err(|e| {
        cleanup_keys(&private_key_path, &public_key_path);
        format!("Failed to spawn SSH process: {}", e)
    })?;
    drop(pair.slave);

    let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
    let mut writer = pair.master.take_writer().map_err(|e| e.to_string())?;

    // 3. State machine: read PTY output and respond to prompts
    #[derive(Debug, PartialEq)]
    enum State {
        WaitingForPrompt,    // Waiting for password or any prompt
        WaitingForDuo,       // Password was sent, looking for Duo prompt
        WaitingForApproval,  // Duo push sent, waiting for approval
        WaitingForResult,    // Authenticated, waiting for key install confirmation
        Done,
        Failed,
    }

    let mut state_machine = State::WaitingForPrompt;
    let mut accumulated = String::new();
    let mut buf = vec![0u8; 4096];
    let mut password_sent = false;
    let mut duo_responded = false;
    let timeout = std::time::Instant::now();
    let max_wait = std::time::Duration::from_secs(120); // 2 min for Duo approval

    // Set a short read timeout so we can poll without blocking forever
    // (portable-pty doesn't support non-blocking reads directly, so we use
    //  a thread with a channel)
    let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
    let reader_thread = std::thread::spawn(move || {
        loop {
            match reader.read(&mut buf) {
                Ok(0) => { let _ = tx.send(Vec::new()); break; }
                Ok(n) => { let _ = tx.send(buf[..n].to_vec()); }
                Err(_) => { let _ = tx.send(Vec::new()); break; }
            }
        }
    });

    loop {
        if timeout.elapsed() > max_wait {
            cleanup_keys(&private_key_path, &public_key_path);
            emit_progress(&app, "error", "Timed out waiting for authentication");
            return Err("Timed out waiting for authentication (120s). If using Duo, make sure to approve the push.".to_string());
        }

        // Try to read with a short timeout
        match rx.recv_timeout(std::time::Duration::from_millis(200)) {
            Ok(data) => {
                if data.is_empty() {
                    // EOF — process exited
                    if state_machine != State::Done {
                        // Check if we got the success marker before EOF
                        if accumulated.contains("OPERON_KEY_INSTALLED_OK") {
                            state_machine = State::Done;
                        } else {
                            state_machine = State::Failed;
                        }
                    }
                    break;
                }
                let text = String::from_utf8_lossy(&data).to_string();
                accumulated.push_str(&text);
                let lower = accumulated.to_lowercase();

                match state_machine {
                    State::WaitingForPrompt => {
                        // Look for password prompt
                        if !password_sent && (
                            lower.contains("password:") ||
                            lower.contains("password for") ||
                            lower.ends_with("'s password: ") ||
                            // keyboard-interactive prompt
                            lower.contains("(current) password") ||
                            lower.contains("verification code")
                        ) {
                            emit_progress(&app, "password", "Sending password...");
                            let _ = writer.write_all(format!("{}\n", password).as_bytes());
                            let _ = writer.flush();
                            password_sent = true;
                            accumulated.clear();
                            state_machine = State::WaitingForDuo;
                        }
                        // Some servers show "Permission denied" immediately
                        if lower.contains("permission denied") {
                            cleanup_keys(&private_key_path, &public_key_path);
                            emit_progress(&app, "error", "Permission denied — wrong password");
                            return Err("Permission denied — check your password".to_string());
                        }
                        // Connection refused / timeout
                        if lower.contains("connection refused") || lower.contains("no route to host") || lower.contains("connection timed out") {
                            cleanup_keys(&private_key_path, &public_key_path);
                            let msg = format!("Could not connect to {}", profile.host);
                            emit_progress(&app, "error", &msg);
                            return Err(msg);
                        }
                    }
                    State::WaitingForDuo => {
                        // Check for Duo MFA prompt
                        if !duo_responded && (
                            lower.contains("duo two-factor") ||
                            lower.contains("duo login") ||
                            lower.contains("passcode or option") ||
                            lower.contains("1. duo push") ||
                            lower.contains("enter a passcode")
                        ) {
                            // Duo detected! Respond based on preferred method
                            let mfa_response = match mfa_method.as_deref() {
                                Some("phone") | Some("2") => "2",
                                Some("passcode") => {
                                    // If mfa_method is "passcode", we can't proceed without the actual code
                                    // The user should pass the actual passcode as mfa_method
                                    "1" // fallback to push
                                }
                                Some(code) if code.chars().all(|c| c.is_ascii_digit()) && code.len() >= 6 => {
                                    // User passed an actual passcode
                                    code
                                }
                                _ => "1", // Default: Duo Push
                            };

                            if mfa_response == "1" {
                                emit_progress(&app, "mfa_waiting", "Duo push sent — approve on your phone...");
                            } else if mfa_response == "2" {
                                emit_progress(&app, "mfa_waiting", "Calling your phone for Duo approval...");
                            } else {
                                emit_progress(&app, "mfa_waiting", "Sending Duo passcode...");
                            }

                            let _ = writer.write_all(format!("{}\n", mfa_response).as_bytes());
                            let _ = writer.flush();
                            duo_responded = true;
                            accumulated.clear();
                            state_machine = State::WaitingForApproval;
                        }
                        // No Duo prompt — might be simple password auth, check if we're in
                        else if lower.contains("operon_key_installed_ok") {
                            state_machine = State::Done;
                        }
                        // Or we got another password prompt (wrong password)
                        else if lower.contains("permission denied") || (password_sent && lower.contains("password:")) {
                            cleanup_keys(&private_key_path, &public_key_path);
                            emit_progress(&app, "error", "Authentication failed — wrong password");
                            return Err("Authentication failed — wrong password or MFA rejected".to_string());
                        }
                        // Might already be logged in (fast password-only servers)
                        else if lower.contains("last login") || lower.contains("welcome") {
                            emit_progress(&app, "installing", "Authenticated. Installing SSH key...");
                            state_machine = State::WaitingForResult;
                        }
                    }
                    State::WaitingForApproval => {
                        if lower.contains("success") || lower.contains("operon_key_installed_ok") || lower.contains("last login") {
                            if lower.contains("operon_key_installed_ok") {
                                state_machine = State::Done;
                            } else {
                                emit_progress(&app, "installing", "MFA approved. Installing SSH key...");
                                state_machine = State::WaitingForResult;
                            }
                        }
                        if lower.contains("denied") || lower.contains("timed out") || lower.contains("error") {
                            cleanup_keys(&private_key_path, &public_key_path);
                            emit_progress(&app, "error", "Duo authentication denied or timed out");
                            return Err("Duo MFA denied or timed out. Please try again.".to_string());
                        }
                    }
                    State::WaitingForResult => {
                        if lower.contains("operon_key_installed_ok") {
                            state_machine = State::Done;
                        }
                    }
                    State::Done | State::Failed => break,
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                // Keep waiting
                continue;
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                // Reader thread exited
                if accumulated.contains("OPERON_KEY_INSTALLED_OK") {
                    state_machine = State::Done;
                } else {
                    state_machine = State::Failed;
                }
                break;
            }
        }

        if state_machine == State::Done {
            break;
        }
    }

    // Clean up the PTY
    drop(writer);
    let _ = reader_thread.join();
    drop(child);

    if state_machine != State::Done {
        cleanup_keys(&private_key_path, &public_key_path);
        emit_progress(&app, "error", "Key installation could not be confirmed");
        return Err(format!(
            "Key installation could not be confirmed. Server output: {}",
            accumulated.chars().take(300).collect::<String>()
        ));
    }

    // 4. Verify key-based auth works (quick non-interactive test)
    emit_progress(&app, "verifying", "Verifying key-based authentication...");
    let verify_shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
    let verify_cmd = format!(
        "ssh -o BatchMode=yes -o ConnectTimeout=10 -o StrictHostKeyChecking=accept-new -i {} -p {} {}@{} echo OPERON_KEY_VERIFY_OK",
        private_key_path.to_string_lossy(), profile.port, profile.user, profile.host
    );
    let verify_output = std::process::Command::new(&verify_shell)
        .arg("-l").arg("-c").arg(&verify_cmd)
        .output()
        .map_err(|e| format!("Verification failed: {}", e))?;

    let verify_stdout = String::from_utf8_lossy(&verify_output.stdout);

    if !verify_stdout.contains("OPERON_KEY_VERIFY_OK") {
        // Key installed but verification failed — server might still require MFA even with key.
        // Don't delete the keys (they're installed remotely), but warn the user.
        // We'll set use_control_master = true as the fallback strategy.
        eprintln!("[SSH] Key verification failed — server may require MFA on every connection. Enabling ControlMaster fallback.");
        emit_progress(&app, "done", "Key installed, but server still requires MFA. ControlMaster will keep sessions alive.");

        let key_path_str = private_key_path.to_string_lossy().to_string();
        {
            let mut profiles_lock = state.profiles.lock().map_err(|e| e.to_string())?;
            if let Some(p) = profiles_lock.iter_mut().find(|p| p.id == profile_id) {
                p.key_file = Some(key_path_str.clone());
                p.auth_type = AuthType::DuoMfa;
                p.use_control_master = true;
            }
            save_profiles_to_disk(&profiles_lock)?;
        }
        return Ok(key_path_str);
    }

    // Key works without MFA — full success!
    emit_progress(&app, "done", "SSH key installed and verified! No more passwords or MFA needed.");

    let key_path_str = private_key_path.to_string_lossy().to_string();
    {
        let mut profiles_lock = state.profiles.lock().map_err(|e| e.to_string())?;
        if let Some(p) = profiles_lock.iter_mut().find(|p| p.id == profile_id) {
            p.key_file = Some(key_path_str.clone());
            p.auth_type = AuthType::Key;
            p.use_control_master = true;
        }
        save_profiles_to_disk(&profiles_lock)?;
    }

    Ok(key_path_str)
}

// ── Connection Testing ──

#[tauri::command]
pub async fn test_ssh_connection(
    state: tauri::State<'_, SSHManager>,
    profile_id: String,
) -> Result<String, String> {
    let profile = {
        let profiles = state.profiles.lock().map_err(|e| e.to_string())?;
        profiles
            .iter()
            .find(|p| p.id == profile_id)
            .cloned()
            .ok_or_else(|| format!("SSH profile {} not found", profile_id))?
    };

    let result = ssh_exec(&profile, "echo ok && hostname")?;
    Ok(result.trim().to_string())
}

/// Check if a ControlMaster connection is active for a profile.
#[tauri::command]
pub async fn check_control_master(
    state: tauri::State<'_, SSHManager>,
    profile_id: String,
) -> Result<bool, String> {
    let profile = {
        let profiles = state.profiles.lock().map_err(|e| e.to_string())?;
        profiles
            .iter()
            .find(|p| p.id == profile_id)
            .cloned()
            .ok_or_else(|| format!("SSH profile {} not found", profile_id))?
    };

    Ok(control_master_active(&profile))
}

/// Gracefully close a ControlMaster connection.
#[tauri::command]
pub async fn stop_control_master(
    state: tauri::State<'_, SSHManager>,
    profile_id: String,
) -> Result<(), String> {
    let profile = {
        let profiles = state.profiles.lock().map_err(|e| e.to_string())?;
        profiles
            .iter()
            .find(|p| p.id == profile_id)
            .cloned()
            .ok_or_else(|| format!("SSH profile {} not found", profile_id))?
    };

    let sock = control_socket_path(&profile);
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
    let cmd = format!(
        "ssh -o ControlPath={} -O exit {}@{} -p {} 2>/dev/null",
        sock, profile.user, profile.host, profile.port
    );
    let _ = std::process::Command::new(&shell)
        .arg("-l").arg("-c").arg(&cmd)
        .output();

    Ok(())
}

// ── Server Config Auto-Detection ──

/// Auto-detect server environment settings (SLURM accounts, partitions, conda envs, etc.)
/// by running lightweight commands over SSH. Returns a map of detected key-value pairs
/// that the user can review and save to their profile.
#[tauri::command]
pub async fn detect_server_config(
    state: tauri::State<'_, SSHManager>,
    profile_id: String,
) -> Result<HashMap<String, String>, String> {
    let profile = {
        let profiles = state.profiles.lock().map_err(|e| e.to_string())?;
        profiles
            .iter()
            .find(|p| p.id == profile_id)
            .cloned()
            .ok_or_else(|| format!("SSH profile {} not found", profile_id))?
    };

    // Run a single compound command that probes everything in one SSH call.
    // Each section outputs KEY=VALUE pairs, one per line.
    let detect_script = r#"
# ── SLURM ──
if command -v sacctmgr &>/dev/null; then
    # Get the user's default SLURM account
    ACCT=$(sacctmgr -n -P show assoc user=$USER format=Account | head -1)
    [ -n "$ACCT" ] && echo "slurm_account=$ACCT"

    # List partitions available to the user
    PARTS=$(sacctmgr -n -P show assoc user=$USER format=Partition | sort -u | grep -v '^$' | tr '\n' ',')
    PARTS="${PARTS%,}"
    [ -n "$PARTS" ] && echo "slurm_all_partitions=$PARTS"

    # Try to detect GPU partition(s) — common naming conventions
    if sinfo &>/dev/null; then
        GPU_PART=$(sinfo -h -o "%P %G" 2>/dev/null | grep -i 'gpu' | awk '{print $1}' | tr -d '*' | head -1)
        [ -n "$GPU_PART" ] && echo "slurm_gpu_partition=$GPU_PART"

        CPU_PART=$(sinfo -h -o "%P" 2>/dev/null | grep -iv 'gpu' | tr -d '*' | head -1)
        [ -n "$CPU_PART" ] && echo "slurm_partition=$CPU_PART"

        # Detect GPU types available
        GPU_TYPES=$(sinfo -h -o "%G" 2>/dev/null | grep 'gpu' | sed 's/.*://' | sort -u | tr '\n' ',' )
        GPU_TYPES="${GPU_TYPES%,}"
        [ -n "$GPU_TYPES" ] && echo "slurm_gpu_type=$GPU_TYPES"
    fi
fi

# ── Conda ──
if command -v conda &>/dev/null; then
    # List user's conda environments (names only, skip base)
    ENVS=$(conda env list 2>/dev/null | grep -v '^#' | grep -v '^base' | grep -v '^$' | awk '{print $1}' | tr '\n' ',')
    ENVS="${ENVS%,}"
    [ -n "$ENVS" ] && echo "conda_envs=$ENVS"

    # Current active env
    ACTIVE=$(conda info --envs 2>/dev/null | grep '*' | awk '{print $1}')
    [ -n "$ACTIVE" ] && [ "$ACTIVE" != "base" ] && echo "conda_env=$ACTIVE"
fi

# ── Module system ──
if command -v module &>/dev/null; then
    # Currently loaded modules
    LOADED=$(module list 2>&1 | grep -v 'Currently Loaded' | grep -v '^$' | tr -s ' ' | sed 's/^ //' | tr '\n' ',' )
    LOADED="${LOADED%,}"
    [ -n "$LOADED" ] && echo "modules=$LOADED"
fi

# ── Common paths ──
# Scratch directories (common HPC conventions)
for d in /dfs3b /scratch /data /dfs5 /dfs6 /pub /share; do
    USER_DIR=$(find "$d" -maxdepth 3 -type d -name "$USER" 2>/dev/null | head -1)
    if [ -n "$USER_DIR" ]; then
        echo "scratch_dir=$USER_DIR"
        break
    fi
done

# Home directory as work_dir fallback
echo "work_dir=$HOME"
"#;

    let output = ssh_exec(&profile, detect_script)?;

    let mut config = HashMap::new();
    for line in output.lines() {
        let line = line.trim();
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            let value = value.trim();
            if !key.is_empty() && !value.is_empty() {
                config.insert(key.to_string(), value.to_string());
            }
        }
    }

    eprintln!("[ServerConfig] Detected {} settings for {}", config.len(), profile.name);
    Ok(config)
}

// get_server_config is defined earlier in this file (near list_ssh_profiles)
