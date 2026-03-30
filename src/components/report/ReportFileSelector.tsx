import { useState, useCallback } from 'react';
import {
  Folder, FolderOpen, File, Image, FileText, Table2, Code2,
  ChevronRight, ChevronDown, Check, AlertTriangle,
} from 'lucide-react';
import type { ScanTreeNode, ScannedFile, ProjectScan } from '../../types/report';
import { formatBytes } from '../../lib/report';

interface ReportFileSelectorProps {
  scan: ProjectScan;
  selectedFiles: string[];
  onSelectionChange: (paths: string[]) => void;
  maxFiles?: number;
  maxSize?: number; // bytes
}

function getFileIcon(fileType: string) {
  switch (fileType) {
    case 'pdf': return <FileText className="w-3.5 h-3.5 text-red-400 shrink-0" />;
    case 'image': return <Image className="w-3.5 h-3.5 text-green-400 shrink-0" />;
    case 'csv': return <Table2 className="w-3.5 h-3.5 text-yellow-400 shrink-0" />;
    case 'code': return <Code2 className="w-3.5 h-3.5 text-purple-400 shrink-0" />;
    default: return <File className="w-3.5 h-3.5 text-zinc-400 shrink-0" />;
  }
}

function getHintBadge(hint?: string) {
  if (!hint) return null;
  const colors: Record<string, string> = {
    results: 'bg-green-900/30 text-green-400 border-green-800/30',
    plots: 'bg-blue-900/30 text-blue-400 border-blue-800/30',
    raw: 'bg-zinc-800 text-zinc-500 border-zinc-700',
    intermediate: 'bg-zinc-800 text-zinc-500 border-zinc-700',
    scripts: 'bg-purple-900/30 text-purple-400 border-purple-800/30',
    reference: 'bg-amber-900/30 text-amber-400 border-amber-800/30',
  };
  return (
    <span className={`text-[8px] uppercase tracking-wide px-1 py-[1px] rounded border ${colors[hint] || 'bg-zinc-800 text-zinc-500 border-zinc-700'}`}>
      {hint}
    </span>
  );
}

function FileRow({ file, selected, onToggle }: {
  file: ScannedFile;
  selected: boolean;
  onToggle: (path: string) => void;
}) {
  return (
    <button
      onClick={() => onToggle(file.path)}
      className={`w-full flex items-center gap-1.5 h-[26px] px-2 text-[12px] transition-colors ${
        selected ? 'bg-blue-950/30 text-blue-300' : 'text-zinc-400 hover:bg-zinc-800/60'
      }`}
    >
      <div className={`w-3.5 h-3.5 rounded border flex items-center justify-center shrink-0 ${
        selected ? 'bg-blue-500 border-blue-500' : 'border-zinc-600'
      }`}>
        {selected && <Check className="w-2.5 h-2.5 text-white" />}
      </div>
      {getFileIcon(file.file_type)}
      <span className="truncate">{file.name}</span>
      {file.size > 1024 * 1024 && (
        <span title="Large file (>1 MB) — may use significant context"><AlertTriangle className="w-3 h-3 text-amber-500 shrink-0" /></span>
      )}
      <span className={`ml-auto text-[10px] shrink-0 ${file.size > 1024 * 1024 ? 'text-amber-500' : 'text-zinc-600'}`}>{formatBytes(file.size)}</span>
      {file.columns && (
        <span className="text-[9px] text-zinc-600 shrink-0">{file.columns.length} cols</span>
      )}
    </button>
  );
}

function DirNode({ node, depth, selectedFiles, onToggle, onToggleDir }: {
  node: ScanTreeNode;
  depth: number;
  selectedFiles: string[];
  onToggle: (path: string) => void;
  onToggleDir: (node: ScanTreeNode, select: boolean) => void;
}) {
  const [expanded, setExpanded] = useState(depth < 2); // Auto-expand first 2 levels

  const allFiles = getAllFiles(node);
  const selectedCount = allFiles.filter(f => selectedFiles.includes(f.path)).length;
  const allSelected = allFiles.length > 0 && selectedCount === allFiles.length;
  const someSelected = selectedCount > 0 && selectedCount < allFiles.length;

  if (node.total_file_count === 0 && node.files.length === 0) return null;

  return (
    <div>
      <div className="flex items-center" style={{ paddingLeft: `${depth * 12}px` }}>
        {/* Dir checkbox */}
        <button
          onClick={() => onToggleDir(node, !allSelected)}
          className={`w-3.5 h-3.5 rounded border flex items-center justify-center shrink-0 mr-1.5 ${
            allSelected ? 'bg-blue-500 border-blue-500' :
            someSelected ? 'bg-blue-500/30 border-blue-500' : 'border-zinc-600'
          }`}
        >
          {(allSelected || someSelected) && <Check className="w-2.5 h-2.5 text-white" />}
        </button>

        {/* Dir row */}
        <button
          onClick={() => setExpanded(!expanded)}
          className="flex-1 flex items-center gap-1 h-[28px] text-[12px] text-zinc-300 hover:bg-zinc-800/40 transition-colors rounded px-1"
        >
          {expanded
            ? <ChevronDown className="w-3 h-3 text-zinc-500 shrink-0" />
            : <ChevronRight className="w-3 h-3 text-zinc-500 shrink-0" />
          }
          {expanded
            ? <FolderOpen className="w-3.5 h-3.5 text-blue-400 shrink-0" />
            : <Folder className="w-3.5 h-3.5 text-zinc-500 shrink-0" />
          }
          <span className="truncate font-medium">{node.name}</span>
          {getHintBadge(node.hint)}
          <span className="ml-auto text-[9px] text-zinc-600 shrink-0">
            {node.total_file_count} file{node.total_file_count !== 1 ? 's' : ''}
          </span>
        </button>
      </div>

      {expanded && (
        <>
          {node.files.map(f => (
            <div key={f.path} style={{ paddingLeft: `${(depth + 1) * 12 + 8}px` }}>
              <FileRow file={f} selected={selectedFiles.includes(f.path)} onToggle={onToggle} />
            </div>
          ))}
          {node.children.map(child => (
            <DirNode
              key={child.path}
              node={child}
              depth={depth + 1}
              selectedFiles={selectedFiles}
              onToggle={onToggle}
              onToggleDir={onToggleDir}
            />
          ))}
        </>
      )}
    </div>
  );
}

