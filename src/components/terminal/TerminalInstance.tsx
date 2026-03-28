import { useEffect, useRef, useCallback } from 'react';
import { Terminal } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import { WebglAddon } from '@xterm/addon-webgl';
import { WebLinksAddon } from '@xterm/addon-web-links';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import '@xterm/xterm/css/xterm.css';

/**
 * Detects OAuth URLs in terminal output (e.g. from `claude login` on remote servers)
 * and automatically opens them in the local browser. On headless servers the browser
 * can't open, so we intercept the URL and open it locally for the user.
 */

// Match only valid URL characters (RFC 3986) — stops before control chars, >, <, etc.
const OAUTH_URL_REGEX = /https:\/\/claude\.ai\/oauth\/authorize[A-Za-z0-9%&=?._~:/@!$'()*+,;\-[\]]+/;

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

  // Trim any trailing characters that aren't valid URL chars
  cleaned = cleaned.replace(/[^A-Za-z0-9%&=?._~:/@!$'()*+,;\-[\]]+$/, '');

  return cleaned;
}

interface TerminalInstanceProps {
  terminalId: string;
  isVisible: boolean;
  /** Command to send to the shell once it's ready (e.g. an SSH command) */
  initialCommand?: string;
  onTitleChange?: (title: string) => void;
  onExit?: () => void;
}

export function TerminalInstance({ terminalId, isVisible, initialCommand, onTitleChange, onExit }: TerminalInstanceProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const termRef = useRef<Terminal | null>(null);
  const fitAddonRef = useRef<FitAddon | null>(null);
  const unlistenOutputRef = useRef<UnlistenFn | null>(null);
  const unlistenExitRef = useRef<UnlistenFn | null>(null);
  const resizeTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Stable refs for callbacks — avoids re-creating the terminal when parent re-renders
  const onTitleChangeRef = useRef(onTitleChange);
  onTitleChangeRef.current = onTitleChange;
  const onExitRef = useRef(onExit);
  onExitRef.current = onExit;

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

  useEffect(() => {
    if (!containerRef.current) return;

    // Create xterm.js terminal instance
    const term = new Terminal({
      rows: 24,
      cols: 80,
      cursorBlink: true,
      cursorStyle: 'block',
      fontSize: 13,
      fontFamily: "'JetBrains Mono', 'SF Mono', Menlo, Monaco, 'Courier New', monospace",
      lineHeight: 1.4,
      letterSpacing: 0,
      theme: {
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
      },
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

    // Try WebGL renderer (falls back to canvas)
    try {
      term.loadAddon(new WebglAddon());
    } catch {
      console.warn('WebGL renderer not available, using canvas');
    }

    // Initial fit
    fitAddon.fit();

    // Watch for container resize
    const resizeObserver = new ResizeObserver(handleResize);
    resizeObserver.observe(containerRef.current);

    // Track whether we've already opened an OAuth URL for this terminal session
    let oauthOpened = false;
    // Per-instance buffer for partial URL accumulation (not shared across terminals)
    let oauthBuffer = '';

    // Listen for PTY output from Rust backend
    listen<{ output: string }>(`pty-output-${terminalId}`, (event) => {
      const data = event.payload.output;
      term.write(data);

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
      unlistenOutputRef.current = unlisten;
    });

    // Listen for process exit
    listen(`pty-exit-${terminalId}`, () => {
      term.write('\r\n\x1b[90m[Process exited]\x1b[0m\r\n');
      onExitRef.current?.();
    }).then((unlisten) => {
      unlistenExitRef.current = unlisten;
    });

    // Send user input to PTY
    term.onData((data) => {
      invoke('write_terminal', {
        terminalId,
        data: Array.from(new TextEncoder().encode(data)),
      }).catch(console.error);
    });

    // Auto-copy: when user selects text, copy it to clipboard automatically (like iTerm2)
    term.onSelectionChange(() => {
      const selection = term.getSelection();
      if (selection) {
        navigator.clipboard.writeText(selection).catch(() => {});
      }
    });

    // Track terminal title changes (for tab names)
    term.onTitleChange((title) => {
      onTitleChangeRef.current?.(title);
    });

    // Spawn terminal. For SSH, pass structured args so SSH is the PTY root process.
    // No shell wrapper, no delayed stdin write — SSH runs directly.
    let sshArgs: string[] | null = null;
    if (initialCommand && initialCommand.startsWith('ssh ')) {
      // Parse SSH command into individual args, respecting quoted strings
      const raw = initialCommand.slice(4); // strip "ssh "
      const matches = raw.match(/(?:[^\s"]+|"[^"]*")+/g);
      if (matches) {
        sshArgs = matches.map(a => a.replace(/^"|"$/g, '')); // strip surrounding quotes
      }
    }

    invoke('spawn_terminal', { terminalId, sshArgs })
      .then(() => {
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
      resizeObserver.disconnect();
      if (resizeTimeoutRef.current) clearTimeout(resizeTimeoutRef.current);
      unlistenOutputRef.current?.();
      unlistenExitRef.current?.();
      term.dispose();
      oauthBuffer = '';
    };
  }, [terminalId, initialCommand, handleResize]); // Only re-run when terminalId changes — callbacks use refs

  return (
    <div
      ref={containerRef}
      className="h-full w-full bg-[#09090b] p-1"
      style={{ visibility: isVisible ? 'visible' : 'hidden' }}
    />
  );
}
