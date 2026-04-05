import { useState, useRef, useEffect, useCallback } from 'react';
import { MarkdownRenderer } from './MarkdownRenderer';
import {
  Send,
  Square,
  Sparkles,
  FileEdit,
  TerminalSquare,
  ChevronDown,
  ChevronRight,
  Key,
  AlertCircle,
  LogIn,
  CheckCircle,
  Loader2,
  Bot,
  ClipboardList,
  MessageCircle,
  Server,
  RotateCcw,
  Trash2,
  X,
  FolderOpen,
  FileText,
  File,
  AtSign,
  Plus,
  BookOpen,
  BookMarked,
  Search,
  ExternalLink,
  Mic,
  MicOff,
  Download,
  AlertTriangle,
  RefreshCw,
  Paperclip,
  Image,
  FlaskConical,
  History,
  Copy,
  Check,
} from 'lucide-react';
import { emit, listen, type UnlistenFn } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import { useProject } from '../../context/ProjectContext';
import type {
  ChatMessage,
  ContentBlock,
  TextBlock,
  ThinkingBlock,
  ToolUseBlock,
  ClaudeEvent,
  SessionMetadata,
  SessionFileStatus,
} from '../../types/chat';
import type { ReportPhase, ProjectScan, MethodsInfo, FilePreview } from '../../types/report';
import { scanProjectFiles, scanRemoteProjectFiles, extractMethodsInfo, generateReportPdf, generateReportFilename, batchReadFilePreviews, batchReadRemoteFilePreviews } from '../../lib/report';
import { ReportPhasePanel } from '../report/ReportPhasePanel';
import type { ReportScope } from '../report/ReportPhasePanel';
import { listPlanHistory, readPlanHistoryEntry } from '../../lib/plans';
import type { PlanHistoryEntry } from '../../lib/plans';

type ClaudeMode = 'agent' | 'plan' | 'ask' | 'report';

interface PubMedArticle {
  pmid: string;
  title: string;
  authors: string;
  journal: string;
  year: string;
  abstract_text: string;
  doi: string;
  url: string;
}

interface PubMedSearchResult {
  query: string;
  total_found: number;
  articles: PubMedArticle[];
}

interface RemoteInfo {
  profileId: string;
  profileName: string;
  remotePath: string;
}

// --- Thinking Block Display (collapsed by default, supports merged text) ---

function ThinkingDisplay({ text }: { text: string }) {
  const [expanded, setExpanded] = useState(false);

  // Extract a one-line summary from the thinking text
  const firstLine = text.split('\n').find(l => l.trim().length > 0) || 'Reasoning...';
  const summary = firstLine.trim().slice(0, 100) + (firstLine.trim().length > 100 ? '...' : '');

  return (
    <div className="my-1 border border-zinc-700/50 rounded overflow-hidden">
      <button
        onClick={() => setExpanded((v) => !v)}
        className="flex items-center gap-2 w-full px-2 py-1 text-xs bg-zinc-900/60 hover:bg-zinc-800/60"
      >
        {expanded ? (
          <ChevronDown className="w-3 h-3 text-zinc-600" />
        ) : (
          <ChevronRight className="w-3 h-3 text-zinc-600" />
        )}
        <Loader2 className="w-3 h-3 text-purple-400" />
        <span className="text-purple-400/80 text-[11px]">Thinking</span>
        {!expanded && (
          <span className="text-zinc-600 text-[10px] truncate ml-1">{summary}</span>
        )}
      </button>

      {expanded && (
        <div className="px-2 py-1.5 text-[11px] bg-zinc-950/80 border-t border-zinc-800/50 max-h-64 overflow-y-auto">
          <pre className="text-zinc-500 whitespace-pre-wrap leading-relaxed">{text}</pre>
        </div>
      )}
    </div>
  );
}

// --- Tool Use Display ---

// --- Helpers for tool display ---

const IMPORTANT_TOOLS = new Set(['TodoWrite', 'Bash', 'Write', 'Edit']);

function isImportantTool(block: ToolUseBlock): boolean {
  if (IMPORTANT_TOOLS.has(block.name)) return true;
  // Any tool with "error" status is important
  if (block.status === 'error') return true;
  return false;
}

function shortenPath(p: string): string {
  const parts = p.split('/');
  return parts.length > 3 ? '.../' + parts.slice(-2).join('/') : p;
}

// Render TodoWrite as a readable checklist
function TodoDisplay({ block }: { block: ToolUseBlock }) {
  const todos = (block.input.todos as Array<{ content: string; status: string; activeForm?: string }>) || [];
  if (todos.length === 0) return null;

  const completed = todos.filter(t => t.status === 'completed').length;
  const inProgress = todos.find(t => t.status === 'in_progress');

  return (
    <div className="my-1 rounded-lg border border-indigo-800/40 bg-indigo-950/20 overflow-hidden">
      <div className="flex items-center gap-2 px-2.5 py-1.5 bg-indigo-900/20 border-b border-indigo-800/30">
        <ClipboardList className="w-3.5 h-3.5 text-indigo-400" />
        <span className="text-[11px] text-indigo-300 font-medium">Plan</span>
        <span className="text-[10px] text-indigo-400/60 ml-auto">{completed}/{todos.length} done</span>
      </div>
      <div className="px-2.5 py-1.5 space-y-0.5">
        {todos.map((todo, i) => (
          <div key={i} className="flex items-start gap-2 py-0.5">
            <span className="mt-0.5 shrink-0 text-[11px]">
              {todo.status === 'completed' ? (
                <span className="text-green-400">{'\u2713'}</span>
              ) : todo.status === 'in_progress' ? (
                <Loader2 className="w-3 h-3 text-blue-400 animate-spin" />
              ) : (
                <span className="text-zinc-600">{'\u25CB'}</span>
              )}
            </span>
            <span className={`text-[11px] leading-relaxed ${
              todo.status === 'completed' ? 'text-zinc-500 line-through' :
              todo.status === 'in_progress' ? 'text-blue-300' :
              'text-zinc-400'
            }`}>
              {todo.status === 'in_progress' ? (todo.activeForm || todo.content) : todo.content}
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}

// Detect if a command is a long-running wait/poll (sleep, watch, polling loops)
function isWaitCommand(cmd: string): boolean {
  return /^\s*(sleep\s+\d|while\s|until\s|watch\s)/.test(cmd) ||
    /sleep\s+\d{2,}/.test(cmd);
}

// Detect if a command is a job submission (sbatch, qsub, bsub, etc.)
function isJobSubmission(cmd: string): boolean {
  return /\b(sbatch|qsub|bsub|srun\s|condor_submit)\b/.test(cmd);
}

// Extract the key action from a compound command for display
function summarizeCommand(cmd: string): string {
  // For compound commands with sleep, show the meaningful part
  if (/sleep\s+\d+\s*&&/.test(cmd)) {
    const afterSleep = cmd.replace(/^.*?sleep\s+\d+\s*&&\s*/, '');
    const firstCmd = afterSleep.split(/[;&|]/).map(s => s.trim()).find(s => s && !s.startsWith('echo'));
    if (firstCmd) return firstCmd.length > 80 ? firstCmd.slice(0, 80) + '...' : firstCmd;
  }
  // For sbatch, show the script being submitted
  if (/\bsbatch\b/.test(cmd)) {
    const match = cmd.match(/sbatch\s+(.+)/);
    return match ? `sbatch ${match[1].trim().slice(0, 60)}` : 'sbatch job submission';
  }
  return cmd.length > 120 ? cmd.slice(0, 120) + '...' : cmd;
}

// Render Bash/Run as a clean command block
function BashDisplay({ block }: { block: ToolUseBlock }) {
  const [showCmd, setShowCmd] = useState(false);
  const [showOutput, setShowOutput] = useState(false);
  const cmd = ((block.input.command as string) || '').trim();
  const desc = (block.input.description as string) || '';
  const isWait = isWaitCommand(cmd);
  const isJob = isJobSubmission(cmd);

  const statusIcon = block.status === 'complete'
    ? <CheckCircle className="w-3 h-3 text-green-400/70" />
    : block.status === 'running'
    ? <Loader2 className="w-3 h-3 text-yellow-400 animate-spin" />
    : block.status === 'error'
    ? <AlertCircle className="w-3 h-3 text-red-400" />
    : <span className="text-zinc-600 text-[10px]">{'\u25CF'}</span>;

  // For wait/poll commands or when description is available, show description prominently
  const showDescOnly = desc && (isWait || cmd.length > 120);

  return (
    <div className={`my-1 rounded-lg border overflow-hidden ${
      isJob ? 'border-amber-800/40' : 'border-zinc-700/50'
    }`}>
      {/* Header */}
      <div className={`flex items-center gap-2 px-2.5 py-1.5 ${
        isJob ? 'bg-amber-950/30' : 'bg-zinc-900/60'
      }`}>
        {statusIcon}
        <TerminalSquare className="w-3 h-3 text-zinc-500" />
        <span className={`text-[11px] font-medium ${isJob ? 'text-amber-300' : 'text-zinc-300'}`}>
          {isJob ? 'Submit Job' : isWait ? 'Waiting' : 'Run'}
        </span>
        {desc && (
          <span className="text-[11px] text-zinc-400 truncate">{'\u2192'} {desc}</span>
        )}
        {isWait && block.status === 'running' && (
          <span className="text-[10px] text-yellow-500/60 ml-auto">polling...</span>
        )}
      </div>

      {/* Command display: show summary or full command */}
      {showDescOnly ? (
        <button
          onClick={() => setShowCmd(v => !v)}
          className="flex items-center gap-1.5 w-full px-2.5 py-1 text-[10px] text-zinc-600 hover:text-zinc-400 bg-zinc-950/40 border-t border-zinc-800/30"
        >
          {showCmd ? <ChevronDown className="w-2.5 h-2.5" /> : <ChevronRight className="w-2.5 h-2.5" />}
          <span>Command</span>
        </button>
      ) : (
        <div className="px-2.5 py-1.5 bg-zinc-950/60 border-t border-zinc-800/30">
          <pre className="text-[11px] text-emerald-400/80 font-mono whitespace-pre-wrap leading-relaxed">$ {summarizeCommand(cmd)}</pre>
        </div>
      )}
      {showDescOnly && showCmd && (
        <div className="px-2.5 py-1.5 bg-zinc-950/60">
          <pre className="text-[10px] text-emerald-400/60 font-mono whitespace-pre-wrap leading-relaxed">$ {cmd}</pre>
        </div>
      )}

      {/* Output section */}
      {block.result && (
        <button
          onClick={() => setShowOutput(v => !v)}
          className="flex items-center gap-1.5 w-full px-2.5 py-1 text-[10px] text-zinc-500 hover:text-zinc-400 bg-zinc-900/40 border-t border-zinc-800/50"
        >
          {showOutput ? <ChevronDown className="w-2.5 h-2.5" /> : <ChevronRight className="w-2.5 h-2.5" />}
          <span>Output{block.result.length > 100 ? ` (${(block.result.length / 1000).toFixed(1)}k chars)` : ''}</span>
        </button>
      )}
      {showOutput && block.result && (
        <div className="px-2.5 py-1.5 bg-zinc-950/80 border-t border-zinc-800/30 max-h-48 overflow-y-auto">
          <pre className="text-[10px] text-zinc-500 whitespace-pre-wrap font-mono">{block.result.slice(0, 3000)}{block.result.length > 3000 ? '\n... (truncated)' : ''}</pre>
        </div>
      )}
    </div>
  );
}

// Render Write/Edit as a file action with readable summary
function FileActionDisplay({ block }: { block: ToolUseBlock }) {
  const [expanded, setExpanded] = useState(false);
  const fp = (block.input.file_path as string) || 'file';
  const isWrite = block.name === 'Write';

  const statusIcon = block.status === 'complete'
    ? <CheckCircle className="w-3 h-3 text-green-400/70" />
    : block.status === 'running'
    ? <Loader2 className="w-3 h-3 text-yellow-400 animate-spin" />
    : block.status === 'error'
    ? <AlertCircle className="w-3 h-3 text-red-400" />
    : <span className="text-zinc-600 text-[10px]">{'\u25CF'}</span>;

  // For Edit, show what changed
  const oldStr = (block.input.old_string as string) || '';
  const newStr = (block.input.new_string as string) || '';
  const hasEditDiff = block.name === 'Edit' && (oldStr || newStr);

  return (
    <div className="my-1 rounded-lg border border-zinc-700/50 overflow-hidden">
      <button
        onClick={() => setExpanded(v => !v)}
        className="flex items-center gap-2 w-full px-2.5 py-1.5 bg-zinc-900/60 hover:bg-zinc-800/50"
      >
        {statusIcon}
        <FileEdit className="w-3 h-3 text-zinc-500" />
        <span className="text-[11px] text-zinc-300 font-medium">{isWrite ? 'Create' : 'Edit'}</span>
        <span className="text-zinc-600">{'\u2192'}</span>
        <span className="text-[11px] text-zinc-400 font-mono truncate">{shortenPath(fp)}</span>
        <span className="ml-auto">
          {expanded ? <ChevronDown className="w-3 h-3 text-zinc-600" /> : <ChevronRight className="w-3 h-3 text-zinc-600" />}
        </span>
      </button>
      {expanded && (
        <div className="px-2.5 py-1.5 bg-zinc-950/60 border-t border-zinc-800/50 max-h-64 overflow-y-auto">
          {hasEditDiff ? (
            <div className="space-y-1">
              {oldStr && (
                <div>
                  <div className="text-[10px] text-red-400/70 font-medium mb-0.5">Removed:</div>
                  <pre className="text-[10px] text-red-300/50 font-mono whitespace-pre-wrap bg-red-950/20 rounded px-1.5 py-1">{oldStr.slice(0, 1000)}</pre>
                </div>
              )}
              {newStr && (
                <div>
                  <div className="text-[10px] text-green-400/70 font-medium mb-0.5">Added:</div>
                  <pre className="text-[10px] text-green-300/50 font-mono whitespace-pre-wrap bg-green-950/20 rounded px-1.5 py-1">{newStr.slice(0, 1000)}</pre>
                </div>
              )}
            </div>
          ) : isWrite && block.input.content ? (
            <pre className="text-[10px] text-zinc-400 font-mono whitespace-pre-wrap">{String(block.input.content).slice(0, 1500)}{String(block.input.content).length > 1500 ? '\n... (truncated)' : ''}</pre>
          ) : (
            <pre className="text-[10px] text-zinc-500 whitespace-pre-wrap">{JSON.stringify(block.input, null, 2)}</pre>
          )}
        </div>
      )}
    </div>
  );
}

// Generic collapsed tool display for minor tools (Read, Grep, Glob, etc.)
function MinorToolDisplay({ block }: { block: ToolUseBlock }) {
  const [expanded, setExpanded] = useState(false);

  const getInfo = (): { label: string; detail?: string } => {
    switch (block.name) {
      case 'Read': {
        const fp = (block.input.file_path as string) || '';
        return { label: 'Read', detail: shortenPath(fp) };
      }
      case 'Grep': return { label: 'Search', detail: (block.input.pattern as string) || '' };
      case 'Glob': return { label: 'Find files', detail: (block.input.pattern as string) || '' };
      default: return { label: block.name };
    }
  };

  const info = getInfo();
  const statusColor =
    block.status === 'complete' ? 'text-green-400'
    : block.status === 'running' ? 'text-yellow-400 animate-pulse'
    : block.status === 'error' ? 'text-red-400'
    : 'text-zinc-500';

  return (
    <div className="my-0.5 rounded overflow-hidden">
      <button
        onClick={() => setExpanded(v => !v)}
        className="flex items-center gap-2 w-full px-2 py-0.5 text-[11px] hover:bg-zinc-800/30"
      >
        <span className={`text-[8px] ${statusColor}`}>{'\u25CF'}</span>
        <span className="text-zinc-500">{info.label}</span>
        {info.detail && <span className="text-zinc-600 font-mono truncate text-[10px]">{info.detail}</span>}
        {expanded ? <ChevronDown className="w-2.5 h-2.5 text-zinc-700 ml-auto" /> : <ChevronRight className="w-2.5 h-2.5 text-zinc-700 ml-auto" />}
      </button>
      {expanded && block.result && (
        <div className="px-2 py-1 bg-zinc-950/60 max-h-36 overflow-y-auto">
          <pre className="text-[10px] text-zinc-500 whitespace-pre-wrap font-mono">{block.result.slice(0, 2000)}</pre>
        </div>
      )}
    </div>
  );
}

// Known MCP tool prefixes → display name + color
const MCP_TOOL_PREFIXES: Array<{ prefix: string; label: string; color: string }> = [
  { prefix: 'encode_', label: 'ENCODE', color: 'text-teal-400' },
  { prefix: 'analyze-active-site', label: 'BioMCP', color: 'text-violet-400' },
  { prefix: 'search-disease-proteins', label: 'BioMCP', color: 'text-violet-400' },
];

function getMCPInfo(toolName: string): { label: string; color: string } | null {
  for (const p of MCP_TOOL_PREFIXES) {
    if (toolName.startsWith(p.prefix)) return { label: p.label, color: p.color };
  }
  return null;
}

// MCP tool display — shows server badge and tool details
function MCPToolDisplay({ block, mcpInfo }: { block: ToolUseBlock; mcpInfo: { label: string; color: string } }) {
  const [expanded, setExpanded] = useState(false);

  const statusIcon = block.status === 'complete'
    ? <CheckCircle className="w-3 h-3 text-green-400/70" />
    : block.status === 'running'
    ? <Loader2 className="w-3 h-3 text-yellow-400 animate-spin" />
    : block.status === 'error'
    ? <AlertCircle className="w-3 h-3 text-red-400" />
    : <span className="text-zinc-600 text-[10px]">{'\u25CF'}</span>;

  return (
    <div className="my-1 rounded-lg border border-zinc-700/50 overflow-hidden">
      <button
        onClick={() => setExpanded(v => !v)}
        className="flex items-center gap-2 w-full px-2.5 py-1.5 bg-zinc-900/60 hover:bg-zinc-800/50"
      >
        {statusIcon}
        <Server className="w-3 h-3 text-zinc-500" />
        <span className={`text-[10px] px-1.5 py-0.5 rounded bg-zinc-800 font-medium ${mcpInfo.color}`}>
          {mcpInfo.label}
        </span>
        <span className="text-[11px] text-zinc-300 font-mono truncate">{block.name}</span>
        <span className="ml-auto">
          {expanded ? <ChevronDown className="w-3 h-3 text-zinc-600" /> : <ChevronRight className="w-3 h-3 text-zinc-600" />}
        </span>
      </button>
      {expanded && (
        <div className="px-2.5 py-1.5 bg-zinc-950/60 border-t border-zinc-800/50 max-h-48 overflow-y-auto space-y-1">
          {Object.keys(block.input).length > 0 && (
            <pre className="text-[10px] text-zinc-500 whitespace-pre-wrap font-mono">{JSON.stringify(block.input, null, 2)}</pre>
          )}
          {block.result && (
            <pre className="text-[10px] text-zinc-400 whitespace-pre-wrap font-mono mt-1">{block.result.slice(0, 2000)}{block.result.length > 2000 ? '\n... (truncated)' : ''}</pre>
          )}
        </div>
      )}
    </div>
  );
}

// Route to the right display component
function ToolUseDisplay({ block }: { block: ToolUseBlock }) {
  if (block.name === 'TodoWrite') return <TodoDisplay block={block} />;
  if (block.name === 'Bash') return <BashDisplay block={block} />;
  if (block.name === 'Write' || block.name === 'Edit') return <FileActionDisplay block={block} />;
  const mcpInfo = getMCPInfo(block.name);
  if (mcpInfo) return <MCPToolDisplay block={block} mcpInfo={mcpInfo} />;
  return <MinorToolDisplay block={block} />;
}

// --- Session Row with click-to-rename ---

function SessionRow({ session, displayName, ageStr, onResume, onDelete, onRename }: {
  session: SessionMetadata;
  displayName: string;
  ageStr: string;
  onResume: () => void;
  onDelete: () => void;
  onRename: (name: string) => void;
}) {
  const [editing, setEditing] = useState(false);
  const [editValue, setEditValue] = useState(displayName);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (editing) inputRef.current?.focus();
  }, [editing]);

  return (
    <div className="flex items-center gap-2 py-1.5 px-2 rounded hover:bg-indigo-900/30 transition-colors group">
      <span className={`w-1.5 h-1.5 rounded-full shrink-0 ${
        session.status === 'running' ? 'bg-green-400 animate-pulse' : 'bg-zinc-500'
      }`} />

      {editing ? (
        <input
          ref={inputRef}
          value={editValue}
          onChange={(e) => setEditValue(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter' && editValue.trim()) {
              onRename(editValue.trim());
              setEditing(false);
            }
            if (e.key === 'Escape') {
              setEditValue(displayName);
              setEditing(false);
            }
          }}
          onBlur={() => {
            if (editValue.trim() && editValue.trim() !== displayName) {
              onRename(editValue.trim());
            }
            setEditing(false);
          }}
          className="flex-1 bg-zinc-800 border border-indigo-500/50 rounded px-1.5 py-0.5 text-[11px] text-zinc-200 outline-none"
        />
      ) : (
        <span
          onClick={() => { setEditValue(displayName); setEditing(true); }}
          className="text-[11px] text-zinc-300 truncate flex-1 cursor-pointer hover:text-indigo-300 transition-colors"
          title="Click to rename session"
        >
          {displayName}
          <span className="text-zinc-600 ml-1">{'\u00B7'} {ageStr}</span>
          {session.status === 'running' && (
            <span className="text-green-400 ml-1">(running)</span>
          )}
        </span>
      )}

      <button
        onClick={onResume}
        className="text-[10px] bg-indigo-700/60 text-indigo-200 px-2 py-0.5 rounded hover:bg-indigo-600/60 transition-colors shrink-0"
      >
        Resume
      </button>
      <button
        onClick={onDelete}
        className="text-zinc-600 hover:text-red-400 transition-colors opacity-0 group-hover:opacity-100 shrink-0"
      >
        <Trash2 className="w-3 h-3" />
      </button>
    </div>
  );
}

// --- Collapsible "Working" section that groups thinking + tool blocks ---

function WorkingSection({ thinkingText, tools, isActive }: {
  thinkingText: string;
  tools: ToolUseBlock[];
  isActive: boolean;
}) {
  const [showMinor, setShowMinor] = useState(false);

  // Split tools into important (always visible) and minor (collapsed)
  const importantTools = tools.filter(t => isImportantTool(t));
  const minorTools = tools.filter(t => !isImportantTool(t));

  const runningTool = tools.find(t => t.status === 'running');
  const completedCount = tools.filter(t => t.status === 'complete').length;
  const totalCount = tools.length;

  return (
    <div className="my-1">
      {/* Important tools: always shown with rich formatting */}
      {importantTools.map((tool, i) => (
        <ToolUseDisplay key={tool.id || `imp-${i}`} block={tool} />
      ))}

      {/* Minor tools: collapsed into a single line */}
      {minorTools.length > 0 && (
        <div className="my-1 rounded-lg border border-zinc-800/40 overflow-hidden">
          <button
            onClick={() => setShowMinor(v => !v)}
            className="flex items-center gap-2 w-full px-2.5 py-1 text-[11px] bg-zinc-900/40 hover:bg-zinc-800/40"
          >
            {showMinor ? (
              <ChevronDown className="w-3 h-3 text-zinc-600 shrink-0" />
            ) : (
              <ChevronRight className="w-3 h-3 text-zinc-600 shrink-0" />
            )}
            <span className="text-zinc-500">
              {minorTools.length} background {minorTools.length === 1 ? 'step' : 'steps'}
            </span>
            <span className="text-zinc-600 text-[10px]">
              ({minorTools.map(t => t.name).filter((v, i, a) => a.indexOf(v) === i).join(', ')})
            </span>
          </button>
          {showMinor && (
            <div className="border-t border-zinc-800/30 bg-zinc-950/40">
              {minorTools.map((tool, i) => (
                <MinorToolDisplay key={tool.id || `min-${i}`} block={tool} />
              ))}
            </div>
          )}
        </div>
      )}

      {/* Thinking section: collapsed by default */}
      {thinkingText && (
        <details className="my-1 rounded-lg border border-purple-900/30 overflow-hidden group">
          <summary className="flex items-center gap-2 px-2.5 py-1 text-[11px] text-purple-400/60 cursor-pointer hover:bg-purple-950/20 bg-zinc-900/30">
            <ChevronRight className="w-3 h-3 shrink-0 group-open:rotate-90 transition-transform" />
            <span>Reasoning</span>
            <span className="text-zinc-700 truncate ml-1 text-[10px]">
              {thinkingText.split('\n').find(l => l.trim())?.trim().slice(0, 60)}...
            </span>
          </summary>
          <div className="px-2.5 py-1.5 max-h-48 overflow-y-auto border-t border-purple-900/20 bg-zinc-950/40">
            <pre className="text-[11px] text-zinc-500 whitespace-pre-wrap leading-relaxed">{thinkingText}</pre>
          </div>
        </details>
      )}

      {/* Active status indicator */}
      {isActive && runningTool && (
        <div className="flex items-center gap-2 px-2 py-1 text-[11px]">
          <Loader2 className="w-3 h-3 text-blue-400 animate-spin" />
          <span className="text-blue-400/70">
            {runningTool.name === 'Bash'
              ? ((runningTool.input.description as string) || 'Running command...')
              : `${runningTool.name}...`}
          </span>
          {totalCount > 1 && (
            <span className="text-zinc-600 text-[10px] ml-auto">{completedCount}/{totalCount}</span>
          )}
        </div>
      )}
    </div>
  );
}

// --- Message Context Menu ---

function MessageContextMenu({
  x,
  y,
  onCopy,
  onClose,
}: {
  x: number;
  y: number;
  onCopy: () => void;
  onClose: () => void;
}) {
  const menuRef = useRef<HTMLDivElement>(null);
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    const handleClick = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        onClose();
      }
    };
    const handleEsc = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    document.addEventListener('mousedown', handleClick);
    document.addEventListener('keydown', handleEsc);
    return () => {
      document.removeEventListener('mousedown', handleClick);
      document.removeEventListener('keydown', handleEsc);
    };
  }, [onClose]);

  // Adjust position so menu doesn't overflow viewport
  const adjustedStyle: React.CSSProperties = {
    position: 'fixed',
    top: y,
    left: x,
    zIndex: 9999,
  };

  return (
    <div ref={menuRef} style={adjustedStyle}
      className="min-w-[140px] py-1 bg-zinc-800 border border-zinc-700 rounded-lg shadow-xl shadow-black/40 backdrop-blur-sm"
    >
      <button
        onClick={() => {
          onCopy();
          setCopied(true);
          setTimeout(() => onClose(), 600);
        }}
        className="w-full flex items-center gap-2.5 px-3 py-1.5 text-[12px] text-zinc-300 hover:bg-zinc-700 hover:text-white transition-colors"
      >
        {copied ? <Check className="w-3.5 h-3.5 text-emerald-400" /> : <Copy className="w-3.5 h-3.5" />}
        {copied ? 'Copied!' : 'Copy message'}
      </button>
    </div>
  );
}

