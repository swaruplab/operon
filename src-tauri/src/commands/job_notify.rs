// Job completion notifications.
//
// Builds on top of `watchdog.rs` — the bash daemon polls SLURM and writes
// terminal events into ~/.operon/jobs/<jobid>.jsonl on the remote. This
// module adds the local-side glue so Operon can:
//
//   * remember which agent session/profile owns each job + extra context
//     (job name, expected output) — stored at <config_dir>/job_registry.json
//   * detect "new" terminal events on app launch by comparing against a
//     last-seen marker stored at <config_dir>/job_notifications.json
//   * surface them as a banner in the chat panel + bounce the dock icon
//
// The actual reconciliation is a single SSH round-trip per profile that
// concatenates the JSONL files for all watched jobs. We grep for `"type":
// "terminal"` events and ship the freshest one for each job back to the
// frontend, along with the registry metadata so the banner can render the
// status-aware Option-C card.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::Manager;

use super::ssh::{ssh_exec, SSHManager};

#[allow(unused_imports)]
use tauri::Manager as _;

const REGISTRY_FILENAME: &str = "job_registry.json";
const SEEN_FILENAME: &str = "job_notifications.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobRegistryEntry {
    pub profile_id: String,
    pub job_id: String,
    /// Chat session that submitted this job (for routing the banner / resume).
    pub session_id: String,
    /// Human-readable session name shown in the banner ("hdWGCNA agent").
    pub session_name: String,
    /// SLURM job name (--job-name), if known.
    #[serde(default)]
    pub job_name: Option<String>,
    /// File the agent expected to produce. Optional — shown as
    /// "config/array_index.tsv is ready" on success when present.
    #[serde(default)]
    pub expected_output: Option<String>,
    /// Path on remote to the sbatch script (used for re-locating the log).
    #[serde(default)]
    pub sbatch_path: Option<String>,
    pub registered_at_ms: u64,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct JobRegistry {
    entries: Vec<JobRegistryEntry>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct SeenState {
    /// key = "{profile_id}::{job_id}", value = terminal-event ts_ms when last shown
    seen: HashMap<String, u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PendingCompletion {
    pub profile_id: String,
    pub job_id: String,
    pub session_id: String,
    pub session_name: String,
    pub job_name: Option<String>,
    pub expected_output: Option<String>,
    /// SLURM state — COMPLETED, FAILED, TIMEOUT, OUT_OF_MEMORY, CANCELLED, etc.
    pub state: String,
    pub exit_code: String,
    pub elapsed_seconds: u64,
    pub log_path: Option<String>,
    pub log_tail: String,
    pub terminal_at_ms: u64,
    /// Best-effort autodetected error line, only populated for FAILED.
    /// Empty string means "no confident pick — show (failed — see log) instead".
    pub last_error_line: String,
}

#[derive(Default)]
pub struct JobNotifyManager {
    registry: Mutex<JobRegistry>,
    seen: Mutex<SeenState>,
}

impl JobNotifyManager {
    pub fn new() -> Self {
        let mgr = Self::default();
        if let Some(reg) = load_registry() {
            *mgr.registry.lock().unwrap() = reg;
        }
        if let Some(seen) = load_seen() {
            *mgr.seen.lock().unwrap() = seen;
        }
        mgr
    }
}

fn config_path(name: &str) -> PathBuf {
    crate::platform::config_dir().join(name)
}

fn load_registry() -> Option<JobRegistry> {
    let path = config_path(REGISTRY_FILENAME);
    let data = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&data).ok()
}

fn save_registry(reg: &JobRegistry) -> Result<(), String> {
    let path = config_path(REGISTRY_FILENAME);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let data = serde_json::to_string_pretty(reg).map_err(|e| e.to_string())?;
    std::fs::write(&path, data).map_err(|e| e.to_string())
}

fn load_seen() -> Option<SeenState> {
    let path = config_path(SEEN_FILENAME);
    let data = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&data).ok()
}

