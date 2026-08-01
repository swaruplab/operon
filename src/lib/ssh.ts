import { invoke } from '@tauri-apps/api/core';

export type AuthType = 'password' | 'key' | 'duo_mfa';

export interface SSHProfile {
  id: string;
  name: string;
  host: string;
  user: string;
  port: number;
  key_file: string | null;
  use_agent: boolean;
  /** What kind of auth this server uses */
  auth_type: AuthType;
  /** For Duo MFA: preferred method ("push", "phone", "passcode") */
  mfa_method: string | null;
  /** Whether to use ControlMaster multiplexing */
  use_control_master: boolean;
  /** Whether key_file is passphrase-protected (passphrase lives in the OS keychain) */
  key_has_passphrase?: boolean;
  /** Server-level config: SLURM accounts, partitions, conda envs, etc. */
  server_config: Record<string, string>;
}

/** Well-known server config keys with labels and placeholders for the UI */
export const SERVER_CONFIG_FIELDS: Array<{
  key: string;
  label: string;
  placeholder: string;
  group: 'slurm' | 'interactive' | 'environment' | 'paths';
}> = [
  { key: 'slurm_account', label: 'SLURM Account', placeholder: 'e.g. swarup_lab', group: 'slurm' },
  { key: 'slurm_partition', label: 'CPU Partition', placeholder: 'e.g. standard, free', group: 'slurm' },
  { key: 'slurm_gpu_partition', label: 'GPU Partition', placeholder: 'e.g. gpu, free-gpu', group: 'slurm' },
  { key: 'slurm_gpu_type', label: 'GPU Type', placeholder: 'e.g. A100, V100, H100', group: 'slurm' },
  // How to grab an interactive/compute node. Free-form so any scheduler works:
  // srun --pty bash, salloc, sinteractive, qsub -I, or a site wrapper. Operon
  // injects the account below if the command doesn't already name one.
  {
    key: 'interactive_cmd',
    label: 'Interactive Node Command',
    placeholder: 'e.g. srun --pty -t 4:00:00 --mem=16G bash -l   (must land you ON a compute node)',
    group: 'interactive',
  },
  {
    key: 'interactive_account',
    label: 'Interactive Account',
    placeholder: 'allocation charged for the interactive node (defaults to SLURM Account)',
    group: 'interactive',
  },
  { key: 'conda_env', label: 'Default Conda Env', placeholder: 'e.g. scanpy_env', group: 'environment' },
  { key: 'modules', label: 'Default Modules', placeholder: 'e.g. python/3.10, cuda/12.0', group: 'environment' },
  { key: 'scratch_dir', label: 'Scratch Directory', placeholder: 'e.g. /dfs3b/swarup_lab/vivek', group: 'paths' },
  { key: 'work_dir', label: 'Working Directory', placeholder: 'e.g. /dfs3b/operonws/$USER (sessions open here; $USER expands)', group: 'paths' },
];

export interface KeySetupProgress {
  stage: string;   // "connecting" | "password" | "mfa_waiting" | "installing" | "verifying" | "done" | "error"
  message: string;
}

export async function saveSSHProfile(profile: SSHProfile): Promise<void> {
  return invoke('save_ssh_profile', { profile });
}

export async function listSSHProfiles(): Promise<SSHProfile[]> {
  return invoke('list_ssh_profiles');
}

export async function deleteSSHProfile(profileId: string): Promise<void> {
  return invoke('delete_ssh_profile', { profileId });
}

/** Persist a new ordering for the SSH profile list (drag-to-reorder). */
export async function reorderSSHProfiles(orderedIds: string[]): Promise<void> {
  return invoke('reorder_ssh_profiles', { orderedIds });
}

export async function listRemoteDirectory(profileId: string, path: string): Promise<import('./files').FileEntry[]> {
  return invoke('list_remote_directory', { profileId, path });
}

export async function getRemoteHome(profileId: string): Promise<string> {
  return invoke('get_remote_home', { profileId });
}

export async function setupSSHKey(
  profileId: string,
  password: string,
  mfaMethod?: string,
  keyPassphrase?: string,
): Promise<string> {
  return invoke('setup_ssh_key', { profileId, password, mfaMethod, keyPassphrase });
}

export async function checkControlMaster(profileId: string): Promise<boolean> {
  return invoke('check_control_master', { profileId });
}

// ── SSH key passphrase (OS keychain + ssh-agent) ──

/** Canonical ControlMaster socket path — frontend uses this so the terminal's
 *  master lands exactly where the backend looks for it. */
export async function getSshSocketPath(host: string, port: number, user: string): Promise<string> {
  return invoke('get_ssh_socket_path', { host, port, user });
}

/** Load a profile's passphrase-protected key into the ssh-agent before connecting. */
export async function prepareSshAuth(profileId: string): Promise<void> {
  return invoke('prepare_ssh_auth', { profileId });
}

/** Store (and verify) a key passphrase in the OS keychain. */
export async function setKeyPassphrase(profileId: string, passphrase: string, keyFile: string | null): Promise<void> {
  return invoke('set_ssh_key_passphrase', { profileId, passphrase, keyFile });
}

/** Remove a stored key passphrase. */
export async function deleteKeyPassphrase(profileId: string): Promise<void> {
  return invoke('delete_ssh_key_passphrase', { profileId });
}

/** Whether a passphrase is already stored for this profile. */
export async function hasKeyPassphrase(profileId: string): Promise<boolean> {
  return invoke('has_ssh_key_passphrase', { profileId });
}

