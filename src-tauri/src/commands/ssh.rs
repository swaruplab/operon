use base64::Engine;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::ToSocketAddrs;
use std::sync::Mutex;
use tauri::{Emitter, Manager};

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

/// Ensure a backgrounded ControlMaster is alive for this profile. Returns
/// Ok(true) if a master socket is usable after the call, Ok(false) if the
/// platform or profile opts out, Err with a diagnostic if the spawn failed.
/// Lets short-lived `ssh_exec` callers (file browser, status polls) avoid a
/// full handshake when no interactive SSH terminal has been opened yet.
fn ensure_control_master(profile: &SSHProfile) -> Result<bool, String> {
    if !profile.use_control_master || !crate::platform::supports_ssh_mux() {
        return Ok(false);
    }
    if control_master_active(profile) {
        return Ok(true);
    }

    let sock = control_socket_path(profile);
    let mut cmd = format!(
        "ssh -M -N -f \
         -o ControlMaster=yes \
         -o ControlPath='{}' \
         -o ControlPersist=10m \
         -o ServerAliveInterval=30 \
         -o ServerAliveCountMax=3 \
         -o ConnectTimeout=15 \
         -o BatchMode=yes",
        sock.replace('\'', "'\\''")
    );
    if let Some(key) = &profile.key_file {
        if std::path::Path::new(key).exists() {
            cmd.push_str(&format!(" -i '{}'", key.replace('\'', "'\\''")));
        }
    }
    cmd.push_str(&format!(
        " -p {} {}@{}",
        profile.port, profile.user, profile.host
    ));

    let status = crate::platform::shell_exec(&cmd)
        .status()
        .map_err(|e| format!("Failed to spawn ControlMaster: {}", e))?;

    if control_master_active(profile) {
        Ok(true)
    } else {
        Err(format!(
            "ControlMaster spawn exited {} but socket {} is not active",
            status.code().unwrap_or(-1),
            sock
        ))
    }
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
    pub cache: SshCache,
}

impl SSHManager {
    pub fn new() -> Self {
        let profiles = load_profiles_from_disk();
        // Ensure socket directory exists at startup
        let _ = crate::platform::ssh_sockets_dir();
        Self {
            profiles: Mutex::new(profiles),
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
    /// Lines forwarded by a dedicated reader thread (newline preserved). Reading
    /// through a channel — instead of blocking directly on the pipe — lets
    /// `exec` enforce a wall-clock timeout, so a stalled connection can never
    /// hang the caller forever.
    rx: std::sync::mpsc::Receiver<String>,
    child: std::process::Child,
}

#[cfg(target_os = "windows")]
impl WinSshExecChannel {
    /// If a command produces no output at all for this long, the connection is
    /// treated as stalled and the channel is torn down. Real long-running work
    /// (npm installs, large transfers) streams progress continuously and never
    /// trips this — only a genuinely wedged command does.
    const IDLE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

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

        // Dedicated reader thread: owns stdout and forwards every line over an
        // mpsc channel. `exec`/probe then receive with a timeout, so a hung
        // remote command can never block a plain `read_line` indefinitely.
        let (tx, rx) = std::sync::mpsc::channel::<String>();
        std::thread::spawn(move || {
            let mut reader = std::io::BufReader::new(stdout);
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line) {
                    Ok(0) | Err(_) => break, // EOF or read error — channel closed
                    Ok(_) => {
                        if tx.send(line.clone()).is_err() {
                            break; // receiver dropped — channel is being torn down
                        }
                    }
                }
            }
        });

        let mut channel = Self { stdin, rx, child };

        // Send a probe command and read back to synchronize the channel.
        // This consumes any MOTD/banner output and confirms the shell is ready.
        let probe_delim = "__OPERON_READY__";
        channel
            .stdin
            .write_all(format!("echo {}\n", probe_delim).as_bytes())
            .map_err(|e| format!("Failed to send probe: {}", e))?;
        channel
            .stdin
            .flush()
            .map_err(|e| format!("Failed to flush probe: {}", e))?;

        // Read until we see the probe delimiter (skip any MOTD/login noise).
        let probe_deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        loop {
            let remaining = probe_deadline
                .checked_duration_since(std::time::Instant::now())
                .unwrap_or_default();
            if remaining.is_zero() {
                return Err("Exec channel probe timed out — shell not responding".to_string());
            }
            match channel.rx.recv_timeout(remaining) {
                Ok(line) => {
                    if line.trim() == probe_delim {
                        break;
                    }
                    // Skip MOTD/banner lines
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    return Err("Exec channel probe timed out — shell not responding".to_string());
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    return Err("Exec channel closed during probe".to_string());
                }
            }
        }

