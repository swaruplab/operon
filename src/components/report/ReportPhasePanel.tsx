import { useState } from 'react';
import {
  Search, MessageCircle, FileText, Printer,
  CheckCircle, Circle, Loader2, ArrowRight, ChevronDown, ChevronRight,
  BookOpen, Wrench, FlaskConical, ClipboardList, RefreshCw,
} from 'lucide-react';
import type { ReportPhase, MethodsInfo, ProjectScan } from '../../types/report';
import type { PlanHistoryEntry } from '../../lib/plans';
import { ReportFileSelector } from './ReportFileSelector';
import { formatBytes } from '../../lib/report';

export type ReportScope = 'comprehensive' | 'focused';

interface ReportPhasePanelProps {
  phase: ReportPhase;
  scan: ProjectScan | null;
  scanProgress?: { dirsScanned: number; filesFound: number; currentDir: string } | null;
  selectedFiles: string[];
  onSelectionChange: (paths: string[]) => void;
  methodsInfo: MethodsInfo | null;
  onProceed: (scope: ReportScope, selectedPlanTitle?: string) => void;
  onRescan: () => void;
  onCancel: () => void;
  onGenerate: () => void;
  isStreaming: boolean;
  outputPath: string | null;
  error: string | null;
  isLoading: boolean;
  planHistory: PlanHistoryEntry[];
  currentPlanTitle: string | null;
}

const PHASES: { id: ReportPhase; label: string; icon: typeof Search }[] = [
  { id: 'scan', label: 'Scan', icon: Search },
  { id: 'select', label: 'Select Files', icon: FileText },
  { id: 'clarify', label: 'Context', icon: MessageCircle },
  { id: 'draft', label: 'Draft', icon: BookOpen },
  { id: 'render', label: 'Generate PDF', icon: Printer },
];

function PhaseIndicator({ currentPhase }: { currentPhase: ReportPhase }) {
  const activeIdx = PHASES.findIndex(p => p.id === currentPhase);

  return (
    <div className="flex items-center gap-0.5 px-2 py-2">
      {PHASES.map((phase, i) => {
        const Icon = phase.icon;
        const isActive = phase.id === currentPhase;
        const isDone = i < activeIdx;
        const isFuture = i > activeIdx;

        return (
          <div key={phase.id} className="flex items-center gap-0.5">
            <div className={`flex items-center gap-1 px-1.5 py-0.5 rounded-md text-[10px] ${
              isActive ? 'bg-purple-900/30 text-purple-300 border border-purple-700/30' :
              isDone ? 'text-green-400' :
              'text-zinc-600'
            }`}>
              {isDone ? (
                <CheckCircle className="w-3 h-3" />
              ) : isActive ? (
                <Icon className="w-3 h-3" />
              ) : (
                <Circle className="w-3 h-3" />
              )}
              <span className={isFuture ? 'hidden sm:inline' : ''}>{phase.label}</span>
            </div>
            {i < PHASES.length - 1 && (
              <ArrowRight className={`w-2.5 h-2.5 ${isDone ? 'text-green-600' : 'text-zinc-700'}`} />
            )}
          </div>
        );
      })}
    </div>
  );
}

function MethodsPreview({ methods }: { methods: MethodsInfo }) {
  const [expanded, setExpanded] = useState(false);

  return (
    <div className="border border-zinc-800 rounded-lg bg-zinc-900/40 overflow-hidden">
      <button
        onClick={() => setExpanded(!expanded)}
        className="w-full flex items-center gap-2 px-3 py-2 text-left hover:bg-zinc-800/40 transition-colors"
      >
        <Wrench className="w-3.5 h-3.5 text-zinc-400 shrink-0" />
        <span className="text-[11px] font-medium text-zinc-300">
          Detected Methods
        </span>
        <span className="text-[10px] text-zinc-500 ml-auto">
          {methods.tools.length} tools
          {methods.r_version ? ` | R ${methods.r_version}` : ''}
          {methods.python_version ? ` | Python ${methods.python_version}` : ''}
        </span>
        {expanded ? <ChevronDown className="w-3 h-3 text-zinc-500" /> : <ChevronRight className="w-3 h-3 text-zinc-500" />}
      </button>

      {expanded && (
        <div className="px-3 pb-2 space-y-1.5">
          {methods.tools.slice(0, 20).map((tool, i) => (
            <div key={i} className="flex items-center gap-2 text-[10px]">
              <span className="text-zinc-300 font-mono">{tool.name}</span>
              {tool.version && <span className="text-zinc-500">{tool.version}</span>}
              {tool.language && (
                <span className="text-[9px] text-zinc-600 bg-zinc-800 px-1 rounded">{tool.language}</span>
              )}
            </div>
          ))}
          {methods.tools.length > 20 && (
            <span className="text-[10px] text-zinc-600">+{methods.tools.length - 20} more</span>
          )}
        </div>
      )}
    </div>
  );
}