// --- Message Bubble ---

function MessageBubble({ message }: { message: ChatMessage }) {
  const isUser = message.role === 'user';
  const isSystem = message.role === 'system';
  const [ctxMenu, setCtxMenu] = useState<{ x: number; y: number } | null>(null);

  // Extract all text content from the message for copying
  const getMessageText = useCallback(() => {
    return message.content
      .filter((b) => b.type === 'text')
      .map((b) => (b as TextBlock).text)
      .join('\n\n');
  }, [message.content]);

  const handleContextMenu = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    setCtxMenu({ x: e.clientX, y: e.clientY });
  }, []);

  const handleCopy = useCallback(() => {
    const text = getMessageText();
    if (text) navigator.clipboard.writeText(text).catch(() => {});
  }, [getMessageText]);

  if (isUser || isSystem) {
    // System message styling based on variant
    const systemStyles = message.variant === 'success'
      ? 'bg-emerald-900/30 text-emerald-300 border border-emerald-800'
      : message.variant === 'info'
        ? 'bg-blue-900/30 text-blue-300 border border-blue-800'
        : 'bg-red-900/30 text-red-300 border border-red-800';

    return (
      <div className={`flex ${isUser ? 'justify-end' : 'justify-start'}`}>
        <div
          onContextMenu={handleContextMenu}
          className={`max-w-[90%] rounded-xl px-3.5 py-2.5 text-[13px] leading-relaxed cursor-default break-words overflow-hidden ${
            isUser
              ? 'bg-blue-600/90 text-white'
              : systemStyles
          }`}
        >
          {message.content.map((block, i) => (
            block.type === 'text' ? (
              <div key={i} className="whitespace-pre-wrap">{(block as TextBlock).text}</div>
            ) : null
          ))}
        </div>
        {ctxMenu && (
          <MessageContextMenu
            x={ctxMenu.x}
            y={ctxMenu.y}
            onCopy={handleCopy}
            onClose={() => setCtxMenu(null)}
          />
        )}
      </div>
    );
  }

  // Assistant message: group thinking + tools into WorkingSection, show text blocks separately
  const thinkingParts: string[] = [];
  const toolBlocks: ToolUseBlock[] = [];
  const textParts: Array<{ idx: number; text: string }> = [];

  message.content.forEach((block, i) => {
    if (block.type === 'thinking') {
      thinkingParts.push((block as ThinkingBlock).thinking);
    } else if (block.type === 'tool_use') {
      toolBlocks.push(block as ToolUseBlock);
    } else if (block.type === 'text') {
      textParts.push({ idx: i, text: (block as TextBlock).text });
    }
  });

  const hasWorkingContent = thinkingParts.length > 0 || toolBlocks.length > 0;
  const isActive = !!message.isStreaming;
  const mergedThinking = thinkingParts.join('\n\n');

  return (
    <div className="flex justify-start">
      <div
        onContextMenu={handleContextMenu}
        className="max-w-[90%] rounded-xl px-3.5 py-2.5 text-[13px] leading-relaxed bg-zinc-800/80 text-zinc-200 cursor-default break-words overflow-hidden"
      >
        {/* Working section: collapsed by default */}
        {hasWorkingContent && (
          <WorkingSection
            thinkingText={mergedThinking}
            tools={toolBlocks}
            isActive={isActive}
          />
        )}

        {/* Text output — always visible, rendered as Markdown for assistant */}
        {textParts.map(({ idx, text }) => (
          <MarkdownRenderer key={idx} text={text} />
        ))}
      </div>
      {ctxMenu && (
        <MessageContextMenu
          x={ctxMenu.x}
          y={ctxMenu.y}
          onCopy={handleCopy}
          onClose={() => setCtxMenu(null)}
        />
      )}
    </div>
  );
}

// --- Auth Setup (API Key or OAuth) ---

function AuthSetup({ onDone }: { onDone: (method: string) => void }) {
  const [mode, setMode] = useState<'choose' | 'api_key' | 'oauth'>('choose');
  const [key, setKey] = useState('');
  const [saving, setSaving] = useState(false);
  const [verifying, setVerifying] = useState(false);
  const [oauthError, setOauthError] = useState('');
  const [terminalOpened, setTerminalOpened] = useState(false);

  const saveApiKey = async () => {
    if (!key.trim()) return;
    setSaving(true);
    try {
      await invoke('store_api_key', { key: key.trim() });
      onDone('api_key');
    } catch (err) {
      console.error('Failed to store API key:', err);
    }
    setSaving(false);
  };

  const openTerminalLogin = async () => {
    setOauthError('');
    try {
      // Open a terminal tab inside the app and run `claude login`
      const terminalId = crypto.randomUUID();
      await emit('open-login-terminal', {
        terminalId,
        title: 'Claude Login',
        command: 'claude login',
      });
      setTerminalOpened(true);
    } catch (err) {
      setOauthError(`${err}`);
    }
  };

  const verifyLogin = async () => {
    setVerifying(true);
    setOauthError('');
    try {
      const status = await invoke<{ authenticated: boolean; method: string }>('check_auth_status');
      if (status.authenticated) {
        onDone(status.method);
      } else {
        setOauthError('Not authenticated yet. Complete the login in Terminal, then try again.');
      }
    } catch {
      setOauthError('Verification failed. Try again.');
    }
    setVerifying(false);
  };

  // --- Choose auth method ---
  if (mode === 'choose') {
    return (
      <div className="flex flex-col items-center justify-center h-full px-6 text-center">
        <Sparkles className="w-10 h-10 text-blue-400/60 mb-3" />
        <h3 className="text-sm font-medium text-zinc-300 mb-1">Connect to Claude</h3>
        <p className="text-xs text-zinc-500 mb-5">
          Choose how to authenticate with Anthropic
        </p>

        <button
          onClick={() => { setMode('oauth'); openTerminalLogin(); }}
          className="w-full flex items-center gap-3 px-4 py-3 bg-orange-600 hover:bg-orange-700 rounded-lg text-sm text-white transition-colors mb-3"
        >
          <LogIn className="w-4 h-4 shrink-0" />
          <div className="text-left">
            <div className="font-medium">Sign in with Claude</div>
            <div className="text-[11px] text-orange-200/80 mt-0.5">
              For Max, Pro &amp; Team subscribers
            </div>
          </div>
        </button>

        <button
          onClick={() => setMode('api_key')}
          className="w-full flex items-center gap-3 px-4 py-3 bg-zinc-800 hover:bg-zinc-700 rounded-lg text-sm text-zinc-200 transition-colors"
        >
          <Key className="w-4 h-4 shrink-0 text-zinc-400" />
          <div className="text-left">
            <div className="font-medium">Use API Key</div>
            <div className="text-[11px] text-zinc-500 mt-0.5">
              For API &amp; direct billing users
            </div>
          </div>
        </button>
      </div>
    );
  }

  // --- OAuth flow ---
  if (mode === 'oauth') {
    return (
      <div className="flex flex-col items-center justify-center h-full px-6 text-center">
        <LogIn className="w-10 h-10 text-orange-400 mb-3" />
        <h3 className="text-sm font-medium text-zinc-300 mb-1">
          {terminalOpened ? 'Complete login in the terminal below' : 'Opening terminal...'}
        </h3>
        <p className="text-xs text-zinc-500 mb-2">
          <code className="bg-zinc-800 px-1 py-0.5 rounded text-orange-300 text-[11px]">claude login</code> is running in a terminal tab below.
        </p>
        <p className="text-xs text-zinc-500 mb-5">
          Follow the prompts in the terminal, then click Verify below.
        </p>

        {oauthError && (
          <div className="w-full mb-3 px-3 py-2 bg-red-900/20 border border-red-800/30 rounded text-xs text-red-400">
            {oauthError}
          </div>
        )}

        <button
          onClick={verifyLogin}
          disabled={verifying}
          className="w-full px-3 py-2.5 bg-green-600 hover:bg-green-700 disabled:bg-green-800 rounded-lg text-sm text-white font-medium transition-colors mb-2"
        >
          {verifying ? (
            <span className="flex items-center justify-center gap-2">
              <Loader2 className="w-4 h-4 animate-spin" /> Verifying...
            </span>
          ) : (
            <span className="flex items-center justify-center gap-2">
              <CheckCircle className="w-4 h-4" /> I've logged in — verify
            </span>
          )}
        </button>

        <button
          onClick={openTerminalLogin}
          className="w-full px-3 py-2 bg-zinc-800 hover:bg-zinc-700 rounded-lg text-xs text-zinc-400 transition-colors mb-2"
        >
          Reopen Terminal
        </button>

        <button
          onClick={() => { setMode('choose'); setOauthError(''); setTerminalOpened(false); }}
          className="text-xs text-zinc-600 hover:text-zinc-400 transition-colors"
        >
          Back to options
        </button>
      </div>
    );
  }

  // --- API key entry ---
  return (
    <div className="flex flex-col items-center justify-center h-full px-6 text-center">
      <Key className="w-10 h-10 text-zinc-700 mb-3" />
      <h3 className="text-sm font-medium text-zinc-300 mb-1">API Key</h3>
      <p className="text-xs text-zinc-500 mb-4">
        Enter your Anthropic API key
      </p>
      <input
        type="password"
        value={key}
        onChange={(e) => setKey(e.target.value)}
        onKeyDown={(e) => e.key === 'Enter' && saveApiKey()}
        placeholder="sk-ant-..."
        className="w-full px-3 py-2 bg-zinc-900 border border-zinc-700 rounded text-sm text-zinc-100 placeholder:text-zinc-600 outline-none focus:border-blue-500 mb-3"
      />
      <button
        onClick={saveApiKey}
        disabled={saving || !key.trim()}
        className="w-full px-3 py-2 bg-blue-600 hover:bg-blue-700 disabled:bg-zinc-700 rounded text-sm text-white transition-colors mb-2"
      >
        {saving ? 'Saving...' : 'Save API Key'}
      </button>
      <button
        onClick={() => setMode('choose')}
        className="text-xs text-zinc-600 hover:text-zinc-400 transition-colors"
      >
        Back to options
      </button>
    </div>
  );
}

// --- @-Mention Types & Popup ---

interface MentionItem {
  name: string;
  path: string;
  isDir: boolean;
  extension?: string;
}

interface MentionRef {
  name: string;      // display name e.g. "de_results" or "results.csv"
  path: string;      // full path
  isDir: boolean;
}

function MentionPopup({
  items,
  selectedIndex,
  onSelect,
  visible,
}: {
  items: MentionItem[];
  selectedIndex: number;
  onSelect: (item: MentionItem) => void;
  visible: boolean;
}) {
  if (!visible || items.length === 0) return null;

  return (
    <div className="absolute bottom-full left-0 mb-1 w-72 max-h-48 overflow-y-auto bg-zinc-900 border border-zinc-700 rounded-lg shadow-xl z-50">
      {items.map((item, i) => {
        const Icon = item.isDir ? FolderOpen : (
          item.extension && ['csv', 'tsv', 'txt', 'md', 'R', 'py', 'sh'].includes(item.extension) ? FileText : File
        );
        return (
          <button
            key={item.path}
            onClick={() => onSelect(item)}
            className={`w-full flex items-center gap-2 px-3 py-1.5 text-left text-xs transition-colors ${
              i === selectedIndex ? 'bg-blue-600/30 text-blue-200' : 'text-zinc-300 hover:bg-zinc-800'
            }`}
          >
            <Icon className={`w-3.5 h-3.5 shrink-0 ${item.isDir ? 'text-amber-400' : 'text-zinc-500'}`} />
            <span className="truncate">{item.name}</span>
            {item.isDir && <span className="text-[10px] text-zinc-600 ml-auto shrink-0">folder</span>}
            {item.extension && !item.isDir && (
              <span className="text-[10px] text-zinc-600 ml-auto shrink-0">.{item.extension}</span>
            )}
          </button>
        );
      })}
      <div className="px-3 py-1 border-t border-zinc-800 text-[10px] text-zinc-600">
        {'\u2191\u2193'} navigate {'\u00B7'} Enter select {'\u00B7'} Esc dismiss
      </div>
    </div>
  );
}

