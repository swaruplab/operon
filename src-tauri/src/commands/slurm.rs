//! SLURM (and basic PBS) job submission commands.
//!
//! `slurm_submit_job` builds an sbatch script from a [`SlurmJobSpec`], writes
//! it to the user's chosen output directory on the remote host, then runs
//! `sbatch <path>` (or `qsub` if the profile is PBS-flavoured) over the
//! persistent SSH channel. Returns the parsed scheduler job id.
//!
//! `slurm_query_jobs` lists the current user's queue (`squeue -u $USER` or
//! `qstat -u $USER`).
//!
//! `slurm_cancel_job` runs `scancel` / `qdel`.

use serde::{Deserialize, Serialize};

use super::ssh::{ssh_exec_async, SSHManager, SSHProfile};

#[derive(Debug, Deserialize)]
pub struct SlurmJobSpec {
    pub profile_id: String,
    pub partition: Option<String>,
    pub account: Option<String>,
    pub nodes: Option<u32>,
    pub cores: Option<u32>,
    pub memory_gb: Option<u32>,
    /// "HH:MM:SS"
    pub time_hms: Option<String>,
    /// e.g. "a100" — translates to `--gres=gpu:a100:N`
    pub gpu_type: Option<String>,
    pub gpu_count: Option<u32>,
    pub job_name: Option<String>,
    /// Where slurm-NNN.out / the submit script land.
    pub output_dir: Option<String>,
    /// SLURM `--mail-user`. Blank means NO mail header at all — the caller owns
    /// this decision so the rendered preview matches the submitted bytes.
    #[serde(default)]
    pub mail_user: Option<String>,
    /// SLURM `--mail-type`. Defaults to `END,FAIL` when a mail_user is present.
    #[serde(default)]
    pub mail_type: Option<String>,
    /// Body of the script (after the #SBATCH header).
    pub command: String,
}

#[derive(Debug, Serialize)]
pub struct SlurmJob {
    pub job_id: String,
    pub state: String,
    pub partition: String,
    pub name: String,
    pub user: String,
    pub time: String,
    pub nodes: String,
    pub reason: String,
}

fn lookup_profile(state: &SSHManager, profile_id: &str) -> Result<SSHProfile, String> {
    let profiles = state.profiles.lock().map_err(|e| e.to_string())?;
    profiles
        .iter()
        .find(|p| p.id == profile_id)
        .cloned()
        .ok_or_else(|| format!("SSH profile {} not found", profile_id))
}

fn scheduler_for(profile: &SSHProfile) -> &str {
    profile
        .server_config
        .get("scheduler")
        .map(|s| s.as_str())
        .unwrap_or("slurm")
}

fn shq(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Build the sbatch script body from a spec. Public for the preview pane —
/// the frontend uses an identical generator, but we keep this here as the
/// single source of truth that runs on the wire.
pub fn build_sbatch_script(spec: &SlurmJobSpec) -> String {
    let mut s = String::from("#!/bin/bash\n");
    if let Some(name) = spec.job_name.as_ref().filter(|v| !v.trim().is_empty()) {
        s.push_str(&format!("#SBATCH --job-name={}\n", name.trim()));
    }
    if let Some(p) = spec.partition.as_ref().filter(|v| !v.trim().is_empty()) {
        s.push_str(&format!("#SBATCH --partition={}\n", p.trim()));
    }
    if let Some(a) = spec.account.as_ref().filter(|v| !v.trim().is_empty()) {
        s.push_str(&format!("#SBATCH --account={}\n", a.trim()));
    }
    if let Some(n) = spec.nodes {
        s.push_str(&format!("#SBATCH --nodes={}\n", n));
    }
    if let Some(c) = spec.cores {
        s.push_str(&format!("#SBATCH --cpus-per-task={}\n", c));
    }
    if let Some(m) = spec.memory_gb {
        s.push_str(&format!("#SBATCH --mem={}G\n", m));
    }
    if let Some(t) = spec.time_hms.as_ref().filter(|v| !v.trim().is_empty()) {
        s.push_str(&format!("#SBATCH --time={}\n", t.trim()));
    }
    if let Some(count) = spec.gpu_count.filter(|c| *c > 0) {
        let gres = match spec.gpu_type.as_ref().filter(|v| !v.trim().is_empty()) {
            Some(gtype) => format!("gpu:{}:{}", gtype.trim(), count),
            None => format!("gpu:{}", count),
        };
        s.push_str(&format!("#SBATCH --gres={}\n", gres));
    }
    if let Some(dir) = spec.output_dir.as_ref().filter(|v| !v.trim().is_empty()) {
        let dir = dir.trim().trim_end_matches('/');
        s.push_str(&format!("#SBATCH --output={}/slurm-%j.out\n", dir));
        s.push_str(&format!("#SBATCH --error={}/slurm-%j.err\n", dir));
    }
    // Mail. Any whitespace in the address would let the value inject further
    // header lines (or script body) into the generated file, so a malformed
    // address is dropped rather than emitted.
    if let Some(mail) = spec
        .mail_user
        .as_ref()
        .map(|v| v.trim())
        .filter(|v| !v.is_empty() && !v.contains(char::is_whitespace))
    {
        s.push_str(&format!("#SBATCH --mail-user={}\n", mail));
        let mt = spec
            .mail_type
            .as_ref()
            .map(|v| v.split_whitespace().collect::<String>())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "END,FAIL".to_string());
        s.push_str(&format!("#SBATCH --mail-type={}\n", mt));
    }
    s.push('\n');
    s.push_str(spec.command.trim_end());
    s.push('\n');
    s
}

