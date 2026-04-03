/**
 * Cross-platform detection utilities.
 *
 * Uses navigator.userAgent for synchronous, instant access to the current
 * platform. This avoids async Tauri API calls in render paths and works
 * identically in dev (browser) and production (Tauri webview).
 */

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
