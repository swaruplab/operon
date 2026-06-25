import { useEffect, useRef, useCallback } from 'react';
import { Terminal, type ITheme } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import { WebglAddon } from '@xterm/addon-webgl';
import { WebLinksAddon } from '@xterm/addon-web-links';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { getSettings } from '../../lib/settings';
import { parseSbatchIds, registerWatchedJob } from '../../lib/watchdog';
import { useTheme } from '../../context/ThemeContext';
import { copyText } from '../../lib/clipboard';
import '@xterm/xterm/css/xterm.css';

// Xterm theme palettes — kept in sync with the app's CSS variables in
// styles.css. ANSI colors (red/green/…) stay vivid in both themes; only the
// canvas + foreground swap so text contrast stays high.
const XTERM_DARK: ITheme = {
  background: '#09090b',
  foreground: '#fafafa',
  cursor: '#fafafa',
  cursorAccent: '#09090b',
  selectionBackground: '#3f3f46',
  selectionForeground: '#fafafa',
  black: '#27272a',
  red: '#ef4444',
  green: '#22c55e',
  yellow: '#eab308',
  blue: '#3b82f6',
  magenta: '#a855f7',
  cyan: '#06b6d4',
  white: '#fafafa',
  brightBlack: '#71717a',
  brightRed: '#f87171',
  brightGreen: '#4ade80',
  brightYellow: '#facc15',
  brightBlue: '#60a5fa',
  brightMagenta: '#c084fc',
  brightCyan: '#22d3ee',
  brightWhite: '#ffffff',
};

const XTERM_LIGHT: ITheme = {
  background: '#fafafa',
  foreground: '#18181b',
  cursor: '#18181b',
  cursorAccent: '#fafafa',
  selectionBackground: '#d4d4d8',
  selectionForeground: '#18181b',
  // ANSI palette tuned darker for contrast against light backgrounds.
  black: '#18181b',
  red: '#b91c1c',
  green: '#15803d',
  yellow: '#a16207',
  blue: '#1d4ed8',
  magenta: '#7c3aed',
  cyan: '#0e7490',
  white: '#52525b',
  brightBlack: '#52525b',
  brightRed: '#dc2626',
  brightGreen: '#16a34a',
  brightYellow: '#ca8a04',
  brightBlue: '#2563eb',
  brightMagenta: '#9333ea',
  brightCyan: '#0891b2',
  brightWhite: '#18181b',
};

/**
 * Detects OAuth URLs in terminal output (e.g. from `claude login` on remote servers)
 * and automatically opens them in the local browser. On headless servers the browser
 * can't open, so we intercept the URL and open it locally for the user.
 */

