import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  Send,
  RefreshCw,
  Server,
  Cpu,
  X,
  AlertCircle,
  CheckCircle2,
  Clock,
  ShieldCheck,
} from 'lucide-react';
import { listSSHProfiles, type SSHProfile } from '../../lib/ssh';
import {
  buildSbatchPreview,
  slurmCancelJob,
  slurmQueryJobs,
  slurmSubmitJob,
  type SlurmJob,
  type SlurmJobSpec,
} from '../../lib/slurm';
import { reviewCode, type ReviewResult } from '../../lib/review';
import { getSettings } from '../../lib/settings';
import { ReviewFindings } from '../review/ReviewFindings';

interface JobSubmissionPanelProps {
  initialProfileId?: string | null;
}

interface FormState {
  partition: string;
  account: string;
  nodes: string;
  cores: string;
  memoryGb: string;
  timeHms: string;
  gpuType: string;
  gpuCount: string;
  jobName: string;
  outputDir: string;
  command: string;
}

const EMPTY_FORM: FormState = {
  partition: '',
  account: '',
  nodes: '1',
  cores: '1',
  memoryGb: '4',
  timeHms: '01:00:00',
  gpuType: '',
  gpuCount: '',
  jobName: '',
  outputDir: '',
  command: '',
};

function stateColor(state: string): string {
  const s = state.toUpperCase();
  if (s === 'R' || s === 'RUNNING') return 'text-blue-600 dark:text-blue-400';
  if (s === 'PD' || s === 'PENDING') return 'text-yellow-600 dark:text-yellow-400';
  if (s === 'CD' || s === 'COMPLETED') return 'text-green-600 dark:text-green-400';
  if (s === 'CG' || s === 'COMPLETING') return 'text-secondary';
  if (s === 'F' || s === 'FAILED' || s === 'TO' || s === 'TIMEOUT') return 'text-red-600 dark:text-red-400';
  if (s === 'CA' || s === 'CANCELLED') return 'text-muted';
  return 'text-muted';
}

function toSpec(form: FormState, profileId: string): SlurmJobSpec {
  const num = (s: string) => {
    const n = parseInt(s, 10);
    return Number.isFinite(n) && n > 0 ? n : undefined;
  };
  const text = (s: string) => (s.trim() ? s.trim() : undefined);
  return {
    profile_id: profileId,
    partition: text(form.partition),
    account: text(form.account),
    nodes: num(form.nodes),
    cores: num(form.cores),
    memory_gb: num(form.memoryGb),
    time_hms: text(form.timeHms),
    gpu_type: text(form.gpuType),
    gpu_count: num(form.gpuCount),
    job_name: text(form.jobName),
    output_dir: text(form.outputDir),
    command: form.command,
  };
}