#[tauri::command]
pub async fn slurm_submit_job(
    state: tauri::State<'_, SSHManager>,
    spec: SlurmJobSpec,
) -> Result<String, String> {
    if spec.command.trim().is_empty() {
        return Err("Command is empty".to_string());
    }

    let profile = lookup_profile(&state, &spec.profile_id)?;
    let scheduler = scheduler_for(&profile).to_string();

    // NOTE: no server_config fallback here. The caller decides the mail
    // fields, so `buildSbatchPreview` in the UI renders the exact bytes that
    // get submitted — and clearing the address really does mean no email.
    let script = build_sbatch_script(&spec);

    // Pick a target directory. Fall back to $HOME if the user left it blank
    // so we never silently drop the script into /.
    let dir = spec
        .output_dir
        .clone()
        .map(|d| d.trim().trim_end_matches('/').to_string())
        .filter(|d| !d.is_empty())
        .unwrap_or_else(|| "$HOME".to_string());

    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let script_path = format!("{}/operon_submit_{}.sh", dir, ts);

    // Write the script via base64 to avoid quoting headaches across the SSH
    // chain (local shell → ssh → remote shell → bash -c).
    let b64 = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        script.as_bytes(),
    );
    let mkdir_cmd = format!("mkdir -p {}", shq(&dir));
    let _ = ssh_exec_async(profile.clone(), mkdir_cmd).await;

    let write_cmd = format!(
        "printf %s {} | base64 -d > {} && chmod +x {}",
        b64,
        shq(&script_path),
        shq(&script_path)
    );
    ssh_exec_async(profile.clone(), write_cmd)
        .await
        .map_err(|e| format!("Failed to upload sbatch script: {}", e))?;

    let submit_cmd = if scheduler == "pbs" {
        format!("qsub {}", shq(&script_path))
    } else {
        format!("sbatch {}", shq(&script_path))
    };
    let output = ssh_exec_async(profile, submit_cmd)
        .await
        .map_err(|e| format!("Submission failed: {}", e))?;

    // SLURM: "Submitted batch job 12345"
    // PBS:   "12345.host"
    let job_id = if scheduler == "pbs" {
        output
            .lines()
            .map(|l| l.trim())
            .find(|l| !l.is_empty())
            .map(|l| l.split('.').next().unwrap_or(l).to_string())
    } else {
        output.lines().find_map(|l| {
            l.trim()
                .strip_prefix("Submitted batch job ")
                .map(|s| s.trim().to_string())
        })
    };

    job_id.ok_or_else(|| format!("Could not parse job id from output: {}", output.trim()))
}

