import { useState } from 'react';
import { DiffEditor } from '@monaco-editor/react';
import { detectLanguage, monacoThemeNameFor } from './CodeEditor';
import { useTheme } from '../../context/ThemeContext';

interface DiffViewerProps {
  filePath: string;
  original: string;
  modified: string;
  onAccept?: () => void;
  onReject?: () => void;
}

export function DiffViewer({
  filePath,
  original,
  modified,
  onAccept,
  onReject,
}: DiffViewerProps) {
  const [sideBySide, setSideBySide] = useState(true);
  const { resolved } = useTheme();
  const activeTheme = monacoThemeNameFor(resolved);

  return (
    <div className="flex flex-col h-full">
      {/* Toolbar */}
      <div className="flex items-center justify-between px-3 py-1.5 bg-panel border-b border-border-default">
        <span className="text-xs text-secondary truncate">{filePath}</span>
        <div className="flex items-center gap-2">
          <button
            onClick={() => setSideBySide((v) => !v)}
            className="text-xs text-secondary hover:text-primary px-2 py-0.5 rounded bg-surface"
          >
            {sideBySide ? 'Inline' : 'Side by Side'}
          </button>
          {onReject && (
            <button
              onClick={onReject}
              className="text-xs text-red-600 dark:text-red-400 hover:text-red-800 dark:hover:text-red-700 px-2 py-0.5 rounded bg-surface"
            >
              Reject
            </button>
          )}
          {onAccept && (
            <button
              onClick={onAccept}
              className="text-xs text-green-600 dark:text-green-400 hover:text-green-800 dark:hover:text-green-700 px-2 py-0.5 rounded bg-surface"
            >
              Accept
            </button>
          )}
        </div>
      </div>

      {/* Diff Editor */}
      <div className="flex-1">
        <DiffEditor
          height="100%"
          original={original}
          modified={modified}
          language={detectLanguage(filePath)}
          theme={activeTheme}
          options={{
            readOnly: true,
            renderSideBySide: sideBySide,
            enableSplitViewResizing: true,
            ignoreTrimWhitespace: false,
            renderIndicators: true,
            originalEditable: false,
            automaticLayout: true,
            fontSize: 13,
            fontFamily: "'JetBrains Mono', 'SF Mono', Menlo, Monaco, monospace",
            minimap: { enabled: false },
            scrollBeyondLastLine: false,
          }}
        />
      </div>
    </div>
  );
}
