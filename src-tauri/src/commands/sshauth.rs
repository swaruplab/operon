// ── SSH key passphrase support (keychain + ssh-agent) ───────────────────────
//
// Problem this solves: a user with an EXISTING passphrase-protected private key
// (UCI RCIC's recommended setup) could not use Operon's file explorer. The
// interactive terminal can prompt for the passphrase, but every *background*
// connection (file listing/read, scp, the libssh2 SFTP downloads) runs
// non-interactively under `BatchMode=yes`, which cannot prompt — so the key
// stays locked and auth fails ("explorer fails to communicate").
//
// The fix, uniform across macOS/Linux/Windows:
//   1. The passphrase is captured once in the UI and stored in the OS keychain
//      (macOS Keychain / Windows Credential Manager / Linux Secret Service) via
//      the `keyring` crate — never in the plaintext ssh_profiles.json.
//   2. At connect time we load the decrypted key into an ssh-agent that Operon
//      owns (or an existing one already in the environment), feeding the
//      passphrase to `ssh-add` non-interactively through SSH_ASKPASS.
//   3. We publish that agent's `SSH_AUTH_SOCK` into Operon's own process
//      environment, so every child process Operon spawns (ssh, scp, the PTY
//      terminal) inherits it and authenticates against the agent.
//   4. For the libssh2 SFTP download path we additionally pass the passphrase
//      straight to `userauth_pubkey_file` (libssh2's Windows agent client does
//      not read Git's MSYS agent socket), so downloads work without the agent.
//
// NOTE (Windows): the OpenSSH binaries Operon spawns for the explorer (ssh/scp)
// are resolved by PATH and may be either Git's MSYS build or Windows-native
// OpenSSH. The agent socket published here is Git's MSYS socket; on a box where
// Windows-native ssh.exe wins PATH resolution it cannot read that socket, so the
// agent path may not cover scp/exec there. The libssh2 download path (direct
// passphrase) and the Git-bash terminal still work. Routing all Windows backend
// ssh/scp through Git's binaries is a known follow-up.

use crate::commands::ssh::SSHProfile;
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

/// Keychain service name. The account is the SSH profile id.
const KEYRING_SERVICE: &str = "operon-ssh-key-passphrase";
/// Give up auto-loading a key after this many failed ssh-add attempts in a
/// session (bounds hammering on a genuinely bad key without permanently
/// disabling recovery — re-entering the passphrase resets the counter).
const MAX_LOAD_ATTEMPTS: u32 = 3;
/// Wall-clock ceiling for any OpenSSH helper subprocess. ensure_key_loaded runs
/// on the file-explorer hot path, so a wedged child must not hang the UI.
const SUBPROCESS_TIMEOUT_SECS: u64 = 20;

// ── Keychain (keyring crate) ────────────────────────────────────────────────

fn keyring_entry(profile_id: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYRING_SERVICE, profile_id)
        .map_err(|e| format!("keychain open failed: {}", e))
}

pub fn store_passphrase(profile_id: &str, passphrase: &str) -> Result<(), String> {
    keyring_entry(profile_id)?
        .set_password(passphrase)
        .map_err(|e| format!("keychain store failed: {}", e))
}

pub fn read_passphrase(profile_id: &str) -> Option<String> {
    keyring_entry(profile_id).ok()?.get_password().ok()
}

pub fn delete_passphrase(profile_id: &str) -> Result<(), String> {
    match keyring_entry(profile_id)?.delete_credential() {
        Ok(_) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("keychain delete failed: {}", e)),
    }
}

// ── ssh-agent state ─────────────────────────────────────────────────────────

struct AgentState {
    /// SSH_AUTH_SOCK of the agent we are using (owned or inherited).
    sock: Option<String>,
    /// SSH_AGENT_PID — Some only when WE spawned the agent (so we can kill it).
    pid: Option<String>,
    /// Whether Operon spawned this agent (true) or reused an existing one.
    owned: bool,
    /// Private-key paths successfully ssh-add-ed this session (dedup).
    loaded: HashSet<String>,
    /// Failed ssh-add attempt counts per key path (bounded retry).
    attempts: HashMap<String, u32>,
}