fn save_seen(seen: &SeenState) -> Result<(), String> {
    let path = config_path(SEEN_FILENAME);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let data = serde_json::to_string_pretty(seen).map_err(|e| e.to_string())?;
    std::fs::write(&path, data).map_err(|e| e.to_string())
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Register a SLURM job: persists local metadata AND calls the existing
/// `register_watched_job` so the remote daemon starts polling it.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn register_slurm_job_metadata(
    state: tauri::State<'_, JobNotifyManager>,
    ssh_state: tauri::State<'_, SSHManager>,
    profile_id: String,
    job_id: String,
    session_id: String,
    session_name: String,
    job_name: Option<String>,
    expected_output: Option<String>,
    sbatch_path: Option<String>,
) -> Result<(), String> {
    // Local registry first (so even if the remote registration races, we
    // remember the metadata).
    {
        let mut reg = state.registry.lock().map_err(|e| e.to_string())?;
        // Replace any prior entry for the same (profile_id, job_id).
        reg.entries
            .retain(|e| !(e.profile_id == profile_id && e.job_id == job_id));
        reg.entries.push(JobRegistryEntry {
            profile_id: profile_id.clone(),
            job_id: job_id.clone(),
            session_id,
            session_name,
            job_name,
            expected_output,
            sbatch_path: sbatch_path.clone(),
            registered_at_ms: now_ms(),
        });
        save_registry(&reg)?;
    }

    // Then delegate to the existing watchdog-side registration.
    super::watchdog::register_watched_job(
        ssh_state,
        profile_id,
        job_id,
        Some("slurm".to_string()),
        sbatch_path,
    )
    .await
}

/// Pull JSONL events for every locally-registered job on the given profile
/// in one SSH round-trip. Returns completions whose terminal-event ts is
/// newer than the last time we surfaced it (or that we've never seen).
async fn scan_profile(
    ssh_state: &SSHManager,
    profile_id: &str,
    entries: &[JobRegistryEntry],
    seen: &SeenState,
) -> Result<Vec<PendingCompletion>, String> {
    let profile = ssh_state
        .profiles
        .lock()
        .map_err(|e| e.to_string())?
        .iter()
        .find(|p| p.id == profile_id)
        .cloned()
        .ok_or_else(|| format!("SSH profile {} not found", profile_id))?;

    if entries.is_empty() {
        return Ok(Vec::new());
    }

    // Build a remote script that prints, per registered job, the latest
    // terminal event (if any). Format: `JOBID\tJSON\n`. Missing → skipped.
    let mut script = String::from("set -u\n");
    for e in entries {
        // Single-quote escape the job id (digits in practice, but defensive).
        let jid = e.job_id.replace('\'', "'\\''");
        script.push_str(&format!(
            "if [ -f $HOME/.operon/jobs/'{jid}'.jsonl ]; then \
               line=$(grep '\"type\":\"terminal\"' $HOME/.operon/jobs/'{jid}'.jsonl | tail -n1); \
               if [ -n \"$line\" ]; then printf '%s\\t%s\\n' '{jid}' \"$line\"; fi; \
             fi\n",
            jid = jid
        ));
    }

    let raw = ssh_exec(&profile, &script).unwrap_or_default();
    let mut out = Vec::new();
    for line in raw.lines() {
        let mut split = line.splitn(2, '\t');
        let Some(jid) = split.next() else { continue };
        let Some(json) = split.next() else { continue };
        let entry = entries.iter().find(|e| e.job_id == jid);
        let Some(entry) = entry else { continue };

        // Parse the terminal event
        let parsed: serde_json::Value = match serde_json::from_str(json) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let ts = parsed.get("ts").and_then(|v| v.as_u64()).unwrap_or(0);
        let key = format!("{}::{}", profile_id, jid);
        let already = seen.seen.get(&key).copied().unwrap_or(0);
        if ts != 0 && ts <= already {
            continue;
        }

        let state = parsed
            .get("state")
            .and_then(|v| v.as_str())
            .unwrap_or("UNKNOWN")
            .to_string();
        let exit_code = parsed
            .get("exit_code")
            .and_then(|v| v.as_str())
            .unwrap_or("0:0")
            .to_string();
        let elapsed = parsed
            .get("elapsed_seconds")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let log_path = parsed
            .get("log_path")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        let log_tail = parsed
            .get("log_tail")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let last_error_line = if state.eq_ignore_ascii_case("FAILED") {
            extract_error_line(&log_tail)
        } else {
            String::new()
        };

        out.push(PendingCompletion {
            profile_id: profile_id.to_string(),
            job_id: jid.to_string(),
            session_id: entry.session_id.clone(),
            session_name: entry.session_name.clone(),
            job_name: entry.job_name.clone(),
            expected_output: entry.expected_output.clone(),
            state,
            exit_code,
            elapsed_seconds: elapsed,
            log_path,
            log_tail,
            terminal_at_ms: ts,
            last_error_line,
        });
    }
    Ok(out)
}