#[tauri::command]
pub async fn slurm_query_jobs(
    state: tauri::State<'_, SSHManager>,
    profile_id: String,
) -> Result<Vec<SlurmJob>, String> {
    let profile = lookup_profile(&state, &profile_id)?;
    let scheduler = scheduler_for(&profile).to_string();

    // `$USER` is not guaranteed in a `bash --noprofile --norc` exec channel, and
    // an empty expansion drops the user filter entirely — `squeue -u ""` lists
    // the whole cluster. Resolve it defensively and bail rather than over-report.
    let cmd = if scheduler == "pbs" {
        format!(
            "{}; [ -n \"$_u\" ] || exit 0; qstat -u \"$_u\" 2>/dev/null",
            RESOLVE_REMOTE_USER
        )
    } else {
        format!(
            "{}; [ -n \"$_u\" ] || exit 0; \
             squeue -u \"$_u\" --format='%i|%T|%P|%j|%u|%M|%D|%R' --noheader 2>/dev/null",
            RESOLVE_REMOTE_USER
        )
    };

    let out = ssh_exec_async(profile, cmd).await?;
    let mut jobs = Vec::new();

    if scheduler == "pbs" {
        for line in out.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with("Job ") || line.starts_with("---") {
                continue;
            }
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() < 6 {
                continue;
            }
            jobs.push(SlurmJob {
                job_id: fields[0].split('.').next().unwrap_or(fields[0]).to_string(),
                state: fields[4].to_string(),
                partition: fields[5].to_string(),
                name: fields[1].to_string(),
                user: fields[2].to_string(),
                time: fields[3].to_string(),
                nodes: "-".to_string(),
                reason: "-".to_string(),
            });
        }
    } else {
        for line in out.lines() {
            let parts: Vec<&str> = line.split('|').collect();
            if parts.len() < 8 {
                continue;
            }
            jobs.push(SlurmJob {
                job_id: parts[0].trim().to_string(),
                state: parts[1].trim().to_string(),
                partition: parts[2].trim().to_string(),
                name: parts[3].trim().to_string(),
                user: parts[4].trim().to_string(),
                time: parts[5].trim().to_string(),
                nodes: parts[6].trim().to_string(),
                reason: parts[7].trim().to_string(),
            });
        }
    }

    Ok(jobs)
}

#[tauri::command]
pub async fn slurm_cancel_job(
    state: tauri::State<'_, SSHManager>,
    profile_id: String,
    job_id: String,
) -> Result<(), String> {
    let profile = lookup_profile(&state, &profile_id)?;
    let scheduler = scheduler_for(&profile).to_string();
    let cmd = if scheduler == "pbs" {
        format!("qdel {}", shq(&job_id))
    } else {
        format!("scancel {}", shq(&job_id))
    };
    ssh_exec_async(profile, cmd).await?;
    Ok(())
}

// ─── On-demand job status (replaces the login-node watchdog daemon) ──────
//
// Job tracking used to depend on `operon-watchdog.sh`, a detached bash daemon
// polling the scheduler on the LOGIN node forever. HPC sites reap exactly that
// (UCI RCIC terminated ours by name and emailed the account owner), and it
// bought nothing that an on-demand query doesn't: the Jobs panel needs *data*,
// not a resident process. `squeue` answers for live jobs and `sacct` for
// finished ones, both in a single round-trip issued only while the panel is
// open. Notification while Operon is closed is SLURM's own `--mail-user`.

