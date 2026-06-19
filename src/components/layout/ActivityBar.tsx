import { useEffect, useRef, useState } from "react";
import {
  Files, Search, MonitorSmartphone, Settings, BookOpen, HelpCircle, GitBranch, Blocks, Activity,
  OctagonX, Unplug, AlertTriangle, Loader2, Trash2, Send,
} from "lucide-react";
import { listen } from "@tauri-apps/api/event";
import { Tooltip } from "../ui/Tooltip";
import { disconnectRemote } from "../../lib/disconnect";
import { scanRemoteFootprint, teardownRemoteFootprint, type RemoteFootprint } from "../../lib/teardown";

interface ActivityBarProps {
  activeView: string;
  onViewChange: (view: string) => void;
}

const items = [
  { id: "files", icon: Files, label: "Explorer", shortcut: undefined, description: "Browse project files" },
  { id: "search", icon: Search, label: "Search", shortcut: undefined, description: "Search across files" },
  { id: "git", icon: GitBranch, label: "Git & GitHub", shortcut: undefined, description: "Version control and GitHub" },
  { id: "ssh", icon: MonitorSmartphone, label: "Remote SSH", shortcut: undefined, description: "Connect to remote servers" },
  { id: "jobs", icon: Activity, label: "HPC Jobs", shortcut: undefined, description: "Watch HPC jobs and auto-resubmit on failure" },
  { id: "submit", icon: Send, label: "Submit Job", shortcut: undefined, description: "Submit SLURM/PBS jobs to a remote scheduler" },
  { id: "extensions", icon: Blocks, label: "Extensions", shortcut: undefined, description: "Manage extensions" },
  { id: "protocols", icon: BookOpen, label: "Protocols", shortcut: undefined, description: "Analysis protocols" },
  { id: "help", icon: HelpCircle, label: "Help", shortcut: undefined, description: "Documentation and guides" },
  { id: "settings", icon: Settings, label: "Settings", shortcut: "⌘,", description: "App preferences" },
];

interface ConnectedRemote { profileId: string; profileName: string }

export function ActivityBar({ activeView, onViewChange }: ActivityBarProps) {
  const [remote, setRemote] = useState<ConnectedRemote | null>(null);
  const [menuOpen, setMenuOpen] = useState(false);
  const [dialogOpen, setDialogOpen] = useState(false);
  const stopRef = useRef<HTMLDivElement>(null);

  // Track the connected remote (same signals StatusBar listens to).
  useEffect(() => {
    const unlistenConnect = listen<{ profileId?: string; profileName?: string }>("open-ssh-terminal", (e) => {
      const { profileId, profileName } = e.payload;
      if (profileId && profileName) setRemote({ profileId, profileName });
    });
    const unlistenDisconnect = listen<{ profileId: string }>("disconnect-remote", (e) => {
      setRemote((prev) => (prev?.profileId === e.payload.profileId ? null : prev));
    });
    return () => {
      unlistenConnect.then((u) => u());
      unlistenDisconnect.then((u) => u());
    };
  }, []);

  // Close the little menu on outside click / Escape.
  useEffect(() => {
    if (!menuOpen) return;
    const onDown = (e: MouseEvent) => {
      if (stopRef.current && !stopRef.current.contains(e.target as Node)) setMenuOpen(false);
    };
    const onKey = (e: KeyboardEvent) => { if (e.key === "Escape") setMenuOpen(false); };
    window.addEventListener("mousedown", onDown);
    window.addEventListener("keydown", onKey);
    return () => { window.removeEventListener("mousedown", onDown); window.removeEventListener("keydown", onKey); };
  }, [menuOpen]);

  const connected = !!remote;

  return (
    <div className="w-12 flex flex-col items-center py-2 gap-0.5 bg-panel border-r border-border-default shrink-0 relative z-10">
      {items.map((item) => {
        const Icon = item.icon;
        const isActive = activeView === item.id;
        return (
          <Tooltip key={item.id} label={item.description} shortcut={item.shortcut} position="right" delay={80}>
            <button
              onClick={() => onViewChange(item.id)}
              className={`relative w-10 h-10 flex items-center justify-center rounded-md transition-colors ${
                isActive ? "text-primary" : "text-muted hover:text-secondary"
              }`}
            >
              {isActive && <div className="absolute left-0 top-2 bottom-2 w-[2px] bg-blue-500 rounded-r" />}
              <Icon className="w-5 h-5" strokeWidth={1.5} />
            </button>
          </Tooltip>
        );
      })}

      {/* Stop sign — pushed to the bottom */}
      <div ref={stopRef} className="mt-auto relative">
        <Tooltip
          label={connected ? `End / disconnect remote session (${remote!.profileName})` : "No remote session connected"}
          position="right"
          delay={80}
        >
          <button
            onClick={() => connected && setMenuOpen((o) => !o)}
            className={`relative w-10 h-10 flex items-center justify-center rounded-md transition-colors ${
              connected ? "text-red-500 hover:text-red-700 dark:hover:text-red-600 hover:bg-red-500/10" : "text-subtle cursor-default"
            }`}
            aria-label="End remote session"
          >
            <OctagonX className="w-5 h-5" strokeWidth={1.5} />
          </button>
        </Tooltip>

        {menuOpen && connected && (
          <div className="absolute left-full bottom-0 ml-1 w-64 rounded-md border border-border-strong bg-panel shadow-xl py-1 text-sm z-50">
            <div className="px-3 py-1.5 text-[11px] uppercase tracking-wide text-muted">{remote!.profileName}</div>
            <button
              onClick={() => { setMenuOpen(false); disconnectRemote(remote!.profileId); }}
              className="w-full flex items-start gap-2.5 px-3 py-2 text-left text-primary hover:bg-hover"
            >
              <Unplug className="w-4 h-4 mt-0.5 shrink-0 text-yellow-600 dark:text-yellow-400" />
              <span>
                Disconnect <span className="text-secondary">(keep remote session)</span>
                <span className="block text-[11px] text-muted">Drops the SSH link. tmux + agents keep running so you can reconnect.</span>
              </span>
            </button>
            <button
              onClick={() => { setMenuOpen(false); setDialogOpen(true); }}
              className="w-full flex items-start gap-2.5 px-3 py-2 text-left text-red-700 dark:text-red-300 hover:bg-red-500/10"
            >
              <OctagonX className="w-4 h-4 mt-0.5 shrink-0 text-red-500" />
              <span>
                End session &amp; clean up everything
                <span className="block text-[11px] text-red-600 dark:text-red-400/70">Kills the SSH link, Operon's tmux session, leftover helpers &amp; files. Never runs scancel.</span>
              </span>
            </button>
          </div>
        )}
      </div>

      {dialogOpen && remote && (
        <EndSessionDialog
          remote={remote}
          onClose={() => setDialogOpen(false)}
          onDone={() => { setDialogOpen(false); disconnectRemote(remote.profileId); }}
        />
      )}
    </div>
  );
}

