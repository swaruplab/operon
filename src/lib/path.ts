/**
 * Separator-aware path utilities for LOCAL filesystem paths.
 *
 * Local paths follow the host OS convention: POSIX uses '/', Windows uses '\\'
 * (and Windows paths can be mixed, e.g. `C:\a/b`). These helpers split and join
 * on BOTH separators and infer the dominant separator from the string itself —
 * no platform global is imported, which makes them robust regardless of where
 * the path originated (terminal CWD, settings, drag-drop, etc.).
 *
 * NOTE: These are for LOCAL paths only. Remote paths are always POSIX and
 * should keep using plain '/' splitting.
 */

/** Matches either separator. */
const SEP_RE = /[/\\]/;

/** Trailing separators (one or more of either kind) at the end of a string. */
const TRAILING_SEP_RE = /[/\\]+$/;

/**
 * Pick the separator to use when joining onto `dir`: backslash only if `dir`
 * contains a backslash and no forward slash (a pure-Windows path); otherwise
 * forward slash. This keeps joins consistent with the existing path's style.
 */
function separatorFor(dir: string): '/' | '\\' {
  return dir.includes('\\') && !dir.includes('/') ? '\\' : '/';
}

/** True if `p` is a Windows drive root like `C:\`, `C:`, or `C:/`. */
function isWindowsDriveRoot(p: string): boolean {
  return /^[A-Za-z]:[/\\]?$/.test(p);
}

/**
 * Match a UNC share root prefix: two leading separators, a non-empty server
 * segment, a separator, and a non-empty share segment (e.g. `//server/share`
 * or `\\server\share`). The matched prefix is `//server/share` WITHOUT any
 * trailing separator or sub-path — it is the deepest point dirname/navigateUp
 * may walk up to, since `//server` alone is not a real location.
 *
 * Capture group 1 is the share-root prefix; the regex is intentionally not
 * anchored at the end so it also matches `//server/share/sub...`.
 */
const UNC_ROOT_RE = /^([/\\]{2}[^/\\]+[/\\][^/\\]+)/;

/** True if `p` is exactly a bare UNC share root like `//server/share`. */
function isUncShareRoot(p: string): boolean {
  const m = UNC_ROOT_RE.exec(p);
  // It's a bare share root only when nothing (or just a trailing sep) follows.
  return m !== null && p.replace(TRAILING_SEP_RE, '') === m[1];
}

/** Last path segment, splitting on both '/' and '\\'. */
export function basename(p: string): string {
  // Strip trailing separators so `C:\a\b\` → `b`, not ``.
  const trimmed = p.replace(TRAILING_SEP_RE, '');
  const parts = trimmed.split(SEP_RE);
  return parts[parts.length - 1] || trimmed;
}

/**
 * Everything before the last segment, preserving the original separator style.
 * - POSIX root `/` → `/`
 * - Windows drive root `C:\` (or `C:`) → `C:\`
 * - `C:\a\b` → `C:\a`, `/a/b` → `/a`
 * - UNC share root `//server/share` → `//server/share` (already a root)
 * - `//server/share/sub` → `//server/share` (never below the share)
 * - relative single segment `project` (no separator) → `project` unchanged;
 *   callers treat an unchanged return as "can't go up" (navigateUp stops,
 *   rename keeps it in place) so this must not strip to '' or loop.
 */
export function dirname(p: string): string {
  // A bare UNC share root is itself a root — don't strip down to `//server`.
  // Checked before the generic isRoot() so a trailing separator is normalized
  // (`//server/share/` → `//server/share`).
  if (isUncShareRoot(p)) return p.replace(TRAILING_SEP_RE, '');
  if (isRoot(p)) return p;
  // For deeper UNC paths, clamp the floor to the share root so we never walk
  // up into the bogus `//server` (the share prefix is the root boundary).
  const unc = UNC_ROOT_RE.exec(p);
  if (unc) {
    const trimmed = p.replace(TRAILING_SEP_RE, '');
    const idx = Math.max(trimmed.lastIndexOf('/'), trimmed.lastIndexOf('\\'));
    // If the last separator sits inside (or at the end of) the share prefix,
    // the parent IS the share root.
    if (idx <= unc[1].length - 1) return unc[1];
    return trimmed.slice(0, idx);
  }
  // Drop trailing separators before locating the last segment.
  const trimmed = p.replace(TRAILING_SEP_RE, '');
  const idx = Math.max(trimmed.lastIndexOf('/'), trimmed.lastIndexOf('\\'));
  if (idx < 0) return trimmed; // no separator — relative single segment, returned unchanged
  // A Windows drive root keeps its separator: `C:\a` → dirname `C:\`.
  if (/^[A-Za-z]:$/.test(trimmed.slice(0, idx))) {
    return trimmed.slice(0, idx + 1);
  }
  // POSIX root: dirname of `/a` is `/`, not ``.
  if (idx === 0) return trimmed[0]; // the leading '/' or '\\'
  return trimmed.slice(0, idx);
}

/**
 * Join `name` onto `dir` using the separator already present in `dir`
 * (backslash for a pure-Windows path, otherwise forward slash). A trailing
 * separator on `dir` (including a root) is handled without doubling up.
 *
 * An empty `dir` means "no base", so the result is just `name` — joining onto
 * nothing is a relative reference, NOT an absolute path. (Without this guard
 * `separatorFor('')` would pick '/' and produce a bogus root path `/name`.)
 */
export function joinLocal(dir: string, name: string): string {
  if (dir === '') return name;
  const sep = separatorFor(dir);
  if (TRAILING_SEP_RE.test(dir)) return `${dir}${name}`;
  return `${dir}${sep}${name}`;
}

/**
 * True for the POSIX root `/`, a Windows drive root like `C:\` or `C:`, or a
 * bare UNC share root like `//server/share` (`\\server\share`). A UNC share
 * root has no navigable parent, so it counts as a root just like the others.
 */
export function isRoot(p: string): boolean {
  return p === '/' || p === '\\' || isWindowsDriveRoot(p) || isUncShareRoot(p);
}