/// One job as the Jobs panel sees it. Fields are best-effort: `squeue` supplies
/// live rows, `sacct` supplies terminal ones, and a job can appear in both
/// (squeue wins, since it is authoritative while the job is alive).
#[derive(Debug, Clone, Serialize)]
pub struct ClusterJob {
    pub job_id: String,
    pub name: String,
    pub state: String,
    pub partition: String,
    /// Human elapsed from squeue ("1:23:45"), empty for sacct-only rows.
    pub elapsed: String,
    /// Seconds from sacct; 0 when unknown.
    pub elapsed_seconds: u64,
    /// squeue's NODELIST(REASON) — why a job is pending, or where it runs.
    pub reason: String,
    /// sacct ExitCode ("0:0"); empty while running.
    pub exit_code: String,
    /// End time exactly as sacct reported it in the cluster's local time
    /// ("2026-08-13T07:20:03"); empty while running or unknown. Deliberately not
    /// converted to epoch: that cost a `date` fork per row on the login node,
    /// and ISO 8601 already sorts correctly as a string.
    pub ended_at: String,
    /// "squeue" (live) or "sacct" (historical).
    pub source: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClusterJobsResult {
    pub jobs: Vec<ClusterJob>,
    /// False when the cluster has no usable SLURM accounting. Without it a job
    /// vanishes from `squeue` on completion and there is no record to read, so
    /// the panel must say so rather than silently showing nothing.
    pub accounting: bool,
    /// False when the remote shell could resolve neither `$USER` nor `id -un`.
    /// The query is skipped entirely rather than run unfiltered — `squeue -u ""`
    /// lists the WHOLE CLUSTER, and every row would carry a working Cancel.
    pub user_resolved: bool,
}

/// Every SLURM state that means the job is over.
///
/// Single source of truth: `is_terminal_state` matches against it, and
/// `job_notify`'s remote `case` pattern is GENERATED from it. Keeping the remote
/// filter as a hand-written copy meant it could be narrower than the Rust one —
/// it was, silently dropping SPECIAL_EXIT — and the "defence in depth" re-check
/// in Rust only catches the remote list being too WIDE, never too narrow.
pub const TERMINAL_STATES: &[&str] = &[
    "COMPLETED",
    "FAILED",
    "TIMEOUT",
    "OUT_OF_MEMORY",
    "NODE_FAIL",
    "BOOT_FAIL",
    "DEADLINE",
    "PREEMPTED",
    "REVOKED",
    "SPECIAL_EXIT",
];

/// A `case` pattern matching every terminal state, for embedding in remote sh.
/// `CANCELLED*` because sacct reports "CANCELLED by 12345".
pub fn terminal_state_case_pattern() -> String {
    let mut p = TERMINAL_STATES.join("|");
    p.push_str("|CANCELLED*");
    p
}

/// True for SLURM states that mean the job is over.
pub fn is_terminal_state(state: &str) -> bool {
    let s = state.trim().to_ascii_uppercase();
    let s = s.split_whitespace().next().unwrap_or("");
    TERMINAL_STATES.contains(&s) || s.starts_with("CANCELLED")
}

/// Resolve the remote username defensively.
///
/// `$USER` is normally set by sshd, but not under every ForceCommand /
/// restricted-shell / channel configuration, and our exec shell is
/// `bash --noprofile --norc` with no `set -u`. An empty expansion turns
/// `squeue -u "$USER"` into `squeue -u ""`, which applies NO user filter — the
/// panel would fill with the whole cluster's jobs, each with a working Cancel
/// button. The daemon this replaced guarded exactly this way.
const RESOLVE_REMOTE_USER: &str = r#"_u="${USER:-$(id -un 2>/dev/null)}""#;

/// Build the one-shot remote script: live queue, then accounting history for
/// the window plus any explicitly-requested ids.
fn cluster_jobs_script(since: &str) -> String {
    // `-P` (pipe-delimited, no alignment) + `-n` (no header) + `-X` (allocation
    // rows only, not .batch/.extern steps) keeps parsing trivial.
    // JobName goes LAST in both formats. It is free text and sbatch accepts `|`
    // in --job-name; with the name in the middle, one pipe shifted every
    // subsequent field, so state showed a name fragment and the row came back
    // wrong rather than dropped. Parsing takes the tail as the name.
    //
    // No `date` here. This used to fork `date` once per row to convert `End`
    // into epoch — thousands of spawns per poll on the shared login node, which
    // is the load that got the daemon killed in the first place. The raw sacct
    // stamp is passed through and sorts correctly as a string (ISO 8601).
    //
    // The accounting probe is the real query's exit status, not a second
    // throwaway `sacct` call.
    format!(
        r#"
{resolve_user}
if [ -z "$_u" ]; then echo '__OPERON_NOUSER__'; echo '__OPERON_END__'; exit 0; fi
echo '__OPERON_SQUEUE__'
squeue -h -r -u "$_u" -o '%i|%T|%P|%M|%R|%j' 2>/dev/null
echo '__OPERON_ACCT__'
if command -v sacct >/dev/null 2>&1; then
  _acct_out=$(sacct -n -X -P -u "$_u" -S {since} -o JobID,State,Partition,ExitCode,ElapsedRaw,End,JobName 2>/dev/null)
  if [ $? -eq 0 ]; then
    echo 'acct=1'
    [ -n "$_acct_out" ] && printf '%s\n' "$_acct_out"
  else
    echo 'acct=0'
  fi
else
  echo 'acct=0'
fi
echo '__OPERON_END__'
"#,
        resolve_user = RESOLVE_REMOTE_USER,
        since = since,
    )
}

/// Query the cluster for the user's jobs — live (`squeue`) merged with recent
/// history (`sacct`) — in one SSH round-trip.
///
/// There is deliberately no per-id parameter. An earlier version took
/// `extra_job_ids` to "rescue" jobs older than the window, but the script always
/// emitted `-S {since}` alongside `-j`, and sacct only defaults to the epoch when
/// `-S` is absent — so passing ids returned nothing extra while narrowing the
/// window it was supposed to widen. Nothing called it. Widen `since` instead.
#[tauri::command]
pub async fn list_cluster_jobs(
    state: tauri::State<'_, SSHManager>,
    profile_id: String,
    since: Option<String>,
) -> Result<ClusterJobsResult, String> {
    let profile = lookup_profile(&state, &profile_id)?;

    // SLURM understands `now-7days` natively, so no local/remote date math and
    // no GNU-vs-BSD `date` portability problem.
    let since = since
        .filter(|s| {
            !s.is_empty()
                && s.len() <= 32
                && s.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == ':' || c == '+')
        })
        .unwrap_or_else(|| "now-7days".to_string());