        eprintln!("[operon-ssh] Exec channel ready (probe OK)");
        Ok(channel)
    }

    fn is_alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    fn exec(&mut self, remote_cmd: &str) -> Result<String, String> {
        use std::io::Write;

        // Use a unique delimiter that won't appear in command output
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let delim = format!("__OPERON_DONE_{}_{}__", std::process::id(), ts);

        // Run the command as a brace group with stdin redirected from
        // /dev/null. This is critical: the channel's stdin *is* its command
        // stream, so a command that reads stdin — most notably `npx`, which
        // prompts "Ok to proceed?" when a package isn't cached — would
        // otherwise swallow the next command (and the delimiter) and wedge the
        // whole channel. `</dev/null` gives such commands an immediate EOF.
        // The group is closed on its own line so a trailing comment in
        // `remote_cmd` can't absorb the `}`.
        let wrapped = format!(
            "{{ {}\n}} </dev/null 2>&1\necho \"{}\"\n",
            remote_cmd, delim
        );

        self.stdin.write_all(wrapped.as_bytes()).map_err(|e| {
            format!(
                "SSH channel write failed (connection may have dropped): {}",
                e
            )
        })?;
        self.stdin
            .flush()
            .map_err(|e| format!("SSH channel flush failed: {}", e))?;

        // Read lines until we see the delimiter, bounded by an idle timeout.
        let mut output = String::new();
        loop {
            match self.rx.recv_timeout(Self::IDLE_TIMEOUT) {
                Ok(line) => {
                    let trimmed = line.trim_end_matches(['\r', '\n']);
                    if trimmed == delim {
                        break;
                    }
                    // If the command's final line lacked a trailing newline,
                    // `echo` fuses the delimiter onto it. The delimiter is a
                    // per-call nonce, so a suffix match unambiguously means
                    // "end of output" — keep the real prefix and stop.
                    if let Some(prefix) = trimmed.strip_suffix(delim.as_str()) {
                        output.push_str(prefix);
                        break;
                    }
                    output.push_str(&line);
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    return Err(format!(
                        "SSH channel stalled — no output for {}s. The connection will be rebuilt.",
                        Self::IDLE_TIMEOUT.as_secs()
                    ));
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    return Err("SSH channel closed unexpectedly".to_string());
                }
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

// ── Unix Persistent SSH Exec Channel ──
// macOS/Linux mirror of WinSshExecChannel. Even with a live ControlMaster
// socket, every per-call `ssh` subprocess pays a fork+exec+channel-open cost
// (~50-200ms on macOS). This holds one `ssh -T ... bash -l` process per host
// and pipes commands through stdin/stdout via the same delimiter-framed
// protocol the Windows path uses. When ControlMaster is available the
// in-process channel itself multiplexes through the socket, so the spawn is
// effectively instant.

#[cfg(not(target_os = "windows"))]
struct UnixSshExecChannel {
    stdin: std::sync::Arc<Mutex<std::process::ChildStdin>>,
    rx: std::sync::mpsc::Receiver<String>,
    child: std::sync::Arc<Mutex<std::process::Child>>,
    /// Set true on Drop so the heartbeat thread exits cleanly.
    shutdown: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Millis since spawn instant; reader thread bumps this on every line
    /// (including ping acks) so the heartbeat can confirm liveness without
    /// stealing bytes from exec()'s receiver.
    last_seen_ms: std::sync::Arc<std::sync::atomic::AtomicU64>,
    spawn_instant: std::time::Instant,
    /// True while an exec() call is in flight. Heartbeat must NOT kill the
    /// channel during chunked file uploads (silent on the remote for several
    /// seconds — looks identical to "wedged shell" from the heartbeat's POV).
    busy: std::sync::Arc<std::sync::atomic::AtomicBool>,
    heartbeat_handle: Option<std::thread::JoinHandle<()>>,
}

#[cfg(not(target_os = "windows"))]
impl UnixSshExecChannel {
    const IDLE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

    fn spawn(profile: &SSHProfile) -> Result<Self, String> {
        use std::io::{BufRead, Read, Write};

        let mut cmd = std::process::Command::new("ssh");
        cmd.args([
            "-T",
            "-o",
            "BatchMode=yes",
            "-o",
            "ServerAliveInterval=30",
            "-o",
            "ServerAliveCountMax=3",
            "-o",
            "ConnectTimeout=15",
            "-o",
            "LogLevel=ERROR",
        ]);

        // Do NOT multiplex over the existing ControlMaster socket here.
        // The master is shared with terminal sessions, claude tails, scp, etc.,
        // and sshd's MaxSessions cap (default 10) routinely refuses new
        // channels on busy HPC hosts ("Session open refused by peer"). The
        // persistent channel pays one TCP+auth handshake at spawn time, then
        // every subsequent command rides the same long-lived bash process.
        cmd.args(["-o", "ControlMaster=no", "-o", "ControlPath=none"]);

        cmd.args(["-p", &profile.port.to_string()]);
        if let Some(key) = &profile.key_file {
            if std::path::Path::new(key).exists() {
                cmd.args(["-i", key]);
            }
        }
        cmd.arg(format!("{}@{}", profile.user, profile.host));
        // Skip login files (~/.bash_profile, ~/.bashrc) — on HPC clusters those
        // do conda init, module load, etc. and can take 15-60s before the shell
        // is responsive. The channel only runs ls/cat/mkdir/base64, all in the
        // default PATH that sshd's PAM session sets up. No user env needed.
        cmd.arg("bash --noprofile --norc");
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

        // Detect early auth failure: give ssh a moment to die, then poll.
        std::thread::sleep(std::time::Duration::from_millis(500));
        match child.try_wait() {
            Ok(Some(status)) => {
                let err_msg = if let Some(mut se) = stderr {
                    let mut buf = String::new();
                    let _ = se.read_to_string(&mut buf);
                    buf
                } else {
                    String::new()
                };
                eprintln!(
                    "[operon-ssh] Unix exec channel auth failed (exit {}): {}",
                    status,
                    err_msg.trim()
                );
                return Err(format!(
                    "SSH auth failed for exec channel (exit {}): {}",
                    status.code().unwrap_or(-1),
                    err_msg.trim()
                ));
            }
            Ok(None) => {
                eprintln!(
                    "[operon-ssh] Unix exec channel opened for {}@{}:{}",
                    profile.user, profile.host, profile.port
                );
            }
            Err(e) => return Err(format!("Failed to check SSH channel status: {}", e)),
        }

        // Stderr drain — without this the pipe buffer can fill and block
        // remote ssh from writing more output.
        if let Some(se) = stderr {
            std::thread::spawn(move || {
                let mut reader = std::io::BufReader::new(se);
                let mut line = String::new();
                loop {
                    line.clear();
                    match reader.read_line(&mut line) {
                        Ok(0) | Err(_) => break,
                        Ok(_) => {
                            let lt = line.trim();
                            if lt.is_empty() {
                                continue;
                            }
                            // Filter the usual OpenSSH noise (post-quantum
                            // warnings, host-key announcements) and only log
                            // signal-bearing lines.
                            if lt.starts_with("Warning: Permanently added")
                                || lt.contains("sntrup")
                                || lt.contains("mlkem")
                            {
                                continue;
                            }
                            eprintln!("[operon-ssh-stderr] {}", lt);
                        }
                    }
                }
            });
        }

        use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
        use std::sync::Arc;

        let spawn_instant = std::time::Instant::now();
        let last_seen_ms = Arc::new(AtomicU64::new(0));
        let shutdown = Arc::new(AtomicBool::new(false));

        let (tx, rx) = std::sync::mpsc::channel::<String>();
        let reader_last_seen = last_seen_ms.clone();
        std::thread::spawn(move || {
            let mut reader = std::io::BufReader::new(stdout);
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        let ms = spawn_instant.elapsed().as_millis() as u64;
                        reader_last_seen.store(ms, Ordering::Relaxed);
                        if tx.send(line.clone()).is_err() {
                            break;
                        }
                    }
                }
            }
        });

        let stdin = Arc::new(Mutex::new(stdin));
        let child = Arc::new(Mutex::new(child));

        let busy = Arc::new(AtomicBool::new(false));

        let mut channel = Self {
            stdin,
            rx,
            child,
            shutdown,
            last_seen_ms,
            spawn_instant,
            busy,
            heartbeat_handle: None,
        };

        let probe_delim = "__OPERON_READY__";
        {
            let mut stdin_guard = channel
                .stdin
                .lock()
                .map_err(|e| format!("stdin lock poisoned: {}", e))?;
            stdin_guard
                .write_all(format!("echo {}\n", probe_delim).as_bytes())
                .map_err(|e| format!("Failed to send probe: {}", e))?;
            stdin_guard
                .flush()
                .map_err(|e| format!("Failed to flush probe: {}", e))?;
        }

        let probe_deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        loop {
            let remaining = probe_deadline
                .checked_duration_since(std::time::Instant::now())
                .unwrap_or_default();
            if remaining.is_zero() {
                return Err("Exec channel probe timed out — shell not responding".to_string());
            }
            match channel.rx.recv_timeout(remaining) {
                Ok(line) => {
                    if line.trim() == probe_delim {
                        break;
                    }
                    // Discard MOTD / login banner lines.
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    return Err("Exec channel probe timed out — shell not responding".to_string());
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    return Err("Exec channel closed during probe".to_string());
                }
            }
        }

        // Heartbeat: every 60s, IF the channel is idle AND has had no output
        // for a while, send `echo __OPERON_PING__` and require a response in
        // 10s. If still nothing, kill the child so the next is_alive() returns
        // false. Skips entirely when busy (chunked uploads are silent for 5-30s
        // by design — looks identical to "wedged shell" from the heartbeat's
        // POV; killing them then is what broke v0.7.5 chat). TCP-level liveness
        // is already handled by ServerAliveInterval=30 on the ssh command line.
        let hb_stdin = channel.stdin.clone();
        let hb_child = channel.child.clone();
        let hb_shutdown = channel.shutdown.clone();
        let hb_last_seen = channel.last_seen_ms.clone();
        let hb_busy = channel.busy.clone();
        let hb_spawn = channel.spawn_instant;
        let hb_handle = std::thread::spawn(move || {
            let interval = std::time::Duration::from_secs(60);
            let response_window = std::time::Duration::from_secs(10);
            let recent_activity_window_ms: u64 = 45_000;
            let tick = std::time::Duration::from_millis(200);
            loop {
                let wake = std::time::Instant::now() + interval;
                while std::time::Instant::now() < wake {
                    if hb_shutdown.load(Ordering::Relaxed) {
                        return;
                    }
                    let now = std::time::Instant::now();
                    let nap = (wake - now).min(tick);
                    std::thread::sleep(nap);
                }
                if hb_shutdown.load(Ordering::Relaxed) {
                    return;
                }

                // Skip ping if an exec call is in flight — chunked uploads
                // produce no remote stdout for seconds at a stretch.
                if hb_busy.load(Ordering::Relaxed) {
                    continue;
                }

                // Skip ping if the channel has emitted any line recently —
                // recent activity already proves liveness; no need to probe.
                let now_ms = hb_spawn.elapsed().as_millis() as u64;
                let last_ms = hb_last_seen.load(Ordering::Relaxed);
                if last_ms > 0 && now_ms.saturating_sub(last_ms) < recent_activity_window_ms {
                    continue;
                }

                let send_ms = now_ms;
                let write_ok = match hb_stdin.lock() {
                    Ok(mut s) => s
                        .write_all(b"echo __OPERON_PING__\n")
                        .and_then(|_| s.flush())
                        .is_ok(),
                    Err(_) => false,
                };
                if !write_ok {
                    eprintln!("[operon-ssh] Heartbeat write failed; killing channel.");
                    if let Ok(mut c) = hb_child.lock() {
                        let _ = c.kill();
                    }
                    return;
                }

                let deadline = std::time::Instant::now() + response_window;
                let mut got_pong = false;
                while std::time::Instant::now() < deadline {
                    if hb_shutdown.load(Ordering::Relaxed) {
                        return;
                    }
                    if hb_last_seen.load(Ordering::Relaxed) >= send_ms {
                        got_pong = true;
                        break;
                    }
                    // If exec() started mid-wait, abandon this probe — don't
                    // kill an actively-working channel just because its ping
                    // happens to be queued behind a slow command.
                    if hb_busy.load(Ordering::Relaxed) {
                        got_pong = true;
                        break;
                    }
                    std::thread::sleep(tick);
                }
                if !got_pong {
                    eprintln!(
                        "[operon-ssh] Heartbeat: no output {}s after ping; killing channel.",
                        response_window.as_secs()
                    );
                    if let Ok(mut c) = hb_child.lock() {
                        let _ = c.kill();
                    }
                    return;
                }
            }
        });
        channel.heartbeat_handle = Some(hb_handle);

        eprintln!("[operon-ssh] Unix exec channel ready (probe OK, heartbeat armed)");
        Ok(channel)
    }

    fn is_alive(&mut self) -> bool {
        match self.child.lock() {
            Ok(mut c) => matches!(c.try_wait(), Ok(None)),
            Err(_) => false,
        }
    }

    /// Run a remote command and return (stdout, exit_code).
    fn exec(&mut self, remote_cmd: &str) -> Result<(String, i32), String> {
        use std::io::Write;

        // RAII guard: heartbeat must not kill this channel mid-call, even if
        // exec returns early via `?`.
        struct BusyGuard<'a>(&'a std::sync::atomic::AtomicBool);
        impl Drop for BusyGuard<'_> {
            fn drop(&mut self) {
                self.0.store(false, std::sync::atomic::Ordering::Relaxed);
            }
        }
        self.busy.store(true, std::sync::atomic::Ordering::Relaxed);
        let _busy_guard = BusyGuard(&self.busy);

        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let delim = format!("__OPERON_DONE_{}_{}__", std::process::id(), ts);

        // Same `{ … } </dev/null 2>&1` wrapper as Windows so commands that
        // would otherwise read stdin (npx prompts, etc.) get an immediate EOF
        // and can't steal the next command from the channel's stdin stream.
        // `echo "{delim}$?"` appends the exit code right after the delimiter
        // so callers can recover non-zero exits without a second round-trip.
        let wrapped = format!(
            "{{ {}\n}} </dev/null 2>&1\necho \"{}$?\"\n",
            remote_cmd, delim
        );

        {
            let mut stdin_guard = self
                .stdin
                .lock()
                .map_err(|e| format!("stdin lock poisoned: {}", e))?;
            stdin_guard.write_all(wrapped.as_bytes()).map_err(|e| {
                format!(
                    "SSH channel write failed (connection may have dropped): {}",
                    e
                )
            })?;
            stdin_guard
                .flush()
                .map_err(|e| format!("SSH channel flush failed: {}", e))?;
        }

        let per_call_timeout = std::time::Duration::from_secs(30);
        let mut output = String::new();
        loop {
            match self
                .rx
                .recv_timeout(per_call_timeout.min(Self::IDLE_TIMEOUT))
            {
                Ok(line) => {
                    // Heartbeat ping ack — never part of exec output.
                    if line.contains("__OPERON_PING__") {
                        continue;
                    }
                    let trimmed = line.trim_end_matches(['\r', '\n']);
                    if let Some(code_str) = trimmed.strip_prefix(delim.as_str()) {
                        let code = code_str.trim().parse::<i32>().unwrap_or(-1);
                        return Ok((output, code));
                    }
                    // Delimiter fused onto a no-trailing-newline final line.
                    if let Some(idx) = trimmed.find(delim.as_str()) {
                        let (prefix, suffix) = trimmed.split_at(idx);
                        let code_str = &suffix[delim.len()..];
                        let code = code_str.trim().parse::<i32>().unwrap_or(-1);
                        output.push_str(prefix);
                        return Ok((output, code));
                    }
                    output.push_str(&line);
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    return Err(format!(
                        "SSH channel stalled — no output for {}s. The connection will be rebuilt.",
                        per_call_timeout.as_secs()
                    ));
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    return Err("SSH channel closed unexpectedly".to_string());
                }
            }
        }
    }
}

