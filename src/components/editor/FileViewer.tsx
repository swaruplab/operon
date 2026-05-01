import { useState, useCallback, useMemo, useEffect } from 'react';
import {
  ZoomIn,
  ZoomOut,
  RotateCw,
  Maximize2,
  Download,
  Image as ImageIcon,
  FileText,
  Globe,
  Code2,
  ExternalLink,
} from 'lucide-react';
import { XlsxViewer } from './XlsxViewer';
import { PptxViewer } from './PptxViewer';

interface FileViewerProps {
  filePath: string;
  base64Content: string;
  mimeType: string;
  binaryType: 'image' | 'pdf' | 'html' | 'xlsx' | 'pptx';
}

// Separate PDF viewer that uses Blob URL instead of data: URI.
// Tauri's WKWebView CSP blocks data: URIs in iframes (default-src 'self'),
// but blob: URLs work because they're same-origin.
function PdfViewer({ fileName, base64Content, onDownload }: {
  fileName: string;
  base64Content: string;
  onDownload: (e: React.MouseEvent) => void;
}) {
  const blobUrl = useMemo(() => {
    try {
      const byteChars = atob(base64Content);
      const byteArray = new Uint8Array(byteChars.length);
      for (let i = 0; i < byteChars.length; i++) {
        byteArray[i] = byteChars.charCodeAt(i);
      }
      const blob = new Blob([byteArray], { type: 'application/pdf' });
      return URL.createObjectURL(blob);
    } catch {
      return null;
    }
  }, [base64Content]);

  // Revoke blob URL on unmount
  useEffect(() => {
    return () => {
      if (blobUrl) URL.revokeObjectURL(blobUrl);
    };
  }, [blobUrl]);

  return (
    <div className="flex flex-col h-full bg-zinc-950">
      <div className="flex items-center justify-between px-3 py-1.5 bg-zinc-900 border-b border-zinc-800 shrink-0">
        <div className="flex items-center gap-2 text-xs text-zinc-400">
          <FileText className="w-4 h-4 text-red-400" />
          <span className="font-medium">{fileName}</span>
        </div>
        <div className="flex items-center gap-1">
          <button
            type="button"
            onClick={onDownload}
            className="flex items-center gap-1 px-2 py-1 rounded text-xs text-zinc-400 hover:text-zinc-200 hover:bg-zinc-800 transition-colors cursor-pointer"
            title="Download"
          >
            <Download className="w-3.5 h-3.5" />
          </button>
        </div>
      </div>
      <div className="flex-1 overflow-hidden">
        {blobUrl ? (
          <iframe
            src={blobUrl}
            className="w-full h-full border-0"
            title={fileName}
          />
        ) : (
          <div className="flex items-center justify-center h-full text-zinc-500 text-sm">
            Failed to load PDF preview
          </div>
        )}
      </div>
    </div>
  );
}