    let out = ssh_exec_async(profile, cluster_jobs_script(&since)).await?;
    Ok(parse_cluster_jobs(&out))
}

/// Split the remote output into live + historical rows. Pure so it can be
/// tested without a cluster.
pub fn parse_cluster_jobs(out: &str) -> ClusterJobsResult {
    #[derive(PartialEq)]
    enum Section {
        None,
        Squeue,
        Acct,
    }
    let mut section = Section::None;
    let mut accounting = false;
    let mut user_resolved = true;
    let mut jobs: Vec<ClusterJob> = Vec::new();
    // Job ids already taken from squeue — a live row always beats an sacct row.
    let mut live: std::collections::HashSet<String> = std::collections::HashSet::new();

    for line in out.lines() {
        let line = line.trim_end();
        match line.trim() {
            "__OPERON_SQUEUE__" => {
                section = Section::Squeue;
                continue;
            }
            "__OPERON_ACCT__" => {
                section = Section::Acct;
                continue;
            }
            "__OPERON_END__" => {
                section = Section::None;
                continue;
            }
            "__OPERON_NOUSER__" => {
                user_resolved = false;
                continue;
            }
            "acct=1" => {
                accounting = true;
                continue;
            }
            "acct=0" => {
                accounting = false;
                continue;
            }
            _ => {}
        }
        if line.trim().is_empty() {
            continue;
        }
        match section {
            Section::Squeue => {
                // %i|%T|%P|%M|%R|%j — JobName LAST because it is free text and
                // may itself contain the delimiter. splitn keeps the remainder.
                let f: Vec<&str> = line.splitn(6, '|').collect();
                if f.len() < 6 {
                    continue;
                }
                let id = f[0].trim().to_string();
                if id.is_empty() {
                    continue;
                }
                live.insert(id.clone());
                jobs.push(ClusterJob {
                    job_id: id,
                    name: f[5].trim().to_string(),
                    state: f[1].trim().to_string(),
                    partition: f[2].trim().to_string(),
                    elapsed: f[3].trim().to_string(),
                    elapsed_seconds: 0,
                    reason: f[4].trim().to_string(),
                    exit_code: String::new(),
                    ended_at: String::new(),
                    source: "squeue".to_string(),
                });
            }
            Section::Acct => {
                // JobID|State|Partition|ExitCode|ElapsedRaw|End|JobName
                let f: Vec<&str> = line.splitn(7, '|').collect();
                if f.len() < 7 {
                    continue;
                }
                let id = f[0].trim().to_string();
                if id.is_empty() || live.contains(&id) {
                    continue;
                }
                // `End` is passed through verbatim ("2026-08-13T07:20:03",
                // "Unknown", or empty). ISO 8601 sorts correctly as a string, so
                // no conversion — and therefore no per-row `date` fork on the
                // login node — is needed.
                let ended = match f[5].trim() {
                    "" | "Unknown" | "None" => String::new(),
                    v => v.to_string(),
                };
                jobs.push(ClusterJob {
                    job_id: id,
                    name: f[6].trim().to_string(),
                    state: f[1].trim().to_string(),
                    partition: f[2].trim().to_string(),
                    elapsed: String::new(),
                    elapsed_seconds: f[4].trim().parse().unwrap_or(0),
                    reason: String::new(),
                    exit_code: f[3].trim().to_string(),
                    ended_at: ended,
                    source: "sacct".to_string(),
                });
            }
            Section::None => {}
        }
    }

    // Newest first: running jobs at the top, then most recently ended.
    jobs.sort_by(|a, b| {
        let rank = |j: &ClusterJob| if j.source == "squeue" { 0 } else { 1 };
        rank(a)
            .cmp(&rank(b))
            .then(b.ended_at.cmp(&a.ended_at))
            .then(b.job_id.cmp(&a.job_id))
    });

    ClusterJobsResult {
        jobs,
        accounting,
        user_resolved,
    }
}

