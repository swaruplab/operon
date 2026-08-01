import { useState, useCallback, useEffect, useRef } from 'react';
import { Plus, X, Terminal as TerminalIcon } from 'lucide-react';
import { listen, emit } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import { TerminalInstance } from './TerminalInstance';
import { Tooltip } from '../ui/Tooltip';
import { saveSessionState, type SessionTerminal } from '../../lib/session';

interface TerminalTab {
  id: string;
  title: string;
  type: 'local' | 'ssh';
  /** Command to run once the shell is ready (non-SSH only, e.g. `claude login`). */
  initialCommand?: string;
  /** What kind of command `initialCommand` is. Set explicitly by the emitter
   *  rather than sniffed from the string: the login command carries an absolute
   *  binary path, so a `/^claude login/` pattern would no longer match it and
   *  would silently drop TERM=dumb and the auth-env clearing prefix. */
  commandKind?: 'claude-login';
  /** Structured SSH argument vector — passed to spawn_terminal verbatim. */
  sshArgs?: string[];
  /** SSH profile id — present for SSH tabs, enables HPC watchdog auto-register. */
  sshProfileId?: string;
  /** Wall-clock ms when this tab was added. Used to gate the SSH
   *  early-exit watchdog: a non-zero exit within ~15 s of spawn is treated
   *  as a connection failure (auth/network), not a user-initiated logout. */
  spawnedAt: number;
  exited: boolean;
}

/** Window of time after SSH tab spawn during which a non-zero exit is treated
 *  as connection failure (vs user-initiated `exit`/Ctrl-D). */
const SSH_EARLY_EXIT_MS = 15_000;

function describeSshExit(exitCode: number | null): string {
  if (exitCode === null) return 'SSH connection terminated unexpectedly';
  if (exitCode === 255) return 'SSH connection failed (auth, network, or unknown host)';
  return `SSH connection failed (exit code ${exitCode})`;
}

export interface PendingLoginTab {
  terminalId: string;
  title: string;
  command: string;
  kind?: 'claude-login';
}

interface TerminalAreaProps {
  /** A login terminal AppShell was asked to open. Delivered as a prop rather
   *  than listened for here because this component is unmounted whenever the
   *  terminal panel is collapsed — the event would simply be dropped, and the
   *  Claude panel would still claim a login was running. */
  pendingLoginTab?: PendingLoginTab | null;
  onPendingLoginConsumed?: () => void;
}

