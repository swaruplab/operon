import { useEffect, useState, useCallback, useRef } from 'react';
import { emit } from '@tauri-apps/api/event';
import {
  Activity,
  RefreshCw,
  Play,
  Square,
  Trash2,
  Download,
  AlertCircle,
  CheckCircle2,
  Clock,
  Server,
} from 'lucide-react';
import { listSSHProfiles, type SSHProfile } from '../../lib/ssh';
import {
  detectScheduler,
  installWatchdog,
  startWatchdog,
  stopWatchdog,
  watchdogStatus,
  listWatchedJobs,
  unregisterWatchedJob,
  readJobEvents,
  getJobPolicy,
  setJobPolicy,
  type WatchedJob,
  type WatchdogStatus,
  type JobEvent,
  type JobPolicy,
} from '../../lib/watchdog';

const TERMINAL_STATES = new Set([
  'COMPLETED',
  'FAILED',
  'CANCELLED',
  'TIMEOUT',
  'OUT_OF_MEMORY',
  'NODE_FAIL',
  'BOOT_FAIL',
  'DEADLINE',
  'PREEMPTED',
]);

function stateColor(state?: string): string {
  if (!state) return 'text-zinc-500';
  if (state.startsWith('CANCELLED')) return 'text-zinc-500';
  if (TERMINAL_STATES.has(state)) {
    if (state === 'COMPLETED') return 'text-green-400';
    return 'text-red-400';
  }
  if (state === 'RUNNING') return 'text-blue-400';
  if (state === 'PENDING') return 'text-yellow-400';
  return 'text-zinc-400';
}

