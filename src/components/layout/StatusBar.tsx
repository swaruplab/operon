import { useEffect, useRef, useState } from 'react';
import { GitBranch, Loader, AlertCircle, Activity, Server } from "lucide-react";
import { listen } from '@tauri-apps/api/event';
import { Tooltip } from "../ui/Tooltip";
import { getActiveClient } from '../../lib/lspClient';
import {
  getSshDiagnostics,
  resetSshDiagnostics,
  type SshDiagnostics,
} from '../../lib/ssh';

interface WatchdogTick {
  profileId: string;
  watchdogRunning: boolean;
  total: number;
  running: number;
  pending: number;
  failed: number;
}

interface StatusBarProps {
  sidebarVisible: boolean;
  terminalVisible: boolean;
  chatVisible: boolean;
  activeLanguageId?: string;
}

export function StatusBar({ sidebarVisible, terminalVisible, chatVisible, activeLanguageId }: StatusBarProps) {
  const [lspStatus, setLspStatus] = useState<'idle' | 'starting' | 'running' | 'error'>('idle');
  const [lspServerName, setLspServerName] = useState<string>('');
  const [watchdog, setWatchdog] = useState<WatchdogTick | null>(null);
  const [remote, setRemote] = useState<{ profileId: string; profileName: string } | null>(null);

  useEffect(() => {
    const unlistenP = listen<WatchdogTick>('watchdog-tick', (e) => {
      setWatchdog(e.payload);
    });
    return () => {
      unlistenP.then((u) => u());
    };
  }, []);

  // Track remote connection so we can show a global Disconnect chip in the status bar.
  useEffect(() => {
    const unlistenConnect = listen<{ profileId?: string; profileName?: string }>('open-ssh-terminal', (e) => {
      const { profileId, profileName } = e.payload;
      if (profileId && profileName) setRemote({ profileId, profileName });
    });
    const unlistenDisconnect = listen<{ profileId: string }>('disconnect-remote', (e) => {
      setRemote((prev) => (prev?.profileId === e.payload.profileId ? null : prev));
    });
    return () => {
      unlistenConnect.then((u) => u());
      unlistenDisconnect.then((u) => u());
    };
  }, []);

  useEffect(() => {
    if (!activeLanguageId || activeLanguageId === 'plaintext') {
      setLspStatus('idle');
      setLspServerName('');
      return;
    }

    // Poll for LSP client status
    const interval = setInterval(() => {
      const client = getActiveClient(activeLanguageId);
      if (client && client.isRunning()) {
        setLspStatus('running');
        setLspServerName(activeLanguageId);
      } else {
        setLspStatus('idle');
        setLspServerName('');
      }
    }, 2000);

    return () => clearInterval(interval);
  }, [activeLanguageId]);

  const getLspIndicator = () => {
    if (lspStatus === 'running') {
      return (
        <Tooltip label="Language server is running" position="top">
          <div className="flex items-center gap-1.5 text-green-600 dark:text-green-400">
            <span className="w-2 h-2 bg-green-400 rounded-full"></span>
            <span>{lspServerName} (LSP)</span>
          </div>
        </Tooltip>
      );
    }
    if (lspStatus === 'starting') {
      return (
        <Tooltip label="Language server is starting up" position="top">
          <div className="flex items-center gap-1.5 text-yellow-600 dark:text-yellow-400">
            <Loader className="w-3 h-3 animate-spin" />
            <span>Starting LSP...</span>
          </div>
        </Tooltip>
      );
    }
    if (lspStatus === 'error') {
      return (
        <Tooltip label="Language server encountered an error" position="top">
          <div className="flex items-center gap-1.5 text-red-600 dark:text-red-400">
            <AlertCircle className="w-3 h-3" />
            <span>LSP Error</span>
          </div>
        </Tooltip>
      );
    }
    return activeLanguageId ? (
      <Tooltip label="Detected file language" position="top">
        <span>{activeLanguageId}</span>
      </Tooltip>
    ) : null;
  };

  return (
    <div className="h-6 flex items-center justify-between px-3 bg-panel border-t border-border-default text-[11px] text-muted shrink-0">
      {/* Left */}
      <div className="flex items-center gap-3">
        <Tooltip label="Current Git branch" position="top">
          <div className="flex items-center gap-1">
            <GitBranch className="w-3 h-3" />
            <span>main</span>
          </div>
        </Tooltip>
        <Tooltip label="Cursor position in active editor" position="top">
          <span>Ln 1, Col 1</span>
        </Tooltip>
      </div>

      {/* Right */}
      <div className="flex items-center gap-3">
        {remote && (
          <SshHealthPill profileId={remote.profileId} profileName={remote.profileName} />
        )}
        {watchdog && watchdog.total > 0 && (
          <Tooltip
            label={`Watchdog ${watchdog.watchdogRunning ? 'running' : 'idle'} — ${watchdog.total} job(s): ${watchdog.running}R / ${watchdog.pending}P / ${watchdog.failed}F`}
            position="top"
          >
            <div
              className={`flex items-center gap-1 px-1.5 rounded ${
                watchdog.failed > 0
                  ? 'text-red-600 dark:text-red-400'
                  : watchdog.running > 0
                    ? 'text-blue-600 dark:text-blue-400'
                    : 'text-secondary'
              }`}
            >
              <Activity className="w-3 h-3" />
              <span>
                {watchdog.total} job{watchdog.total !== 1 ? 's' : ''}
              </span>
            </div>
          </Tooltip>
        )}
        <Tooltip label="File encoding" position="top">
          <span>UTF-8</span>
        </Tooltip>
        {getLspIndicator()}
        <div className="flex items-center gap-1.5">
          <Tooltip label="Toggle sidebar" shortcut={"\u2318B"} position="top">
            <span className={`cursor-default ${sidebarVisible ? "text-secondary" : ""}`}>Sidebar</span>
          </Tooltip>
          <Tooltip label="Toggle terminal" shortcut={"\u2318J"} position="top">
            <span className={`cursor-default ${terminalVisible ? "text-secondary" : ""}`}>Terminal</span>
          </Tooltip>
          <Tooltip label="Toggle chat panel" shortcut={"\u2318L"} position="top">
            <span className={`cursor-default ${chatVisible ? "text-secondary" : ""}`}>Chat</span>
          </Tooltip>
        </div>
      </div>
    </div>
  );
}

