import { invoke } from '@tauri-apps/api/core';

export interface SessionTab {
  file_path: string;
  is_remote: boolean;
  remote_profile_id?: string | null;
  cursor_line?: number | null;
  cursor_col?: number | null;
  scroll_top?: number | null;
}

export interface SessionTerminal {
  id: string;
  cwd: string;
  is_remote: boolean;
  remote_profile_id?: string | null;
}

export interface SessionState {
  project_path?: string | null;
  editor_tabs: SessionTab[];
  active_tab_id?: string | null;
  terminal_tabs: SessionTerminal[];
  active_chat_session_id?: string | null;
  active_sidebar_view?: string | null;
  saved_at: number;
}

/** Partial slice. Only the fields the caller owns should be provided —
 *  the backend merges them into the on-disk state. Use the explicit
 *  `null` for "clear this field"; omit a field to leave it untouched. */
export interface SessionStatePatch {
  project_path?: string | null;
  editor_tabs?: SessionTab[];
  active_tab_id?: string | null;
  terminal_tabs?: SessionTerminal[];
  active_chat_session_id?: string | null;
  active_sidebar_view?: string | null;
}

export async function saveSessionState(patch: SessionStatePatch): Promise<void> {
  return invoke('save_session_state', { patch });
}

export async function loadSessionState(): Promise<SessionState | null> {
  return invoke('load_session_state');
}

export async function clearSessionState(): Promise<void> {
  return invoke('clear_session_state');
}

/** Discard a candidate restore if older than 30 days. */
export const RESTORE_MAX_AGE_SECS = 30 * 24 * 60 * 60;

export function isRestoreCandidate(state: SessionState | null): boolean {
  if (!state) return false;
  if (!state.saved_at) return false;
  const ageSecs = Math.floor(Date.now() / 1000) - state.saved_at;
  if (ageSecs > RESTORE_MAX_AGE_SECS) return false;
  const hasContent =
    (state.editor_tabs && state.editor_tabs.length > 0) ||
    (state.terminal_tabs && state.terminal_tabs.length > 0) ||
    !!state.active_chat_session_id;
  return hasContent;
}
