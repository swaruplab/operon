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
    /// Whether `key_file` is passphrase-protected. When true, Operon loads the
    /// key into an ssh-agent at connect time using the passphrase stored in the
    /// OS keychain (see commands/sshauth.rs). The passphrase itself is NEVER
    /// stored in this struct (it would be serialized to plaintext JSON).
    #[serde(default)]
    pub key_has_passphrase: bool,
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

/// Returns the ControlMaster socket path only if a LIVE master exists for this
/// profile (file present AND `ssh -O check` passes). None on Windows or if dead.
///
/// Attaching `-o ControlPath=` to scp whenever the socket FILE merely exists is
/// unsafe: a stale socket (master died) makes scp silently re-authenticate or
/// hang. Callers should use this to decide whether to pass ControlPath at all.
pub fn live_control_socket(profile: &SSHProfile) -> Option<std::path::PathBuf> {
    if !crate::platform::supports_ssh_mux() {
        return None;
    }
    let sock = crate::platform::ssh_socket_path(&profile.host, profile.port, &profile.user);
    if sock.exists() && control_master_active(profile) {
        Some(sock)
    } else {
        None
    }
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

    // The cold-start master runs under BatchMode=yes and cannot prompt for a
    // passphrase — so unlock the key into the agent first.
    crate::commands::sshauth::ensure_key_loaded(profile);

    let sock = control_socket_path(profile);
    // Match the ControlPersist of the interactive/mux masters (ssh_mux_args) so
    // a cold-start master and an interactive master persist identically.
    let mut cmd = format!(
        "ssh -M -N -f \
         -o ControlMaster=yes \
         -o ControlPath='{}' \
         -o ControlPersist={} \
         -o ServerAliveInterval=30 \
         -o ServerAliveCountMax=3 \
         -o ConnectTimeout=15 \
         -o BatchMode=yes",
        sock.replace('\'', "'\\''"),
        crate::platform::SSH_CONTROL_PERSIST
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

    // shell_exec runs this through a LOGIN shell ($SHELL -l -c), which sources
    // the user's rc first — a line like `export SSH_AUTH_SOCK=...` there would
    // override the agent ensure_key_loaded just populated. Prefix an explicit
    // assignment so this one ssh invocation pins Operon's agent regardless.
    if let Some(sock) = crate::commands::sshauth::current_auth_sock() {
        cmd = format!("SSH_AUTH_SOCK='{}' {}", sock.replace('\'', "'\\''"), cmd);
    }

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

/// Preflight for non-interactive (BatchMode) background callers — cluster job
/// queries, the legacy-daemon sweep: confirm we can reach `profile` WITHOUT an
/// interactive prompt, or return an actionable error.
///
/// These SSH calls run under `BatchMode=yes`, which disables keyboard-interactive
/// auth. On a Duo/MFA cluster that can only succeed by riding a ControlMaster the
/// user already authenticated from an interactive terminal. Without this
/// preflight the failure surfaces as an opaque "transient SSH error"; with it we
/// can tell the user exactly what to do. Returns `Ok` when a live master exists
/// (or we can cold-start one with a key), and a human-readable `Err` otherwise.
/// No-op (Ok) on Windows, whose persistent `WinSshExecChannel` manages its own
/// connection lifecycle.
pub(crate) fn ensure_live_connection(profile: &SSHProfile) -> Result<(), String> {
    if !crate::platform::supports_ssh_mux() {
        return Ok(());
    }
    // Load a stored-passphrase key into the agent first (no-op for keys without
    // a passphrase / already loaded) so a key-only cold start can authenticate.
    crate::commands::sshauth::ensure_key_loaded(profile);
    if live_control_socket(profile).is_some() {
        return Ok(());
    }
    if profile.use_control_master {
        if let Ok(true) = ensure_control_master(profile) {
            return Ok(());
        }
    }
    Err(format!(
        "No live SSH connection to {}. Open an SSH terminal to this host and \
         complete login (e.g. Duo) — the HPC job watchdog reuses your terminal's \
         already-authenticated connection.",
        profile.host
    ))
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

/// Return the canonical ControlMaster socket path for a connection. The frontend
/// MUST use this for the interactive terminal's ControlPath so it matches
/// exactly where the backend looks for the live master — otherwise the explorer
/// can't reuse the connection the terminal authenticated (the literal-vs-hashed
/// filename mismatch that broke passphrase/Duo reuse).
#[tauri::command]
pub fn get_ssh_socket_path(host: String, port: u16, user: String) -> String {
    crate::platform::ssh_socket_path(&host, port, &user)
        .to_string_lossy()
        .to_string()
}

/// Pre-load a profile's passphrase-protected key into the ssh-agent before
/// connecting, so the terminal and the file explorer both authenticate without
/// prompting. No-op for keys without a passphrase.
#[tauri::command]
pub async fn prepare_ssh_auth(
    state: tauri::State<'_, SSHManager>,
    profile_id: String,
) -> Result<(), String> {
    let profile = {
        let profiles = state.profiles.lock().map_err(|e| e.to_string())?;
        profiles.iter().find(|p| p.id == profile_id).cloned()
    };
    if let Some(p) = profile {
        tokio::task::spawn_blocking(move || crate::commands::sshauth::ensure_key_loaded(&p))
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ── Windows Persistent SSH Exec Channel ──
// On macOS/Linux, ControlMaster multiplexes all SSH commands through one TCP connection.
// Windows doesn't support ControlMaster, so university servers that rate-limit SSH
// connections will reject rapid-fire file browsing commands. This provides the equivalent:
// a single persistent SSH process with commands piped through stdin/stdout.

/// Frame a remote command for a persistent exec channel. Shared by the Windows
/// and Unix channels so their wire protocol can never drift apart.
///
/// Three properties matter, all encoded here:
///  1. The command runs in a SUBSHELL `( … )`, never a brace group `{ … }`, so
///     an `exit` inside `remote_cmd` (our own auth/install scripts call it, and
///     sourced profiles may too) exits only the subshell — not the long-lived
///     parent bash that *is* the channel.
///  2. Stdin is redirected from `/dev/null` so a command that reads stdin (most
///     notably `npx`'s "Ok to proceed?") gets an immediate EOF and can't swallow
///     the next command plus the delimiter from the channel's stdin stream.
///  3. `echo "{delim}$?"` prints a per-call nonce delimiter fused with the
///     subshell's exit code, so the reader knows exactly where output ends and
///     can recover non-zero exits without a second round-trip. The `)` sits on
///     its own line so a trailing comment in `remote_cmd` can't absorb it.
fn wrap_remote_cmd(remote_cmd: &str, delim: &str) -> String {
    format!(
        "( {}\n) </dev/null 2>&1\necho \"{}$?\"\n",
        remote_cmd, delim
    )
}

/// Standard "transient SSH failure" message. A non-zero exit with no usable
/// stdout is almost always a connection/auth blip rather than a real command
/// failure, so we tell the user to retry. Shared by every channel/one-shot path
/// so the wording and heuristic can't drift between platforms.
fn ssh_transient_error(code: i32) -> String {
    format!(
        "SSH command failed (exit code {}). This may be a transient connection issue — try clicking Retry.",
        code
    )
}

/// Drop the benign noise OpenSSH writes to stderr (host-key add notices,
/// post-quantum kex warnings, debug lines) so only real errors survive. Shared
/// by the macOS/Linux and Windows one-shot paths.
fn filter_ssh_stderr(stderr: &str) -> String {
    stderr
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
        .join("\n")
}

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
        // Start a NON-login, NON-rc shell, matching the Unix channel
        // (`bash --noprofile --norc`). Login/profile startup on HPC hosts runs
        // conda init, `module load`s, and other side effects that are slow,
        // emit stray stdout into the channel, and — worst — may call `exit`,
        // which (before the subshell wrapper) killed the channel. PATH-sensitive
        // commands (claude/node/npx) export PATH explicitly in their own scripts.
        cmd.arg("bash --noprofile --norc");
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

    fn exec(&mut self, remote_cmd: &str) -> Result<(String, i32), String> {
        use std::io::Write;

        // Use a unique delimiter that won't appear in command output
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let delim = format!("__OPERON_DONE_{}_{}__", std::process::id(), ts);

        // Subshell-framed so a remote `exit` can't kill this channel; see
        // `wrap_remote_cmd` for the full rationale.
        let wrapped = wrap_remote_cmd(remote_cmd, &delim);

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
        // The delimiter line carries the exit code as its suffix (`{delim}$?`).
        let mut output = String::new();
        loop {
            match self.rx.recv_timeout(Self::IDLE_TIMEOUT) {
                Ok(line) => {
                    let trimmed = line.trim_end_matches(['\r', '\n']);
                    // Delimiter on its own line: the suffix is the exit code.
                    if let Some(code_str) = trimmed.strip_prefix(delim.as_str()) {
                        let code = code_str.trim().parse::<i32>().unwrap_or(-1);
                        return Ok((output, code));
                    }
                    // If the command's final line lacked a trailing newline,
                    // `echo` fuses the delimiter onto it. The delimiter is a
                    // per-call nonce, so an inner match unambiguously means
                    // "end of output" — keep the real prefix, parse the code.
                    if let Some(idx) = trimmed.find(delim.as_str()) {
                        let (prefix, suffix) = trimmed.split_at(idx);
                        let code = suffix[delim.len()..].trim().parse::<i32>().unwrap_or(-1);
                        output.push_str(prefix);
                        return Ok((output, code));
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

        // Identical framing to the Windows channel; see `wrap_remote_cmd`.
        let wrapped = wrap_remote_cmd(remote_cmd, &delim);

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
fn channel_failures() -> &'static Mutex<HashMap<String, Vec<std::time::Instant>>> {
    use std::sync::OnceLock;
    static FAILURES: OnceLock<Mutex<HashMap<String, Vec<std::time::Instant>>>> = OnceLock::new();
    FAILURES.get_or_init(|| Mutex::new(HashMap::new()))
}

const CHANNEL_FAIL_WINDOW: std::time::Duration = std::time::Duration::from_secs(300);
const CHANNEL_FAIL_THRESHOLD: usize = 3;
const CHANNEL_COOLDOWN: std::time::Duration = std::time::Duration::from_secs(60);

fn channel_spawn_blocked(key: &str) -> bool {
    let mut guard = match channel_failures().lock() {
        Ok(g) => g,
        Err(_) => return false,
    };
    let Some(stamps) = guard.get_mut(key) else {
        return false;
    };
    let now = std::time::Instant::now();
    stamps.retain(|t| now.duration_since(*t) < CHANNEL_FAIL_WINDOW);
    if stamps.len() < CHANNEL_FAIL_THRESHOLD {
        return false;
    }
    // Block while the MOST RECENT failure is still within the cooldown window.
    // (Was: based on `.first()` — that meant once the oldest aged past 60s, the
    // gate opened forever even though new failures kept arriving.)
    stamps
        .last()
        .map(|t| now.duration_since(*t) < CHANNEL_COOLDOWN)
        .unwrap_or(false)
}

fn channel_mark_spawn_failed(key: &str) {
    if let Ok(mut guard) = channel_failures().lock() {
        let now = std::time::Instant::now();
        let stamps = guard.entry(key.to_string()).or_default();
        stamps.retain(|t| now.duration_since(*t) < CHANNEL_FAIL_WINDOW);
        stamps.push(now);
    }
}

fn channel_reset_failures(key: &str) {
    if let Ok(mut guard) = channel_failures().lock() {
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
        if let Ok(mut guard) = channel_failures().lock() {
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
    // Don't orphan the key passphrase in the OS keychain (no-op if none stored).
    let _ = crate::commands::sshauth::delete_passphrase(&profile_id);
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
/// Outcome of one remote command, as reported by whichever transport ran it.
///
/// `Err` from [`ssh_exec_status`] is reserved for TRANSPORT failures: ssh could
/// not be spawned, the channel stalled, the connection dropped. A command that
/// ran and exited non-zero is a normal `Ok` with `code != 0`, so each caller
/// decides what a failure means for it instead of the transport guessing.
///
/// The persistent channels merge stderr into stdout (`wrap_remote_cmd`), so
/// `stderr` is empty there and the diagnostic text lives in `stdout`; the
/// one-shot paths keep the two streams apart. [`RemoteOutput::diagnostic`]
/// hides that difference.
#[derive(Debug, Clone)]
pub(crate) struct RemoteOutput {
    pub stdout: String,
    pub stderr: String,
    pub code: i32,
}

impl RemoteOutput {
    fn merged(stdout: String, code: i32) -> Self {
        Self {
            stdout,
            stderr: String::new(),
            code,
        }
    }

    /// The most useful text to show a user for a failed command: stderr when
    /// the transport kept it separate, otherwise the tail of the merged output
    /// (the error is almost always the last thing the command printed).
    pub(crate) fn diagnostic(&self) -> String {
        let text = if self.stderr.trim().is_empty() {
            self.stdout.trim()
        } else {
            self.stderr.trim()
        };
        let lines: Vec<&str> = text.lines().collect();
        let tail = if lines.len() > 20 {
            lines[lines.len() - 20..].join("\n")
        } else {
            text.to_string()
        };
        if tail.chars().count() > 4000 {
            tail.chars().skip(tail.chars().count() - 4000).collect()
        } else {
            tail
        }
    }
}

/// Run a remote command and return its stdout, stderr and exit code.
///
/// This is the transport layer. Prefer [`ssh_exec_checked`] for commands whose
/// failure must not be mistaken for success (every file operation), and
/// [`ssh_exec`] where partial output on a non-zero exit is still meaningful.
pub(crate) fn ssh_exec_status(
    profile: &SSHProfile,
    remote_cmd: &str,
) -> Result<RemoteOutput, String> {
    // Ensure a passphrase-protected key is unlocked into the ssh-agent that all
    // our ssh/scp children inherit. No-op for keys without a passphrase.
    crate::commands::sshauth::ensure_key_loaded(profile);

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

        // If recent spawns to this host failed repeatedly, skip the channel and
        // go straight to one-shot — don't hammer a struggling sshd. Mirrors the
        // Unix path via the shared cooldown.
        if channel_spawn_blocked(&channel_key) {
            return ssh_exec_oneshot_windows(profile, remote_cmd);
        }

        let slot: ChannelSlot = {
            // Recover from poison instead of propagating it: a panic in one exec
            // must not permanently brick SSH to every host. The map only holds
            // Arc clones, so its contents are always consistent.
            let mut map = map_mutex.lock().unwrap_or_else(|e| e.into_inner());
            map.entry(channel_key.clone())
                .or_insert_with(|| Arc::new(Mutex::new(None)))
                .clone()
        };

        // Per-host lock: serializes calls to the *same* server (one shared
        // pipe), but the IDLE_TIMEOUT inside `exec` guarantees it is always
        // released within a bounded time. Recover from poison (into_inner) so a
        // panic mid-exec can't permanently brick this host — the next call just
        // rebuilds the channel.
        let mut guard = slot.lock().unwrap_or_else(|e| e.into_inner());

        let need_new = match guard.as_mut() {
            Some(ch) => !ch.is_alive(),
            None => true,
        };
        if need_new {
            eprintln!(
                "[operon-ssh] Opening persistent exec channel for {}",
                channel_key
            );
            match WinSshExecChannel::spawn(profile) {
                Ok(ch) => {
                    *guard = Some(ch);
                    channel_reset_failures(&channel_key);
                }
                Err(e) => {
                    channel_mark_spawn_failed(&channel_key);
                    eprintln!(
                        "[operon-ssh] Channel spawn failed ({}); falling back to one-shot",
                        e
                    );
                    drop(guard);
                    return ssh_exec_oneshot_windows(profile, remote_cmd);
                }
            }
        }

        // The channel merges stderr into stdout; what a non-zero exit means is
        // decided by the `ssh_exec` / `ssh_exec_checked` wrappers, identically
        // to the Unix path.
        fn win_channel_result(stdout: String, exit_code: i32) -> Result<RemoteOutput, String> {
            Ok(RemoteOutput::merged(stdout, exit_code))
        }

        match guard.as_mut().unwrap().exec(remote_cmd) {
            Ok((stdout, exit_code)) => return win_channel_result(stdout, exit_code),
            Err(e) => {
                // Channel stalled or died — drop it (Drop kills the ssh.exe
                // process), then rebuild once and retry. If the rebuild or the
                // replay also fails, fall back to a one-shot direct ssh.exe.
                eprintln!("[operon-ssh] Exec channel error: {}. Reconnecting...", e);
                *guard = None;
                match WinSshExecChannel::spawn(profile) {
                    Ok(mut fresh) => {
                        channel_reset_failures(&channel_key);
                        let result = fresh.exec(remote_cmd);
                        *guard = Some(fresh);
                        match result {
                            Ok((stdout, exit_code)) => {
                                return win_channel_result(stdout, exit_code)
                            }
                            Err(e2) => {
                                eprintln!(
                                    "[operon-ssh] Retry on fresh channel failed: {}. Falling back to one-shot.",
                                    e2
                                );
                                *guard = None;
                                drop(guard);
                                return ssh_exec_oneshot_windows(profile, remote_cmd);
                            }
                        }
                    }
                    Err(spawn_err) => {
                        channel_mark_spawn_failed(&channel_key);
                        eprintln!(
                            "[operon-ssh] Respawn after channel error failed: {}. Falling back to one-shot.",
                            spawn_err
                        );
                        drop(guard);
                        return ssh_exec_oneshot_windows(profile, remote_cmd);
                    }
                }
            }
        }
    }

    // ── macOS/Linux: persistent in-process channel, fall back to one-shot fork ──
    // Even with ControlMaster active, every `ssh` subprocess pays fork+exec+
    // channel-open (~50-200ms on macOS). The persistent channel keeps one
    // `ssh ... bash --noprofile --norc` alive per host and pipes commands
    // through it, mirroring
    // the Windows path.
    #[cfg(not(target_os = "windows"))]
    {
        let channel_key = format!("{}@{}:{}", profile.user, profile.host, profile.port);

        // If a recent spawn failed for this host, skip the channel path
        // entirely for the cooldown window — don't keep hammering sshd
        // with reconnect attempts that will just time out again.
        if channel_spawn_blocked(&channel_key) {
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
                    channel_reset_failures(&channel_key);
                }
                Err(e) => {
                    channel_mark_spawn_failed(&channel_key);
                    pool.record_error(&e);
                    let blocked_now = channel_spawn_blocked(&channel_key);
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
            Ok((stdout, exit_code)) => Ok(RemoteOutput::merged(stdout, exit_code)),
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
                        channel_reset_failures(&channel_key);
                        *guard = Some(fresh);
                        pool.record_respawn();
                        match guard.as_mut().unwrap().exec(remote_cmd) {
                            Ok((stdout, exit_code)) => Ok(RemoteOutput::merged(stdout, exit_code)),
                            Err(e2) => {
                                eprintln!(
                                    "[operon-ssh] Retry on fresh channel failed: {}. Falling back to one-shot.",
                                    e2
                                );
                                *guard = None;
                                channel_mark_spawn_failed(&channel_key);
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
                        channel_mark_spawn_failed(&channel_key);
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
) -> Result<RemoteOutput, String> {
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

    let filtered_stderr = filter_ssh_stderr(&stderr);
    let code = output.status.code().unwrap_or(-1);

    // A dead multiplexing socket is a transport failure, not a command result.
    if code != 0 && stdout.trim().is_empty() {
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
    }

    Ok(RemoteOutput {
        stdout,
        stderr: filtered_stderr,
        code,
    })
}

/// Per-call direct `ssh.exe` fallback for Windows. Used when the persistent
/// channel can't be spawned or an in-flight call errors out. Spawns `ssh.exe`
/// directly via the process API — no `cmd.exe` and no Git Bash shell layer — so
/// argument quoting can't be mangled across the local-shell → ssh → remote-shell
/// chain. The remote command is run under an explicit `bash --noprofile --norc`,
/// mirroring the persistent channel so behaviour is identical.
#[cfg(target_os = "windows")]
fn ssh_exec_oneshot_windows(
    profile: &SSHProfile,
    remote_cmd: &str,
) -> Result<RemoteOutput, String> {
    use std::os::windows::process::CommandExt;

    let mut cmd = std::process::Command::new("ssh");
    cmd.args([
        "-T",
        "-o",
        "BatchMode=yes",
        "-o",
        "StrictHostKeyChecking=accept-new",
        "-o",
        "ConnectTimeout=15",
        "-o",
        "LogLevel=ERROR",
        "-o",
        "PreferredAuthentications=publickey",
    ]);
    cmd.args(["-p", &profile.port.to_string()]);
    if let Some(key) = &profile.key_file {
        if std::path::Path::new(key).exists() {
            cmd.args(["-i", key]);
        }
    }
    cmd.arg(format!("{}@{}", profile.user, profile.host));
    // Single argv element: ssh forwards it verbatim as the remote command, and
    // the remote bash parses the single-quoted payload. No local shell touches it.
    cmd.arg(format!(
        "bash --noprofile --norc -c {}",
        shell_escape(remote_cmd)
    ));
    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW

    let output = cmd
        .output()
        .map_err(|e| format!("Failed to run ssh.exe: {}", e))?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    let filtered_stderr = filter_ssh_stderr(&stderr);

    Ok(RemoteOutput {
        stdout,
        stderr: filtered_stderr,
        code: output.status.code().unwrap_or(-1),
    })
}

/// Run a remote command, tolerating a non-zero exit as long as it produced
/// output.
///
/// This is the historical contract most callers rely on: `grep` with no match,
/// a partially failing `ls`, and probe scripts that end with `|| true` all exit
/// non-zero while their output is exactly what the caller wants. A non-zero
/// exit with NO output is reported as an error (on the persistent channels that
/// is almost always a transport blip, hence the retry wording).
///
/// Do NOT use this for anything that writes, reads a file's content, or must
/// distinguish "it worked" from "it printed an error": the channels merge
/// stderr into stdout, so `cat: x: No such file` would come back as `Ok`. Use
/// [`ssh_exec_checked`] there.
pub(crate) fn ssh_exec(profile: &SSHProfile, remote_cmd: &str) -> Result<String, String> {
    let out = ssh_exec_status(profile, remote_cmd)?;
    if out.code != 0 && out.stdout.trim().is_empty() {
        let stderr = out.stderr.trim();
        if stderr.is_empty() {
            return Err(ssh_transient_error(out.code));
        }
        return Err(format!("SSH command failed: {}", stderr));
    }
    Ok(out.stdout)
}

/// Run a remote command and fail on ANY non-zero exit, with the command's own
/// diagnostic as the error text.
///
/// This is the contract every file operation needs. With the lenient
/// [`ssh_exec`], a save whose `base64 -d` failed, a `cat` of a missing file or
/// an `mv` onto a read-only directory all returned `Ok(<error text>)` because
/// the persistent channel merges stderr into stdout — the editor then showed
/// "Saved" over a truncated file, or opened `cat: ...: Permission denied` as if
/// it were the file. Here the exit code is the verdict and the diagnostic is
/// what the user sees.
pub(crate) fn ssh_exec_checked(profile: &SSHProfile, remote_cmd: &str) -> Result<String, String> {
    let out = ssh_exec_status(profile, remote_cmd)?;
    if out.code != 0 {
        let diag = out.diagnostic();
        return Err(if diag.is_empty() {
            format!("remote command failed with exit status {}", out.code)
        } else {
            diag
        });
    }
    Ok(out.stdout)
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

/// Async wrapper around [`ssh_exec_checked`]; same threading rationale as
/// [`ssh_exec_async`].
pub(crate) async fn ssh_exec_checked_async(
    profile: SSHProfile,
    remote_cmd: String,
) -> Result<String, String> {
    tokio::task::spawn_blocking(move || ssh_exec_checked(&profile, &remote_cmd))
        .await
        .map_err(|e| format!("SSH task failed to run: {}", e))?
}

/// Async wrapper around [`ssh_exec_status`]; same threading rationale as
/// [`ssh_exec_async`].
pub(crate) async fn ssh_exec_status_async(
    profile: SSHProfile,
    remote_cmd: String,
) -> Result<RemoteOutput, String> {
    tokio::task::spawn_blocking(move || ssh_exec_status(&profile, &remote_cmd))
        .await
        .map_err(|e| format!("SSH task failed to run: {}", e))?
}

/// True when a user-typed remote path still needs the remote shell to expand it
/// (`~`, `~user`, `$HOME`, `$SCRATCH/...`).
///
/// Every path operand now reaches the remote inside SINGLE quotes, which is what
/// makes remote file operations injection-proof — but single quotes also suppress
/// the `$VAR` expansion that double quotes used to allow. HPC users legitimately
/// type `$SCRATCH/run` or `~/data` into the Remote Explorer's path bar (the app's
/// own `work_dir` setting documents `$USER`/`$SCRATCH` support), so those must
/// keep working. The answer is to expand ONCE, in a dedicated resolver, and then
/// treat the concrete result as a literal everywhere else — never to loosen the
/// quoting of the file operations themselves.
pub(crate) fn needs_remote_expansion(path: &str) -> bool {
    path.starts_with('~') || path.contains('$')
}

/// Shell snippet that expands `raw` on the remote and prints the resulting
/// absolute path, with command substitution defanged.
///
/// `\`, `"` and backtick are escaped so the value cannot break out of the
/// double-quoted assignment, and `$(` is escaped so parameter expansion still
/// works while command substitution does not. That is the whole difference
/// between "expand a variable" and "run whatever the string says".
pub(crate) fn remote_expansion_script(raw: &str) -> String {
    let esc = raw
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('`', "\\`")
        .replace("$(", "\\$(");
    // Tilde expansion does NOT happen inside double quotes, so `~`/`~/x` has to be
    // rewritten explicitly after the assignment. `~user` is left alone: resolving
    // another account's home needs the shell's own expansion, which is exactly what
    // we are refusing to give this string.
    format!(
        "d=\"{}\"; case \"$d\" in \"~\") d=\"$HOME\";; \"~/\"*) d=\"$HOME/${{d#\"~/\"}}\";; esac; printf '%s' \"$d\"",
        esc
    )
}

/// Resolve a user-typed remote path to a concrete one.
///
/// Called at the ONE point raw text enters the app — the Remote Explorer's path
/// bar — so that everything downstream (listing, mkdir, upload, cd-to-terminal,
/// the chat session's working directory) operates on the same resolved literal.
///
/// Resolving inside `list_remote_directory` instead would be worse than not
/// resolving at all: the listing would succeed while `remotePath` still held
/// `$SCRATCH/run`, so New Folder would create a directory literally named
/// `$SCRATCH` and the user would have no signal anything was wrong.
#[tauri::command]
pub async fn resolve_remote_path(
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
    Ok(expand_remote_path(&profile, &path).await)
}

/// Expand a user-typed remote path to a concrete one. Falls back to the input
/// unchanged if the remote round-trip fails — navigation degrades to "folder not
/// found", never to executing the string.
async fn expand_remote_path(profile: &SSHProfile, path: &str) -> String {
    if !needs_remote_expansion(path) {
        return path.to_string();
    }
    let b64 = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        remote_expansion_script(path).as_bytes(),
    );
    let wrapped = format!("echo '{}' | base64 -d | bash", b64);
    match ssh_exec_async(profile.clone(), wrapped).await {
        Ok(out) => {
            let t = out.trim();
            if t.is_empty() {
                path.to_string()
            } else {
                t.to_string()
            }
        }
        Err(_) => path.to_string(),
    }
}

/// Reject remote paths that a legacy `scp` would let the remote shell execute.
///
/// `scp` is the one remote operation whose path CANNOT be protected by quoting.
/// OpenSSH ≥ 9.0 transfers over SFTP and treats the operand literally, so quotes
/// would become part of the filename; older clients (RHEL/Rocky 8 ships 8.0p1 —
/// a platform Operon targets) use the legacy protocol, where the operand IS
/// expanded by the remote user's shell. One escaping cannot be right for both.
///
/// So instead of escaping, refuse: a filename containing shell-active characters
/// is blocked from transfer with an explanation. That is safe on every client, and
/// it costs nothing for real paths — spaces, brackets, quotes and non-ASCII are all
/// still allowed, because none of them can start a command.
///
/// This is the same threat model as the path-quoting fix: on a shared HPC
/// filesystem the directory contents are not under the user's control, so
/// downloading a file someone else named must not run anything.
pub(crate) fn scp_path_is_safe(path: &str) -> bool {
    !path.chars().any(|c| {
        matches!(
            c,
            '`' | '$' | ';' | '&' | '|' | '<' | '>' | '(' | ')' | '\n' | '\r' | '\\'
        )
    })
}

fn reject_unsafe_scp_path(path: &str) -> Result<(), String> {
    if scp_path_is_safe(path) {
        return Ok(());
    }
    Err(format!(
        "Refusing to transfer a remote path containing shell metacharacters: {path}\n\
         Older scp clients let the remote shell expand this path, so a name like \
         `$(...)` would execute. Rename the file on the server, or move it into a \
         directory whose full path is free of ` $ ; & | < > ( ) \\ and newlines."
    ))
}

/// POSIX single-quote escaping — the ONLY correct way to put a remote path into
/// a shell command.
///
/// There used to be a sibling `shell_escape_inner` that wrapped values in DOUBLE
/// quotes and escaped only `"`. Inside double quotes a POSIX shell still performs
/// command substitution and parameter expansion, so every remote file operation
/// (`ls`, `cat`, `base64`, `mkdir`, `rm`, `mv`, write) executed whatever was in
/// the path: a file named `$(touch ~/.operon-pwn)` ran that command when the user
/// merely listed the directory. On a shared HPC filesystem the directory contents
/// are not under the user's control, so this was reachable without any mistake on
/// their part.
///
/// It is deleted rather than fixed. A "safe double-quote escaper" cannot exist,
/// and leaving one in the module means the next remote command reaches for it.
fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

// ── Remote File Operations ──
//
// Every command below must behave identically on GNU/Linux (HPC clusters),
// macOS/BSD (a lab Mac Studio) and BusyBox, under `bash --noprofile --norc`
// 3.2 as well as the user's login shell. The rules that keep it that way:
//
//  * `base64` never gets a file operand. BSD `base64` has no positional
//    argument (`base64 -- file` is "invalid argument", exit 64), so files are
//    always redirected in with `<`. The one-time cost of that bug was every
//    image on a macOS remote rendering blank and every large save emptying the
//    file it was saving.
//  * A destination is never truncated before the data that replaces it exists.
//    Decoding happens into a temp file first; the target is written only from
//    a successful decode, so a failing decoder (or a dropped connection
//    mid-stream) leaves the original untouched.
//  * `LC_ALL=C` in front of `ls`, so the date columns the parser counts on
//    cannot change shape with whatever LANG the client's ssh forwards.
//  * Every one of these runs through `ssh_exec_checked`: a non-zero exit is
//    an error carrying the command's own diagnostic, never "success with an
//    error message as the content".

/// `base64 < 'path'` — the only encoding form GNU, BSD and BusyBox all accept.
/// Output may be line-wrapped (GNU wraps at 76 columns); callers strip
/// whitespace.
pub(crate) fn remote_read_base64_cmd(path: &str) -> String {
    format!("base64 < {}", shell_escape(path))
}

/// True when `s` is nothing but base64 alphabet (after whitespace removal).
/// A remote `base64` that printed a usage or error message instead of data is
/// caught here rather than handed to the viewer as an image.
pub(crate) fn looks_like_base64(s: &str) -> bool {
    s.bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'/' || b == b'=')
}

/// Shell snippet that decodes base64 (produced by `decode_pipeline`) into
/// `path` without ever truncating `path` before the replacement bytes exist.
///
/// The decode lands in a temp file first — `mktemp` in the TARGET'S directory,
/// so it shares the filesystem and quota the real write will face, falling back
/// to `$TMPDIR` when that directory is not writable (a read-only directory can
/// still hold a writable file). Only a decode that SUCCEEDED is then copied
/// into the target with `cat > path`, which keeps the target's inode, mode,
/// owner and symlink-ness, exactly like an in-place editor save.
///
/// Failure paths: a bad decode removes the temp and exits non-zero, leaving the
/// original file untouched. A failed copy (quota, permissions) keeps the temp
/// and prints where it is, so the user's content is never the thing that is
/// lost. `cleanup_extra` is any additional path to remove on both paths (the
/// chunk file of a chunked upload).
fn remote_write_from_b64(decode_pipeline: &str, path: &str, cleanup_extra: &str) -> String {
    let target = shell_escape(path);
    let dir = match path.rsplit_once('/') {
        Some(("", _)) => "/".to_string(),
        Some((d, _)) => d.to_string(),
        None => ".".to_string(),
    };
    let dir_tmpl = shell_escape(&format!("{}/.operon.XXXXXX", dir));
    format!(
        "t=$(mktemp {dir_tmpl} 2>/dev/null) || t=$(mktemp \"${{TMPDIR:-/tmp}}/operon.XXXXXX\") || \
         {{ echo \"operon: no writable temp location for {target}\" >&2; exit 1; }}; \
         if {decode_pipeline} > \"$t\"; then \
         {{ cat -- \"$t\" > {target} && rm -f -- \"$t\" {cleanup_extra}; }} || \
         {{ echo \"operon: could not write {target}; the new content was left at $t\" >&2; false; }}; \
         else rm -f -- \"$t\" {cleanup_extra}; echo \"operon: remote base64 decode failed\" >&2; false; fi",
    )
}

/// Single-round-trip write for content whose base64 fits in one command.
pub(crate) fn remote_write_small_cmd(b64: &str, path: &str) -> String {
    remote_write_from_b64(&format!("printf %s {} | base64 -d", b64), path, "")
}

/// Name of the shared-filesystem file that accumulates a chunked upload's
/// base64. It sits next to the target on purpose: consecutive chunks may be
/// sent over different SSH connections (channel rebuilt, one-shot fallback),
/// and on a load-balanced login-node pool those can land on different hosts
/// whose `/tmp` are not shared.
fn remote_chunk_file(path: &str) -> String {
    format!("{}.__operon_tmp_b64__", path)
}

/// Append (or start) a chunk of base64 in the chunk file.
pub(crate) fn remote_write_chunk_cmd(chunk: &str, path: &str, first: bool) -> String {
    format!(
        "printf %s {} {} {}",
        chunk,
        if first { ">" } else { ">>" },
        shell_escape(&remote_chunk_file(path))
    )
}

/// Decode the assembled chunk file into the target, then remove it.
pub(crate) fn remote_write_finish_cmd(path: &str) -> String {
    let chunk = shell_escape(&remote_chunk_file(path));
    remote_write_from_b64(&format!("base64 -d < {}", chunk), path, &chunk)
}

/// `LC_ALL=C ls -lL[A] -- 'path'`. Locale pinned so the date is always three
/// tokens; `-L` so a symlink to a directory lists as a directory. stderr is NOT
/// discarded: a missing or unreadable directory must say so.
pub(crate) fn remote_list_cmd(path: &str, show_hidden: bool) -> String {
    let ls_flag = if show_hidden { "-lLA" } else { "-lL" };
    format!("LC_ALL=C ls {} -- {}", ls_flag, shell_escape(path))
}

/// `-rw-r--r--`, `drwxr-xr-x@`, `-rw-r--r--.` — a type letter, nine mode
/// characters, and optionally the macOS/SELinux/ACL suffix. Anything else in
/// the first column is not a listing line.
fn looks_like_ls_perms(p: &str) -> bool {
    let b = p.as_bytes();
    b.len() >= 10
        && matches!(b[0], b'-' | b'd' | b'l' | b'c' | b'b' | b'p' | b's')
        && b[1..10]
            .iter()
            .all(|c| matches!(c, b'r' | b'w' | b'x' | b's' | b'S' | b't' | b'T' | b'-'))
}

/// Parse `ls -l` output into entries.
///
/// The first eight whitespace-delimited fields (perms, links, user, group,
/// size, month, day, time-or-year) are walked in the ORIGINAL line and the
/// name is everything after the single separator space, verbatim — so a name
/// with two consecutive spaces, a tab, or a trailing `@`/`*`/`=` survives.
/// (`ls -l` never appends indicator characters without `-F`; the old parser
/// stripped them from real names.) Only symlink lines can carry ` -> target`,
/// and with `-L` in effect those are dangling links, which are skipped like
/// any other line that does not parse. Lines that are not listings (the
/// `total` header, an error message from a partially failed `ls`) are ignored.
pub(crate) fn parse_ls_long(output: &str, base_path: &str) -> Vec<FileEntry> {
    let base = if base_path.ends_with('/') {
        base_path.to_string()
    } else {
        format!("{}/", base_path)
    };
    let mut entries: Vec<FileEntry> = Vec::new();

    for raw in output.lines() {
        let line = raw.trim_end_matches(['\r', '\n']);
        if line.is_empty() || line.starts_with("total ") {
            continue;
        }
        let bytes = line.as_bytes();
        let mut idx = 0usize;
        let mut fields: Vec<&str> = Vec::with_capacity(8);
        for _ in 0..8 {
            while idx < bytes.len() && bytes[idx].is_ascii_whitespace() {
                idx += 1;
            }
            let start = idx;
            while idx < bytes.len() && !bytes[idx].is_ascii_whitespace() {
                idx += 1;
            }
            if start == idx {
                break;
            }
            fields.push(&line[start..idx]);
        }
        if fields.len() < 8 {
            continue;
        }
        // Exactly one separator space precedes the name; a name that itself
        // starts with a space keeps it.
        if idx < bytes.len() && bytes[idx] == b' ' {
            idx += 1;
        }
        let name_part = &line[idx..];
        let perms = fields[0];
        if !looks_like_ls_perms(perms) {
            continue; // an error line, or something that is not a listing
        }
        if fields[4].ends_with(',') {
            continue; // device node: "8, 0" in the size column shifts every field
        }
        let first = perms.as_bytes()[0];
        let name = if first == b'l' {
            name_part.split(" -> ").next().unwrap_or(name_part)
        } else {
            name_part
        };
        if name.is_empty() || name == "." || name == ".." {
            continue;
        }

        let is_dir = first == b'd';
        let size = fields[4].parse::<u64>().unwrap_or(0);
        let extension = if !is_dir {
            name.rsplit('.')
                .next()
                .and_then(|e| if e != name { Some(e.to_string()) } else { None })
        } else {
            None
        };

        entries.push(FileEntry {
            name: name.to_string(),
            path: format!("{}{}", base, name),
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
    entries
}

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

    let out = ssh_exec_status_async(profile, remote_list_cmd(&path, show_hidden)).await?;
    let entries = parse_ls_long(&out.stdout, &path);

    // `ls` exits non-zero for a partially failed listing too (one unreadable
    // entry); only an empty result is treated as a failure of the directory
    // itself, and then the diagnostic names the reason.
    if entries.is_empty() && out.code != 0 {
        let diag = out.diagnostic();
        return Err(if diag.is_empty() {
            format!("Cannot list {} (exit status {})", path, out.code)
        } else {
            format!("Cannot list {}: {}", path, diag)
        });
    }

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

/// Resolve the directory a remote session should start in: the configured
/// `work_dir` (server config) if set, otherwise the remote `$HOME`.
///
/// `work_dir` may contain shell variables (e.g. `/dfs3b/operonws/$USER`), so it
/// is expanded ON THE REMOTE inside a double-quoted assignment — `\`, `"` and
/// `` ` `` are escaped (but NOT `$`) so env vars expand without letting the
/// string break out of the quotes. If the expanded path doesn't exist we fall
/// back to `$HOME` rather than dropping the user into a literal `$USER` folder.
/// The result becomes the Remote Explorer's initial directory, which in turn
/// drives the Claude session CWD and the remote-search root.
#[tauri::command]
pub async fn get_remote_initial_dir(
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

    let work_dir = profile
        .server_config
        .get("work_dir")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    if let Some(wd) = work_dir {
        // Escape only the chars that would break the double-quoted string;
        // leave `$` so `$USER`/`$HOME`/`$SCRATCH` expand on the remote.
        let esc = wd
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('`', "\\`");
        let script = format!(
            "d=\"{}\"; if [ -d \"$d\" ]; then printf '%s' \"$(cd -- \"$d\" && pwd -P)\"; else printf '%s' \"$HOME\"; fi",
            esc
        );
        // base64-wrap so the multi-quote script survives the local→SSH→remote
        // shell chain unmangled (same trick the remote search uses).
        let b64 = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            script.as_bytes(),
        );
        let wrapped = format!("echo '{}' | base64 -d | bash", b64);
        let resolved = ssh_exec_async(profile.clone(), wrapped).await?;
        let resolved = resolved.trim().to_string();
        if !resolved.is_empty() {
            return Ok(resolved);
        }
    }

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

    // Checked: a failed `cat` is an error with its own message, never a tab
    // whose content is "cat: ...: No such file or directory" — and never cached.
    let content =
        ssh_exec_checked_async(profile, format!("cat -- {}", shell_escape(&path))).await?;
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

    let output = ssh_exec_checked_async(profile, remote_read_base64_cmd(&path)).await?;
    let b64: String = output.chars().filter(|c| !c.is_whitespace()).collect();
    if !looks_like_base64(&b64) {
        // Belt and braces for a transport that merged an error into stdout
        // with a zero exit: never hand the viewer a data: URI made of text.
        return Err(format!(
            "remote base64 produced unexpected output for {}: {}",
            path,
            output.trim().lines().next().unwrap_or("")
        ));
    }
    Ok(b64)
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

    let cmd = format!("mkdir -p -- {}", shell_escape(&path));
    ssh_exec_checked(&profile, &cmd)?;
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
    let escaped = shell_escape(&path);
    let check_cmd = format!(
        "if [ -d {} ]; then echo DIR; elif [ -f {} ]; then echo FILE; else echo NONE; fi",
        escaped, escaped
    );
    let result = ssh_exec_checked(&profile, &check_cmd)?;
    let kind = result.trim();

    match kind {
        "FILE" => {
            let cmd = format!("rm -- {}", escaped);
            ssh_exec_checked(&profile, &cmd)?;
        }
        "DIR" => {
            let cmd = format!("rm -rf -- {}", escaped);
            ssh_exec_checked(&profile, &cmd)?;
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

    let escaped: Vec<String> = paths.iter().map(|p| shell_escape(p)).collect();
    let bulk_cmd = format!("rm -rf -- {}; echo $?", escaped.join(" "));
    let result = ssh_exec_async(profile.clone(), bulk_cmd).await?;
    let exit = result.trim().lines().last().unwrap_or("1").trim();

    let succeeded = if exit == "0" {
        paths.len()
    } else {
        // Per-path verification — count any paths that no longer exist.
        let probes: Vec<String> = paths
            .iter()
            .map(|p| format!("[ -e {} ] || echo MISSING", shell_escape(p)))
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
        "mv -- {} {}",
        shell_escape(&old_path),
        shell_escape(&new_path)
    );
    ssh_exec_checked(&profile, &cmd)?;
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

    // Ensure the parent directory exists. Checked: if this fails the write
    // would fail too, and "cannot create /x/y" is the message the user needs.
    if let Some(parent) = std::path::Path::new(&path).parent() {
        let parent = parent.to_string_lossy();
        if !parent.is_empty() && parent != "/" {
            let mkdir_cmd = format!("mkdir -p -- {}", shell_escape(&parent));
            ssh_exec_checked_async(profile.clone(), mkdir_cmd)
                .await
                .map_err(|e| format!("Cannot create directory {}: {}", parent, e))?;
        }
    }

    // Content travels as base64 so no quoting layer can touch it. Anything that
    // fits in one command is written in one round trip; larger content is
    // accumulated in a chunk file next to the target (see `remote_chunk_file`)
    // and decoded once. In both cases the target is only written from a
    // decode that already succeeded — see `remote_write_from_b64`.
    let b64 = base64::engine::general_purpose::STANDARD.encode(content.as_bytes());

    // ~100 KB per command stays well under the ControlMaster socket message
    // limit (~256 KB).
    const CHUNK_SIZE: usize = 100_000;

    if b64.len() <= CHUNK_SIZE {
        ssh_exec_checked_async(profile.clone(), remote_write_small_cmd(&b64, &path))
            .await
            .map_err(|e| format!("Save failed: {}", e))?;
    } else {
        let mut offset = 0usize;
        let mut first = true;
        while offset < b64.len() {
            let end = std::cmp::min(offset + CHUNK_SIZE, b64.len());
            let cmd = remote_write_chunk_cmd(&b64[offset..end], &path, first);
            if let Err(e) = ssh_exec_checked_async(profile.clone(), cmd).await {
                // Leave nothing behind from a half-uploaded chunk file.
                let _ = ssh_exec_async(
                    profile.clone(),
                    format!("rm -f -- {}", shell_escape(&remote_chunk_file(&path))),
                )
                .await;
                return Err(format!("Save failed while uploading: {}", e));
            }
            first = false;
            offset = end;
        }
        ssh_exec_checked_async(profile.clone(), remote_write_finish_cmd(&path))
            .await
            .map_err(|e| format!("Save failed: {}", e))?;
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
        let mkdir_cmd = format!("mkdir -p -- {}", shell_escape(&parent.to_string_lossy()));
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

    // Only reuse the ControlMaster socket if a LIVE master exists — a stale
    // socket file would make scp silently re-auth/hang.
    if let Some(sock) = live_control_socket(&profile) {
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
    reject_unsafe_scp_path(&remote_path)?;
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

    crate::commands::sshauth::ensure_key_loaded(&profile);

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

    // Only reuse the ControlMaster socket if a LIVE master exists — a stale
    // socket file would make scp silently re-auth/hang.
    if let Some(sock) = live_control_socket(&profile) {
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

    reject_unsafe_scp_path(&remote_path)?;
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

    crate::commands::sshauth::ensure_key_loaded(&profile);

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

    // Only reuse the ControlMaster socket if a LIVE master exists — a stale
    // socket file would make scp silently re-auth/hang.
    if let Some(sock) = live_control_socket(&profile) {
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

    reject_unsafe_scp_path(&remote_path)?;
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

    // Seed the agent (and SSH_AUTH_SOCK) so the agent fallback below can work
    // for passphrase-protected keys; harmless for keys without a passphrase.
    crate::commands::sshauth::ensure_key_loaded(profile);

    let mut sess = ssh2::Session::new().map_err(|e| format!("ssh2 new: {}", e))?;
    sess.set_tcp_stream(tcp);
    sess.handshake().map_err(|e| format!("handshake: {}", e))?;

    // Try the explicit key file first. libssh2's Windows agent client does not
    // read Git's MSYS agent socket, so for the SFTP download path we pass the
    // passphrase (from the OS keychain) straight to libssh2 rather than relying
    // on the agent. `None` for an unencrypted key works as before.
    if let Some(key_path) = &profile.key_file {
        let key = std::path::Path::new(key_path);
        if key.exists() {
            let passphrase = crate::commands::sshauth::read_passphrase(&profile.id);
            let _ = sess.userauth_pubkey_file(&profile.user, None, key, passphrase.as_deref());
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

/// Join a remote (POSIX) base path with a relative sub-path using forward
/// slashes. Remote paths must never be built with `std::path` on Windows,
/// where `PathBuf::join` would insert backslashes the SFTP server rejects.
fn remote_join(base: &str, rel: &str) -> String {
    if rel.is_empty() {
        return base.to_string();
    }
    format!("{}/{}", base.trim_end_matches('/'), rel)
}

/// Basename of a forward-slash remote path.
fn remote_basename(path: &str) -> &str {
    path.trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(path)
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

        // Remote side stays a POSIX forward-slash string; only the local
        // destination uses native path separators.
        let remote = std::path::Path::new(&remote_path);
        let stat = sftp
            .stat(remote)
            .map_err(|e| format!("stat {}: {}", remote_path, e))?;
        let total = stat.size.unwrap_or(0);
        let name = remote_basename(&remote_path).to_string();

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
    remote_root: &str,
    rel: &str,
    out: &mut Vec<(String, u64)>,
) -> Result<(), String> {
    // Remote paths are forward-slash POSIX strings, never std::path joins.
    let path = remote_join(remote_root, rel);
    let entries = sftp
        .readdir(std::path::Path::new(&path))
        .map_err(|e| format!("readdir {}: {}", path, e))?;
    for (entry_path, stat) in entries {
        let name = entry_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");
        if name.is_empty() || name == "." || name == ".." {
            continue;
        }
        let new_rel = remote_join(rel, name);
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

        // Remote root is a POSIX forward-slash string; only the local root
        // is a native PathBuf.
        let local_root = std::path::PathBuf::from(&local_path);
        let root_name = remote_basename(&remote_path).to_string();

        let mut files: Vec<(String, u64)> = Vec::new();
        sftp_walk_collect(&sftp, &remote_path, "", &mut files)?;
        let total: u64 = files.iter().map(|(_, sz)| *sz).sum();

        std::fs::create_dir_all(&local_root)
            .map_err(|e| format!("create {}: {}", local_path, e))?;

        emit_progress(&app, 0, total, &root_name);
        let mut buf = vec![0u8; 64 * 1024];
        let mut completed: u64 = 0;
        let mut last_emit = std::time::Instant::now();

        for (rel, _) in &files {
            // `rel` is forward-slash; remote side joins as a string, local
            // side maps each segment onto a native PathBuf.
            let remote_file_path = remote_join(&remote_path, rel);
            let mut local_file_path = local_root.clone();
            for seg in rel.split('/') {
                local_file_path.push(seg);
            }
            if let Some(parent) = local_file_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }

            let mut remote_file = sftp
                .open(std::path::Path::new(&remote_file_path))
                .map_err(|e| format!("open {}: {}", remote_file_path, e))?;
            let mut local_file = std::fs::File::create(&local_file_path)
                .map_err(|e| format!("create {:?}: {}", local_file_path, e))?;
            let current_name = rel.clone();

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
    let mkdir_cmd = format!("mkdir -p -- {}", shell_escape(&remote_dir));
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
    // Reuse the live ControlMaster socket the interactive SSH terminal creates
    if let Some(sock) = live_control_socket(&profile) {
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
        reject_unsafe_scp_path(&remote_dest)?;
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
    key_passphrase: Option<String>, // optional: encrypt the generated key with this passphrase
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

    // Optional: encrypt the generated key with a passphrase. Stored in the OS
    // keychain and auto-loaded into ssh-agent so the user is never prompted.
    let passphrase_arg = key_passphrase
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or("");
    let used_passphrase = !passphrase_arg.is_empty();

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
        // Resolve ssh-keygen to a full path on Windows: it lives in
        // System32\OpenSSH (or Git's usr\bin), a dir that often isn't on the
        // app's inherited PATH, so a bare `Command::new("ssh-keygen")` fails
        // with "program not found" even when ssh.exe was detected.
        #[cfg(windows)]
        let keygen =
            crate::platform::windows::find_ssh_keygen().unwrap_or_else(|| "ssh-keygen".to_string());
        #[cfg(not(windows))]
        let keygen = "ssh-keygen".to_string();

        let output = hide_window(std::process::Command::new(&keygen).args([
            "-t",
            "ed25519",
            "-f",
            &private_key_path.to_string_lossy(),
            "-N",
            passphrase_arg,
            "-C",
            &format!("operon@{}", profile.host),
        ]))
        .output()
        .map_err(|e| {
            format!(
                "Failed to run ssh-keygen: {}. Ensure OpenSSH (or Git Bash on Windows) is installed.",
                e
            )
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("ssh-keygen failed: {}", stderr));
        }

        // On Windows the new key inherits the parent dir ACL, so ssh.exe rejects
        // it with "UNPROTECTED PRIVATE KEY FILE ... bad permissions". Tighten the
        // ACL to the current user only. (Unix already gets 0600 from ssh-keygen.)
        #[cfg(windows)]
        {
            let key_path = private_key_path.to_string_lossy().to_string();
            // Only tighten if we actually know the user. `/inheritance:r` strips
            // ALL inherited ACEs first; if it ran with an empty principal the
            // grant would fail and leave the key with NO usable ACE (locked out).
            // Skipping leaves the inherited ACL (ssh.exe may warn) — recoverable,
            // unlike a lockout. %USERNAME% is set in any normal interactive session.
            match std::env::var("USERNAME") {
                Ok(username) if !username.is_empty() => {
                    let _ = hide_window(std::process::Command::new("icacls").args([
                        key_path.as_str(),
                        "/inheritance:r",
                        "/grant:r",
                        &format!("{}:F", username),
                    ]))
                    .output();
                }
                _ => {
                    eprintln!(
                        "[operon-ssh] USERNAME unset — skipping icacls hardening for {}; ssh.exe may reject the key until ACLs are tightened manually",
                        key_path
                    );
                }
            }
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

    // If the generated key is passphrase-protected, store the passphrase in the
    // OS keychain and load the key into the agent NOW — otherwise the BatchMode
    // verification below cannot decrypt it and would wrongly report failure.
    if used_passphrase {
        let _ = crate::commands::sshauth::store_passphrase(&profile_id, passphrase_arg);
        let mut p2 = profile.clone();
        p2.key_file = Some(private_key_path.to_string_lossy().to_string());
        p2.key_has_passphrase = true;
        crate::commands::sshauth::ensure_key_loaded(&p2);
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
                p.key_has_passphrase = used_passphrase;
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
            p.key_has_passphrase = used_passphrase;
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
        let in_cooldown = channel_spawn_blocked(&channel_key);
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
        channel_reset_failures(&channel_key);

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_remote_cmd_uses_subshell_and_exit_code() {
        let w = wrap_remote_cmd("echo hi", "__D__");
        assert!(w.starts_with("( echo hi\n)"), "must open a subshell: {w:?}");
        assert!(w.contains("</dev/null"), "must redirect stdin: {w:?}");
        assert!(
            w.contains("echo \"__D__$?\""),
            "must emit delimiter fused with exit code: {w:?}"
        );
        // Guard against regressing to a brace group, which leaks `exit`.
        assert!(!w.contains("{ echo"), "must not use a brace group: {w:?}");
    }

    /// The core P0 regression, proven against a real shell: piping two wrapped
    /// commands through ONE long-lived bash, where the first calls `exit 7`,
    /// must NOT kill the shell — the second command still has to run. With the
    /// old brace-group wrapper this test fails (the shell dies after `exit`).
    /// Skips cleanly if no POSIX `bash` is available (e.g. bare Windows CI).
    #[test]
    fn remote_exit_does_not_kill_persistent_shell() {
        use std::io::Write;
        use std::process::{Command, Stdio};

        let bash = ["/bin/bash", "bash"].into_iter().find(|b| {
            Command::new(b)
                .arg("--version")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
        });
        let bash = match bash {
            Some(b) => b,
            None => {
                eprintln!("skipping remote_exit test: no POSIX bash found");
                return;
            }
        };

        let delim = "__OPERON_TEST_DONE__";
        // Command #1 exits 7 inside its subshell; command #2 must still run.
        let script = format!(
            "{}{}",
            wrap_remote_cmd("echo before; exit 7", delim),
            wrap_remote_cmd("echo after", delim)
        );

        let mut child = Command::new(bash)
            .args(["--noprofile", "--norc"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn bash");
        child
            .stdin
            .take()
            .unwrap()
            .write_all(script.as_bytes())
            .unwrap();
        let out = child.wait_with_output().unwrap();
        let s = String::from_utf8_lossy(&out.stdout);

        assert!(s.contains("before"), "missing first command output: {s:?}");
        assert!(
            s.contains(&format!("{delim}7")),
            "first command exit code 7 not reported: {s:?}"
        );
        // The crucial assertion: the shell survived `exit 7` and ran command #2.
        assert!(
            s.contains("after"),
            "persistent shell died after a remote `exit` — P0 regression: {s:?}"
        );
        assert!(
            s.contains(&format!("{delim}0")),
            "second command exit code 0 not reported: {s:?}"
        );
    }
}

/// The remote-path escaping contract.
///
/// Every one of these payloads was executable on the remote host before
/// `shell_escape_inner` was deleted: paths reached `ls`/`cat`/`base64`/`mkdir`/
/// `rm`/`mv`/write wrapped in double quotes, which stop neither `$(...)` nor
/// backticks nor `$VAR`. On a shared HPC filesystem the attacker only has to
/// create a filename in a directory the user will browse.
#[cfg(test)]
mod remote_path_escaping_tests {
    use super::*;

    /// Filenames that are legal on a POSIX filesystem and hostile in a shell.
    const HOSTILE: &[&str] = &[
        "$(touch /tmp/operon-pwn)",
        "`touch /tmp/operon-pwn`",
        "$(id)",
        "${HOME}",
        "$HOME",
        "a$(id)b.txt",
        "back\\slash",
        "semi;colon",
        "pipe|char",
        "amp&ersand",
        "new\nline",
        "quote'single",
        "quote\"double",
        "-rf",
        "--no-preserve-root",
        "* glob",
        "tab\there",
    ];

    /// Everything a POSIX shell would act on if it were not inside single quotes.
    fn is_inert(escaped: &str) -> bool {
        // Must be single-quoted end to end.
        if !escaped.starts_with('\'') || !escaped.ends_with('\'') {
            return false;
        }
        // Inside single quotes the ONLY character with meaning is `'` itself, and
        // the escaper must have rewritten every one as the '\'' idiom. Strip those
        // and no bare quote may remain.
        let body = &escaped[1..escaped.len() - 1];
        !body.replace("'\\''", "").contains('\'')
    }

    #[test]
    fn every_hostile_filename_is_rendered_inert() {
        for p in HOSTILE {
            let e = shell_escape(p);
            assert!(is_inert(&e), "not inert: {p:?} -> {e}");
        }
    }

    #[test]
    fn escaping_survives_a_round_trip_through_sh() {
        // The real proof: hand the escaped value to a POSIX shell and check the
        // argument it reconstructs is byte-identical to what we started with.
        // If any expansion had survived, the printed value would differ.
        for p in HOSTILE {
            let out = std::process::Command::new("sh")
                .arg("-c")
                .arg(format!("printf %s {}", shell_escape(p)))
                .output()
                .expect("run sh");
            assert_eq!(
                String::from_utf8_lossy(&out.stdout),
                **p,
                "shell altered {p:?}"
            );
        }
    }

    #[test]
    fn command_substitution_does_not_execute() {
        // Belt and braces: run the escaped payload as a command operand and prove
        // the side effect never happened.
        let marker = std::env::temp_dir().join("operon-escape-test-marker");
        let _ = std::fs::remove_file(&marker);
        let payload = format!("$(touch {})", marker.display());
        let out = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!("printf %s {}", shell_escape(&payload)))
            .output()
            .expect("run sh");
        assert_eq!(String::from_utf8_lossy(&out.stdout), payload);
        assert!(
            !marker.exists(),
            "command substitution executed — the escaper is not holding"
        );
    }

    /// `ssh.rs` with this test module removed, so a needle cannot be satisfied by
    /// the assertion that searches for it. Without this, `include_str!` hands the
    /// test its own source and any unquoted needle matches itself — the
    /// `--`-separator check below passed even with the production `ls` reverted
    /// to the unseparated form.
    fn production_code() -> &'static str {
        let src = include_str!("ssh.rs");
        let marker = concat!("mod remote_path_", "escaping_tests");
        match src.find(marker) {
            Some(i) => &src[..i],
            None => src,
        }
    }

    #[test]
    fn a_leading_dash_is_still_a_filename_not_a_flag() {
        // Quoting alone does not stop `rm -rf` reading `-rf` as options, which is
        // why every path operand also gets a `--` separator. Guard the separator
        // here so a future edit cannot quietly drop it.
        let src = production_code();
        // `base64` never takes a path operand any more (BSD base64 has none —
        // files are redirected in with `<`, and a redirection target is never
        // option-parsed), so the guarded forms are the ones that still name a
        // path as an argument.
        for pat in [
            "cat -- {}",
            "base64 < {}",
            "mkdir -p -- {}",
            "rm -- {}",
            "rm -rf -- {}",
            "mv -- {} {}",
            "LC_ALL=C ls {} -- {}",
            " > {target} && rm -f -- ",
            "else rm -f -- ",
        ] {
            assert!(src.contains(pat), "lost the -- separator: {pat}");
        }
    }

    #[test]
    fn the_double_quote_escaper_is_gone_and_stays_gone() {
        // The fix is the deletion. If someone reintroduces a double-quote
        // "escaper", every remote command becomes injectable again.
        //
        // The needle is assembled at runtime: spelled literally it would appear
        // in this test's own source, which `include_str!` then matches.
        let needle = format!("fn shell_escape{}", "_inner");
        for (file, src) in [
            ("ssh.rs", include_str!("ssh.rs")),
            ("platform/common.rs", include_str!("../platform/common.rs")),
        ] {
            assert!(
                !src.contains(&needle),
                "{file}: the double-quote escaper is back — remote paths are injectable again"
            );
        }
    }

    #[test]
    fn only_paths_that_need_expansion_pay_for_it() {
        for p in ["~", "~/data", "$SCRATCH/run", "/a/$USER/b", "~alice/x"] {
            assert!(needs_remote_expansion(p), "should expand {p}");
        }
        for p in [
            "/absolute/path",
            "relative/path",
            "/with space/x",
            "/with'quote",
        ] {
            assert!(!needs_remote_expansion(p), "should NOT expand {p}");
        }
    }

    #[test]
    fn expansion_keeps_variables_but_kills_command_substitution() {
        // The whole point: `$VAR` must still expand, `$(...)` and backticks must not.
        let run = |raw: &str| {
            let out = std::process::Command::new("sh")
                .arg("-c")
                .arg(remote_expansion_script(raw))
                .env("SCRATCH", "/scratch/me")
                .output()
                .expect("run sh");
            String::from_utf8_lossy(&out.stdout).to_string()
        };
        assert_eq!(run("$SCRATCH/run"), "/scratch/me/run");
        // Tilde: not expanded by double quotes, so the script rewrites it. These two
        // assertions failed before that rewrite existed, while the predicate test
        // above happily claimed `~/data` was "handled".
        let home = std::env::var("HOME").expect("HOME");
        assert_eq!(run("~/data"), format!("{home}/data"));
        assert_eq!(run("~"), home);
        // `~user` is deliberately NOT expanded — it stays literal rather than
        // reaching for the shell expansion we are withholding.
        assert_eq!(run("~alice/x"), "~alice/x");
        // Command substitution is defanged — the text survives verbatim.
        assert_eq!(run("/x/$(id)"), "/x/$(id)");
        assert_eq!(run("/x/`id`"), "/x/`id`");
        // And a quote cannot break out of the assignment.
        assert_eq!(
            run("/x/\"; touch /tmp/pwn; \""),
            "/x/\"; touch /tmp/pwn; \""
        );
    }

    #[test]
    fn expansion_cannot_execute_a_command() {
        let marker = std::env::temp_dir().join("operon-expand-test-marker");
        let _ = std::fs::remove_file(&marker);
        let payload = format!("/x/$(touch {})", marker.display());
        let out = std::process::Command::new("sh")
            .arg("-c")
            .arg(remote_expansion_script(&payload))
            .output()
            .expect("run sh");
        assert_eq!(String::from_utf8_lossy(&out.stdout), payload);
        assert!(!marker.exists(), "expansion executed a command");
    }

    #[test]
    fn scp_refuses_paths_a_legacy_client_would_execute() {
        // scp's remote operand cannot be quoted portably, so these are refused.
        for p in [
            "/data/$(touch /tmp/pwn)",
            "/data/`id`",
            "/data/a;rm -rf x",
            "/data/a&b",
            "/data/a|b",
            "/data/a>b",
            "/data/a<b",
            "/data/a(b)",
            "/data/a\\b",
            "/data/a\nb",
        ] {
            assert!(!scp_path_is_safe(p), "should refuse {p:?}");
        }
    }

    #[test]
    fn scp_still_allows_the_filenames_researchers_actually_use() {
        // Refusing must not cost normal usage: spaces, brackets, quotes, dots,
        // dashes and non-ASCII are all harmless to a shell operand.
        for p in [
            "/data/sample 01.fastq.gz",
            "/data/run[1].txt",
            "/data/o'brien.csv",
            "/data/résultats.tsv",
            "/data/GSE12345_RAW.tar",
            "/dfs3b/scratch/user/-leading-dash",
            "/data/a{b}c",
            "/data/star*",
        ] {
            assert!(scp_path_is_safe(p), "should allow {p:?}");
        }
    }

    #[test]
    fn every_scp_operand_is_guarded() {
        // Four call sites build `host:path` for scp. Each must reject first.
        let src = production_code();
        let operands = src.matches("format!(\"{}:{}\", host_str").count();
        let guards = src.matches("reject_unsafe_scp_path(&").count();
        assert_eq!(
            operands, guards,
            "an scp host:path operand was added without a reject_unsafe_scp_path guard"
        );
        assert!(
            operands >= 4,
            "expected at least 4 scp operands, found {operands}"
        );
    }
}

#[cfg(test)]
mod remote_file_op_tests {
    //! The command builders above are exercised through a REAL shell here, so
    //! `cargo test` on a Mac runs them against BSD userland and CI's Linux
    //! runners against GNU — the two remotes Operon has to be right about.
    use super::*;

    const MACOS_LISTING: &str = "total 320\n\
-rw-r--r--@ 1 vivek-mbp  wheel  76800 Sep  6 17:56 bin.png\n\
drwxr-xr-x@ 2 vivek-mbp  wheel     64 Sep  6 17:56 dir with space\n\
-rwxr-xr-x@ 1 vivek-mbp  wheel      0 Sep  6 17:56 exec.sh\n\
drwxr-xr-x+ 2 vivek-mbp  wheel     64 Sep  6 17:56 sub\n\
-rw-r--r--@ 1 vivek-mbp  wheel      6 Sep  6 17:56 two  spaces.txt\n\
-rw-r--r--@ 1 vivek-mbp  wheel      0 Sep  6 17:56 \u{e9}_\u{fc}n\u{ef}code.txt\n\
-rw-r--r--  1 vivek-mbp  wheel      3 Jan  9  2024 ends@\n\
-rw-r--r--  1 vivek-mbp  wheel      3 Jan  9  2024 not a link -> really.txt\n";

    const GNU_LISTING: &str = "total 12\n\
-rw-r--r--. 1 user group 1234 Sep  5 10:10 selinux.txt\n\
lrwxrwxrwx  1 user group    7 Sep  5 10:10 dangling -> nowhere\n\
brw-rw----  1 root disk 8, 0 Sep  5 10:10 sda\n\
-rw-r--r--  1 user group    0 Sep  5 10:10 \ttabbed name\n\
ls: cannot access '/x/hidden': Permission denied\n\
drwxr-xr-x  2 user group 4096 Sep  5 10:10 .\n\
drwxr-xr-x  9 user group 4096 Sep  5 10:10 ..\n";

    fn names(v: &[FileEntry]) -> Vec<&str> {
        v.iter().map(|e| e.name.as_str()).collect()
    }

    fn entry<'a>(v: &'a [FileEntry], name: &str) -> &'a FileEntry {
        v.iter()
            .find(|e| e.name == name)
            .unwrap_or_else(|| panic!("missing {name:?} in {:?}", names(v)))
    }

    #[test]
    fn macos_listing_keeps_names_verbatim() {
        let v = parse_ls_long(MACOS_LISTING, "/Users/x/proj");
        assert_eq!(
            names(&v),
            vec![
                "dir with space",
                "sub",
                "bin.png",
                "ends@",
                "exec.sh",
                "not a link -> really.txt",
                "two  spaces.txt",
                "\u{e9}_\u{fc}n\u{ef}code.txt",
            ]
        );
        assert!(entry(&v, "sub").is_dir);
        assert!(!entry(&v, "exec.sh").is_dir);
        assert_eq!(entry(&v, "bin.png").size, 76800);
        assert_eq!(entry(&v, "bin.png").extension.as_deref(), Some("png"));
        assert_eq!(
            entry(&v, "two  spaces.txt").path,
            "/Users/x/proj/two  spaces.txt"
        );
        assert_eq!(entry(&v, "ends@").extension, None);
    }

    #[test]
    fn gnu_listing_survives_selinux_devices_tabs_and_error_lines() {
        let v = parse_ls_long(GNU_LISTING, "/x/");
        assert_eq!(
            names(&v),
            vec!["\ttabbed name", "dangling", "selinux.txt"],
            "device nodes, `.`/`..` and ls error lines must be dropped"
        );
        assert_eq!(entry(&v, "selinux.txt").size, 1234);
        assert_eq!(entry(&v, "dangling").path, "/x/dangling");
    }

    #[test]
    fn the_first_column_must_look_like_a_mode_string() {
        assert!(looks_like_ls_perms("-rw-r--r--"));
        assert!(looks_like_ls_perms("drwxr-xr-x@"));
        assert!(looks_like_ls_perms("-rw-r--r--."));
        assert!(looks_like_ls_perms("lrwxrwxrwx"));
        assert!(looks_like_ls_perms("-rwsr-sr-t"));
        assert!(!looks_like_ls_perms("ls:"));
        assert!(!looks_like_ls_perms("total"));
        assert!(!looks_like_ls_perms("-rw-r--r"));
    }

    #[test]
    fn base64_alphabet_check_rejects_a_usage_message() {
        assert!(looks_like_base64("iVBORw0KGgo="));
        assert!(looks_like_base64(""));
        let usage = "base64: invalid argument /p/x.png\nUsage:\tbase64 [-Ddh] [-b num]"
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>();
        assert!(!looks_like_base64(&usage));
    }

    #[test]
    fn base64_is_never_given_a_path_operand() {
        // BSD base64 has no positional argument; only the `<` form is portable.
        assert_eq!(
            remote_read_base64_cmd("/a/b c.png"),
            "base64 < '/a/b c.png'"
        );
        assert!(remote_write_finish_cmd("/a/f.txt")
            .contains("base64 -d < '/a/f.txt.__operon_tmp_b64__'"));
        assert!(remote_write_small_cmd("aGk=", "/a/f.txt").contains("printf %s aGk= | base64 -d"));
        assert!(remote_list_cmd("/a/b", true).starts_with("LC_ALL=C ls -lLA -- '/a/b'"));
        assert_eq!(remote_list_cmd("/a/b", false), "LC_ALL=C ls -lL -- '/a/b'");
    }

    #[cfg(unix)]
    mod through_a_real_shell {
        use super::*; // includes the file's `base64::Engine` import
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        use std::path::{Path, PathBuf};

        struct Scratch(PathBuf);
        impl Scratch {
            fn new(tag: &str) -> Self {
                let dir = std::env::temp_dir().join(format!(
                    "operon-fileops-{}-{}",
                    std::process::id(),
                    tag
                ));
                let _ = std::fs::remove_dir_all(&dir);
                std::fs::create_dir_all(&dir).unwrap();
                Scratch(dir)
            }
            fn path(&self, name: &str) -> String {
                self.0.join(name).to_string_lossy().into_owned()
            }
        }
        impl Drop for Scratch {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }

        fn sh(cmd: &str) -> std::process::Output {
            std::process::Command::new("sh")
                .arg("-c")
                .arg(cmd)
                .output()
                .expect("run sh")
        }

        /// Anything the write path should have cleaned up: its chunk file and
        /// its decode temp (`mktemp` names it `.operon.XXXXXX` beside the
        /// target).
        fn leftovers(dir: &Path) -> Vec<String> {
            std::fs::read_dir(dir)
                .unwrap()
                .flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| n.contains("__operon_") || n.starts_with(".operon."))
                .collect()
        }

        fn is_root() -> bool {
            String::from_utf8_lossy(&sh("id -u").stdout).trim() == "0"
        }

        #[test]
        fn base64_read_round_trips_a_binary_file() {
            let s = Scratch::new("read");
            let path = s.path("all bytes  (2).png");
            let data: Vec<u8> = (0..=255u8).cycle().take(3000).collect();
            std::fs::write(&path, &data).unwrap();
            let out = sh(&remote_read_base64_cmd(&path));
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            let text: String = String::from_utf8_lossy(&out.stdout)
                .chars()
                .filter(|c| !c.is_whitespace())
                .collect();
            assert!(looks_like_base64(&text));
            let decoded = base64::engine::general_purpose::STANDARD
                .decode(text.as_bytes())
                .unwrap();
            assert_eq!(decoded, data);
        }

        #[test]
        fn base64_read_of_a_missing_file_exits_non_zero() {
            let s = Scratch::new("read-missing");
            let out = sh(&remote_read_base64_cmd(&s.path("nope.png")));
            assert!(!out.status.success());
            assert!(String::from_utf8_lossy(&out.stdout).trim().is_empty());
        }

        #[test]
        fn small_write_round_trips_and_keeps_the_inode() {
            let s = Scratch::new("write-small");
            let path = s.path("notes 'quoted'.md");
            std::fs::write(&path, "old").unwrap();
            let ino = std::fs::metadata(&path).unwrap().ino();
            let content =
                "line one\n\"quotes\" and 'apostrophes' and $HOME and \\backslash\n\u{e9}\u{fc}\n";
            let b64 = base64::engine::general_purpose::STANDARD.encode(content.as_bytes());
            let out = sh(&remote_write_small_cmd(&b64, &path));
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            assert_eq!(std::fs::read_to_string(&path).unwrap(), content);
            assert_eq!(
                std::fs::metadata(&path).unwrap().ino(),
                ino,
                "must write in place"
            );
            assert!(leftovers(&s.0).is_empty(), "{:?}", leftovers(&s.0));
        }

        #[test]
        fn small_write_with_a_bad_payload_leaves_the_target_untouched() {
            let s = Scratch::new("write-bad");
            let path = s.path("precious.csv");
            std::fs::write(&path, "PRECIOUS").unwrap();
            let out = sh(&remote_write_small_cmd("@@@@not-base64@@@@", &path));
            assert!(
                !out.status.success(),
                "a failed decode must be a failed save"
            );
            assert_eq!(std::fs::read_to_string(&path).unwrap(), "PRECIOUS");
            assert!(String::from_utf8_lossy(&out.stderr).contains("decode failed"));
            assert!(leftovers(&s.0).is_empty(), "{:?}", leftovers(&s.0));
        }

        #[test]
        fn chunked_write_round_trips_and_cleans_up() {
            let s = Scratch::new("write-chunked");
            let path = s.path("big.tsv");
            let content: String = (0..12_000).map(|i| format!("{i}\tsome text\n")).collect();
            let b64 = base64::engine::general_purpose::STANDARD.encode(content.as_bytes());
            assert!(b64.len() > 100_000);
            let mut first = true;
            for chunk in b64.as_bytes().chunks(100_000) {
                let out = sh(&remote_write_chunk_cmd(
                    std::str::from_utf8(chunk).unwrap(),
                    &path,
                    first,
                ));
                assert!(
                    out.status.success(),
                    "{}",
                    String::from_utf8_lossy(&out.stderr)
                );
                first = false;
            }
            let out = sh(&remote_write_finish_cmd(&path));
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            assert_eq!(std::fs::read_to_string(&path).unwrap(), content);
            assert!(leftovers(&s.0).is_empty(), "{:?}", leftovers(&s.0));
        }

        #[test]
        fn chunked_write_with_a_corrupt_chunk_file_leaves_the_target_untouched() {
            let s = Scratch::new("write-chunked-bad");
            let path = s.path("precious.tsv");
            std::fs::write(&path, "PRECIOUS").unwrap();
            assert!(sh(&remote_write_chunk_cmd("@@@@", &path, true))
                .status
                .success());
            let out = sh(&remote_write_finish_cmd(&path));
            assert!(!out.status.success());
            assert_eq!(std::fs::read_to_string(&path).unwrap(), "PRECIOUS");
            assert!(
                leftovers(&s.0).is_empty(),
                "chunk file must be removed: {:?}",
                leftovers(&s.0)
            );
        }

        #[test]
        fn write_onto_an_unwritable_target_fails_and_keeps_the_content_somewhere() {
            if is_root() {
                return; // root can write anything; nothing to prove
            }
            let s = Scratch::new("write-readonly");
            let path = s.path("readonly.txt");
            std::fs::write(&path, "PRECIOUS").unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o444)).unwrap();
            let b64 = base64::engine::general_purpose::STANDARD.encode(b"NEW");
            let out = sh(&remote_write_small_cmd(&b64, &path));
            assert!(!out.status.success());
            assert_eq!(std::fs::read_to_string(&path).unwrap(), "PRECIOUS");
            let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
            assert!(stderr.contains("left at"), "{stderr}");
            // The message names where the decoded content survived; clean it up.
            if let Some(kept) = stderr.split("left at ").nth(1).map(|t| t.trim()) {
                assert_eq!(std::fs::read(kept).unwrap(), b"NEW");
                let _ = std::fs::remove_file(kept);
            }
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        }

        #[test]
        fn the_real_ls_output_parses_on_this_platform() {
            let s = Scratch::new("ls");
            std::fs::write(s.path("two  spaces.txt"), "x").unwrap();
            std::fs::write(s.path("ends@"), "x").unwrap();
            std::fs::write(s.path(".hidden"), "x").unwrap();
            std::fs::create_dir(s.path("sub dir")).unwrap();
            let dir = s.0.to_string_lossy().into_owned();

            let out = sh(&remote_list_cmd(&dir, true));
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            let v = parse_ls_long(&String::from_utf8_lossy(&out.stdout), &dir);
            assert_eq!(
                names(&v),
                vec!["sub dir", ".hidden", "ends@", "two  spaces.txt"]
            );
            assert!(entry(&v, "sub dir").is_dir);
            assert_eq!(entry(&v, "two  spaces.txt").size, 1);

            let out = sh(&remote_list_cmd(&dir, false));
            let v = parse_ls_long(&String::from_utf8_lossy(&out.stdout), &dir);
            assert_eq!(names(&v), vec!["sub dir", "ends@", "two  spaces.txt"]);

            let out = sh(&remote_list_cmd(&s.path("missing"), false));
            assert!(!out.status.success());
            assert!(parse_ls_long(&String::from_utf8_lossy(&out.stdout), &dir).is_empty());
        }
    }
}