#[cfg(not(target_os = "windows"))]
impl Drop for UnixSshExecChannel {
    fn drop(&mut self) {
        self.shutdown
            .store(true, std::sync::atomic::Ordering::Relaxed);
        if let Ok(mut c) = self.child.lock() {
            let _ = c.kill();
        }
        if let Some(h) = self.heartbeat_handle.take() {
            let _ = h.join();
        }
    }
}

#[cfg(not(target_os = "windows"))]
type UnixChannelSlot = std::sync::Arc<Mutex<Option<UnixSshExecChannel>>>;

/// Cap on parallel persistent channels per host. Stays well under the typical
/// sshd MaxSessions=10 (each channel rides its own TCP+auth handshake) while
/// still giving us 3-way parallelism so a slow `ls` on one channel can't gate
/// a fast `cat` waiting behind it.
#[cfg(not(target_os = "windows"))]
// Was 3 — set to 1 because parallel pool spawns triggered sshd MaxStartups
// rate-limiting (server refuses connections after N parallel auth attempts),
// which then cascaded across all profiles. With size=1 the single slot's
// mutex naturally serializes spawn attempts. Cyberduck achieves its speed
// with a single TCP connection too — channel pool is over-engineered here.
const CHANNEL_POOL_SIZE: usize = 1;

#[cfg(not(target_os = "windows"))]
pub(crate) struct UnixChannelPool {
    slots: Mutex<Vec<UnixChannelSlot>>,
    total_calls: std::sync::atomic::AtomicUsize,
    cache_hits: std::sync::atomic::AtomicUsize,
    respawns: std::sync::atomic::AtomicUsize,
    last_error: Mutex<Option<String>>,
}

