import { useState, useEffect, useCallback } from 'react';
import { Download, X, Loader2, CheckCircle, RefreshCw } from 'lucide-react';
import { check } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';

type UpdateState = 'idle' | 'checking' | 'available' | 'downloading' | 'ready' | 'up-to-date';

interface UpdateInfo {
  version: string;
  body: string;
}

export function UpdateChecker() {
  const [state, setState] = useState<UpdateState>('idle');
  const [updateInfo, setUpdateInfo] = useState<UpdateInfo | null>(null);
  const [progress, setProgress] = useState(0);
  const [dismissed, setDismissed] = useState(false);

  // Skip update checks in dev mode
  const isDev = __APP_VERSION__ === 'dev';

  const checkForUpdate = useCallback(async () => {
    if (isDev) return;
    setState('checking');
    try {
      const update = await check();
      if (update) {
        setUpdateInfo({
          version: update.version,
          body: update.body || '',
        });
        setState('available');
      } else {
        setState('up-to-date');
        // Auto-dismiss "up to date" after 3s
        setTimeout(() => setState('idle'), 3000);
      }
    } catch (err) {
      console.warn('Update check failed:', err);
      setState('idle');
    }
  }, [isDev]);

  // Check for updates on mount (with a small delay to not block startup)
  useEffect(() => {
    if (isDev) return;
    const timer = setTimeout(() => {
      checkForUpdate();
    }, 5000);
    return () => clearTimeout(timer);
  }, [checkForUpdate]);

  const downloadAndInstall = useCallback(async () => {
    setState('downloading');
    setProgress(0);
    try {
      const update = await check();
      if (!update) {
        setState('idle');
        return;
      }

      let downloaded = 0;
      let contentLength = 0;

      await update.downloadAndInstall((event) => {
        switch (event.event) {
          case 'Started':
            contentLength = event.data.contentLength ?? 0;
            break;
          case 'Progress':
            downloaded += event.data.chunkLength;
            if (contentLength > 0) {
              setProgress(Math.round((downloaded / contentLength) * 100));
            }
            break;
          case 'Finished':
            break;
        }
      });

      setState('ready');
    } catch (err) {
      console.warn('Update download failed:', err);
      setState('idle');
    }
  }, []);

  const handleRelaunch = useCallback(async () => {
    await relaunch();
  }, []);

  // Don't show anything if idle or dismissed
  if (state === 'idle' || dismissed) return null;

  // Checking spinner in TopBar area
  if (state === 'checking') return null; // Silent check

  // Up to date — brief toast
  if (state === 'up-to-date') {
    return (
      <div className="flex items-center gap-1.5 px-2 py-1 rounded text-xs text-green-400">
        <CheckCircle className="w-3.5 h-3.5 pointer-events-none" />
        <span>Up to date</span>
      </div>
    );
  }


  // Update available
  if (state === 'available' && updateInfo) {
    return (
      <div className="flex items-center gap-2 px-2 py-1 rounded bg-blue-900/30 border border-blue-800/40 text-xs">
        <Download className="w-3.5 h-3.5 text-blue-400 pointer-events-none shrink-0" />
        <span className="text-blue-300">v{updateInfo.version} available</span>
        <button
          onClick={downloadAndInstall}
          className="px-2 py-0.5 rounded bg-blue-600 hover:bg-blue-500 text-white text-[11px] font-medium transition-colors"
        >
          Update
        </button>
        <button
          onClick={() => { setDismissed(true); }}
          className="p-0.5 rounded hover:bg-zinc-700 text-zinc-400"
        >
          <X className="w-3 h-3 pointer-events-none" />
        </button>
      </div>
    );
  }

  // Downloading
  if (state === 'downloading') {
    return (
      <div className="flex items-center gap-2 px-2 py-1 rounded bg-blue-900/30 border border-blue-800/40 text-xs">
        <Loader2 className="w-3.5 h-3.5 text-blue-400 animate-spin pointer-events-none" />
        <span className="text-blue-300">Downloading... {progress}%</span>
        <div className="w-16 h-1.5 bg-zinc-700 rounded-full overflow-hidden">
          <div
            className="h-full bg-blue-500 rounded-full transition-all duration-300"
            style={{ width: `${progress}%` }}
          />
        </div>
      </div>
    );
  }

  // Ready to relaunch
  if (state === 'ready') {
    return (
      <div className="flex items-center gap-2 px-2 py-1 rounded bg-green-900/30 border border-green-800/40 text-xs">
        <CheckCircle className="w-3.5 h-3.5 text-green-400 pointer-events-none" />
        <span className="text-green-300">Update ready</span>
        <button
          onClick={handleRelaunch}
          className="px-2 py-0.5 rounded bg-green-600 hover:bg-green-500 text-white text-[11px] font-medium transition-colors flex items-center gap-1"
        >
          <RefreshCw className="w-3 h-3 pointer-events-none" />
          Restart
        </button>
      </div>
    );
  }

  return null;
}
