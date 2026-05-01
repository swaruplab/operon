import { invoke } from '@tauri-apps/api/core';
import { emit } from '@tauri-apps/api/event';

/**
 * Tear down everything tied to a remote profile so the user can switch servers.
 *
 * Steps:
 * 1. Close the OpenSSH ControlMaster socket (drops the multiplexed connection).
 * 2. Clear cached remote directory listings for this profile.
 * 3. Emit `disconnect-remote` so the chat panel, sidebar, terminal area, and
 *    any other listeners can reset their per-profile UI state.
 *
 * All backend calls are best-effort — if the ControlMaster socket is already
 * gone the SSH `-O exit` will fail silently and that's fine.
 */
export async function disconnectRemote(profileId: string): Promise<void> {
  try {
    await invoke('stop_control_master', { profileId });
  } catch {
    /* ControlMaster may already be closed */
  }
  try {
    await invoke('clear_ssh_cache');
  } catch {
    /* cache may not exist */
  }
  await emit('disconnect-remote', { profileId });
}