#[cfg(not(target_os = "windows"))]
impl UnixChannelPool {
    fn new() -> Self {
        Self {
            slots: Mutex::new(Vec::with_capacity(CHANNEL_POOL_SIZE)),
            total_calls: std::sync::atomic::AtomicUsize::new(0),
            cache_hits: std::sync::atomic::AtomicUsize::new(0),
            respawns: std::sync::atomic::AtomicUsize::new(0),
            last_error: Mutex::new(None),
        }
    }

    fn record_error(&self, msg: impl Into<String>) {
        if let Ok(mut g) = self.last_error.lock() {
            *g = Some(msg.into());
        }
    }

    fn reset_stats(&self) {
        use std::sync::atomic::Ordering;
        self.total_calls.store(0, Ordering::Relaxed);
        self.cache_hits.store(0, Ordering::Relaxed);
        self.respawns.store(0, Ordering::Relaxed);
        if let Ok(mut g) = self.last_error.lock() {
            *g = None;
        }
    }

    /// Acquire a slot. Returns the locked slot guard via the slot's Arc — the
    /// caller still has to `.lock()` it (or, in the fast paths, already holds
    /// the lock because we used `try_lock` to pick it). The bool indicates
    /// whether the chosen slot already held a live channel (cache hit).
    ///
    /// Algorithm:
    ///   1. Walk existing slots. First one whose try_lock succeeds AND whose
    ///      channel is alive → cache hit.
    ///   2. No live idle slot: first one whose try_lock succeeds (dead/empty)
    ///      → caller spawns into it. Records a respawn.
    ///   3. All existing slots busy AND slots.len() < CHANNEL_POOL_SIZE: push
    ///      a fresh empty slot and return it.
    ///   4. All slots busy and at cap: block on slot 0's mutex. This is the
    ///      serialization fallback — acceptable under load, never under
    ///      normal interactive use because 3 channels is more than the UI's
    ///      typical concurrency (one file tree + one open file + one chat).
    fn acquire(&self) -> Result<(UnixChannelSlot, bool), String> {
        self.total_calls
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        let mut slots = self
            .slots
            .lock()
            .map_err(|e| format!("pool slots lock poisoned: {}", e))?;

        for slot in slots.iter() {
            if let Ok(mut guard) = slot.try_lock() {
                if let Some(ch) = guard.as_mut() {
                    if ch.is_alive() {
                        drop(guard);
                        self.cache_hits
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        return Ok((slot.clone(), true));
                    }
                }
            }
        }

        for slot in slots.iter() {
            if let Ok(guard) = slot.try_lock() {
                drop(guard);
                return Ok((slot.clone(), false));
            }
        }

        if slots.len() < CHANNEL_POOL_SIZE {
            let fresh: UnixChannelSlot = std::sync::Arc::new(Mutex::new(None));
            slots.push(fresh.clone());
            return Ok((fresh, false));
        }

        let fallback = slots[0].clone();
        Ok((fallback, false))
    }

    fn record_respawn(&self) {
        self.respawns
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
}

#[cfg(not(target_os = "windows"))]
fn unix_channels_map() -> &'static Mutex<HashMap<String, std::sync::Arc<UnixChannelPool>>> {
    use std::sync::{Arc, OnceLock};
    static UNIX_CHANNELS: OnceLock<Mutex<HashMap<String, Arc<UnixChannelPool>>>> = OnceLock::new();
    UNIX_CHANNELS.get_or_init(|| Mutex::new(HashMap::new()))
}

#[cfg(not(target_os = "windows"))]
fn get_unix_channel(profile: &SSHProfile) -> Result<std::sync::Arc<UnixChannelPool>, String> {
    use std::sync::Arc;

    let key = format!("{}@{}:{}", profile.user, profile.host, profile.port);

    let mut map = unix_channels_map()
        .lock()
        .map_err(|e| format!("channel map poisoned: {}", e))?;
    Ok(map
        .entry(key)
        .or_insert_with(|| Arc::new(UnixChannelPool::new()))
        .clone())
}

/// Tracks recent persistent-channel spawn failures per host. A single failure
/// is normal noise (transient socket drop, MFA timeout race); we only trip the
/// oneshot-cooldown after 3 failures inside 5 minutes — that pattern indicates
/// the host is actually unhealthy and we should stop spamming sshd. The first
/// successful spawn clears the count.
#[cfg(not(target_os = "windows"))]
fn unix_channel_failures() -> &'static Mutex<HashMap<String, Vec<std::time::Instant>>> {
    use std::sync::OnceLock;
    static FAILURES: OnceLock<Mutex<HashMap<String, Vec<std::time::Instant>>>> = OnceLock::new();
    FAILURES.get_or_init(|| Mutex::new(HashMap::new()))
}

#[cfg(not(target_os = "windows"))]
const UNIX_CHANNEL_FAIL_WINDOW: std::time::Duration = std::time::Duration::from_secs(300);
#[cfg(not(target_os = "windows"))]
const UNIX_CHANNEL_FAIL_THRESHOLD: usize = 3;
#[cfg(not(target_os = "windows"))]
const UNIX_CHANNEL_COOLDOWN: std::time::Duration = std::time::Duration::from_secs(60);

#[cfg(not(target_os = "windows"))]
fn unix_channel_spawn_blocked(key: &str) -> bool {
    let mut guard = match unix_channel_failures().lock() {
        Ok(g) => g,
        Err(_) => return false,
    };
    let Some(stamps) = guard.get_mut(key) else {
        return false;
    };
    let now = std::time::Instant::now();
    stamps.retain(|t| now.duration_since(*t) < UNIX_CHANNEL_FAIL_WINDOW);
    if stamps.len() < UNIX_CHANNEL_FAIL_THRESHOLD {
        return false;
    }
    // Block while the MOST RECENT failure is still within the cooldown window.
    // (Was: based on `.first()` — that meant once the oldest aged past 60s, the
    // gate opened forever even though new failures kept arriving.)
    stamps
        .last()
        .map(|t| now.duration_since(*t) < UNIX_CHANNEL_COOLDOWN)
        .unwrap_or(false)
}

#[cfg(not(target_os = "windows"))]
fn unix_channel_mark_spawn_failed(key: &str) {
    if let Ok(mut guard) = unix_channel_failures().lock() {
        let now = std::time::Instant::now();
        let stamps = guard.entry(key.to_string()).or_default();
        stamps.retain(|t| now.duration_since(*t) < UNIX_CHANNEL_FAIL_WINDOW);
        stamps.push(now);
    }
}

#[cfg(not(target_os = "windows"))]
fn unix_channel_reset_failures(key: &str) {
    if let Ok(mut guard) = unix_channel_failures().lock() {
        guard.remove(key);
    }
}

// ── Wake-from-sleep recovery ──
//
// After a laptop wake the ControlMaster socket and every persistent channel
// process is still alive from the OS's point of view, but the underlying TCP
// connection is dead. The kernel will eventually surface this as a write
// error, but only after the keepalive probe fails — a several-minute window
// during which every SSH op stalls. We instead detect wake at the source (a
// >30s gap between 5s heartbeat ticks) and proactively tear everything down.

/// Drop every persistent SSH channel across every host, kill ControlMaster
/// sockets for every known profile, and clear the spawn-cooldown map. Called
/// from the wake detector and exposed for tests / manual recovery.
pub fn invalidate_all_unix_channels(profiles: &[SSHProfile]) {
    #[cfg(not(target_os = "windows"))]
    {
        if let Ok(map) = unix_channels_map().lock() {
            for pool in map.values() {
                if let Ok(slots) = pool.slots.lock() {
                    for slot in slots.iter() {
                        if let Ok(mut guard) = slot.lock() {
                            *guard = None;
                        }
                    }
                }
                pool.reset_stats();
            }
        }
        if let Ok(mut guard) = unix_channel_failures().lock() {
            guard.clear();
        }
    }

    if crate::platform::supports_ssh_mux() {
        for profile in profiles {
            let sock = control_socket_path(profile);
            let cmd = format!(
                "ssh -o \"ControlPath={}\" -O exit {}@{} -p {} 2>/dev/null",
                sock, profile.user, profile.host, profile.port
            );
            let _ = crate::platform::shell_exec(&cmd).output();
        }
    }
}

