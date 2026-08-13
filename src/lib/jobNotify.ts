import { invoke } from '@tauri-apps/api/core';

export interface PendingCompletion {
  profile_id: string;
  job_id: string;
  session_id: string;
  session_name: string;
  job_name: string | null;
  expected_output: string | null;
  state: string;
  exit_code: string;
  elapsed_seconds: number;
  log_path: string | null;
  log_tail: string;
  terminal_at_ms: number;
  last_error_line: string;
}

export type CompletionVariant = 'success' | 'failure' | 'timeout' | 'cancelled';

export function variantForState(state: string): CompletionVariant {
  const s = state.toUpperCase();
  if (s === 'COMPLETED') return 'success';
  if (s === 'TIMEOUT' || s === 'DEADLINE') return 'timeout';
  if (s.startsWith('CANCELLED')) return 'cancelled';
  return 'failure';
}

export function formatElapsed(seconds: number): string {
  if (seconds < 60) return `${seconds}s`;
  if (seconds < 3600) {
    const m = Math.floor(seconds / 60);
    const s = seconds % 60;
    return s === 0 ? `${m}m` : `${m}m ${s}s`;
  }
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  return m === 0 ? `${h}h` : `${h}h ${m}m`;
}

export interface PendingCompletionsResult {
  completions: PendingCompletion[];
  /** Profiles whose cluster has no usable `sacct`. Completion tracking cannot
   *  work there at all, so the UI can say so once rather than looking like
   *  nothing has finished. */
  profiles_without_accounting: string[];
}

export async function listPendingCompletions(): Promise<PendingCompletionsResult> {
  return invoke<PendingCompletionsResult>('list_pending_completions');
}

export async function markCompletionSeen(
  profileId: string,
  jobId: string,
  terminalAtMs: number,
): Promise<void> {
  return invoke('mark_completion_seen', {
    profileId,
    jobId,
    terminalAtMs,
  });
}

export async function requestUserAttention(): Promise<void> {
  return invoke('request_user_attention');
}

export async function registerSlurmJob(params: {
  profileId: string;
  jobId: string;
  sessionId: string;
  sessionName: string;
  jobName?: string | null;
  expectedOutput?: string | null;
}): Promise<void> {
  return invoke('register_slurm_job_metadata', {
    profileId: params.profileId,
    jobId: params.jobId,
    sessionId: params.sessionId,
    sessionName: params.sessionName,
    jobName: params.jobName ?? null,
    expectedOutput: params.expectedOutput ?? null,
  });
}