interface SshHealthPillProps {
  profileId: string;
  profileName: string;
}

function SshHealthPill({ profileId, profileName }: SshHealthPillProps) {
  const [diag, setDiag] = useState<SshDiagnostics | null>(null);
  const [open, setOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const popoverRef = useRef<HTMLDivElement | null>(null);

  // Poll every 5s while a remote connection is active. Re-runs on profileId
  // change so switching hosts immediately repoints the pill.
  useEffect(() => {
    let alive = true;
    const tick = async () => {
      try {
        const d = await getSshDiagnostics(profileId);
        if (alive) setDiag(d);
      } catch {
        if (alive) setDiag(null);
      }
    };
    tick();
    const id = setInterval(tick, 5000);
    return () => {
      alive = false;
      clearInterval(id);
    };
  }, [profileId]);

  // Close the popover on outside click.
  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (popoverRef.current && !popoverRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    window.addEventListener('mousedown', onDown);
    return () => window.removeEventListener('mousedown', onDown);
  }, [open]);

  const rtt = diag?.rtt_ms ?? null;
  const inCooldown = !!diag?.in_cooldown;
  const colorClass =
    inCooldown || (rtt !== null && rtt > 500)
      ? 'bg-red-500/10 text-red-600 dark:text-red-400'
      : rtt !== null && rtt >= 100
        ? 'bg-amber-500/10 text-amber-600 dark:text-amber-400'
        : 'bg-green-500/10 text-green-600 dark:text-green-400';

  const rttLabel = rtt === null ? 'n/a' : `${rtt}ms`;
  const channelsLabel = diag ? `${diag.channels_alive}/${diag.pool_max}` : '–/–';

  const handleReset = async () => {
    if (busy) return;
    setBusy(true);
    try {
      await resetSshDiagnostics(profileId);
      const d = await getSshDiagnostics(profileId);
      setDiag(d);
    } catch {
      /* surfaced via last_error on next poll */
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="relative">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        className={`flex items-center gap-1 px-1.5 rounded ${colorClass} hover:brightness-110`}
        title={`${profileName} — ${inCooldown ? 'in cooldown' : 'connected'}`}
      >
        <Server className="w-3 h-3 pointer-events-none" />
        <span>{profileName}</span>
        <span className="opacity-70">{`· ${rttLabel} · ${channelsLabel}`}</span>
      </button>
      {open && (
        <div
          ref={popoverRef}
          className="absolute right-0 bottom-7 z-40 w-72 rounded border border-border-default bg-panel text-[11px] text-primary shadow-lg p-3"
        >
          <div className="text-xs font-medium mb-2 flex items-center justify-between">
            <span>{profileName}</span>
            <span className="text-muted">{rttLabel}</span>
          </div>
          <table className="w-full">
            <tbody className="[&_td]:py-0.5">
              <tr>
                <td className="text-muted pr-2">RTT</td>
                <td className="text-right">{rttLabel}</td>
              </tr>
              <tr>
                <td className="text-muted pr-2">Channels alive</td>
                <td className="text-right">
                  {diag ? `${diag.channels_alive} / ${diag.channels_total} (max ${diag.pool_max})` : '—'}
                </td>
              </tr>
              <tr>
                <td className="text-muted pr-2">Total calls</td>
                <td className="text-right">{diag?.total_calls ?? '—'}</td>
              </tr>
              <tr>
                <td className="text-muted pr-2">Cache hits</td>
                <td className="text-right">{diag?.cache_hits ?? '—'}</td>
              </tr>
              <tr>
                <td className="text-muted pr-2">Respawns</td>
                <td className="text-right">{diag?.respawns ?? '—'}</td>
              </tr>
              <tr>
                <td className="text-muted pr-2">Cooldown</td>
                <td className="text-right">{inCooldown ? 'yes' : 'no'}</td>
              </tr>
            </tbody>
          </table>
          {diag?.last_error && (
            <div className="mt-2 text-[10.5px] text-red-500 break-words">
              <span className="font-medium">Last error: </span>
              {diag.last_error}
            </div>
          )}
          <div className="flex items-center justify-end gap-2 mt-3">
            <button
              type="button"
              disabled={busy}
              onClick={handleReset}
              className="px-2 py-0.5 rounded border border-border-default hover:bg-zinc-800 disabled:opacity-50"
            >
              Reset stats
            </button>
            <button
              type="button"
              disabled={busy}
              onClick={handleReset}
              className="px-2 py-0.5 rounded bg-blue-600/80 text-white hover:bg-blue-600 disabled:opacity-50"
            >
              Reconnect
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
