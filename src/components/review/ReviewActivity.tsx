import { useState } from 'react';
import { ShieldCheck, ShieldAlert, ShieldQuestion, ChevronDown, ChevronRight } from 'lucide-react';
import type { ReviewEvent } from '../../lib/review';

interface ReviewActivityProps {
  events: ReviewEvent[];
}

/**
 * Visible proof that the Sonnet-5 pre-submit reviewer ran on the agent's
 * sbatch submissions. A PreToolUse hook does the review off-stream, so without
 * this the user has no signal it happened — especially on a CLEAN pass. One
 * chip per reviewed script, newest first, blocked ones expandable.
 */
const COLLAPSE_KEY = 'operon.reviewActivity.collapsed';

export function ReviewActivity({ events }: ReviewActivityProps) {
  const [expanded, setExpanded] = useState<number | null>(null);
  // Whole-strip collapse (the "hide this" dropdown), remembered across sessions.
  const [collapsed, setCollapsed] = useState<boolean>(() => {
    try {
      return localStorage.getItem(COLLAPSE_KEY) === '1';
    } catch {
      return false;
    }
  });
  if (!events.length) return null;

  const toggleCollapsed = () => {
    setCollapsed((c) => {
      const next = !c;
      try {
        localStorage.setItem(COLLAPSE_KEY, next ? '1' : '0');
      } catch {
        /* ignore */
      }
      return next;
    });
  };

  // Newest first; keep it compact.
  const ordered = [...events].sort((a, b) => (b.ts || 0) - (a.ts || 0));
  const clean = events.filter((e) => e.outcome === 'clean').length;
  const blocked = events.filter((e) => e.outcome === 'blocked').length;
  const warned = events.filter((e) => e.outcome === 'warned').length;

  return (
    <div className="mx-3 mb-2 rounded-lg border border-border-default bg-panel/60">
      <button
        onClick={toggleCollapsed}
        title={collapsed ? 'Show reviewed scripts' : 'Hide reviewed scripts'}
        className={`w-full flex items-center gap-2 px-3 py-1.5 hover:bg-hover transition-colors ${
          collapsed ? '' : 'border-b border-border-default'
        }`}
      >
        <ShieldCheck className="w-3.5 h-3.5 text-blue-500 pointer-events-none" />
        <span className="text-[11px] font-medium text-secondary">Sonnet&nbsp;5 pre-submit review</span>
        <span className="text-[10px] text-muted ml-auto">
          {events.length} script{events.length !== 1 ? 's' : ''}
          {clean > 0 && <span className="text-green-600 dark:text-green-400"> · {clean} clean</span>}
          {warned > 0 && <span className="text-yellow-600 dark:text-yellow-400"> · {warned} warned</span>}
          {blocked > 0 && <span className="text-red-600 dark:text-red-400"> · {blocked} blocked</span>}
        </span>
        {collapsed ? (
          <ChevronRight className="w-3.5 h-3.5 shrink-0 text-muted pointer-events-none" />
        ) : (
          <ChevronDown className="w-3.5 h-3.5 shrink-0 text-muted pointer-events-none" />
        )}
      </button>
      <div className={`p-1.5 space-y-1 max-h-40 overflow-y-auto ${collapsed ? 'hidden' : ''}`}>
        {ordered.map((e, i) => {
          const blockedOne = e.outcome === 'blocked';
          const warnedOne = e.outcome === 'warned';
          // Both carry findings worth reading, so both expand.
          const hasFindings = blockedOne || warnedOne;
          const isOpen = expanded === i;
          return (
            <div key={`${e.ts}-${e.script}-${i}`}>
              <button
                onClick={() => hasFindings && setExpanded(isOpen ? null : i)}
                className={`w-full flex items-center gap-1.5 px-2 py-1 rounded text-[11px] text-left ${
                  hasFindings ? 'hover:bg-hover cursor-pointer' : 'cursor-default'
                }`}
              >
                {e.outcome === 'clean' ? (
                  <ShieldCheck className="w-3.5 h-3.5 shrink-0 text-green-600 dark:text-green-400 pointer-events-none" />
                ) : blockedOne ? (
                  <ShieldAlert className="w-3.5 h-3.5 shrink-0 text-red-600 dark:text-red-400 pointer-events-none" />
                ) : warnedOne ? (
                  <ShieldAlert className="w-3.5 h-3.5 shrink-0 text-yellow-600 dark:text-yellow-400 pointer-events-none" />
                ) : (
                  <ShieldQuestion className="w-3.5 h-3.5 shrink-0 text-muted pointer-events-none" />
                )}
                <span className="font-mono text-secondary truncate">{e.script || 'sbatch'}</span>
                <span
                  className={`ml-auto shrink-0 text-[10px] font-medium ${
                    e.outcome === 'clean'
                      ? 'text-green-600 dark:text-green-400'
                      : blockedOne
                        ? 'text-red-600 dark:text-red-400'
                        : warnedOne
                          ? 'text-yellow-600 dark:text-yellow-400'
                          : 'text-muted'
                  }`}
                >
                  {e.outcome === 'clean'
                    ? 'no issues'
                    : blockedOne
                      ? `blocked · ${e.n} issue${e.n !== 1 ? 's' : ''}`
                      : warnedOne
                        ? `submitted · ${e.n} warning${e.n !== 1 ? 's' : ''}`
                        : 'review unavailable'}
                </span>
                {hasFindings &&
                  (isOpen ? (
                    <ChevronDown className="w-3 h-3 shrink-0 text-muted pointer-events-none" />
                  ) : (
                    <ChevronRight className="w-3 h-3 shrink-0 text-muted pointer-events-none" />
                  ))}
              </button>
              {hasFindings && isOpen && (
                <div className="ml-6 mr-2 mb-1 space-y-1">
                  {e.findings.map((f, j) => (
                    <div
                      key={j}
                      className={`rounded border px-2 py-1 text-[10px] ${
                        blockedOne
                          ? 'border-red-600/40 bg-red-600/5'
                          : 'border-yellow-600/40 bg-yellow-500/5'
                      }`}
                    >
                      <div className="font-medium text-primary break-words">
                        {f.line != null && <span className="text-muted font-mono mr-1">L{f.line}</span>}
                        {f.title}
                      </div>
                      {f.why_wrong && <div className="text-secondary break-words mt-0.5">{f.why_wrong}</div>}
                      {f.fix && (
                        <div className="text-secondary break-words mt-0.5">
                          <span className="text-muted">Fix: </span>
                          {f.fix}
                        </div>
                      )}
                    </div>
                  ))}
                </div>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
