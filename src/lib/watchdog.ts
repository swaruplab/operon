import { invoke } from '@tauri-apps/api/core';

/**
 * HPC job tracking.
 *
 * Operon used to run a bash daemon (`operon-watchdog.sh`) on the cluster's LOGIN
 * node that polled SLURM forever so job state stayed fresh while the app was
 * closed. HPC sites reap exactly that kind of process — UCI RCIC terminated ours
 * and emailed the account owner — and it was re-bootstrapped on every SSH
 * connect, so the kill/restart cycle never ended.
 *
 * There is no daemon now. The panel asks the scheduler directly, only while it
 * is open: {@link listClusterJobs} for state, {@link readJobLogTail} for logs.
 * For "tell me when it finishes even though Operon is closed", set the optional
 * notification email in the SSH profile's server settings — SLURM mails you
 * itself, which needs nothing of ours to be running.
 */

/** A job as the Jobs panel sees it — a live `squeue` row or an `sacct` record. */
export interface ClusterJob {
  job_id: string;
  name: string;
  state: string;
  partition: string;
  /** Human elapsed from squeue ("1:23:45"); empty for historical rows. */
  elapsed: string;
  /** Seconds from sacct; 0 when unknown. */
  elapsed_seconds: number;
  /** squeue's NODELIST(REASON) — why it's pending, or where it runs. */
  reason: string;
  /** sacct ExitCode ("0:0"); empty while running. */
  exit_code: string;
  /** End time exactly as sacct reported it, in the cluster's local time
   *  ("2026-08-13T07:20:03"); empty while running or unknown. Deliberately not
   *  an epoch — converting cost a `date` fork per row on the login node. */
  ended_at: string;
  source: 'squeue' | 'sacct';
}

export interface ClusterJobsResult {
  jobs: ClusterJob[];
  /** False when the remote shell could resolve neither $USER nor `id -un`, so
   *  the query was skipped rather than run unfiltered (which would list the
   *  whole cluster, each row with a working Cancel button). */
  user_resolved: boolean;
  /**
   * False when the cluster has no usable SLURM accounting. Without `sacct` a job
   * simply vanishes from `squeue` when it ends and there is no record left to
   * read, so the panel must say so rather than imply the job never existed.
   */
  accounting: boolean;
}

/** SLURM states that mean the job is over. Mirrors `is_terminal_state` in slurm.rs. */
const TERMINAL = new Set([
  'COMPLETED',
  'FAILED',
  'TIMEOUT',
  'OUT_OF_MEMORY',
  'NODE_FAIL',
  'BOOT_FAIL',
  'DEADLINE',
  'PREEMPTED',
  'REVOKED',
  'SPECIAL_EXIT',
]);

export function isTerminalState(state: string): boolean {
  const s = (state || '').trim().toUpperCase().split(/\s+/)[0] || '';
  return TERMINAL.has(s) || s.startsWith('CANCELLED');
}

/**
 * Query the cluster for this user's jobs — live plus recent history — in one
 * round-trip.
 *
 * @param since       SLURM time expression; defaults to `now-7days`.
 */
export async function listClusterJobs(
  profileId: string,
  since?: string,
): Promise<ClusterJobsResult> {
  return invoke('list_cluster_jobs', { profileId, since: since || null });
}

/** Read the tail of a job's stdout log on demand (no `tail -f` left running). */
export async function readJobLogTail(
  profileId: string,
  jobId: string,
  logPath?: string | null,
  lines?: number,
): Promise<string> {
  return invoke('read_job_log_tail', {
    profileId,
    jobId,
    logPath: logPath || null,
    lines: lines ?? null,
  });
}


export interface LegacyCleanupResult {
  /** True only when a leftover daemon or its files were actually found. */
  removed: boolean;
  details: string;
}

/**
 * Kill and delete any `operon-watchdog.sh` daemon left over from an older
 * Operon. Idempotent and safe on hosts that never had one.
 *
 * Needed because upgrading does not stop a daemon that is already running on the
 * user's login node — without this sweep they would keep receiving kill notices
 * from their site long after the feature was removed.
 */
export async function cleanupLegacyWatchdog(profileId: string): Promise<LegacyCleanupResult> {
  return invoke('cleanup_legacy_watchdog', { profileId });
}

// ── auto-register helper ─────────────────────────────────────────────────

const SBATCH_RE = /Submitted batch job\s+(\d+)/g;

/**
 * Scan a chunk of terminal output for `Submitted batch job NNNN` and return
 * any job ids found. Caller dedupes and calls `registerSlurmJob` for each hit.
 */
export function parseSbatchIds(text: string): string[] {
  // Global match, and over the whole text rather than line-by-line: agent output
  // arrives as NDJSON, so several `Submitted batch job` strings can share one
  // physical line (their newlines are escaped inside the JSON). The old
  // first-match-per-line scan silently dropped all but one.
  const ids: string[] = [];
  for (const m of text.matchAll(SBATCH_RE)) ids.push(m[1]);
  return ids;
}
