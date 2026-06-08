//! HPC job watchdog — Operon 0.6.1 Phase 1–5.
//!
//! A long-running bash agent (`scripts/operon-watchdog.sh`) is uploaded to the
//! remote host and launched inside a dedicated tmux session. It polls the
//! scheduler (SLURM today, PBS/LSF stubs below) for every job in
//! `~/.operon/watchlist`, appends NDJSON events to `~/.operon/jobs/<id>.jsonl`,
//! and applies the policy in `~/.operon/policy.json` (auto-resubmit on
//! TIMEOUT / OOM up to a retry budget).
//!
//! Unlike the Claude-side turn loop, this watchdog:
//!   * survives Operon quitting (tmux-detached)
//!   * survives the login session ending (nohup / tmux)
//!   * costs $0 (no LLM turns to poll)
//!
//! Operon streams events back via `tail_job_events` (same SSH tail pattern as
//! the existing HPC Claude session), and the JobsView sidebar + optional
//! ChatPanel banner surface the live state.
//!
//! ── Scheduler abstraction ──
//! The watchdog bash script handles SLURM directly. On the Rust side we keep
//! a thin `Scheduler` trait so future support (PBS/LSF/SGE) can slot in.

use base64::Engine;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter};

use super::ssh::{ssh_exec, SSHManager};

// ─── Scheduler abstraction ───────────────────────────────────────────────

/// Supported HPC schedulers. SLURM is fully wired; the others are recognized
/// so protocol code can target them but the watchdog bash only polls SLURM
/// for 0.6.1. PBS/LSF stubs are here to lock down the public API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scheduler {
    Slurm,
    Pbs,
    Lsf,
    Sge,
}

impl Scheduler {
    #[allow(dead_code)]
    pub fn as_str(&self) -> &'static str {
        match self {
            Scheduler::Slurm => "slurm",
            Scheduler::Pbs => "pbs",
            Scheduler::Lsf => "lsf",
            Scheduler::Sge => "sge",
        }
    }

    /// Shell expression (stdout-only) that prints the scheduler name if its
    /// submit binary is on PATH, else empty. Used by `detect_scheduler`.
    pub fn detect_script() -> &'static str {
        r#"
if command -v sbatch >/dev/null 2>&1; then echo slurm
elif command -v qsub >/dev/null 2>&1 && command -v qstat >/dev/null 2>&1; then echo pbs
elif command -v bsub >/dev/null 2>&1; then echo lsf
elif command -v qsub >/dev/null 2>&1; then echo sge
else echo none
fi
"#
    }
}

// ─── Types ──────────────────────────────────────────────────────────────

/// A job Operon is tracking on a given SSH profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchedJob {
    pub profile_id: String,
    pub job_id: String,
    pub scheduler: String,
    pub submit_ts: u64,
    /// Path (on the remote) to the sbatch script — used for auto-resubmit.
    pub sbatch_path: Option<String>,
    pub retries_left: u32,
}

/// Policy describing how to react to terminal job states.
/// Serialized to `~/.operon/policy.json` on the remote so the bash watchdog
/// can read it without bringing a JSON parser.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobPolicy {
    /// Maximum auto-resubmits per original job.
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    /// Multiplier for --time when resubmitting after TIMEOUT. (Applied in-app
    /// when we rewrite the sbatch, not by the bash watchdog itself.)
    #[serde(default = "default_walltime_mult")]
    pub on_timeout_walltime_mult: f32,
    /// Multiplier for --mem when resubmitting after OOM.
    #[serde(default = "default_mem_mult")]
    pub on_oom_mem_mult: f32,
}

fn default_max_retries() -> u32 {
    2
}
fn default_walltime_mult() -> f32 {
    1.5
}
fn default_mem_mult() -> f32 {
    2.0
}

