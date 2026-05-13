import { invoke } from '@tauri-apps/api/core';

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
  /** true if an `srun`/`salloc`/`sbatch` is running inside an `operon-*` tmux pane —
   *  i.e. killing that session would release a SLURM allocation. */
  slurm_in_pane: boolean;
  /** `squeue` lines for the user's currently-running jobs (context for the warning). */
  running_jobs: string[];
}

export async function scanRemoteFootprint(profileId: string): Promise<RemoteFootprint> {
  return invoke<RemoteFootprint>('scan_remote_footprint', { profileId });
}

/**
 * Kill everything Operon spawned for this profile: local SSH children, remote
 * log-streamers, leftover scratch files, and (if `killTmux`) the `operon-*`
 * tmux session(s). Never runs `scancel`. Returns a human-readable summary.
 */
export async function teardownRemoteFootprint(
  profileId: string,
  killTmux: boolean,
): Promise<string> {
  return invoke<string>('teardown_remote_footprint', { profileId, killTmux });
}
