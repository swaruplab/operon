import { useEffect, useState } from 'react';
import { GitBranch, Loader, AlertCircle } from "lucide-react";
import { Tooltip } from "../ui/Tooltip";
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
        <Tooltip label="Language server is running" position="top">
          <div className="flex items-center gap-1.5 text-green-400">
            <span className="w-2 h-2 bg-green-400 rounded-full"></span>
            <span>{lspServerName} (LSP)</span>
          </div>
        </Tooltip>
      );
    }
    if (lspStatus === 'starting') {
      return (
        <Tooltip label="Language server is starting up" position="top">
          <div className="flex items-center gap-1.5 text-yellow-400">
            <Loader className="w-3 h-3 animate-spin" />
            <span>Starting LSP...</span>
          </div>
        </Tooltip>
      );
    }
    if (lspStatus === 'error') {
      return (
        <Tooltip label="Language server encountered an error" position="top">
          <div className="flex items-center gap-1.5 text-red-400">
            <AlertCircle className="w-3 h-3" />
            <span>LSP Error</span>
          </div>
        </Tooltip>
      );
    }
    return activeLanguageId ? (
      <Tooltip label="Detected file language" position="top">
        <span>{activeLanguageId}</span>
      </Tooltip>
    ) : null;
  };

  return (
    <div className="h-6 flex items-center justify-between px-3 bg-zinc-900 border-t border-zinc-800 text-[11px] text-zinc-500 shrink-0">
      {/* Left */}
      <div className="flex items-center gap-3">
        <Tooltip label="Current Git branch" position="top">
          <div className="flex items-center gap-1">
            <GitBranch className="w-3 h-3" />
            <span>main</span>
          </div>
        </Tooltip>
        <Tooltip label="Cursor position in active editor" position="top">
          <span>Ln 1, Col 1</span>
        </Tooltip>
      </div>

      {/* Right */}
      <div className="flex items-center gap-3">
        <Tooltip label="File encoding" position="top">
          <span>UTF-8</span>
        </Tooltip>
        {getLspIndicator()}
        <div className="flex items-center gap-1.5">
          <Tooltip label="Toggle sidebar" shortcut={"\u2318B"} position="top">
            <span className={`cursor-default ${sidebarVisible ? "text-zinc-400" : ""}`}>Sidebar</span>
          </Tooltip>
          <Tooltip label="Toggle terminal" shortcut={"\u2318J"} position="top">
            <span className={`cursor-default ${terminalVisible ? "text-zinc-400" : ""}`}>Terminal</span>
          </Tooltip>
          <Tooltip label="Toggle chat panel" shortcut={"\u2318L"} position="top">
            <span className={`cursor-default ${chatVisible ? "text-zinc-400" : ""}`}>Chat</span>
          </Tooltip>
        </div>
      </div>
    </div>
  );
}
