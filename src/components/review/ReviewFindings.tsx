import { AlertOctagon, AlertTriangle, CheckCircle2, Loader2, ShieldAlert } from 'lucide-react';
import type { ReviewResult } from '../../lib/review';

interface ReviewFindingsProps {
  result: ReviewResult | null;
  loading: boolean;
  /** Reviewer failed (not installed, timed out, unparseable). Advisory only —
   *  the caller must still let the user proceed. */
  error: string | null;
  /** Shown while loading, e.g. "job.sbatch". */
  target?: string;
}

export function ReviewFindings({ result, loading, error, target }: ReviewFindingsProps) {
  if (loading) {
    return (
      <div className="flex items-center gap-2 px-3 py-2 text-xs text-muted">
        <Loader2 className="w-3.5 h-3.5 animate-spin shrink-0" />
        <span>Reviewing{target ? ` ${target}` : ''}&hellip;</span>
      </div>
    );
  }

  if (error) {
    // Never dressed up as a failure of the user's code — the reviewer is the
    // thing that broke, and it must not stand between them and their work.
    return (
      <div className="flex items-start gap-2 px-3 py-2 text-xs rounded border border-amber-500/40 bg-amber-500/10 text-amber-700 dark:text-amber-300">
        <ShieldAlert className="w-3.5 h-3.5 mt-0.5 shrink-0" />
        <div>
          <div className="font-medium">Review unavailable</div>
          <div className="text-[11px] opacity-80 break-words">{error}</div>
        </div>
      </div>
    );
  }

  if (!result) return null;

  if (result.clean) {
    return (
      <div className="flex items-start gap-2 px-3 py-2 text-xs rounded border border-green-600/40 bg-green-600/10 text-green-700 dark:text-green-400">
        <CheckCircle2 className="w-3.5 h-3.5 mt-0.5 shrink-0" />
        <div>
          <div className="font-medium">No known pitfalls detected</div>
          {/* Deliberate wording: a passed checklist is not a proof of correctness. */}
          <div className="text-[11px] opacity-80">
            Checklist passed ({result.model}). This is not a proof of correctness.
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-1.5">
      {result.findings.map((f, i) => {
        const blocking = f.severity === 'blocking';
        return (
          <div
            key={`${f.title}-${i}`}
            className={`rounded border px-3 py-2 text-xs ${
              blocking
                ? 'border-red-600/50 bg-red-600/10'
                : 'border-amber-500/50 bg-amber-500/10'
            }`}
          >
            <div className="flex items-start gap-2">
              {blocking ? (
                <AlertOctagon className="w-3.5 h-3.5 mt-0.5 shrink-0 text-red-600 dark:text-red-400" />
              ) : (
                <AlertTriangle className="w-3.5 h-3.5 mt-0.5 shrink-0 text-amber-600 dark:text-amber-400" />
              )}
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-1.5 flex-wrap">
                  <span
                    className={`text-[9px] font-semibold uppercase tracking-wider px-1 py-0.5 rounded ${
                      blocking
                        ? 'bg-red-600/20 text-red-700 dark:text-red-300'
                        : 'bg-amber-500/20 text-amber-700 dark:text-amber-300'
                    }`}
                  >
                    {f.severity}
                  </span>
                  {f.line != null && (
                    <span className="text-[10px] font-mono text-muted">line {f.line}</span>
                  )}
                  {f.category && (
                    <span className="text-[10px] font-mono text-subtle">{f.category}</span>
                  )}
                </div>
                <div className="mt-1 font-medium text-primary break-words">{f.title}</div>
                {f.why_wrong && (
                  <div className="mt-1 text-secondary break-words">{f.why_wrong}</div>
                )}
                {f.fix && (
                  <div className="mt-1 text-[11px] text-secondary break-words">
                    <span className="text-muted">Fix: </span>
                    {f.fix}
                  </div>
                )}
              </div>
            </div>
          </div>
        );
      })}
      <div className="px-1 text-[10px] text-subtle">
        Advisory second opinion from {result.model}. It can be wrong — check the cited line.
      </div>
    </div>
  );
}