/// Read the tail of a job's stdout log. On-demand replacement for the daemon's
/// `tail -f` helper, which held a process open on the login node for hours.
#[tauri::command]
pub async fn read_job_log_tail(
    state: tauri::State<'_, SSHManager>,
    profile_id: String,
    job_id: String,
    log_path: Option<String>,
    lines: Option<u32>,
) -> Result<String, String> {
    if !super::claude::is_valid_job_id(&job_id) {
        return Err(format!("Invalid job id: {}", job_id));
    }
    let profile = lookup_profile(&state, &profile_id)?;
    let n = lines.unwrap_or(200).clamp(1, 5000);

    // Byte cap as well as a line cap. A slurm .out written by an R or Python
    // progress bar is one enormous `\r`-delimited line, so `tail -n 30` alone can
    // be megabytes pulled over SSH into an IPC string.
    const MAX_BYTES: u32 = 64_000;

    // The profile's working directory is where `build_sbatch_script` points
    // `--output`, so it is the most likely home for slurm-<id>.out when sacct
    // can't tell us (no accounting, or too old for the StdOut field).
    let work_dir = profile
        .server_config
        .get("work_dir")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    // Prefer an explicit path; otherwise ask sacct where StdOut went, then fall
    // back to the conventional slurm-<id>.out in the work dir and $HOME.
    let explicit = log_path
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty());
    let cmd = match explicit {
        Some(p) => format!(
            "tail -c {b} {f} 2>/dev/null | tail -n {n}",
            b = MAX_BYTES,
            n = n,
            f = shq(&p)
        ),
        None => {
            // `./slurm-<id>.out` was a duplicate: the exec shell's cwd is $HOME.
            let mut candidates = vec![format!("\"$HOME/slurm-{}.out\"", job_id)];
            if let Some(d) = &work_dir {
                candidates.insert(
                    0,
                    shq(&format!("{}/slurm-{}.out", d.trim_end_matches('/'), job_id)),
                );
            }
            format!(
                r#"_p=$(sacct -j {jid}.batch -n -X -P -o StdOut 2>/dev/null | head -n1)
if [ -n "$_p" ] && [ -f "$_p" ]; then exec sh -c 'tail -c {b} "$1" | tail -n {n}' _ "$_p"; fi
for _c in {cands}; do
  [ -f "$_c" ] && exec sh -c 'tail -c {b} "$1" | tail -n {n}' _ "$_c"
done
echo '(no log file found — it may not have been created yet, or it lives somewhere Operon cannot guess. Set the Working Directory on this server profile to help.)'"#,
                jid = job_id,
                n = n,
                b = MAX_BYTES,
                cands = candidates.join(" "),
            )
        }
    };
    ssh_exec_async(profile, cmd).await
}
