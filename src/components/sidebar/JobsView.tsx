import { useEffect, useState, useCallback, useRef } from 'react';
import { emit } from '@tauri-apps/api/event';
import {
  Activity,
  RefreshCw,
  XCircle,
  AlertCircle,
  CheckCircle2,
  Clock,
  Server,
  Info,
} from 'lucide-react';
import { listSSHProfiles, type SSHProfile } from '../../lib/ssh';
import { slurmCancelJob } from '../../lib/slurm';
import {
  listClusterJobs,
  readJobLogTail,
  isTerminalState,
  type ClusterJob,
} from '../../lib/watchdog';

/**
 * Jobs panel.
 *
 * Reads the scheduler directly — `squeue` for live jobs, `sacct` for finished
 * ones — only while this panel is mounted. There is no longer a watchdog daemon
 * on the login node; see src/lib/watchdog.ts for why it was removed.
 */

function stateColor(state?: string): string {
  if (!state) return 'text-muted';
  const s = state.trim().toUpperCase();
  if (s.startsWith('CANCELLED')) return 'text-muted';
  if (s === 'COMPLETED') return 'text-green-600 dark:text-green-400';
  if (isTerminalState(s)) return 'text-red-600 dark:text-red-400';
  if (s === 'RUNNING') return 'text-blue-600 dark:text-blue-400';
  if (s === 'PENDING') return 'text-yellow-600 dark:text-yellow-400';
  return 'text-secondary';
}

function fmtElapsed(job: ClusterJob): string {
  if (job.elapsed) return job.elapsed;
  if (!job.elapsed_seconds) return '';
  const s = job.elapsed_seconds;
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  return h > 0
    ? `${h}:${String(m).padStart(2, '0')}:${String(sec).padStart(2, '0')}`
    : `${m}:${String(sec).padStart(2, '0')}`;
}

