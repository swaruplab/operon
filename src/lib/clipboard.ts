/**
 * Copy text to the clipboard, reliably, inside a Tauri WKWebView.
 *
 * `navigator.clipboard.writeText` requires a "secure context" + an explicit
 * permission that the Tauri custom-protocol webview (tauri://, not https://)
 * does NOT grant — so on macOS it rejects/throws and, with the swallowed
 * `.catch(() => {})` callers used, every copy silently no-ops. We try the async
 * Clipboard API first (works where it's permitted), then fall back to the
 * legacy synchronous `document.execCommand('copy')` via an off-screen textarea,
 * which works in any webview. Returns true only if a path actually succeeded.
 */
export async function copyText(text: string): Promise<boolean> {
  if (!text) return false;

  // Path 0 — Tauri native clipboard plugin. This is the ONLY path that reliably
  // works in the WKWebView (it goes through Rust to NSPasteboard, bypassing the
  // webview's blocked navigator.clipboard / restricted execCommand). Dynamically
  // imported so this util still works in a plain browser/test context.
  try {
    const { writeText } = await import('@tauri-apps/plugin-clipboard-manager');
    await writeText(text);
    return true;
  } catch {
    // Plugin unavailable (non-Tauri context) — fall through to web paths.
  }

  // Path 1 — async Clipboard API (only when the context allows it).
  try {
    if (window.isSecureContext && navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(text);
      return true;
    }
  } catch {
    // Fall through to the legacy path.
  }

  // Path 2 — off-screen textarea + execCommand('copy'). Works in WKWebView.
  try {
    const ta = document.createElement('textarea');
    ta.value = text;
    ta.setAttribute('readonly', '');
    // Keep it in the layout (some webviews ignore selection on display:none /
    // visibility:hidden elements) but off-screen and inert.
    ta.style.position = 'fixed';
    ta.style.top = '0';
    ta.style.left = '0';
    ta.style.width = '1px';
    ta.style.height = '1px';
    ta.style.padding = '0';
    ta.style.border = 'none';
    ta.style.outline = 'none';
    ta.style.boxShadow = 'none';
    ta.style.background = 'transparent';
    ta.style.opacity = '0';
    ta.style.pointerEvents = 'none';
    document.body.appendChild(ta);

    const prevSelection = document.getSelection()?.rangeCount
      ? document.getSelection()!.getRangeAt(0)
      : null;

    ta.focus();
    ta.select();
    ta.setSelectionRange(0, text.length);
    const ok = document.execCommand('copy');

    document.body.removeChild(ta);

    // Restore any prior selection we clobbered.
    if (prevSelection) {
      const sel = document.getSelection();
      sel?.removeAllRanges();
      sel?.addRange(prevSelection);
    }
    return ok;
  } catch {
    return false;
  }
}