/** Read file contents with a size budget. Returns a summary string for context injection. */
/** Resolve @-mention to lightweight metadata. Does NOT read file contents.
 *  The @ mention just tells Claude what file/folder the user is referring to
 *  so Claude Code can decide how to inspect it using its own tools. */
function resolveMentionContext(ref: MentionRef): string {
  const ext = ref.path.split('.').pop()?.toLowerCase() || '';
  if (ref.isDir) {
    return `[Mentioned folder: ${ref.path}]`;
  }
  return `[Mentioned file: ${ref.path} (type: .${ext})]`;
}

// --- Main Chat Panel ---

// --- Mode Selector ---

const MODE_CONFIG: Record<ClaudeMode, { label: string; icon: typeof Bot; color: string; desc: string }> = {
  agent: { label: 'Agent', icon: Bot, color: 'text-blue-400', desc: 'Full tool use — reads, writes, runs commands' },
  plan: { label: 'Plan', icon: ClipboardList, color: 'text-amber-400', desc: 'Creates implementation_plan.md — no execution' },
  ask: { label: 'Ask', icon: MessageCircle, color: 'text-green-400', desc: 'Answer questions with PubMed-grounded literature' },
  report: { label: 'Report', icon: FlaskConical, color: 'text-purple-400', desc: 'Generate PDF report from analysis files with PubMed citations' },
};

function ModeSelector({ mode, onChange }: { mode: ClaudeMode; onChange: (m: ClaudeMode) => void }) {
  const [open, setOpen] = useState(false);
  const current = MODE_CONFIG[mode];
  const Icon = current.icon;

  return (
    <div className="relative">
      <button
        onClick={() => setOpen((v) => !v)}
        className="flex items-center gap-1.5 px-2 py-1 rounded hover:bg-zinc-800 transition-colors text-xs"
      >
        <Icon className={`w-3.5 h-3.5 ${current.color}`} />
        <span className="text-zinc-300 font-medium">{current.label}</span>
        <ChevronDown className="w-3 h-3 text-zinc-600" />
      </button>

      {open && (
        <>
          <div className="fixed inset-0 z-40" onClick={() => setOpen(false)} />
          <div className="absolute bottom-full left-0 mb-1 w-56 bg-zinc-900 border border-zinc-700 rounded-lg shadow-xl z-50 overflow-hidden">
            {(Object.entries(MODE_CONFIG) as [ClaudeMode, typeof current][]).map(([key, cfg]) => {
              const MIcon = cfg.icon;
              return (
                <button
                  key={key}
                  onClick={() => { onChange(key); setOpen(false); }}
                  className={`w-full flex items-start gap-2.5 px-3 py-2.5 text-left hover:bg-zinc-800 transition-colors ${
                    mode === key ? 'bg-zinc-800/60' : ''
                  }`}
                >
                  <MIcon className={`w-4 h-4 mt-0.5 shrink-0 ${cfg.color}`} />
                  <div>
                    <div className="text-xs font-medium text-zinc-200">{cfg.label}</div>
                    <div className="text-[10px] text-zinc-500 mt-0.5">{cfg.desc}</div>
                  </div>
                  {mode === key && <span className="ml-auto text-blue-400 text-xs mt-0.5">✓</span>}
                </button>
              );
            })}
          </div>
        </>
      )}
    </div>
  );
}

// --- PubMed Results Bar (shown above input when literature was found) ---