export function ReportPhasePanel({
  phase, scan, scanProgress, selectedFiles, onSelectionChange,
  methodsInfo, onProceed, onRescan, onCancel, onGenerate, isStreaming, outputPath, error, isLoading,
  planHistory, currentPlanTitle,
}: ReportPhasePanelProps) {
  const [reportScope, setReportScope] = useState<ReportScope>('comprehensive');
  const [selectedPlan, setSelectedPlan] = useState<string>('current');

  if (phase === 'idle') return null;

  return (
    <div className="border border-purple-800/30 rounded-lg bg-purple-950/10">
      {/* Phase indicator */}
      <PhaseIndicator currentPhase={phase} />

      <div className="px-3 pb-3 space-y-2.5">
        {/* Scan phase */}
        {phase === 'scan' && (
          <div className="flex flex-col items-center gap-1 py-3">
            <div className="flex items-center gap-2">
              <Loader2 className="w-4 h-4 text-purple-400 animate-spin" />
              <span className="text-[11px] text-zinc-400">
                {scanProgress
                  ? `Scanning... ${scanProgress.dirsScanned} folder${scanProgress.dirsScanned === 1 ? '' : 's'}, ${scanProgress.filesFound} file${scanProgress.filesFound === 1 ? '' : 's'} found`
                  : 'Scanning project for analysis files...'}
              </span>
            </div>
            {scanProgress && scanProgress.currentDir && (
              <span className="text-[10px] text-zinc-600 font-mono truncate max-w-full px-2" title={scanProgress.currentDir}>
                {scanProgress.currentDir}
              </span>
            )}
          </div>
        )}

        {/* Select phase */}
        {phase === 'select' && scan && (
          <>
            <ReportFileSelector
              scan={scan}
              selectedFiles={selectedFiles}
              onSelectionChange={onSelectionChange}
            />
            {methodsInfo && <MethodsPreview methods={methodsInfo} />}

            {/* Report scope selector */}
            <div className="border border-zinc-800 rounded-lg bg-zinc-900/40 p-2.5 space-y-2">
              <span className="text-[10px] text-zinc-400 font-medium">Report scope</span>
              <div className="flex gap-2">
                <button
                  onClick={() => setReportScope('comprehensive')}
                  className={`flex-1 px-2.5 py-1.5 rounded-md text-[10px] font-medium border transition-colors ${
                    reportScope === 'comprehensive'
                      ? 'bg-purple-900/30 border-purple-600/40 text-purple-300'
                      : 'bg-zinc-800/50 border-zinc-700/50 text-zinc-500 hover:text-zinc-300'
                  }`}
                >
                  Comprehensive — full paper with all sections
                </button>
                <button
                  onClick={() => setReportScope('focused')}
                  className={`flex-1 px-2.5 py-1.5 rounded-md text-[10px] font-medium border transition-colors ${
                    reportScope === 'focused'
                      ? 'bg-purple-900/30 border-purple-600/40 text-purple-300'
                      : 'bg-zinc-800/50 border-zinc-700/50 text-zinc-500 hover:text-zinc-300'
                  }`}
                >
                  Focused — summary with key results only
                </button>
              </div>
            </div>

            {/* Plan selector (if plans exist) */}
            {(currentPlanTitle || planHistory.length > 0) && (
              <div className="border border-zinc-800 rounded-lg bg-zinc-900/40 p-2.5 space-y-1.5">
                <div className="flex items-center gap-1.5">
                  <ClipboardList className="w-3 h-3 text-amber-400" />
                  <span className="text-[10px] text-zinc-400 font-medium">Base report on plan</span>
                </div>
                <select
                  value={selectedPlan}
                  onChange={(e) => setSelectedPlan(e.target.value)}
                  className="w-full px-2 py-1.5 bg-zinc-800 border border-zinc-700 rounded text-[11px] text-zinc-300 outline-none focus:border-purple-600/50"
                >
                  {currentPlanTitle && (
                    <option value="current">Current: {currentPlanTitle}</option>
                  )}
                  {planHistory.map((entry) => (
                    <option key={entry.filename} value={entry.path}>
                      {entry.title} ({entry.timestamp})
                    </option>
                  ))}
                  <option value="none">No plan — report from files only</option>
                </select>
              </div>
            )}

            <div className="flex items-center gap-2 pt-1">
              <button
                onClick={() => onProceed(reportScope, selectedPlan === 'none' ? undefined : selectedPlan === 'current' ? currentPlanTitle || undefined : selectedPlan)}
                disabled={selectedFiles.length === 0 || isLoading}
                className="flex items-center gap-1.5 px-3 py-1.5 bg-purple-600 hover:bg-purple-500 disabled:bg-zinc-700 disabled:text-zinc-500 text-white text-[11px] font-medium rounded-md transition-colors"
              >
                {isLoading ? <Loader2 className="w-3 h-3 animate-spin" /> : <FlaskConical className="w-3 h-3" />}
                Continue with {selectedFiles.length} files
              </button>
              <button onClick={onCancel} className="px-2 py-1.5 text-[11px] text-zinc-500 hover:text-zinc-300">
                Cancel
              </button>
              <button
                onClick={onRescan}
                disabled={isLoading}
                className="ml-auto p-1.5 rounded-md text-zinc-500 hover:text-zinc-300 hover:bg-zinc-800 transition-colors"
                title="Rescan project files"
              >
                <RefreshCw className="w-3.5 h-3.5" />
              </button>
            </div>
          </>
        )}

        {/* Clarify phase — handled by chat messages + Generate button */}
        {phase === 'clarify' && (
          <div className="flex items-center gap-2 py-2">
            <MessageCircle className="w-3.5 h-3.5 text-purple-400 pointer-events-none" />
            {isStreaming ? (
              <span className="text-[11px] text-zinc-400">
                Claude is reviewing your context...
              </span>
            ) : (
              <>
                <span className="text-[11px] text-zinc-400">
                  Provide context above, then click Generate or type <span className="text-purple-300 font-medium">"generate report"</span>.
                </span>
                <button
                  onClick={(e) => { e.stopPropagation(); onGenerate(); }}
                  className="relative z-10 ml-auto flex items-center gap-1.5 px-4 py-2 bg-purple-600 hover:bg-purple-500 active:bg-purple-700 text-white text-xs font-medium rounded-md transition-colors shrink-0 cursor-pointer select-none"
                >
                  <Printer className="w-3.5 h-3.5 pointer-events-none" />
                  Generate Report
                </button>
              </>
            )}
          </div>
        )}

        {/* Draft phase */}
        {phase === 'draft' && (
          <div className="flex items-center gap-2 py-2">
            <Loader2 className="w-3.5 h-3.5 text-purple-400 animate-spin" />
            <span className="text-[11px] text-zinc-400">
              Claude is drafting the report with PubMed citations...
            </span>
          </div>
        )}

        {/* Render phase */}
        {phase === 'render' && (
          <div className="flex items-center gap-2 py-2">
            <Loader2 className="w-3.5 h-3.5 text-purple-400 animate-spin" />
            <span className="text-[11px] text-zinc-400">Generating PDF...</span>
          </div>
        )}

        {/* Done */}
        {phase === 'done' && outputPath && (
          <div className="flex items-center gap-2 py-2 bg-green-950/20 border border-green-800/30 rounded-md px-3">
            <CheckCircle className="w-4 h-4 text-green-400 shrink-0" />
            <div>
              <div className="text-[11px] text-green-300 font-medium">Report generated!</div>
              <div className="text-[10px] text-zinc-500 font-mono truncate">{outputPath}</div>
            </div>
          </div>
        )}

        {/* Error */}
        {error && (
          <div className="flex items-center gap-2 py-2 bg-red-950/20 border border-red-800/30 rounded-md px-3">
            <span className="text-[11px] text-red-400">{error}</span>
          </div>
        )}
      </div>
    </div>
  );
}