/// Spawn a single OS thread that detects sleep/wake by watching for large
/// gaps in wall-clock time between 5s ticks. On wake, drop every channel +
/// ControlMaster and notify the frontend via `ssh-wake-reconnect`.
pub fn start_wake_detector(app: tauri::AppHandle) {
    // 30s gap = definitely slept; >5s of normal scheduling jitter would never
    // come close. SystemTime is wall-clock so it reflects sleep on every OS;
    // Instant is monotonic on macOS/Linux and may freeze across sleep.
    const TICK: std::time::Duration = std::time::Duration::from_secs(5);
    const WAKE_GAP: std::time::Duration = std::time::Duration::from_secs(30);

    std::thread::spawn(move || {
        let mut last_wall = std::time::SystemTime::now();
        let mut last_mono = std::time::Instant::now();
        loop {
            std::thread::sleep(TICK);
            let now_wall = std::time::SystemTime::now();
            let now_mono = std::time::Instant::now();

            let elapsed = match now_wall.duration_since(last_wall) {
                Ok(d) => d,
                Err(_) => now_mono.duration_since(last_mono),
            };

            if elapsed >= WAKE_GAP {
                eprintln!(
                    "[operon-ssh] Wake detected ({}s gap) — invalidating channels",
                    elapsed.as_secs()
                );
                let profiles: Vec<SSHProfile> = if let Some(state) = app.try_state::<SSHManager>() {
                    state
                        .profiles
                        .lock()
                        .ok()
                        .map(|g| g.clone())
                        .unwrap_or_default()
                } else {
                    Vec::new()
                };
                invalidate_all_unix_channels(&profiles);
                let _ = app.emit("ssh-wake-reconnect", ());
            }

            last_wall = now_wall;
            last_mono = now_mono;
        }
    });
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

/// Reorder the saved SSH profiles to match `ordered_ids` (drag-to-reorder in
/// the Remote SSH panel). The list order *is* the display order — it's just
/// the on-disk `Vec`. Uses a stable sort keyed on the position of each id in
/// `ordered_ids`; any profile not in the list (e.g. a stale frontend) keeps
/// its relative order and sinks to the end, so reordering can never drop a
/// profile.
#[tauri::command]
pub async fn reorder_ssh_profiles(
    state: tauri::State<'_, SSHManager>,
    ordered_ids: Vec<String>,
) -> Result<(), String> {
    let mut profiles = state.profiles.lock().map_err(|e| e.to_string())?;
    profiles.sort_by_key(|p| {
        ordered_ids
            .iter()
            .position(|id| id == &p.id)
            .unwrap_or(usize::MAX)
    });
    save_profiles_to_disk(&profiles)?;
    Ok(())
}

// ── Remote Command Execution (uses ControlMaster when available) ──

/// Run a command on a remote server via SSH.
/// On macOS/Linux: uses ControlMaster socket if active, bypassing re-auth.
/// On Windows: uses a persistent SSH exec channel (single TCP connection reused
/// for all commands — the Windows equivalent of ControlMaster).
pub(crate) fn ssh_exec(profile: &SSHProfile, remote_cmd: &str) -> Result<String, String> {
    #[allow(unused_mut)]
    let mut _has_mux = crate::platform::supports_ssh_mux();

    #[cfg(not(target_os = "windows"))]
    {
        if profile.use_control_master && _has_mux && !control_master_active(profile) {
            if let Ok(true) = ensure_control_master(profile) {
                _has_mux = true;
            }
        }
    }

    // ── Windows: persistent exec channel (replaces ControlMaster) ──
    // Maintains one SSH connection per server and pipes all commands through it.
    // This avoids opening a new TCP connection for every file operation, which
    // triggers rate-limiting on university/HPC SSH servers.
    #[cfg(target_os = "windows")]
    {
        use std::sync::{Arc, OnceLock};

        // Each host gets its own slot behind its own lock. The global map lock
        // is held only long enough to clone a slot's `Arc` — never across SSH
        // I/O — so a stalled connection to one server can no longer block
        // calls to a different server (the cause of "everything times out").
        type ChannelSlot = Arc<Mutex<Option<WinSshExecChannel>>>;
        static WIN_CHANNELS: OnceLock<Mutex<HashMap<String, ChannelSlot>>> = OnceLock::new();
        let map_mutex = WIN_CHANNELS.get_or_init(|| Mutex::new(HashMap::new()));
        let channel_key = format!("{}@{}:{}", profile.user, profile.host, profile.port);

        let slot: ChannelSlot = {
            let mut map = map_mutex.lock().map_err(|e| e.to_string())?;
            map.entry(channel_key.clone())
                .or_insert_with(|| Arc::new(Mutex::new(None)))
                .clone()
        };

        // Per-host lock: serializes calls to the *same* server (one shared
        // pipe), but the IDLE_TIMEOUT inside `exec` guarantees it is always
        // released within a bounded time.
        let mut guard = slot.lock().map_err(|e| e.to_string())?;

        let need_new = match guard.as_mut() {
            Some(ch) => !ch.is_alive(),
            None => true,
        };
        if need_new {
            eprintln!(
                "[operon-ssh] Opening persistent exec channel for {}",
                channel_key
            );
            *guard = Some(WinSshExecChannel::spawn(profile)?);
        }

        match guard.as_mut().unwrap().exec(remote_cmd) {
            Ok(stdout) => return Ok(stdout),
            Err(e) => {
                // Channel stalled or died — drop it (Drop kills the ssh.exe
                // process), then rebuild once and retry.
                eprintln!("[operon-ssh] Exec channel error: {}. Reconnecting...", e);
                *guard = None;
                let mut fresh = WinSshExecChannel::spawn(profile)?;
                let result = fresh.exec(remote_cmd);
                *guard = Some(fresh);
                return result;
            }
        }
    }

    // ── macOS/Linux: persistent in-process channel, fall back to one-shot fork ──
    // Even with ControlMaster active, every `ssh` subprocess pays fork+exec+
    // channel-open (~50-200ms on macOS). The persistent channel keeps one
    // `ssh ... bash -l` alive per host and pipes commands through it, mirroring
    // the Windows path.
    #[cfg(not(target_os = "windows"))]
    {
        let channel_key = format!("{}@{}:{}", profile.user, profile.host, profile.port);

        // If a recent spawn failed for this host, skip the channel path
        // entirely for the cooldown window — don't keep hammering sshd
        // with reconnect attempts that will just time out again.
        if unix_channel_spawn_blocked(&channel_key) {
            return ssh_exec_oneshot(profile, remote_cmd, _has_mux);
        }

        let pool = match get_unix_channel(profile) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("[operon-ssh] channel pool unavailable: {}", e);
                return ssh_exec_oneshot(profile, remote_cmd, _has_mux);
            }
        };

        let (slot, _was_hit) = match pool.acquire() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[operon-ssh] pool acquire failed: {}", e);
                return ssh_exec_oneshot(profile, remote_cmd, _has_mux);
            }
        };

        let mut guard = match slot.lock() {
            Ok(g) => g,
            Err(e) => {
                eprintln!("[operon-ssh] channel lock poisoned: {}", e);
                return ssh_exec_oneshot(profile, remote_cmd, _has_mux);
            }
        };

        let need_new = match guard.as_mut() {
            Some(ch) => !ch.is_alive(),
            None => true,
        };
        if need_new {
            eprintln!(
                "[operon-ssh] Opening persistent exec channel for {}",
                channel_key
            );
            match UnixSshExecChannel::spawn(profile) {
                Ok(ch) => {
                    *guard = Some(ch);
                    pool.record_respawn();
                    unix_channel_reset_failures(&channel_key);
                }
                Err(e) => {
                    unix_channel_mark_spawn_failed(&channel_key);
                    pool.record_error(&e);
                    let blocked_now = unix_channel_spawn_blocked(&channel_key);
                    eprintln!(
                        "[operon-ssh] Persistent channel spawn failed ({}); {}",
                        e,
                        if blocked_now {
                            "cooldown tripped — falling back to per-call ssh for 60s"
                        } else {
                            "will retry on next call"
                        }
                    );
                    drop(guard);
                    return ssh_exec_oneshot(profile, remote_cmd, _has_mux);
                }
            }
        }

        // First attempt on the (possibly long-lived) channel.
        let first = guard.as_mut().unwrap().exec(remote_cmd);
        match first {
            Ok((stdout, exit_code)) => {
                if exit_code != 0 && stdout.trim().is_empty() {
                    return Err(format!(
                        "SSH command failed (exit code {}). This may be a transient connection issue — try clicking Retry.",
                        exit_code
                    ));
                }
                Ok(stdout)
            }
            Err(e) => {
                // Channel died mid-call (heartbeat killed it, sshd MaxSessions,
                // network blip). Drop, respawn synchronously, replay the same
                // command once. Only if THAT also fails do we mark the host
                // unhealthy and fall back to the per-call oneshot path.
                eprintln!(
                    "[operon-ssh] Exec channel error: {}. Respawning channel and retrying once.",
                    e
                );
                *guard = None;
                match UnixSshExecChannel::spawn(profile) {
                    Ok(fresh) => {
                        unix_channel_reset_failures(&channel_key);
                        *guard = Some(fresh);
                        pool.record_respawn();
                        match guard.as_mut().unwrap().exec(remote_cmd) {
                            Ok((stdout, exit_code)) => {
                                if exit_code != 0 && stdout.trim().is_empty() {
                                    return Err(format!(
                                        "SSH command failed (exit code {}). This may be a transient connection issue — try clicking Retry.",
                                        exit_code
                                    ));
                                }
                                Ok(stdout)
                            }
                            Err(e2) => {
                                eprintln!(
                                    "[operon-ssh] Retry on fresh channel failed: {}. Falling back to one-shot.",
                                    e2
                                );
                                *guard = None;
                                unix_channel_mark_spawn_failed(&channel_key);
                                pool.record_error(&e2);
                                drop(guard);
                                ssh_exec_oneshot(profile, remote_cmd, _has_mux)
                            }
                        }
                    }
                    Err(spawn_err) => {
                        eprintln!(
                            "[operon-ssh] Respawn after channel error failed: {}. Falling back to one-shot.",
                            spawn_err
                        );
                        unix_channel_mark_spawn_failed(&channel_key);
                        pool.record_error(&spawn_err);
                        drop(guard);
                        ssh_exec_oneshot(profile, remote_cmd, _has_mux)
                    }
                }
            }
        }
    }

    // Unreachable on non-Windows, but needed for Windows cfg where the function
    // returns early from the #[cfg(target_os = "windows")] block above.
    #[cfg(target_os = "windows")]
    #[allow(unreachable_code)]
    {
        unreachable!()
    }
}

