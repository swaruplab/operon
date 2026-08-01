import { invoke } from '@tauri-apps/api/core';

/**
 * A running *interactive* SLURM allocation (`BatchFlag=0`) belonging to the user.
 * Batch (`sbatch`) jobs are never reported here and can never be cancelled
 * through this path.
 */
export interface InteractiveJob {
  id: string;
  name: string;
  nodes: string;
  /** Elapsed run time (`squeue %M`). */
  time: string;
  /** Node list (`squeue %R`). */
  nodelist: string;
  /**
   * How Operon tied this allocation to its own tmux pane:
   * - `pane` — a `--jobid=<id>` was found on an `operon-*` pane process. Certain.
   * - `sole` — the only running interactive allocation while an `srun`/`salloc`
   *   is live in an `operon-*` pane. Near-certain.
   * - `none` — unattributed. Shown to the user, never pre-selected.
   */
  attribution: 'pane' | 'sole' | 'none';
}

/**
 * What Operon has left running / lying around on a remote host — returned by
 * `scan_remote_footprint` and used to populate the "End session & clean up
 * everything" confirmation dialog.
 */
export interface RemoteFootprint {
  /** `ps`-style lines for leftover Operon helper processes (log-streamers, tails). */
  helpers: string[];
  /** tmux sessions whose name starts with `operon` (e.g. `operon-main`). */
  tmux_sessions: string[];
  /** true if an `srun`/`salloc`/`sbatch` is running inside an `operon-*` tmux pane. */
  slurm_in_pane: boolean;
  /** `squeue` lines for the user's currently-running jobs (context for the warning). */
  running_jobs: string[];
  /** Running interactive allocations the user can choose to release. */
  interactive_jobs: InteractiveJob[];
  /**
   * Set when the remote scan could not complete. An empty footprint with a null
   * `scan_error` means "nothing is running"; with an error it means "we don't
   * know" — never render the two the same way.
   */
  scan_error: string | null;
}

export async function scanRemoteFootprint(profileId: string): Promise<RemoteFootprint> {
  return invoke<RemoteFootprint>('scan_remote_footprint', { profileId });
}

/**
 * Kill everything Operon spawned for this profile: local SSH children, remote
 * log-streamers, leftover scratch files, and (if `killTmux`) the `operon-*`
 * tmux session(s).
 *
 * `cancelJobIds` releases interactive allocations with `scancel` — and ONLY
 * those the remote re-confirms are `BatchFlag=0` and owned by the connecting
 * user. Batch jobs are never cancellable here. Pass `[]` to touch no jobs.
 *
 * Returns a verified, human-readable summary: the backend re-checks tmux and
 * `squeue` afterwards and reports what is actually left, not what it asked for.
 */
export async function teardownRemoteFootprint(
  profileId: string,
  killTmux: boolean,
  cancelJobIds: string[] = [],
): Promise<string> {
  return invoke<string>('teardown_remote_footprint', { profileId, killTmux, cancelJobIds });
}