/** Whether the private key at keyFile is encrypted (needs a passphrase). */
export async function keyNeedsPassphrase(keyFile: string): Promise<boolean> {
  return invoke('key_needs_passphrase', { keyFile });
}

/** Encrypt an existing key with a passphrase (ssh-keygen -p) and save it to the keychain. */
export async function addKeyPassphrase(
  profileId: string,
  keyFile: string,
  newPassphrase: string,
  oldPassphrase?: string,
): Promise<void> {
  return invoke('add_ssh_key_passphrase', { profileId, keyFile, newPassphrase, oldPassphrase });
}

export async function stopControlMaster(profileId: string): Promise<void> {
  return invoke('stop_control_master', { profileId });
}

export interface SshDiagnostics {
  rtt_ms: number | null;
  channels_alive: number;
  channels_total: number;
  pool_max: number;
  total_calls: number;
  cache_hits: number;
  respawns: number;
  in_cooldown: boolean;
  last_error: string | null;
}

/** Snapshot the SSH channel pool for the given profile (RTT + slot stats). */
export async function getSshDiagnostics(profileId: string): Promise<SshDiagnostics> {
  return invoke('get_ssh_diagnostics', { profileId });
}

/** Reset diagnostic counters and tear down all persistent channels for the profile. */
export async function resetSshDiagnostics(profileId: string): Promise<void> {
  return invoke('reset_ssh_diagnostics', { profileId });
}

/** Auto-detect server environment (SLURM, conda, paths) via SSH */
export async function detectServerConfig(profileId: string): Promise<Record<string, string>> {
  return invoke('detect_server_config', { profileId });
}

/** Get saved server_config for a profile */
export async function getServerConfig(profileId: string): Promise<Record<string, string>> {
  return invoke('get_server_config', { profileId });
}

// ── File Transfer ──

/** Upload a local file to the remote server via SCP */
export async function scpToRemote(profileId: string, localPath: string, remotePath: string): Promise<void> {
  return invoke('scp_to_remote', { profileId, localPath, remotePath });
}

/** Download a remote file to the local machine via SCP */
export async function scpFromRemote(profileId: string, remotePath: string, localPath: string): Promise<void> {
  return invoke('scp_from_remote', { profileId, remotePath, localPath });
}

/** Download a remote directory to the local machine via SCP -r */
export async function scpDirFromRemote(profileId: string, remotePath: string, localPath: string): Promise<void> {
  return invoke('scp_dir_from_remote', { profileId, remotePath, localPath });
}

/** Upload multiple local files to a remote directory. Returns count of successful uploads. */
export async function scpBatchUpload(profileId: string, localPaths: string[], remoteDir: string): Promise<number> {
  return invoke('scp_batch_upload', { profileId, localPaths, remoteDir });
}

/** Clear the SSH remote file/directory cache. Forces fresh data on next load. */
export async function clearSshCache(): Promise<void> {
  return invoke('clear_ssh_cache');
}

export async function batchDeleteRemoteFiles(profileId: string, paths: string[]): Promise<number> {
  return invoke('batch_delete_remote_files', { profileId, paths });
}

import type { FileEntry } from './files';

interface CachedDir {
  entries: FileEntry[];
  ts: number;
}

const REMOTE_DIR_CACHE = new Map<string, CachedDir>();
const REMOTE_DIR_TTL_MS = 15_000;

function cacheKey(profileId: string, path: string, showHidden: boolean): string {
  return `${profileId}:${path}:${showHidden ? 1 : 0}`;
}

export async function listRemoteDirectoryCached(
  profileId: string,
  path: string,
  showHidden = false,
): Promise<FileEntry[]> {
  const key = cacheKey(profileId, path, showHidden);
  const hit = REMOTE_DIR_CACHE.get(key);
  if (hit && Date.now() - hit.ts < REMOTE_DIR_TTL_MS) {
    return hit.entries;
  }
  const entries = await invoke<FileEntry[]>('list_remote_directory', {
    profileId,
    path,
    showHidden,
  });
  REMOTE_DIR_CACHE.set(key, { entries, ts: Date.now() });
  return entries;
}

export function invalidateRemotePath(profileId: string, path: string): void {
  const prefix = `${profileId}:${path}:`;
  const parent = path.replace(/\/[^/]*\/?$/, '') || '/';
  const parentPrefix = `${profileId}:${parent}:`;
  for (const k of Array.from(REMOTE_DIR_CACHE.keys())) {
    if (k.startsWith(prefix) || k.startsWith(parentPrefix)) {
      REMOTE_DIR_CACHE.delete(k);
    }
  }
}

export async function clearRemoteCache(): Promise<void> {
  REMOTE_DIR_CACHE.clear();
  try {
    await invoke('clear_ssh_cache');
  } catch {
    /* backend may be unavailable; client cache already cleared */
  }
}

/**
 * Resolve a user-typed remote path (`~`, `~/x`, `$SCRATCH/run`) to a concrete one.
 *
 * Remote paths reach the shell inside single quotes, which is what makes remote
 * file operations injection-proof but also stops `$VAR` expanding. Resolution
 * therefore happens once, here, at the point the user's text enters the app — so
 * everything downstream (listing, mkdir, upload, cd-to-terminal, the chat
 * session's working directory) shares one concrete path. Returns the input
 * unchanged if it needs no expansion or the server can't be reached.
 */
export async function resolveRemotePath(profileId: string, path: string): Promise<string> {
  return invoke<string>('resolve_remote_path', { profileId, path });
}
