import { createContext, useContext, useState, useCallback, useEffect, type ReactNode } from 'react';
import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';

export type BinaryFileType = 'image' | 'pdf' | 'html' | null;

export interface EditorTab {
  id: string;
  filePath: string;
  fileName: string;
  content: string;
  originalContent: string;
  isModified: boolean;
  isPreview: boolean;
  mode: 'edit' | 'diff';
  diffOriginal?: string;
  /** If set, this tab shows a binary file viewer instead of the code editor */
  binaryType: BinaryFileType;
  /** MIME type for binary content (e.g. 'image/png', 'application/pdf') */
  mimeType?: string;
  /** SSH profile ID — if set, this file is remote and needs write_remote_file to save */
  remoteProfileId?: string;
}

interface ProjectContextType {
  // Project path
  projectPath: string | null;
  setProjectPath: (path: string) => void;

  // Editor tabs
  tabs: EditorTab[];
  activeTabId: string | null;
  openFile: (filePath: string, content: string, preview?: boolean, remoteProfileId?: string) => void;
  openBinaryFile: (filePath: string, base64Content: string, mimeType: string, binaryType: BinaryFileType, preview?: boolean, remoteProfileId?: string) => void;
  closeTab: (tabId: string) => void;
  setActiveTabId: (tabId: string) => void;
  updateTabContent: (tabId: string, content: string) => void;
  saveTab: (tabId: string, newContent: string) => void;
  showDiff: (tabId: string, original: string) => void;
  closeDiff: (tabId: string) => void;
}

const ProjectContext = createContext<ProjectContextType | null>(null);

export function useProject() {
  const ctx = useContext(ProjectContext);
  if (!ctx) throw new Error('useProject must be used within ProjectProvider');
  return ctx;
}

export function ProjectProvider({ children }: { children: ReactNode }) {
  const [projectPath, setProjectPath] = useState<string | null>(null);
  const [tabs, setTabs] = useState<EditorTab[]>([]);
  const [activeTabId, setActiveTabId] = useState<string | null>(null);

  const openFile = useCallback(
    (filePath: string, content: string, preview = false, remoteProfileId?: string) => {
      setTabs((prev) => {
        // Check if already open
        const existing = prev.find((t) => t.filePath === filePath);
        if (existing) {
          setActiveTabId(existing.id);
          // Refresh content from disk if the tab isn't dirty (user-modified)
          // This ensures externally changed files (e.g. plan mode edits) are shown
          const shouldRefresh = !existing.isModified && content !== existing.content;
          const shouldPromote = !preview && existing.isPreview;
          if (shouldRefresh || shouldPromote) {
            return prev.map((t) =>
              t.id === existing.id
                ? {
                    ...t,
                    isPreview: shouldPromote ? false : t.isPreview,
                    ...(shouldRefresh
                      ? { content, originalContent: content, isModified: false }
                      : {}),
                  }
                : t,
            );
          }
          return prev;
        }

        const fileName = filePath.split('/').pop() || filePath;
        const newTab: EditorTab = {
          id: crypto.randomUUID(),
          filePath,
          fileName,
          content,
          originalContent: content,
          isModified: false,
          isPreview: preview,
          mode: 'edit',
          binaryType: null,
          remoteProfileId,
        };

        // Replace existing preview tab if this is also a preview
        if (preview) {
          const previewIdx = prev.findIndex((t) => t.isPreview);
          if (previewIdx !== -1) {
            const next = [...prev];
            next[previewIdx] = newTab;
            setActiveTabId(newTab.id);
            return next;
          }
        }

        setActiveTabId(newTab.id);
        return [...prev, newTab];
      });
    },
    [],
  );

  const openBinaryFile = useCallback(
    (filePath: string, base64Content: string, mimeType: string, binaryType: BinaryFileType, preview = false, remoteProfileId?: string) => {
      setTabs((prev) => {
        const existing = prev.find((t) => t.filePath === filePath);
        if (existing) {
          setActiveTabId(existing.id);
          const shouldRefresh = !existing.isModified && base64Content !== existing.content;
          const shouldPromote = !preview && existing.isPreview;
          if (shouldRefresh || shouldPromote) {
            return prev.map((t) =>
              t.id === existing.id
                ? {
                    ...t,
                    isPreview: shouldPromote ? false : t.isPreview,
                    ...(shouldRefresh
                      ? { content: base64Content, originalContent: base64Content, isModified: false }
                      : {}),
                  }
                : t,
            );
          }
          return prev;
        }

        const fileName = filePath.split('/').pop() || filePath;
        const newTab: EditorTab = {
          id: crypto.randomUUID(),
          filePath,
          fileName,
          content: base64Content,
          originalContent: base64Content,
          isModified: false,
          isPreview: preview,
          mode: 'edit',
          binaryType,
          mimeType,
          remoteProfileId,
        };

        if (preview) {
          const previewIdx = prev.findIndex((t) => t.isPreview);
          if (previewIdx !== -1) {
            const next = [...prev];
            next[previewIdx] = newTab;
            setActiveTabId(newTab.id);
            return next;
          }
        }

        setActiveTabId(newTab.id);
        return [...prev, newTab];
      });
    },
    [],
  );

  const closeTab = useCallback(
    (tabId: string) => {
      setTabs((prev) => {
        const idx = prev.findIndex((t) => t.id === tabId);
        const filtered = prev.filter((t) => t.id !== tabId);

        if (activeTabId === tabId && filtered.length > 0) {
          const newIdx = Math.min(idx, filtered.length - 1);
          setActiveTabId(filtered[newIdx].id);
        } else if (filtered.length === 0) {
          setActiveTabId(null);
        }

        return filtered;
      });
    },
    [activeTabId],
  );

  const updateTabContent = useCallback((tabId: string, content: string) => {
    setTabs((prev) =>
      prev.map((t) =>
        t.id === tabId
          ? { ...t, content, isModified: content !== t.originalContent, isPreview: false }
          : t,
      ),
    );
  }, []);

  const saveTab = useCallback((tabId: string, newContent: string) => {
    setTabs((prev) =>
      prev.map((t) =>
        t.id === tabId
          ? { ...t, content: newContent, originalContent: newContent, isModified: false }
          : t,
      ),
    );
  }, []);

  const showDiff = useCallback((tabId: string, original: string) => {
    setTabs((prev) =>
      prev.map((t) =>
        t.id === tabId ? { ...t, mode: 'diff' as const, diffOriginal: original } : t,
      ),
    );
  }, []);

  const closeDiff = useCallback((tabId: string) => {
    setTabs((prev) =>
      prev.map((t) =>
        t.id === tabId
          ? { ...t, mode: 'edit' as const, content: t.originalContent, isModified: false }
          : t,
      ),
    );
  }, []);

  // Listen for open-file events (e.g. from plan mode, file clicks)
  useEffect(() => {
    const unlisten = listen<{ path: string }>('open-file', async (event) => {
      const { path } = event.payload;
      try {
        const content = await invoke<string>('read_file', { path });
        openFile(path, content);
      } catch {
        // File might not exist yet or be binary — skip
      }
    });
    return () => { unlisten.then(u => u()); };
  }, [openFile]);

  return (
    <ProjectContext.Provider
      value={{
        projectPath,
        setProjectPath,
        tabs,
        activeTabId,
        openFile,
        openBinaryFile,
        closeTab,
        setActiveTabId,
        updateTabContent,
        saveTab,
        showDiff,
        closeDiff,
      }}
    >
      {children}
    </ProjectContext.Provider>
  );
}
