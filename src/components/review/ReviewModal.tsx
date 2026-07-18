import { RefreshCw, ShieldCheck, X } from 'lucide-react';
import type { ReviewResult } from '../../lib/review';
import { ReviewFindings } from './ReviewFindings';

interface ReviewModalProps {
  isOpen: boolean;
  onClose: () => void;
  result: ReviewResult | null;
  loading: boolean;
  error: string | null;
  /** File under review, e.g. "analysis.py". */
  target: string;
  onRerun: () => void;
}

export function ReviewModal({
  isOpen,
  onClose,
  result,
  loading,
  error,
  target,
  onRerun,
}: ReviewModalProps) {
  if (!isOpen) return null;

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center pt-24 bg-black/40"
      onClick={onClose}
    >
      <div
        className="w-[640px] max-w-[90vw] max-h-[70vh] flex flex-col bg-panel border border-border-strong rounded-lg shadow-2xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center gap-2 px-3 py-2 border-b border-border-default shrink-0">
          <ShieldCheck className="w-4 h-4 text-blue-500 pointer-events-none" />
          <span className="text-sm font-medium text-primary">Code review</span>
          <span className="text-xs font-mono text-muted truncate flex-1" title={target}>
            {target}
          </span>
          <button
            onClick={onRerun}
            disabled={loading}
            title="Re-run review"
            className="p-1 rounded hover:bg-hover text-secondary disabled:opacity-40 transition-colors"
          >
            <RefreshCw className={`w-3.5 h-3.5 pointer-events-none ${loading ? 'animate-spin' : ''}`} />
          </button>
          <button
            onClick={onClose}
            className="p-1 rounded hover:bg-hover text-secondary transition-colors"
          >
            <X className="w-3.5 h-3.5 pointer-events-none" />
          </button>
        </div>

        <div className="flex-1 overflow-y-auto p-3">
          <ReviewFindings result={result} loading={loading} error={error} target={target} />
        </div>
      </div>
    </div>
  );
}
