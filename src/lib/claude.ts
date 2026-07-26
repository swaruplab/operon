import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { ClaudeStatus, AuthStatus } from '../types/chat';

/** Credential/routing vars Operon takes ownership of, mirroring
 *  `MANAGED_AUTH_VARS` in src-tauri/src/commands/claude.rs. Claude Code picks
 *  its auth source from the environment, and any of these set by the user's
 *  shell profile outranks their claude.ai login — so an interactive shell we
 *  drive (`claude login` in a terminal tab, local or over SSH) has to clear
 *  them, or the CLI reports "connectors are disabled because ANTHROPIC_API_KEY
 *  ... takes precedence over your claude.ai login" and the subscription the
 *  user just signed in with is never the credential actually used.
 *  Keep the two lists in sync. */
export const MANAGED_AUTH_VARS = [
  'ANTHROPIC_API_KEY',
  'ANTHROPIC_AUTH_TOKEN',
  'ANTHROPIC_BASE_URL',
] as const;

/** Shell prefix that clears {@link MANAGED_AUTH_VARS} before a command runs. */
export const CLEAR_AUTH_ENV_PREFIX = `unset ${MANAGED_AUTH_VARS.join(' ')}; `;

export async function checkClaudeInstalled(): Promise<ClaudeStatus> {
  return invoke('check_claude_installed');
}

export interface SshStatus {
  available: boolean;
  path: string | null;
}

/** Check whether the OpenSSH client (`ssh`) is available on this machine. */
export async function checkSshAvailable(): Promise<SshStatus> {
  return invoke('check_ssh_available');
}

export async function installClaude(method: string): Promise<void> {
  return invoke('install_claude', { method });
}

export async function storeApiKey(key: string): Promise<void> {
  return invoke('store_api_key', { key });
}

export async function getApiKey(): Promise<string | null> {
  return invoke('get_api_key');
}

export async function deleteApiKey(): Promise<void> {
  return invoke('delete_api_key');
}

export async function checkOAuthStatus(): Promise<boolean> {
  return invoke('check_oauth_status');
}

export async function launchClaudeLogin(): Promise<string> {
  return invoke('launch_claude_login');
}

export async function checkAuthStatus(): Promise<AuthStatus> {
  return invoke('check_auth_status');
}

export async function startClaudeSession(params: {
  sessionId: string;
  prompt: string;
  projectPath: string;
  model?: string;
  maxTurns?: number;
  resumeSession?: string;
}): Promise<void> {
  return invoke('start_claude_session', params);
}

export async function stopClaudeSession(sessionId: string): Promise<void> {
  return invoke('stop_claude_session', { sessionId });
}

export async function onClaudeEvent(
  sessionId: string,
  callback: (line: string) => void,
): Promise<UnlistenFn> {
  return listen<{ line: string }>(`claude-event-${sessionId}`, (event) => {
    callback(event.payload.line);
  });
}

export async function onClaudeDone(
  sessionId: string,
  callback: () => void,
): Promise<UnlistenFn> {
  return listen(`claude-done-${sessionId}`, callback);
}
