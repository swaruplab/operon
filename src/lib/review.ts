import { invoke } from '@tauri-apps/api/core';
import { getSettings } from './settings';

/** Only these two severities ever reach the UI — the backend drops the rest. */
export type ReviewSeverity = 'blocking' | 'high';

export interface ReviewFinding {
  severity: ReviewSeverity;
  category: string;
  line: number | null;
  title: string;
  /** The concrete consequence: what will be wrong, and why. */
  why_wrong: string;
  fix: string;
}

export interface ReviewResult {
  findings: ReviewFinding[];
  model: string;
  /** No pitfalls on the checklist were hit. NOT a proof of correctness. */
  clean: boolean;
}

/** "sbatch" = HPC batch script; "script" = analysis code (Python/R). */
export type ReviewMode = 'sbatch' | 'script';

/**
 * Run the light reviewer over a snippet of code.
 *
 * Deliberately advisory: callers MUST treat a rejected promise as
 * "review unavailable" and still let the user proceed. The reviewer is a
 * second opinion, never a gate you can't get past.
 */
export async function reviewCode(
  code: string,
  mode: ReviewMode,
  filename?: string,
): Promise<ReviewResult> {
  const settings = await getSettings().catch(() => null);
  return invoke<ReviewResult>('review_code', {
    code,
    mode,
    filename: filename ?? null,
    model: settings?.reviewer_model || 'claude-sonnet-5',
    effort: settings?.reviewer_effort || 'low',
  });
}

/** Language hint from a filename — used only to label the review. */
export function isReviewableCode(path: string): boolean {
  return /\.(py|r|rmd|sh|bash|smk|nf|jl|ipynb|sbatch|slurm)$/i.test(path.trim());
}

// ── Review events (the visible-chip side channel) ──────────────────────────
// The agent's sbatch is reviewed by a PreToolUse hook, whose decisions never
// appear in Claude Code's stream. So the hook appends one record per review to
// a per-session file; Operon polls it to show a chip — a CLEAN review is as
// visible as a blocked one.

export type ReviewOutcome = 'clean' | 'blocked' | 'unavailable';

export interface ReviewEvent {
  ts: number;
  kind: string; // "sbatch"
  script: string;
  outcome: ReviewOutcome;
  n: number;
  findings: ReviewFinding[];
}

/** Read the review events the sbatch hook wrote for a session (clean / blocked /
 *  unavailable). Empty when none yet; never throws (missing file → []). */
export async function readReviewEvents(
  sessionId: string,
  profileId?: string,
): Promise<ReviewEvent[]> {
  if (!sessionId) return [];
  return invoke<ReviewEvent[]>('read_review_events', {
    sessionId,
    profileId: profileId ?? null,
  }).catch(() => []);
}

/**
 * Runtime on/off for the sbatch review (the chat-box checkbox). Drops/removes a
 * marker the guard hook honors mid-session, so unchecking takes effect on the
 * next sbatch without a session restart. Persist the default separately via
 * `reviewer_auto_sbatch` in settings. Fail-safe: swallows errors.
 */
export async function setReviewMarker(off: boolean, profileId?: string): Promise<void> {
  await invoke('set_review_marker', { off, profileId: profileId ?? null }).catch(() => {});
}
