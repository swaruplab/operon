use base64::Engine;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::Emitter;

use crate::commands::files::FileEntry;

/// Suppress console window creation on Windows for subprocess calls.
#[cfg(windows)]
fn hide_window(cmd: &mut std::process::Command) -> &mut std::process::Command {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(0x08000000)
}
#[cfg(not(windows))]
fn hide_window(cmd: &mut std::process::Command) -> &mut std::process::Command {
    cmd
}

// ── Profile Model ──

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum AuthType {
    /// Simple password auth (no MFA)
    #[default]
    Password,
    /// Key-based auth (key already installed)
    Key,
    /// Keyboard-interactive / Duo MFA (password + push/passcode)
    DuoMfa,
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

/// Returns the path to the SSH profiles file in Operon's data directory.
fn profiles_path() -> Result<std::path::PathBuf, String> {
    let dir = crate::platform::data_dir();
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

pub(crate) fn save_profiles_to_disk(profiles: &[SSHProfile]) -> Result<(), String> {
    let path = profiles_path()?;
    let json = serde_json::to_string_pretty(profiles)
        .map_err(|e| format!("Failed to serialize profiles: {}", e))?;
    std::fs::write(&path, json).map_err(|e| format!("Failed to write profiles: {}", e))?;
    Ok(())
}

// ── ControlMaster Helpers ──

/// Returns the ControlMaster socket path for a given profile.
fn control_socket_path(profile: &SSHProfile) -> String {
    crate::platform::ssh_socket_path(&profile.host, profile.port, &profile.user)
        .to_string_lossy()
        .to_string()
}

/// Check if a ControlMaster socket is active for this profile.
fn control_master_active(profile: &SSHProfile) -> bool {
    crate::platform::ssh_mux_check(&profile.host, profile.port, &profile.user)
}

/// Build common SSH args including ControlMaster and ControlPath.
fn control_master_args(profile: &SSHProfile, as_master: bool) -> String {
    if !profile.use_control_master || !crate::platform::supports_ssh_mux() {
        return String::new();
    }
    crate::platform::ssh_mux_args(&profile.host, profile.port, &profile.user, as_master)
}

// ── Cache ──

/// A single cached value with an expiration time.
struct CacheEntry<T> {
    value: T,
    expires: std::time::Instant,
}

/// TTL cache for remote SSH operations.
/// Keyed by "{profile_id}:{path}" — entries expire after `ttl`.
pub struct SshCache {
    dir_listings: Mutex<HashMap<String, CacheEntry<Vec<FileEntry>>>>,
    file_contents: Mutex<HashMap<String, CacheEntry<String>>>,
    ttl: std::time::Duration,
}

impl SshCache {
    fn new(ttl_secs: u64) -> Self {
        Self {
            dir_listings: Mutex::new(HashMap::new()),
            file_contents: Mutex::new(HashMap::new()),
            ttl: std::time::Duration::from_secs(ttl_secs),
        }
    }

    /// Get a cached directory listing if it hasn't expired.
    fn get_dir(&self, key: &str) -> Option<Vec<FileEntry>> {
        let cache = self.dir_listings.lock().ok()?;
        let entry = cache.get(key)?;
        if std::time::Instant::now() < entry.expires {
            Some(entry.value.clone())
        } else {
            None
        }
    }

    /// Store a directory listing in the cache.
    fn put_dir(&self, key: String, value: Vec<FileEntry>) {
        if let Ok(mut cache) = self.dir_listings.lock() {
            cache.insert(
                key,
                CacheEntry {
                    value,
                    expires: std::time::Instant::now() + self.ttl,
                },
            );
        }
    }

    /// Get a cached file read if it hasn't expired.
    fn get_file(&self, key: &str) -> Option<String> {
        let cache = self.file_contents.lock().ok()?;
        let entry = cache.get(key)?;
        if std::time::Instant::now() < entry.expires {
            Some(entry.value.clone())
        } else {
            None
        }
    }

    /// Store a file read in the cache.
    fn put_file(&self, key: String, value: String) {
        if let Ok(mut cache) = self.file_contents.lock() {
            // Only cache files under 1MB to avoid memory bloat
            if value.len() < 1_048_576 {
                cache.insert(
                    key,
                    CacheEntry {
                        value,
                        expires: std::time::Instant::now() + self.ttl,
                    },
                );
            }
        }
    }

    /// Invalidate all cached entries whose key starts with the given profile prefix.
    /// Called after write operations to ensure fresh data.
    #[allow(dead_code)]
    pub fn invalidate_profile(&self, profile_id: &str) {
        let prefix = format!("{}:", profile_id);
        if let Ok(mut cache) = self.dir_listings.lock() {
            cache.retain(|k, _| !k.starts_with(&prefix));
        }
        if let Ok(mut cache) = self.file_contents.lock() {
            cache.retain(|k, _| !k.starts_with(&prefix));
        }
    }

    /// Invalidate cached entries for a specific directory (and its parent).
    /// More targeted than invalidate_profile — used after single-file writes.
    pub fn invalidate_path(&self, profile_id: &str, path: &str) {
        let dir_key = format!("{}:{}", profile_id, path);
        let parent = std::path::Path::new(path)
            .parent()
            .map(|p| format!("{}:{}", profile_id, p.display()))
            .unwrap_or_default();
        let file_key = format!("{}:{}", profile_id, path);

        if let Ok(mut cache) = self.dir_listings.lock() {
            cache.remove(&dir_key);
            if !parent.is_empty() {
                cache.remove(&parent);
            }
        }
        if let Ok(mut cache) = self.file_contents.lock() {
            cache.remove(&file_key);
        }
    }

    /// Clear everything (used by manual refresh).
    pub fn clear_all(&self) {
        if let Ok(mut cache) = self.dir_listings.lock() {
            cache.clear();
        }
        if let Ok(mut cache) = self.file_contents.lock() {
            cache.clear();
        }
    }

    /// Evict expired entries to prevent unbounded growth.
    fn evict_expired(&self) {
        let now = std::time::Instant::now();
        if let Ok(mut cache) = self.dir_listings.lock() {
            cache.retain(|_, v| now < v.expires);
        }
        if let Ok(mut cache) = self.file_contents.lock() {
            cache.retain(|_, v| now < v.expires);
        }
    }
}

// ── Manager State ──

pub struct SSHManager {
    pub profiles: Mutex<Vec<SSHProfile>>,
    pub active_connections: Mutex<HashMap<String, String>>, // profile_id -> terminal_id
    pub cache: SshCache,
}

impl SSHManager {
    pub fn new() -> Self {
        let profiles = load_profiles_from_disk();
        // Ensure socket directory exists at startup
        let _ = crate::platform::ssh_sockets_dir();
        Self {
            profiles: Mutex::new(profiles),
            active_connections: Mutex::new(HashMap::new()),
            cache: SshCache::new(10), // 10-second TTL
        }
    }
}

// ── Windows Persistent SSH Exec Channel ──
// On macOS/Linux, ControlMaster multiplexes all SSH commands through one TCP connection.
// Windows doesn't support ControlMaster, so university servers that rate-limit SSH
// connections will reject rapid-fire file browsing commands. This provides the equivalent:
// a single persistent SSH process with commands piped through stdin/stdout.

#[cfg(target_os = "windows")]
struct WinSshExecChannel {
    stdin: std::process::ChildStdin,
    reader: std::io::BufReader<std::process::ChildStdout>,
    child: std::process::Child,
}

#[cfg(target_os = "windows")]
impl WinSshExecChannel {
    fn spawn(profile: &SSHProfile) -> Result<Self, String> {
        use std::io::{BufRead, Read, Write};

        use std::os::windows::process::CommandExt;

        let mut cmd = std::process::Command::new("ssh.exe");
        cmd.args([
            "-T", // no PTY allocation on the remote side
            "-o",
            "ServerAliveInterval=30",
            "-o",
            "StrictHostKeyChecking=accept-new",
            "-o",
            "ConnectTimeout=15",
            "-o",
            "LogLevel=ERROR",
        ]);
        cmd.args(["-p", &profile.port.to_string()]);
        if let Some(key) = &profile.key_file {
            if std::path::Path::new(key).exists() {
                cmd.args(["-i", key]);
            }
        }
        // Force key-only auth to avoid hanging on password prompts
        cmd.args(["-o", "PreferredAuthentications=publickey"]);
        cmd.arg(format!("{}@{}", profile.user, profile.host));
        // Start a login shell so PATH and user environment are loaded,
        // matching macOS/Linux behavior where shell_exec uses `-l`.
        cmd.arg("bash -l");
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        cmd.stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("Failed to spawn persistent SSH channel: {}", e))?;

        let stdin = child
            .stdin
            .take()
            .ok_or("Failed to capture SSH channel stdin")?;
        let stdout = child
            .stdout
            .take()
            .ok_or("Failed to capture SSH channel stdout")?;
        let stderr = child.stderr.take();

        // Quick check: give SSH a moment to fail, then check if it's still alive
        std::thread::sleep(std::time::Duration::from_millis(500));
        match child.try_wait() {
            Ok(Some(status)) => {
                // SSH exited immediately — auth failed
                let err_msg = if let Some(mut se) = stderr {
                    let mut buf = String::new();
                    let _ = se.read_to_string(&mut buf);
                    buf
                } else {
                    String::new()
                };
                eprintln!(
                    "[operon-ssh] Exec channel auth failed (exit {}): {}",
                    status,
                    err_msg.trim()
                );
                return Err(format!(
                    "SSH key auth failed for exec channel. Server may require MFA on every connection. Error: {}",
                    err_msg.trim()
                ));
            }
            Ok(None) => {
                // Still running — good, auth succeeded
                eprintln!(
                    "[operon-ssh] Windows exec channel opened for {}@{}:{}",
                    profile.user, profile.host, profile.port
                );
            }
            Err(e) => return Err(format!("Failed to check SSH channel status: {}", e)),
        }

        // Drain stderr in a background thread to prevent pipe buffer deadlock
        if let Some(se) = stderr {
            std::thread::spawn(move || {
                let mut reader = std::io::BufReader::new(se);
                let mut line = String::new();
                loop {
                    line.clear();
                    match reader.read_line(&mut line) {
                        Ok(0) | Err(_) => break,
                        Ok(_) => eprintln!("[operon-ssh-stderr] {}", line.trim()),
                    }
                }
            });
        }

        // Send a probe command and read back to synchronize the channel
        // This consumes any MOTD/banner output and confirms the shell is ready
        let mut channel = Self {
            stdin,
            reader: std::io::BufReader::new(stdout),
            child,
        };
        let probe_delim = "__OPERON_READY__";
        let probe_cmd = format!("echo {}\n", probe_delim);
        channel
            .stdin
            .write_all(probe_cmd.as_bytes())
            .map_err(|e| format!("Failed to send probe: {}", e))?;
        channel
            .stdin
            .flush()
            .map_err(|e| format!("Failed to flush probe: {}", e))?;

        // Read until we see the probe delimiter (skip any MOTD/login noise)
        let mut line = String::new();
        let start = std::time::Instant::now();
        loop {
            if start.elapsed() > std::time::Duration::from_secs(10) {
                return Err("Exec channel probe timed out — shell not responding".to_string());
            }
            line.clear();
            match channel.reader.read_line(&mut line) {
                Ok(0) => return Err("Exec channel closed during probe".to_string()),
                Ok(_) => {
                    if line.trim() == probe_delim {
                        break;
                    }
                    // Skip MOTD/banner lines
                }
                Err(e) => return Err(format!("Exec channel probe read failed: {}", e)),
            }
        }

        eprintln!("[operon-ssh] Exec channel ready (probe OK)");
        Ok(channel)
    }

    fn is_alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    fn exec(&mut self, remote_cmd: &str) -> Result<String, String> {
        use std::io::{BufRead, Write};

        // Use a unique delimiter that won't appear in command output
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let delim = format!("__OPERON_DONE_{}_{}__", std::process::id(), ts);

        // Send command, capture both stdout and stderr, then print delimiter
        let wrapped = format!("{} 2>&1; echo \"{}\"\n", remote_cmd, delim);

        self.stdin.write_all(wrapped.as_bytes()).map_err(|e| {
            format!(
                "SSH channel write failed (connection may have dropped): {}",
                e
            )
        })?;
        self.stdin
            .flush()
            .map_err(|e| format!("SSH channel flush failed: {}", e))?;

        // Read lines until we see the delimiter
        let mut output = String::new();
        let mut line = String::new();
        loop {
            line.clear();
            match self.reader.read_line(&mut line) {
                Ok(0) => return Err("SSH channel closed unexpectedly".to_string()),
                Ok(_) => {
                    if line.trim_end() == delim {
                        break;
                    }
                    output.push_str(&line);
                }
                Err(e) => return Err(format!("SSH channel read failed: {}", e)),
            }
        }

        Ok(output)
    }
}

#[cfg(target_os = "windows")]
impl Drop for WinSshExecChannel {
    fn drop(&mut self) {
        let _ = self.child.kill();
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
    use crate::commands::terminal::TerminalHandle;
    use portable_pty::{native_pty_system, CommandBuilder, PtySize};
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

    // On Windows, route SSH through Git Bash to avoid the ConPTY stall/deadlock bug
    // with interactive SSH sessions. Also disable ControlMaster options which fail on
    // Windows (no Unix domain socket support). Falls back to direct ssh.exe if no Git Bash.
    // On macOS/Linux, use a login shell so PATH, aliases, and SSH agent are available.
    //#[cfg(target_os = "windows")]
    //let mut cmd = {
    //    let mut c = CommandBuilder::new("ssh.exe");
    //    c.args([
    //        &format!("{}@{}", profile.user, profile.host),
    //        "-p",
    //        &profile.port.to_string(),
    //        "-o",
    //        "ServerAliveInterval=30",
    //        // Suppress "getsockname failed: Not a socket" ConPTY warning on Windows
    //        "-o",
    //        "LogLevel=ERROR",
    //    ]);
    //    // Add key file if set
    //    if let Some(key) = &profile.key_file {
    //        c.args(["-i", key]);
    //    }
    //    c
    //};

    #[cfg(target_os = "windows")]
    let mut cmd = {
        // Disable ControlMaster — Windows doesn't support Unix domain sockets for control paths.
        // Also add StrictHostKeyChecking=accept-new and ConnectTimeout for better UX.
        let win_ssh_cmd = format!(
            "{} -o ControlMaster=no -o ControlPath=none -o StrictHostKeyChecking=accept-new -o ConnectTimeout=15",
            ssh_cmd
        );
        if let Some(bash_path) = crate::platform::find_git_bash_path() {
            // Git Bash gives ssh a proper POSIX PTY environment —
            // avoids the ConPTY stall bug with interactive SSH sessions
            let mut c = CommandBuilder::new(&bash_path);
            c.arg("-c");
            c.arg(&win_ssh_cmd);
            c
        } else {
            // Fallback: direct ssh.exe (ConPTY path, less stable for interactive sessions)
            let mut c = CommandBuilder::new("ssh.exe");
            c.args([
                &format!("{}@{}", profile.user, profile.host),
                "-p",
                &profile.port.to_string(),
                "-o",
                "ServerAliveInterval=30",
                "-o",
                "LogLevel=ERROR",
                "-o",
                "ControlMaster=no",
                "-o",
                "ControlPath=none",
                "-o",
                "StrictHostKeyChecking=accept-new",
                "-o",
                "ConnectTimeout=15",
            ]);
            if let Some(key) = &profile.key_file {
                c.args(["-i", key]);
            }
            c
        }
    };

    #[cfg(not(target_os = "windows"))]
    let mut cmd = {
        let shell = crate::platform::default_shell();
        let mut c = CommandBuilder::new(&shell);
        c.arg("-l");
        c.arg("-c");
        c.arg(&ssh_cmd);
        c
    };
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");

    if let Some(home) = crate::platform::home_dir() {
        cmd.env("HOME", home.to_string_lossy().as_ref());
        #[cfg(target_os = "windows")]
        cmd.env("USERPROFILE", home.to_string_lossy().as_ref());
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
                    let _ = app_handle.emit(&event_name, serde_json::json!({ "output": output }));
                }
                Err(_) => break,
            }
        }

        let _ = app_handle.emit(&format!("pty-exit-{}", tid), serde_json::json!({}));
    });

    Ok(())
}