function PubMedResultsBar({ articles, onClear }: { articles: PubMedArticle[]; onClear: () => void }) {
  const [expanded, setExpanded] = useState(false);

  return (
    <div className="mb-2 rounded-lg border border-emerald-800/40 bg-emerald-950/30 overflow-hidden">
      <button
        onClick={() => setExpanded(v => !v)}
        className="w-full flex items-center gap-2 px-2.5 py-1.5 text-[11px] hover:bg-emerald-900/20 transition-colors"
      >
        {expanded ? <ChevronDown className="w-3 h-3 text-emerald-500" /> : <ChevronRight className="w-3 h-3 text-emerald-500" />}
        <BookMarked className="w-3 h-3 text-emerald-400" />
        <span className="text-emerald-300 font-medium">{articles.length} PubMed articles found</span>
        <span className="ml-auto text-emerald-600 hover:text-emerald-400 text-[10px]" onClick={(e) => { e.stopPropagation(); onClear(); }}>
          Clear
        </span>
      </button>
      {expanded && (
        <div className="px-2.5 pb-2 space-y-1.5 max-h-[200px] overflow-y-auto">
          {articles.map((a, i) => (
            <div key={a.pmid} className="flex gap-2 py-1 border-t border-emerald-900/30">
              <span className="text-[10px] text-emerald-500 font-mono shrink-0 mt-0.5">[{i + 1}]</span>
              <div className="min-w-0">
                <p className="text-[11px] text-zinc-300 leading-snug line-clamp-2">{a.title}</p>
                <p className="text-[10px] text-zinc-500 truncate mt-0.5">
                  {a.authors.split(',').slice(0, 3).join(',')}
                  {a.authors.split(',').length > 3 ? ' et al.' : ''} — {a.journal} ({a.year})
                </p>
                <a
                  href={a.url}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="inline-flex items-center gap-0.5 text-[10px] text-emerald-500 hover:text-emerald-300 mt-0.5"
                  onClick={(e) => e.stopPropagation()}
                >
                  PMID: {a.pmid} <ExternalLink className="w-2.5 h-2.5" />
                </a>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

// --- Main Chat Panel ---

export function ChatPanel() {
  const { projectPath, openBinaryFile } = useProject();
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [input, setInput] = useState('');
  const [isStreaming, setIsStreaming] = useState(false);
  const [streamStalled, setStreamStalled] = useState(false);
  const lastEventTime = useRef<number>(0);
  const [sessionId, setSessionId] = useState(() => crypto.randomUUID());
  const [claudeSessionId, setClaudeSessionId] = useState<string | null>(null);
  const [totalCost, setTotalCost] = useState(0);
  const [model, setModel] = useState('claude-sonnet-4-20250514');

  // Load default model from user settings
  useEffect(() => {
    invoke<{ model?: string }>('get_settings')
      .then((settings) => {
        if (settings.model) setModel(settings.model);
      })
      .catch(() => {});
  }, []);

  const [authState, setAuthState] = useState<{ authenticated: boolean; method: string } | null>(null);
  const [mode, setMode] = useState<ClaudeMode>('agent');
  const [remoteInfo, setRemoteInfo] = useState<RemoteInfo | null>(null);
  const [existingPlan, setExistingPlan] = useState<string | null>(null);
  // Report mode state
  const [reportPhase, setReportPhase] = useState<ReportPhase>('idle');
  const [reportScan, setReportScan] = useState<ProjectScan | null>(null);
  const [reportSelectedFiles, setReportSelectedFiles] = useState<string[]>([]);
  const [reportFilePreviews, setReportFilePreviews] = useState<FilePreview[]>([]);
  const [reportMethodsInfo, setReportMethodsInfo] = useState<MethodsInfo | null>(null);
  const [reportOutputPath, setReportOutputPath] = useState<string | null>(null);
  const [reportError, setReportError] = useState<string | null>(null);
  const [reportLoading, setReportLoading] = useState(false);
  const [reportScope, setReportScope] = useState<'comprehensive' | 'focused'>('comprehensive');
  const [reportSelectedPlan, setReportSelectedPlan] = useState<string | undefined>(undefined);
  const [planReady, setPlanReady] = useState(false); // true when plan is written and awaiting approval
  const [planHistory, setPlanHistory] = useState<PlanHistoryEntry[]>([]);
  const [showPlanHistory, setShowPlanHistory] = useState(false);
  // Plan conflict resolution: when user requests a new plan while one exists
  const [planConflict, setPlanConflict] = useState<{ pendingRequest: string } | null>(null);
  const [activeProtocol, setActiveProtocol] = useState<{ id: string; name: string } | null>(null);
  const [protocolContent, setProtocolContent] = useState<string | null>(null);
  const [useTerminal, setUseTerminal] = useState(true); // Default ON for HPC use
  const [sshTerminalId, setSshTerminalId] = useState<string | null>(null);
  const [previousSessions, setPreviousSessions] = useState<SessionMetadata[]>([]);
  const [showResumeModal, setShowResumeModal] = useState(false);
  // Session resume is disabled — it consistently fails for both local and remote modes.
  // The output files are often missing, stale, or the SSH connection times out.
  // TODO: Re-enable once session persistence is reliable.
  const [resumeChecked, setResumeChecked] = useState(true);
  // Remote Claude Code status
  const [remoteDeps, setRemoteDeps] = useState<{
    checked: boolean;
    hasNode: boolean;
    hasClaude: boolean;
    hasAuth: boolean | null; // null = not checked yet
    installing: boolean;
    error: string | null;
  } | null>(null);

  // Voice dictation via native macOS speech recognition
  const [isDictating, setIsDictating] = useState(false);
  // PubMed knowledge base — disabled by default to avoid irrelevant searches
  // for non-scientific queries (e.g. "list files in this folder"). Users can
  // toggle it on via the PubMed button when they need literature grounding.
  const [pubmedEnabled, setPubmedEnabled] = useState(false);
  const [pubmedSearching, setPubmedSearching] = useState(false);
  const [lastPubmedResults, setLastPubmedResults] = useState<PubMedArticle[] | null>(null);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  // Track which assistant message IDs we've already seen, to handle
  // multi-turn conversations and avoid duplicates within a turn
  const seenMsgIds = useRef<Set<string>>(new Set());
  const modeRef = useRef(mode);
  modeRef.current = mode;
  const remoteInfoRef = useRef(remoteInfo);
  remoteInfoRef.current = remoteInfo;
  const projectPathRef = useRef(projectPath);
  projectPathRef.current = projectPath;
  // Pin the working directory for the lifetime of a session so that folder
  // navigation in the sidebar doesn't break an active streaming session.
  // Set on the first message; cleared when the session is reset.
  const sessionProjectPath = useRef<string | null>(null);
  const reportPhaseRef = useRef<ReportPhase>(reportPhase);
  reportPhaseRef.current = reportPhase;
  const reportSelectedFilesRef = useRef(reportSelectedFiles);
  reportSelectedFilesRef.current = reportSelectedFiles;
  const reportFilePreviewsRef = useRef(reportFilePreviews);
  reportFilePreviewsRef.current = reportFilePreviews;
  const reportMethodsInfoRef = useRef(reportMethodsInfo);
  reportMethodsInfoRef.current = reportMethodsInfo;
  const messagesRef = useRef(messages);
  messagesRef.current = messages;

  // Ref for the report generation trigger — defined later, called from both
  // the `result` event (reliable) and `claude-done` event (backup).
  const triggerReportGenerationRef = useRef<(() => void) | null>(null);

  // @-mention state
  const [mentionActive, setMentionActive] = useState(false);
  const [mentionQuery, setMentionQuery] = useState('');
  const [mentionItems, setMentionItems] = useState<MentionItem[]>([]);
  const [mentionIndex, setMentionIndex] = useState(0);
  const [mentionCursorStart, setMentionCursorStart] = useState(0); // position of '@' in input
  const [mentions, setMentions] = useState<MentionRef[]>([]);      // accumulated mentions for current message
  const [attachments, setAttachments] = useState<Array<{ name: string; path: string; type: 'file' | 'image' }>>([]);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const mentionDebounce = useRef<ReturnType<typeof setTimeout> | null>(null);
  // Cache directory listings to avoid repeated SSH calls (key = dir path, value = entries + timestamp)
  const dirCache = useRef<Map<string, { entries: MentionItem[]; ts: number }>>(new Map());

  // Project file index — cached manifest of all files in the project
  const projectIndex = useRef<string | null>(null);
  const projectIndexPath = useRef<string | null>(null); // track which path was indexed

  // Dictation event listeners — receive transcribed text from native macOS speech recognition
  // We store the text that existed BEFORE dictation started, so we can replace only the dictated portion.
  const preDictationText = useRef('');

  useEffect(() => {
    let unlistenResult: UnlistenFn | null = null;
    let unlistenDone: UnlistenFn | null = null;
    let unlistenError: UnlistenFn | null = null;

    const setup = async () => {
      unlistenResult = await listen('dictation-result', (event: any) => {
        const { text, isFinal } = event.payload;
        if (text) {
          // SFSpeechRecognizer sends the FULL cumulative transcription each time,
          // so we replace everything after the pre-dictation text (not append).
          const base = preDictationText.current;
          const separator = base && !base.endsWith(' ') ? ' ' : '';
          setInput(base + separator + text + (isFinal ? ' ' : ''));
        }
      });

      unlistenDone = await listen('dictation-done', () => {
        setIsDictating(false);
      });

      unlistenError = await listen('dictation-error', (event: any) => {
        alert(event.payload);
        setIsDictating(false);
      });
    };
    setup();

    return () => {
      unlistenResult?.();
      unlistenDone?.();
      unlistenError?.();
    };
  }, []);

  // Start a fresh chat session
  const resetChat = useCallback(() => {
    if (isStreaming) {
      invoke('stop_claude_session', { sessionId }).catch(() => {});
    }
    setMessages([]);
    setInput('');
    setIsStreaming(false);
    setSessionId(crypto.randomUUID());
    setClaudeSessionId(null);
    setTotalCost(0);
    setMentions([]);
    setMentionActive(false);
    setMentionItems([]);
    setShowResumeModal(false);
    setPreviousSessions([]);
    setResumeChecked(true); // Don't re-check sessions immediately — user explicitly started fresh
    setActiveProtocol(null);
    setProtocolContent(null);
    setExistingPlan(null);
    setPlanReady(false);
    seenMsgIds.current.clear();
    dirCache.current.clear();
    projectIndex.current = null;
    projectIndexPath.current = null;
    sessionProjectPath.current = null;
  }, [isStreaming, sessionId]);

  // @-mention: search files when query changes (with caching + adaptive debounce)
  useEffect(() => {
    if (!mentionActive || mentionQuery === undefined) {
      setMentionItems([]);
      return;
    }

    const isRemote = !!remoteInfo;
    const debounceMs = isRemote ? 400 : 120; // Longer debounce for SSH
    const CACHE_TTL = 30000; // 30s cache for directory listings

    if (mentionDebounce.current) clearTimeout(mentionDebounce.current);
    mentionDebounce.current = setTimeout(async () => {
      const basePath = remoteInfo?.remotePath || projectPath;
      if (!basePath) { setMentionItems([]); return; }

      const lastSlash = mentionQuery.lastIndexOf('/');
      const searchDir = lastSlash >= 0
        ? `${basePath}/${mentionQuery.slice(0, lastSlash)}`
        : basePath;
      const filter = lastSlash >= 0 ? mentionQuery.slice(lastSlash + 1).toLowerCase() : mentionQuery.toLowerCase();

      // Check cache first
      const cached = dirCache.current.get(searchDir);
      if (cached && (Date.now() - cached.ts) < CACHE_TTL) {
        const filtered = cached.entries
          .filter(e => !filter || e.name.toLowerCase().includes(filter))
          .slice(0, 15);
        setMentionItems(filtered);
        setMentionIndex(0);
        return;
      }

      try {
        let entries: Array<{ name: string; path: string; is_dir: boolean; size: number; extension?: string }>;
        if (isRemote) {
          entries = await invoke<typeof entries>('list_remote_directory', {
            profileId: remoteInfo.profileId,
            path: searchDir,
            showHidden: false,
          });
        } else {
          entries = await invoke<typeof entries>('list_directory', {
            path: searchDir,
            showHidden: false,
          });
        }
        const allItems = entries.map(e => ({
          name: e.name,
          path: e.path,
          isDir: e.is_dir,
          extension: e.extension ?? undefined,
        }));

        // Cache the full listing
        dirCache.current.set(searchDir, { entries: allItems, ts: Date.now() });

        const filtered = allItems
          .filter(e => !filter || e.name.toLowerCase().includes(filter))
          .slice(0, 15);
        setMentionItems(filtered);
        setMentionIndex(0);
      } catch {
        setMentionItems([]);
      }
    }, debounceMs);

    return () => { if (mentionDebounce.current) clearTimeout(mentionDebounce.current); };
  }, [mentionActive, mentionQuery, projectPath, remoteInfo]);

  // @-mention: insert selected item into input
  const handleMentionSelect = useCallback((item: MentionItem) => {
    // Replace @query with @name in the input text
    const before = input.slice(0, mentionCursorStart);
    const after = input.slice(textareaRef.current?.selectionStart ?? input.length);
    const mentionText = `@${item.name} `;
    setInput(before + mentionText + after);
    setMentions(prev => [...prev, { name: item.name, path: item.path, isDir: item.isDir }]);
    setMentionActive(false);
    setMentionItems([]);
    setMentionQuery('');
    // Refocus textarea
    setTimeout(() => textareaRef.current?.focus(), 0);
  }, [input, mentionCursorStart]);

  // Check for previous sessions that can be resumed
  useEffect(() => {
    if (resumeChecked) return;
    const checkPreviousSessions = async () => {
      try {
        const sessions = await invoke<SessionMetadata[]>('list_sessions', {
          projectPath: remoteInfo?.remotePath || projectPath || null,
          profileId: remoteInfo?.profileId || null,
        });
        // Only show sessions that are running or recently completed (last 24h)
        const recent = sessions.filter((s) => {
          const age = Date.now() - s.last_activity;
          return s.status === 'running' || (s.status === 'completed' && age < 24 * 60 * 60 * 1000);
        });
        if (recent.length > 0) {
          setPreviousSessions(recent);
          setShowResumeModal(true);
        }
        setResumeChecked(true);
      } catch {
        setResumeChecked(true);
      }
    };
    if (remoteInfo || projectPath) {
      checkPreviousSessions();
    }
  }, [remoteInfo, projectPath, resumeChecked]);

  // Resume a previous session
  const handleResumeSession = useCallback(async (meta: SessionMetadata) => {
    setShowResumeModal(false);
    setIsStreaming(true);

    // Restore the claude session ID for --resume on next message
    if (meta.claude_session_id) {
      setClaudeSessionId(meta.claude_session_id);
    }

    try {
      const remote = meta.remote_path && meta.profile_id
        ? { profileId: meta.profile_id, remotePath: meta.remote_path }
        : undefined;

      // Check file status first (with timeout to avoid hanging on stale SSH)
      const statusTimeout = new Promise<never>((_, reject) =>
        setTimeout(() => reject(new Error('Timed out checking session status')), 15000)
      );
      const status = await Promise.race([
        invoke<SessionFileStatus>('check_session_files', {
          sessionId: meta.session_id,
          remote: remote ?? null,
        }),
        statusTimeout,
      ]);

      if (status.is_running) {
        // Agent still running — reconnect the tail stream
        // Pass both old session ID (to find files) and current session ID (for event channels)
        await invoke('reconnect_session', {
          sessionId: meta.session_id,
          eventSessionId: sessionId,
          remote: remote ?? null,
        });
      } else if (status.is_completed) {
        // Agent finished — read all output and hydrate messages
        const output = await invoke<string>('read_session_output', {
          sessionId: meta.session_id,
          remote: remote ?? null,
        });
        // Parse each JSONL line and emit as events to reuse existing handler
        for (const line of output.split('\n')) {
          if (!line.trim()) continue;
          try {
            const data = JSON.parse(line) as ClaudeEvent;
            if (data.type === 'system' && 'session_id' in data && data.session_id) {
              setClaudeSessionId(data.session_id);
            }
            if (data.type === 'assistant' && 'message' in data) {
              const msgId = data.message.id || crypto.randomUUID();
              const blocks: ContentBlock[] = data.message.content.map((c) => {
                if (c.type === 'text') return { type: 'text' as const, text: c.text };
                if (c.type === 'thinking' && 'thinking' in c) return { type: 'thinking' as const, thinking: (c as { type: 'thinking'; thinking: string }).thinking };
                return {
                  type: 'tool_use' as const,
                  id: (c as { id: string }).id,
                  name: (c as { name: string }).name,
                  input: (c as { input: Record<string, unknown> }).input,
                  status: 'complete' as const,
                };
              });
              // Add each unique message
              if (!seenMsgIds.current.has(msgId)) {
                seenMsgIds.current.add(msgId);
                setMessages((prev) => [
                  ...prev,
                  {
                    id: msgId,
                    role: 'assistant' as const,
                    content: blocks,
                    timestamp: Date.now(),
                  },
                ]);
              } else {
                // Update existing message with latest content
                setMessages((prev) =>
                  prev.map((m) =>
                    m.id === msgId ? { ...m, content: blocks } : m
                  )
                );
              }
            }
          } catch {
            // Skip non-JSON lines
          }
        }
        setIsStreaming(false);
        // Add a system message indicating this is a resumed session
        setMessages((prev) => [
          ...prev,
          {
            id: crypto.randomUUID(),
            role: 'system' as const,
            content: [{ type: 'text' as const, text: 'Previous session loaded. Send a message to continue the conversation.' }],
            timestamp: Date.now(),
          },
        ]);
      } else {
        setIsStreaming(false);
        // No output file found
        setMessages((prev) => [
          ...prev,
          {
            id: crypto.randomUUID(),
            role: 'system' as const,
            content: [{ type: 'text' as const, text: 'Previous session output not found. Starting a new session.' }],
            timestamp: Date.now(),
          },
        ]);
      }
    } catch (e) {
      setIsStreaming(false);
      setMessages((prev) => [
        ...prev,
        {
          id: crypto.randomUUID(),
          role: 'system' as const,
          content: [{ type: 'text' as const, text: `Failed to resume session: ${e}` }],
          timestamp: Date.now(),
        },
      ]);
    }
  }, [sessionId]);

  const handleDismissResume = () => {
    setShowResumeModal(false);
  };

  // Check for existing implementation_plan.md whenever path changes
  useEffect(() => {
    const checkPlan = async () => {
      try {
        const remote = remoteInfo
          ? { profileId: remoteInfo.profileId, remotePath: remoteInfo.remotePath }
          : undefined;
        const plan = await invoke<string>('check_existing_plan', {
          projectPath: projectPath || '.',
          remote: remote ?? null,
        });
        setExistingPlan(plan || null);
        // Also load plan history (local only for now)
        if (!remoteInfo && projectPath) {
          const history = await listPlanHistory(projectPath);
          setPlanHistory(history);
        }
      } catch {
        setExistingPlan(null);
      }
    };
    checkPlan();
  }, [remoteInfo, projectPath]);

  // Check auth status on mount (API key or OAuth)
  useEffect(() => {
    invoke<{ authenticated: boolean; method: string }>('check_auth_status').then((status) => {
      setAuthState(status);
    }).catch(() => setAuthState({ authenticated: false, method: 'none' }));
  }, []);

  // Listen for SSH connection events to know when we're in remote mode
  useEffect(() => {
    const unlisteners: (() => void)[] = [];

    listen<{
      terminalId: string;
      profileId?: string;
      profileName?: string;
    }>('open-ssh-terminal', (event) => {
      const { terminalId, profileId, profileName } = event.payload;
      if (profileId && profileName) {
        setRemoteInfo((prev) => ({
          profileId,
          profileName,
          remotePath: prev?.remotePath || '~',
        }));
        setSshTerminalId(terminalId);
      }
    }).then((u) => unlisteners.push(u));

    // Listen for remote path changes from the explorer
    listen<{ profileId: string; profileName?: string; remotePath: string }>('remote-path-changed', (event) => {
      const { profileId, profileName, remotePath } = event.payload;
      setRemoteInfo((prev) => {
        // If we already have remote info, update path (and optionally name)
        if (prev) return { ...prev, profileId, remotePath, ...(profileName ? { profileName } : {}) };
        // If no remote info yet, create it (explorer is browsing remote files)
        if (profileName) return { profileId, profileName, remotePath };
        return null; // Can't create without profile name
      });
    }).then((u) => unlisteners.push(u));

    // Listen for protocol activation from sidebar
    listen<{ id: string; name: string } | null>('protocol-changed', async (event) => {
      const protocol = event.payload;
      if (protocol) {
        setActiveProtocol(protocol);
        try {
          const content = await invoke<string>('read_protocol', { protocolId: protocol.id });
          setProtocolContent(content);
        } catch {
          setProtocolContent(null);
        }
      } else {
        setActiveProtocol(null);
        setProtocolContent(null);
      }
    }).then((u) => unlisteners.push(u));

    return () => unlisteners.forEach((u) => u());
  }, []);

  // Auto-check remote server for Claude Code + auth when connecting
  useEffect(() => {
    if (!remoteInfo?.profileId) {
      setRemoteDeps(null);
      return;
    }

    let cancelled = false;
    const checkRemote = async () => {
      try {
        const status = await invoke<{
          xcode_cli: boolean;
          node: boolean;
          node_version: string | null;
          npm: boolean;
          npm_version: string | null;
          claude_code: boolean;
          claude_version: string | null;
        }>('check_remote_claude', { profileId: remoteInfo.profileId });

        if (cancelled) return;

        // If Claude Code is installed, check authentication on the remote server
        let hasAuth: boolean | null = null;
        if (status.claude_code) {
          try {
            // Always check remote auth first
            const authResult = await invoke<string>('check_remote_claude_auth', { profileId: remoteInfo.profileId });
            hasAuth = authResult === 'authenticated';
            // If remote has no auth, check for a local API key that gets forwarded
            if (!hasAuth) {
              const localAuth = await invoke<{ authenticated: boolean; method: string }>('check_auth_status');
              if (localAuth.authenticated && localAuth.method === 'api_key') {
                hasAuth = true;
              }
            }
          } catch {
            hasAuth = null; // couldn't determine
          }
        }

        if (!cancelled) {
          setRemoteDeps({
            checked: true,
            hasNode: status.node,
            hasClaude: status.claude_code,
            hasAuth,
            installing: false,
            error: null,
          });
        }
      } catch (err) {
        if (!cancelled) {
          setRemoteDeps({
            checked: true,
            hasNode: false,
            hasClaude: false,
            hasAuth: null,
            installing: false,
            error: `Could not check server: ${err}`,
          });
        }
      }
    };

    checkRemote();
    return () => { cancelled = true; };
  }, [remoteInfo?.profileId]);

  // Build project file index when project path changes
  useEffect(() => {
    const currentPath = remoteInfo?.remotePath || projectPath;
    if (!currentPath || currentPath === projectIndexPath.current) return;

    const buildIndex = async () => {
      try {
        let entries: Array<{ path: string; size: number; is_dir: boolean; extension?: string }>;
        if (remoteInfo) {
          entries = await invoke<typeof entries>('index_remote_project', {
            profileId: remoteInfo.profileId,
            remotePath: currentPath,
          });
        } else {
          entries = await invoke<typeof entries>('index_project', { rootPath: currentPath });
        }

        // Build compact manifest string
        const lines = entries.map(e => {
          if (e.is_dir) {
            return `${e.path} (${e.size} items)`;
          }
          const sizeStr = e.size < 1024 ? `${e.size}B`
            : e.size < 1048576 ? `${(e.size / 1024).toFixed(1)}KB`
            : `${(e.size / 1048576).toFixed(1)}MB`;
          return `${e.path} (${sizeStr})`;
        });
        projectIndex.current = lines.join('\n');
        projectIndexPath.current = currentPath;
      } catch {
        projectIndex.current = null;
        projectIndexPath.current = currentPath;
      }
    };

    buildIndex();
  }, [projectPath, remoteInfo]);

  // Auto-scroll
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages]);

  // Watchdog: detect stalled remote streams (no events for 90 seconds).
  // On remote SSH, the connection can drop silently (network hiccup, SSH timeout,
  // NFS stall) leaving the UI stuck in streaming mode with no way to recover.
  useEffect(() => {
    if (!isStreaming || !remoteInfo) {
      setStreamStalled(false);
      return;
    }
    const interval = setInterval(() => {
      if (lastEventTime.current > 0 && Date.now() - lastEventTime.current > 90_000) {
        setStreamStalled(true);
      }
    }, 10_000);
    return () => clearInterval(interval);
  }, [isStreaming, remoteInfo]);

  // Listen for Claude events
  useEffect(() => {
    const unlisteners: UnlistenFn[] = [];

    listen<{ line: string }>(`claude-event-${sessionId}`, (event) => {
      lastEventTime.current = Date.now();
      setStreamStalled(false);
      const line = event.payload.line;
      try {
        const data = JSON.parse(line) as ClaudeEvent;

        if (data.type === 'system' && 'session_id' in data && data.session_id) {
          setClaudeSessionId(data.session_id);
          // Persist the Claude session ID so it survives app restarts
          invoke('update_session_claude_id', {
            sessionId,
            claudeSessionId: data.session_id,
          }).catch(() => {}); // Best-effort
        }

        if (data.type === 'assistant' && 'message' in data) {
          const msgId = data.message.id || crypto.randomUUID();
          const isNewMsg = !seenMsgIds.current.has(msgId);

          // Parse content blocks from this assistant event
          const newBlocks: ContentBlock[] = data.message.content.map((c) => {
            if (c.type === 'text') {
              return { type: 'text' as const, text: c.text };
            }
            if (c.type === 'thinking' && 'thinking' in c) {
              return { type: 'thinking' as const, thinking: c.thinking };
            }
            return {
              type: 'tool_use' as const,
              id: c.id,
              name: c.name,
              input: c.input as Record<string, unknown>,
              status: 'running' as const,
            };
          });

          // Tag each block with the turn's message ID so we can distinguish
          // blocks from previous turns vs the current turn during content updates.
          const taggedNewBlocks = newBlocks.map((b) => ({ ...b, _turnId: msgId }));

          if (isNewMsg) {
            // First time seeing this message ID — it's a new turn
            seenMsgIds.current.add(msgId);

            setMessages((prev) => {
              const existingIdx = prev.findIndex(
                (m) => m.role === 'assistant' && m.isStreaming
              );

              if (existingIdx >= 0) {
                // Append new turn's blocks to the existing streaming message
                const existing = prev[existingIdx];
                const updated = [...prev];
                updated[existingIdx] = {
                  ...existing,
                  content: [...existing.content, ...taggedNewBlocks],
                };
                return updated;
              }

              // No streaming message yet — create one
              return [
                ...prev,
                {
                  id: crypto.randomUUID(),
                  role: 'assistant',
                  content: taggedNewBlocks,
                  timestamp: Date.now(),
                  isStreaming: true,
                },
              ];
            });
          } else {
            // Same message ID seen again — replace THIS turn's blocks (content update)
            setMessages((prev) => {
              const existingIdx = prev.findIndex(
                (m) => m.role === 'assistant' && m.isStreaming
              );
              if (existingIdx < 0) return prev;

              const existing = prev[existingIdx];

              // Keep blocks from PREVIOUS turns (those with a different _turnId).
              // This preserves text, thinking, and tool_use blocks from earlier turns
              // while replacing only the current turn's content.
              const prevTurnBlocks = existing.content.filter((b) => {
                const blockTurnId = (b as ContentBlock & { _turnId?: string })._turnId;
                return blockTurnId !== undefined && blockTurnId !== msgId;
              });

              // Preserve tool results for blocks that already have results
              const mergedNewBlocks = taggedNewBlocks.map((block) => {
                if (block.type === 'tool_use') {
                  const prevBlock = existing.content.find(
                    (b) => b.type === 'tool_use' && b.id === (block as ToolUseBlock).id
                  );
                  if (prevBlock && prevBlock.type === 'tool_use' && prevBlock.result) {
                    return { ...block, result: prevBlock.result, status: prevBlock.status };
                  }
                }
                return block;
              });

              const updated = [...prev];
              updated[existingIdx] = {
                ...existing,
                content: [...prevTurnBlocks, ...mergedNewBlocks],
              };
              return updated;
            });
          }
        }

        if (data.type === 'tool' && 'tool_use_id' in data) {
          setMessages((prev) =>
            prev.map((msg) => ({
              ...msg,
              content: msg.content.map((block) =>
                block.type === 'tool_use' && block.id === data.tool_use_id
                  ? { ...block, result: data.content, status: 'complete' as const }
                  : block,
              ),
            })),
          );
        }

        if (data.type === 'result') {
          setIsStreaming(false);
          if ('cost_usd' in data && data.cost_usd) {
            setTotalCost((prev) => prev + data.cost_usd!);
          }
          // Mark ALL remaining running/pending tool blocks as complete
          // (some tool result events may have been missed or arrived out of order)
          setMessages((prev) =>
            prev.map((msg) => ({
              ...msg,
              isStreaming: false,
              content: msg.content.map((block) =>
                block.type === 'tool_use' && (block.status === 'running' || block.status === 'pending')
                  ? { ...block, status: 'complete' as const }
                  : block,
              ),
            })),
          );

          // Plan mode: detect plan file after Claude finishes
          // Use the pinned session path — the plan file was written there, not
          // wherever the sidebar may have navigated to since.
          if (modeRef.current === 'plan') {
            const basePath = remoteInfoRef.current?.remotePath || sessionProjectPath.current || projectPathRef.current || '.';
            const planPath = `${basePath}/implementation_plan.md`;
            // Delay slightly to let file writes flush
            setTimeout(async () => {
              try {
                let content: string;
                if (remoteInfoRef.current) {
                  content = await invoke<string>('read_remote_file', {
                    profileId: remoteInfoRef.current.profileId,
                    path: planPath,
                  });
                } else {
                  content = await invoke<string>('read_file', { path: planPath });
                }
                if (content.trim()) {
                  setExistingPlan(content);
                  setPlanReady(true);
                  emit('open-file', {
                    path: planPath,
                    ...(remoteInfoRef.current ? { profileId: remoteInfoRef.current.profileId } : {}),
                  });
                }
              } catch {
                // File doesn't exist — extract from chat text as fallback
                setMessages((prev) => {
                  const planText = prev
                    .filter(m => m.role === 'assistant')
                    .flatMap(m => m.content.filter(b => b.type === 'text').map(b => (b as { type: 'text'; text: string }).text))
                    .join('\n\n');
                  if (planText.trim() && planText.length > 100) {
                    setExistingPlan(planText);
                    setPlanReady(true);
                    if (remoteInfoRef.current) {
                      // Remote: write via SSH and open
                      invoke('write_remote_file', {
                        profileId: remoteInfoRef.current.profileId,
                        path: planPath,
                        content: planText,
                      }).then(() => {
                        emit('open-file', { path: planPath, profileId: remoteInfoRef.current!.profileId });
                      }).catch(() => {});
                    } else {
                      invoke('write_file', { path: planPath, content: planText }).then(() => {
                        emit('open-file', { path: planPath });
                      }).catch(() => {});
                    }
                  }
                  return prev;
                });
              }
            }, 1000);
          }

          // Report mode: trigger report generation from the result event.
          // Only auto-trigger when in 'draft' phase (user already clicked
          // "Generate Report").  During 'clarify' phase, let the user decide
          // when they've provided enough context — the button will appear once
          // streaming finishes.
          if (modeRef.current === 'report' && reportPhaseRef.current === 'draft') {
            setTimeout(() => {
              triggerReportGenerationRef.current?.();
            }, 500);
          }
        }

        // Handle errors from SSH/remote execution
        if (data.type === 'error') {
          setIsStreaming(false);
          const errMsg = data.error.message;
          setMessages((prev) => [
            ...prev.map((msg) => (msg.isStreaming ? { ...msg, isStreaming: false } : msg)),
            {
              id: crypto.randomUUID(),
              role: 'system' as const,
              content: [{ type: 'text' as const, text: `Remote error: ${errMsg}` }],
              timestamp: Date.now(),
            },
          ]);
        }
      } catch {
        // Unparseable line, ignore
      }
    }).then((u) => unlisteners.push(u));

    listen(`claude-done-${sessionId}`, () => {
      setIsStreaming(false);
      // Mark all remaining running/pending tool blocks as complete
      setMessages((prev) =>
        prev.map((msg) => ({
          ...msg,
          isStreaming: false,
          content: msg.content.map((block) =>
            block.type === 'tool_use' && (block.status === 'running' || block.status === 'pending')
              ? { ...block, status: 'complete' as const }
              : block,
          ),
        })),
      );
      invoke('update_session_status', {
        sessionId,
        status: 'completed',
      }).catch(() => {});

      // If in plan mode, check for implementation_plan.md and show approval UI
      // Use the pinned session path — the plan file was written there.
      if (modeRef.current === 'plan') {
        const basePath = remoteInfoRef.current?.remotePath || sessionProjectPath.current || projectPathRef.current || '.';
        const planPath = `${basePath}/implementation_plan.md`;

        // Try to read the plan file (Claude Code writes it during plan mode)
        const tryReadPlan = async () => {
          try {
            let content: string;
            if (remoteInfoRef.current) {
              content = await invoke<string>('read_remote_file', {
                profileId: remoteInfoRef.current.profileId,
                path: planPath,
              });
            } else {
              content = await invoke<string>('read_file', { path: planPath });
            }
            if (content.trim()) {
              setExistingPlan(content);
              setPlanReady(true);
              emit('open-file', {
                path: planPath,
                ...(remoteInfoRef.current ? { profileId: remoteInfoRef.current.profileId } : {}),
              });
            }
          } catch {
            // Plan file not found — Claude may have output it as text instead
            // Fall back to extracting from messages via ref (avoids React 18 batching issues)
            const planText = messagesRef.current
              .filter(m => m.role === 'assistant')
              .flatMap(m => m.content.filter(b => b.type === 'text').map(b => (b as { type: 'text'; text: string }).text))
              .join('\n\n');
            if (planText.trim()) {
              setExistingPlan(planText);
              setPlanReady(true);
              if (remoteInfoRef.current) {
                invoke('write_remote_file', {
                  profileId: remoteInfoRef.current.profileId,
                  path: planPath,
                  content: planText,
                }).then(() => {
                  emit('open-file', { path: planPath, profileId: remoteInfoRef.current!.profileId });
                }).catch(() => {});
              } else {
                invoke('write_file', { path: planPath, content: planText }).then(() => {
                  emit('open-file', { path: planPath });
                }).catch(() => {});
              }
            }
          }
        };
        // Small delay to let Claude Code finish writing the file
        setTimeout(tryReadPlan, 500);
      }

      // Report mode: trigger generation only during draft phase (user clicked Generate Report)
      if (modeRef.current === 'report' && reportPhaseRef.current === 'draft') {
        triggerReportGenerationRef.current?.();
      }
    }).then((u) => unlisteners.push(u));

    return () => unlisteners.forEach((u) => u());
  }, [sessionId]);

  // ── Report Generation Logic (extracted so it can be called from both
  //    the `result` event and `claude-done` event) ──
  triggerReportGenerationRef.current = () => {
      // Only generate when the user has explicitly clicked "Generate Report"
      // (which sets phase to 'draft').  Never auto-trigger during 'clarify'.
      if (!(modeRef.current === 'report' && reportPhaseRef.current === 'draft')) return;
      {
        // Extract the latest CLAUDE-generated assistant message (not system-generated ones
        // like the scan summary or clarify prompt). This ensures we get Claude's actual
        // report draft, not the frontend-generated scaffolding messages.
        let draftText = '';
        const currentMessages = messagesRef.current;
        const claudeMsgs = currentMessages.filter(m => m.role === 'assistant' && !m.systemGenerated);
        if (claudeMsgs.length > 0) {
          const lastMsg = claudeMsgs[claudeMsgs.length - 1];
          draftText = lastMsg.content
            .filter(b => b.type === 'text')
            .map(b => (b as { type: 'text'; text: string }).text)
            .join('\n\n');
        }

        const shouldGenerate = draftText.trim().length > 0;

        if (shouldGenerate && draftText.trim()) {
          // Guard against duplicate execution: setReportPhase is async (React batching),
          // so if the done event fires twice before the state flushes, both calls would
          // pass the !== 'render' check. Use the ref for an immediate synchronous guard.
          // Re-check after potential async gap — the ref may have been mutated by a concurrent call.
          // TypeScript narrows this out due to the check on line 2050, but at runtime the ref can change.
          const currentPhase = reportPhaseRef.current as ReportPhase;
          if (currentPhase === 'render' || currentPhase === 'done') return;
          reportPhaseRef.current = 'render'; // Immediate sync guard
          setReportPhase('render');

          setTimeout(async () => {
            try {
              const remote = remoteInfoRef.current;
              const basePath = remote?.remotePath || sessionProjectPath.current || projectPathRef.current || '.';
              const filename = generateReportFilename();

              // ── Step 1: Always save markdown file as primary output ──
              const mdFilename = filename.replace(/\.pdf$/i, '.md');
              const mdPath = `${basePath}/${mdFilename}`;
              try {
                if (remote) {
                  // Remote mode: write via SSH
                  await invoke('write_remote_file', {
                    profileId: remote.profileId,
                    path: mdPath,
                    content: draftText,
                  });
                } else {
                  await invoke('write_file', { path: mdPath, content: draftText });
                }
              } catch (mdErr) {
                console.error('Failed to save markdown:', mdErr);
              }

              // Parse Claude's draft into structured sections
              const parseSection = (text: string, heading: string): string => {
                const patterns = [
                  new RegExp(`##?\\s*${heading}[\\s\\S]*?\\n([\\s\\S]*?)(?=\\n##?\\s|$)`, 'i'),
                  new RegExp(`\\*\\*${heading}\\*\\*[\\s\\S]*?\\n([\\s\\S]*?)(?=\\n\\*\\*|\\n##?\\s|$)`, 'i'),
                ];
                for (const pat of patterns) {
                  const match = text.match(pat);
                  if (match?.[1]?.trim()) return match[1].trim();
                }
                return '';
              };

              const title = parseSection(draftText, 'Title') || draftText.match(/^#\s+(.+)/m)?.[1] || 'Analysis Report';
              const abstractText = parseSection(draftText, 'Abstract') || '';
              const introduction = parseSection(draftText, 'Introduction') || '';
              const results = parseSection(draftText, 'Results') || draftText.slice(0, 2000);
              const discussion = parseSection(draftText, 'Discussion') || '';
              const methodsText = parseSection(draftText, 'Methods') || '';

              // Extract references
              const refSection = parseSection(draftText, 'References');
              const references: Array<{ pmid: string; title: string; authors: string; journal: string; year: string; doi: string }> = [];
              if (refSection) {
                const refLines = refSection.split('\n').filter(l => l.trim());
                for (const line of refLines) {
                  const match = line.match(/^\[?\d+\]?\s*(.+)/);
                  if (match) {
                    const pmidMatch = line.match(/PMID:\s*(\d+)/i);
                    references.push({
                      pmid: pmidMatch?.[1] || '',
                      title: match[1].slice(0, 200),
                      authors: '',
                      journal: '',
                      year: '',
                      doi: '',
                    });
                  }
                }
              }

              // Build figures list from selected image files
              const figures = reportSelectedFilesRef.current
                .filter(p => /\.(png|jpg|jpeg|tiff?|bmp|svg)$/i.test(p))
                .map((p, i) => ({
                  path: p,
                  caption: p.split('/').pop() || `Figure ${i + 1}`,
                  label: `Figure ${i + 1}`,
                }));

              const methods = reportMethodsInfoRef.current;

              // For remote mode, generate PDF locally in /tmp, then SCP to remote.
              // Figures won't embed (they're remote paths), but text content will render.
              const localTmpDir = remote ? '/tmp/operon-report' : basePath;
              const config = {
                filename,
                output_dir: localTmpDir,
                title,
                date: new Date().toISOString().split('T')[0],
                abstract_text: abstractText,
                introduction,
                results,
                discussion,
                methods: {
                  overview: methodsText,
                  tools: methods?.tools || [],
                  data_sources: '',
                },
                // For remote mode, skip figures (they're remote paths the local Python can't access)
                figures: remote ? [] : figures,
                tables: [] as Array<{ title: string; headers: string[]; rows: string[][]; caption?: string }>,
                references: references.map((r, i) => ({
                  index: i + 1,
                  ...r,
                  url: r.pmid ? `https://pubmed.ncbi.nlm.nih.gov/${r.pmid}/` : '',
                })),
              };

              // ── Step 2: Try PDF generation ──
              try {
                const localResultPath = await generateReportPdf(config);

                if (remote) {
                  // Copy the locally generated PDF to the remote server
                  const remotePdfPath = `${basePath}/${filename}`;
                  await invoke('scp_to_remote', {
                    profileId: remote.profileId,
                    localPath: localResultPath,
                    remotePath: remotePdfPath,
                  });
                  setReportOutputPath(remotePdfPath);
                  setReportPhase('done');
                  setMessages((prev) => [
                    ...prev,
                    {
                      id: crypto.randomUUID(),
                      role: 'system' as const,
                      variant: 'success' as const,
                      content: [{ type: 'text' as const, text: `Report generated on remote server:\n- **PDF:** ${remotePdfPath}\n- **Markdown:** ${mdPath}` }],
                      timestamp: Date.now(),
                    },
                  ]);
                  // Auto-open the locally generated PDF (before SCP) in preview
                  try {
                    const pdfBase64 = await invoke<string>('read_file_base64', { path: localResultPath });
                    openBinaryFile(remotePdfPath, pdfBase64, 'application/pdf', 'pdf', false, remote.profileId);
                  } catch (openErr) {
                    console.warn('[Report] Could not auto-open remote PDF:', openErr);
                  }
                } else {
                  setReportOutputPath(localResultPath);
                  setReportPhase('done');
                  setMessages((prev) => [
                    ...prev,
                    {
                      id: crypto.randomUUID(),
                      role: 'system' as const,
                      variant: 'success' as const,
                      content: [{ type: 'text' as const, text: `Report generated:\n- **PDF:** ${localResultPath}\n- **Markdown:** ${mdPath}` }],
                      timestamp: Date.now(),
                    },
                  ]);
                  // Auto-open the generated PDF in the editor preview
                  try {
                    const pdfBase64 = await invoke<string>('read_file_base64', { path: localResultPath });
                    openBinaryFile(localResultPath, pdfBase64, 'application/pdf', 'pdf');
                  } catch (openErr) {
                    console.warn('[Report] Could not auto-open PDF:', openErr);
                    // Try opening markdown as fallback
                    emit('open-file', { path: mdPath });
                  }
                }
              } catch (pdfErr) {
                // PDF failed — markdown is still saved
                setReportOutputPath(mdPath);
                setReportPhase('done');
                setMessages((prev) => [
                  ...prev,
                  {
                    id: crypto.randomUUID(),
                    role: 'system' as const,
                    variant: 'info' as const,
                    content: [{
                      type: 'text' as const,
                      text: `**Report saved as Markdown:** ${mdPath}\n\nPDF generation failed: ${String(pdfErr)}\nTo enable PDF output, run: \`pip install reportlab\``,
                    }],
                    timestamp: Date.now(),
                  },
                ]);
              }
            } catch (err) {
              const errMsg = String(err);
              setReportError(`Report generation failed: ${errMsg}`);
              setReportPhase('idle');
              setMessages((prev) => [
                ...prev,
                {
                  id: crypto.randomUUID(),
                  role: 'system' as const,
                  content: [{ type: 'text' as const, text: `**Report generation failed:** ${errMsg}` }],
                  timestamp: Date.now(),
                },
              ]);
            }
          }, 1000);
        }
      }
  };

  // Helper to check remote deps + auth in one go
  const fullRemoteCheck = async (profileId: string) => {
    const status = await invoke<{
      xcode_cli: boolean; node: boolean; node_version: string | null;
      npm: boolean; npm_version: string | null;
      claude_code: boolean; claude_version: string | null;
    }>('check_remote_claude', { profileId });

    let hasAuth: boolean | null = null;
    if (status.claude_code) {
      try {
        // Always check the REMOTE server's auth status directly.
        // (A local API key doesn't help if it's not forwarded to the remote.)
        const authResult = await invoke<string>('check_remote_claude_auth', { profileId });
        console.log('[Operon] Remote auth result:', authResult);
        hasAuth = authResult === 'authenticated';

        // If remote has no auth, check if we have a local API key that gets forwarded
        if (!hasAuth) {
          const localAuth = await invoke<{ authenticated: boolean; method: string }>('check_auth_status');
          console.log('[Operon] Local auth:', JSON.stringify(localAuth));
          if (localAuth.authenticated && localAuth.method === 'api_key') {
            hasAuth = true; // API key gets forwarded to remote via env var
          }
        }
      } catch (e) {
        console.error('[Operon] Auth check error:', e);
        hasAuth = null;
      }
    }

    return { ...status, hasAuth };
  };

  // Install Claude Code on the remote server
  const installRemoteClaude = async () => {
    if (!remoteInfo?.profileId) return;
    setRemoteDeps((prev) => prev ? { ...prev, installing: true, error: null } : prev);
    try {
      await invoke('install_remote_claude', { profileId: remoteInfo.profileId });
      // Re-check after install
      const result = await fullRemoteCheck(remoteInfo.profileId);
      console.log('[Operon] Post-install check:', JSON.stringify(result));
      // After a fresh install, if auth is null (indeterminate), treat as false
      // so the auth banner always shows for fresh installs
      const hasAuth = result.hasAuth === true ? true : false;
      const newDeps = {
        checked: true,
        hasNode: result.node,
        hasClaude: result.claude_code,
        hasAuth,
        installing: false,
        error: result.claude_code ? null : 'Installation completed but Claude Code was not detected. You may need to run "source ~/.bashrc" in the terminal, then click Re-check.',
      };
      console.log('[Operon] Setting remoteDeps to:', JSON.stringify(newDeps));
      setRemoteDeps(newDeps);
    } catch (err) {
      setRemoteDeps((prev) => prev ? {
        ...prev,
        installing: false,
        error: `${err}`,
      } : prev);
    }
  };

  // Re-check remote dependencies (manual trigger)
  const recheckRemoteDeps = async () => {
    if (!remoteInfo?.profileId) return;
    setRemoteDeps((prev) => prev ? { ...prev, error: null, installing: true } : prev);
    try {
      const result = await fullRemoteCheck(remoteInfo.profileId);
      console.log('[Operon] Remote check result:', JSON.stringify(result));
      setRemoteDeps({
        checked: true,
        hasNode: result.node,
        hasClaude: result.claude_code,
        hasAuth: result.hasAuth,
        installing: false,
        error: result.hasAuth === false
          ? 'Auth check returned not_authenticated. You may need to complete the OAuth flow, then click Re-check Auth again.'
          : null,
      });
    } catch (err) {
      console.error('[Operon] Remote check error:', err);
      setRemoteDeps((prev) => prev ? { ...prev, installing: false, error: `${err}` } : prev);
    }
  };

  // Report mode: user confirmed file selection → move to clarify phase
  const reportProceedFromSelect = useCallback(async (scope: 'comprehensive' | 'focused', selectedPlanTitle?: string) => {
    if (reportPhase !== 'select' || reportSelectedFiles.length === 0) return;

    setReportScope(scope);
    setReportSelectedPlan(selectedPlanTitle);
    setReportPhase('clarify');

    // Pre-read file contents in background while user answers questions
    try {
      console.log('[Report] Pre-reading', reportSelectedFiles.length, 'file previews...');
      const previews = remoteInfo
        ? await batchReadRemoteFilePreviews(remoteInfo.profileId, reportSelectedFiles)
        : await batchReadFilePreviews(reportSelectedFiles);
      setReportFilePreviews(previews);
      console.log('[Report] Pre-read complete:', previews.length, 'previews,',
        previews.filter(p => p.content.length > 0).length, 'with content');
    } catch (err) {
      console.warn('[Report] File preview pre-read failed:', err);
      // Non-fatal — report will still work, just without inline content
    }

    const scopeLabel = scope === 'comprehensive'
      ? 'a **comprehensive** report (full paper with all sections)'
      : 'a **focused** report (key results summary)';

    const planNote = selectedPlanTitle
      ? `\n\nI'll base this on the plan: **${selectedPlanTitle}**.`
      : '';

    const systemMsg: ChatMessage = {
      id: crypto.randomUUID(),
      role: 'assistant',
      content: [{
        type: 'text',
        text: `Great, I'll generate ${scopeLabel} using those ${reportSelectedFiles.length} files.${planNote}\n\nBefore I generate the report, I need some context:\n\n` +
          `1. **What biological question** were you investigating?\n` +
          `2. **What are the key findings** from your analysis?\n` +
          `3. **Who is the audience** for this report (lab meeting, publication, collaborator)?\n` +
          `4. **Any specific figures or tables** you want highlighted?\n\n` +
          `Answer as many as you can, then type **"generate report"** when ready.`,
      }],
      timestamp: Date.now(),
      systemGenerated: true,
    };
    setMessages((prev) => [...prev, systemMsg]);
  }, [reportPhase, reportSelectedFiles, remoteInfo]);

  // Report mode: cancel/reset
  const reportCancel = useCallback(() => {
    setReportPhase('idle');
    setReportScan(null);
    setReportSelectedFiles([]);
    setReportFilePreviews([]);
    setReportMethodsInfo(null);
    setReportOutputPath(null);
    setReportError(null);
  }, []);

  const reportRescan = useCallback(async () => {
    try {
      setReportSelectedFiles([]);
      setReportFilePreviews([]);
      setReportError(null);
      console.log('[Report] Rescanning. remoteInfo:', remoteInfo, 'projectPath:', projectPath);
      const scanResult = remoteInfo
        ? await scanRemoteProjectFiles(remoteInfo.profileId, remoteInfo.remotePath || '.')
        : await scanProjectFiles(projectPath || '.');
      console.log('[Report] Rescan result:', JSON.stringify(scanResult).slice(0, 500));
      setReportScan(scanResult);

      // Re-extract methods info for local
      if (!remoteInfo && projectPath) {
        try {
          const methods = await extractMethodsInfo(projectPath);
          setReportMethodsInfo(methods);
        } catch { /* best-effort */ }
      }
    } catch (err) {
      setReportError(`Rescan failed: ${err}`);
    }
  }, [remoteInfo, projectPath]);

  // Plan conflict handlers: called when user picks an option from the conflict UI
  const handlePlanConflictChoice = useCallback(async (choice: 'new' | 'replace', archiveName?: string) => {
    if (!planConflict) return;
    const pendingText = planConflict.pendingRequest;
    setPlanConflict(null);

    if (choice === 'new') {
      // Archive the old plan with the user-confirmed name, then generate new
      const basePath = remoteInfo?.remotePath || projectPath || '.';
      const historyDir = `${basePath}/.operon/plan_history`;
      const finalName = archiveName || (() => {
        const dateMatch = existingPlan?.match(/\*\*Date:\*\*\s*(.+)/);
        if (dateMatch) return `plan_${dateMatch[1].trim().replace(/\s+/g, '_').replace(/:/g, '')}.md`;
        return `plan_${new Date().toISOString().replace(/[:.]/g, '-').slice(0, 19)}.md`;
      })();

      try {
        await invoke('create_directory', { path: historyDir });
        await invoke('write_file', { path: `${historyDir}/${finalName}`, content: existingPlan || '' });
        setMessages((prev) => [...prev, {
          id: crypto.randomUUID(), role: 'system' as const, variant: 'success' as const,
          content: [{ type: 'text' as const, text: `Old plan archived → .operon/plan_history/${finalName}` }],
          timestamp: Date.now(),
        }]);
      } catch (err) {
        console.error('[Plan] Archive failed:', err);
      }

      // Clear old plan state and generate fresh
      setExistingPlan(null);
      setPlanReady(false);
      setInput(pendingText);
      setTimeout(() => {
        const sendBtn = document.querySelector('[data-send-btn]') as HTMLButtonElement;
        sendBtn?.click();
      }, 50);
    } else {
      // Replace mode: just overwrite, no archive
      setExistingPlan(null);
      setPlanReady(false);
      setInput(pendingText);
      setTimeout(() => {
        const sendBtn = document.querySelector('[data-send-btn]') as HTMLButtonElement;
        sendBtn?.click();
      }, 50);
    }
  }, [planConflict, existingPlan, projectPath, remoteInfo]);

  const sendMessage = useCallback(async (overrideTextOrEvent?: string | React.MouseEvent) => {
    const overrideText = typeof overrideTextOrEvent === 'string' ? overrideTextOrEvent : undefined;
    const textToSend = overrideText || input.trim();
    // When called with an explicit override string (e.g. from "Generate Report"
    // button), skip the isStreaming guard — the caller knows the previous turn
    // is done even if the done-event was missed over SSH.
    if (!textToSend) return;
    if (isStreaming && !overrideText) return;

    const rawText = textToSend;

    // ── Plan mode: if existing plan detected, ask user what to do ──
    if (mode === 'plan' && existingPlan && existingPlan.trim().length > 0 && !planConflict && !planReady) {
      // Show the user's message in chat
      setMessages((prev) => [...prev, {
        id: crypto.randomUUID(), role: 'user' as const,
        content: [{ type: 'text' as const, text: rawText }],
        timestamp: Date.now(),
      }]);
      setInput('');
      // Trigger conflict resolution UI
      setPlanConflict({ pendingRequest: rawText });
      return; // Don't send yet — wait for user choice
    }

    // ── Report mode: handle phase transitions ──
    if (mode === 'report') {
      // First message in report mode → trigger scan
      if (reportPhase === 'idle') {
        setReportPhase('scan');
        setReportError(null);
        setReportOutputPath(null);

        // Show user message
        const userMsg: ChatMessage = {
          id: crypto.randomUUID(),
          role: 'user',
          content: [{ type: 'text', text: rawText }],
          timestamp: Date.now(),
        };
        setMessages((prev) => [...prev, userMsg]);
        setInput('');

        try {
          // Scan project files
          console.log('[Report] Scanning project files. remoteInfo:', remoteInfo, 'projectPath:', projectPath);
          const scanResult = remoteInfo
            ? await scanRemoteProjectFiles(remoteInfo.profileId, remoteInfo.remotePath || '.')
            : await scanProjectFiles(projectPath || '.');
          console.log('[Report] Scan result:', JSON.stringify(scanResult).slice(0, 500));

          setReportScan(scanResult);

          // Also extract methods info
          if (!remoteInfo && projectPath) {
            try {
              const methods = await extractMethodsInfo(projectPath);
              setReportMethodsInfo(methods);
            } catch { /* methods extraction is best-effort */ }
          }

          setReportPhase('select');

          // Add system message about scan results
          const scanMsg: ChatMessage = {
            id: crypto.randomUUID(),
            role: 'assistant',
            content: [{
              type: 'text',
              text: `I scanned your project and found **${scanResult.total_pdfs} PDFs**, **${scanResult.total_images} images**, **${scanResult.total_csvs} CSVs**, **${scanResult.total_docs} documents** (.md, .txt, .json, .html), and **${scanResult.total_code} scripts** (.R, .py, .sh, .nf).\n\nSelect the files you want to include in your report using the picker above, then click **Continue** to proceed.`,
            }],
            timestamp: Date.now(),
            systemGenerated: true,
          };
          setMessages((prev) => [...prev, scanMsg]);
        } catch (err) {
          setReportError(`Scan failed: ${err}`);
          setReportPhase('idle');
        }
        return;
      }

      // User confirms file selection → move to clarify phase
      // (handled by onProceed callback in ReportPhasePanel)
    }

    // Layer 1: Project file index (automatic, always included if available)
    let indexPrefix = '';
    if (projectIndex.current) {
      indexPrefix = `<project_files>\n${projectIndex.current}\n</project_files>\n\n`;
    }

    // Layer 2: Active protocol (user-selected from sidebar)
    let protocolPrefix = '';
    if (activeProtocol && protocolContent) {
      protocolPrefix = `<protocol name="${activeProtocol.name}">\n${protocolContent}\n</protocol>\n\nFollow the above protocol for this task. `;
    }

    // Layer 2.5: Server configuration (auto-injected when connected to remote server)
    let serverConfigPrefix = '';
    if (remoteInfo?.profileId) {
      try {
        const config = await invoke<Record<string, string>>('get_server_config', { profileId: remoteInfo.profileId });
        if (config && Object.keys(config).length > 0) {
          const configLines = Object.entries(config)
            .map(([k, v]) => `  ${k}: ${v}`)
            .join('\n');
          serverConfigPrefix = `<server_config>\nThe user's HPC server settings — use these values in any generated scripts (SLURM headers, conda activate, paths, etc.):\n${configLines}\n</server_config>\n\n`;
        }
      } catch {
        // Server config not available, continue without it
      }
    }

    // Layer 3: Existing plan context (for plan iteration — user gives feedback)
    let planPrefix = '';
    if (mode === 'plan' && existingPlan) {
      planPrefix = `<current_plan>\n${existingPlan}\n</current_plan>\n\nThe user has feedback on this plan. Update the plan accordingly and output the complete revised plan.\n\n`;
    }

    // Layer 4: @-mentions (user-typed, lightweight metadata only)
    const currentMentions = [...mentions];
    const currentAttachments = [...attachments];
    let mentionPrefix = '';
    if (currentMentions.length > 0) {
      const contextParts = currentMentions.map(ref => resolveMentionContext(ref));
      mentionPrefix = `The user is referencing the following files/folders:\n${contextParts.join('\n')}\n\n`;
    }
    if (currentAttachments.length > 0) {
      const attachParts = currentAttachments.map(a =>
        `- ${a.type === 'image' ? 'Image' : 'File'}: ${a.path} (use Read tool to view this file)`
      );
      mentionPrefix += `The user has attached these files for context:\n${attachParts.join('\n')}\n\n`;
    }

    // Layer 5: PubMed literature (auto-search in Ask mode or Report mode when enabled)
    // Skip PubMed for report-trigger phrases like "Generate report", "make the report", etc.
    const isReportTriggerPhrase =
      /\b(generate|create|make|write|produce|build|start)\b.*\breport\b/i.test(rawText) ||
      /\breport\b.*\b(now|please|go|ready)\b/i.test(rawText);
    let pubmedPrefix = '';
    if (((mode === 'ask' && pubmedEnabled) || (mode === 'report' && reportPhase === 'clarify')) && !isReportTriggerPhrase) {
      try {
        setPubmedSearching(true);

        // Build a better PubMed query from the user's natural language question.
        // Remove common filler words and keep scientific terms for better search results.
        const stopWords = new Set(['what', 'does', 'the', 'how', 'is', 'are', 'can', 'do', 'why', 'when', 'which', 'where',
          'about', 'explain', 'tell', 'me', 'please', 'help', 'understand', 'describe', 'with', 'for', 'and', 'or', 'in',
          'of', 'to', 'a', 'an', 'this', 'that', 'it', 'its', 'be', 'been', 'being', 'have', 'has', 'had', 'i', 'my', 'we',
          'they', 'you', 'your', 'their', 'our', 'would', 'could', 'should', 'will', 'shall', 'may', 'might', 'between',
          'from', 'into', 'through', 'during', 'before', 'after', 'above', 'below', 'any', 'all', 'each', 'every', 'some']);
        const searchTerms = rawText
          .replace(/[?!.,;:'"()[\]{}]/g, ' ')
          .split(/\s+/)
          .filter(w => w.length > 1 && !stopWords.has(w.toLowerCase()))
          .join(' ');

        const pubmedQuery = searchTerms || rawText;
        console.log('[PubMed] Searching for:', pubmedQuery);

        const result = await invoke<PubMedSearchResult>('search_pubmed', {
          query: pubmedQuery,
          maxResults: 5,
        });
        setPubmedSearching(false);

        console.log('[PubMed] Found', result.articles.length, 'articles out of', result.total_found, 'total');

        if (result.articles.length > 0) {
          setLastPubmedResults(result.articles);

          // Include full abstracts — this is the key data that grounds the response
          const citations = result.articles.map((a, i) => {
            const abstract_section = a.abstract_text
              ? `\n    Abstract: ${a.abstract_text}`
              : '\n    Abstract: Not available.';
            return `[${i + 1}] "${a.title}"\n    Authors: ${a.authors}\n    Journal: ${a.journal} (${a.year})\n    PMID: ${a.pmid} | URL: ${a.url}${a.doi ? `\n    DOI: ${a.doi}` : ''}${abstract_section}`;
          }).join('\n\n');

          pubmedPrefix = `<pubmed_literature>\n` +
            `You MUST ground your answer in the following ${result.articles.length} peer-reviewed articles retrieved from PubMed (out of ${result.total_found} total results for: "${pubmedQuery}").\n\n` +
            `INSTRUCTIONS:\n` +
            `- Cite articles by number, e.g. [1], [2], when referencing specific findings.\n` +
            `- Include the PubMed URL for each article you cite so the user can read the original paper.\n` +
            `- Synthesize information across multiple articles when relevant.\n` +
            `- If the articles contradict each other, note the disagreement.\n` +
            `- If the retrieved articles don't adequately address the question, clearly state this and provide your best answer with the caveat that it's not supported by the provided literature.\n` +
            `- At the end of your response, include a "References" section listing all cited articles.\n\n` +
            `ARTICLES:\n\n${citations}\n` +
            `</pubmed_literature>\n\n`;
        } else {
          setLastPubmedResults(null);
          console.log('[PubMed] No articles found for query:', pubmedQuery);
        }
      } catch (err) {
        console.error('[PubMed] Search failed:', err);
        setPubmedSearching(false);
        setLastPubmedResults(null);
      }
    }

    // Assemble: index → protocol → plan → mentions → pubmed → user message
    let finalText = rawText;

    // Plan archival is handled by the conflict resolution UI (handlePlanConflictChoice).
    // By the time we reach here in plan mode, existingPlan is cleared if user chose "new".

    // Wrap the prompt for plan mode
    if (mode === 'plan' && !planReady) {
      finalText = `Create a detailed implementation plan for the following request and write it to "implementation_plan.md" in the current project directory. Do NOT implement anything yet — only create the plan file.\n\nRequest: ${rawText}`;
    } else if (mode === 'plan' && planReady) {
      finalText = `Update the existing implementation_plan.md based on this feedback. Rewrite the complete plan file with the changes applied. Do NOT implement anything.\n\nFeedback: ${rawText}`;
    }

    // In report mode, inject file context and methods info
    if (mode === 'report' && reportPhase === 'clarify') {
      let reportContext = '';

      // Selected files with pre-read content
      if (reportSelectedFiles.length > 0) {
        if (reportFilePreviews.length > 0) {
          // Include actual file contents — Claude doesn't need to read anything
          reportContext += `<selected_files>\nThe user selected ${reportSelectedFiles.length} files. Their contents are provided below — do NOT use Read or any tools to access these files.\n\n`;
          for (const preview of reportFilePreviews) {
            reportContext += `<file path="${preview.path}" name="${preview.name}"${preview.truncated ? ' truncated="true"' : ''}>\n`;
            if (preview.error) {
              reportContext += `[Error reading file: ${preview.error}]\n`;
            } else {
              reportContext += preview.content + '\n';
            }
            reportContext += '</file>\n\n';
          }
          reportContext += '</selected_files>\n\n';
        } else {
          // Fallback: just list paths (if pre-read failed)
          reportContext += `<selected_files>\nThe user selected these ${reportSelectedFiles.length} files for the report:\n`;
          reportContext += reportSelectedFiles.map(p => `- ${p}`).join('\n');
          reportContext += '\n</selected_files>\n\n';
        }
      }

      // Methods info
      if (reportMethodsInfo) {
        reportContext += '<methods_info>\n';
        if (reportMethodsInfo.r_version) reportContext += `R version: ${reportMethodsInfo.r_version}\n`;
        if (reportMethodsInfo.python_version) reportContext += `Python version: ${reportMethodsInfo.python_version}\n`;
        if (reportMethodsInfo.tools.length > 0) {
          reportContext += 'Detected tools/packages:\n';
          reportContext += reportMethodsInfo.tools.map(t =>
            `- ${t.name}${t.version ? ' v' + t.version : ''} (${t.language || 'unknown'})`
          ).join('\n');
        }
        reportContext += '\n</methods_info>\n\n';
      }

      // Plan context — use selected plan if user picked one, else current
      if (reportSelectedPlan && reportSelectedPlan !== 'current' && reportSelectedPlan !== existingPlan?.match(/^#\s+(.+)/m)?.[1]) {
        // User selected an archived plan — load its content
        // (already loaded into existingPlan via the plan history dropdown)
        if (existingPlan) {
          reportContext += `<implementation_plan>\n${existingPlan}\n</implementation_plan>\n\n`;
        }
      } else if (existingPlan) {
        reportContext += `<implementation_plan>\n${existingPlan}\n</implementation_plan>\n\n`;
      }

      // Check if user wants to generate the report now
      // Flexible matching: "generate report", "make the report", "create a report", etc.
      const lowerText = rawText.toLowerCase();
      const wantsGenerate =
        /\b(generate|create|make|write|produce|build|start)\b.*\breport\b/i.test(lowerText) ||
        /\breport\b.*\b(now|please|go|ready)\b/i.test(lowerText);

      if (wantsGenerate) {
        setReportPhase('draft');
        reportPhaseRef.current = 'draft'; // Sync ref immediately
        const scopeInstr = reportScope === 'focused'
          ? `Write a FOCUSED summary report — keep it concise with key results, a brief methods overview, and main conclusions. Skip lengthy introductions and exhaustive discussion. Aim for 2-3 pages.\n\n`
          : `Write a COMPREHENSIVE scientific report suitable for publication. Include all standard sections with thorough, publication-quality prose.\n\n`;

        finalText = reportContext + scopeInstr +
          `The user wants to generate the final report now. Based on all the context provided (files, methods, previous conversation), ` +
          `write a ${reportScope === 'focused' ? 'focused' : 'complete'} scientific report with these sections:\n` +
          `1. **Title** — a descriptive title for this analysis\n` +
          `2. **Abstract** — ${reportScope === 'focused' ? '100-150' : '150-250'} word summary\n` +
          `3. **Introduction** — biological context and motivation\n` +
          `4. **Results** — interpret the analysis outputs biologically, reference the figures/tables\n` +
          `5. **Discussion** — connect findings to broader literature, cite PubMed references\n` +
          `6. **Methods** — tools with versions (no SLURM/conda details), data sources\n\n` +
          `Use the PubMed articles provided to cite references as [1], [2], etc.\n` +
          `Write thorough, publication-quality prose for each section.\n\n` +
          `User's instructions: ${rawText}`;
      } else {
        // User is providing context / answering questions — NOT generating yet.
        // Do NOT send file contents here — just send the user's answer.
        // Explicitly instruct Claude to NOT write the report yet.
        finalText = `The user is providing additional context for an upcoming report. ` +
          `DO NOT write the report yet. DO NOT create any files. ` +
          `Just acknowledge their input briefly (1-2 sentences), ask any follow-up questions if needed, ` +
          `and let them know they can click "Generate Report" or type "generate report" when ready.\n\n` +
          `User's context: ${rawText}`;
      }
    }

    const prompt = indexPrefix + protocolPrefix + serverConfigPrefix + planPrefix + mentionPrefix + pubmedPrefix + finalText;

    // Show the raw user text in the UI (without the injected context)
    const userMessage: ChatMessage = {
      id: crypto.randomUUID(),
      role: 'user',
      content: [{ type: 'text', text: rawText }],
      timestamp: Date.now(),
    };

    setMessages((prev) => [...prev, userMessage]);
    setInput('');
    setMentions([]); // Clear mentions for next message
    setAttachments([]); // Clear attachments for next message
    setMentionActive(false);
    setIsStreaming(true);
    setStreamStalled(false);
    lastEventTime.current = Date.now();
    seenMsgIds.current.clear(); // Reset for new conversation turn

    // Pin the working directory on the first message of this session so that
    // navigating folders in the sidebar doesn't break an active session.
    if (!sessionProjectPath.current) {
      sessionProjectPath.current = projectPath || '.';
    }
    const pinnedPath = sessionProjectPath.current;

    try {
      // For report draft generation, start a FRESH session (don't resume).
      // The full context is already in the prompt — resuming would duplicate
      // everything and make Opus extremely slow or hang.
      const isReportDraft = mode === 'report' && reportPhase === 'draft';
      const isReportClarify = mode === 'report' && reportPhase === 'clarify';
      // Start a FRESH session (no resume) for:
      // - Report drafts (full context already in prompt)
      // - Report clarify (just acknowledging user context — no tools needed)
      // - Plan mode (always fresh — existing plan context is injected into the prompt,
      //   resuming an old agent session causes Claude to ignore the plan prompt)
      const shouldStartFresh = isReportDraft || isReportClarify || mode === 'plan';
      const invokeArgs: Record<string, unknown> = {
        sessionId,
        prompt,
        projectPath: pinnedPath,
        model,
        resumeSession: shouldStartFresh ? null : claudeSessionId,
        // Send the actual mode — the backend handles plan/report prompt construction,
        // archival, and max-turns based on the mode string.
        mode,
        // Report draft & clarify: limit to 1 turn — no tool use needed.
        // Draft has full context; clarify is just acknowledging user input.
        ...((isReportDraft || isReportClarify) ? { maxTurns: 1 } : {}),
      };

      // If connected to a remote server, pass SSH context
      if (remoteInfo) {
        invokeArgs.remote = {
          profileId: remoteInfo.profileId,
          remotePath: remoteInfo.remotePath,
        };

        // Terminal mode: run inside the existing SSH terminal session
        if (useTerminal && sshTerminalId) {
          invokeArgs.useTerminal = true;
          invokeArgs.terminalId = sshTerminalId;
        }
      }

      // Add a timeout so the UI doesn't hang forever if the backend stalls
      const timeout = new Promise<never>((_, reject) =>
        setTimeout(() => reject(new Error('Session start timed out after 60s. Check your SSH connection.')), 60000)
      );
      await Promise.race([invoke('start_claude_session', invokeArgs), timeout]);
    } catch (err) {
      setIsStreaming(false);
      setMessages((prev) => [
        ...prev,
        {
          id: crypto.randomUUID(),
          role: 'system',
          content: [{ type: 'text', text: `Error: ${err}` }],
          timestamp: Date.now(),
        },
      ]);
    }
  // NOTE: projectPath is intentionally excluded from deps — we use
  // sessionProjectPath (pinned on first message) so that sidebar folder
  // navigation does not break an active streaming session.
  }, [input, isStreaming, sessionId, model, claudeSessionId, mode, remoteInfo, useTerminal, sshTerminalId, mentions, activeProtocol, protocolContent, existingPlan, pubmedEnabled, reportPhase, reportSelectedFiles, reportMethodsInfo, reportScope, reportSelectedPlan, planConflict, planReady]);

  // Report mode: user clicks "Generate Report" button during clarify phase.
  // Calls sendMessage with an override string, which bypasses the isStreaming
  // guard (the previous Claude turn is done even if the done-event was missed).
  // sendMessage will detect "generate report" and set phase to 'draft'.
  const reportGenerateFromButton = useCallback(() => {
    if (reportPhase !== 'clarify') return;
    console.log('[Report] Generate Report button clicked — moving to draft phase');
    setIsStreaming(false);   // Reset in case stuck from previous turn
    setInput('');
    sendMessage('Generate report');
  }, [reportPhase, sendMessage]);

  const handleKeyDown = (e: React.KeyboardEvent) => {
    // @-mention popup keyboard navigation
    if (mentionActive && mentionItems.length > 0) {
      if (e.key === 'ArrowDown') {
        e.preventDefault();
        setMentionIndex(i => (i + 1) % mentionItems.length);
        return;
      }
      if (e.key === 'ArrowUp') {
        e.preventDefault();
        setMentionIndex(i => (i - 1 + mentionItems.length) % mentionItems.length);
        return;
      }
      if (e.key === 'Enter' || e.key === 'Tab') {
        e.preventDefault();
        handleMentionSelect(mentionItems[mentionIndex]);
        return;
      }
      if (e.key === 'Escape') {
        e.preventDefault();
        setMentionActive(false);
        setMentionItems([]);
        return;
      }
    }

    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      sendMessage();
    }
  };

  const handleInputChange = (e: React.ChangeEvent<HTMLTextAreaElement>) => {
    const val = e.target.value;
    setInput(val);
    const el = e.target;
    el.style.height = 'auto';
    el.style.height = Math.min(el.scrollHeight, 300) + 'px';

    // @-mention detection
    const cursor = el.selectionStart;
    // Walk backward from cursor to find an '@' that starts a mention
    let atPos = -1;
    for (let i = cursor - 1; i >= 0; i--) {
      if (val[i] === '@') {
        // '@' should be at start or preceded by whitespace
        if (i === 0 || /\s/.test(val[i - 1])) {
          atPos = i;
        }
        break;
      }
      // Stop if we hit whitespace before finding '@' (but allow '/' and '.' in query)
      if (val[i] === ' ' || val[i] === '\n') break;
    }

    if (atPos >= 0) {
      const query = val.slice(atPos + 1, cursor);
      setMentionActive(true);
      setMentionCursorStart(atPos);
      setMentionQuery(query);
    } else {
      setMentionActive(false);
    }
  };

  // Show auth setup if needed
  if (authState && !authState.authenticated) {
    return (
      <div className="flex flex-col h-full bg-zinc-950">
        <div className="flex items-center justify-between px-3 py-2 border-b border-zinc-800 shrink-0">
          <div className="flex items-center gap-2">
            <Sparkles className="w-4 h-4 text-blue-400" />
            <span className="text-sm font-medium text-zinc-300">Claude</span>
          </div>
        </div>
        <AuthSetup onDone={(method) => setAuthState({ authenticated: true, method })} />
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full bg-zinc-950">
      {/* Header */}
      <div className="flex items-center justify-between px-3 py-2 border-b border-zinc-800 shrink-0">
        <div className="flex items-center gap-2">
          <Sparkles className="w-4 h-4 text-blue-400" />
          <span className="text-sm font-medium text-zinc-300">Claude</span>
        </div>
        <div className="flex items-center gap-2">
          <button
            onClick={resetChat}
            className="flex items-center gap-1 px-1.5 py-0.5 rounded text-[10px] text-zinc-500 hover:text-zinc-300 hover:bg-zinc-800 transition-colors"
            title="New chat session"
          >
            <Plus className="w-3 h-3" />
            <span>New</span>
          </button>
          {authState?.method === 'oauth' && (
            <span className="text-[10px] text-orange-400/70 bg-orange-400/10 px-1.5 py-0.5 rounded">
              Max/Pro
            </span>
          )}
          {authState?.method === 'api_key' && (
            <span className="text-[10px] text-blue-400/70 bg-blue-400/10 px-1.5 py-0.5 rounded">
              API
            </span>
          )}
          <span className="text-[11px] text-zinc-600">${totalCost.toFixed(4)}</span>
        </div>
      </div>

      {/* Model selector + Remote indicator */}
      <div className="px-3 py-1.5 border-b border-zinc-800/50 shrink-0 flex items-center gap-2">
        <select
          value={model}
          onChange={(e) => setModel(e.target.value)}
          className="flex-1 bg-zinc-900 border border-zinc-800 rounded px-2 py-1 text-xs text-zinc-400 outline-none"
        >
          <option value="claude-sonnet-4-20250514">claude-sonnet-4-20250514</option>
          <option value="claude-opus-4-20250514">claude-opus-4-20250514</option>
          <option value="claude-haiku-4-5-20251001">claude-haiku-4-5-20251001</option>
        </select>
      </div>

      {/* Remote connection banner */}
      {remoteInfo && (
        <div className="flex items-center gap-1.5 px-3 py-1 border-b border-zinc-800/30 shrink-0 bg-zinc-900/50">
          <Server className="w-3 h-3 text-green-400 shrink-0" />
          <span className="text-[10px] text-green-400 font-medium">{remoteInfo.profileName}</span>
          <span className="text-[10px] text-zinc-600 mx-0.5">{'\u00B7'}</span>
          <span className="text-[10px] text-zinc-500 font-mono truncate">{remoteInfo.remotePath}</span>

          {/* Use Terminal toggle */}
          <button
            onClick={() => setUseTerminal((v) => !v)}
            className={`ml-auto flex items-center gap-1 px-1.5 py-0.5 rounded text-[10px] font-medium transition-colors shrink-0 ${
              useTerminal && sshTerminalId
                ? 'bg-amber-900/40 text-amber-400 hover:bg-amber-900/60'
                : 'bg-zinc-800 text-zinc-500 hover:text-zinc-400'
            }`}
            title={useTerminal
              ? 'Using terminal session (tmux/compute node) — click to use direct SSH instead'
              : 'Using direct SSH (login node) — click to use terminal session instead'
            }
          >
            <TerminalSquare className="w-3 h-3" />
            {useTerminal && sshTerminalId ? 'Terminal' : 'Direct'}
          </button>

          <button
            onClick={() => setRemoteInfo(null)}
            className="text-[10px] text-zinc-600 hover:text-zinc-400 transition-colors shrink-0 ml-1"
            title="Disconnect from remote — run Claude locally"
          >
            {'\u2715'}
          </button>
        </div>
      )}

      {/* Remote Claude Code setup banner — Step 1: Not installed */}
      {remoteInfo && remoteDeps && remoteDeps.checked && !remoteDeps.hasClaude && (
        <div className="px-3 py-2 border-b border-amber-800/30 shrink-0 bg-amber-950/30">
          <div className="flex items-start gap-2">
            <AlertTriangle className="w-4 h-4 text-amber-400 shrink-0 mt-0.5" />
            <div className="flex-1 min-w-0">
              <p className="text-xs text-amber-300 font-medium">
                Step 1: Install Claude Code on {remoteInfo.profileName}
              </p>
              <p className="text-[10px] text-zinc-400 mt-0.5 leading-relaxed">
                Claude Code needs to be installed on the server to run plans and agent tasks remotely.
              </p>

              <div className="mt-1.5 flex items-center gap-1.5 bg-zinc-900/80 rounded px-2 py-1 border border-zinc-700/50">
                <code className="text-[10px] text-zinc-300 font-mono select-all flex-1">
                  curl -fsSL https://claude.ai/install.sh | bash
                </code>
                <button
                  onClick={() => {
                    navigator.clipboard.writeText('curl -fsSL https://claude.ai/install.sh | bash');
                  }}
                  className="text-[9px] text-zinc-500 hover:text-zinc-300 shrink-0 px-1"
                  title="Copy to clipboard"
                >
                  Copy
                </button>
              </div>

              {remoteDeps.error && (
                <p className="text-[10px] text-red-400 mt-1.5 leading-relaxed whitespace-pre-wrap">
                  {remoteDeps.error}
                </p>
              )}

              <div className="flex items-center gap-2 mt-2">
                <button
                  onClick={installRemoteClaude}
                  disabled={remoteDeps.installing}
                  className="flex items-center gap-1.5 px-2.5 py-1 bg-amber-600 hover:bg-amber-500 disabled:bg-zinc-700 rounded text-[11px] text-white font-medium transition-colors"
                >
                  {remoteDeps.installing ? (
                    <><Loader2 className="w-3 h-3 animate-spin" /> Installing...</>
                  ) : (
                    <><Download className="w-3 h-3" /> Install on Server</>
                  )}
                </button>
                <button
                  onClick={recheckRemoteDeps}
                  className="flex items-center gap-1 px-2 py-1 bg-zinc-800 hover:bg-zinc-700 rounded text-[11px] text-zinc-400 transition-colors"
                >
                  <RefreshCw className="w-3 h-3" /> Re-check
                </button>
                <span className="text-[9px] text-zinc-600 ml-auto">
                  or paste the command in the terminal below
                </span>
              </div>
            </div>
          </div>
        </div>
      )}

      {/* Remote Claude Code setup banner — Step 2: Installed but not authenticated */}
      {/* Show when hasAuth is false OR null (null = couldn't determine, so prompt user) */}
      {remoteInfo && remoteDeps && remoteDeps.checked && remoteDeps.hasClaude && remoteDeps.hasAuth !== true && (
        <div className="px-3 py-2 border-b border-blue-800/30 shrink-0 bg-blue-950/30">
          <div className="flex items-start gap-2">
            <Key className="w-4 h-4 text-blue-400 shrink-0 mt-0.5" />
            <div className="flex-1 min-w-0">
              <p className="text-xs text-blue-300 font-medium">
                Step 2: Authenticate Claude Code on {remoteInfo.profileName}
              </p>
              <p className="text-[10px] text-zinc-400 mt-0.5 leading-relaxed">
                Claude Code is installed but needs to be authenticated. Choose one option:
              </p>

              {/* Option A: claude login in terminal */}
              <div className="mt-2 p-2 bg-zinc-900/60 rounded border border-zinc-700/50">
                <p className="text-[10px] text-zinc-300 font-medium mb-1">Option A: Log in with your Claude subscription</p>
                <div className="flex items-center gap-2 mt-1.5">
                  <button
                    onClick={() => {
                      if (sshTerminalId) {
                        // Send the login command directly to the active SSH terminal
                        const cmd = 'TERM=dumb claude login\n';
                        invoke('write_terminal', {
                          terminalId: sshTerminalId,
                          data: Array.from(new TextEncoder().encode(cmd)),
                        }).catch(console.error);
                      }
                    }}
                    disabled={!sshTerminalId}
                    className="flex items-center gap-1.5 px-3 py-1.5 bg-orange-600 hover:bg-orange-500 disabled:bg-zinc-700 disabled:text-zinc-500 rounded text-[11px] text-white font-medium transition-colors"
                  >
                    <LogIn className="w-3 h-3 pointer-events-none" />
                    Login on Server
                  </button>
                  <span className="text-[9px] text-zinc-600">or</span>
                  <div className="flex items-center gap-1.5 bg-zinc-800/80 rounded px-2 py-1 border border-zinc-700/30">
                    <code className="text-[10px] text-zinc-300 font-mono select-all">TERM=dumb claude login</code>
                    <button
                      onClick={() => navigator.clipboard.writeText('TERM=dumb claude login')}
                      className="text-[9px] text-zinc-500 hover:text-zinc-300 shrink-0 px-1"
                    >
                      Copy
                    </button>
                  </div>
                </div>
                <p className="text-[9px] text-zinc-500 mt-1.5">The OAuth link will automatically open in your local browser — sign in and paste the code back in the terminal.</p>
              </div>

              {/* Option B: API key */}
              <div className="mt-1.5 p-2 bg-zinc-900/60 rounded border border-zinc-700/50">
                <p className="text-[10px] text-zinc-300 font-medium mb-1">Option B: Use an API key</p>
                <p className="text-[9px] text-zinc-500">
                  Set your API key in Operon settings — it gets passed to the server automatically.
                  Get a key from{' '}
                  <a href="https://console.anthropic.com" target="_blank" rel="noopener noreferrer" className="text-blue-400 hover:text-blue-300 underline">
                    console.anthropic.com
                  </a>
                </p>
              </div>

              <div className="flex items-center gap-2 mt-2">
                <button
                  onClick={recheckRemoteDeps}
                  disabled={remoteDeps?.installing}
                  className="flex items-center gap-1 px-2.5 py-1 bg-blue-600 hover:bg-blue-500 disabled:opacity-50 disabled:cursor-wait rounded text-[11px] text-white font-medium transition-colors"
                >
                  <RefreshCw className={`w-3 h-3 ${remoteDeps?.installing ? 'animate-spin' : ''}`} />
                  {remoteDeps?.installing ? 'Checking...' : 'Re-check Auth'}
                </button>
              </div>
            </div>
          </div>
        </div>
      )}

      {/* Plan workflow banner */}
      {existingPlan && (
        <div className={`px-3 py-1.5 border-b shrink-0 ${planReady ? 'bg-amber-950/30 border-amber-800/30' : 'bg-blue-950/30 border-zinc-800/30'}`}>
          <div className="flex items-center gap-1.5">
            <ClipboardList className={`w-3 h-3 shrink-0 ${planReady ? 'text-amber-400' : 'text-blue-400'}`} />
            <span className={`text-[10px] font-medium ${planReady ? 'text-amber-400' : 'text-blue-400'}`}>
              {planReady ? 'Plan ready for review' : 'Plan detected'}
            </span>
            <span className="text-[10px] text-zinc-600 mx-0.5">{'\u00B7'}</span>
            <span className="text-[10px] text-zinc-500 truncate">
              implementation_plan.md ({existingPlan.split('\n').length} lines)
            </span>
            {/* Plan date extracted from content */}
            {(() => {
              const dateMatch = existingPlan.match(/\*\*Date:\*\*\s*(.+)/);
              return dateMatch ? (
                <>
                  <span className="text-[10px] text-zinc-600 mx-0.5">{'\u00B7'}</span>
                  <span className="text-[10px] text-zinc-600">{dateMatch[1].trim()}</span>
                </>
              ) : null;
            })()}
            {/* History dropdown */}
            {planHistory.length > 0 && (
              <div className="relative ml-1">
                <button
                  onClick={() => setShowPlanHistory(!showPlanHistory)}
                  className="flex items-center gap-0.5 text-[10px] text-zinc-500 hover:text-zinc-300 transition-colors"
                  title={`${planHistory.length} archived plan${planHistory.length > 1 ? 's' : ''}`}
                >
                  <History className="w-3 h-3" />
                  <span>{planHistory.length}</span>
                </button>
                {showPlanHistory && (
                  <div className="absolute top-5 left-0 z-50 w-72 bg-zinc-900 border border-zinc-700 rounded-lg shadow-xl overflow-hidden">
                    <div className="px-3 py-1.5 border-b border-zinc-800 text-[10px] text-zinc-400 font-medium">
                      Plan History ({planHistory.length} archived)
                    </div>
                    <div className="max-h-48 overflow-y-auto">
                      {planHistory.map((entry) => (
                        <button
                          key={entry.filename}
                          onClick={async () => {
                            try {
                              const content = await readPlanHistoryEntry(entry.path);
                              setExistingPlan(content);
                              setPlanReady(false);
                              setShowPlanHistory(false);
                              setMessages(prev => [...prev, {
                                id: crypto.randomUUID(),
                                role: 'system' as const,
                                content: [{ type: 'text' as const, text: `Loaded archived plan: **${entry.title}** (${entry.timestamp})` }],
                                timestamp: Date.now(),
                              }]);
                            } catch (e) {
                              console.error('Failed to load plan:', e);
                            }
                          }}
                          className="w-full px-3 py-1.5 text-left hover:bg-zinc-800 transition-colors border-b border-zinc-800/50 last:border-0"
                        >
                          <div className="text-[10px] text-zinc-300 font-medium truncate">{entry.title}</div>
                          <div className="text-[9px] text-zinc-500">{entry.timestamp} · {entry.lines} lines</div>
                        </button>
                      ))}
                    </div>
                  </div>
                )}
              </div>
            )}

            {planReady ? (
              <div className="flex items-center gap-1.5 ml-auto shrink-0">
                <button
                  onClick={() => {
                    // Switch to agent mode and execute the plan
                    setMode('agent');
                    setPlanReady(false);
                    setInput('Execute the implementation plan in implementation_plan.md. Follow each step precisely.');
                    setTimeout(() => {
                      // Auto-send after mode switch
                      const sendBtn = document.querySelector('[data-send-btn]') as HTMLButtonElement;
                      sendBtn?.click();
                    }, 100);
                  }}
                  className="flex items-center gap-1 px-2 py-0.5 bg-green-600 hover:bg-green-700 rounded text-[10px] text-white font-medium transition-colors"
                >
                  <CheckCircle className="w-3 h-3" />
                  Approve & Execute
                </button>
                <button
                  onClick={() => {
                    setPlanReady(false);
                    // Stay in plan mode for iteration
                  }}
                  className="text-[10px] text-zinc-500 hover:text-zinc-300 transition-colors"
                >
                  Keep editing
                </button>
              </div>
            ) : (
              <span className="text-[10px] text-zinc-600 ml-auto">
                {mode === 'agent' ? 'Agent will follow this plan' : mode === 'plan' ? 'Send feedback to update' : 'Available as context'}
              </span>
            )}
          </div>
          {planReady && (
            <div className="mt-1.5 space-y-1.5">
              <div className="flex items-center gap-1.5">
                <span className="text-[10px] text-amber-500/70">{'\u2193'} Type feedback below to revise the plan, or use a suggestion:</span>
              </div>
              <div className="flex flex-wrap gap-1">
                {[
                  'Change the output format',
                  'Add more detail to the steps',
                  'Simplify the approach',
                  'Use a different library/tool',
                ].map((suggestion) => (
                  <button
                    key={suggestion}
                    onClick={() => {
                      setInput(suggestion);
                      textareaRef.current?.focus();
                    }}
                    className="text-[10px] px-2 py-0.5 bg-zinc-800 hover:bg-zinc-700 text-zinc-400 hover:text-zinc-200 rounded-full transition-colors"
                  >
                    {suggestion}
                  </button>
                ))}
              </div>
            </div>
          )}
        </div>
      )}

      {/* Report mode phase panel */}
      {mode === 'report' && (reportPhase !== 'idle' || reportError) && (
        <div className="mx-3 mt-2 shrink-0 relative z-10">
          <ReportPhasePanel
            phase={reportPhase}
            scan={reportScan}
            selectedFiles={reportSelectedFiles}
            onSelectionChange={setReportSelectedFiles}
            methodsInfo={reportMethodsInfo}
            onProceed={reportProceedFromSelect}
            onRescan={reportRescan}
            onCancel={reportCancel}
            onGenerate={reportGenerateFromButton}
            isStreaming={isStreaming}
            outputPath={reportOutputPath}
            error={reportError}
            isLoading={reportLoading}
            planHistory={planHistory}
            currentPlanTitle={existingPlan ? (existingPlan.match(/^#\s+(.+)/m)?.[1] || 'Current Plan') : null}
          />
        </div>
      )}

      {/* Active Protocol banner */}
      {activeProtocol && (
        <div className="flex items-center gap-1.5 px-3 py-1 border-b border-zinc-800/30 shrink-0 bg-teal-950/30">
          <BookOpen className="w-3 h-3 text-teal-400 shrink-0" />
          <span className="text-[10px] text-teal-400 font-medium">Protocol</span>
          <span className="text-[10px] text-zinc-600 mx-0.5">{'\u00B7'}</span>
          <span className="text-[10px] text-zinc-300 truncate">{activeProtocol.name}</span>
          <button
            onClick={() => { setActiveProtocol(null); setProtocolContent(null); }}
            className="text-[10px] text-zinc-600 hover:text-zinc-400 transition-colors ml-auto shrink-0"
            title="Remove protocol"
          >
            {'\u2715'}
          </button>
        </div>
      )}

      {/* Session Resume Banner */}
      {showResumeModal && previousSessions.length > 0 && (
        <div className="mx-3 mt-2 p-3 bg-indigo-950/40 border border-indigo-800/40 rounded-lg shrink-0">
          <div className="flex items-center justify-between mb-2">
            <div className="flex items-center gap-2">
              <RotateCcw className="w-3.5 h-3.5 text-indigo-400" />
              <span className="text-xs font-medium text-indigo-300">Previous Sessions</span>
            </div>
            <button
              onClick={handleDismissResume}
              className="text-zinc-500 hover:text-zinc-300 transition-colors"
            >
              <X className="w-3.5 h-3.5" />
            </button>
          </div>
          {previousSessions.slice(0, 5).map((s) => {
            const age = Date.now() - s.last_activity;
            const ageStr = age < 60000 ? 'just now'
              : age < 3600000 ? `${Math.floor(age / 60000)}m ago`
              : age < 86400000 ? `${Math.floor(age / 3600000)}h ago`
              : `${Math.floor(age / 86400000)}d ago`;
            const displayName = s.name || `${s.mode} session`;
            return (
              <SessionRow
                key={s.session_id}
                session={s}
                displayName={displayName}
                ageStr={ageStr}
                onResume={() => handleResumeSession(s)}
                onDelete={() => {
                  invoke('delete_session', { sessionId: s.session_id, remote: null, deleteOutput: false }).catch(() => {});
                  setPreviousSessions((prev) => prev.filter((p) => p.session_id !== s.session_id));
                }}
                onRename={(newName) => {
                  invoke('rename_session', { sessionId: s.session_id, name: newName }).catch(() => {});
                  setPreviousSessions((prev) =>
                    prev.map((p) => p.session_id === s.session_id ? { ...p, name: newName } : p)
                  );
                }}
              />
            );
          })}
          <button
            onClick={handleDismissResume}
            className="mt-1 text-[10px] text-zinc-500 hover:text-zinc-400 transition-colors"
          >
            Start new session instead
          </button>
        </div>
      )}

      {/* Messages */}
      <div className="flex-1 overflow-y-auto p-3 space-y-3">
        {messages.length === 0 && !showResumeModal && (
          <div className="flex flex-col items-center justify-center h-full px-6">
            <div className="w-14 h-14 rounded-2xl bg-gradient-to-br from-blue-500/20 to-purple-500/20 border border-blue-500/10 flex items-center justify-center mb-4">
              <Sparkles className="w-7 h-7 text-blue-400/80" />
            </div>
            <h3 className="text-base font-medium text-zinc-300 mb-1">What would you like to build?</h3>
            <p className="text-xs text-zinc-500 text-center max-w-[220px] leading-relaxed">
              {remoteInfo
                ? `Claude will run on ${remoteInfo.profileName} in ${remoteInfo.remotePath}`
                : 'Describe your task below and Claude will help you build it'}
            </p>
            <div className="flex flex-wrap gap-1.5 mt-5 justify-center max-w-[260px]">
              {['Analyze data', 'Write a pipeline', 'Search PubMed', 'Debug an error'].map((hint) => (
                <button
                  key={hint}
                  onClick={() => {
                    setInput(hint + ' ');
                    textareaRef.current?.focus();
                  }}
                  className="px-2.5 py-1 rounded-full border border-zinc-700/60 text-[11px] text-zinc-500 hover:text-zinc-300 hover:border-zinc-600 hover:bg-zinc-800/50 transition-all"
                >
                  {hint}
                </button>
              ))}
            </div>
          </div>
        )}
        {messages.map((msg) => (
          <MessageBubble key={msg.id} message={msg} />
        ))}
        {isStreaming && messages[messages.length - 1]?.role !== 'assistant' && (
          <div className="flex items-center gap-2 text-zinc-500 text-sm">
            <span className="animate-pulse">{'\u25CF'}</span>
            <span>Claude is thinking...</span>
          </div>
        )}
        {/* Stalled stream warning — shown when no events received for 90s on remote */}
        {isStreaming && streamStalled && (
          <div className="my-2 p-2.5 rounded-lg bg-amber-950/30 border border-amber-800/40 text-[12px]">
            <p className="text-amber-300 font-medium mb-1">
              No response received for over 90 seconds
            </p>
            <p className="text-amber-200/60 mb-2">
              The remote SSH connection may have stalled. You can wait, or stop and retry.
            </p>
            <button
              onClick={() => {
                invoke('stop_claude_session', { sessionId }).catch(() => {});
                setIsStreaming(false);
                setStreamStalled(false);
                setMessages((prev) => [
                  ...prev.map((m) => (m.isStreaming ? { ...m, isStreaming: false } : m)),
                  {
                    id: crypto.randomUUID(),
                    role: 'system' as const,
                    content: [{ type: 'text' as const, text: 'Session stopped due to stalled connection. You can send a new message to retry.' }],
                    timestamp: Date.now(),
                  },
                ]);
              }}
              className="px-2.5 py-1 rounded bg-amber-800/40 hover:bg-amber-800/60 text-amber-200 text-[11px] font-medium transition-colors"
            >
              Stop session
            </button>
          </div>
        )}
        {/* Plan conflict resolution UI */}
        {planConflict && (
          <div className="my-3 p-3 rounded-lg bg-amber-950/30 border border-amber-800/40">
            <p className="text-[12px] text-amber-300 font-medium mb-2">
              An implementation plan already exists. What would you like to do?
            </p>
            <div className="flex flex-col gap-2">
              <button
                onClick={() => {
                  // Generate archive name from existing plan date
                  const dateMatch = existingPlan?.match(/\*\*Date:\*\*\s*(.+)/);
                  const titleMatch = existingPlan?.match(/^#\s+(?:Implementation Plan:\s*)?(.+)/m);
                  const planDate = dateMatch
                    ? dateMatch[1].trim().replace(/\s+/g, '_').replace(/:/g, '')
                    : new Date().toISOString().replace(/[:.]/g, '-').slice(0, 19);
                  const shortTitle = titleMatch
                    ? '_' + titleMatch[1].trim().replace(/\s+/g, '_').replace(/[^a-zA-Z0-9_-]/g, '').slice(0, 40)
                    : '';
                  const archiveName = `plan_${planDate}${shortTitle}.md`;
                  handlePlanConflictChoice('new', archiveName);
                }}
                className="flex items-center gap-2 px-3 py-2 bg-emerald-700/40 hover:bg-emerald-700/60 border border-emerald-600/40 rounded text-[11px] text-emerald-200 font-medium transition-colors text-left"
              >
                <FileText className="w-3.5 h-3.5 shrink-0" />
                <div>
                  <div>Archive old plan & create new</div>
                  <div className="text-[9px] text-emerald-400/70 font-normal mt-0.5">
                    Saves current plan to .operon/plan_history/ then generates a fresh plan
                  </div>
                </div>
              </button>
              <button
                onClick={() => handlePlanConflictChoice('replace')}
                className="flex items-center gap-2 px-3 py-2 bg-zinc-700/40 hover:bg-zinc-700/60 border border-zinc-600/40 rounded text-[11px] text-zinc-300 font-medium transition-colors text-left"
              >
                <RotateCcw className="w-3.5 h-3.5 shrink-0" />
                <div>
                  <div>Replace existing plan</div>
                  <div className="text-[9px] text-zinc-500 font-normal mt-0.5">
                    Overwrites implementation_plan.md without saving the old one
                  </div>
                </div>
              </button>
              <button
                onClick={() => setPlanConflict(null)}
                className="text-[10px] text-zinc-600 hover:text-zinc-400 transition-colors mt-0.5"
              >
                Cancel
              </button>
            </div>
          </div>
        )}

        <div ref={messagesEndRef} />
      </div>

      {/* Input area — prominent, resizable */}
      <div className="shrink-0">
        {/* Drag handle to resize input area */}
        <div
          className="h-[6px] cursor-row-resize group flex items-center justify-center hover:bg-blue-500/20 transition-colors"
          onMouseDown={(e) => {
            e.preventDefault();
            const container = e.currentTarget.parentElement;
            const textarea = container?.querySelector('textarea');
            if (!textarea) return;
            const startY = e.clientY;
            const startH = textarea.offsetHeight;
            const onMove = (ev: MouseEvent) => {
              const delta = startY - ev.clientY;
              const newH = Math.max(60, Math.min(startH + delta, 400));
              textarea.style.height = newH + 'px';
              textarea.style.maxHeight = newH + 'px';
            };
            const onUp = () => {
              document.removeEventListener('mousemove', onMove);
              document.removeEventListener('mouseup', onUp);
            };
            document.addEventListener('mousemove', onMove);
            document.addEventListener('mouseup', onUp);
          }}
        >
          <div className="w-8 h-[3px] rounded-full bg-zinc-700 group-hover:bg-blue-400 transition-colors" />
        </div>

        <div className="px-3 pb-3 pt-1">
          {!projectPath && !remoteInfo && (
            <div className="flex items-center gap-2 mb-2 px-2 py-1.5 bg-yellow-900/20 border border-yellow-800/30 rounded text-xs text-yellow-400">
              <AlertCircle className="w-3.5 h-3.5 shrink-0" />
              <span>Open a folder or connect to a remote server to use Claude</span>
            </div>
          )}

          {/* Mention + Attachment chips */}
          {(mentions.length > 0 || attachments.length > 0) && (
            <div className="flex flex-wrap gap-1 mb-2">
              {mentions.map((ref, idx) => (
                <span
                  key={`mention-${ref.path}-${idx}`}
                  className="inline-flex items-center gap-1 px-2 py-0.5 bg-blue-900/30 border border-blue-700/40 rounded-full text-[11px] text-blue-300"
                >
                  {ref.isDir ? (
                    <FolderOpen className="w-3 h-3 text-amber-400" />
                  ) : (
                    <FileText className="w-3 h-3 text-zinc-400" />
                  )}
                  {ref.name}
                  <button
                    onClick={() => setMentions(prev => prev.filter((_, i) => i !== idx))}
                    className="text-zinc-500 hover:text-red-400 transition-colors ml-0.5"
                  >
                    {'\u2715'}
                  </button>
                </span>
              ))}
              {attachments.map((att, idx) => (
                <span
                  key={`attach-${att.path}-${idx}`}
                  className="inline-flex items-center gap-1 px-2 py-0.5 bg-emerald-900/30 border border-emerald-700/40 rounded-full text-[11px] text-emerald-300"
                >
                  {att.type === 'image' ? (
                    <Image className="w-3 h-3 text-emerald-400" />
                  ) : (
                    <Paperclip className="w-3 h-3 text-emerald-400" />
                  )}
                  {att.name}
                  <button
                    onClick={() => setAttachments(prev => prev.filter((_, i) => i !== idx))}
                    className="text-zinc-500 hover:text-red-400 transition-colors ml-0.5"
                  >
                    {'\u2715'}
                  </button>
                </span>
              ))}
            </div>
          )}

          {/* Mode selector row — above input for cleaner layout */}
          <div className="flex items-center justify-between mb-2 px-0.5">
            <div className="flex items-center gap-2">
              <ModeSelector mode={mode} onChange={(m) => {
                // Reset report state when leaving report mode
                if (mode === 'report' && m !== 'report') {
                  reportCancel();
                }
                setMode(m);
              }} />
              {(projectPath || remoteInfo) && (
                <button
                  onClick={() => {
                    setInput(prev => prev + '@');
                    textareaRef.current?.focus();
                    setTimeout(() => {
                      setMentionActive(true);
                      setMentionCursorStart(input.length);
                      setMentionQuery('');
                    }, 0);
                  }}
                  className="flex items-center gap-1 px-1.5 py-0.5 rounded hover:bg-zinc-800 transition-colors text-[11px] text-zinc-500 hover:text-zinc-400"
                  title="Reference a file or folder (@mention)"
                >
                  <AtSign className="w-3 h-3" />
                </button>
              )}
              {/* Attach file/screenshot button */}
              {(projectPath || remoteInfo) && (
                <>
                  <button
                    onClick={() => fileInputRef.current?.click()}
                    className="flex items-center gap-1 px-1.5 py-0.5 rounded hover:bg-zinc-800 transition-colors text-[11px] text-zinc-500 hover:text-zinc-400"
                    title="Attach a file or screenshot for context"
                  >
                    <Paperclip className="w-3 h-3" />
                  </button>
                  <input
                    ref={fileInputRef}
                    type="file"
                    multiple
                    accept="image/*,.txt,.md,.py,.js,.ts,.tsx,.jsx,.rs,.json,.yaml,.yml,.toml,.csv,.log,.sh,.bash,.r,.R,.html,.css,.sql,.xml,.ipynb,.h5ad,.h5,.pdf"
                    className="hidden"
                    onChange={async (e) => {
                      const files = e.target.files;
                      if (!files) return;
                      const imageExts = new Set(['png', 'jpg', 'jpeg', 'gif', 'bmp', 'webp', 'svg', 'tiff', 'tif']);
                      const newAttachments: typeof attachments = [];
                      for (const file of Array.from(files)) {
                        const ext = file.name.split('.').pop()?.toLowerCase() || '';
                        const isImage = imageExts.has(ext);
                        // In Tauri 2, File.path is not available like in Electron.
                        // Read the file content and save it to a temp location so Claude can access it.
                        try {
                          const buffer = await file.arrayBuffer();
                          const bytes = new Uint8Array(buffer);
                          let binary = '';
                          for (let i = 0; i < bytes.length; i++) {
                            binary += String.fromCharCode(bytes[i]);
                          }
                          const base64 = btoa(binary);
                          const savedPath = await invoke<string>('save_attachment_file', {
                            data: base64,
                            filename: file.name,
                          });
                          newAttachments.push({
                            name: file.name,
                            path: savedPath,
                            type: isImage ? 'image' : 'file',
                          });
                        } catch (err) {
                          console.error('Failed to save attachment:', file.name, err);
                          // Fallback: use filename only (won't be accessible to Claude)
                          newAttachments.push({
                            name: file.name,
                            path: file.name,
                            type: isImage ? 'image' : 'file',
                          });
                        }
                      }
                      setAttachments(prev => [...prev, ...newAttachments]);
                      // Reset so the same file can be re-selected
                      e.target.value = '';
                    }}
                  />
                </>
              )}
              {/* PubMed toggle — in Ask mode; always-on indicator in Report mode */}
              {mode === 'ask' && (
                <button
                  onClick={() => setPubmedEnabled(v => !v)}
                  className={`flex items-center gap-1 px-1.5 py-0.5 rounded transition-colors text-[11px] ${
                    pubmedEnabled
                      ? 'bg-emerald-900/40 border border-emerald-700/40 text-emerald-400 hover:bg-emerald-900/60'
                      : 'text-zinc-500 hover:text-zinc-400 hover:bg-zinc-800'
                  }`}
                  title={pubmedEnabled ? 'PubMed literature search enabled — click to disable' : 'Enable PubMed literature search for grounded answers'}
                >
                  <BookMarked className="w-3 h-3" />
                  <span>PubMed</span>
                  {pubmedSearching && <Loader2 className="w-2.5 h-2.5 animate-spin ml-0.5" />}
                </button>
              )}
              {mode === 'report' && (
                <span className="flex items-center gap-1 px-1.5 py-0.5 rounded text-[11px] bg-purple-900/40 border border-purple-700/40 text-purple-400">
                  <BookMarked className="w-3 h-3" />
                  <span>PubMed</span>
                  {pubmedSearching && <Loader2 className="w-2.5 h-2.5 animate-spin ml-0.5" />}
                  <span className="text-[9px] text-purple-500">auto</span>
                </span>
              )}
            </div>
            <span className="text-[10px] text-zinc-600">
              {claudeSessionId ? `Session: ${claudeSessionId.slice(0, 8)}` : 'New session'}
            </span>
          </div>

          {/* PubMed results indicator */}
          {lastPubmedResults && lastPubmedResults.length > 0 && (mode === 'ask' || mode === 'report') && (
            <PubMedResultsBar articles={lastPubmedResults} onClear={() => setLastPubmedResults(null)} />
          )}

          <div className="relative">
            {/* @-mention autocomplete popup */}
            <MentionPopup
              items={mentionItems}
              selectedIndex={mentionIndex}
              onSelect={handleMentionSelect}
              visible={mentionActive}
            />

            <textarea
              ref={textareaRef}
              value={input}
              onChange={handleInputChange}
              onKeyDown={handleKeyDown}
              onPaste={async (e) => {
                const items = e.clipboardData?.items;
                if (!items) return;
                for (const item of Array.from(items)) {
                  if (item.type.startsWith('image/')) {
                    e.preventDefault();
                    const blob = item.getAsFile();
                    if (!blob) continue;
                    try {
                      const buffer = await blob.arrayBuffer();
                      const bytes = new Uint8Array(buffer);
                      let binary = '';
                      for (let i = 0; i < bytes.length; i++) {
                        binary += String.fromCharCode(bytes[i]);
                      }
                      const base64 = btoa(binary);
                      const ext = item.type.split('/')[1]?.replace('jpeg', 'jpg') || 'png';
                      const savedPath = await invoke<string>('save_clipboard_image', {
                        data: base64,
                        extension: ext,
                      });
                      const name = savedPath.split('/').pop() || `clipboard.${ext}`;
                      setAttachments(prev => [...prev, { name, path: savedPath, type: 'image' }]);
                    } catch (err) {
                      console.error('Failed to save clipboard image:', err);
                    }
                    return; // handled the image, don't also paste text
                  }
                }
                // If no image items, let the default text paste happen
              }}
              placeholder={
                mode === 'report'
                  ? (reportPhase === 'idle'
                    ? 'Describe your analysis — Claude will scan for files and generate a report...'
                    : reportPhase === 'clarify'
                    ? 'Answer the questions above, then type "generate report" when ready...'
                    : reportPhase === 'done'
                    ? 'Report generated! Start a new report or switch modes...'
                    : 'Report generation in progress...')
                  : mode === 'plan'
                  ? (planReady
                    ? 'Give feedback on the plan — Claude will update implementation_plan.md...'
                    : 'Describe what you want to build — Claude will create a plan...')
                  : mode === 'ask'
                  ? (pubmedEnabled ? 'Ask a question — answers grounded in PubMed literature...' : 'Ask Claude a question — no code changes...')
                  : 'Ask Claude to do something... (type @ to reference files)'
              }
              rows={3}
              className="w-full px-3.5 py-3 pr-20 bg-zinc-900 border border-zinc-700/80 rounded-xl text-[13px] text-zinc-100 placeholder:text-zinc-500 resize-none outline-none focus:border-blue-500/60 focus:ring-1 focus:ring-blue-500/20 transition-all shadow-lg shadow-black/20"
              style={{ minHeight: '72px', maxHeight: '300px' }}
            />
            {/* Mic button — native macOS speech recognition */}
            <button
              onClick={async () => {
                if (isDictating) {
                  try {
                    await invoke('stop_dictation');
                  } catch { /* ignore */ }
                  setIsDictating(false);
                } else {
                  // Save current text so we know what was typed before dictation
                  preDictationText.current = input;
                  textareaRef.current?.focus();
                  try {
                    await invoke('start_dictation');
                    setIsDictating(true);
                  } catch (err: any) {
                    alert(err?.toString() || 'Failed to start dictation');
                  }
                }
              }}
              className={`absolute right-10 bottom-2.5 z-10 p-1.5 rounded-lg transition-all cursor-pointer ${
                isDictating
                  ? 'bg-red-500/30 animate-pulse'
                  : 'opacity-50 hover:opacity-80 hover:bg-zinc-800'
              }`}
              title={isDictating ? 'Stop dictation' : 'Voice input'}
              type="button"
            >
              {isDictating ? (
                <MicOff className="w-4 h-4 text-red-400" />
              ) : (
                <Mic className="w-4 h-4 text-zinc-400" />
              )}
            </button>
            {/* Send / Stop button */}
            <button
              data-send-btn
              onClick={isStreaming ? () => {
                invoke('stop_claude_session', { sessionId }).catch(() => {});
                setIsStreaming(false);
                setMessages((prev) =>
                  prev.map((msg) => (msg.isStreaming ? { ...msg, isStreaming: false } : msg)),
                );
                invoke('update_session_status', { sessionId, status: 'stopped' }).catch(() => {});
              } : sendMessage}
              disabled={!isStreaming && !input.trim()}
              className={`absolute right-2.5 bottom-2.5 z-10 p-1.5 rounded-lg transition-all ${
                isStreaming
                  ? 'bg-red-500/20 hover:bg-red-500/30'
                  : input.trim()
                  ? 'bg-blue-600 hover:bg-blue-500 shadow-md shadow-blue-900/40'
                  : 'opacity-40'
              }`}
              title={isStreaming ? 'Stop' : 'Send (Enter)'}
            >
              {isStreaming ? (
                <Square className="w-4 h-4 text-red-400" />
              ) : (
                <Send className="w-4 h-4 text-white" />
              )}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
