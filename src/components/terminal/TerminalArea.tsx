import { useState, useCallback, useEffect, useRef } from 'react';
import { Plus, X, Terminal as TerminalIcon } from 'lucide-react';
import { listen, emit } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import { TerminalInstance } from './TerminalInstance';
import { Tooltip } from '../ui/Tooltip';

interface TerminalTab {
  id: string;
  title: string;
  type: 'local' | 'ssh';
  /** Command to run once the shell is ready (e.g. an SSH command) */
  initialCommand?: string;
  exited: boolean;
}

export function TerminalArea() {
  const [tabs, setTabs] = useState<TerminalTab[]>(() => {
    const id = crypto.randomUUID();
    return [{ id, title: 'Terminal', type: 'local', exited: false }];
  });
  const [activeTab, setActiveTab] = useState<string>(() => tabs[0].id);

  // Listen for SSH terminal open events from the sidebar
  useEffect(() => {
    const unlisten = listen<{ terminalId: string; title: string; sshCommand?: string }>('open-ssh-terminal', (event) => {
      const { terminalId, title, sshCommand } = event.payload;
      const newTab: TerminalTab = {
        id: terminalId,
        title,
        type: 'ssh',
        initialCommand: sshCommand,
        exited: false,
      };
      setTabs((prev) => [...prev, newTab]);
      setActiveTab(terminalId);
    });

    return () => { unlisten.then((u) => u()); };
  }, []);

  // Listen for login terminal open events (claude login)
  useEffect(() => {
    const unlisten = listen<{ terminalId: string; title: string; command: string }>('open-login-terminal', (event) => {
      const { terminalId, title, command } = event.payload;
      const newTab: TerminalTab = {
        id: terminalId,
        title,
        type: 'local',
        initialCommand: command,
        exited: false,
      };
      setTabs((prev) => [...prev, newTab]);
      setActiveTab(terminalId);
    });

    return () => { unlisten.then((u) => u()); };
  }, []);

  // Emit the active local terminal ID so the file explorer can use it for cd
  useEffect(() => {
    const activeTabObj = tabs.find((t) => t.id === activeTab);
    if (activeTabObj && activeTabObj.type === 'local' && !activeTabObj.exited) {
      emit('local-terminal-active', { terminalId: activeTabObj.id });
    }
  }, [activeTab, tabs]);

  // --- CWD polling for local terminals (terminal → sidebar sync) ---
  const lastCwd = useRef<string>('');
  useEffect(() => {
    const activeTabObj = tabs.find((t) => t.id === activeTab);
    if (!activeTabObj || activeTabObj.type !== 'local' || activeTabObj.exited) {
      return;
    }

    const pollCwd = async () => {
      try {
        const cwd = await invoke<string>('get_terminal_cwd', { terminalId: activeTabObj.id });
        if (cwd && cwd !== lastCwd.current) {
          lastCwd.current = cwd;
          emit('terminal-cwd-changed', { terminalId: activeTabObj.id, cwd });
        }
      } catch {
        // Terminal may have exited or CWD detection not supported — silently ignore
      }
    };

    // Poll every 1 second for responsive terminal → sidebar sync
    const interval = setInterval(pollCwd, 1000);
    // Also check immediately on tab switch
    pollCwd();

    return () => clearInterval(interval);
  }, [activeTab, tabs]);

  const createTab = useCallback((type: 'local' | 'ssh' = 'local') => {
    const id = crypto.randomUUID();
    const newTab: TerminalTab = {
      id,
      title: type === 'local' ? 'Terminal' : 'SSH',
      type,
      exited: false,
    };
    setTabs((prev) => [...prev, newTab]);
    setActiveTab(id);
  }, []);

  const closeTab = useCallback(
    (id: string) => {
      // Explicitly kill the backend terminal process when user closes the tab
      invoke('kill_terminal', { terminalId: id }).catch(console.error);
      setTabs((prev) => {
        const filtered = prev.filter((t) => t.id !== id);
        if (activeTab === id && filtered.length > 0) {
          setActiveTab(filtered[filtered.length - 1].id);
        }
        return filtered;
      });
    },
    [activeTab],
  );

  const handleTitleChange = useCallback((id: string, title: string) => {
    setTabs((prev) =>
      prev.map((t) => (t.id === id ? { ...t, title: title || 'Terminal' } : t)),
    );
  }, []);

  const handleExit = useCallback((id: string) => {
    setTabs((prev) =>
      prev.map((t) => (t.id === id ? { ...t, exited: true } : t)),
    );
  }, []);

  // If all tabs closed, show empty state
  if (tabs.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center h-full bg-[#09090b] text-zinc-500">
        <TerminalIcon className="w-8 h-8 mb-2 opacity-40" />
        <p className="text-xs">No terminals open</p>
        <button
          onClick={() => createTab()}
          className="mt-2 px-3 py-1 text-xs rounded bg-zinc-800 hover:bg-zinc-700 text-zinc-300 transition-colors"
        >
          New Terminal
        </button>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full bg-[#09090b]">
      {/* Tab bar */}
      <div className="flex items-center h-[33px] bg-zinc-900 border-b border-zinc-800 shrink-0">

        <div className="flex items-center gap-0.5 px-1 flex-1 overflow-x-auto">
          {tabs.map((tab) => (
            <button
              key={tab.id}
              onClick={() => setActiveTab(tab.id)}
              className={`
                group flex items-center gap-1.5 px-2.5 py-1 rounded text-xs transition-colors
                ${
                  activeTab === tab.id
                    ? 'bg-zinc-800 text-zinc-200'
                    : 'text-zinc-500 hover:text-zinc-300 hover:bg-zinc-800/50'
                }
              `}
            >
              <TerminalIcon className="w-3 h-3" />
              <span className={tab.exited ? 'line-through opacity-50' : ''}>
                {tab.title}
              </span>
              <span
                onClick={(e) => {
                  e.stopPropagation();
                  closeTab(tab.id);
                }}
                className="p-0.5 rounded hover:bg-zinc-700 opacity-0 group-hover:opacity-100"
              >
                <X className="w-2.5 h-2.5" />
              </span>
            </button>
          ))}
        </div>

        <div className="flex items-center gap-0.5 mr-1">
          <Tooltip label="New terminal" position="top">
            <button
              onClick={() => createTab()}
              className="p-1.5 rounded hover:bg-zinc-800 text-zinc-500 hover:text-zinc-300 transition-colors"
            >
              <Plus className="w-3.5 h-3.5" />
            </button>
          </Tooltip>
        </div>
      </div>

      {/* Terminal instances — all rendered, only active visible */}
      <div className="flex-1 relative">
        {tabs.map((tab) => (
          <div
            key={tab.id}
            className="absolute inset-0"
            style={{ zIndex: activeTab === tab.id ? 1 : 0 }}
          >
            <TerminalInstance
              terminalId={tab.id}
              isVisible={activeTab === tab.id}
              initialCommand={tab.initialCommand}
              onTitleChange={(title) => handleTitleChange(tab.id, title)}
              onExit={() => handleExit(tab.id)}
            />
          </div>
        ))}
      </div>
    </div>
  );
}
