// Job completion notifications.
//
// Operon keeps a LOCAL registry (<config_dir>/job_registry.json) mapping a
// scheduler job id to the chat session that submitted it, plus optional context
// (job name, expected output). Nothing is written on the cluster.
//
// Reconciliation is one SSH round-trip per profile: `sacct` is asked for the
// state of every registered job that has not already been acknowledged, and any
// that reached a terminal state becomes a completion card. A marker file at
// <config_dir>/job_notifications.json records what has been surfaced so a card
// appears exactly once.
//
// This previously read `~/.operon/jobs/<jobid>.jsonl`, written by a bash daemon
// that polled SLURM on the login node forever. That daemon was removed (HPC
// sites reap it — see watchdog.rs); `sacct` is the authoritative record and
// costs one command, issued only while Operon is running.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::Manager;

use super::ssh::SSHManager;

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

/// The allocation id a scheduler row belongs to: `12345_7` → `12345`,
/// `12345+0` → `12345`, `12345.batch` → `12345`, `12345` → `12345`.
///
/// Registration only ever sees the bare id printed by sbatch, while sacct
/// reports per-task and per-component rows, so every lookup that joins the two
/// has to normalise.
pub(crate) fn base_job_id(id: &str) -> &str {
    id.split(['_', '+', '.']).next().unwrap_or(id)
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Register a SLURM job so Operon can attribute it to a chat session and
/// surface its completion.
///
/// Local-only by design. This used to also write the job id into a remote
/// `~/.operon/watchlist` for a login-node daemon to poll; that daemon is gone
/// and job state now comes from `sacct` on demand, so registration leaves no
/// footprint on the cluster at all.
/// `sbatch_path` was removed rather than repurposed: it was documented as "used
/// for re-locating the log", written on every call, persisted, and read by
/// nothing — its only consumer was the deleted remote-watchlist delegation. All
/// three callers passed `null`. `#[serde(default)]` on the struct means existing
/// `job_registry.json` files still deserialize.
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
) -> Result<(), String> {
    // Completion tracking is `sacct`-based, so a PBS/LSF profile can never
    // produce one. Registering there only grows a registry that is re-queried
    // every 30s forever.
    {
        let profiles = ssh_state.profiles.lock().map_err(|e| e.to_string())?;
        if let Some(p) = profiles.iter().find(|p| p.id == profile_id) {
            let sched = p
                .server_config
                .get("scheduler")
                .map(|s| s.as_str())
                .unwrap_or("slurm");
            if sched != "slurm" {
                return Ok(());
            }
        }
    }
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
            registered_at_ms: now_ms(),
        });
        sweep_registry(&mut reg);
        save_registry(&reg)?;
    }
    Ok(())
}

/// Ask `sacct` for the state of every not-yet-acknowledged registered job on
/// this profile, in one SSH round-trip. Returns one completion per submitted
/// job that has reached a terminal state.
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

    // Only query ids we haven't already surfaced. The dedup used to happen in
    // Rust AFTER the remote work, so every acknowledged-but-unpruned job still
    // paid for a `sacct` row and a base64'd log tail on every 30s poll.
    let ids: Vec<String> = entries
        .iter()
        .filter(|e| {
            let key = format!("{}::{}", profile_id, e.job_id);
            seen.seen.get(&key).copied().unwrap_or(0) == 0
        })
        .map(|e| e.job_id.clone())
        .filter(|id| super::claude::is_valid_job_id(id))
        .collect();
    if ids.is_empty() {
        return Ok(Vec::new());
    }

    // Ask the scheduler directly. This used to grep `~/.operon/jobs/*.jsonl`,
    // written by a bash daemon that polled SLURM on the login node forever —
    // the process HPC sites reap. `sacct` is the authoritative record and costs
    // one command, issued only while Operon is open.
    //
    // Path and log tail are base64'd so neither can collide with the field
    // delimiter or smuggle a newline into the record format. The tail is capped
    // in BYTES as well as lines: a progress-bar log is one huge `\r`-delimited
    // line, so `tail -n 30` alone can be megabytes, +33% for base64, per poll.
    let script = format!(
        r#"set -u
command -v sacct >/dev/null 2>&1 || {{ echo '__OPERON_NOACCT__'; exit 0; }}
sacct -j '{ids}' -n -X -P -o JobID,State,ExitCode,ElapsedRaw,End 2>/dev/null \
| while IFS='|' read -r _id _st _ec _el _en; do
    case "$_st" in
      {terminal_pattern}) ;;
      *) continue ;;
    esac
    _ts=0
    case "$_en" in
      ''|Unknown|None) ;;
      # GNU date first (every Linux cluster), then the BSD/macOS form. A 0 here
      # is handled by the caller, which substitutes the local registration time.
      *) _ts=$(date -d "$_en" +%s 2>/dev/null \
               || date -j -f '%Y-%m-%dT%H:%M:%S' "$_en" +%s 2>/dev/null \
               || echo 0) ;;
    esac
    _lp=$(sacct -j "${{_id}}.batch" -n -X -P -o StdOut 2>/dev/null | head -n1)
    if [ -z "$_lp" ] || [ ! -f "$_lp" ]; then
      _lp=''
      for _c in "$HOME/slurm-${{_id}}.out"; do
        [ -f "$_c" ] && {{ _lp="$_c"; break; }}
      done
    fi
    _tb=''
    _pb=''
    if [ -n "$_lp" ] && [ -f "$_lp" ]; then
      _pb=$(printf '%s' "$_lp" | base64 | tr -d '\n')
      _tb=$(tail -c 8000 "$_lp" 2>/dev/null | tail -n 30 | base64 | tr -d '\n')
    fi
    printf '%s|%s|%s|%s|%s|%s|%s\n' "$_id" "$_st" "$_ec" "$_el" "$_ts" "$_pb" "$_tb"
  done