export function JobsView() {
  const [profiles, setProfiles] = useState<SSHProfile[]>([]);
  const [profileId, setProfileId] = useState<string>('');
  const [status, setStatus] = useState<WatchdogStatus | null>(null);
  const [jobs, setJobs] = useState<WatchedJob[]>([]);
  const [policy, setPolicy] = useState<JobPolicy | null>(null);
  const [loading, setLoading] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [jobStates, setJobStates] = useState<Record<string, string>>({});
  const [expanded, setExpanded] = useState<string | null>(null);
  const [events, setEvents] = useState<Record<string, JobEvent[]>>({});
  const pollRef = useRef<number | null>(null);

  useEffect(() => {
    listSSHProfiles()
      .then((ps) => {
        setProfiles(ps);
        if (ps.length && !profileId) setProfileId(ps[0].id);
      })
      .catch((e) => setError(String(e)));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const refresh = useCallback(async () => {
    if (!profileId) return;
    setLoading(true);
    setError(null);
    try {
      const [s, j, p] = await Promise.all([
        watchdogStatus(profileId),
        listWatchedJobs(profileId),
        getJobPolicy(profileId).catch(() => null),
      ]);
      setStatus(s);
      setJobs(j);
      if (p) setPolicy(p);

      // derive latest state per job from event log
      const states: Record<string, string> = {};
      await Promise.all(
        j.map(async (job) => {
          try {
            const evs = await readJobEvents(profileId, job.job_id);
            const last = [...evs].reverse().find((e) => e.state);
            if (last?.state) states[job.job_id] = last.state;
          } catch {
            /* ignore */
          }
        }),
      );
      setJobStates(states);

      // Broadcast a tick so the status bar can surface an aggregate pill.
      const running = Object.values(states).filter((st) => st === 'RUNNING').length;
      const pending = Object.values(states).filter((st) => st === 'PENDING').length;
      const failed = Object.values(states).filter(
        (st) => st && TERMINAL_STATES.has(st) && st !== 'COMPLETED',
      ).length;
      emit('watchdog-tick', {
        profileId,
        watchdogRunning: s.running,
        total: j.length,
        running,
        pending,
        failed,
      });
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, [profileId]);

  useEffect(() => {
    refresh();
    if (pollRef.current) window.clearInterval(pollRef.current);
    pollRef.current = window.setInterval(() => refresh(), 15_000);
    return () => {
      if (pollRef.current) window.clearInterval(pollRef.current);
    };
  }, [profileId, refresh]);

  const install = async () => {
    if (!profileId) return;
    setBusy(true);
    setError(null);
    try {
      await installWatchdog(profileId);
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const start = async () => {
    if (!profileId) return;
    setBusy(true);
    setError(null);
    try {
      await startWatchdog(profileId);
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const stop = async () => {
    if (!profileId) return;
    setBusy(true);
    setError(null);
    try {
      await stopWatchdog(profileId);
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const unregister = async (jobId: string) => {
    try {
      await unregisterWatchedJob(profileId, jobId);
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  };

  const toggleExpand = async (jobId: string) => {
    if (expanded === jobId) {
      setExpanded(null);
      return;
    }
    setExpanded(jobId);
    try {
      const evs = await readJobEvents(profileId, jobId);
      setEvents((prev) => ({ ...prev, [jobId]: evs }));
    } catch (e) {
      setError(String(e));
    }
  };

  const updatePolicy = async (patch: Partial<JobPolicy>) => {
    if (!policy) return;
    const next = { ...policy, ...patch };
    setPolicy(next);
    try {
      await setJobPolicy(profileId, next);
    } catch (e) {
      setError(String(e));
    }
  };

  const detect = async () => {
    setBusy(true);
    try {
      const s = await detectScheduler(profileId);
      setError(`Detected: ${s}`);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="flex flex-col h-full bg-zinc-900 text-zinc-300">
      <div className="flex items-center gap-2 px-3 py-2 border-b border-zinc-800">
        <Activity className="w-3.5 h-3.5 text-zinc-500" />
        <span className="text-[11px] font-semibold text-zinc-500 uppercase tracking-wider flex-1">
          Jobs
        </span>
        <button
          onClick={refresh}
          disabled={loading || !profileId}
          className="p-1 rounded hover:bg-zinc-800 disabled:opacity-40"
          title="Refresh"
        >
          <RefreshCw className={`w-3.5 h-3.5 ${loading ? 'animate-spin' : ''}`} />
        </button>
      </div>

      <div className="px-3 py-2 border-b border-zinc-800 space-y-2">
        <label className="flex items-center gap-2 text-xs">
          <Server className="w-3 h-3 text-zinc-500" />
          <select
            value={profileId}
            onChange={(e) => setProfileId(e.target.value)}
            className="flex-1 bg-zinc-800 border border-zinc-700 rounded px-2 py-1 text-xs"
          >
            {profiles.length === 0 && <option value="">No SSH profiles</option>}
            {profiles.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name}
              </option>
            ))}
          </select>
        </label>

        {status && (
          <div className="text-[11px] text-zinc-500 flex items-center gap-3">
            <span className={status.installed ? 'text-zinc-400' : 'text-zinc-600'}>
              {status.installed ? 'installed' : 'not installed'}
            </span>
            <span className={status.running ? 'text-green-400' : 'text-zinc-600'}>
              {status.running ? 'running' : 'stopped'}
            </span>
            <span className="text-zinc-500">{status.scheduler ?? '—'}</span>
          </div>
        )}

        <div className="flex gap-1">
          {!status?.installed && (
            <button
              onClick={install}
              disabled={busy || !profileId}
              className="flex-1 text-[11px] px-2 py-1 rounded bg-blue-600 hover:bg-blue-500 disabled:opacity-40"
            >
              <Download className="inline w-3 h-3 mr-1" />
              Install
            </button>
          )}
          {status?.installed && !status.running && (
            <button
              onClick={start}
              disabled={busy}
              className="flex-1 text-[11px] px-2 py-1 rounded bg-green-700 hover:bg-green-600 disabled:opacity-40"
            >
              <Play className="inline w-3 h-3 mr-1" />
              Start
            </button>
          )}
          {status?.running && (
            <button
              onClick={stop}
              disabled={busy}
              className="flex-1 text-[11px] px-2 py-1 rounded bg-zinc-700 hover:bg-zinc-600 disabled:opacity-40"
            >
              <Square className="inline w-3 h-3 mr-1" />
              Stop
            </button>
          )}
          <button
            onClick={detect}
            disabled={busy || !profileId}
            className="text-[11px] px-2 py-1 rounded bg-zinc-800 hover:bg-zinc-700 disabled:opacity-40"
            title="Detect scheduler"
          >
            Detect
          </button>
        </div>

        {policy && (
          <div className="text-[11px] text-zinc-500 space-y-1 pt-1">
            <div className="flex items-center gap-2">
              <span className="flex-1">Max retries</span>
              <input
                type="number"
                min={0}
                max={10}
                value={policy.max_retries}
                onChange={(e) => updatePolicy({ max_retries: Number(e.target.value) })}
                className="w-12 bg-zinc-800 border border-zinc-700 rounded px-1 py-0.5 text-right"
              />
            </div>
            <div className="flex items-center gap-2">
              <span className="flex-1">Timeout × walltime</span>
              <input
                type="number"
                step={0.1}
                min={1}
                max={5}
                value={policy.on_timeout_walltime_mult}
                onChange={(e) =>
                  updatePolicy({ on_timeout_walltime_mult: Number(e.target.value) })
                }
                className="w-12 bg-zinc-800 border border-zinc-700 rounded px-1 py-0.5 text-right"
              />
            </div>
            <div className="flex items-center gap-2">
              <span className="flex-1">OOM × mem</span>
              <input
                type="number"
                step={0.1}
                min={1}
                max={8}
                value={policy.on_oom_mem_mult}
                onChange={(e) => updatePolicy({ on_oom_mem_mult: Number(e.target.value) })}
                className="w-12 bg-zinc-800 border border-zinc-700 rounded px-1 py-0.5 text-right"
              />
            </div>
          </div>
        )}
      </div>

      {error && (
        <div className="px-3 py-1.5 text-[11px] text-red-400 bg-red-950/30 border-b border-red-900/40 flex items-start gap-1.5">
          <AlertCircle className="w-3 h-3 mt-0.5 shrink-0" />
          <span className="break-all">{error}</span>
        </div>
      )}

      <div className="flex-1 overflow-y-auto">
        {jobs.length === 0 ? (
          <div className="px-3 py-8 text-center text-xs text-zinc-600">
            <Clock className="w-5 h-5 mx-auto mb-2 opacity-50" />
            No jobs being watched.
            <div className="mt-1 text-[10px]">
              Submit an sbatch in any terminal — Operon auto-registers it.
            </div>
          </div>
        ) : (
          jobs.map((job) => {
            const state = jobStates[job.job_id];
            const isExpanded = expanded === job.job_id;
            return (
              <div
                key={job.job_id}
                className="border-b border-zinc-800/60 hover:bg-zinc-800/40"
              >
                <button
                  onClick={() => toggleExpand(job.job_id)}
                  className="w-full flex items-center gap-2 px-3 py-2 text-left"
                >
                  {state === 'COMPLETED' ? (
                    <CheckCircle2 className="w-3.5 h-3.5 text-green-400 shrink-0" />
                  ) : state && TERMINAL_STATES.has(state) ? (
                    <AlertCircle className="w-3.5 h-3.5 text-red-400 shrink-0" />
                  ) : (
                    <Clock className="w-3.5 h-3.5 text-zinc-500 shrink-0" />
                  )}
                  <div className="flex-1 min-w-0">
                    <div className="text-xs font-mono">{job.job_id}</div>
                    <div className={`text-[10px] ${stateColor(state)}`}>
                      {state ?? 'polling…'}
                      {job.retries_left > 0 && (
                        <span className="ml-1 text-zinc-600">· {job.retries_left} retries</span>
                      )}
                    </div>
                  </div>
                  <span
                    role="button"
                    onClick={(e) => {
                      e.stopPropagation();
                      unregister(job.job_id);
                    }}
                    className="p-1 rounded hover:bg-zinc-700"
                    title="Stop watching"
                  >
                    <Trash2 className="w-3 h-3 text-zinc-500" />
                  </span>
                </button>
                {isExpanded && (
                  <div className="px-3 pb-2 text-[10px] text-zinc-500 font-mono space-y-0.5 max-h-48 overflow-y-auto">
                    {(events[job.job_id] ?? []).slice(-30).map((ev, i) => (
                      <div key={i} className="truncate">
                        <span className="text-zinc-600">
                          {new Date(ev.ts).toLocaleTimeString()}
                        </span>{' '}
                        <span className={stateColor(ev.state)}>{ev.type}</span>
                        {ev.state && ` ${ev.state}`}
                        {ev.action && ev.action !== 'none' && ` → ${ev.action}`}
                        {ev.resubmitted_as && ` #${ev.resubmitted_as}`}
                      </div>
                    ))}
                    {(events[job.job_id] ?? []).length === 0 && (
                      <div className="text-zinc-700 italic">no events yet</div>
                    )}
                  </div>
                )}
              </div>
            );
          })
        )}
      </div>
    </div>
  );
}
