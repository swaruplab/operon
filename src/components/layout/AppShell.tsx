import { useState, useCallback, useEffect, useRef } from 'react';
import { Panel, PanelGroup, PanelResizeHandle, type ImperativePanelHandle } from 'react-resizable-panels';
import { listen } from '@tauri-apps/api/event';
import { TopBar } from './TopBar';
import { ActivityBar } from './ActivityBar';
import { StatusBar } from './StatusBar';
import { Sidebar, type SSHConnection } from '../sidebar/Sidebar';
import { EditorArea } from '../editor/EditorArea';
import { TerminalArea, type PendingLoginTab } from '../terminal/TerminalArea';
import { ChatPanel } from '../chat/ChatPanel';
import { CommandPalette } from './CommandPalette';
import { SettingsPanel } from '../settings/SettingsPanel';
import { HelpPanel } from '../help/HelpPanel';
import { ReviewModal } from '../review/ReviewModal';
import { useKeyboardShortcuts } from '../../hooks/useKeyboardShortcuts';
import { loadAllExtensionContributions } from '../../lib/extensionLoader';
import { useProject } from '../../context/ProjectContext';
import { reviewCode, type ReviewResult } from '../../lib/review';

export function AppShell() {
  const [sidebarVisible, setSidebarVisible] = useState(true);
  const [chatVisible, setChatVisible] = useState(true);
  const [terminalVisible, setTerminalVisible] = useState(true);
  // Drives the terminal panel imperatively so it can collapse to zero height
  // while staying mounted — see the Panel below for why unmounting is unsafe.
  const terminalPanelRef = useRef<ImperativePanelHandle>(null);

  // Keep the panel's collapsed state in sync with the toggle. Collapsing rather
  // than unmounting is what keeps live PTYs reachable: TerminalInstance's cleanup
  // intentionally does NOT kill the backend process, so an unmount used to strand
  // every running shell and SSH session with no UI left to reach it.
  useEffect(() => {
    const panel = terminalPanelRef.current;
    if (!panel) return;
    if (terminalVisible) {
      if (panel.isCollapsed()) panel.expand();
    } else if (!panel.isCollapsed()) {
      panel.collapse();
    }
  }, [terminalVisible]);
  // Login-terminal requests are received HERE and handed down as a prop rather
  // than listened for inside TerminalArea. TerminalArea no longer unmounts (the
  // panel collapses instead), but the prop hand-off is still the right shape: it
  // lets AppShell reveal the panel first, and it delivers even if TerminalArea
  // mounts after the request was made.
  const [pendingLoginTab, setPendingLoginTab] = useState<PendingLoginTab | null>(null);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [settingsInitialSection, setSettingsInitialSection] = useState<string | undefined>(undefined);
  const [helpOpen, setHelpOpen] = useState(false);
  const [activeView, setActiveView] = useState<string>('files');
  const [wakeToastVisible, setWakeToastVisible] = useState(false);
  // Remote-session state lives here (AppShell never unmounts) so it survives the
  // Files panel being toggled closed/open — fixes issue #16, where reopening the
  // panel dropped the SSH connection and forced the user to reconnect.
  const [sshConnection, setSSHConnection] = useState<SSHConnection | null>(null);
  const [currentRemotePath, setCurrentRemotePath] = useState<string>('');
  const [localTerminalId, setLocalTerminalId] = useState<string | null>(null);

  const handlePendingLoginConsumed = useCallback(() => setPendingLoginTab(null), []);

  useEffect(() => {
    const unlisten = listen<PendingLoginTab>('open-login-terminal', (event) => {
      // Reveal the panel first — the request is meaningless if the user can't
      // see the terminal it targets.
      setTerminalVisible(true);
      setPendingLoginTab(event.payload);
    });
    return () => { unlisten.then((u) => u()); };
  }, []);

  const toggleSidebar = useCallback(() => setSidebarVisible((v) => !v), []);
  const toggleChat = useCallback(() => setChatVisible((v) => !v), []);
  const toggleTerminal = useCallback(() => setTerminalVisible((v) => !v), []);
  const openPalette = useCallback(() => setPaletteOpen(true), []);
  const closePalette = useCallback(() => setPaletteOpen(false), []);

  // ── Light code reviewer ────────────────────────────────────────────────
  // Reviews the file in the active editor tab on a different (cheaper) model
  // with a fresh context. Advisory: it never edits your code, and a failed
  // review is surfaced as "unavailable" rather than as a problem with the file.
  const { tabs, activeTabId } = useProject();
  const [reviewOpen, setReviewOpen] = useState(false);
  const [reviewResult, setReviewResult] = useState<ReviewResult | null>(null);
  const [reviewLoading, setReviewLoading] = useState(false);
  const [reviewError, setReviewError] = useState<string | null>(null);
  const [reviewTarget, setReviewTarget] = useState('');

  const reviewCurrentFile = useCallback(async () => {
    const tab = tabs.find((t) => t.id === activeTabId);
    setReviewOpen(true);
    setReviewResult(null);
    setReviewError(null);

    if (!tab) {
      setReviewError('No file is open. Open a script in the editor first.');
      return;
    }
    if (tab.binaryType) {
      setReviewError(`${tab.fileName} is a binary file — nothing to review.`);
      return;
    }
    if (!tab.content.trim()) {
      setReviewError(`${tab.fileName} is empty.`);
      return;
    }

    setReviewTarget(tab.fileName);
    setReviewLoading(true);
    try {
      // sbatch/slurm scripts get the HPC checklist; everything else the
      // single-cell / genomics methods checklist.
      const mode = /\.(sbatch|slurm)$/i.test(tab.fileName) ? 'sbatch' : 'script';
      setReviewResult(await reviewCode(tab.content, mode, tab.fileName));
    } catch (e) {
      setReviewError(String(e));
    } finally {
      setReviewLoading(false);
    }
  }, [tabs, activeTabId]);

  const commands = [
    { id: 'toggle-sidebar', label: 'Toggle Sidebar', shortcut: '\u2318B', action: toggleSidebar },
    { id: 'toggle-terminal', label: 'Toggle Terminal', shortcut: '\u2318J', action: toggleTerminal },
    { id: 'toggle-chat', label: 'Toggle Chat Panel', shortcut: '\u2318L', action: toggleChat },
    { id: 'command-palette', label: 'Command Palette', shortcut: '\u2318\u21E7P', action: openPalette },
    { id: 'open-settings', label: 'Open Settings', shortcut: '\u2318,', action: () => setSettingsOpen(true) },
    {
      id: 'review-file',
      label: 'Review: Check current file for analysis pitfalls',
      action: reviewCurrentFile,
    },
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

  // Wake-from-sleep toast: backend has already torn down stale SSH channels
  // and the next remote op will rebuild — this is purely a heads-up.
  useEffect(() => {
    let timer: ReturnType<typeof setTimeout> | undefined;
    const unlisten = listen('ssh-wake-reconnect', () => {
      setWakeToastVisible(true);
      if (timer) clearTimeout(timer);
      timer = setTimeout(() => setWakeToastVisible(false), 3000);
    });
    return () => {
      if (timer) clearTimeout(timer);
      unlisten.then((u) => u());
    };
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

  // --- Remote-session listeners (issue #16) ---
  // These were in Sidebar, which unmounts when the Files panel is hidden — so the
  // connection was lost on reopen. AppShell stays mounted for the app's lifetime.
  useEffect(() => {
    const unlisten = listen<{ profileId: string; remotePath: string }>(
      'remote-path-changed',
      (e) => setCurrentRemotePath(e.payload.remotePath)
    );
    return () => { unlisten.then((u) => u()); };
  }, []);

  useEffect(() => {
    const unlisten = listen<{ terminalId: string }>(
      'local-terminal-active',
      (e) => setLocalTerminalId(e.payload.terminalId)
    );
    return () => { unlisten.then((u) => u()); };
  }, []);

  useEffect(() => {
    const unlisten = listen<{ terminalId: string; profileId?: string; profileName?: string }>(
      'open-ssh-terminal',
      (e) => {
        const { profileId, profileName, terminalId } = e.payload;
        if (profileId && profileName) {
          setSSHConnection({ profileId, profileName, terminalId });
          // Surface the remote explorer: open the sidebar on the files view.
          setActiveView('files');
          setSidebarVisible(true);
          // And reveal the terminal, which is where the SSH session actually is.
          // The panel now stays mounted while hidden, so the tab would otherwise
          // be created inside a zero-height panel the user cannot see.
          setTerminalVisible(true);
        }
      }
    );
    return () => { unlisten.then((u) => u()); };
  }, []);

  useEffect(() => {
    const unlisten = listen<{ profileId: string }>('disconnect-remote', (e) => {
      const { profileId } = e.payload;
      setSSHConnection((prev) => (prev?.profileId === profileId ? null : prev));
      setCurrentRemotePath((prev) => (prev ? '' : prev));
    });
    return () => { unlisten.then((u) => u()); };
  }, []);

  useKeyboardShortcuts([
    { key: 'b', meta: true, handler: toggleSidebar },
    { key: 'j', meta: true, handler: toggleTerminal },
    { key: 'l', meta: true, handler: toggleChat },
    { key: 'p', meta: true, shift: true, handler: openPalette },
    { key: ',', meta: true, handler: () => setSettingsOpen(true) },
  ]);


  return (
    <div className="h-screen flex flex-col bg-canvas text-primary select-none">
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
                <Sidebar
                  activeView={activeView}
                  onViewChange={setActiveView}
                  sshConnection={sshConnection}
                  currentRemotePath={currentRemotePath}
                  localTerminalId={localTerminalId}
                />
              </Panel>
              <PanelResizeHandle className="w-px bg-zinc-300 dark:bg-zinc-800 hover:bg-blue-500 active:bg-blue-500 transition-colors duration-150 data-[resize-handle-state=hover]:bg-blue-500 data-[resize-handle-state=hover]:w-[3px]" />
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
              {/* The terminal panel COLLAPSES; it must never unmount. TerminalArea
                  owns the tab list, and TerminalInstance deliberately leaves the
                  backend PTY running on cleanup — so unmounting it threw away the
                  only handle to shells and SSH sessions that were still alive,
                  leaving them running invisibly until app exit. */}
              <PanelResizeHandle
                className={`h-px bg-zinc-300 dark:bg-zinc-800 hover:bg-blue-500 active:bg-blue-500 transition-colors duration-150 data-[resize-handle-state=hover]:bg-blue-500 data-[resize-handle-state=hover]:h-[3px] ${
                  terminalVisible ? '' : 'hidden'
                }`}
              />
              <Panel
                ref={terminalPanelRef}
                id="terminal"
                order={2}
                defaultSize={terminalVisible ? 35 : 0}
                minSize={10}
                collapsible
                collapsedSize={0}
                // The PanelGroup persists its layout (autoSaveId="center-v"), so the
                // panel can come back collapsed from a previous run while
                // `terminalVisible` defaults to true. Mirroring the panel's real
                // state back into it keeps the toggle honest — otherwise Cmd+J
                // would appear to do nothing the first time it was pressed.
                onCollapse={() => setTerminalVisible(false)}
                onExpand={() => setTerminalVisible(true)}
              >
                <TerminalArea
                  pendingLoginTab={pendingLoginTab}
                  onPendingLoginConsumed={handlePendingLoginConsumed}
                />
              </Panel>
            </PanelGroup>
          </Panel>

          {/* Right Chat Panel */}
          {chatVisible && (
            <>
              <PanelResizeHandle className="w-px bg-zinc-300 dark:bg-zinc-800 hover:bg-blue-500 active:bg-blue-500 transition-colors duration-150 data-[resize-handle-state=hover]:bg-blue-500 data-[resize-handle-state=hover]:w-[3px]" />
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

      {/* Light code reviewer */}
      <ReviewModal
        isOpen={reviewOpen}
        onClose={() => setReviewOpen(false)}
        result={reviewResult}
        loading={reviewLoading}
        error={reviewError}
        target={reviewTarget}
        onRerun={reviewCurrentFile}
      />

      {/* Wake-from-sleep reconnect toast */}
      {wakeToastVisible && (
        <div className="fixed bottom-10 right-4 z-50 px-3 py-2 text-xs rounded border border-amber-500/40 bg-zinc-900/95 text-amber-300 shadow-lg pointer-events-none">
          Reconnecting after sleep&hellip;
        </div>
      )}
    </div>
  );
}