"#,
        ids = ids.join(","),
        terminal_pattern = super::slurm::terminal_state_case_pattern(),
    );

    // Async: this runs inside an async command, sequentially over every profile.
    // The blocking `ssh_exec` parked a tokio worker for up to the channel idle
    // timeout per unreachable profile, so a couple of stale profiles made the
    // poll outlast its own 30s interval. Errors propagate now instead of being
    // swallowed into an empty string, which made a broken feature look identical
    // to "nothing finished".
    let raw = super::ssh::ssh_exec_async(profile, script).await?;
    if raw.lines().any(|l| l.trim() == "__OPERON_NOACCT__") {
        return Err("no-accounting".to_string());
    }
    let b64 = base64::engine::general_purpose::STANDARD;
    let decode = |s: &str| -> String {
        if s.is_empty() {
            return String::new();
        }
        base64::Engine::decode(&b64, s)
            .ok()
            .and_then(|v| String::from_utf8(v).ok())
            .unwrap_or_default()
    };

    // Keyed by the REGISTERED job id so an array's task rows collapse to one.
    let mut out: HashMap<String, PendingCompletion> = HashMap::new();
    for line in raw.lines() {
        let f: Vec<&str> = line.trim().split('|').collect();
        if f.len() < 7 {
            continue;
        }
        let jid = f[0].trim();
        // Match the BASE id too. `sbatch --array=1-10` prints "Submitted batch
        // job 12345", so we register `12345` — but sacct reports one row per
        // task (`12345_1`, `12345_2`, …) and one per het component (`12345+0`).
        // Exact matching meant array jobs never notified at all, and because
        // `mark_completion_seen` is the only prune path, their registry entries
        // then lived forever. The prune below must split the same way.
        let base = base_job_id(jid);
        let Some(entry) = entries.iter().find(|e| e.job_id == jid || e.job_id == base) else {
            continue;
        };

        let state = f[1].trim().to_string();
        // Defence in depth: the remote `case` above already filters to terminal
        // states, but that list and the Rust one are separate copies. Re-check
        // here so a drift between them can never surface a "your job finished"
        // banner for a job that is still running.
        if !super::slurm::is_terminal_state(&state) {
            continue;
        }
        let exit_code = {
            let v = f[2].trim();
            if v.is_empty() {
                "0:0".to_string()
            } else {
                v.to_string()
            }
        };
        let elapsed: u64 = f[3].trim().parse().unwrap_or(0);
        // sacct's End in epoch ms. When the cluster's `date` can't parse it we
        // fall back to the local registration time — any STABLE non-zero value
        // works, because the dedup below compares against what was last
        // acknowledged. A zero here would re-notify on every poll forever.
        let ts = match f[4].trim().parse::<u64>().unwrap_or(0) {
            0 => entry.registered_at_ms.max(1),
            secs => secs * 1000,
        };

        // Key on the REGISTERED id, not the sacct row id — otherwise an array's
        // ten task rows would each need acknowledging separately, and none of
        // those keys would ever match what `mark_completion_seen` writes.
        let key = format!("{}::{}", profile_id, entry.job_id);
        let already = seen.seen.get(&key).copied().unwrap_or(0);
        if ts <= already {
            continue;
        }

        let log_path = {
            let p = decode(f[5].trim());
            if p.is_empty() {
                None
            } else {
                Some(p)
            }
        };
        let log_tail = decode(f[6].trim());
        let last_error_line = if state.eq_ignore_ascii_case("FAILED") {
            extract_error_line(&log_tail)
        } else {
            String::new()
        };

        let completion = PendingCompletion {
            profile_id: profile_id.to_string(),
            // Report the id the user actually submitted, so acknowledging it
            // matches the registry entry and the banner reads sensibly.
            job_id: entry.job_id.clone(),
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
        };

        // Aggregate policy for arrays: one card per submitted job, and a failing
        // task wins over a completed one. Ten green tasks and one OOM is an OOM
        // the user needs to see, not ten notifications to dismiss.
        match out.entry(entry.job_id.clone()) {
            std::collections::hash_map::Entry::Vacant(v) => {
                v.insert(completion);
            }
            std::collections::hash_map::Entry::Occupied(mut o) => {
                let keep = {
                    let prev = o.get();
                    let prev_ok = prev.state.eq_ignore_ascii_case("COMPLETED");
                    let new_bad = !completion.state.eq_ignore_ascii_case("COMPLETED");
                    (prev_ok && new_bad) || completion.terminal_at_ms > prev.terminal_at_ms
                };
                if keep {
                    o.insert(completion);
                }
            }
        }
    }
    Ok(out.into_values().collect())
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

