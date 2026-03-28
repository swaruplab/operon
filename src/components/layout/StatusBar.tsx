import { useEffect, useState } from 'react';
import { GitBranch, Loader, AlertCircle } from "lucide-react";
import { getActiveClient } from '../../lib/lspClient';

interface StatusBarProps {
  sidebarVisible: boolean;
  terminalVisible: boolean;
  chatVisible: boolean;
  activeLanguageId?: string;
}

export function StatusBar({ sidebarVisible, terminalVisible, chatVisible, activeLanguageId }: StatusBarProps) {
  const [lspStatus, setLspStatus] = useState<'idle' | 'starting' | 'running' | 'error'>('idle');
  const [lspServerName, setLspServerName] = useState<string>('');

  useEffect(() => {
    if (!activeLanguageId || activeLanguageId === 'plaintext') {
      setLspStatus('idle');
      setLspServerName('');
      return;
    }

    // Poll for LSP client status
    const interval = setInterval(() => {
      const client = getActiveClient(activeLanguageId);
      if (client && client.isRunning()) {
        setLspStatus('running');
        setLspServerName(activeLanguageId);
      } else {
        setLspStatus('idle');
        setLspServerName('');
      }
    }, 2000);

    return () => clearInterval(interval);
  }, [activeLanguageId]);

  const getLspIndicator = () => {
    if (lspStatus === 'running') {
      return (
        <div className="flex items-center gap-1.5 text-green-400">
          <span className="w-2 h-2 bg-green-400 rounded-full"></span>
          <span>{lspServerName} (LSP)</span>
        </div>
      );
    }
    if (lspStatus === 'starting') {
      return (
        <div className="flex items-center gap-1.5 text-yellow-400">
          <Loader className="w-3 h-3 animate-spin" />
          <span>Starting LSP...</span>
        </div>
      );
    }
    if (lspStatus === 'error') {
      return (
        <div className="flex items-center gap-1.5 text-red-400">
          <AlertCircle className="w-3 h-3" />
          <span>LSP Error</span>
        </div>
      );
    }
    return activeLanguageId ? <span>{activeLanguageId}</span> : null;
  };

  return (
    <div className="h-6 flex items-center justify-between px-3 bg-zinc-900 border-t border-zinc-800 text-[11px] text-zinc-500 shrink-0">
      {/* Left */}
      <div className="flex items-center gap-3">
        <div className="flex items-center gap-1">
          <GitBranch className="w-3 h-3" />
          <span>main</span>
        </div>
        <span>Ln 1, Col 1</span>
      </div>

      {/* Right */}
      <div className="flex items-center gap-3">
        <span>UTF-8</span>
        {getLspIndicator()}
        <div className="flex items-center gap-1.5">
          <span className={sidebarVisible ? "text-zinc-400" : ""}>Sidebar</span>
          <span className={terminalVisible ? "text-zinc-400" : ""}>Terminal</span>
          <span className={chatVisible ? "text-zinc-400" : ""}>Chat</span>
        </div>
      </div>
    </div>
  );
}