fn agent() -> &'static Mutex<AgentState> {
    static AGENT: OnceLock<Mutex<AgentState>> = OnceLock::new();
    AGENT.get_or_init(|| {
        Mutex::new(AgentState {
            sock: None,
            pid: None,
            owned: false,
            loaded: HashSet::new(),
            attempts: HashMap::new(),
        })
    })
}

/// The active agent's SSH_AUTH_SOCK, if one has been established. Used by
/// callers that build a shell command string and want to pin the agent
/// explicitly (defeating a login-shell rc that re-points SSH_AUTH_SOCK).
pub fn current_auth_sock() -> Option<String> {
    agent().lock().ok().and_then(|g| g.sock.clone())
}

// ── Platform binary resolution ──────────────────────────────────────────────

#[cfg(windows)]
fn git_companion(tool: &str) -> Option<String> {
    // ssh-add.exe / ssh-agent.exe / ssh-keygen.exe live next to the ssh.exe
    // Operon already uses: <git>\usr\bin\, derived from <git>\bin\bash.exe.
    let bash = crate::platform::find_git_bash_path()?;
    let git_root = std::path::Path::new(&bash).parent()?.parent()?;
    let p = git_root.join("usr").join("bin").join(format!("{}.exe", tool));
    if p.exists() {
        Some(p.to_string_lossy().to_string())
    } else {
        None
    }
}

fn ssh_add_program() -> String {
    #[cfg(windows)]
    {
        git_companion("ssh-add").unwrap_or_else(|| "ssh-add.exe".to_string())
    }
    #[cfg(not(windows))]
    {
        "ssh-add".to_string()
    }
}

fn ssh_agent_program() -> String {
    #[cfg(windows)]
    {
        git_companion("ssh-agent").unwrap_or_else(|| "ssh-agent.exe".to_string())
    }
    #[cfg(not(windows))]
    {
        "ssh-agent".to_string()
    }
}

fn ssh_keygen_program() -> String {
    #[cfg(windows)]
    {
        git_companion("ssh-keygen").unwrap_or_else(|| "ssh-keygen.exe".to_string())
    }
    #[cfg(not(windows))]
    {
        "ssh-keygen".to_string()
    }
}

#[cfg(windows)]
fn no_window(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NO_WINDOW);
}
#[cfg(not(windows))]
fn no_window(_cmd: &mut Command) {}

/// Run a configured Command with a wall-clock timeout. On expiry the child is
/// killed and an Err returned, so a wedged helper cannot hang the calling
/// thread (which may be a file-explorer operation). Output buffers are small
/// (one line), so reading after wait does not deadlock.
fn output_bounded(mut cmd: Command, secs: u64) -> Result<Output, String> {
    use wait_timeout::ChildExt;
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    no_window(&mut cmd);
    let mut child = cmd.spawn().map_err(|e| format!("spawn failed: {}", e))?;
    match child
        .wait_timeout(Duration::from_secs(secs))
        .map_err(|e| format!("wait failed: {}", e))?
    {
        Some(status) => {
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            if let Some(mut o) = child.stdout.take() {
                let _ = o.read_to_end(&mut stdout);
            }
            if let Some(mut e) = child.stderr.take() {
                let _ = e.read_to_end(&mut stderr);
            }
            Ok(Output {
                status,
                stdout,
                stderr,
            })
        }
        None => {
            let _ = child.kill();
            let _ = child.wait();
            Err("subprocess timed out".to_string())
        }
    }
}

// ── SSH_ASKPASS helper ──────────────────────────────────────────────────────
//
// `ssh-add` / `ssh-keygen` cannot read a passphrase from stdin; the standard
// non-interactive route is SSH_ASKPASS. We write a tiny script that echoes an
// environment variable. The SECRET is never written to the script — it is
// passed only in the child process's environment (OPERON_KEY_PASSPHRASE).
//
// Each call writes a UNIQUE file (pid + atomic counter) created atomically with
// O_CREAT|O_EXCL|0700 on Unix, then deletes it after the child returns. This
// avoids both the cross-user /tmp symlink-clobber attack and the concurrent
// truncate-then-write race a fixed shared path would have.

static ASKPASS_COUNTER: AtomicU64 = AtomicU64::new(0);

