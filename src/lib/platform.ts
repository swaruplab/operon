/**
 * Cross-platform detection utilities.
 *
 * Uses navigator.userAgent for synchronous, instant access to the current
 * platform. This avoids async Tauri API calls in render paths and works
 * identically in dev (browser) and production (Tauri webview).
 *
 * For authoritative capability flags (instead of fragile UA sniffing), use
 * the async `getPlatformInfo()` API at the bottom of this file — it reads
 * the truth from the Rust backend's `get_platform_info` command.
 */

import { invoke } from '@tauri-apps/api/core';

export type Platform = 'macos' | 'windows' | 'linux';

/** Detect the current platform from the user-agent string. */
function detectPlatform(): Platform {
  const ua = navigator.userAgent.toLowerCase();
  if (ua.includes('mac')) return 'macos';
  if (ua.includes('win')) return 'windows';
  return 'linux';
}

/** Cached result — the platform never changes at runtime. */
export const platform: Platform = detectPlatform();

export const isMac    = platform === 'macos';
export const isWindows = platform === 'windows';
export const isLinux   = platform === 'linux';

/**
 * The human-readable modifier key name for the current platform.
 * - macOS: "Cmd"
 * - Windows/Linux: "Ctrl"
 */
export const modKey = isMac ? 'Cmd' : 'Ctrl';

/**
 * The modifier key symbol for the current platform.
 * - macOS: "⌘"
 * - Windows/Linux: "Ctrl"
 */
export const modSymbol = isMac ? '⌘' : 'Ctrl';

/**
 * Replace "Cmd" with the platform-appropriate modifier in a shortcut string.
 * e.g. "Cmd+S" → "Ctrl+S" on Windows/Linux, unchanged on macOS.
 */
export function adaptShortcut(s: string): string {
  if (isMac) return s;
  return s.replace(/Cmd/g, 'Ctrl');
}

/**
 * Replace "⌘" with "Ctrl+" on non-macOS platforms.
 * e.g. "⌘P" → "Ctrl+P" on Windows/Linux.
 */
export function adaptSymbol(s: string): string {
  if (isMac) return s;
  return s.replace(/⌘/g, 'Ctrl+');
}

// ─── Backend capability flags ───────────────────────────────────────
//
// Authoritative platform capabilities reported by the Rust backend's
// `get_platform_info` command. Mirrors the `PlatformInfo` struct in
// `src-tauri/src/commands/platform_info.rs` (camelCase fields). Prefer
// these over the UA-derived isMac/isWindows/isLinux booleans for gating
// platform-specific features.

export interface PlatformInfo {
  /** Host OS: "macos" | "windows" | "linux". */
  os: Platform;
  /** Path to Git Bash on Windows (used by Claude Code); null elsewhere. */
  gitBashPath: string | null;
  /** Whether SSH ControlMaster multiplexing is supported. */
  supportsSshMux: boolean;
  /** Whether the bundled Anthropic→OpenAI translation proxy sidecar exists. */
  translationProxySupported: boolean;
}

let platformInfoPromise: Promise<PlatformInfo> | null = null;

/**
 * Load the backend's platform capability flags. Memoized — the underlying
 * `get_platform_info` command is invoked at most once per app session, and
 * subsequent calls return the cached result.
 */
export function getPlatformInfo(): Promise<PlatformInfo> {
  if (!platformInfoPromise) {
    platformInfoPromise = invoke<PlatformInfo>('get_platform_info').catch((err) => {
      // Reset so a transient failure can be retried on the next call.
      platformInfoPromise = null;
      throw err;
    });
  }
  return platformInfoPromise;
}