export function JobsView() {
  const [profiles, setProfiles] = useState<SSHProfile[]>([]);
  const [profileId, setProfileId] = useState<string>('');
  const [jobs, setJobs] = useState<ClusterJob[]>([]);
  const [accounting, setAccounting] = useState(true);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [expanded, setExpanded] = useState<string | null>(null);
  const [logs, setLogs] = useState<Record<string, string>>({});
  // Errors are kept OUT of `logs` so a transient failure is never cached as if
  // it were log content.
  const [logErrors, setLogErrors] = useState<Record<string, string>>({});
  const [logLoading, setLogLoading] = useState<string | null>(null);
  const [userResolved, setUserResolved] = useState(true);
  const pollRef = useRef<number | null>(null);
  const logsRef = useRef<Record<string, string>>({});
  // Mirrors `expanded` for the poll callback, which must not re-fire on every
  // expand/collapse (that would restart the 30s interval).
  const expandedRef = useRef<string | null>(null);

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
      const res = await listClusterJobs(profileId);
      setJobs(res.jobs);
      setAccounting(res.accounting);
      setUserResolved(res.user_resolved);

      // Refresh the log of whatever is currently expanded. Without this the
      // pane froze at first expansion — the replacement for a `tail -f` that
      // never updated would be worse than none.
      if (expandedRef.current) {
        void loadLog(expandedRef.current, true);
      }

      // Broadcast a tick for the status-bar pill. LIVE rows only: `jobs` also
      // carries a week of sacct history, so counting all of it left one job the
      // user cancelled days ago pinning the pill red forever.
      const live = res.jobs.filter((j) => j.source === 'squeue');
      const running = live.filter((j) => j.state.toUpperCase() === 'RUNNING').length;
      const pending = live.filter((j) => j.state.toUpperCase() === 'PENDING').length;
      const failed = live.filter(
        (j) => isTerminalState(j.state) && j.state.toUpperCase() !== 'COMPLETED',
      ).length;
      emit('watchdog-tick', {
        profileId,
        total: live.length,
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
    // 30s. One `squeue` + one `sacct` per tick, and only while this panel is
    // open — scheduler polling is something HPC sites actively police, and job
    // state is minutes-granular anyway.
    pollRef.current = window.setInterval(() => refresh(), 30_000);
    return () => {
      if (pollRef.current) window.clearInterval(pollRef.current);
      // Zero the status-bar pill. Nothing else emits this event, so without a
      // clear on unmount the pill froze on the last counts for the rest of the
      // app run — showing "3 jobs" long after they finished.
      emit('watchdog-tick', {
        profileId,
        total: 0,
        running: 0,
        pending: 0,
        failed: 0,
      });
    };
  }, [profileId, refresh]);

  const cancel = async (jobId: string) => {
    try {
      await slurmCancelJob(profileId, jobId);
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  };

  /** Fetch a job's log. `force` re-reads even when we already have content. */
  const loadLog = useCallback(
    async (jobId: string, force = false) => {
      if (!profileId) return;
      // A pending array RANGE ("12345_[5-9]") is not a real job id — the backend
      // validator rejects it, so requesting a log is a guaranteed failure.
      if (!/^\d[\d_+.]*$/.test(jobId)) {
        setLogErrors((prev) => ({
          ...prev,
          [jobId]: 'Pending array tasks — no log until they start.',
        }));
        return;
      }
      // Read through a ref, not `logs` state: depending on `logs` here would
      // make `refresh` a new function on every fetch, which restarts the 30s
      // poll interval each time.
      if (!force && logsRef.current[jobId] !== undefined) return;
      setLogLoading(jobId);
      try {
        const tail = await readJobLogTail(profileId, jobId, null, 100);
        logsRef.current = { ...logsRef.current, [jobId]: tail };
        setLogs(logsRef.current);
        setLogErrors((prev) => {
          const { [jobId]: _drop, ...rest } = prev;
          return rest;
        });
      } catch (e) {
        // Never cached as content — the next poll or re-expand retries.
        setLogErrors((prev) => ({ ...prev, [jobId]: String(e) }));
      } finally {
        setLogLoading(null);
      }
    },
    [profileId],
  );

  const toggleExpand = async (jobId: string) => {
    if (expanded === jobId) {
      setExpanded(null);
      expandedRef.current = null;
      return;
    }
    setExpanded(jobId);
    expandedRef.current = jobId;
    // Always re-read for a job that is still running; its log is growing.
    const job = jobs.find((j) => j.job_id === jobId);
    await loadLog(jobId, job ? !isTerminalState(job.state) : false);
  };

  return (
    <div className="flex flex-col h-full bg-panel text-secondary">
      <div className="flex items-center gap-2 px-3 py-2 border-b border-border-default">
        <Activity className="w-3.5 h-3.5 text-muted" />
        <span className="text-[11px] font-semibold text-muted uppercase tracking-wider flex-1">
          Jobs
        </span>
        <button
          onClick={refresh}
          disabled={loading || !profileId}
          className="p-1 rounded hover:bg-hover disabled:opacity-40"
          title="Refresh"
        >
          <RefreshCw className={`w-3.5 h-3.5 ${loading ? 'animate-spin' : ''}`} />
        </button>
      </div>

      <div className="px-3 py-2 border-b border-border-default">
        <label className="flex items-center gap-2 text-xs">
          <Server className="w-3 h-3 text-muted" />
          <select
            value={profileId}
            onChange={(e) => setProfileId(e.target.value)}
            className="flex-1 bg-surface border border-border-strong rounded px-2 py-1 text-xs"
          >
            {profiles.length === 0 && <option value="">No SSH profiles</option>}
            {profiles.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name}
              </option>
            ))}
          </select>
        </label>
      </div>

      {!userResolved && profileId && (
        <div className="px-3 py-1.5 text-[10px] text-yellow-700 dark:text-yellow-400 bg-yellow-950/20 border-b border-border-default flex items-start gap-1.5">
          <AlertCircle className="w-3 h-3 mt-0.5 shrink-0" />
          <span>
            Couldn&rsquo;t determine your username on this host, so the job query was
            skipped rather than run unfiltered. Check that the account has a normal
            login shell.
          </span>
        </div>
      )}

      {!accounting && profileId && (
        <div className="px-3 py-1.5 text-[10px] text-muted bg-surface/60 border-b border-border-default flex items-start gap-1.5">
          <Info className="w-3 h-3 mt-0.5 shrink-0" />
          <span>
            This cluster has no SLURM accounting (<code>sacct</code>), so finished jobs
            disappear from the queue and leave no record to show. Set a notification
            email in this server&rsquo;s settings to be told when jobs end.
          </span>
        </div>
      )}

      {error && (
        <div className="px-3 py-1.5 text-[11px] text-red-600 dark:text-red-400 bg-red-950/30 border-b border-red-900/40 flex items-start gap-1.5">
          <AlertCircle className="w-3 h-3 mt-0.5 shrink-0" />
          <span className="break-all">{error}</span>
        </div>
      )}

      <div className="flex-1 overflow-y-auto">
        {jobs.length === 0 ? (
          <div className="px-3 py-8 text-center text-xs text-subtle">
            <Clock className="w-5 h-5 mx-auto mb-2 opacity-50" />
            {error ? (
              <>
                Couldn&rsquo;t reach the cluster.
                <div className="mt-1 text-[10px]">
                  Open an SSH terminal to this host and complete login (e.g. Duo) —
                  this panel reuses that authenticated connection.
                </div>
              </>
            ) : (
              <>
                No jobs in the queue.
                <div className="mt-1 text-[10px]">
                  Anything you submit shows up here — including jobs started outside
                  Operon. {accounting && 'Recent finished jobs are listed too.'}
                </div>
              </>
            )}
          </div>
        ) : (
          jobs.map((job) => {
            const isExpanded = expanded === job.job_id;
            const terminal = isTerminalState(job.state);
            const elapsed = fmtElapsed(job);
            return (
              <div key={job.job_id} className="border-b border-border-default/60 hover:bg-hover/40">
                <button
                  onClick={() => toggleExpand(job.job_id)}
                  className="w-full flex items-center gap-2 px-3 py-2 text-left"
                >
                  {job.state.toUpperCase() === 'COMPLETED' ? (
                    <CheckCircle2 className="w-3.5 h-3.5 text-green-600 dark:text-green-400 shrink-0" />
                  ) : terminal ? (
                    <AlertCircle className="w-3.5 h-3.5 text-red-600 dark:text-red-400 shrink-0" />
                  ) : (
                    <Clock className="w-3.5 h-3.5 text-muted shrink-0" />
                  )}
                  <div className="flex-1 min-w-0">
                    <div className="text-xs font-mono truncate">
                      {job.job_id}
                      {job.name && <span className="text-subtle ml-1.5">{job.name}</span>}
                    </div>
                    <div className={`text-[10px] ${stateColor(job.state)}`}>
                      {job.state}
                      {elapsed && <span className="ml-1 text-subtle">· {elapsed}</span>}
                      {job.partition && <span className="ml-1 text-subtle">· {job.partition}</span>}
                    </div>
                  </div>
                  {!terminal && (
                    <span
                      role="button"
                      onClick={(e) => {
                        e.stopPropagation();
                        cancel(job.job_id);
                      }}
                      className="p-1 rounded hover:bg-elevated"
                      title="Cancel job (scancel)"
                    >
                      <XCircle className="w-3 h-3 text-muted" />
                    </span>
                  )}
                </button>
                {isExpanded && (
                  <div className="px-3 pb-2 space-y-1">
                    <div className="text-[10px] text-muted space-y-0.5">
                      {job.reason && (
                        <div>
                          <span className="text-subtle">reason </span>
                          {job.reason}
                        </div>
                      )}
                      {job.exit_code && (
                        <div>
                          <span className="text-subtle">exit </span>
                          {job.exit_code}
                        </div>
                      )}
                      {job.ended_at && (
                        <div>
                          {/* Cluster-local wall clock, exactly as sacct reported
                              it — not reinterpreted in the viewer's timezone. */}
                          <span className="text-subtle">ended </span>
                          {job.ended_at.replace('T', ' ')}
                        </div>
                      )}
                      <div className="text-subtle">via {job.source}</div>
                    </div>
                    <pre className="text-[10px] text-muted font-mono whitespace-pre-wrap break-all max-h-48 overflow-y-auto bg-surface/50 rounded px-2 py-1">
                      {logLoading === job.job_id
                        ? 'reading log…'
                        : logErrors[job.job_id]
                          ? `(${logErrors[job.job_id]})`
                          : logs[job.job_id] || '(no log output)'}
                    </pre>
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