/// Pick the most likely "error" line from a slurm log tail. Confidence-
/// preserving — empty string if nothing looks clearly like an error, so the
/// UI can render "(failed — see log)" instead of a guess.
fn extract_error_line(tail: &str) -> String {
    let candidates: Vec<&str> = tail
        .lines()
        .filter(|l| {
            let lower = l.to_ascii_lowercase();
            lower.contains("error")
                || lower.contains("traceback")
                || lower.starts_with("killed")
                || lower.contains("segmentation fault")
                || lower.contains("aborted")
                || lower.contains("oom")
        })
        .collect();
    if candidates.is_empty() {
        return String::new();
    }
    // Prefer the LAST error-looking line (most recent = root cause for many
    // runtimes). Trim to a sane length.
    let pick = candidates.last().unwrap().trim();
    if pick.len() > 240 {
        format!("{}…", &pick[..237])
    } else {
        pick.to_string()
    }
}

/// Scan every SSH profile that owns registered jobs for new terminal events.
/// Returns the full list (caller decides which to render / dock-bounce on).
#[tauri::command]
pub async fn list_pending_completions(
    state: tauri::State<'_, JobNotifyManager>,
    ssh_state: tauri::State<'_, SSHManager>,
) -> Result<Vec<PendingCompletion>, String> {
    // Group registry entries by profile so we batch the SSH round-trips.
    let by_profile: HashMap<String, Vec<JobRegistryEntry>> = {
        let reg = state.registry.lock().map_err(|e| e.to_string())?;
        let mut m: HashMap<String, Vec<JobRegistryEntry>> = HashMap::new();
        for e in &reg.entries {
            m.entry(e.profile_id.clone()).or_default().push(e.clone());
        }
        m
    };
    let seen_snapshot = state.seen.lock().map_err(|e| e.to_string())?.clone();

    let mut out: Vec<PendingCompletion> = Vec::new();
    for (profile_id, entries) in &by_profile {
        match scan_profile(&ssh_state, profile_id, entries, &seen_snapshot).await {
            Ok(mut completions) => out.append(&mut completions),
            Err(_) => continue, // unreachable profile — skip silently
        }
    }
    Ok(out)
}

/// Acknowledge a completion so it doesn't surface again. Also removes the
/// job from the local registry since we no longer need to track it.
#[tauri::command]
pub async fn mark_completion_seen(
    state: tauri::State<'_, JobNotifyManager>,
    profile_id: String,
    job_id: String,
    terminal_at_ms: u64,
) -> Result<(), String> {
    let key = format!("{}::{}", profile_id, job_id);
    {
        let mut seen = state.seen.lock().map_err(|e| e.to_string())?;
        seen.seen.insert(key, terminal_at_ms);
        save_seen(&seen)?;
    }
    {
        let mut reg = state.registry.lock().map_err(|e| e.to_string())?;
        reg.entries
            .retain(|e| !(e.profile_id == profile_id && e.job_id == job_id));
        save_registry(&reg)?;
    }
    Ok(())
}

/// Bounce the dock icon to grab the user's attention. On macOS this uses
/// `requestUserAttention(.criticalRequest)`. On Windows it flashes the
/// taskbar entry. Linux WMs vary — Tauri exposes the same API and degrades
/// gracefully where unsupported.
#[tauri::command]
pub async fn request_user_attention(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.request_user_attention(Some(tauri::UserAttentionType::Critical));
    }
    Ok(())
}