// ── Remote Command Execution (uses ControlMaster when available) ──

/// Run a command on a remote server via SSH.
/// On macOS/Linux: uses ControlMaster socket if active, bypassing re-auth.
/// On Windows: uses a persistent SSH exec channel (single TCP connection reused
/// for all commands — the Windows equivalent of ControlMaster).
pub(crate) fn ssh_exec(profile: &SSHProfile, remote_cmd: &str) -> Result<String, String> {
    let _has_mux = crate::platform::supports_ssh_mux();

    // ── Windows: persistent exec channel (replaces ControlMaster) ──
    // Maintains one SSH connection per server and pipes all commands through it.
    // This avoids opening a new TCP connection for every file operation, which
    // triggers rate-limiting on university/HPC SSH servers.
    #[cfg(target_os = "windows")]
    {
        use std::sync::OnceLock;
        static WIN_CHANNELS: OnceLock<Mutex<HashMap<String, WinSshExecChannel>>> = OnceLock::new();
        let channels_mutex = WIN_CHANNELS.get_or_init(|| Mutex::new(HashMap::new()));
        let channel_key = format!("{}@{}:{}", profile.user, profile.host, profile.port);

        let mut channels = channels_mutex.lock().map_err(|e| e.to_string())?;

        // Get existing channel or create a new one
        let need_new = match channels.get_mut(&channel_key) {
            Some(ch) => !ch.is_alive(),
            None => true,
        };
        if need_new {
            eprintln!(
                "[operon-ssh] Opening persistent exec channel for {}",
                channel_key
            );
            let ch = WinSshExecChannel::spawn(profile)?;
            channels.insert(channel_key.clone(), ch);
        }

        let channel = channels.get_mut(&channel_key).unwrap();
        match channel.exec(remote_cmd) {
            Ok(stdout) => return Ok(stdout),
            Err(e) => {
                // Channel died — remove it and try once more with a fresh connection
                eprintln!("[operon-ssh] Exec channel error: {}. Reconnecting...", e);
                channels.remove(&channel_key);
                let ch = WinSshExecChannel::spawn(profile)?;
                channels.insert(channel_key.clone(), ch);
                let channel = channels.get_mut(&channel_key).unwrap();
                return channel.exec(remote_cmd);
            }
        }
    }

    // ── macOS/Linux: use shell_exec with ControlMaster ──
    #[cfg(not(target_os = "windows"))]
    let output = {
        let mut ssh_args = if _has_mux {
            format!(
                "ssh -o BatchMode=yes -o ConnectTimeout=5 -o ServerAliveInterval=30 {}@{} -p {}",
                profile.user, profile.host, profile.port
            )
        } else {
            format!(
                "ssh -o BatchMode=yes -o ConnectTimeout=10 -o ServerAliveInterval=30 \
                 -o PreferredAuthentications=publickey {}@{} -p {}",
                profile.user, profile.host, profile.port
            )
        };

        ssh_args.push_str(&control_master_args(profile, false));
        if let Some(key) = &profile.key_file {
            if std::path::Path::new(key).exists() {
                ssh_args.push_str(&format!(" -i {}", key));
            }
        }
        ssh_args.push_str(&format!(" -- {}", shell_escape(remote_cmd)));

        crate::platform::shell_exec(&ssh_args)
            .output()
            .map_err(|e| format!("Failed to run SSH: {}", e))?
    };

    #[cfg(not(target_os = "windows"))]
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    #[cfg(not(target_os = "windows"))]
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    // Filter out common SSH noise from stderr (post-quantum warnings, MOTD, etc.)
    #[cfg(not(target_os = "windows"))]
    let filtered_stderr: String = stderr
        .lines()
        .filter(|l| {
            let lt = l.trim();
            !lt.is_empty()
                && !lt.starts_with("Warning: Permanently added")
                && !lt.contains("sntrup")
                && !lt.contains("mlkem")
                && !lt.contains("kex_exchange_identification")
                && !lt.starts_with("debug")
        })
        .collect::<Vec<_>>()
        .join("\n");

    #[cfg(not(target_os = "windows"))]
    if !output.status.success() && stdout.trim().is_empty() {
        // Check if ControlMaster socket exists
        let mux_active = control_master_active(profile);
        let sock_path = control_socket_path(profile);

        if _has_mux && !mux_active {
            return Err(format!(
                "SSH connection not ready for file browsing. The SSH multiplexing socket is not active \
                 (expected at {}). Try disconnecting and reconnecting the SSH terminal, or set up SSH keys \
                 using the key icon in the SSH connection panel.",
                sock_path
            ));
        }

        if filtered_stderr.is_empty() {
            return Err(format!(
                "SSH command failed (exit code {}). This may be a transient connection issue — try clicking Retry.",
                output.status.code().unwrap_or(-1)
            ));
        }

        return Err(format!("SSH command failed: {}", filtered_stderr));
    }

    #[cfg(not(target_os = "windows"))]
    return Ok(stdout);

    // Unreachable on non-Windows, but needed for Windows cfg where the function
    // returns early from the #[cfg(target_os = "windows")] block above.
    #[cfg(target_os = "windows")]
    unreachable!()
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

    // Check cache first (include show_hidden in key to avoid mixing results)
    let cache_key = format!("{}:{}:{}", profile_id, path, show_hidden);
    if let Some(cached) = state.cache.get_dir(&cache_key) {
        return Ok(cached);
    }

    // Periodically evict expired entries (cheap — just a HashMap scan)
    state.cache.evict_expired();

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
        ls_flag,
        shell_escape_inner(&path),
        la_flag,
        shell_escape_inner(&path)
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

        let clean = line.trim_end_matches(['/', '*', '@', '=', '|']);
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
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    // Store in cache for subsequent requests
    state.cache.put_dir(cache_key, entries.clone());

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
    // Check cache first
    let cache_key = format!("{}:{}", profile_id, path);
    if let Some(cached) = state.cache.get_file(&cache_key) {
        return Ok(cached);
    }

    let profile = {
        let profiles = state.profiles.lock().map_err(|e| e.to_string())?;
        profiles
            .iter()
            .find(|p| p.id == profile_id)
            .cloned()
            .ok_or_else(|| format!("SSH profile {} not found", profile_id))?
    };

    let content = ssh_exec(&profile, &format!("cat {}", shell_escape_inner(&path)))?;
    state.cache.put_file(cache_key, content.clone());
    Ok(content)
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
    state.cache.invalidate_path(&profile_id, &path);
    Ok(())
}