fn write_askpass_script() -> Result<std::path::PathBuf, String> {
    let nonce = ASKPASS_COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = crate::platform::temp_dir()
        .join(format!("operon-askpass-{}-{}.sh", std::process::id(), nonce));
    let body = b"#!/bin/sh\nprintf '%s\\n' \"$OPERON_KEY_PASSPHRASE\"\n";
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        // O_CREAT|O_EXCL|0700 — refuse to follow a pre-existing/symlinked path.
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o700)
            .open(&path)
            .map_err(|e| format!("create askpass: {}", e))?;
        f.write_all(body)
            .map_err(|e| format!("write askpass: {}", e))?;
    }
    #[cfg(not(unix))]
    {
        std::fs::write(&path, body).map_err(|e| format!("write askpass: {}", e))?;
    }
    Ok(path)
}

/// Run an OpenSSH helper that needs a passphrase, feeding it via SSH_ASKPASS.
/// `extra_env` lets callers add e.g. SSH_AUTH_SOCK for the target agent.
fn run_with_askpass(
    program: &str,
    args: &[&str],
    passphrase: &str,
    extra_env: &[(&str, String)],
) -> Result<Output, String> {
    let askpass = write_askpass_script()?;
    let mut cmd = Command::new(program);
    cmd.args(args)
        .env("SSH_ASKPASS", &askpass)
        // OpenSSH 8.4+: use askpass even with a tty / no DISPLAY.
        .env("SSH_ASKPASS_REQUIRE", "force")
        .env("OPERON_KEY_PASSPHRASE", passphrase)
        .env(
            "DISPLAY",
            std::env::var("DISPLAY").unwrap_or_else(|_| ":0".to_string()),
        );
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    let result = output_bounded(cmd, SUBPROCESS_TIMEOUT_SECS);
    let _ = std::fs::remove_file(&askpass);
    result
}

// ── Agent lifecycle ─────────────────────────────────────────────────────────

fn parse_agent_var(output: &str, name: &str) -> Option<String> {
    let needle = format!("{}=", name);
    for token in output.split([';', '\n']) {
        let token = token.trim();
        if let Some(rest) = token.strip_prefix(&needle) {
            let v = rest.trim();
            if !v.is_empty() {
                return Some(v.to_string());
            }
        }
    }
    None
}

/// Is the agent at `sock` reachable? `ssh-add -l` exits 0 (has keys) or 1
/// (reachable, no keys) when the agent answers, and 2 when it cannot connect.
fn agent_alive(sock: &str) -> bool {
    let mut cmd = Command::new(ssh_add_program());
    cmd.arg("-l").env("SSH_AUTH_SOCK", sock);
    match output_bounded(cmd, 8) {
        Ok(o) => o.status.code() != Some(2),
        Err(_) => false,
    }
}

