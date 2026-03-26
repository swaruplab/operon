import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { loader } from '@monaco-editor/react';
import * as monaco from 'monaco-editor';
import { AppShell } from './components/layout/AppShell';
import { ProjectProvider } from './context/ProjectContext';
import { SetupWizard } from './components/setup/SetupWizard';

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
  }, []);

  // Loading state — checking settings
  if (setupComplete === null) {
    return (
      <div className="h-screen w-screen bg-zinc-950 flex items-center justify-center">
        <div className="w-6 h-6 border-2 border-blue-500 border-t-transparent rounded-full animate-spin" />
      </div>
    );
  }

  // First-time setup
  if (!setupComplete) {
    return <SetupWizard onComplete={() => setSetupComplete(true)} />;
  }

  // Normal app
  return (
    <ProjectProvider>
      <AppShell />
    </ProjectProvider>
  );
}

export default App;