/// Delete a file or directory on the remote server via SSH.
#[tauri::command]
pub async fn delete_remote_file(
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

    // Check if path is a file or directory
    let escaped = shell_escape_inner(&path);
    let check_cmd = format!(
        "if [ -d {} ]; then echo DIR; elif [ -f {} ]; then echo FILE; else echo NONE; fi",
        escaped, escaped
    );
    let result = ssh_exec(&profile, &check_cmd)?;
    let kind = result.trim();

    match kind {
        "FILE" => {
            let cmd = format!("rm {}", escaped);
            ssh_exec(&profile, &cmd)?;
        }
        "DIR" => {
            let cmd = format!("rm -rf {}", escaped);
            ssh_exec(&profile, &cmd)?;
        }
        _ => return Err("Path does not exist".to_string()),
    }
    Ok(())
}

/// Rename a file or directory on the remote server via SSH.
#[tauri::command]
pub async fn rename_remote_path(
    state: tauri::State<'_, SSHManager>,
    profile_id: String,
    old_path: String,
    new_path: String,
) -> Result<(), String> {
    let profile = {
        let profiles = state.profiles.lock().map_err(|e| e.to_string())?;
        profiles
            .iter()
            .find(|p| p.id == profile_id)
            .cloned()
            .ok_or_else(|| format!("SSH profile {} not found", profile_id))?
    };

    let cmd = format!(
        "mv {} {}",
        shell_escape_inner(&old_path),
        shell_escape_inner(&new_path)
    );
    ssh_exec(&profile, &cmd)?;
    Ok(())
}

