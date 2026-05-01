import { useEffect, useRef, useState, useCallback, useMemo } from 'react';
import { Presentation, Download, AlertTriangle } from 'lucide-react';
import { init as initPptxPreview } from 'pptx-preview';

interface PptxViewerProps {
  filePath: string;
  base64Content: string;
  mimeType: string;
}

export function PptxViewer({ filePath, base64Content, mimeType }: PptxViewerProps) {
  const fileName = filePath.split('/').pop() || filePath;
  const containerRef = useRef<HTMLDivElement | null>(null);
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
      console.error('Failed to decode pptx:', err);
      return null;
    }
  }, [base64Content]);

  useEffect(() => {
    if (!containerRef.current || !arrayBuffer) return;

    // Clear any prior render before re-initializing
    const dom = containerRef.current;
    dom.innerHTML = '';

    setLoading(true);
    setError(null);

    let cancelled = false;
    try {
      const previewer = initPptxPreview(dom, {
        width: dom.clientWidth || 960,
        height: dom.clientHeight || 540,
        mode: 'list',
      });
      previewer
        .preview(arrayBuffer.slice(0))
        .then(() => {
          if (!cancelled) setLoading(false);
        })
        .catch((err: unknown) => {
          if (!cancelled) {
            setError(err instanceof Error ? err.message : 'Failed to render presentation');
            setLoading(false);
          }
        });
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to initialize previewer');
      setLoading(false);
    }

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
    <div className="flex flex-col h-full bg-zinc-950">
      {/* Toolbar */}
      <div className="flex items-center justify-between px-3 py-1.5 bg-zinc-900 border-b border-zinc-800 shrink-0">
        <div className="flex items-center gap-2 text-xs text-zinc-400 min-w-0">
          <Presentation className="w-4 h-4 text-orange-400 shrink-0" />
          <span className="font-medium truncate">{fileName}</span>
          <span className="text-[10px] text-zinc-600 ml-1">Preview (no animations)</span>
        </div>
        <div className="flex items-center gap-1 shrink-0">
          <button
            type="button"
            onClick={handleDownload}
            className="flex items-center gap-1 px-2 py-1 rounded text-xs text-zinc-400 hover:text-zinc-200 hover:bg-zinc-800 transition-colors cursor-pointer"
            title="Download to open in PowerPoint/Keynote"
          >
            <Download className="w-3.5 h-3.5 pointer-events-none" />
          </button>
        </div>
      </div>

      {/* Slide list */}
      <div className="flex-1 overflow-auto bg-zinc-200">
        {error ? (
          <div className="flex flex-col items-center justify-center h-full text-zinc-700 text-sm p-6 gap-3">
            <AlertTriangle className="w-8 h-8 text-orange-500" />
            <div className="font-medium">Could not render presentation</div>
            <div className="text-xs text-zinc-600 max-w-md text-center">{error}</div>
            <button
              type="button"
              onClick={handleDownload}
              className="flex items-center gap-2 px-3 py-1.5 mt-2 rounded text-xs text-white bg-orange-600 hover:bg-orange-500 transition-colors"
            >
              <Download className="w-3.5 h-3.5 pointer-events-none" />
              Download to open externally
            </button>
          </div>
        ) : (
          <>
            {loading && (
              <div className="flex items-center justify-center h-full text-zinc-500 text-sm">
                Rendering slides…
              </div>
            )}
            <div ref={containerRef} className="pptx-preview-host w-full" />
          </>
        )}
      </div>

      <style>{`
        .pptx-preview-host {
          padding: 16px;
          display: flex;
          flex-direction: column;
          gap: 16px;
          align-items: center;
        }
        .pptx-preview-host > * {
          background: white;
          box-shadow: 0 2px 8px rgba(0,0,0,0.15);
        }
      `}</style>
    </div>
  );
}