/// Ensure an ssh-agent is available and return its SSH_AUTH_SOCK. Reuses an
/// agent already present in the environment; otherwise spawns one Operon owns.
/// Holds the agent lock across the whole body (it runs rarely) so two cold-start
/// callers cannot each spawn an agent and orphan one. Publishes SSH_AUTH_SOCK
/// into Operon's process env so child processes inherit it.
fn ensure_agent() -> Result<String, String> {
    let mut g = agent().lock().map_err(|e| e.to_string())?;

    // Reuse our established agent if it is still alive.
    if let Some(sock) = g.sock.clone() {
        if agent_alive(&sock) {
            return Ok(sock);
        }
        // Stale — fall through and re-establish.
        g.sock = None;
        g.pid = None;
        g.owned = false;
        g.loaded.clear();
    }

    // Prefer an existing, live agent already in the environment.
    if let Ok(existing) = std::env::var("SSH_AUTH_SOCK") {
        if !existing.is_empty() && agent_alive(&existing) {
            g.sock = Some(existing.clone());
            g.owned = false;
            return Ok(existing);
        }
    }

    // Spawn our own agent.
    let mut cmd = Command::new(ssh_agent_program());
    cmd.arg("-s");
    let out = output_bounded(cmd, 10).map_err(|e| format!("ssh-agent spawn failed: {}", e))?;
    if !out.status.success() {
        return Err(format!(
            "ssh-agent exited with error: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let sock = parse_agent_var(&stdout, "SSH_AUTH_SOCK")
        .ok_or_else(|| "ssh-agent did not report SSH_AUTH_SOCK".to_string())?;
    let pid = parse_agent_var(&stdout, "SSH_AGENT_PID");

    std::env::set_var("SSH_AUTH_SOCK", &sock);
    if let Some(p) = &pid {
        std::env::set_var("SSH_AGENT_PID", p);
    }

    g.sock = Some(sock.clone());
    g.pid = pid;
    g.owned = true;
    Ok(sock)
}

/// Load a profile's passphrase-protected key into the agent (idempotent).
/// Cheap and a no-op for profiles without a passphrase-protected key, so it is
/// safe to call on every SSH operation.
pub fn ensure_key_loaded(profile: &SSHProfile) {
    if !profile.key_has_passphrase {
        return;
    }
    let key = match &profile.key_file {
        Some(k) if std::path::Path::new(k).exists() => k.clone(),
        _ => return,
    };

    {
        if let Ok(g) = agent().lock() {
            if g.loaded.contains(&key) {
                return; // already in the agent
            }
            if g.attempts.get(&key).copied().unwrap_or(0) >= MAX_LOAD_ATTEMPTS {
                return; // gave up this session — re-entering the passphrase resets this
            }
        }
    }

    let passphrase = match read_passphrase(&profile.id) {
        Some(p) => p,
        None => return, // no stored passphrase — nothing we can do silently
    };
    let sock = match ensure_agent() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[operon-ssh] could not start ssh-agent: {}", e);
            return;
        }
    };

    let res = run_with_askpass(
        &ssh_add_program(),
        &[key.as_str()],
        &passphrase,
        &[("SSH_AUTH_SOCK", sock)],
    );

    // Record the outcome: mark loaded ONLY on success, so a transient failure is
    // retried on the next operation rather than being permanently suppressed.
    if let Ok(mut g) = agent().lock() {
        match &res {
            Ok(o) if o.status.success() => {
                g.loaded.insert(key.clone());
                g.attempts.remove(&key);
            }
            Ok(o) => {
                *g.attempts.entry(key.clone()).or_insert(0) += 1;
                eprintln!(
                    "[operon-ssh] ssh-add failed for {}: {}",
                    key,
                    String::from_utf8_lossy(&o.stderr).trim()
                );
            }
            Err(e) => {
                *g.attempts.entry(key.clone()).or_insert(0) += 1;
                eprintln!("[operon-ssh] ssh-add error for {}: {}", key, e);
            }
        }
    }
}

/// Forget the load/failure state for a key so the next operation re-attempts
/// ssh-add. Called when the user (re)stores a passphrase, so correcting a wrong
/// passphrase recovers without restarting Operon.
fn reset_key_state(key_file: &str) {
    if let Ok(mut g) = agent().lock() {
        g.loaded.remove(key_file);
        g.attempts.remove(key_file);
    }
}

/// Kill the agent if (and only if) Operon spawned it. Called on app exit.
pub fn shutdown_agent() {
    let (owned, sock, pid) = match agent().lock() {
        Ok(g) => (g.owned, g.sock.clone(), g.pid.clone()),
        Err(_) => return,
    };
    if !owned {
        return;
    }
    if let (Some(sock), Some(pid)) = (sock, pid) {
        let mut cmd = Command::new(ssh_agent_program());
        cmd.arg("-k")
            .env("SSH_AUTH_SOCK", sock)
            .env("SSH_AGENT_PID", pid);
        let _ = output_bounded(cmd, 5);
    }
}

// ── Key inspection ──────────────────────────────────────────────────────────

/// True if the private key at `path` is encrypted (needs a passphrase).
/// `ssh-keygen -y -P "" -f <key>` succeeds for an unencrypted key. For anything
/// else we FAIL OPEN (assume encrypted) unless the error clearly indicates the
/// file is not a usable private key — so a divergent OpenSSH wording or a
/// missing ssh-keygen never silently hides the passphrase field and locks the
/// user out. (Empty `-P ""` carries no secret, so it is safe on the cmd line.)
fn key_is_encrypted(path: &str) -> bool {
    if !std::path::Path::new(path).exists() {
        return false;
    }
    let mut cmd = Command::new(ssh_keygen_program());
    cmd.args(["-y", "-P", "", "-f", path]);
    match output_bounded(cmd, 8) {
        Ok(o) if o.status.success() => false, // empty passphrase worked → not encrypted
        Ok(o) => {
            let err = String::from_utf8_lossy(&o.stderr).to_lowercase();
            // Clearly-not-a-usable-private-key signals → not "encrypted".
            let not_a_key = err.contains("is not a public key")
                || err.contains("invalid format")
                || err.contains("no such file")
                || err.contains("bad permissions")
                || err.contains("unknown key type")
                || err.contains("could not load");
            !not_a_key // everything else (incl. "incorrect passphrase") → encrypted
        }
        Err(_) => true, // can't probe → offer the field rather than lock the user out
    }
}

/// Verify a passphrase unlocks the key, via SSH_ASKPASS (no arg leak).
/// `ssh-keygen -y -f <key>` exits 0 when the supplied passphrase is correct.
fn verify_passphrase(path: &str, passphrase: &str) -> Result<(), String> {
    let out = run_with_askpass(&ssh_keygen_program(), &["-y", "-f", path], passphrase, &[])?;
    if out.status.success() {
        Ok(())
    } else {
        Err("Incorrect passphrase for this key.".to_string())
    }
}

// ── Tauri commands ──────────────────────────────────────────────────────────

/// Store a key passphrase in the OS keychain (verifying it first). Pass
/// `key_file` to validate the passphrase against the actual key.
#[tauri::command]
pub fn set_ssh_key_passphrase(
    profile_id: String,
    passphrase: String,
    key_file: Option<String>,
) -> Result<(), String> {
    if let Some(kf) = &key_file {
        if std::path::Path::new(kf).exists() {
            verify_passphrase(kf, &passphrase)?;
        }
    }
    store_passphrase(&profile_id, &passphrase)?;
    // Allow the next operation to re-attempt ssh-add with the (corrected) passphrase.
    if let Some(kf) = &key_file {
        reset_key_state(kf);
    }
    Ok(())
}

/// Remove a stored key passphrase.
#[tauri::command]
pub fn delete_ssh_key_passphrase(profile_id: String) -> Result<(), String> {
    delete_passphrase(&profile_id)
}

/// Whether a passphrase is stored for this profile.
#[tauri::command]
pub fn has_ssh_key_passphrase(profile_id: String) -> bool {
    read_passphrase(&profile_id).is_some()
}

/// Whether the private key at `key_file` is encrypted (UI reveals the
/// passphrase field when true).
#[tauri::command]
pub fn key_needs_passphrase(key_file: String) -> bool {
    key_is_encrypted(&key_file)
}

/// Encrypt an existing private key with a passphrase (or change its passphrase),
/// in place, via `ssh-keygen -p`. The keypair itself is unchanged — only the
/// on-disk encryption — so the installed public key and server access still work.
/// `old_passphrase` is the key's CURRENT passphrase (None/empty if it has none).
/// On success the new passphrase is stored in the OS keychain for this profile.
#[tauri::command]
pub fn add_ssh_key_passphrase(
    profile_id: String,
    key_file: String,
    new_passphrase: String,
    old_passphrase: Option<String>,
) -> Result<(), String> {
    if new_passphrase.is_empty() {
        return Err("Passphrase cannot be empty.".to_string());
    }
    if !std::path::Path::new(&key_file).exists() {
        return Err(format!("Key file not found: {}", key_file));
    }
    let old = old_passphrase.as_deref().unwrap_or("");
    // ssh-keygen needs the old/new passphrases here (-P/-N); it has no askpass
    // path for the two-prompt -p flow. They are visible to a same-user `ps` for
    // the brief lifetime of this process — acceptable on the user's own machine.
    let mut cmd = Command::new(ssh_keygen_program());
    cmd.args(["-p", "-P", old, "-N", &new_passphrase, "-f", &key_file]);
    let out = output_bounded(cmd, SUBPROCESS_TIMEOUT_SECS)?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let err_l = err.to_lowercase();
        if err_l.contains("incorrect")
            || err_l.contains("failed to load")
            || err_l.contains("load failed")
        {
            return Err("Couldn't set the passphrase — the key's current passphrase is wrong, or it is already encrypted. If it already has a passphrase, enter it in the field above instead.".to_string());
        }
        return Err(format!("ssh-keygen -p failed: {}", err.trim()));
    }
    store_passphrase(&profile_id, &new_passphrase)?;
    reset_key_state(&key_file);
    Ok(())
}