/// Write a file to the remote server via SSH.
/// For text files, pipes content through base64 to avoid quoting issues.
/// Uses chunked transfer to avoid ControlMaster socket message size limits.
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

    let escaped_path = shell_escape_inner(&path);

    // Encode content as base64 and write in chunks to avoid ControlMaster
    // socket message size limits (~256KB). Each chunk is appended to a temp
    // b64 file, then decoded in one shot.
    let b64 = base64::engine::general_purpose::STANDARD.encode(content.as_bytes());
    let tmp_b64 = format!("{}.__operon_tmp_b64__", escaped_path);

    // Max chunk size ~100KB to stay well under the socket limit
    const CHUNK_SIZE: usize = 100_000;

    if b64.len() <= CHUNK_SIZE {
        // Small file — single command, no temp file needed
        let cmd = format!("printf %s {} | base64 -d > {}", b64, escaped_path);
        ssh_exec(&profile, &cmd)?;
    } else {
        // Large file — write base64 in chunks, then decode
        // First chunk: truncate (>)
        let first_chunk = &b64[..CHUNK_SIZE];
        let cmd = format!("printf %s {} > {}", first_chunk, tmp_b64);
        ssh_exec(&profile, &cmd)?;

        // Remaining chunks: append (>>)
        let mut offset = CHUNK_SIZE;
        while offset < b64.len() {
            let end = std::cmp::min(offset + CHUNK_SIZE, b64.len());
            let chunk = &b64[offset..end];
            let cmd = format!("printf %s {} >> {}", chunk, tmp_b64);
            ssh_exec(&profile, &cmd)?;
            offset = end;
        }

        // Decode the assembled base64 file and clean up
        let cmd = format!(
            "base64 -d {} > {} && rm -f {}",
            tmp_b64, escaped_path, tmp_b64
        );
        ssh_exec(&profile, &cmd)?;
    }

    state.cache.invalidate_path(&profile_id, &path);
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
        "-o".to_string(),
        "BatchMode=yes".to_string(),
        "-o".to_string(),
        "ConnectTimeout=10".to_string(),
    ];
    // On Windows (no ControlMaster), restrict to publickey auth to avoid Duo hang
    if !crate::platform::supports_ssh_mux() {
        scp_args.push("-o".to_string());
        scp_args.push("PreferredAuthentications=publickey".to_string());
    }

    // Use ControlMaster socket if available
    // Use the same ControlMaster socket that spawn_ssh_terminal creates
    let sock = crate::platform::ssh_socket_path(&profile.host, profile.port, &profile.user);
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

    let output = hide_window(std::process::Command::new("scp").args(&scp_args))
        .output()
        .map_err(|e| format!("Failed to run scp: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("SCP failed: {}", stderr));
    }

    state.cache.invalidate_path(&profile_id, &remote_path);
    Ok(())
}