impl Default for JobPolicy {
    fn default() -> Self {
        Self {
            max_retries: default_max_retries(),
            on_timeout_walltime_mult: default_walltime_mult(),
            on_oom_mem_mult: default_mem_mult(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct WatchdogStatus {
    pub installed: bool,
    pub running: bool,
    pub tmux_session: Option<String>,
    pub scheduler: Option<String>,
    pub watchlist_len: usize,
}

// ─── Manager (track which tails are open per-session) ───────────────────

#[derive(Default)]
pub struct WatchdogManager {
    // session_id -> child handle
    pub tails: Mutex<HashMap<String, tokio::process::Child>>,
}

impl WatchdogManager {
    pub fn new() -> Self {
        Self::default()
    }
}

// ─── Remote paths ────────────────────────────────────────────────────────

const REMOTE_DIR: &str = "$HOME/.operon";
const REMOTE_SCRIPT: &str = "$HOME/.operon/operon-watchdog.sh";
const TMUX_SESSION: &str = "operon-watchdog";

fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

// ─── Commands: install / start / stop / status ──────────────────────────

/// Detect which scheduler is on PATH for the given profile.
#[tauri::command]
pub async fn detect_scheduler(
    ssh_state: tauri::State<'_, SSHManager>,
    profile_id: String,
) -> Result<String, String> {
    let profile = ssh_state
        .profiles
        .lock()
        .map_err(|e| e.to_string())?
        .iter()
        .find(|p| p.id == profile_id)
        .cloned()
        .ok_or_else(|| format!("SSH profile {} not found", profile_id))?;
    let out = ssh_exec(&profile, Scheduler::detect_script())?;
    Ok(out.trim().to_string())
}

/// Upload `scripts/operon-watchdog.sh` to the remote host at
/// `~/.operon/operon-watchdog.sh` and seed a default policy.json.
#[tauri::command]
pub async fn install_watchdog(
    ssh_state: tauri::State<'_, SSHManager>,
    profile_id: String,
) -> Result<(), String> {
    let profile = ssh_state
        .profiles
        .lock()
        .map_err(|e| e.to_string())?
        .iter()
        .find(|p| p.id == profile_id)
        .cloned()
        .ok_or_else(|| format!("SSH profile {} not found", profile_id))?;

    // Embedded script — avoids depending on the distribution layout.
    let script = include_str!("../../../scripts/operon-watchdog.sh");
    let b64 = base64::engine::general_purpose::STANDARD.encode(script.as_bytes());

    // mkdir, decode, chmod +x
    let mkdir_cmd = format!("mkdir -p {}/jobs && chmod 700 {}", REMOTE_DIR, REMOTE_DIR);
    ssh_exec(&profile, &mkdir_cmd).map_err(|e| format!("mkdir failed: {}", e))?;

    let write_cmd = format!(
        "printf %s {} | base64 -d > {} && chmod +x {}",
        b64, REMOTE_SCRIPT, REMOTE_SCRIPT
    );
    ssh_exec(&profile, &write_cmd).map_err(|e| format!("upload failed: {}", e))?;

    // Seed a default policy if none exists.
    let default_policy = serde_json::to_string(&JobPolicy::default())
        .map_err(|e| format!("policy serialize: {}", e))?;
    let policy_cmd = format!(
        "[ -f {dir}/policy.json ] || printf %s {json} > {dir}/policy.json",
        dir = REMOTE_DIR,
        json = shell_quote(&default_policy),
    );
    ssh_exec(&profile, &policy_cmd).map_err(|e| format!("policy seed failed: {}", e))?;

    Ok(())
}

/// Start the watchdog inside a detached tmux session (creates one if needed).
/// Requires `install_watchdog` to have been run at least once.
#[tauri::command]
pub async fn start_watchdog(
    ssh_state: tauri::State<'_, SSHManager>,
    profile_id: String,
) -> Result<(), String> {
    let profile = ssh_state
        .profiles
        .lock()
        .map_err(|e| e.to_string())?
        .iter()
        .find(|p| p.id == profile_id)
        .cloned()
        .ok_or_else(|| format!("SSH profile {} not found", profile_id))?;

    // `-A` attaches if the session exists, creates if not. Combined with `-d`
    // we get idempotent "detached, running" semantics. Inside the session we
    // run the script in a loop so if it crashes tmux shows the error.
    //
    // If tmux is missing, fall back to nohup (still survives logout).
    let cmd = format!(
        "if command -v tmux >/dev/null 2>&1; then \
           tmux has-session -t {session} 2>/dev/null || \
             tmux new-session -d -s {session} -A {script}; \
           echo tmux; \
         else \
           nohup bash {script} </dev/null >/dev/null 2>&1 & disown; \
           echo nohup; \
         fi",
        session = TMUX_SESSION,
        script = REMOTE_SCRIPT,
    );
    ssh_exec(&profile, &cmd).map_err(|e| format!("start failed: {}", e))?;
    Ok(())
}

/// Kill the tmux session (and thus the watchdog) on the remote host.
#[tauri::command]
pub async fn stop_watchdog(
    ssh_state: tauri::State<'_, SSHManager>,
    profile_id: String,
) -> Result<(), String> {
    let profile = ssh_state
        .profiles
        .lock()
        .map_err(|e| e.to_string())?
        .iter()
        .find(|p| p.id == profile_id)
        .cloned()
        .ok_or_else(|| format!("SSH profile {} not found", profile_id))?;
    let cmd = format!(
        "tmux kill-session -t {session} 2>/dev/null; \
         if [ -f $HOME/.operon/watchdog.pid ]; then \
           kill $(cat $HOME/.operon/watchdog.pid) 2>/dev/null; \
           rm -f $HOME/.operon/watchdog.pid; \
         fi; echo ok",
        session = TMUX_SESSION,
    );
    ssh_exec(&profile, &cmd).map_err(|e| format!("stop failed: {}", e))?;
    Ok(())
}

#[tauri::command]
pub async fn watchdog_status(
    ssh_state: tauri::State<'_, SSHManager>,
    profile_id: String,
) -> Result<WatchdogStatus, String> {
    let profile = ssh_state
        .profiles
        .lock()
        .map_err(|e| e.to_string())?
        .iter()
        .find(|p| p.id == profile_id)
        .cloned()
        .ok_or_else(|| format!("SSH profile {} not found", profile_id))?;

    // single round-trip — prints 5 lines we parse.
    let script = r#"
if [ -f $HOME/.operon/operon-watchdog.sh ]; then echo installed=1; else echo installed=0; fi
if tmux has-session -t operon-watchdog 2>/dev/null; then echo running=1; else echo running=0; fi
if command -v sbatch >/dev/null 2>&1; then echo scheduler=slurm
elif command -v qsub >/dev/null 2>&1; then echo scheduler=pbs
elif command -v bsub >/dev/null 2>&1; then echo scheduler=lsf
else echo scheduler=
fi
if [ -f $HOME/.operon/watchlist ]; then wc -l < $HOME/.operon/watchlist; else echo 0; fi
"#;
    let out = ssh_exec(&profile, script)?;
    let mut installed = false;
    let mut running = false;
    let mut scheduler: Option<String> = None;
    let mut watchlist_len: usize = 0;
    for line in out.lines() {
        let line = line.trim();
        if let Some(v) = line.strip_prefix("installed=") {
            installed = v == "1";
        } else if let Some(v) = line.strip_prefix("running=") {
            running = v == "1";
        } else if let Some(v) = line.strip_prefix("scheduler=") {
            let v = v.trim();
            if !v.is_empty() {
                scheduler = Some(v.to_string());
            }
        } else if let Ok(n) = line.parse::<usize>() {
            watchlist_len = n;
        }
    }
    Ok(WatchdogStatus {
        installed,
        running,
        tmux_session: Some(TMUX_SESSION.to_string()),
        scheduler,
        watchlist_len,
    })
}

// ─── Commands: watchlist + policy ───────────────────────────────────────

#[tauri::command]
pub async fn register_watched_job(
    ssh_state: tauri::State<'_, SSHManager>,
    profile_id: String,
    job_id: String,
    scheduler: Option<String>,
    sbatch_path: Option<String>,
) -> Result<(), String> {
    let profile = ssh_state
        .profiles
        .lock()
        .map_err(|e| e.to_string())?
        .iter()
        .find(|p| p.id == profile_id)
        .cloned()
        .ok_or_else(|| format!("SSH profile {} not found", profile_id))?;

    let sched = scheduler.unwrap_or_else(|| "slurm".to_string());
    let sbatch = sbatch_path.unwrap_or_default();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    // Append atomically. Tab-separated: job_id \t scheduler \t submit_ts \t sbatch \t retries_left.
    // Dedup by scanning for job_id first so repeat registrations are no-ops.
    let line = format!(
        "{}\t{}\t{}\t{}\t{}",
        job_id,
        sched,
        now,
        sbatch,
        default_max_retries()
    );
    let cmd = format!(
        "mkdir -p {dir}/jobs && \
         if [ -f {dir}/watchlist ] && grep -q \"^{jid}\\b\" {dir}/watchlist; then \
           echo 'already watched'; \
         else \
           printf '%s\\n' {line} >> {dir}/watchlist; \
           echo 'ok'; \
         fi",
        dir = REMOTE_DIR,
        jid = job_id,
        line = shell_quote(&line),
    );
    ssh_exec(&profile, &cmd).map_err(|e| format!("register failed: {}", e))?;
    Ok(())
}

#[tauri::command]
pub async fn unregister_watched_job(
    ssh_state: tauri::State<'_, SSHManager>,
    profile_id: String,
    job_id: String,
) -> Result<(), String> {
    let profile = ssh_state
        .profiles
        .lock()
        .map_err(|e| e.to_string())?
        .iter()
        .find(|p| p.id == profile_id)
        .cloned()
        .ok_or_else(|| format!("SSH profile {} not found", profile_id))?;
    // grep -v leaves the line out; tolerate an empty result with `|| true`.
    let cmd = format!(
        "touch {dir}/watchlist && grep -v \"^{jid}\\b\" {dir}/watchlist > {dir}/watchlist.tmp || true; \
         mv {dir}/watchlist.tmp {dir}/watchlist",
        dir = REMOTE_DIR,
        jid = job_id,
    );
    ssh_exec(&profile, &cmd).map_err(|e| format!("unregister failed: {}", e))?;
    Ok(())
}

#[tauri::command]
pub async fn list_watched_jobs(
    ssh_state: tauri::State<'_, SSHManager>,
    profile_id: String,
) -> Result<Vec<WatchedJob>, String> {
    let profile = ssh_state
        .profiles
        .lock()
        .map_err(|e| e.to_string())?
        .iter()
        .find(|p| p.id == profile_id)
        .cloned()
        .ok_or_else(|| format!("SSH profile {} not found", profile_id))?;
    let out = ssh_exec(&profile, "cat $HOME/.operon/watchlist 2>/dev/null").unwrap_or_default();
    let mut jobs = Vec::new();
    for line in out.lines() {
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() < 5 {
            continue;
        }
        jobs.push(WatchedJob {
            profile_id: profile_id.clone(),
            job_id: fields[0].to_string(),
            scheduler: fields[1].to_string(),
            submit_ts: fields[2].parse().unwrap_or(0),
            sbatch_path: if fields[3].is_empty() {
                None
            } else {
                Some(fields[3].to_string())
            },
            retries_left: fields[4].parse().unwrap_or(0),
        });
    }
    Ok(jobs)
}

#[tauri::command]
pub async fn get_job_policy(
    ssh_state: tauri::State<'_, SSHManager>,
    profile_id: String,
) -> Result<JobPolicy, String> {
    let profile = ssh_state
        .profiles
        .lock()
        .map_err(|e| e.to_string())?
        .iter()
        .find(|p| p.id == profile_id)
        .cloned()
        .ok_or_else(|| format!("SSH profile {} not found", profile_id))?;
    let out = ssh_exec(&profile, "cat $HOME/.operon/policy.json 2>/dev/null").unwrap_or_default();
    if out.trim().is_empty() {
        return Ok(JobPolicy::default());
    }
    serde_json::from_str(&out).map_err(|e| format!("policy parse: {}", e))
}

#[tauri::command]
pub async fn set_job_policy(
    ssh_state: tauri::State<'_, SSHManager>,
    profile_id: String,
    policy: JobPolicy,
) -> Result<(), String> {
    let profile = ssh_state
        .profiles
        .lock()
        .map_err(|e| e.to_string())?
        .iter()
        .find(|p| p.id == profile_id)
        .cloned()
        .ok_or_else(|| format!("SSH profile {} not found", profile_id))?;
    let json = serde_json::to_string(&policy).map_err(|e| format!("policy serialize: {}", e))?;
    let cmd = format!(
        "mkdir -p {dir} && printf %s {json} > {dir}/policy.json",
        dir = REMOTE_DIR,
        json = shell_quote(&json),
    );
    ssh_exec(&profile, &cmd).map_err(|e| format!("policy write: {}", e))?;
    Ok(())
}

// ─── Commands: event tail ───────────────────────────────────────────────

/// Read the full event log for a job (non-streaming).
#[tauri::command]
pub async fn read_job_events(
    ssh_state: tauri::State<'_, SSHManager>,
    profile_id: String,
    job_id: String,
) -> Result<String, String> {
    let profile = ssh_state
        .profiles
        .lock()
        .map_err(|e| e.to_string())?
        .iter()
        .find(|p| p.id == profile_id)
        .cloned()
        .ok_or_else(|| format!("SSH profile {} not found", profile_id))?;
    let cmd = format!(
        "cat $HOME/.operon/jobs/{}.jsonl 2>/dev/null",
        shell_quote(&job_id)
    );
    ssh_exec(&profile, &cmd)
}

/// Start tailing a job's event log and emit each NDJSON line as
/// `job-event-<job_id>`. Cancel by calling `stop_job_tail`.
#[tauri::command]
pub async fn start_job_tail(
    app: AppHandle,
    ssh_state: tauri::State<'_, SSHManager>,
    watchdog_state: tauri::State<'_, WatchdogManager>,
    profile_id: String,
    job_id: String,
) -> Result<(), String> {
    use tokio::io::{AsyncBufReadExt, BufReader};
    use tokio::process::Command as AsyncCommand;

    let profile = ssh_state
        .profiles
        .lock()
        .map_err(|e| e.to_string())?
        .iter()
        .find(|p| p.id == profile_id)
        .cloned()
        .ok_or_else(|| format!("SSH profile {} not found", profile_id))?;

    // Stop any previous tail for this (profile, job) pair.
    let key = format!("{}::{}", profile_id, job_id);
    {
        let mut tails = watchdog_state.tails.lock().map_err(|e| e.to_string())?;
        if let Some(mut prev) = tails.remove(&key) {
            let _ = prev.start_kill();
        }
    }

    // Remote script: wait for file, then tail -n +1 -f, line-buffered.
    let tail_script = format!(
        "f=$HOME/.operon/jobs/{jid}.jsonl; \
         i=0; while [ ! -f \"$f\" ] && [ $i -lt 600 ]; do sleep 0.5; i=$((i+1)); done; \
         [ -f \"$f\" ] || exit 0; \
         if command -v stdbuf >/dev/null 2>&1; then \
           stdbuf -oL tail -n +1 -f \"$f\"; \
         else tail -n +1 -f \"$f\"; fi",
        jid = job_id,
    );
    let b64 = base64::engine::general_purpose::STANDARD.encode(tail_script.as_bytes());

    let mut ssh_args = format!(
        "ssh -o BatchMode=yes -o ConnectTimeout=10 -o ServerAliveInterval=15 {}@{} -p {}",
        profile.user, profile.host, profile.port
    );
    if crate::platform::supports_ssh_mux() {
        let sock = crate::platform::ssh_socket_path(&profile.host, profile.port, &profile.user);
        if sock.exists() {
            ssh_args.push_str(&format!(" -o ControlPath={}", sock.to_string_lossy()));
        }
    }
    if let Some(key) = &profile.key_file {
        // Single-quote the key path: it runs through `bash -l -c`, and a
        // Windows path (C:\Users\...) would otherwise have its backslashes
        // eaten as shell escapes, breaking key auth.
        ssh_args.push_str(&format!(" -i '{}'", key.replace('\'', "'\\''")));
    }
    ssh_args.push_str(&format!(" \"echo {} | base64 -d | bash\"", b64));

    // The `-l -c` invocation below needs a POSIX shell. On Windows
    // `default_shell()` is cmd.exe, which rejects `-l`/`-c` — use Git Bash.
    let shell =
        crate::platform::find_git_bash_path().unwrap_or_else(crate::platform::default_shell);
    let mut cmd = AsyncCommand::new(&shell);
    cmd.arg("-l").arg("-c").arg(&ssh_args);
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        cmd.creation_flags(0x08000000);
    }

    let mut child = cmd.spawn().map_err(|e| format!("spawn tail: {}", e))?;
    let stdout = child.stdout.take().ok_or("no stdout")?;

    let app_handle = app.clone();
    let evt = format!("job-event-{}", job_id);
    tokio::spawn(async move {
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if line.trim().is_empty() {
                continue;
            }
            let _ = app_handle.emit(&evt, line);
        }
        let _ = app_handle.emit(&format!("job-tail-exit-{}", job_id), ());
    });

    watchdog_state
        .tails
        .lock()
        .map_err(|e| e.to_string())?
        .insert(key, child);
    Ok(())
}

#[tauri::command]
pub async fn stop_job_tail(
    watchdog_state: tauri::State<'_, WatchdogManager>,
    profile_id: String,
    job_id: String,
) -> Result<(), String> {
    let key = format!("{}::{}", profile_id, job_id);
    let mut tails = watchdog_state.tails.lock().map_err(|e| e.to_string())?;
    if let Some(mut child) = tails.remove(&key) {
        let _ = child.start_kill();
    }
    Ok(())
}