function getAllFiles(node: ScanTreeNode): ScannedFile[] {
  const files = [...node.files];
  for (const child of node.children) {
    files.push(...getAllFiles(child));
  }
  return files;
}

export function ReportFileSelector({ scan, selectedFiles, onSelectionChange, maxFiles = 40, maxSize = 15 * 1024 * 1024 }: ReportFileSelectorProps) {
  const allFiles = getAllFiles(scan.root);
  const selectedSize = allFiles
    .filter(f => selectedFiles.includes(f.path))
    .reduce((sum, f) => sum + f.size, 0);

  const overLimit = selectedFiles.length > maxFiles || selectedSize > maxSize;

  const toggleFile = useCallback((path: string) => {
    onSelectionChange(
      selectedFiles.includes(path)
        ? selectedFiles.filter(p => p !== path)
        : [...selectedFiles, path]
    );
  }, [selectedFiles, onSelectionChange]);

  const toggleDir = useCallback((node: ScanTreeNode, select: boolean) => {
    const dirFiles = getAllFiles(node).map(f => f.path);
    if (select) {
      const newPaths = new Set([...selectedFiles, ...dirFiles]);
      onSelectionChange([...newPaths]);
    } else {
      onSelectionChange(selectedFiles.filter(p => !dirFiles.includes(p)));
    }
  }, [selectedFiles, onSelectionChange]);

  const selectAll = () => onSelectionChange(allFiles.map(f => f.path));
  const clearAll = () => onSelectionChange([]);

  return (
    <div className="flex flex-col border border-zinc-800 rounded-lg bg-zinc-900/40 overflow-hidden">
      {/* Header */}
      <div className="flex items-center justify-between px-3 py-2 border-b border-zinc-800/60 bg-zinc-800/20">
        <div className="flex items-center gap-2">
          <span className="text-[11px] font-medium text-zinc-300">Select files for report</span>
          <span className="text-[10px] text-zinc-500">
            {scan.total_pdfs} PDFs, {scan.total_images} images, {scan.total_csvs} CSVs, {scan.total_docs} docs
          </span>
        </div>
        <div className="flex items-center gap-2">
          <button onClick={selectAll} className="text-[10px] text-blue-400 hover:text-blue-300">Select all</button>
          <button onClick={clearAll} className="text-[10px] text-zinc-500 hover:text-zinc-300">Clear</button>
        </div>
      </div>

      {/* Budget bar */}
      <div className="flex items-center gap-3 px-3 py-1.5 border-b border-zinc-800/40 bg-zinc-950/30">
        <div className="flex items-center gap-1.5 text-[10px]">
          <span className={selectedFiles.length > maxFiles ? 'text-red-400' : 'text-zinc-400'}>
            {selectedFiles.length}/{maxFiles} files
          </span>
        </div>
        <div className="flex-1 h-1.5 rounded-full bg-zinc-800 overflow-hidden">
          <div
            className={`h-full rounded-full transition-all ${overLimit ? 'bg-red-500' : 'bg-blue-500'}`}
            style={{ width: `${Math.min((selectedSize / maxSize) * 100, 100)}%` }}
          />
        </div>
        <span className={`text-[10px] ${overLimit ? 'text-red-400' : 'text-zinc-500'}`}>
          {formatBytes(selectedSize)} / {formatBytes(maxSize)}
        </span>
      </div>

      {overLimit ? (
        <div className="flex items-center gap-1.5 px-3 py-1.5 bg-red-950/20 text-[10px] text-red-400">
          <AlertTriangle className="w-3 h-3 shrink-0" />
          Too many files or too large. Select representative files for the best report.
        </div>
      ) : (
        <div className="px-3 py-1 text-[9px] text-zinc-600">
          Tip: Avoid selecting files over 1 MB — they consume significant context and may reduce report quality.
        </div>
      )}

      {/* File tree */}
      <div className="max-h-[300px] overflow-y-auto py-1">
        {scan.root.files.map(f => (
          <div key={f.path} style={{ paddingLeft: '8px' }}>
            <FileRow file={f} selected={selectedFiles.includes(f.path)} onToggle={toggleFile} />
          </div>
        ))}
        {scan.root.children.map(child => (
          <DirNode
            key={child.path}
            node={child}
            depth={0}
            selectedFiles={selectedFiles}
            onToggle={toggleFile}
            onToggleDir={toggleDir}
          />
        ))}
        {allFiles.length === 0 && (
          <div className="px-4 py-6 text-center text-zinc-600 text-[11px]">
            No PDFs, images, or CSVs found in this project.
          </div>
        )}
      </div>
    </div>
  );
}
