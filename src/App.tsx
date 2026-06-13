import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { loader } from '@monaco-editor/react';
import * as monaco from 'monaco-editor';
import { AppShell } from './components/layout/AppShell';
import { ProjectProvider } from './context/ProjectContext';
import { ThemeProvider } from './context/ThemeContext';
import { SetupWizard } from './components/setup/SetupWizard';
import { getApiKey } from './lib/claude';
import { refreshModelsIfStale } from './lib/models';
import { refreshPortkeyPresets } from './lib/portkey';

// Configure Monaco to use the local bundle instead of CDN.
// This is critical for Tauri because CSP blocks external scripts.
loader.config({ monaco });

function App() {
  const [setupComplete, setSetupComplete] = useState<boolean | null>(null);

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
  }, []);

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
        </ProjectProvider>
      )}
    </ThemeProvider>
  );
}

export default App;
