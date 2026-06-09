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
        output
            .lines()
            .find_map(|l| l.trim().strip_prefix("Submitted batch job ").map(|s| s.trim().to_string()))
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

    let cmd = if scheduler == "pbs" {
        // qstat: "JobID Name User Time S Queue"
        "qstat -u $USER 2>/dev/null".to_string()
    } else {
        "squeue -u \"$USER\" --format='%i|%T|%P|%j|%u|%M|%D|%R' --noheader 2>/dev/null".to_string()
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