/// Per-call fork fallback for macOS/Linux. Used when the persistent channel
/// can't be opened (auth fail, network down) or when an in-flight call errors
/// out. Preserves the original `shell_exec` based code path verbatim so the
/// behaviour matches what users had before the channel optimisation landed.
#[cfg(not(target_os = "windows"))]
fn ssh_exec_oneshot(
    profile: &SSHProfile,
    remote_cmd: &str,
    has_mux: bool,
) -> Result<String, String> {
    let output = {
        let mut ssh_args = if has_mux {
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
                ssh_args.push_str(&format!(" -i '{}'", key.replace('\'', "'\\''")));
            }
        }
        ssh_args.push_str(&format!(" -- {}", shell_escape(remote_cmd)));

        crate::platform::shell_exec(&ssh_args)
            .output()
            .map_err(|e| format!("Failed to run SSH: {}", e))?
    };

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

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

    if !output.status.success() && stdout.trim().is_empty() {
        let mux_active = control_master_active(profile);
        let sock_path = control_socket_path(profile);

        if has_mux && !mux_active {
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

    Ok(stdout)
}

/// Async wrapper around the blocking [`ssh_exec`]. Runs the SSH call on a
/// dedicated blocking thread (`spawn_blocking`) so a slow or stalled connection
/// cannot pin an async runtime worker — which, multiplied across concurrent
/// file-browser and Claude calls, would otherwise freeze all IPC.
pub(crate) async fn ssh_exec_async(
    profile: SSHProfile,
    remote_cmd: String,
) -> Result<String, String> {
    tokio::task::spawn_blocking(move || ssh_exec(&profile, &remote_cmd))
        .await
        .map_err(|e| format!("SSH task failed to run: {}", e))?
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

    let ls_flag = if show_hidden { "-lLA" } else { "-lL" };
    let cmd = format!("ls {} {} 2>/dev/null", ls_flag, shell_escape_inner(&path));

    let output = ssh_exec_async(profile, cmd).await?;

    let base_path = if path.ends_with('/') {
        path.clone()
    } else {
        format!("{}/", path)
    };

    let mut entries: Vec<FileEntry> = Vec::new();

    for line in output.lines() {
        let line = line.trim_end();
        if line.is_empty() || line.starts_with("total ") {
            continue;
        }

        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 9 {
            continue;
        }

        let perms = fields[0];
        let size = fields[4].parse::<u64>().unwrap_or(0);
        let raw_name = fields[8..].join(" ");

        let name_no_link = raw_name.split(" -> ").next().unwrap_or(&raw_name);
        let name = name_no_link
            .trim_end_matches(['/', '*', '@', '=', '|'])
            .to_string();

        if name.is_empty() || name == "." || name == ".." {
            continue;
        }

        let is_dir = perms.starts_with('d');
        let full_path = format!("{}{}", base_path, name);

        let extension = if !is_dir {
            name.rsplit('.')
                .next()
                .and_then(|e| if e != name { Some(e.to_string()) } else { None })
        } else {
            None
        };

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

    let output = ssh_exec_async(profile, "echo $HOME".to_string()).await?;
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

    let content = ssh_exec_async(profile, format!("cat {}", shell_escape_inner(&path))).await?;
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

    let output = ssh_exec_async(profile, format!("base64 {}", shell_escape_inner(&path))).await?;
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
    state.cache.invalidate_path(&profile_id, &path);
    Ok(())
}

/// Delete a batch of remote paths in a single SSH round-trip.
/// Uses one compound `rm -rf` so it handles a mix of files and directories
/// without per-path stat probes. Returns the number of paths whose removal
/// the remote shell reported as successful (best-effort: rm exits 0 if all
/// args were removed; on any failure we fall back to a per-path probe).
#[tauri::command]
pub async fn batch_delete_remote_files(
    state: tauri::State<'_, SSHManager>,
    profile_id: String,
    paths: Vec<String>,
) -> Result<usize, String> {
    if paths.is_empty() {
        return Ok(0);
    }

    let profile = {
        let profiles = state.profiles.lock().map_err(|e| e.to_string())?;
        profiles
            .iter()
            .find(|p| p.id == profile_id)
            .cloned()
            .ok_or_else(|| format!("SSH profile {} not found", profile_id))?
    };

    let escaped: Vec<String> = paths.iter().map(|p| shell_escape_inner(p)).collect();
    let bulk_cmd = format!("rm -rf {}; echo $?", escaped.join(" "));
    let result = ssh_exec_async(profile.clone(), bulk_cmd).await?;
    let exit = result.trim().lines().last().unwrap_or("1").trim();

    let succeeded = if exit == "0" {
        paths.len()
    } else {
        // Per-path verification — count any paths that no longer exist.
        let probes: Vec<String> = paths
            .iter()
            .map(|p| format!("[ -e {} ] || echo MISSING", shell_escape_inner(p)))
            .collect();
        let probe_out = ssh_exec_async(profile, probes.join("; ")).await?;
        probe_out.lines().filter(|l| l.trim() == "MISSING").count()
    };

    for p in &paths {
        state.cache.invalidate_path(&profile_id, p);
    }
    Ok(succeeded)
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
    state.cache.invalidate_path(&profile_id, &old_path);
    state.cache.invalidate_path(&profile_id, &new_path);
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
    // Use the same ControlMaster socket the interactive SSH terminal creates
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

    // Use the same ControlMaster socket the interactive SSH terminal creates
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

    // Use the same ControlMaster socket the interactive SSH terminal creates
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

// ── SFTP downloads with byte-level progress ──
//
// scp_from_remote / scp_dir_from_remote shell out to the system `scp`
// binary, which gives us no progress signal. For downloads where the user
// wants to see real progress (large files or directories from HPC), we use
// the ssh2 crate's SFTP client to read in 64KB chunks and emit one
// `scp-transfer-progress` event per chunk with byte-level counters.

fn open_ssh2_session(profile: &SSHProfile) -> Result<ssh2::Session, String> {
    let addr = format!("{}:{}", profile.host, profile.port);
    let tcp = std::net::TcpStream::connect_timeout(
        &addr
            .to_socket_addrs()
            .map_err(|e| format!("resolve {}: {}", addr, e))?
            .next()
            .ok_or_else(|| format!("no address for {}", addr))?,
        std::time::Duration::from_secs(15),
    )
    .map_err(|e| format!("connect {}: {}", addr, e))?;
    let _ = tcp.set_read_timeout(Some(std::time::Duration::from_secs(120)));
    let _ = tcp.set_write_timeout(Some(std::time::Duration::from_secs(120)));

    let mut sess = ssh2::Session::new().map_err(|e| format!("ssh2 new: {}", e))?;
    sess.set_tcp_stream(tcp);
    sess.handshake().map_err(|e| format!("handshake: {}", e))?;

    // Try the explicit key file first.
    if let Some(key_path) = &profile.key_file {
        let key = std::path::Path::new(key_path);
        if key.exists() {
            let _ = sess.userauth_pubkey_file(&profile.user, None, key, None);
        }
    }

    // Fall back to ssh-agent identities.
    if !sess.authenticated() {
        if let Ok(mut agent) = sess.agent() {
            if agent.connect().is_ok() && agent.list_identities().is_ok() {
                if let Ok(idents) = agent.identities() {
                    for ident in idents {
                        if agent.userauth(&profile.user, &ident).is_ok() && sess.authenticated() {
                            break;
                        }
                    }
                }
            }
        }
    }

    if !sess.authenticated() {
        return Err(
            "SSH authentication failed (no key matched and ssh-agent had no usable identity)"
                .to_string(),
        );
    }

    Ok(sess)
}

fn emit_progress(app: &tauri::AppHandle, completed: u64, total: u64, current_file: &str) {
    let _ = app.emit(
        "scp-transfer-progress",
        serde_json::json!({
            "completed": completed,
            "total": total,
            "current_file": current_file,
            "errors": 0,
            "status": "downloading",
        }),
    );
}

/// Download a single remote file via SFTP, emitting byte-level progress.
#[tauri::command]
pub async fn sftp_download_with_progress(
    app: tauri::AppHandle,
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

    tokio::task::spawn_blocking(move || -> Result<(), String> {
        let sess = open_ssh2_session(&profile)?;
        let sftp = sess.sftp().map_err(|e| format!("sftp open: {}", e))?;

        let remote = std::path::Path::new(&remote_path);
        let stat = sftp
            .stat(remote)
            .map_err(|e| format!("stat {}: {}", remote_path, e))?;
        let total = stat.size.unwrap_or(0);
        let name = remote
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file")
            .to_string();

        if let Some(parent) = std::path::Path::new(&local_path).parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let mut remote_file = sftp
            .open(remote)
            .map_err(|e| format!("open remote: {}", e))?;
        let mut local_file = std::fs::File::create(&local_path)
            .map_err(|e| format!("create {}: {}", local_path, e))?;

        emit_progress(&app, 0, total, &name);
        let mut buf = vec![0u8; 64 * 1024];
        let mut completed: u64 = 0;
        let mut last_emit = std::time::Instant::now();
        loop {
            let n = std::io::Read::read(&mut remote_file, &mut buf)
                .map_err(|e| format!("read: {}", e))?;
            if n == 0 {
                break;
            }
            std::io::Write::write_all(&mut local_file, &buf[..n])
                .map_err(|e| format!("write: {}", e))?;
            completed += n as u64;
            if last_emit.elapsed().as_millis() > 33 {
                last_emit = std::time::Instant::now();
                emit_progress(&app, completed, total, &name);
            }
        }
        emit_progress(&app, completed, total.max(completed), &name);
        Ok(())
    })
    .await
    .map_err(|e| format!("join: {}", e))?
}

fn sftp_walk_collect(
    sftp: &ssh2::Sftp,
    remote_root: &std::path::Path,
    rel: &std::path::Path,
    out: &mut Vec<(std::path::PathBuf, u64)>,
) -> Result<(), String> {
    let path = if rel.as_os_str().is_empty() {
        remote_root.to_path_buf()
    } else {
        remote_root.join(rel)
    };
    let entries = sftp
        .readdir(&path)
        .map_err(|e| format!("readdir {:?}: {}", path, e))?;
    for (entry_path, stat) in entries {
        let name = entry_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");
        if name.is_empty() || name == "." || name == ".." {
            continue;
        }
        let new_rel = if rel.as_os_str().is_empty() {
            std::path::PathBuf::from(name)
        } else {
            rel.join(name)
        };
        if stat.is_dir() {
            sftp_walk_collect(sftp, remote_root, &new_rel, out)?;
        } else {
            out.push((new_rel, stat.size.unwrap_or(0)));
        }
    }
    Ok(())
}

/// Download a remote directory tree via SFTP. First walks the tree to sum
/// total bytes, then streams files with cumulative byte progress so the UI
/// can show a single accurate progress bar.
#[tauri::command]
pub async fn sftp_dir_download_with_progress(
    app: tauri::AppHandle,
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

    tokio::task::spawn_blocking(move || -> Result<(), String> {
        let sess = open_ssh2_session(&profile)?;
        let sftp = sess.sftp().map_err(|e| format!("sftp open: {}", e))?;

        let remote_root = std::path::PathBuf::from(&remote_path);
        let local_root = std::path::PathBuf::from(&local_path);
        let root_name = remote_root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("directory")
            .to_string();

        let mut files: Vec<(std::path::PathBuf, u64)> = Vec::new();
        sftp_walk_collect(&sftp, &remote_root, std::path::Path::new(""), &mut files)?;
        let total: u64 = files.iter().map(|(_, sz)| *sz).sum();

        std::fs::create_dir_all(&local_root)
            .map_err(|e| format!("create {}: {}", local_path, e))?;

        emit_progress(&app, 0, total, &root_name);
        let mut buf = vec![0u8; 64 * 1024];
        let mut completed: u64 = 0;
        let mut last_emit = std::time::Instant::now();

        for (rel, _) in &files {
            let remote_file_path = remote_root.join(rel);
            let local_file_path = local_root.join(rel);
            if let Some(parent) = local_file_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }

            let mut remote_file = sftp
                .open(&remote_file_path)
                .map_err(|e| format!("open {:?}: {}", remote_file_path, e))?;
            let mut local_file = std::fs::File::create(&local_file_path)
                .map_err(|e| format!("create {:?}: {}", local_file_path, e))?;
            let current_name = rel.to_string_lossy().to_string();

            loop {
                let n = std::io::Read::read(&mut remote_file, &mut buf)
                    .map_err(|e| format!("read: {}", e))?;
                if n == 0 {
                    break;
                }
                std::io::Write::write_all(&mut local_file, &buf[..n])
                    .map_err(|e| format!("write: {}", e))?;
                completed += n as u64;
                if last_emit.elapsed().as_millis() > 33 {
                    last_emit = std::time::Instant::now();
                    emit_progress(&app, completed, total, &current_name);
                }
            }
        }
        emit_progress(&app, completed, total.max(completed), &root_name);
        Ok(())
    })
    .await
    .map_err(|e| format!("join: {}", e))?
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
    // Use the same ControlMaster socket the interactive SSH terminal creates
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

    // ControlMaster is a Unix-domain-socket feature — unsupported on Windows,
    // where no master socket is ever created. Skip the no-op `ssh -O exit`.
    if crate::platform::supports_ssh_mux() {
        let sock = control_socket_path(&profile);
        let cmd = format!(
            "ssh -o \"ControlPath={}\" -O exit {}@{} -p {} 2>/dev/null",
            sock, profile.user, profile.host, profile.port
        );
        let _ = crate::platform::shell_exec(&cmd).output();
    }

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
    let mut seen_aliases: std::collections::HashSet<String> = std::collections::HashSet::new();
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
    let flush = |cur: &mut Vec<SSHConfigHost>, hosts: &mut Vec<SSHConfigHost>| {
        if !cur.is_empty() {
            hosts.append(cur);
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
                    parse_ssh_config_file(&include_path, home, hosts, visited, depth + 1);
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

    if !absolute.to_string_lossy().contains('*') && !absolute.to_string_lossy().contains('?') {
        return if absolute.exists() {
            vec![absolute]
        } else {
            vec![]
        };
    }

    // Only handle a wildcard in the final path component (the common case).
    let parent = absolute
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));
    let pattern = absolute.file_name().and_then(|n| n.to_str()).unwrap_or("");
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

// ── Connection Diagnostics ──
//
// Reports live channel-pool stats so the StatusBar pill can show a green/amber/red
// health dot. Walks the pool with try_lock so a busy channel never blocks the
// poll, and times one quick `echo OK` for RTT on the first idle alive channel.

#[derive(serde::Serialize, Clone)]
pub struct SshDiagnostics {
    pub rtt_ms: Option<u64>,
    pub channels_alive: usize,
    pub channels_total: usize,
    pub pool_max: usize,
    pub total_calls: usize,
    pub cache_hits: usize,
    pub respawns: usize,
    pub in_cooldown: bool,
    pub last_error: Option<String>,
}

#[tauri::command]
pub async fn get_ssh_diagnostics(
    state: tauri::State<'_, SSHManager>,
    profile_id: String,
) -> Result<SshDiagnostics, String> {
    let profile = {
        let profiles = state.profiles.lock().map_err(|e| e.to_string())?;
        profiles
            .iter()
            .find(|p| p.id == profile_id)
            .cloned()
            .ok_or_else(|| format!("SSH profile {} not found", profile_id))?
    };

    #[cfg(target_os = "windows")]
    {
        // Windows uses a single per-host slot, not a pool; report a minimal
        // snapshot so the UI pill still renders.
        let _ = profile; // unused on Windows for now
        return Ok(SshDiagnostics {
            rtt_ms: None,
            channels_alive: 0,
            channels_total: 0,
            pool_max: 1,
            total_calls: 0,
            cache_hits: 0,
            respawns: 0,
            in_cooldown: false,
            last_error: None,
        });
    }

    #[cfg(not(target_os = "windows"))]
    {
        use std::sync::atomic::Ordering;

        let channel_key = format!("{}@{}:{}", profile.user, profile.host, profile.port);
        let pool = get_unix_channel(&profile)?;

        let total_calls = pool.total_calls.load(Ordering::Relaxed);
        let cache_hits = pool.cache_hits.load(Ordering::Relaxed);
        let respawns = pool.respawns.load(Ordering::Relaxed);
        let in_cooldown = unix_channel_spawn_blocked(&channel_key);
        let last_error = pool.last_error.lock().ok().and_then(|g| g.clone());

        // Snapshot slots + RTT probe. We hold the slots lock only long enough
        // to clone the Arcs, then walk them with try_lock so a stalled
        // channel can't block the poll.
        let slot_arcs: Vec<UnixChannelSlot> = {
            let slots = pool
                .slots
                .lock()
                .map_err(|e| format!("pool slots lock poisoned: {}", e))?;
            slots.clone()
        };

        let channels_total = slot_arcs.len();
        let mut channels_alive = 0usize;
        let mut rtt_ms: Option<u64> = None;

        for slot in &slot_arcs {
            // Spend only the RTT probe budget on the *first* idle alive channel
            // so the poll itself stays cheap (<1ms when nothing is alive).
            if let Ok(mut guard) = slot.try_lock() {
                if let Some(ch) = guard.as_mut() {
                    if ch.is_alive() {
                        channels_alive += 1;
                        if rtt_ms.is_none() {
                            let start = std::time::Instant::now();
                            if let Ok((_out, code)) = ch.exec("echo OK") {
                                if code == 0 {
                                    rtt_ms = Some(start.elapsed().as_millis() as u64);
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(SshDiagnostics {
            rtt_ms,
            channels_alive,
            channels_total,
            pool_max: CHANNEL_POOL_SIZE,
            total_calls,
            cache_hits,
            respawns,
            in_cooldown,
            last_error,
        })
    }
}

/// Reset diagnostic counters and tear down all persistent channels for a
/// profile so the next call rebuilds from scratch. Used by the StatusBar
/// "Reconnect" / "Reset stats" actions.
#[tauri::command]
pub async fn reset_ssh_diagnostics(
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

    #[cfg(not(target_os = "windows"))]
    {
        let channel_key = format!("{}@{}:{}", profile.user, profile.host, profile.port);
        let pool = get_unix_channel(&profile)?;
        pool.reset_stats();
        unix_channel_reset_failures(&channel_key);

        // Drop every live channel so the next exec rebuilds. We grab each
        // slot's mutex without try_lock so an in-flight call gets to finish
        // first — a fresh respawn under our feet would be worse than waiting.
        let slot_arcs: Vec<UnixChannelSlot> = {
            let slots = pool
                .slots
                .lock()
                .map_err(|e| format!("pool slots lock poisoned: {}", e))?;
            slots.clone()
        };
        for slot in slot_arcs {
            if let Ok(mut guard) = slot.lock() {
                *guard = None;
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        let _ = profile;
    }

    Ok(())
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
