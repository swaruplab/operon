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
  /** Server-level config: SLURM accounts, partitions, conda envs, etc. */
  server_config: Record<string, string>;
}

/** Well-known server config keys with labels and placeholders for the UI */
export const SERVER_CONFIG_FIELDS: Array<{
  key: string;
  label: string;
  placeholder: string;
  group: 'slurm' | 'environment' | 'paths';
}> = [
  { key: 'slurm_account', label: 'SLURM Account', placeholder: 'e.g. swarup_lab', group: 'slurm' },
  { key: 'slurm_partition', label: 'CPU Partition', placeholder: 'e.g. standard, free', group: 'slurm' },
  { key: 'slurm_gpu_partition', label: 'GPU Partition', placeholder: 'e.g. gpu, free-gpu', group: 'slurm' },
  { key: 'slurm_gpu_type', label: 'GPU Type', placeholder: 'e.g. A100, V100, H100', group: 'slurm' },
  { key: 'conda_env', label: 'Default Conda Env', placeholder: 'e.g. scanpy_env', group: 'environment' },
  { key: 'modules', label: 'Default Modules', placeholder: 'e.g. python/3.10, cuda/12.0', group: 'environment' },
  { key: 'scratch_dir', label: 'Scratch Directory', placeholder: 'e.g. /dfs3b/swarup_lab/vivek', group: 'paths' },
  { key: 'work_dir', label: 'Working Directory', placeholder: 'e.g. /pub/vivek/projects', group: 'paths' },
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

export async function spawnSSHTerminal(terminalId: string, profileId: string): Promise<void> {
  return invoke('spawn_ssh_terminal', { terminalId, profileId });
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
): Promise<string> {
  return invoke('setup_ssh_key', { profileId, password, mfaMethod });
}

export async function checkControlMaster(profileId: string): Promise<boolean> {
  return invoke('check_control_master', { profileId });
}

export async function stopControlMaster(profileId: string): Promise<void> {
  return invoke('stop_control_master', { profileId });
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