export function JobSubmissionPanel({ initialProfileId }: JobSubmissionPanelProps) {
  const [profiles, setProfiles] = useState<SSHProfile[]>([]);
  const [profileId, setProfileId] = useState<string>('');
  const [form, setForm] = useState<FormState>(EMPTY_FORM);
  const [showGpu, setShowGpu] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [submitMsg, setSubmitMsg] = useState<{ ok: boolean; text: string } | null>(null);
  const [jobs, setJobs] = useState<SlurmJob[]>([]);
  const [jobsLoading, setJobsLoading] = useState(false);
  const [jobsError, setJobsError] = useState<string | null>(null);
  const [cancellingId, setCancellingId] = useState<string | null>(null);
  const pollRef = useRef<number | null>(null);

  // Load profiles + pick default
  useEffect(() => {
    listSSHProfiles()
      .then((ps) => {
        setProfiles(ps);
        if (ps.length) {
          const initial = initialProfileId && ps.find((p) => p.id === initialProfileId)
            ? initialProfileId
            : ps[0].id;
          setProfileId(initial);
        }
      })
      .catch((e) => setJobsError(String(e)));
  }, [initialProfileId]);

  const selectedProfile = useMemo(
    () => profiles.find((p) => p.id === profileId) ?? null,
    [profiles, profileId],
  );

  // Pre-fill account/partition from server_config the first time a profile is selected
  useEffect(() => {
    if (!selectedProfile) return;
    setForm((prev) => ({
      ...prev,
      account: prev.account || selectedProfile.server_config.slurm_account || '',
      partition: prev.partition || selectedProfile.server_config.slurm_partition || '',
      gpuType: prev.gpuType || selectedProfile.server_config.slurm_gpu_type || '',
      outputDir: prev.outputDir || selectedProfile.server_config.work_dir || '',
    }));
  }, [selectedProfile]);

  const fetchJobs = useCallback(async () => {
    if (!profileId) return;
    setJobsLoading(true);
    setJobsError(null);
    try {
      const rows = await slurmQueryJobs(profileId);
      setJobs(rows);
    } catch (e) {
      setJobsError(String(e));
    } finally {
      setJobsLoading(false);
    }
  }, [profileId]);

  // Initial fetch + poll every 10s while there are user jobs
  useEffect(() => {
    if (!profileId) return;
    fetchJobs();
    if (pollRef.current) window.clearInterval(pollRef.current);
    pollRef.current = window.setInterval(() => {
      // Only poll if the panel has live data — keep the queue fresh while running.
      fetchJobs();
    }, 10_000);
    return () => {
      if (pollRef.current) window.clearInterval(pollRef.current);
      pollRef.current = null;
    };
  }, [profileId, fetchJobs]);

  const spec = useMemo(() => toSpec(form, profileId), [form, profileId]);
  const preview = useMemo(() => buildSbatchPreview(spec), [spec]);
  const canSubmit = !!profileId && form.command.trim().length > 0 && !submitting;

  // ── Pre-submit review ───────────────────────────────────────────────────
  // The highest-leverage moment in the app: a few seconds of Sonnet against
  // hours of queued cluster time. Strictly advisory — findings pause the submit
  // once, then the button becomes "Submit anyway". A reviewer that is broken,
  // slow, or missing must never stand between the user and their cluster.
  const [reviewing, setReviewing] = useState(false);
  const [review, setReview] = useState<ReviewResult | null>(null);
  const [reviewError, setReviewError] = useState<string | null>(null);
  /** The exact script text the current review applies to (staleness guard). */
  const [reviewedText, setReviewedText] = useState<string | null>(null);
  const [autoReview, setAutoReview] = useState(true);

  useEffect(() => {
    getSettings()
      .then((s) => setAutoReview(s.reviewer_enabled !== false && s.reviewer_auto_sbatch !== false))
      .catch(() => {});
  }, []);

  // Editing the form changes the script — any existing review is now stale.
  useEffect(() => {
    if (reviewedText !== null && reviewedText !== preview) {
      setReview(null);
      setReviewError(null);
      setReviewedText(null);
    }
  }, [preview, reviewedText]);

  const runReview = useCallback(async (): Promise<ReviewResult | null> => {
    setReviewing(true);
    setReview(null);
    setReviewError(null);
    try {
      const r = await reviewCode(preview, 'sbatch', form.jobName?.trim() || 'job.sbatch');
      setReview(r);
      setReviewedText(preview);
      return r;
    } catch (e) {
      setReviewError(String(e));
      // Mark as "reviewed" even on failure so a broken reviewer can't trap the
      // user in a loop where Submit keeps retrying a review that never works.
      setReviewedText(preview);
      return null;
    } finally {
      setReviewing(false);
    }
  }, [preview, form.jobName]);

  const hasFindings = !!review && review.findings.length > 0;
  const reviewIsCurrent = reviewedText === preview;

  const submit = async () => {
    if (!canSubmit || reviewing) return;

    // Gate once: if auto-review is on and this exact script hasn't been reviewed,
    // review first and hold the submit if anything came back.
    if (autoReview && !reviewIsCurrent) {
      const r = await runReview();
      if (r && r.findings.length > 0) return; // hold — user can now "Submit anyway"
    }

    setSubmitting(true);
    setSubmitMsg(null);
    try {
      const jobId = await slurmSubmitJob(spec);
      setSubmitMsg({ ok: true, text: `Submitted job ${jobId}` });
      // Refresh queue shortly after — give the scheduler a moment to register the job.
      window.setTimeout(fetchJobs, 1200);
    } catch (e) {
      setSubmitMsg({ ok: false, text: String(e) });
    } finally {
      setSubmitting(false);
    }
  };

  const cancel = async (jobId: string) => {
    setCancellingId(jobId);
    try {
      await slurmCancelJob(profileId, jobId);
      await fetchJobs();
    } catch (e) {
      setJobsError(String(e));
    } finally {
      setCancellingId(null);
    }
  };

  const update = <K extends keyof FormState>(key: K, val: FormState[K]) =>
    setForm((prev) => ({ ...prev, [key]: val }));

  const partitionSuggestions = useMemo(() => {
    if (!selectedProfile) return [] as string[];
    const raw = selectedProfile.server_config.slurm_partitions;
    if (!raw) return [];
    return raw.split(/[,\s]+/).filter(Boolean);
  }, [selectedProfile]);

  return (
    <div className="flex flex-col h-full bg-panel text-secondary">
      <div className="flex items-center gap-2 px-3 py-2 border-b border-border-default">
        <Cpu className="w-3.5 h-3.5 text-muted" />
        <span className="text-[11px] font-semibold text-muted uppercase tracking-wider flex-1">
          Submit Job
        </span>
        <button
          onClick={fetchJobs}
          disabled={!profileId || jobsLoading}
          className="p-1 rounded hover:bg-hover disabled:opacity-40"
          title="Refresh queue"
        >
          <RefreshCw className={`w-3.5 h-3.5 ${jobsLoading ? 'animate-spin' : ''}`} />
        </button>
      </div>

      <div className="flex-1 overflow-y-auto">
        {/* Server selector */}
        <div className="px-3 py-2 border-b border-border-default">
          <label className="flex items-center gap-2 text-xs">
            <Server className="w-3 h-3 text-muted shrink-0" />
            <select
              value={profileId}
              onChange={(e) => setProfileId(e.target.value)}
              className="flex-1 bg-surface border border-border-strong rounded px-2 py-1 text-xs outline-none focus:border-blue-500"
            >
              {profiles.length === 0 && <option value="">No SSH profiles</option>}
              {profiles.map((p) => (
                <option key={p.id} value={p.id}>{p.name}</option>
              ))}
            </select>
          </label>
        </div>

        {/* Form */}
        <div className="px-3 py-3 space-y-2.5 border-b border-border-default">
          <Field label="Job name" placeholder="my_analysis">
            <input
              type="text"
              value={form.jobName}
              onChange={(e) => update('jobName', e.target.value)}
              className={inputCls}
              placeholder="my_analysis"
            />
          </Field>

          <div className="grid grid-cols-2 gap-2">
            <Field label="Partition">
              <input
                type="text"
                list="operon-slurm-partitions"
                value={form.partition}
                onChange={(e) => update('partition', e.target.value)}
                className={inputCls}
                placeholder="standard"
              />
              {partitionSuggestions.length > 0 && (
                <datalist id="operon-slurm-partitions">
                  {partitionSuggestions.map((p) => <option key={p} value={p} />)}
                </datalist>
              )}
            </Field>
            <Field label="Account">
              <input
                type="text"
                value={form.account}
                onChange={(e) => update('account', e.target.value)}
                className={inputCls}
                placeholder="my_lab"
              />
            </Field>
          </div>

          <div className="grid grid-cols-3 gap-2">
            <Field label="Nodes">
              <input
                type="number"
                min={1}
                value={form.nodes}
                onChange={(e) => update('nodes', e.target.value)}
                className={inputCls}
              />
            </Field>
            <Field label="Cores">
              <input
                type="number"
                min={1}
                value={form.cores}
                onChange={(e) => update('cores', e.target.value)}
                className={inputCls}
              />
            </Field>
            <Field label="Mem (GB)">
              <input
                type="number"
                min={1}
                value={form.memoryGb}
                onChange={(e) => update('memoryGb', e.target.value)}
                className={inputCls}
              />
            </Field>
          </div>

          <Field label="Walltime">
            <input
              type="text"
              value={form.timeHms}
              onChange={(e) => update('timeHms', e.target.value)}
              className={`${inputCls} font-mono`}
              placeholder="HH:MM:SS"
            />
          </Field>

          {/* GPU collapsible */}
          <div>
            <button
              type="button"
              onClick={() => setShowGpu((v) => !v)}
              className="text-[11px] text-blue-700 dark:text-blue-300 hover:text-blue-200"
            >
              {showGpu ? '▾' : '▸'} Request GPU
            </button>
            {showGpu && (
              <div className="grid grid-cols-2 gap-2 mt-1.5">
                <Field label="GPU type">
                  <input
                    type="text"
                    value={form.gpuType}
                    onChange={(e) => update('gpuType', e.target.value)}
                    className={inputCls}
                    placeholder="a100"
                  />
                </Field>
                <Field label="GPU count">
                  <input
                    type="number"
                    min={0}
                    value={form.gpuCount}
                    onChange={(e) => update('gpuCount', e.target.value)}
                    className={inputCls}
                    placeholder="1"
                  />
                </Field>
              </div>
            )}
          </div>

          <Field label="Output directory">
            <input
              type="text"
              value={form.outputDir}
              onChange={(e) => update('outputDir', e.target.value)}
              className={`${inputCls} font-mono`}
              placeholder="/scratch/$USER/jobs"
            />
          </Field>

          <Field label="Command">
            <textarea
              value={form.command}
              onChange={(e) => update('command', e.target.value)}
              rows={5}
              spellCheck={false}
              className={`${inputCls} font-mono resize-y min-h-[80px]`}
              placeholder={'module load python\\npython train.py'}
            />
          </Field>

          {submitMsg && (
            <div
              className={`flex items-start gap-1.5 text-[11px] px-2 py-1.5 rounded border ${
                submitMsg.ok
                  ? 'border-green-700/40 bg-green-700/10 text-green-600 dark:text-green-300'
                  : 'border-red-700/40 bg-red-700/10 text-red-700 dark:text-red-300'
              }`}
            >
              {submitMsg.ok ? (
                <CheckCircle2 className="w-3.5 h-3.5 mt-px shrink-0" />
              ) : (
                <AlertCircle className="w-3.5 h-3.5 mt-px shrink-0" />
              )}
              <span className="break-words">{submitMsg.text}</span>
            </div>
          )}

          {/* Advisory second opinion, before this costs hours of queue time. */}
          {(reviewing || review || reviewError) && (
            <ReviewFindings
              result={review}
              loading={reviewing}
              error={reviewError}
              target={form.jobName?.trim() || 'job.sbatch'}
            />
          )}

          <div className="flex gap-1.5">
            <button
              onClick={runReview}
              disabled={!profileId || !form.command.trim() || reviewing || submitting}
              title="Review this sbatch script before submitting"
              className="flex items-center justify-center gap-1.5 px-3 py-1.5 bg-surface hover:bg-hover disabled:opacity-50 text-secondary text-xs rounded border border-border-default transition-colors shrink-0"
            >
              <ShieldCheck className="w-3.5 h-3.5 pointer-events-none" />
              {reviewing ? 'Reviewing…' : 'Review'}
            </button>
            <button
              onClick={submit}
              disabled={!canSubmit || reviewing}
              className={`flex-1 flex items-center justify-center gap-1.5 px-3 py-1.5 disabled:bg-surface disabled:text-subtle text-white text-xs rounded transition-colors ${
                hasFindings && reviewIsCurrent
                  ? 'bg-amber-600 hover:bg-amber-500'
                  : 'bg-blue-600 hover:bg-blue-500'
              }`}
            >
              <Send className="w-3.5 h-3.5 pointer-events-none" />
              {submitting
                ? 'Submitting…'
                : reviewing
                  ? 'Reviewing…'
                  : hasFindings && reviewIsCurrent
                    ? 'Submit anyway'
                    : 'Submit job'}
            </button>
          </div>
        </div>

        {/* sbatch preview */}
        <div className="px-3 py-2 border-b border-border-default">
          <div className="text-[10px] text-muted uppercase tracking-wider mb-1">sbatch preview</div>
          <pre className="text-[11px] font-mono whitespace-pre-wrap bg-canvas/60 border border-border-default rounded p-2 text-secondary max-h-48 overflow-auto">
            {preview}
          </pre>
        </div>

        {/* Job queue */}
        <div className="px-3 py-2">
          <div className="flex items-center gap-2 mb-1.5">
            <Clock className="w-3 h-3 text-muted" />
            <span className="text-[10px] text-muted uppercase tracking-wider flex-1">Your queue</span>
            <span className="text-[10px] text-subtle">
              {jobs.length} {jobs.length === 1 ? 'job' : 'jobs'}
            </span>
          </div>
          {jobsError && (
            <div className="text-[11px] text-red-600 dark:text-red-400 mb-1.5 break-words">{jobsError}</div>
          )}
          {jobs.length === 0 ? (
            <div className="text-[11px] text-subtle py-2 text-center">
              {jobsLoading ? 'Loading…' : 'No jobs in queue'}
            </div>
          ) : (
            <div className="space-y-1">
              {jobs.map((j) => (
                <div
                  key={j.job_id}
                  className="flex items-center gap-2 px-2 py-1.5 bg-surface rounded text-[11px] border border-border-default"
                >
                  <span className="font-mono text-secondary shrink-0 w-16 truncate" title={j.job_id}>
                    {j.job_id}
                  </span>
                  <span className={`shrink-0 w-12 font-medium ${stateColor(j.state)}`}>
                    {j.state}
                  </span>
                  <span className="flex-1 truncate text-secondary" title={j.name}>{j.name}</span>
                  <span className="shrink-0 text-muted font-mono" title="Walltime used">{j.time}</span>
                  <span className="shrink-0 text-muted truncate max-w-[5rem]" title={j.partition}>
                    {j.partition}
                  </span>
                  <button
                    onClick={() => cancel(j.job_id)}
                    disabled={cancellingId === j.job_id}
                    className="shrink-0 p-0.5 rounded text-muted hover:text-red-700 dark:hover:text-red-400 hover:bg-elevated disabled:opacity-40"
                    title={`Cancel job ${j.job_id}`}
                  >
                    <X className="w-3 h-3" />
                  </button>
                </div>
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

const inputCls =
  'w-full bg-surface border border-border-strong rounded px-2 py-1 text-xs text-primary outline-none focus:border-blue-500 placeholder:text-subtle';

function Field({
  label,
  placeholder,
  children,
}: {
  label: string;
  placeholder?: string;
  children: React.ReactNode;
}) {
  return (
    <label className="block">
      <span className="block text-[10px] uppercase tracking-wider text-muted mb-0.5" title={placeholder}>
        {label}
      </span>
      {children}
    </label>
  );
}
