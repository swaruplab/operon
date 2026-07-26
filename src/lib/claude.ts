import { invoke } from '@tauri-apps/api/core';
import { listen, emit, type UnlistenFn } from '@tauri-apps/api/event';
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

export interface ClaudeInvocation {
  /** False = Claude Code isn't installed. Offer an install step, not a login. */
  resolved: boolean;
  /** Shell-ready command word — absolute and quoted where resolvable. */
  command: string;
}

/** Resolve how to invoke `claude`, and whether it exists at all. */
export async function getClaudeInvocation(): Promise<ClaudeInvocation> {
  return invoke('get_claude_invocation');
}

export type StartLoginResult =
  | { ok: true; terminalId: string }
  | { ok: false; reason: 'not-installed' };

/**
 * Open a terminal tab running `claude login`.
 *
 * Two things this must not do, both of which were real bugs:
 *  - Open a terminal when Claude Code isn't installed. The tab then prints
 *    "command not found" while the panel claims the login is running, and the
 *    Verify button can never succeed.
 *  - Type a bare `claude`. The PTY is an interactive NON-login shell, so on a
 *    Finder/Dock launch it can miss ~/.local/bin even for a working install.
 *    We pin the absolute path the backend resolved.
 *
 * The `kind` field — not a regex on the command string — is what tells
 * TerminalInstance to apply TERM=dumb and {@link CLEAR_AUTH_ENV_PREFIX}, since
 * an absolute path would no longer match a `/^claude login/` pattern.
 */
export async function startClaudeLogin(): Promise<StartLoginResult> {
  const inv = await getClaudeInvocation();
  if (!inv.resolved) return { ok: false, reason: 'not-installed' };
  const terminalId = crypto.randomUUID();
  await emit('open-login-terminal', {
    terminalId,
    title: 'Claude Login',
    command: `${inv.command} login`,
    kind: 'claude-login',
  });
  return { ok: true, terminalId };
}

/** Output patterns that mean the shell could not run `claude` at all. */
const COMMAND_NOT_FOUND = /command not found|not recognized as an internal|No such file or directory/i;

/** True if a chunk of terminal output shows the login command never ran. */
export function looksLikeClaudeMissing(output: string): boolean {
  return COMMAND_NOT_FOUND.test(output);
}

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
