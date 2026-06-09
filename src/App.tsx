import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { emit } from '@tauri-apps/api/event';
import { loader } from '@monaco-editor/react';
import * as monaco from 'monaco-editor';
import { AppShell } from './components/layout/AppShell';
import { ProjectProvider } from './context/ProjectContext';
import { ThemeProvider } from './context/ThemeContext';
import { SetupWizard } from './components/setup/SetupWizard';
import { getApiKey } from './lib/claude';
import { refreshModelsIfStale } from './lib/models';
import { refreshPortkeyPresets } from './lib/portkey';
import {
  loadSessionState,
  clearSessionState,
  isRestoreCandidate,
  type SessionState,
} from './lib/session';

// Configure Monaco to use the local bundle instead of CDN.
// This is critical for Tauri because CSP blocks external scripts.
loader.config({ monaco });

function App() {
  const [setupComplete, setSetupComplete] = useState<boolean | null>(null);
  const [restoreCandidate, setRestoreCandidate] = useState<SessionState | null>(null);

  useEffect(() => {
    // Check if setup has been completed before
    invoke<{ setup_completed?: boolean }>('get_settings')
      .then((settings) => {
        setSetupComplete(settings.setup_completed ?? false);
      })
      .catch(() => {
        // If settings can't be loaded, show setup
        setSetupComplete(false);
      });

    // Auto-refresh the Anthropic model catalog if the cache is > 7 days old.
    // Silent on failure — UI falls back to the bundled list.
    getApiKey()
      .then((key) => refreshModelsIfStale(key))
      .catch(() => {});

    // Same idea for the Portkey gateway preset list — pulls any new
    // institutional presets added to operon/presets/portkey.json on GitHub.
    refreshPortkeyPresets().catch(() => {});

    // Last-session restore candidate (task #72) — only LOCAL files/terminals
    // are restorable in v1. Remote tabs/terminals are skipped with a count
    // surfaced in the prompt.
    loadSessionState()
      .then((state) => {
        if (isRestoreCandidate(state)) setRestoreCandidate(state);
      })
      .catch(() => {});
  }, []);

  const skippedRemoteTabs =
    restoreCandidate?.editor_tabs.filter((t) => t.is_remote).length ?? 0;
  const skippedRemoteTerminals =
    restoreCandidate?.terminal_tabs.filter((t) => t.is_remote).length ?? 0;
  const skippedTotal = skippedRemoteTabs + skippedRemoteTerminals;

  const handleRestore = async () => {
    if (!restoreCandidate) return;
    const state = restoreCandidate;
    setRestoreCandidate(null);
    // Re-emit open-file events for each local tab — ProjectContext already
    // listens for these (lines 239–256), so we get exactly the same code
    // path as a click in the file explorer.
    for (const tab of state.editor_tabs) {
      if (tab.is_remote) continue;
      try {
        await emit('open-file', { path: tab.file_path });
      } catch {
        // file may have moved/been deleted — skip silently
      }
    }
    // Persist the chat session id slot so ChatPanel's existing Phase 9 resume
    // flow picks it up (it queries list_sessions / reconnect_session on mount).
    // No direct event needed — saved_at + the id are already on disk.
  };

  const handleDismissRestore = () => {
    setRestoreCandidate(null);
    clearSessionState().catch(() => {});
  };

  // Loading state — checking settings
  if (setupComplete === null) {
    return (
      <div className="h-screen w-screen bg-canvas flex items-center justify-center">
        <div className="w-6 h-6 border-2 border-blue-500 border-t-transparent rounded-full animate-spin" />
      </div>
    );
  }

  return (
    <ThemeProvider>
      {!setupComplete ? (
        <SetupWizard onComplete={() => setSetupComplete(true)} />
      ) : (
        <ProjectProvider>
          <AppShell />
          {restoreCandidate && (
            <div className="fixed bottom-4 right-4 z-50 max-w-sm rounded-md border border-zinc-800 bg-zinc-900 shadow-lg p-3 text-xs text-zinc-50">
              <div className="font-medium mb-1">Restore last session?</div>
              <div className="text-zinc-400 mb-2">
                {restoreCandidate.editor_tabs.filter((t) => !t.is_remote).length} file(s),{' '}
                {restoreCandidate.terminal_tabs.filter((t) => !t.is_remote).length} terminal(s)
                {skippedTotal > 0 && (
                  <span className="text-amber-400">
                    {' '}
                    ({skippedTotal} remote skipped — reconnect first)
                  </span>
                )}
              </div>
              <div className="flex gap-2">
                <button
                  onClick={handleRestore}
                  className="px-2 py-1 rounded bg-blue-500 hover:bg-blue-400 text-white"
                >
                  Restore
                </button>
                <button
                  onClick={handleDismissRestore}
                  className="px-2 py-1 rounded bg-zinc-800 hover:bg-zinc-700 text-zinc-300"
                >
                  Start fresh
                </button>
              </div>
            </div>
          )}
        </ProjectProvider>
      )}
    </ThemeProvider>
  );
}

export default App;