/// How long an unacknowledged job stays in the registry.
///
/// `mark_completion_seen` is the only other removal path and it requires the
/// user to dismiss a banner, so without this every job that can never produce a
/// completion — a cluster with no accounting, a job aged out of `MinJobAge`, a
/// profile since deleted — accumulates forever and is re-queried every 30s.
const REGISTRY_MAX_AGE_MS: u64 = 14 * 24 * 60 * 60 * 1000;

/// Hard ceiling regardless of age. Bounds the `sacct -j a,b,c…` command line
/// (Windows' one-shot fallback hits a 32KB CreateProcess cap) and the size of
/// any single poll. Oldest entries go first.
const REGISTRY_MAX_ENTRIES: usize = 500;

/// Drop entries that can no longer produce a useful notification. Returns true
/// when anything was removed, so the caller knows to persist.
fn sweep_registry(reg: &mut JobRegistry) -> bool {
    let before = reg.entries.len();
    let now = now_ms();
    reg.entries
        .retain(|e| now.saturating_sub(e.registered_at_ms) < REGISTRY_MAX_AGE_MS);
    if reg.entries.len() > REGISTRY_MAX_ENTRIES {
        reg.entries
            .sort_by_key(|e| std::cmp::Reverse(e.registered_at_ms));
        reg.entries.truncate(REGISTRY_MAX_ENTRIES);
    }
    reg.entries.len() != before
}

/// Scan every SSH profile that owns registered jobs for new terminal events.
#[derive(Debug, Clone, Serialize)]
pub struct PendingCompletionsResult {
    pub completions: Vec<PendingCompletion>,
    /// Profiles whose cluster has no usable `sacct`. Completion tracking cannot
    /// work there at all, so the UI can say so once instead of looking like
    /// nothing has finished — the failure used to be indistinguishable from
    /// success because the error was swallowed.
    pub profiles_without_accounting: Vec<String>,
}

#[tauri::command]
pub async fn list_pending_completions(
    state: tauri::State<'_, JobNotifyManager>,
    ssh_state: tauri::State<'_, SSHManager>,
) -> Result<PendingCompletionsResult, String> {
    // Age out anything unactionable before doing any remote work.
    let by_profile: HashMap<String, Vec<JobRegistryEntry>> = {
        let mut reg = state.registry.lock().map_err(|e| e.to_string())?;
        if sweep_registry(&mut reg) {
            let _ = save_registry(&reg);
        }
        let mut m: HashMap<String, Vec<JobRegistryEntry>> = HashMap::new();
        for e in &reg.entries {
            m.entry(e.profile_id.clone()).or_default().push(e.clone());
        }
        m
    };
    let seen_snapshot = state.seen.lock().map_err(|e| e.to_string())?.clone();

    let mut out: Vec<PendingCompletion> = Vec::new();
    let mut no_accounting: Vec<String> = Vec::new();
    for (profile_id, entries) in &by_profile {
        match scan_profile(&ssh_state, profile_id, entries, &seen_snapshot).await {
            Ok(mut completions) => out.append(&mut completions),
            Err(e) if e == "no-accounting" => no_accounting.push(profile_id.clone()),
            // Unreachable profile (asleep laptop, expired MFA). Not an error
            // worth surfacing — the next poll retries.
            Err(_) => continue,
        }
    }
    Ok(PendingCompletionsResult {
        completions: out,
        profiles_without_accounting: no_accounting,
    })
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
        // Normalise both sides. `scan_profile` reports the registered id, but a
        // caller replaying an older completion may still hand us a task id like
        // `12345_3`; comparing raw would leave the entry behind forever.
        let base = base_job_id(&job_id);
        reg.entries
            .retain(|e| !(e.profile_id == profile_id && (e.job_id == job_id || e.job_id == base)));
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