// Match only valid URL characters (RFC 3986) — stops before control chars, >, <, etc.
// Claude's OAuth URL host/path has changed over time — match the current
// claude.com/cai/oauth form AND the legacy claude.ai/oauth (and console.anthropic)
// so the auto-open keeps working across Claude Code versions. (The old regex only
// matched claude.ai/oauth/authorize, so it silently stopped auto-opening when the
// CLI switched to https://claude.com/cai/oauth/authorize.)
const OAUTH_URL_REGEX = /https:\/\/(?:claude\.ai\/oauth|claude\.com\/cai\/oauth|console\.anthropic\.com\/oauth)\/authorize[A-Za-z0-9%&=?._~:/@!$'()*+,;\-[\]]+/;

// Comprehensive ANSI escape code stripper — handles CSI sequences (including
// private mode with ? like \x1b[?2026l), OSC sequences, and simple two-char escapes.
// Also strips cursor movement/positioning sequences that cause TUI overlay corruption.
const ANSI_REGEX = /\x1b(?:\[[?]?[0-9;]*[a-zA-Z@`]|\][^\x07\x1b]*(?:\x07|\x1b\\)?|[()][A-Z0-9]|[A-Z=><78])/g;

const OAUTH_BUFFER_MAX = 4096;

/**
 * Clean up an OAuth URL extracted from PTY output.
 * Claude Code's TUI renders "(c to copy)" as an overlay using ANSI cursor positioning,
 * which can corrupt the URL in the raw PTY stream (e.g., "scope=org%3Acr(c to copy)y"
 * instead of "scope=org%3Acreate_api_key"). This function repairs known corruptions.
 */
function cleanOAuthUrl(url: string): string {
  let cleaned = url;

  // Remove any Claude Code TUI text that got embedded via cursor positioning overlay
  // Pattern: "(c to copy)" or "(ctocopy)" (with or without spaces, after whitespace collapse)
  cleaned = cleaned.replace(/\(c\s*to\s*copy\)/gi, '');
  cleaned = cleaned.replace(/\(ctocopy\)/gi, '');

  // After removing TUI artifacts, the URL may have broken parameter values.
  // Known fixups for Claude OAuth URLs:
  // - "scope=org%3Acry" → missing "eate_api_key" (the overlay replaced it)
  // - Fix: reconstruct the known scope parameter
  cleaned = cleaned.replace(
    /scope=org%3Acr(?:y|eate_api_key)/,
    'scope=org%3Acreate_api_key'
  );

  // Remove any residual control characters or non-URL bytes
  cleaned = cleaned.replace(/[\x00-\x1f\x7f]/g, '');

  // The TUI positions its "Paste code here…" prompt immediately after the URL
  // using cursor escapes (no whitespace delimiter), so once ANSI is stripped the
  // start of that prose ("Past"/"Paste") gets glued onto the LAST query param,
  // `state=`, corrupting it (the #1 "invalid link" cause). Claude's `state` and
  // `code_challenge` are 43-char base64url tokens (256-bit), so truncate `state`
  // to 43 chars — this drops any glued prose regardless of how much, without
  // assuming what the prose is. (code_challenge can't be glued — it's mid-URL,
  // bounded by the following `&`.)
  cleaned = cleaned.replace(/(state=[A-Za-z0-9_-]{43})[A-Za-z0-9_-]*/, '$1');

  // Trim any trailing characters that aren't valid URL chars
  cleaned = cleaned.replace(/[^A-Za-z0-9%&=?._~:/@!$'()*+,;\-[\]]+$/, '');

  return cleaned;
}

interface TerminalInstanceProps {
  terminalId: string;
  isVisible: boolean;
  /** Command to send to the shell once it's ready (non-SSH only, e.g. `claude login`). */
  initialCommand?: string;
  /** Structured SSH argument vector. When present this is an SSH terminal and the
   *  args are passed to spawn_terminal verbatim — never parsed from a string. */
  sshArgs?: string[];
  /** SSH profile id — if set, Operon auto-registers any `Submitted batch job NNNN`
   *  line with the HPC watchdog for this profile. */
  sshProfileId?: string;
  onTitleChange?: (title: string) => void;
  /** Called when the PTY process exits. Receives the OS exit code (null if it
   *  couldn't be captured). Non-zero on an SSH terminal means auth/network
   *  failure; the parent uses this + the spawn timestamp to gate the
   *  connected-state flip. */
  onExit?: (exitCode: number | null) => void;
  onCwdChange?: (cwd: string) => void;
}

export function TerminalInstance({ terminalId, isVisible, initialCommand, sshArgs, sshProfileId, onTitleChange, onExit, onCwdChange }: TerminalInstanceProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const termRef = useRef<Terminal | null>(null);
  const fitAddonRef = useRef<FitAddon | null>(null);
  const webglAddonRef = useRef<WebglAddon | null>(null);
  const unlistenOutputRef = useRef<UnlistenFn | null>(null);
  const unlistenExitRef = useRef<UnlistenFn | null>(null);
  const resizeTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Track the app theme so we can swap xterm colors when the user toggles.
  const { resolved: themeMode } = useTheme();
  const xtermTheme = themeMode === 'light' ? XTERM_LIGHT : XTERM_DARK;

  // Stable refs for callbacks — avoids re-creating the terminal when parent re-renders
  const onTitleChangeRef = useRef(onTitleChange);
  onTitleChangeRef.current = onTitleChange;
  const onExitRef = useRef(onExit);
  onExitRef.current = onExit;
  const onCwdChangeRef = useRef(onCwdChange);
  onCwdChangeRef.current = onCwdChange;

  // Debounced resize handler — fit first, then sync to backend
  const handleResize = useCallback(() => {
    if (resizeTimeoutRef.current) clearTimeout(resizeTimeoutRef.current);

    resizeTimeoutRef.current = setTimeout(() => {
      if (!fitAddonRef.current || !termRef.current) return;

      fitAddonRef.current.fit();
      const { rows, cols } = termRef.current;
      invoke('resize_terminal', { terminalId, rows, cols }).catch(console.error);
    }, 100);
  }, [terminalId]);

  // Re-fit when visibility changes (tab switches)
  useEffect(() => {
    if (isVisible && fitAddonRef.current && termRef.current) {
      const timeout = setTimeout(() => {
        fitAddonRef.current?.fit();
        const term = termRef.current;
        if (term) {
          invoke('resize_terminal', {
            terminalId,
            rows: term.rows,
            cols: term.cols,
          }).catch(console.error);
          term.focus();
        }
      }, 50);
      return () => clearTimeout(timeout);
    }
  }, [isVisible, terminalId]);

  // Live theme swap — when the user toggles light/dark, update the existing
  // terminal's color palette without recreating the instance (which would
  // wipe the scrollback).
  //
  // Just setting .options.theme is enough for the DOM renderer, but the
  // WebGL renderer (default) caches glyphs in a texture atlas keyed on
  // color — without re-creating the addon, characters stay rendered in the
  // OLD theme's colors until each is repainted. Dispose + re-attach the
  // WebGL addon so the new theme takes immediately on every cell.
  useEffect(() => {
    const term = termRef.current;
    if (!term) return;
    term.options.theme = xtermTheme;
    if (webglAddonRef.current) {
      webglAddonRef.current.dispose();
      webglAddonRef.current = null;
      try {
        const fresh = new WebglAddon();
        term.loadAddon(fresh);
        webglAddonRef.current = fresh;
      } catch {
        // WebGL no longer available — canvas fallback already in use.
      }
    } else {
      // Canvas renderer — force redraw of every cell.
      term.refresh(0, term.rows - 1);
    }
  }, [xtermTheme]);

  useEffect(() => {
    if (!containerRef.current) return;

    // Defense-in-depth: tear down any prior xterm instance that leaked — e.g. an
    // HMR/StrictMode re-run that re-entered this effect before the old cleanup
    // ran — so we never end up with two live terminals (and thus two onData
    // input handlers) bound to the same surviving backend PTY, which would
    // write every keystroke twice ("login" -> "llooggiinn").
    if (termRef.current) {
      termRef.current.dispose();
      termRef.current = null;
    }

    // Guards late-resolving listen() promises: under StrictMode (mount→cleanup→
    // mount) the first mount's listen() can resolve AFTER its cleanup ran, when
    // the unlisten ref was still null — leaking a second pty-output listener
    // that renders every echoed byte twice ("ssqqhhuujj"). If cleanup already
    // ran, the late listener unlistens itself instead of being stored.
    let cancelled = false;

    // Create xterm.js terminal instance
    const term = new Terminal({
      rows: 24,
      cols: 80,
      cursorBlink: true,
      cursorStyle: 'block',
      fontSize: 13,
      fontFamily: "'JetBrains Mono', 'SF Mono', Menlo, Monaco, Consolas, 'Cascadia Code', 'DejaVu Sans Mono', 'Liberation Mono', 'Ubuntu Mono', 'Courier New', monospace",
      lineHeight: 1.4,
      letterSpacing: 0,
      theme: xtermTheme,
      allowProposedApi: true,
    });

    termRef.current = term;

    // Load addons
    const fitAddon = new FitAddon();
    fitAddonRef.current = fitAddon;
    term.loadAddon(fitAddon);
    term.loadAddon(new WebLinksAddon());

    // Open terminal in DOM
    term.open(containerRef.current);

    // WebGL renderer lifecycle:
    //   - Respect the user's `terminal_use_webgl` setting (some GPU + external-
    //     display combos, e.g. Mac mini + Apple Studio Display scaled modes,
    //     render the atlas with hairline artifacts — users turn WebGL off).
    //   - Re-dispose the addon and fall back to canvas if the WebGL context is
    //     lost at runtime.
    //   - Clear the glyph atlas on device-pixel-ratio changes so moving the
    //     window between displays doesn't leave stretched/blurry glyphs.
    let webglAddon: WebglAddon | null = null;
    const attachWebgl = () => {
      if (webglAddon) return;
      try {
        const addon = new WebglAddon();
        addon.onContextLoss(() => {
          console.warn('xterm WebGL context lost — falling back to canvas');
          addon.dispose();
          webglAddon = null;
          webglAddonRef.current = null;
        });
        term.loadAddon(addon);
        webglAddon = addon;
        webglAddonRef.current = addon;
      } catch {
        console.warn('WebGL renderer not available, using canvas');
      }
    };

    getSettings()
      .then((s) => {
        if (s.terminal_use_webgl) attachWebgl();
      })
      .catch(() => {
        // If settings can't load, fall back to defaults (WebGL on).
        attachWebgl();
      });

    // Clear the texture atlas whenever DPR changes (monitor hot-plug, scaling
    // toggle). `clearTextureAtlas` is a no-op when no atlas-backed renderer is
    // loaded, so it's safe to call unconditionally.
    let dprMql: MediaQueryList | null = null;
    const attachDprWatcher = () => {
      try {
        dprMql = window.matchMedia(`(resolution: ${window.devicePixelRatio}dppx)`);
        const onChange = () => {
          // Rebuild atlas against the new DPR.
          (term as unknown as { clearTextureAtlas?: () => void }).clearTextureAtlas?.();
          // Rebind the listener to the *new* DPR so we catch the next change.
          dprMql?.removeEventListener('change', onChange);
          attachDprWatcher();
        };
        dprMql.addEventListener('change', onChange);
      } catch {
        /* matchMedia missing in some embedded webviews — ignore */
      }
    };
    attachDprWatcher();

    // Initial fit
    fitAddon.fit();

    // Watch for container resize
    const resizeObserver = new ResizeObserver(handleResize);
    resizeObserver.observe(containerRef.current);

    // Track whether we've already opened an OAuth URL for this terminal session
    let oauthOpened = false;
    // Per-instance buffer for partial URL accumulation (not shared across terminals)
    let oauthBuffer = '';

    // Small line buffer for detecting `Submitted batch job NNN` — the scheduler
    // prints exactly this one line, so we only need the tail of the stream.
    let sbatchBuffer = '';
    const registered = new Set<string>();

    // Listen for PTY output from Rust backend
    listen<{ output: string }>(`pty-output-${terminalId}`, (event) => {
      const data = event.payload.output;
      term.write(data);

      // --- sbatch auto-register (HPC watchdog, SSH terminals only) ---
      if (sshProfileId) {
        sbatchBuffer = (sbatchBuffer + data.replace(ANSI_REGEX, '')).slice(-4096);
        const ids = parseSbatchIds(sbatchBuffer);
        for (const id of ids) {
          if (registered.has(id)) continue;
          registered.add(id);
          registerWatchedJob(sshProfileId, id, 'slurm', null).catch(() => {});
        }
      }

      // --- OSC 7 CWD detection (shell reports working directory) ---
      // Format: \x1b]7;file://hostname/path\x07  or  \x1b]7;file://hostname/path\x1b\\
      const osc7Match = data.match(/\x1b\]7;file:\/\/[^/]*([^\x07\x1b]+)(?:\x07|\x1b\\)/);
      if (osc7Match) {
        const cwd = decodeURIComponent(osc7Match[1]);
        onCwdChangeRef.current?.(cwd);
      }

      // --- OAuth URL auto-open for `claude login` (local or remote) ---
      if (!oauthOpened) {
        // Accumulate output to handle URLs split across chunks
        oauthBuffer += data;
        if (oauthBuffer.length > OAUTH_BUFFER_MAX) {
          oauthBuffer = oauthBuffer.slice(-OAUTH_BUFFER_MAX);
        }

        // Strip ALL ANSI escape codes (including private mode like \x1b[?2026l)
        const clean = oauthBuffer.replace(ANSI_REGEX, '');
        // Strip ALL carriage returns and newlines to rejoin line-wrapped URLs.
        // Remote terminals use bare \r (without \n) at line-wrap boundaries,
        // which was causing URL truncation at exactly the terminal column width.
        // Spaces are preserved so "Paste code here..." stays separated from the URL.
        const collapsed = clean.replace(/[\r\n]+/g, '');

        const match = collapsed.match(OAUTH_URL_REGEX);
        if (match) {
          oauthOpened = true;
          oauthBuffer = '';
          const rawMatch = match[0];
          const url = cleanOAuthUrl(rawMatch);
          console.log('[OAuth] Raw match length:', rawMatch.length, 'Clean URL length:', url.length);
          console.log('[OAuth] URL:', url);

          // Write the URL to terminal as a copyable fallback, then try to auto-open
          term.write(
            '\r\n\x1b[1;36m━━━ OAuth Login Link ━━━\x1b[0m\r\n' +
            '\x1b[1;37m' + url + '\x1b[0m\r\n' +
            '\x1b[1;36m━━━━━━━━━━━━━━━━━━━━━━━━\x1b[0m\r\n'
          );

          // Open in local browser via Tauri command (window.open is blocked in webview)
          invoke('open_url', { url }).then(() => {
            term.write(
              '\x1b[1;36m✓ Operon opened the link in your browser.\x1b[0m\r\n' +
              '\x1b[90m  If the browser didn\'t open, copy the URL above and paste it manually.\x1b[0m\r\n' +
              '\x1b[90m  Complete sign-in, then paste the code here when prompted.\x1b[0m\r\n'
            );
          }).catch(() => {
            term.write(
              '\x1b[1;33m⚠ Could not auto-open the browser. Copy the URL above and open it manually.\x1b[0m\r\n'
            );
          });
        }
      }
    }).then((unlisten) => {
      if (cancelled) { unlisten(); return; }
      unlistenOutputRef.current = unlisten;
    });

    // Listen for process exit. Payload carries the OS exit code (or null if
    // the backend couldn't reap the child) — forwarded to the parent so it
    // can distinguish ssh auth failure (255) from a clean logout (0).
    listen<{ exit_code: number | null }>(`pty-exit-${terminalId}`, (event) => {
      term.write('\r\n\x1b[90m[Process exited]\x1b[0m\r\n');
      onExitRef.current?.(event.payload?.exit_code ?? null);
    }).then((unlisten) => {
      if (cancelled) { unlisten(); return; }
      unlistenExitRef.current = unlisten;
    });

    // Send user input to PTY. Capture the IDisposable so cleanup can detach
    // THIS handler explicitly — never rely on term.dispose() alone, or a leaked
    // duplicate terminal could keep a stale onData writing into the shared PTY.
    const dataDisp = term.onData((data) => {
      // Strip terminal-generated Device-Attributes REPLIES (DA1 `ESC[?…c`,
      // DA2 `ESC[>…c`) before they reach the PTY. xterm.js auto-answers a
      // device-attributes query (e.g. tmux probing capabilities on attach)
      // through this same onData callback; if that reply reaches the remote
      // shell's stdin it fuses onto the next command (`0;276;0ccd '…'` →
      // bash splits on `;`). A real user never types these, so dropping them
      // on the way OUT is safe and kills the corruption regardless of timing.
      const clean = data.replace(/\x1b\[[>?]?[0-9;]*c/g, '');
      if (!clean) return;
      invoke('write_terminal', {
        terminalId,
        data: Array.from(new TextEncoder().encode(clean)),
      }).catch(console.error);
    });

    // Auto-copy: when user selects text, copy it to clipboard automatically (like iTerm2)
    const selDisp = term.onSelectionChange(() => {
      const selection = term.getSelection();
      if (selection) {
        void copyText(selection);
      }
    });

    // Track terminal title changes (for tab names)
    const titleDisp = term.onTitleChange((title) => {
      onTitleChangeRef.current?.(title);
    });

    // OSC 52 clipboard-write support. Programs like the `claude login` TUI emit
    // ESC ] 52 ; c ; <base64> BEL when you press "c to copy"; xterm.js ignores
    // OSC 52 by default, so that copy silently no-ops. Decode the payload and
    // write it to the (native, plugin-backed) clipboard. data is "<sel>;<b64>".
    const osc52Disp = term.parser.registerOscHandler(52, (data) => {
      const semi = data.indexOf(';');
      const b64 = semi >= 0 ? data.slice(semi + 1) : data;
      try {
        const text = decodeURIComponent(escape(atob(b64)));
        if (text) void copyText(text);
      } catch {
        // malformed / non-base64 OSC 52 payload — ignore
      }
      return true; // handled
    });

    // Spawn terminal. SSH terminals receive a structured argument vector (the
    // `sshArgs` prop, built by SSHView) passed straight to spawn_terminal —
    // never parsed from a string — so the remote command survives intact.
    const isSshTerminal = !!(sshArgs && sshArgs.length > 0);

    invoke('spawn_terminal', { terminalId, sshArgs: isSshTerminal ? sshArgs : null })
      .then(() => {
        // For SSH terminals, inject OSC 7 hook so CWD changes are reported back.
        // This makes terminal→explorer sync work for remote sessions.
        if (isSshTerminal) {
          setTimeout(() => {
            // Inject OSC 7 hook into the remote shell for CWD tracking.
            // The command echoes in the PTY, so we clear the xterm buffer
            // after a short delay to hide it completely.
            const hookScript = `if [ -n "$ZSH_VERSION" ]; then autoload -Uz add-zsh-hook 2>/dev/null; __operon_osc7() { printf '\\033]7;file://%s%s\\a' "$(hostname)" "$(pwd)"; }; add-zsh-hook precmd __operon_osc7 2>/dev/null; elif [ -n "$BASH_VERSION" ]; then __operon_osc7() { printf '\\033]7;file://%s%s\\a' "$(hostname)" "$(pwd)"; }; PROMPT_COMMAND="__operon_osc7;\${PROMPT_COMMAND}"; fi`;
            const b64 = btoa(hookScript);
            const cmd = ` eval "$(echo '${b64}' | base64 -d)"\n`;
            invoke('write_terminal', {
              terminalId,
              data: Array.from(new TextEncoder().encode(cmd)),
            }).catch(console.error);
            // Clear the xterm buffer to hide the injected command
            setTimeout(() => {
              term.clear();
              // Send 'clear' to also reset the remote terminal
              invoke('write_terminal', {
                terminalId,
                data: Array.from(new TextEncoder().encode('clear\n')),
              }).catch(console.error);
            }, 500);
          }, 1500);
        }

        // For non-SSH initialCommands (e.g. `claude login`), send the command
        // as stdin input after a short delay so the shell has time to start.
        // Prefix `claude login` with TERM=dumb to avoid TUI rendering issues
        // in xterm.js — the plain text output makes OAuth URL detection reliable.
        if (initialCommand && !initialCommand.startsWith('ssh ')) {
          setTimeout(() => {
            let cmd = initialCommand;
            // If the command is `claude login` (not already prefixed), add TERM=dumb
            if (/^claude\s+login/.test(cmd) && !cmd.includes('TERM=')) {
              cmd = `TERM=dumb ${cmd}`;
            }
            const cmdWithNewline = cmd + '\n';
            invoke('write_terminal', {
              terminalId,
              data: Array.from(new TextEncoder().encode(cmdWithNewline)),
            }).catch(console.error);
          }, 300);
        }
      })
      .catch((err) => {
        term.write(`\x1b[31mFailed to spawn terminal: ${err}\x1b[0m\r\n`);
      });

    // Cleanup — only dispose the xterm UI; do NOT kill the backend process.
    // The terminal process should survive panel hide/show toggles.
    // kill_terminal is called explicitly from TerminalArea.closeTab instead.
    return () => {
      cancelled = true;
      resizeObserver.disconnect();
      if (resizeTimeoutRef.current) clearTimeout(resizeTimeoutRef.current);
      unlistenOutputRef.current?.();
      unlistenExitRef.current?.();
      // Drop the DPR watcher. We can't reference the exact listener here
      // (it's rebound recursively), but removing the mql itself is enough —
      // it won't fire after the term is disposed.
      dprMql = null;
      // Detach the captured xterm handlers BEFORE disposing the terminal, so a
      // duplicated/leaked instance can never leave a stale onData feeding the
      // surviving PTY (the double-keystroke bug).
      dataDisp.dispose();
      selDisp.dispose();
      titleDisp.dispose();
      osc52Disp.dispose();
      term.dispose();
      oauthBuffer = '';
    };
  }, [terminalId, initialCommand, sshArgs, handleResize]); // Only re-run when terminalId changes — callbacks use refs

  return (
    <div
      ref={containerRef}
      className="h-full w-full bg-canvas p-1"
      style={{ visibility: isVisible ? 'visible' : 'hidden' }}
    />
  );
}
