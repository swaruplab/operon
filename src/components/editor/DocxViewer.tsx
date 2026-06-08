import { useEffect, useMemo, useState, useCallback } from 'react';
import { FileText, Download, AlertTriangle } from 'lucide-react';
// Vite resolves this to mammoth's browser bundle via the "browser" field in
// its package.json. Don't switch back to a Node-style subpath import — it
// pulls in `fs` and breaks the build.
import mammoth from 'mammoth';

interface DocxViewerProps {
  filePath: string;
  base64Content: string;
  mimeType: string;
}

interface ConvertResult {
  html: string;
  warnings: string[];
}

export function DocxViewer({ filePath, base64Content, mimeType }: DocxViewerProps) {
  const fileName = filePath.split('/').pop() || filePath;
  const [result, setResult] = useState<ConvertResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  const arrayBuffer = useMemo(() => {
    try {
      const byteChars = atob(base64Content);
      const byteArray = new Uint8Array(byteChars.length);
      for (let i = 0; i < byteChars.length; i++) {
        byteArray[i] = byteChars.charCodeAt(i);
      }
      return byteArray.buffer;
    } catch (err) {
      console.error('Failed to decode docx:', err);
      return null;
    }
  }, [base64Content]);

  useEffect(() => {
    if (!arrayBuffer) {
      setError('Could not decode document bytes');
      setLoading(false);
      return;
    }

    let cancelled = false;
    setLoading(true);
    setError(null);

    mammoth
      .convertToHtml({ arrayBuffer: arrayBuffer.slice(0) })
      .then((res: { value: string; messages: Array<{ message: string }> }) => {
        if (cancelled) return;
        setResult({
          html: res.value,
          warnings: res.messages.map((m) => m.message).slice(0, 5),
        });
        setLoading(false);
      })
      .catch((err: unknown) => {
        if (cancelled) return;
        setError(err instanceof Error ? err.message : 'Failed to render document');
        setLoading(false);
      });

    return () => {
      cancelled = true;
    };
  }, [arrayBuffer]);

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
          <FileText className="w-4 h-4 text-blue-600 dark:text-blue-400 shrink-0" />
          <span className="font-medium truncate">{fileName}</span>
          <span className="text-[10px] text-subtle ml-1">
            Preview (formatting approximated)
          </span>
        </div>
        <div className="flex items-center gap-1 shrink-0">
          <button
            type="button"
            onClick={handleDownload}
            className="flex items-center gap-1 px-2 py-1 rounded text-xs text-secondary hover:text-primary hover:bg-hover transition-colors cursor-pointer"
            title="Download to open in Word / Pages"
          >
            <Download className="w-3.5 h-3.5 pointer-events-none" />
          </button>
        </div>
      </div>

      {/* Rendered document */}
      <div className="flex-1 overflow-auto bg-zinc-200 dark:bg-zinc-900">
        {error ? (
          <div className="flex flex-col items-center justify-center h-full text-subtle text-sm p-6 gap-3">
            <AlertTriangle className="w-8 h-8 text-orange-500" />
            <div className="font-medium">Could not render document</div>
            <div className="text-xs text-subtle max-w-md text-center">{error}</div>
            <button
              type="button"
              onClick={handleDownload}
              className="flex items-center gap-2 px-3 py-1.5 mt-2 rounded text-xs text-white bg-blue-600 hover:bg-blue-500 transition-colors"
            >
              <Download className="w-3.5 h-3.5 pointer-events-none" />
              Download to open externally
            </button>
          </div>
        ) : loading ? (
          <div className="flex items-center justify-center h-full text-muted text-sm">
            Rendering document…
          </div>
        ) : (
          <div className="docx-host">
            <div
              className="docx-page"
              dangerouslySetInnerHTML={{ __html: result?.html ?? '' }}
            />
          </div>
        )}
      </div>

      <style>{`
        .docx-host {
          padding: 24px;
          display: flex;
          justify-content: center;
        }
        .docx-page {
          background: white;
          color: #1a1a1a;
          width: 100%;
          max-width: 816px;
          min-height: 1056px;
          padding: 72px 96px;
          box-shadow: 0 2px 12px rgba(0, 0, 0, 0.18);
          font-family: "Calibri", ui-sans-serif, system-ui, -apple-system, sans-serif;
          font-size: 14px;
          line-height: 1.5;
        }
        .docx-page h1 {
          font-size: 24px;
          font-weight: 600;
          margin: 16px 0 8px;
        }
        .docx-page h2 {
          font-size: 20px;
          font-weight: 600;
          margin: 14px 0 6px;
        }
        .docx-page h3 {
          font-size: 17px;
          font-weight: 600;
          margin: 12px 0 6px;
        }
        .docx-page h4, .docx-page h5, .docx-page h6 {
          font-weight: 600;
          margin: 10px 0 4px;
        }
        .docx-page p {
          margin: 0 0 10px;
        }
        .docx-page ul, .docx-page ol {
          margin: 0 0 10px 24px;
          padding: 0;
        }
        .docx-page li {
          margin: 0 0 4px;
        }
        .docx-page table {
          border-collapse: collapse;
          margin: 12px 0;
          font-size: 13px;
        }
        .docx-page td, .docx-page th {
          border: 1px solid #d4d4d8;
          padding: 6px 10px;
          vertical-align: top;
        }
        .docx-page th {
          background: #f4f4f5;
          font-weight: 600;
        }
        .docx-page img {
          max-width: 100%;
          height: auto;
        }
        .docx-page a {
          color: #2563eb;
          text-decoration: underline;
        }
        .docx-page blockquote {
          border-left: 3px solid #d4d4d8;
          padding-left: 12px;
          margin: 8px 0 8px 4px;
          color: #52525b;
        }
        .docx-page code, .docx-page pre {
          font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
          font-size: 12.5px;
        }
        .docx-page pre {
          background: #f4f4f5;
          padding: 10px 12px;
          border-radius: 4px;
          overflow-x: auto;
        }
      `}</style>
    </div>
  );
}
