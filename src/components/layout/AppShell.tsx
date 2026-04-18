import { useState, useCallback, useEffect } from 'react';
import { Panel, PanelGroup, PanelResizeHandle } from 'react-resizable-panels';
import { listen } from '@tauri-apps/api/event';
import { TopBar } from './TopBar';
import { ActivityBar } from './ActivityBar';
import { StatusBar } from './StatusBar';
import { Sidebar } from '../sidebar/Sidebar';
import { EditorArea } from '../editor/EditorArea';
import { TerminalArea } from '../terminal/TerminalArea';
import { ChatPanel } from '../chat/ChatPanel';
import { CommandPalette } from './CommandPalette';
import { SettingsPanel } from '../settings/SettingsPanel';
import { HelpPanel } from '../help/HelpPanel';
import { useKeyboardShortcuts } from '../../hooks/useKeyboardShortcuts';
import { loadAllExtensionContributions } from '../../lib/extensionLoader';

export function AppShell() {
  const [sidebarVisible, setSidebarVisible] = useState(true);
  const [chatVisible, setChatVisible] = useState(true);
  const [terminalVisible, setTerminalVisible] = useState(true);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [settingsInitialSection, setSettingsInitialSection] = useState<string | undefined>(undefined);
  const [helpOpen, setHelpOpen] = useState(false);
  const [activeView, setActiveView] = useState<string>('files');

  const toggleSidebar = useCallback(() => setSidebarVisible((v) => !v), []);
  const toggleChat = useCallback(() => setChatVisible((v) => !v), []);
  const toggleTerminal = useCallback(() => setTerminalVisible((v) => !v), []);
  const openPalette = useCallback(() => setPaletteOpen(true), []);
  const closePalette = useCallback(() => setPaletteOpen(false), []);

  const commands = [
    { id: 'toggle-sidebar', label: 'Toggle Sidebar', shortcut: '\u2318B', action: toggleSidebar },
    { id: 'toggle-terminal', label: 'Toggle Terminal', shortcut: '\u2318J', action: toggleTerminal },
    { id: 'toggle-chat', label: 'Toggle Chat Panel', shortcut: '\u2318L', action: toggleChat },
    { id: 'command-palette', label: 'Command Palette', shortcut: '\u2318\u21E7P', action: openPalette },
    { id: 'open-settings', label: 'Open Settings', shortcut: '\u2318,', action: () => setSettingsOpen(true) },
    {
      id: 'view-files',
      label: 'Explorer: Focus on File View',
      action: () => {
        setActiveView('files');
        setSidebarVisible(true);
      },
    },
    {
      id: 'view-search',
      label: 'Search: Focus on Search View',
      action: () => {
        setActiveView('search');
        setSidebarVisible(true);
      },
    },
    {
      id: 'view-ssh',
      label: 'Remote: Focus on SSH Connections',
      action: () => {
        setActiveView('ssh');
        setSidebarVisible(true);
      },
    },
  ];

  // Set window title with version
  useEffect(() => {
    const suffix = __APP_VERSION__ === 'dev' ? ' [DEV]' : ` v${__APP_VERSION__}`;
    document.title = `Operon${suffix}`;
  }, []);

  // Load extension contributions (themes, snippets) on startup
  useEffect(() => {
    loadAllExtensionContributions();
  }, []);

  // Listen for native macOS Help menu click
  useEffect(() => {
    const unlisten = listen('open-help-panel', () => {
      setHelpOpen(true);
    });
    return () => { unlisten.then((u) => u()); };
  }, []);

  // Window-level event from ChatPanel model picker → open Settings to a specific section
  useEffect(() => {
    const handler = (e: Event) => {
      const detail = (e as CustomEvent).detail;
      setSettingsInitialSection(detail?.section);
      setSettingsOpen(true);
    };
    window.addEventListener('open-settings', handler);
    return () => window.removeEventListener('open-settings', handler);
  }, []);

  useKeyboardShortcuts([
    { key: 'b', meta: true, handler: toggleSidebar },
    { key: 'j', meta: true, handler: toggleTerminal },
    { key: 'l', meta: true, handler: toggleChat },
    { key: 'p', meta: true, shift: true, handler: openPalette },
    { key: ',', meta: true, handler: () => setSettingsOpen(true) },
  ]);


  return (
    <div className="h-screen flex flex-col bg-zinc-950 text-zinc-50 select-none">
      {/* Top Bar */}
      <TopBar onToggleSidebar={toggleSidebar} onToggleChat={toggleChat} onOpenSettings={() => setSettingsOpen(true)} onOpenHelp={() => setHelpOpen(true)} />

      {/* Main Content */}
      <div className="flex-1 flex overflow-hidden">
        {/* Activity Bar */}
        <ActivityBar
          activeView={activeView}
          onViewChange={(view) => {
            if (view === 'settings') {
              setSettingsOpen(true);
              return;
            }
            if (view === 'help') {
              setHelpOpen(true);
              return;
            }
            if (activeView === view && sidebarVisible) {
              setSidebarVisible(false);
            } else {
              setActiveView(view);
              setSidebarVisible(true);
            }
          }}
        />

        {/* Horizontal split: Sidebar | Center | Chat */}
        <PanelGroup direction="horizontal" autoSaveId="main-h">
          {/* Left Sidebar */}
          {sidebarVisible && (
            <>
              <Panel id="sidebar" defaultSize={20} minSize={15} maxSize={35} order={1}>
                <Sidebar activeView={activeView} onViewChange={setActiveView} />
              </Panel>
              <PanelResizeHandle className="w-[3px] bg-zinc-900 hover:bg-blue-500 active:bg-blue-500 transition-colors duration-150" />
            </>
          )}

          {/* Center: Editor + Terminal (vertical split) */}
          <Panel
            id="center"
            defaultSize={sidebarVisible && chatVisible ? 55 : 70}
            minSize={30}
            order={2}
          >
            <PanelGroup direction="vertical" autoSaveId="center-v">
              <Panel id="editor" defaultSize={terminalVisible ? 65 : 100} minSize={20} order={1}>
                <EditorArea />
              </Panel>
              {terminalVisible && (
                <>
                  <PanelResizeHandle className="h-[3px] bg-zinc-900 hover:bg-blue-500 active:bg-blue-500 transition-colors duration-150" />
                  <Panel id="terminal" defaultSize={35} minSize={10} order={2}>
                    <TerminalArea />
                  </Panel>
                </>
              )}
            </PanelGroup>
          </Panel>

          {/* Right Chat Panel */}
          {chatVisible && (
            <>
              <PanelResizeHandle className="w-[3px] bg-zinc-900 hover:bg-blue-500 active:bg-blue-500 transition-colors duration-150" />
              <Panel id="chat" defaultSize={30} minSize={20} maxSize={50} order={3}>
                <ChatPanel />
              </Panel>
            </>
          )}
        </PanelGroup>
      </div>

      {/* Status Bar */}
      <StatusBar
        sidebarVisible={sidebarVisible}
        terminalVisible={terminalVisible}
        chatVisible={chatVisible}
      />

      {/* Command Palette */}
      <CommandPalette isOpen={paletteOpen} onClose={closePalette} commands={commands} />

      {/* Settings Panel */}
      <SettingsPanel
        isOpen={settingsOpen}
        onClose={() => { setSettingsOpen(false); setSettingsInitialSection(undefined); }}
        initialSection={settingsInitialSection}
      />

      {/* Help Panel */}
      <HelpPanel
        isOpen={helpOpen}
        onClose={() => setHelpOpen(false)}
        onNavigate={(view) => {
          setHelpOpen(false);
          if (view === 'settings') {
            setSettingsOpen(true);
          } else {
            setActiveView(view);
            setSidebarVisible(true);
          }
        }}
      />
    </div>
  );
}