export function FileViewer({ filePath, base64Content, mimeType, binaryType }: FileViewerProps) {
  const [zoom, setZoom] = useState(100);
  const [rotation, setRotation] = useState(0);

  const dataUri = `data:${mimeType};base64,${base64Content}`;
  const fileName = filePath.split('/').pop() || filePath;

  const zoomIn = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setZoom((z) => Math.min(z + 25, 500));
  }, []);

  const zoomOut = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setZoom((z) => Math.max(z - 25, 25));
  }, []);

  const resetZoom = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setZoom(100);
    setRotation(0);
  }, []);

  const rotate = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setRotation((r) => (r + 90) % 360);
  }, []);

  const handleDownload = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();

    try {
      // Convert base64 to blob and trigger download
      const byteCharacters = atob(base64Content);
      const byteNumbers = new Array(byteCharacters.length);
      for (let i = 0; i < byteCharacters.length; i++) {
        byteNumbers[i] = byteCharacters.charCodeAt(i);
      }
      const byteArray = new Uint8Array(byteNumbers);
      const blob = new Blob([byteArray], { type: mimeType });
      const url = URL.createObjectURL(blob);

      const link = document.createElement('a');
      link.href = url;
      link.download = fileName;
      document.body.appendChild(link);
      link.click();
      document.body.removeChild(link);

      // Clean up the object URL after a short delay
      setTimeout(() => URL.revokeObjectURL(url), 1000);
    } catch (err) {
      console.error('Download failed:', err);
    }
  }, [base64Content, mimeType, fileName]);

  if (binaryType === 'html') {
    return (
      <div className="flex flex-col h-full bg-zinc-950">
        {/* Toolbar */}
        <div className="flex items-center justify-between px-3 py-1.5 bg-zinc-900 border-b border-zinc-800 shrink-0">
          <div className="flex items-center gap-2 text-xs text-zinc-400">
            <Globe className="w-4 h-4 text-orange-400" />
            <span className="font-medium">{fileName}</span>
            <span className="text-zinc-600">|</span>
            <span className="text-zinc-500">Preview</span>
          </div>
          <div className="flex items-center gap-1">
            <button
              type="button"
              onClick={handleDownload}
              className="flex items-center gap-1 px-2 py-1 rounded text-xs text-zinc-400 hover:text-zinc-200 hover:bg-zinc-800 transition-colors cursor-pointer"
              title="Download"
            >
              <Download className="w-3.5 h-3.5" />
            </button>
          </div>
        </div>

        {/* HTML preview via iframe */}
        <div className="flex-1 overflow-hidden bg-white">
          <iframe
            src={dataUri}
            className="w-full h-full border-0"
            title={fileName}
            sandbox="allow-scripts allow-same-origin"
          />
        </div>
      </div>
    );
  }

  if (binaryType === 'pdf') {
    return (
      <PdfViewer
        fileName={fileName}
        base64Content={base64Content}
        onDownload={handleDownload}
      />
    );
  }

  if (binaryType === 'xlsx') {
    return (
      <XlsxViewer
        filePath={filePath}
        base64Content={base64Content}
        mimeType={mimeType}
      />
    );
  }

  if (binaryType === 'pptx') {
    return (
      <PptxViewer
        filePath={filePath}
        base64Content={base64Content}
        mimeType={mimeType}
      />
    );
  }

  // Image viewer
  return (
    <div className="flex flex-col h-full bg-zinc-950">
      {/* Toolbar */}
      <div className="flex items-center justify-between px-3 py-1.5 bg-zinc-900 border-b border-zinc-800 shrink-0 z-10">
        <div className="flex items-center gap-2 text-xs text-zinc-400">
          <ImageIcon className="w-4 h-4 text-green-400" />
          <span className="font-medium">{fileName}</span>
          <span className="text-zinc-600">|</span>
          <span className="text-zinc-500">{zoom}%</span>
        </div>
        <div className="flex items-center gap-0.5">
          <button
            type="button"
            onClick={zoomOut}
            className="p-1.5 rounded text-zinc-500 hover:text-zinc-300 hover:bg-zinc-800 transition-colors cursor-pointer"
            title="Zoom Out"
          >
            <ZoomOut className="w-3.5 h-3.5 pointer-events-none" />
          </button>
          <button
            type="button"
            onClick={zoomIn}
            className="p-1.5 rounded text-zinc-500 hover:text-zinc-300 hover:bg-zinc-800 transition-colors cursor-pointer"
            title="Zoom In"
          >
            <ZoomIn className="w-3.5 h-3.5 pointer-events-none" />
          </button>
          <button
            type="button"
            onClick={rotate}
            className="p-1.5 rounded text-zinc-500 hover:text-zinc-300 hover:bg-zinc-800 transition-colors cursor-pointer"
            title="Rotate"
          >
            <RotateCw className="w-3.5 h-3.5 pointer-events-none" />
          </button>
          <button
            type="button"
            onClick={resetZoom}
            className="p-1.5 rounded text-zinc-500 hover:text-zinc-300 hover:bg-zinc-800 transition-colors cursor-pointer"
            title="Fit to View"
          >
            <Maximize2 className="w-3.5 h-3.5 pointer-events-none" />
          </button>
          <div className="w-px h-4 bg-zinc-800 mx-1" />
          <button
            type="button"
            onClick={handleDownload}
            className="p-1.5 rounded text-zinc-500 hover:text-zinc-300 hover:bg-zinc-800 transition-colors cursor-pointer"
            title="Download"
          >
            <Download className="w-3.5 h-3.5 pointer-events-none" />
          </button>
        </div>
      </div>

      {/* Image canvas with checkerboard background */}
      <div
        className="flex-1 overflow-auto flex items-center justify-center"
        style={{
          backgroundImage:
            'linear-gradient(45deg, #1a1a2e 25%, transparent 25%), linear-gradient(-45deg, #1a1a2e 25%, transparent 25%), linear-gradient(45deg, transparent 75%, #1a1a2e 75%), linear-gradient(-45deg, transparent 75%, #1a1a2e 75%)',
          backgroundSize: '20px 20px',
          backgroundPosition: '0 0, 0 10px, 10px -10px, -10px 0px',
          backgroundColor: '#141422',
        }}
      >
        <img
          src={dataUri}
          alt={fileName}
          className="max-w-none select-none"
          style={{
            width: `${zoom}%`,
            transform: `rotate(${rotation}deg)`,
            transition: 'transform 0.2s ease, width 0.2s ease',
            imageRendering: zoom > 200 ? 'pixelated' : 'auto',
          }}
          draggable={false}
        />
      </div>
    </div>
  );
}
