import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

export interface WatchedJob {
  profile_id: string;
  job_id: string;
  scheduler: string;
  submit_ts: number;
  sbatch_path: string | null;
  retries_left: number;
}

export interface JobPolicy {
  max_retries: number;
  on_timeout_walltime_mult: number;
  on_oom_mem_mult: number;
}

export interface WatchdogStatus {
  installed: boolean;
  running: boolean;
  tmux_session: string | null;
  scheduler: string | null;
  watchlist_len: number;
}

/** One NDJSON event as written by scripts/operon-watchdog.sh. */
export interface JobEvent {
  ts: number;
  type: string; // "poll" | "terminal" | "registered" | "watchdog_start" | "watchdog_stop"
  state?: string;
  action?: string;
  resubmitted_as?: string;
  raw?: string;
  from_job?: string;
  retries_left?: number;
  [k: string]: unknown;
}

// ── install / lifecycle ──────────────────────────────────────────────────

export async function detectScheduler(profileId: string): Promise<string> {
  return invoke('detect_scheduler', { profileId });
}

export async function installWatchdog(profileId: string): Promise<void> {
  return invoke('install_watchdog', { profileId });
}

export async function startWatchdog(profileId: string): Promise<void> {
  return invoke('start_watchdog', { profileId });
}

export async function stopWatchdog(profileId: string): Promise<void> {
  return invoke('stop_watchdog', { profileId });
}

export async function watchdogStatus(profileId: string): Promise<WatchdogStatus> {
  return invoke('watchdog_status', { profileId });
}

// ── watchlist + policy ───────────────────────────────────────────────────

export async function registerWatchedJob(
  profileId: string,
  jobId: string,
  scheduler: string | null = 'slurm',
  sbatchPath: string | null = null,
): Promise<void> {
  return invoke('register_watched_job', {
    profileId,
    jobId,
    scheduler,
    sbatchPath,
  });
}

export async function unregisterWatchedJob(profileId: string, jobId: string): Promise<void> {
  return invoke('unregister_watched_job', { profileId, jobId });
}

export async function listWatchedJobs(profileId: string): Promise<WatchedJob[]> {
  return invoke('list_watched_jobs', { profileId });
}

export async function getJobPolicy(profileId: string): Promise<JobPolicy> {
  return invoke('get_job_policy', { profileId });
}

export async function setJobPolicy(profileId: string, policy: JobPolicy): Promise<void> {
  return invoke('set_job_policy', { profileId, policy });
}

// ── event tail ───────────────────────────────────────────────────────────

export async function readJobEvents(profileId: string, jobId: string): Promise<JobEvent[]> {
  const raw: string = await invoke('read_job_events', { profileId, jobId });
  return raw
    .split('\n')
    .map((l) => l.trim())
    .filter((l) => l.length > 0)
    .map((l) => {
      try {
        return JSON.parse(l) as JobEvent;
      } catch {
        return { ts: 0, type: 'parse-error', raw: l } as JobEvent;
      }
    });
}

export async function startJobTail(profileId: string, jobId: string): Promise<void> {
  return invoke('start_job_tail', { profileId, jobId });
}

export async function stopJobTail(profileId: string, jobId: string): Promise<void> {
  return invoke('stop_job_tail', { profileId, jobId });
}

export async function onJobEvent(
  jobId: string,
  handler: (ev: JobEvent) => void,
): Promise<UnlistenFn> {
  return listen<string>(`job-event-${jobId}`, (e) => {
    try {
      handler(JSON.parse(e.payload) as JobEvent);
    } catch {
      /* tolerate malformed lines */
    }
  });
}

// ── auto-register helper ─────────────────────────────────────────────────

const SBATCH_RE = /Submitted batch job\s+(\d+)/;

/**
 * Scan a chunk of terminal output for `Submitted batch job NNNN` and return
 * any fresh job ids. Caller is responsible for dedupe + calling
 * `registerWatchedJob` for each hit.
 */
export function parseSbatchIds(text: string): string[] {
  const ids: string[] = [];
  for (const line of text.split(/\r?\n/)) {
    const m = line.match(SBATCH_RE);
    if (m) ids.push(m[1]);
  }
  return ids;
}
