import { useMemo, useState, useCallback, useEffect } from 'react';
import { Sheet, Download } from 'lucide-react';
import * as XLSX from 'xlsx';

interface XlsxViewerProps {
  filePath: string;
  base64Content: string;
  mimeType: string;
}

export function XlsxViewer({ filePath, base64Content, mimeType }: XlsxViewerProps) {
  const fileName = filePath.split('/').pop() || filePath;
  const [activeSheet, setActiveSheet] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const workbook = useMemo(() => {
    try {
      const byteChars = atob(base64Content);
      const byteArray = new Uint8Array(byteChars.length);
      for (let i = 0; i < byteChars.length; i++) {
        byteArray[i] = byteChars.charCodeAt(i);
      }
      return XLSX.read(byteArray, { type: 'array' });
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to parse spreadsheet');
      return null;
    }
  }, [base64Content]);

  useEffect(() => {
    if (workbook && !activeSheet) {
      setActiveSheet(workbook.SheetNames[0] ?? null);
    }
  }, [workbook, activeSheet]);

  const sheetHtml = useMemo(() => {
    if (!workbook || !activeSheet) return '';
    try {
      const sheet = workbook.Sheets[activeSheet];
      if (!sheet) return '';
      return XLSX.utils.sheet_to_html(sheet, { editable: false });
    } catch (err) {
      console.error('Failed to render sheet:', err);
      return '';
    }
  }, [workbook, activeSheet]);

  const handleDownload = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    try {
      const byteChars = atob(base64Content);
      const byteArray = new Uint8Array(byteChars.length);
      for (let i = 0; i < byteChars.length; i++) {
        byteArray[i] = byteChars.charCodeAt(i);
      }
      const blob = new Blob([byteArray], { type: mimeType });
      const url = URL.createObjectURL(blob);
      const link = document.createElement('a');
      link.href = url;
      link.download = fileName;
      document.body.appendChild(link);
      link.click();
      document.body.removeChild(link);
      setTimeout(() => URL.revokeObjectURL(url), 1000);
    } catch (err) {
      console.error('Download failed:', err);
    }
  }, [base64Content, mimeType, fileName]);

  return (
    <div className="flex flex-col h-full bg-canvas">
      {/* Toolbar */}
      <div className="flex items-center justify-between px-3 py-1.5 bg-panel border-b border-border-default shrink-0">
        <div className="flex items-center gap-2 text-xs text-secondary min-w-0">
          <Sheet className="w-4 h-4 text-green-600 dark:text-green-400 shrink-0" />
          <span className="font-medium truncate">{fileName}</span>
          {workbook && (
            <>
              <span className="text-subtle">|</span>
              <span className="text-muted">
                {workbook.SheetNames.length} sheet{workbook.SheetNames.length === 1 ? '' : 's'}
              </span>
            </>
          )}
          <span className="text-[10px] text-subtle ml-1">Read-only</span>
        </div>
        <div className="flex items-center gap-1 shrink-0">
          <button
            type="button"
            onClick={handleDownload}
            className="flex items-center gap-1 px-2 py-1 rounded text-xs text-secondary hover:text-primary hover:bg-hover transition-colors cursor-pointer"
            title="Download"
          >
            <Download className="w-3.5 h-3.5 pointer-events-none" />
          </button>
        </div>
      </div>

      {/* Sheet tabs */}
      {workbook && workbook.SheetNames.length > 0 && (
        <div className="flex items-center h-[28px] bg-panel border-b border-border-default overflow-x-auto shrink-0">
          {workbook.SheetNames.map((name) => (
            <button
              key={name}
              type="button"
              onClick={() => setActiveSheet(name)}
              className={`px-3 h-full text-[12px] border-r border-border-default shrink-0 transition-colors ${
                activeSheet === name
                  ? 'bg-canvas text-primary border-t-2 border-t-green-500'
                  : 'text-muted hover:text-secondary bg-panel border-t-2 border-t-transparent'
              }`}
            >
              {name}
            </button>
          ))}
        </div>
      )}

      {/* Sheet content */}
      <div className="flex-1 overflow-auto bg-white text-zinc-900">
        {error ? (
          <div className="flex items-center justify-center h-full text-red-500 text-sm p-4">
            {error}
          </div>
        ) : sheetHtml ? (
          <div className="xlsx-sheet" dangerouslySetInnerHTML={{ __html: sheetHtml }} />
        ) : (
          <div className="flex items-center justify-center h-full text-muted text-sm">
            Loading spreadsheet…
          </div>
        )}
      </div>

      {/* Inline styles for the SheetJS-generated table */}
      <style>{`
        .xlsx-sheet table {
          border-collapse: collapse;
          font-family: ui-sans-serif, system-ui, -apple-system, sans-serif;
          font-size: 12px;
        }
        .xlsx-sheet table, .xlsx-sheet td, .xlsx-sheet th {
          border: 1px solid #e4e4e7;
        }
        .xlsx-sheet td, .xlsx-sheet th {
          padding: 4px 8px;
          vertical-align: top;
          white-space: nowrap;
        }
        .xlsx-sheet tr:nth-child(even) td {
          background: #fafafa;
        }
      `}</style>
    </div>
  );
}
