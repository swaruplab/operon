import { invoke } from '@tauri-apps/api/core';

export interface SlurmJobSpec {
  profile_id: string;
  partition?: string;
  account?: string;
  nodes?: number;
  cores?: number;
  memory_gb?: number;
  /** "HH:MM:SS" */
  time_hms?: string;
  /** e.g. "a100" — translates to --gres=gpu:a100:N */
  gpu_type?: string;
  gpu_count?: number;
  job_name?: string;
  output_dir?: string;
  command: string;
}

export interface SlurmJob {
  job_id: string;
  state: string;
  partition: string;
  name: string;
  user: string;
  time: string;
  nodes: string;
  reason: string;
}

/** Submit an sbatch job. Returns the parsed job id (e.g. "12345"). */
export async function slurmSubmitJob(spec: SlurmJobSpec): Promise<string> {
  return invoke('slurm_submit_job', { spec });
}

/** Query `squeue -u $USER` on the remote. */
export async function slurmQueryJobs(profileId: string): Promise<SlurmJob[]> {
  return invoke('slurm_query_jobs', { profileId });
}

/** Cancel a queued or running job (`scancel` or `qdel`). */
export async function slurmCancelJob(profileId: string, jobId: string): Promise<void> {
  return invoke('slurm_cancel_job', { profileId, jobId });
}

/**
 * Local sbatch-script preview generator. Mirrors `build_sbatch_script` in
 * `src-tauri/src/commands/slurm.rs` so the UI shows exactly what will be
 * submitted.
 */
export function buildSbatchPreview(spec: SlurmJobSpec): string {
  const lines: string[] = ['#!/bin/bash'];
  const push = (key: string, val: string | undefined | null) => {
    if (val !== undefined && val !== null && String(val).trim() !== '') {
      lines.push(`#SBATCH --${key}=${String(val).trim()}`);
    }
  };
  push('job-name', spec.job_name);
  push('partition', spec.partition);
  push('account', spec.account);
  if (spec.nodes) push('nodes', String(spec.nodes));
  if (spec.cores) push('cpus-per-task', String(spec.cores));
  if (spec.memory_gb) push('mem', `${spec.memory_gb}G`);
  push('time', spec.time_hms);
  if (spec.gpu_count && spec.gpu_count > 0) {
    const gtype = spec.gpu_type?.trim();
    push('gres', gtype ? `gpu:${gtype}:${spec.gpu_count}` : `gpu:${spec.gpu_count}`);
  }
  const dir = spec.output_dir?.trim().replace(/\/$/, '');
  if (dir) {
    push('output', `${dir}/slurm-%j.out`);
    push('error', `${dir}/slurm-%j.err`);
  }
  lines.push('');
  lines.push((spec.command || '').replace(/\s+$/, ''));
  return lines.join('\n') + '\n';
}