/// Copy a remote file to the local machine via SCP.
/// Uses ControlMaster socket if available.
#[tauri::command]
pub async fn scp_from_remote(
    state: tauri::State<'_, SSHManager>,
    profile_id: String,
    remote_path: String,
    local_path: String,
) -> Result<(), String> {
    let profile = {
        let profiles = state.profiles.lock().map_err(|e| e.to_string())?;
        profiles
            .iter()
            .find(|p| p.id == profile_id)
            .cloned()
            .ok_or_else(|| format!("SSH profile {} not found", profile_id))?
    };

    // Ensure local parent directory exists
    if let Some(parent) = std::path::Path::new(&local_path).parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let host_str = format!("{}@{}", profile.user, profile.host);
    let mut scp_args: Vec<String> = vec![
        "-o".to_string(),
        "BatchMode=yes".to_string(),
        "-o".to_string(),
        "ConnectTimeout=10".to_string(),
    ];
    if !crate::platform::supports_ssh_mux() {
        scp_args.push("-o".to_string());
        scp_args.push("PreferredAuthentications=publickey".to_string());
    }

    // Use the same ControlMaster socket that spawn_ssh_terminal creates
    let sock = crate::platform::ssh_socket_path(&profile.host, profile.port, &profile.user);
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

    scp_args.push(format!("{}:{}", host_str, remote_path));
    scp_args.push(local_path);

    let output = hide_window(std::process::Command::new("scp").args(&scp_args))
        .output()
        .map_err(|e| format!("Failed to run scp: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("SCP download failed: {}", stderr));
    }

    Ok(())
}

/// Copy a remote directory to the local machine via SCP -r.
#[tauri::command]
pub async fn scp_dir_from_remote(
    state: tauri::State<'_, SSHManager>,
    profile_id: String,
    remote_path: String,
    local_path: String,
) -> Result<(), String> {
    let profile = {
        let profiles = state.profiles.lock().map_err(|e| e.to_string())?;
        profiles
            .iter()
            .find(|p| p.id == profile_id)
            .cloned()
            .ok_or_else(|| format!("SSH profile {} not found", profile_id))?
    };

    if let Some(parent) = std::path::Path::new(&local_path).parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let host_str = format!("{}@{}", profile.user, profile.host);
    let mut scp_args: Vec<String> = vec![
        "-r".to_string(),
        "-o".to_string(),
        "BatchMode=yes".to_string(),
        "-o".to_string(),
        "ConnectTimeout=10".to_string(),
    ];
    if !crate::platform::supports_ssh_mux() {
        scp_args.push("-o".to_string());
        scp_args.push("PreferredAuthentications=publickey".to_string());
    }

    // Use the same ControlMaster socket that spawn_ssh_terminal creates
    let sock = crate::platform::ssh_socket_path(&profile.host, profile.port, &profile.user);
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

    scp_args.push(format!("{}:{}", host_str, remote_path));
    scp_args.push(local_path);

    let output = hide_window(std::process::Command::new("scp").args(&scp_args))
        .output()
        .map_err(|e| format!("Failed to run scp: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("SCP directory download failed: {}", stderr));
    }

    Ok(())
}

/// Upload multiple local files to a remote directory via SCP.
/// Emits `scp-transfer-progress` events for each completed file.
#[tauri::command]
pub async fn scp_batch_upload(
    app: tauri::AppHandle,
    state: tauri::State<'_, SSHManager>,
    profile_id: String,
    local_paths: Vec<String>,
    remote_dir: String,
) -> Result<u32, String> {
    let profile = {
        let profiles = state.profiles.lock().map_err(|e| e.to_string())?;
        profiles
            .iter()
            .find(|p| p.id == profile_id)
            .cloned()
            .ok_or_else(|| format!("SSH profile {} not found", profile_id))?
    };

    // Ensure remote directory exists
    let mkdir_cmd = format!("mkdir -p {}", shell_escape_inner(&remote_dir));
    let _ = ssh_exec(&profile, &mkdir_cmd);

    let total = local_paths.len() as u32;
    let mut completed: u32 = 0;
    let mut errors: Vec<String> = Vec::new();

    let host_str = format!("{}@{}", profile.user, profile.host);

    // Build base SCP args once
    let mut base_args: Vec<String> = vec![
        "-o".to_string(),
        "BatchMode=yes".to_string(),
        "-o".to_string(),
        "ConnectTimeout=10".to_string(),
    ];
    if !crate::platform::supports_ssh_mux() {
        base_args.push("-o".to_string());
        base_args.push("PreferredAuthentications=publickey".to_string());
    }
    // Use the same ControlMaster socket that spawn_ssh_terminal creates
    let sock = crate::platform::ssh_socket_path(&profile.host, profile.port, &profile.user);
    if sock.exists() {
        base_args.push("-o".to_string());
        base_args.push(format!("ControlPath={}", sock.to_string_lossy()));
    }
    if profile.port != 22 {
        base_args.push("-P".to_string());
        base_args.push(profile.port.to_string());
    }
    if let Some(key) = &profile.key_file {
        if std::path::Path::new(key).exists() {
            base_args.push("-i".to_string());
            base_args.push(key.clone());
        }
    }

    for local_path in &local_paths {
        let local = std::path::Path::new(local_path);
        let file_name = local
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "file".to_string());

        let remote_dest = if remote_dir.ends_with('/') {
            format!("{}{}", remote_dir, file_name)
        } else {
            format!("{}/{}", remote_dir, file_name)
        };

        // Use -r flag for directories
        let mut args = base_args.clone();
        if local.is_dir() {
            args.insert(0, "-r".to_string());
        }
        args.push(local_path.clone());
        args.push(format!("{}:{}", host_str, remote_dest));

        let output = hide_window(std::process::Command::new("scp").args(&args))
            .output()
            .map_err(|e| format!("Failed to run scp: {}", e))?;

        if output.status.success() {
            completed += 1;
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            errors.push(format!("{}: {}", file_name, stderr.trim()));
        }

        // Emit progress event
        let _ = app.emit(
            "scp-transfer-progress",
            serde_json::json!({
                "completed": completed,
                "total": total,
                "current_file": file_name,
                "errors": errors.len(),
            }),
        );
    }

    if !errors.is_empty() && completed == 0 {
        return Err(format!("All transfers failed: {}", errors.join("; ")));
    }

    // Invalidate the target directory cache since new files were uploaded
    state.cache.invalidate_path(&profile_id, &remote_dir);

    Ok(completed)
}

/// Clear the SSH remote file/directory cache.
/// Called by the UI refresh button to force fresh data on next load.
#[tauri::command]
pub async fn clear_ssh_cache(state: tauri::State<'_, SSHManager>) -> Result<(), String> {
    state.cache.clear_all();
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
    pub stage: String, // "connecting", "password", "mfa_waiting", "installing", "verifying", "done", "error"
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
    let home = crate::platform::home_dir().ok_or("Could not determine home directory")?;
    let ssh_dir = home.join(".ssh");
    if !ssh_dir.exists() {
        std::fs::create_dir_all(&ssh_dir)
            .map_err(|e| format!("Failed to create .ssh dir: {}", e))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&ssh_dir, std::fs::Permissions::from_mode(0o700))
                .map_err(|e| format!("Failed to set .ssh permissions: {}", e))?;
        }
    }

    let safe_host = profile.host.replace(['.', ':'], "_");
    let key_name = format!("operon_{}", safe_host);
    let private_key_path = ssh_dir.join(&key_name);
    let public_key_path = ssh_dir.join(format!("{}.pub", key_name));

    if !private_key_path.exists() {
        let output = hide_window(std::process::Command::new("ssh-keygen").args([
            "-t",
            "ed25519",
            "-f",
            &private_key_path.to_string_lossy(),
            "-N",
            "",
            "-C",
            &format!("operon@{}", profile.host),
        ]))
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
    emit_progress(
        &app,
        "connecting",
        &format!("Connecting to {}...", profile.host),
    );

    let pty_system = native_pty_system();
    let size = PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
    };
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
        profile.port,
        profile.user,
        profile.host,
        shell_escape(&install_script)
    );

    // On Windows, run ssh.exe directly — cmd.exe doesn't accept -l/-c flags.
    // On macOS/Linux, use a login shell so PATH and aliases are available.
    #[cfg(target_os = "windows")]
    let mut cmd = {
        let mut c = CommandBuilder::new("ssh.exe");
        c.args([
            "-o",
            "StrictHostKeyChecking=accept-new",
            "-o",
            "ConnectTimeout=15",
            "-p",
            &profile.port.to_string(),
            &format!("{}@{}", profile.user, profile.host),
            &install_script,
        ]);
        c
    };
    #[cfg(not(target_os = "windows"))]
    let mut cmd = {
        let shell = crate::platform::default_shell();
        let mut c = CommandBuilder::new(&shell);
        c.arg("-l");
        c.arg("-c");
        c.arg(&ssh_cmd);
        c
    };
    cmd.env("TERM", "xterm-256color");
    if let Some(h) = crate::platform::home_dir() {
        cmd.env("HOME", h.to_string_lossy().as_ref());
        #[cfg(target_os = "windows")]
        cmd.env("USERPROFILE", h.to_string_lossy().as_ref());
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
        WaitingForPrompt,   // Waiting for password or any prompt
        WaitingForDuo,      // Password was sent, looking for Duo prompt
        WaitingForApproval, // Duo push sent, waiting for approval
        WaitingForResult,   // Authenticated, waiting for key install confirmation
        Done,
        Failed,
    }

    // Strip ANSI escape sequences from PTY output.
    // ConPTY on Windows injects cursor positioning, bracketed paste markers,
    // OSC title sequences, and other control codes that break pattern matching.
    fn strip_ansi(s: &str) -> String {
        let mut result = String::with_capacity(s.len());
        let mut chars = s.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\x1b' {
                match chars.peek() {
                    // ESC [ ... (letter)  — CSI sequence (cursor, colors, etc.)
                    Some(&'[') => {
                        chars.next(); // consume '['
                        while let Some(&next) = chars.peek() {
                            chars.next();
                            if next.is_ascii_alphabetic() || next == '~' {
                                break;
                            }
                        }
                    }
                    // ESC ] ... BEL/ST  — OSC sequence (window title, etc.)
                    // Terminates with BEL (\x07) or ST (\x1b\\)
                    Some(&']') => {
                        chars.next(); // consume ']'
                        while let Some(&next) = chars.peek() {
                            chars.next();
                            if next == '\x07' {
                                break;
                            }
                            if next == '\x1b' {
                                if chars.peek() == Some(&'\\') {
                                    chars.next();
                                }
                                break;
                            }
                        }
                    }
                    _ => {
                        chars.next();
                    } // ESC + one char
                }
            } else if c == '\r' || c == '\x07' {
                // Strip carriage returns and stray BEL characters
                continue;
            } else {
                result.push(c);
            }
        }
        result
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
    let _reader_thread = std::thread::spawn(move || loop {
        match reader.read(&mut buf) {
            Ok(0) => {
                let _ = tx.send(Vec::new());
                break;
            }
            Ok(n) => {
                let _ = tx.send(buf[..n].to_vec());
            }
            Err(_) => {
                let _ = tx.send(Vec::new());
                break;
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
                        // Check if we got the success marker before EOF (strip ANSI for Windows)
                        let clean_acc = strip_ansi(&accumulated);
                        if clean_acc.contains("OPERON_KEY_INSTALLED_OK") {
                            state_machine = State::Done;
                        } else {
                            state_machine = State::Failed;
                        }
                    }
                    break;
                }
                let text = String::from_utf8_lossy(&data).to_string();
                accumulated.push_str(&text);
                // Strip ANSI escapes + \r (ConPTY on Windows) before pattern matching
                let clean = strip_ansi(&accumulated);
                let lower = clean.to_lowercase();

                match state_machine {
                    State::WaitingForPrompt => {
                        // Look for password prompt
                        if !password_sent
                            && (lower.contains("password:") ||
                            lower.contains("password for") ||
                            lower.ends_with("'s password: ") ||
                            // keyboard-interactive prompt
                            lower.contains("(current) password") ||
                            lower.contains("verification code"))
                        {
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
                        if lower.contains("connection refused")
                            || lower.contains("no route to host")
                            || lower.contains("connection timed out")
                        {
                            cleanup_keys(&private_key_path, &public_key_path);
                            let msg = format!("Could not connect to {}", profile.host);
                            emit_progress(&app, "error", &msg);
                            return Err(msg);
                        }
                    }
                    State::WaitingForDuo => {
                        // Check for Duo MFA prompt
                        if !duo_responded
                            && (lower.contains("duo two-factor")
                                || lower.contains("duo login")
                                || lower.contains("passcode or option")
                                || lower.contains("1. duo push")
                                || lower.contains("enter a passcode"))
                        {
                            // Duo detected! Respond based on preferred method
                            let mfa_response = match mfa_method.as_deref() {
                                Some("phone") | Some("2") => "2",
                                Some("passcode") => {
                                    // If mfa_method is "passcode", we can't proceed without the actual code
                                    // The user should pass the actual passcode as mfa_method
                                    "1" // fallback to push
                                }
                                Some(code)
                                    if code.chars().all(|c| c.is_ascii_digit())
                                        && code.len() >= 6 =>
                                {
                                    // User passed an actual passcode
                                    code
                                }
                                _ => "1", // Default: Duo Push
                            };

                            if mfa_response == "1" {
                                emit_progress(
                                    &app,
                                    "mfa_waiting",
                                    "Duo push sent — approve on your phone...",
                                );
                            } else if mfa_response == "2" {
                                emit_progress(
                                    &app,
                                    "mfa_waiting",
                                    "Calling your phone for Duo approval...",
                                );
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
                        else if lower.contains("permission denied")
                            || (password_sent && lower.contains("password:"))
                        {
                            cleanup_keys(&private_key_path, &public_key_path);
                            emit_progress(&app, "error", "Authentication failed — wrong password");
                            return Err("Authentication failed — wrong password or MFA rejected"
                                .to_string());
                        }
                        // Might already be logged in (fast password-only servers)
                        else if lower.contains("last login") || lower.contains("welcome") {
                            emit_progress(
                                &app,
                                "installing",
                                "Authenticated. Installing SSH key...",
                            );
                            state_machine = State::WaitingForResult;
                        }
                    }
                    State::WaitingForApproval => {
                        if lower.contains("success")
                            || lower.contains("operon_key_installed_ok")
                            || lower.contains("last login")
                        {
                            if lower.contains("operon_key_installed_ok") {
                                state_machine = State::Done;
                            } else {
                                emit_progress(
                                    &app,
                                    "installing",
                                    "MFA approved. Installing SSH key...",
                                );
                                state_machine = State::WaitingForResult;
                            }
                        }
                        if lower.contains("denied")
                            || lower.contains("timed out")
                            || lower.contains("error")
                        {
                            cleanup_keys(&private_key_path, &public_key_path);
                            emit_progress(&app, "error", "Duo authentication denied or timed out");
                            return Err(
                                "Duo MFA denied or timed out. Please try again.".to_string()
                            );
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
                // Reader thread exited — strip ANSI before checking success marker
                if strip_ansi(&accumulated).contains("OPERON_KEY_INSTALLED_OK") {
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

    // Clean up the PTY — drop child FIRST to terminate the SSH process,
    // which causes the PTY to close and the reader thread to eventually get EOF.
    // On Windows ConPTY, reader.read() can block indefinitely after the child exits
    // unless we drop the child first. Don't join the reader thread — it will self-terminate
    // when the PTY master is dropped and the read returns EOF/error.
    drop(writer);
    drop(child);
    // Don't reader_thread.join() — on Windows ConPTY it can hang.

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
    // Run ssh directly (not through cmd.exe) to avoid path resolution issues on Windows
    let verify_output = {
        let mut cmd = std::process::Command::new("ssh");
        cmd.args([
            "-o",
            "BatchMode=yes",
            "-o",
            "ConnectTimeout=10",
            "-o",
            "StrictHostKeyChecking=accept-new",
        ]);
        cmd.args(["-i", &private_key_path.to_string_lossy()]);
        cmd.args(["-p", &profile.port.to_string()]);
        cmd.arg(format!("{}@{}", profile.user, profile.host));
        cmd.arg("echo OPERON_KEY_VERIFY_OK");
        hide_window(&mut cmd)
            .output()
            .map_err(|e| format!("Verification failed: {}", e))?
    };

    let verify_stdout = String::from_utf8_lossy(&verify_output.stdout);

    if !verify_stdout.contains("OPERON_KEY_VERIFY_OK") {
        // Key installed but verification failed — server might still require MFA even with key.
        // Don't delete the keys (they're installed remotely), but warn the user.
        // We'll set use_control_master = true as the fallback strategy.
        eprintln!("[SSH] Key verification failed — server may require MFA on every connection. Enabling ControlMaster fallback.");
        emit_progress(
            &app,
            "done",
            "Key installed, but server still requires MFA. ControlMaster will keep sessions alive.",
        );

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
    emit_progress(
        &app,
        "done",
        "SSH key installed and verified! No more passwords or MFA needed.",
    );

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
    let cmd = format!(
        "ssh -o \"ControlPath={}\" -O exit {}@{} -p {} 2>/dev/null",
        sock, profile.user, profile.host, profile.port
    );
    let _ = crate::platform::shell_exec(&cmd).output();

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

    eprintln!(
        "[ServerConfig] Detected {} settings for {}",
        config.len(),
        profile.name
    );
    Ok(config)
}

// get_server_config is defined earlier in this file (near list_ssh_profiles)

// ── ~/.ssh/config Parser ─────────────────────────────────────────────────
//
// Lightweight reader for OpenSSH client config files. Surfaces the fields
// Operon actually uses (host, user, port, identity file, ProxyJump) so the
// "Add Connection" form can preload entries for users who already maintain
// a ~/.ssh/config.
//
// Behavior:
//   - Reads ~/.ssh/config (plus any Include'd fragments, max depth 10)
//   - Splits "Host a b c" into individual alias rows
//   - Drops wildcard-only aliases ("*", "*.example.com") — those are defaults,
//     not connectable targets
//   - Expands ~ and $HOME in IdentityFile/Include paths
//   - Honors the SSH override rule: first matching value wins across blocks
//     when the same alias appears multiple times (we just keep the first)

#[derive(Debug, Clone, Serialize)]
pub struct SSHConfigHost {
    /// Alias as written after "Host" (the thing a user types: `ssh <alias>`).
    pub alias: String,
    /// HostName value, or None if absent (SSH would fall back to alias).
    pub hostname: Option<String>,
    pub user: Option<String>,
    pub port: Option<u16>,
    pub identity_file: Option<String>,
    pub proxy_jump: Option<String>,
    /// Absolute path of the config file this entry came from — shown in UI
    /// so advanced users can tell Include'd fragments from the main config.
    pub source_file: String,
}

/// Parse `~/.ssh/config` and return all named Host entries.
/// Silently returns [] if the file doesn't exist.
#[tauri::command]
pub fn list_ssh_config_hosts() -> Result<Vec<SSHConfigHost>, String> {
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return Ok(vec![]),
    };
    let config_path = home.join(".ssh").join("config");
    if !config_path.exists() {
        return Ok(vec![]);
    }
    let mut hosts: Vec<SSHConfigHost> = Vec::new();
    let mut visited: std::collections::HashSet<std::path::PathBuf> =
        std::collections::HashSet::new();
    parse_ssh_config_file(&config_path, &home, &mut hosts, &mut visited, 0);

    // Drop wildcard-only aliases; keep first occurrence for dup aliases.
    let mut seen_aliases: std::collections::HashSet<String> =
        std::collections::HashSet::new();
    let filtered: Vec<SSHConfigHost> = hosts
        .into_iter()
        .filter(|h| {
            !h.alias.contains('*')
                && !h.alias.contains('?')
                && !h.alias.is_empty()
                && seen_aliases.insert(h.alias.clone())
        })
        .collect();
    Ok(filtered)
}

fn parse_ssh_config_file(
    path: &std::path::Path,
    home: &std::path::Path,
    hosts: &mut Vec<SSHConfigHost>,
    visited: &mut std::collections::HashSet<std::path::PathBuf>,
    depth: usize,
) {
    if depth > 10 {
        return;
    }
    let canon = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    if !visited.insert(canon) {
        return;
    }
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return,
    };
    let source_file = path.to_string_lossy().to_string();

    // Current blocks under construction — one "Host a b c" produces multiple.
    let mut current: Vec<SSHConfigHost> = Vec::new();
    let flush =
        |cur: &mut Vec<SSHConfigHost>, hosts: &mut Vec<SSHConfigHost>| {
            if !cur.is_empty() {
                hosts.extend(cur.drain(..));
            }
        };

    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = match split_ssh_kv(line) {
            Some(kv) => kv,
            None => continue,
        };
        let key_lower = key.to_ascii_lowercase();

        match key_lower.as_str() {
            "host" => {
                flush(&mut current, hosts);
                for alias in value.split_whitespace() {
                    current.push(SSHConfigHost {
                        alias: alias.to_string(),
                        hostname: None,
                        user: None,
                        port: None,
                        identity_file: None,
                        proxy_jump: None,
                        source_file: source_file.clone(),
                    });
                }
            }
            "hostname" => {
                for h in current.iter_mut() {
                    h.hostname = Some(value.to_string());
                }
            }
            "user" => {
                for h in current.iter_mut() {
                    h.user = Some(value.to_string());
                }
            }
            "port" => {
                if let Ok(p) = value.parse::<u16>() {
                    for h in current.iter_mut() {
                        h.port = Some(p);
                    }
                }
            }
            "identityfile" => {
                let expanded = expand_home_path(value, home);
                for h in current.iter_mut() {
                    if h.identity_file.is_none() {
                        h.identity_file = Some(expanded.clone());
                    }
                }
            }
            "proxyjump" => {
                for h in current.iter_mut() {
                    h.proxy_jump = Some(value.to_string());
                }
            }
            "include" => {
                // `Include` can appear at the top OR inside a Host block;
                // in the latter case OpenSSH still processes it, but the
                // included fragments are treated as independent config.
                for include_path in expand_include(value, home, path) {
                    parse_ssh_config_file(
                        &include_path,
                        home,
                        hosts,
                        visited,
                        depth + 1,
                    );
                }
            }
            _ => {}
        }
    }
    flush(&mut current, hosts);
}

/// OpenSSH allows either `Key Value` or `Key = Value` with any whitespace.
fn split_ssh_kv(line: &str) -> Option<(&str, &str)> {
    // Find the first '=' or whitespace separator, whichever comes first.
    let bytes = line.as_bytes();
    let mut split = None;
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'=' || b == b' ' || b == b'\t' {
            split = Some(i);
            break;
        }
    }
    let idx = split?;
    let key = line[..idx].trim();
    // Skip any run of '=' and whitespace after the key
    let mut rest = &line[idx..];
    rest = rest.trim_start_matches(|c: char| c == '=' || c.is_whitespace());
    if key.is_empty() || rest.is_empty() {
        return None;
    }
    // Strip surrounding quotes
    let value = rest.trim_matches(|c: char| c == '"' || c == '\'');
    Some((key, value))
}

/// Expand a leading ~ or ${HOME} to the user's home directory. Leaves
/// other paths untouched.
fn expand_home_path(raw: &str, home: &std::path::Path) -> String {
    let v = raw.trim();
    if let Some(rest) = v.strip_prefix("~/") {
        return home.join(rest).to_string_lossy().to_string();
    }
    if v == "~" {
        return home.to_string_lossy().to_string();
    }
    if let Some(rest) = v.strip_prefix("$HOME/") {
        return home.join(rest).to_string_lossy().to_string();
    }
    if let Some(rest) = v.strip_prefix("${HOME}/") {
        return home.join(rest).to_string_lossy().to_string();
    }
    v.to_string()
}

/// Expand a single `Include <pattern>` line into concrete paths. Handles
/// simple shell globs (one `*` per path segment) which is the common HPC
/// setup (`Include ~/.ssh/config.d/*`). Relative paths resolve against
/// the including file's directory, per OpenSSH semantics.
fn expand_include(
    raw: &str,
    home: &std::path::Path,
    including: &std::path::Path,
) -> Vec<std::path::PathBuf> {
    let expanded = expand_home_path(raw, home);
    let candidate = std::path::Path::new(&expanded);
    let absolute: std::path::PathBuf = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else if let Some(parent) = including.parent() {
        parent.join(candidate)
    } else {
        candidate.to_path_buf()
    };

    if !absolute.to_string_lossy().contains('*')
        && !absolute.to_string_lossy().contains('?')
    {
        return if absolute.exists() {
            vec![absolute]
        } else {
            vec![]
        };
    }

    // Only handle a wildcard in the final path component (the common case).
    let parent = absolute.parent().unwrap_or_else(|| std::path::Path::new("."));
    let pattern = absolute
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");
    let mut results = Vec::new();
    if let Ok(rd) = std::fs::read_dir(parent) {
        for entry in rd.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if simple_glob_match(pattern, &name) {
                results.push(entry.path());
            }
        }
    }
    results.sort();
    results
}

/// Very small glob matcher: supports `*` (any substring) and `?` (single
/// char). Good enough for SSH Include patterns.
fn simple_glob_match(pattern: &str, name: &str) -> bool {
    // Exact match fast path
    if !pattern.contains('*') && !pattern.contains('?') {
        return pattern == name;
    }
    let p: Vec<char> = pattern.chars().collect();
    let s: Vec<char> = name.chars().collect();
    fn m(p: &[char], s: &[char]) -> bool {
        match (p.first(), s.first()) {
            (None, None) => true,
            (None, Some(_)) => false,
            (Some('*'), _) => m(&p[1..], s) || (!s.is_empty() && m(p, &s[1..])),
            (Some('?'), Some(_)) => m(&p[1..], &s[1..]),
            (Some(&pc), Some(&sc)) if pc == sc => m(&p[1..], &s[1..]),
            _ => false,
        }
    }
    m(&p, &s)
}
