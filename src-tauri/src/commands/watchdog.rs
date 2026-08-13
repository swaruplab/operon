//! HPC job tracking — scheduler detection and legacy-daemon cleanup.
//!
//! ── What used to be here ──
//! Operon shipped a job "watchdog": `scripts/operon-watchdog.sh`, uploaded to
//! the remote host and launched detached (tmux, or `nohup … & disown`) so it
//! outlived the app. It polled SLURM every 30s forever from the **login node**.
//!
//! That was the wrong design, and it had real consequences:
//!
//!   * HPC sites reap long-lived login-node processes. UCI RCIC terminated ours
//!     by name — its matcher hit the substring `watch` in `operon-watchdog.sh` —
//!     and emailed the account owner every time. Because the daemon was
//!     re-bootstrapped on every SSH connect, the kill/restart cycle repeated
//!     indefinitely.
//!   * The loop had no exit condition: no wall-clock cap, no idle timeout, no
//!     exit when the watchlist emptied. Its `squeue` self-discovery ran *before*
//!     the empty-watchlist guard, so it queried the scheduler every 30s even
//!     with zero jobs, and the watchlist only ever grew.
//!   * Its one genuinely stateful feature — auto-resubmit on TIMEOUT/OOM — was
//!     broken: `slurm_resubmit` re-ran the *identical* script. The
//!     `on_timeout_walltime_mult` / `on_oom_mem_mult` knobs in the UI were never
//!     applied by anything, so a TIMEOUT was resubmitted with the same walltime
//!     and timed out again, burning the allocation to reproduce the failure.
//!
//! ── What replaced it ──
//! Nothing resident. The Jobs panel needs *data*, not a process:
//!
//!   * live + historical job state → `slurm::list_cluster_jobs` (`squeue` +
//!     `sacct` in one round-trip, issued only while the panel is open)
//!   * job logs → `slurm::read_job_log_tail` (on demand, no `tail -f`)
//!   * completion while Operon is closed → SLURM's own `--mail-user`, from the
//!     profile's optional `notify_email` server config
//!   * session↔job attribution → `job_notify`'s LOCAL registry
//!
//! Job tracking now leaves nothing running on the cluster. Note this is not the
//! same as "Operon never has a login-node process": an active HPC *chat* session
//! still opens a `tail -f` there to stream the agent's output (`claude.rs`).
//! That one is bounded (12h cap + PPID watcher), dies with the session, and is
//! reapable — its children reset the HUP/PIPE traps the parent ignores, so it
//! can't survive as an orphan the way the daemon did.

use serde::Serialize;

use super::ssh::{ssh_exec, SSHManager};

// ─── Legacy daemon cleanup ───────────────────────────────────────────────

/// Result of sweeping a host for the retired watchdog daemon.
#[derive(Debug, Clone, Serialize)]
pub struct LegacyCleanupResult {
    /// True when something was actually found and removed — lets the UI stay
    /// silent on the overwhelmingly common "nothing there" case.
    pub removed: bool,
    pub details: String,
}

/// Kill and delete any leftover `operon-watchdog.sh` daemon and its remote
/// state. Idempotent, one-shot, and safe to run on a host that never had one.
///
/// This exists for users upgrading from a build that installed the daemon: it
/// is still running on their login node right now, and nothing in the new code
/// path would ever stop it. Without this they would keep getting kill notices
/// from their site long after the feature was removed.
#[tauri::command]
pub async fn cleanup_legacy_watchdog(
    ssh_state: tauri::State<'_, SSHManager>,
    profile_id: String,
) -> Result<LegacyCleanupResult, String> {
    let profile = ssh_state
        .profiles
        .lock()
        .map_err(|e| e.to_string())?
        .iter()
        .find(|p| p.id == profile_id)
        .cloned()
        .ok_or_else(|| format!("SSH profile {} not found", profile_id))?;

    // Preflight so an MFA cluster returns "open an SSH terminal and complete
    // Duo" rather than an opaque BatchMode auth failure — every path here is
    // non-interactive and can only succeed by riding a live ControlMaster.
    super::ssh::ensure_live_connection(&profile)?;

    // `operon-[w]atchdog` throughout: the bracket makes the pattern unable to
    // match the shell running this very script, whose command line contains the
    // literal "[w]" that the character class does not accept. Without it,
    // pkill would kill the cleanup itself before it finished.
    let script = r#"set -u
_found=0
if tmux has-session -t operon-watchdog 2>/dev/null; then
  _found=1
  tmux kill-session -t operon-watchdog 2>/dev/null
fi
# The pid file is NOT trusted on its own. The population this sweep targets is
# exactly the one holding a STALE pidfile — the site reaper killed the daemon and
# left the file behind — and login nodes have long uptimes, so that pid has very
# likely been recycled onto something else of the user's: a tmux server, an
# editor, an interactive srun shell. `kill -0` only proves "exists and is mine",
# which is not the same question. Verify the process is actually the watchdog
# before signalling it, and ignore a non-numeric file.
if [ -f "$HOME/.operon/watchdog.pid" ]; then
  _p=$(cat "$HOME/.operon/watchdog.pid" 2>/dev/null)
  case "$_p" in
    ''|*[!0-9]*) ;;
    *)
      if ps -p "$_p" -o args= 2>/dev/null | grep -q 'operon-[w]atchdog'; then
        _found=1
        kill "$_p" 2>/dev/null
      fi
      ;;
  esac
fi
if pgrep -f 'operon-[w]atchdog\.sh' >/dev/null 2>&1; then
  _found=1
  pkill -f 'operon-[w]atchdog\.sh' 2>/dev/null
fi
[ -f "$HOME/.operon/operon-watchdog.sh" ] && _found=1
rm -f "$HOME/.operon/operon-watchdog.sh" \
      "$HOME/.operon/watchdog.pid" \
      "$HOME/.operon/watchdog.log" \
      "$HOME/.operon/watchlist" \
      "$HOME/.operon/watchlist.tmp."* \
      "$HOME/.operon/policy.json" 2>/dev/null
rm -rf "$HOME/.operon/jobs" 2>/dev/null
# Leave ~/.operon itself — other Operon state may live there. Remove it only if
# our removals emptied it.
rmdir "$HOME/.operon" 2>/dev/null
echo "removed=$_found"
"#;

    let out = ssh_exec(&profile, script).map_err(|e| format!("cleanup failed: {}", e))?;
    let removed = out.lines().any(|l| l.trim() == "removed=1");
    Ok(LegacyCleanupResult {
        removed,
        details: out.trim().to_string(),
    })
}