export function TerminalArea({ pendingLoginTab, onPendingLoginConsumed }: TerminalAreaProps = {}) {
  const [tabs, setTabs] = useState<TerminalTab[]>(() => {
    const id = crypto.randomUUID();
    return [{ id, title: 'Terminal', type: 'local', exited: false, spawnedAt: Date.now() }];
  });
  const [activeTab, setActiveTab] = useState<string>(() => tabs[0].id);

  // Listen for SSH terminal open events from the sidebar
  useEffect(() => {
    const unlisten = listen<{ terminalId: string; title: string; sshArgs?: string[]; profileId?: string }>('open-ssh-terminal', (event) => {
      const { terminalId, title, sshArgs, profileId } = event.payload;
      const newTab: TerminalTab = {
        id: terminalId,
        title,
        type: 'ssh',
        sshArgs,
        sshProfileId: profileId,
        spawnedAt: Date.now(),
        exited: false,
      };
      // Dedupe on id, exactly as the login-tab path does. `listen()` resolves
      // asynchronously, so a mount→cleanup→mount cycle (StrictMode in dev, or a
      // fast collapse/expand of the terminal panel, which unmounts this
      // component) can transiently leave TWO live listeners: the old one is only
      // detached once its `unlisten` promise resolves. An event landing in that
      // window ran this handler twice and appended the SAME id twice — React
      // then rendered two <TerminalInstance> on one terminalId, each with its
      // own onData writing to the same PTY, so every keystroke reached the shell
      // twice ("clear" -> "cclleeaarr").
      setTabs((prev) => (prev.some((t) => t.id === terminalId) ? prev : [...prev, newTab]));
      setActiveTab(terminalId);
    });

    return () => { unlisten.then((u) => u()); };
  }, []);

  // Listen for disconnect-remote events: kill all SSH tabs for this profile
  // so the user can cleanly switch to a different server.
  useEffect(() => {
    const unlisten = listen<{ profileId: string }>('disconnect-remote', (event) => {
      const { profileId } = event.payload;
      setTabs((prev) => {
        const toClose = prev.filter((t) => t.type === 'ssh' && t.sshProfileId === profileId);
        toClose.forEach((t) => {
          invoke('kill_terminal', { terminalId: t.id }).catch(() => {});
        });
        const remaining = prev.filter((t) => !toClose.includes(t));
        // If the active tab was closed, focus a remaining tab (or none)
        setActiveTab((curr) =>
          toClose.some((t) => t.id === curr)
            ? remaining[remaining.length - 1]?.id ?? curr
            : curr,
        );
        return remaining;
      });
    });
    return () => { unlisten.then((u) => u()); };
  }, []);

  // Open the login terminal AppShell handed us. Runs on prop change rather than
  // on a Tauri event so it still fires when this component mounts *after* the
  // request (terminal panel collapsed at the time the user clicked Sign in).
  useEffect(() => {
    if (!pendingLoginTab) return;
    const { terminalId, title, command, kind } = pendingLoginTab;
    setTabs((prev) =>
      prev.some((t) => t.id === terminalId)
        ? prev
        : [
            ...prev,
            {
              id: terminalId,
              title,
              type: 'local',
              initialCommand: command,
              commandKind: kind,
              spawnedAt: Date.now(),
              exited: false,
            },
          ]
    );
    setActiveTab(terminalId);
    onPendingLoginConsumed?.();
  }, [pendingLoginTab, onPendingLoginConsumed]);

  // Emit the active local terminal ID so the file explorer can use it for cd
  useEffect(() => {
    const activeTabObj = tabs.find((t) => t.id === activeTab);
    if (activeTabObj && activeTabObj.type === 'local' && !activeTabObj.exited) {
      emit('local-terminal-active', { terminalId: activeTabObj.id });
    }
  }, [activeTab, tabs]);

  // --- CWD tracking for terminals (terminal → explorer sync) ---
  const lastCwd = useRef<string>('');
  const lastRemoteCwd = useRef<string>('');

  // Per-terminal cwd map for session persistence — kept off React state to
  // avoid re-rendering every TerminalInstance on every OSC 7 emission.
  const cwdByTerminal = useRef<Map<string, string>>(new Map());
  const persistTimer = useRef<number | null>(null);
  const schedulePersist = useCallback(() => {
    if (persistTimer.current !== null) {
      window.clearTimeout(persistTimer.current);
    }
    persistTimer.current = window.setTimeout(() => {
      const terminalTabs: SessionTerminal[] = tabs
        .filter((t) => t.type === 'local' && !t.exited)
        .map((t) => ({
          id: t.id,
          cwd: cwdByTerminal.current.get(t.id) ?? '',
          is_remote: false,
        }));
      saveSessionState({ terminal_tabs: terminalTabs }).catch(() => {});
    }, 500);
  }, [tabs]);

  useEffect(() => {
    schedulePersist();
    return () => {
      if (persistTimer.current !== null) window.clearTimeout(persistTimer.current);
    };
  }, [tabs, schedulePersist]);

  const handleCwdChange = useCallback((id: string, cwd: string) => {
    const tab = tabs.find((t) => t.id === id);
    if (!tab || !cwd) return;
    const prev = cwdByTerminal.current.get(id);
    if (prev !== cwd) {
      cwdByTerminal.current.set(id, cwd);
      schedulePersist();
    }
    if (tab.type === 'local') {
      if (cwd !== lastCwd.current) {
        lastCwd.current = cwd;
        emit('terminal-cwd-changed', { terminalId: id, cwd });
      }
    } else {
      if (cwd !== lastRemoteCwd.current) {
        lastRemoteCwd.current = cwd;
        emit('remote-terminal-cwd-changed', { terminalId: id, cwd });
      }
    }
  }, [tabs, schedulePersist]);

  // Fallback: poll via lsof for shells that don't emit OSC 7
  useEffect(() => {
    const activeTabObj = tabs.find((t) => t.id === activeTab);
    if (!activeTabObj || activeTabObj.type !== 'local' || activeTabObj.exited) {
      return;
    }

    const pollCwd = async () => {
      try {
        const cwd = await invoke<string>('get_terminal_cwd', { terminalId: activeTabObj.id });
        handleCwdChange(activeTabObj.id, cwd);
      } catch {
        // Terminal may have exited or CWD detection not supported
      }
    };

    // Poll every 3 seconds as fallback (OSC 7 is the primary mechanism)
    const interval = setInterval(pollCwd, 3000);
    pollCwd();

    return () => clearInterval(interval);
  }, [activeTab, tabs, handleCwdChange]);

  const createTab = useCallback((type: 'local' | 'ssh' = 'local') => {
    const id = crypto.randomUUID();
    const newTab: TerminalTab = {
      id,
      title: type === 'local' ? 'Terminal' : 'SSH',
      type,
      spawnedAt: Date.now(),
      exited: false,
    };
    setTabs((prev) => [...prev, newTab]);
    setActiveTab(id);
  }, []);

  const closeTab = useCallback(
    (id: string) => {
      // Explicitly kill the backend terminal process when user closes the tab
      invoke('kill_terminal', { terminalId: id }).catch(console.error);
      setTabs((prev) => {
        const filtered = prev.filter((t) => t.id !== id);
        if (activeTab === id && filtered.length > 0) {
          setActiveTab(filtered[filtered.length - 1].id);
        }
        return filtered;
      });
    },
    [activeTab],
  );

  const handleTitleChange = useCallback((id: string, title: string) => {
    setTabs((prev) =>
      prev.map((t) => (t.id === id ? { ...t, title: title || 'Terminal' } : t)),
    );
  }, []);

  // SSH early-exit watchdog: if an SSH-spawned terminal exits with a non-zero
  // status within SSH_EARLY_EXIT_MS of spawn, treat it as a connection failure
  // (auth, network, host key — anything that kills `ssh` before the user could
  // realistically type `exit`). We then:
  //   1. Auto-close the empty dead tab (no value in keeping it around).
  //   2. Emit `disconnect-remote` so the Sidebar clears `sshConnection` and
  //      the SSHView green dot reverts off.
  //   3. Dispatch the `ssh-spawn-failed` window event so SSHView can flip its
  //      CONNECT button to "FAILED · RETRY" with the exit code in the tooltip.
  // Exits AFTER the window are treated as user-initiated (Ctrl-D, `exit`,
  // logout) and just leave the tab struck-through, as before.
  const handleExit = useCallback((id: string, exitCode: number | null) => {
    let dyingTab: TerminalTab | undefined;
    setTabs((prev) => {
      dyingTab = prev.find((t) => t.id === id);
      return prev.map((t) => (t.id === id ? { ...t, exited: true } : t));
    });

    if (!dyingTab || dyingTab.type !== 'ssh' || !dyingTab.sshProfileId) return;

    const aliveMs = Date.now() - dyingTab.spawnedAt;
    const isFailure = exitCode === null || exitCode !== 0;
    if (aliveMs > SSH_EARLY_EXIT_MS || !isFailure) return;

    const failedProfileId = dyingTab.sshProfileId;
    const message = describeSshExit(exitCode);

    // Auto-close the dead tab so the user isn't left with a struck-through
    // empty terminal that they have to manually close.
    invoke('kill_terminal', { terminalId: id }).catch(() => {});
    setTabs((prev) => {
      const filtered = prev.filter((t) => t.id !== id);
      setActiveTab((curr) =>
        curr === id ? filtered[filtered.length - 1]?.id ?? curr : curr,
      );
      return filtered;
    });

    // Tell the Sidebar to drop its `sshConnection` (which is what drives
    // `connectedProfileId` in SSHView — the green dot / "connected" badge).
    emit('disconnect-remote', { profileId: failedProfileId }).catch(() => {});

    // Surface the failure to SSHView so the CONNECT button flips to red
    // FAILED · RETRY with the exit code in the tooltip. `window.dispatchEvent`
    // matches the existing pattern used by ExtensionsView → Sidebar
    // (`open-tool-panel`) for cross-component DOM signaling.
    window.dispatchEvent(
      new CustomEvent('ssh-spawn-failed', {
        detail: { profileId: failedProfileId, exitCode, message },
      }),
    );
  }, []);

  // Defense in depth: whatever put them there, a duplicated id must never reach
  // the render below. Two <TerminalInstance> sharing a terminalId means two
  // onData handlers writing to one PTY (every keystroke sent twice) and two
  // pty-output listeners, and React would silently warn about duplicate keys
  // rather than fail. First occurrence wins.
  const uniqueTabs = tabs.filter((t, i) => tabs.findIndex((o) => o.id === t.id) === i);

  // If all tabs closed, show empty state
  if (tabs.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center h-full bg-canvas text-muted">
        <TerminalIcon className="w-8 h-8 mb-2 opacity-40" />
        <p className="text-xs">No terminals open</p>
        <button
          onClick={() => createTab()}
          className="mt-2 px-3 py-1 text-xs rounded bg-surface hover:bg-elevated text-secondary transition-colors"
        >
          New Terminal
        </button>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full bg-canvas">
      {/* Tab bar */}
      <div className="flex items-center h-[33px] bg-panel border-b border-border-default shrink-0">

        <div className="flex items-center gap-0.5 px-1 flex-1 overflow-x-auto">
          {uniqueTabs.map((tab) => (
            <button
              key={tab.id}
              onClick={() => setActiveTab(tab.id)}
              className={`
                group flex items-center gap-1.5 px-2.5 py-1 rounded text-xs transition-colors
                ${
                  activeTab === tab.id
                    ? 'bg-surface text-primary'
                    : 'text-muted hover:text-secondary hover:bg-hover/50'
                }
              `}
            >
              <TerminalIcon className="w-3 h-3" />
              <span className={tab.exited ? 'line-through opacity-50' : ''}>
                {tab.title}
              </span>
              <span
                onClick={(e) => {
                  e.stopPropagation();
                  closeTab(tab.id);
                }}
                className="p-0.5 rounded hover:bg-elevated opacity-0 group-hover:opacity-100"
              >
                <X className="w-2.5 h-2.5" />
              </span>
            </button>
          ))}
        </div>

        <div className="flex items-center gap-0.5 mr-1">
          <Tooltip label="New terminal" position="top">
            <button
              onClick={() => createTab()}
              className="p-1.5 rounded hover:bg-hover text-muted hover:text-secondary transition-colors"
            >
              <Plus className="w-3.5 h-3.5" />
            </button>
          </Tooltip>
        </div>
      </div>

      {/* Terminal instances — all rendered, only active visible */}
      <div className="flex-1 relative">
        {uniqueTabs.map((tab) => (
          <div
            key={tab.id}
            className="absolute inset-0"
            style={{ zIndex: activeTab === tab.id ? 1 : 0 }}
          >
            <TerminalInstance
              terminalId={tab.id}
              isVisible={activeTab === tab.id}
              initialCommand={tab.initialCommand}
              commandKind={tab.commandKind}
              sshArgs={tab.sshArgs}
              sshProfileId={tab.sshProfileId}
              onTitleChange={(title) => handleTitleChange(tab.id, title)}
              onExit={(exitCode) => handleExit(tab.id, exitCode)}
              onCwdChange={(cwd) => handleCwdChange(tab.id, cwd)}
            />
          </div>
        ))}
      </div>
    </div>
  );
}