function EndSessionDialog({
  remote, onClose, onDone,
}: {
  remote: ConnectedRemote;
  onClose: () => void;
  onDone: () => void;
}) {
  const [footprint, setFootprint] = useState<RemoteFootprint | null>(null);
  const [scanError, setScanError] = useState<string | null>(null);
  const [killTmux, setKillTmux] = useState(true);
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    scanRemoteFootprint(remote.profileId)
      .then((fp) => {
        if (cancelled) return;
        setFootprint(fp);
        // If a SLURM allocation is living inside the operon tmux pane, default to
        // NOT killing the tmux (which would release it) — make it an explicit opt-in.
        setKillTmux(!fp.slurm_in_pane);
      })
      .catch((e) => { if (!cancelled) setScanError(String(e)); });
    return () => { cancelled = true; };
  }, [remote.profileId]);

  const run = async () => {
    setBusy(true);
    try {
      const msg = await teardownRemoteFootprint(remote.profileId, killTmux);
      setResult(msg);
    } catch (e) {
      setResult(`Teardown failed: ${e}`);
    } finally {
      setBusy(false);
    }
  };

  const tmuxNames = footprint?.tmux_sessions ?? [];
  // squeue output is "%i %j %T %D %m %R" — STATE is the 3rd field.
  // Only RUNNING jobs can be the in-pane allocation; PENDING/queued jobs live
  // entirely in the SLURM controller and are unaffected by killing the tmux pane.
  const allJobs = footprint?.running_jobs ?? [];
  const runningJobs = allJobs.filter((j) => j.trim().split(/\s+/)[2] === "RUNNING");
  const queuedCount = allJobs.length - runningJobs.length;

  return (
    <div className="fixed inset-0 z-[100] flex items-center justify-center bg-black/50" onMouseDown={onClose}>
      <div
        className="w-[28rem] max-w-[90vw] rounded-lg border border-border-strong bg-panel shadow-2xl p-5 text-sm text-primary"
        onMouseDown={(e) => e.stopPropagation()}
      >
        <div className="flex items-center gap-2 mb-3">
          <OctagonX className="w-5 h-5 text-red-500" />
          <h2 className="text-base font-medium">End session &amp; clean up — {remote.profileName}</h2>
        </div>

        {result ? (
          <>
            <p className="text-secondary whitespace-pre-wrap">{result}</p>
            <div className="mt-5 flex justify-end">
              <button onClick={onDone} className="px-3 py-1.5 rounded-md bg-elevated hover:bg-elevated">Close</button>
            </div>
          </>
        ) : (
          <>
            {!footprint && !scanError && (
              <div className="flex items-center gap-2 text-secondary py-2">
                <Loader2 className="w-4 h-4 animate-spin" /> Checking what's running on the server…
              </div>
            )}
            {scanError && (
              <p className="text-yellow-600 dark:text-yellow-400 py-2">Couldn't scan the server ({scanError}). You can still proceed.</p>
            )}

            {footprint && (
              <div className="space-y-2.5 mb-1">
                <p className="text-secondary">This tears down everything Operon spawned on this server:</p>
                <ul className="space-y-1.5 text-secondary">
                  <li className="flex items-start gap-2"><Trash2 className="w-3.5 h-3.5 mt-0.5 text-muted shrink-0" /> Close the SSH connection &amp; ControlMaster.</li>
                  <li className="flex items-start gap-2"><Trash2 className="w-3.5 h-3.5 mt-0.5 text-muted shrink-0" /> Kill leftover Operon log-streamers{footprint.helpers.length ? ` (${footprint.helpers.length} found)` : ""} &amp; remove <code>.operon-*</code> scratch files.</li>
                  <li className="flex items-start gap-2">
                    <Trash2 className="w-3.5 h-3.5 mt-0.5 text-muted shrink-0" />
                    {tmuxNames.length
                      ? <span>Operon tmux session{tmuxNames.length > 1 ? "s" : ""}: <code className="text-primary">{tmuxNames.join(", ")}</code></span>
                      : <span>No Operon tmux session found.</span>}
                  </li>
                </ul>
                <p className="text-[11px] text-muted">Submitted SLURM jobs are never touched.</p>

                {footprint.slurm_in_pane && (
                  <div className="rounded-md border border-yellow-600/40 bg-yellow-500/10 p-2.5 text-yellow-800 dark:text-yellow-200">
                    <div className="flex items-start gap-2">
                      <AlertTriangle className="w-4 h-4 mt-0.5 shrink-0 text-yellow-600 dark:text-yellow-400" />
                      <div>
                        <div className="text-yellow-900 dark:text-yellow-100">
                          One running SLURM allocation is open in this tmux pane. Killing the tmux session will end <em>only that allocation</em>.
                        </div>
                        <div className="mt-1 text-yellow-800 dark:text-yellow-200/90">
                          <code>sbatch</code> jobs in your queue keep running on their own nodes — Operon never calls <code>scancel</code>.
                        </div>
                        {runningJobs.length > 0 && (
                          <div className="mt-2">
                            <div className="text-[11px] text-yellow-700 dark:text-yellow-300/70 mb-0.5">Currently running on this account:</div>
                            <div className="font-mono text-[11px] text-yellow-700 dark:text-yellow-300/90">
                              {runningJobs.slice(0, 4).map((j, i) => <div key={i}>{j}</div>)}
                            </div>
                          </div>
                        )}
                        {queuedCount > 0 && (
                          <div className="mt-1.5 text-[11px] text-yellow-700 dark:text-yellow-300/70">
                            {queuedCount} queued/pending job{queuedCount === 1 ? "" : "s"} not shown — these are independent and will keep waiting in the SLURM queue.
                          </div>
                        )}
                        <label className="mt-2 flex items-center gap-2 text-yellow-900 dark:text-yellow-100">
                          <input type="checkbox" checked={killTmux} onChange={(e) => setKillTmux(e.target.checked)} />
                          Also kill the tmux session (ends the running allocation)
                        </label>
                      </div>
                    </div>
                  </div>
                )}

                {!footprint.slurm_in_pane && tmuxNames.length > 0 && (
                  <label className="flex items-center gap-2 text-secondary">
                    <input type="checkbox" checked={killTmux} onChange={(e) => setKillTmux(e.target.checked)} />
                    Also kill the tmux session
                  </label>
                )}
              </div>
            )}

            <div className="mt-5 flex justify-end gap-2">
              <button onClick={onClose} disabled={busy} className="px-3 py-1.5 rounded-md bg-surface hover:bg-elevated disabled:opacity-50">Cancel</button>
              <button
                onClick={run}
                disabled={busy || (!footprint && !scanError)}
                className="px-3 py-1.5 rounded-md bg-red-600 hover:bg-red-500 text-white disabled:opacity-50 flex items-center gap-1.5"
              >
                {busy && <Loader2 className="w-3.5 h-3.5 animate-spin" />}
                {killTmux ? "End everything" : "Clean up (keep tmux)"}
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
