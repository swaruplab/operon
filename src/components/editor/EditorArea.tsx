import { useCallback, useEffect, useState } from 'react';
import { X, FileText, Code2, Image as ImageIcon, Globe, Pencil, Save, Check, Server, Sheet, Presentation, AlertTriangle } from 'lucide-react';
import { useProject } from '../../context/ProjectContext';
import { CodeEditor } from './CodeEditor';
import { DiffViewer } from './DiffViewer';
import { FileViewer } from './FileViewer';
import { writeFile } from '../../lib/files';
import { invoke } from '@tauri-apps/api/core';
import { modSymbol } from '../../lib/platform';

export function EditorArea() {
  const {
    tabs,
    activeTabId,
    setActiveTabId,
    closeTab,
    updateTabContent,
    saveTab,
    promoteTab,
    closeDiff,
  } = useProject();

  const [saveFlash, setSaveFlash] = useState<string | null>(null);
  const [saveError, setSaveError] = useState<{ tabId: string; message: string } | null>(null);

  const activeTab = tabs.find((t) => t.id === activeTabId);

  // A save error belongs to the tab it happened on; drop it when the user moves on.
  useEffect(() => {
    setSaveError(null);
  }, [activeTabId]);

  const handleSave = useCallback(
    async (tabId: string, content: string) => {
      const tab = tabs.find((t) => t.id === tabId);
      if (!tab) return;
      try {
        if (tab.remoteProfileId) {
          // Remote file — save via SSH
          await invoke('write_remote_file', {
            profileId: tab.remoteProfileId,
            path: tab.filePath,
            content,
          });
        } else {
          // Local file
          await writeFile(tab.filePath, content);
        }
        saveTab(tabId, content);
        setSaveError((prev) => (prev?.tabId === tabId ? null : prev));
        setSaveFlash(tabId);
        setTimeout(() => setSaveFlash(null), 1500);
      } catch (err) {
        // Tab stays dirty (saveTab not called); show the reason in the toolbar.
        console.error('Failed to save file:', err);
        const message = `${err}`;
        setSaveError({ tabId, message: /^save failed/i.test(message) ? message : `Save failed: ${message}` });
      }
    },
    [tabs, saveTab],
  );

  const getFileColor = (fileName: string) => {
    const ext = fileName.split('.').pop()?.toLowerCase();
    switch (ext) {
      case 'tsx':
      case 'ts':
        return 'text-blue-600 dark:text-blue-400';
      case 'rs':
        return 'text-orange-600 dark:text-orange-400';
      case 'json':
        return 'text-yellow-600 dark:text-yellow-400';
      case 'css':
        return 'text-purple-600 dark:text-purple-400';
      case 'py':
        return 'text-green-600 dark:text-green-400';
      case 'js':
      case 'jsx':
        return 'text-yellow-600 dark:text-yellow-400';
      case 'pdf':
        return 'text-red-600 dark:text-red-400';
      case 'xlsx':
      case 'xlsm':
      case 'xls':
        return 'text-green-600 dark:text-green-400';
      case 'pptx':
      case 'pptm':
      case 'ppt':
        return 'text-orange-600 dark:text-orange-400';
      case 'docx':
      case 'docm':
      case 'doc':
        return 'text-blue-600 dark:text-blue-400';
      case 'png':
      case 'jpg':
      case 'jpeg':
      case 'tiff':
      case 'tif':
      case 'gif':
      case 'bmp':
      case 'svg':
      case 'webp':
        return 'text-green-600 dark:text-green-400';
      default:
        return 'text-secondary';
    }
  };

  const getTabIcon = (tab: typeof activeTab) => {
    if (!tab) return <FileText className="w-3.5 h-3.5 text-secondary" />;
    if (tab.binaryType === 'image') return <ImageIcon className={`w-3.5 h-3.5 ${getFileColor(tab.fileName)}`} />;
    if (tab.binaryType === 'pdf') return <FileText className={`w-3.5 h-3.5 ${getFileColor(tab.fileName)}`} />;
    if (tab.binaryType === 'html') return <Globe className={`w-3.5 h-3.5 text-orange-600 dark:text-orange-400`} />;
    if (tab.binaryType === 'xlsx') return <Sheet className={`w-3.5 h-3.5 text-green-600 dark:text-green-400`} />;
    if (tab.binaryType === 'pptx') return <Presentation className={`w-3.5 h-3.5 text-orange-600 dark:text-orange-400`} />;
    if (tab.binaryType === 'docx') return <FileText className={`w-3.5 h-3.5 text-blue-600 dark:text-blue-400`} />;
    return <FileText className={`w-3.5 h-3.5 ${getFileColor(tab.fileName)}`} />;
  };

  return (
    <div className="flex flex-col h-full bg-canvas">
      {/* Tab Bar */}
      {tabs.length > 0 && (
        <div className="flex items-center h-[35px] bg-panel border-b border-border-default overflow-x-auto shrink-0">
          {tabs.map((tab) => (
            <button
              key={tab.id}
              onClick={() => setActiveTabId(tab.id)}
              className={`
                group flex items-center gap-1.5 px-3 h-full text-[13px] border-r border-border-default shrink-0 transition-colors
                ${tab.isPreview ? 'italic' : ''}
                ${
                  activeTabId === tab.id
                    ? 'bg-canvas text-primary border-t-2 border-t-blue-500'
                    : 'text-muted hover:text-secondary bg-panel border-t-2 border-t-transparent'
                }
              `}
            >
              {getTabIcon(tab)}
              <span>{tab.fileName}</span>
              {tab.isModified && (
                <span className="w-2 h-2 rounded-full bg-blue-500 ml-0.5" />
              )}
              <span
                onClick={(e) => {
                  e.stopPropagation();
                  closeTab(tab.id);
                }}
                className="ml-1 p-0.5 rounded hover:bg-elevated text-subtle hover:text-secondary opacity-0 group-hover:opacity-100 transition-opacity"
              >
                <X className="w-3 h-3" />
              </span>
            </button>
          ))}
        </div>
      )}

      {/* Editor Toolbar — shown for text files (not binary, not diff) */}
      {activeTab && !activeTab.binaryType && activeTab.mode !== 'diff' && (
        <div className="flex items-center justify-between px-3 py-1 bg-panel border-b border-border-default shrink-0">
          <div className="flex items-center gap-2 text-xs text-secondary">
            <FileText className={`w-3.5 h-3.5 ${getFileColor(activeTab.fileName)}`} />
            <span className="font-medium text-secondary">{activeTab.fileName}</span>
            {activeTab.remoteProfileId && (
              <span className="flex items-center gap-1 text-[10px] text-cyan-600 dark:text-cyan-400/70 bg-cyan-400/10 px-1.5 py-0.5 rounded">
                <Server className="w-2.5 h-2.5 pointer-events-none" />
                Remote
              </span>
            )}
            {activeTab.isPreview && (
              <span className="text-[10px] text-subtle">Read-only</span>
            )}
            {activeTab.isModified && (
              <span className="text-[10px] text-blue-600 dark:text-blue-400">Modified</span>
            )}
          </div>
          <div className="flex items-center gap-1.5 min-w-0">
            {saveError && saveError.tabId === activeTab.id && (
              <span
                className="flex items-center gap-1 min-w-0 max-w-[280px] px-2 py-0.5 rounded text-[10px] text-red-600 dark:text-red-400 bg-red-400/10"
                title={saveError.message}
              >
                <AlertTriangle className="w-3 h-3 shrink-0 pointer-events-none" />
                <span className="truncate">{saveError.message}</span>
                <button
                  onClick={() => setSaveError(null)}
                  className="p-0.5 rounded hover:bg-red-400/20 shrink-0"
                  title="Dismiss"
                >
                  <X className="w-3 h-3 pointer-events-none" />
                </button>
              </span>
            )}
            {activeTab.isPreview ? (
              <button
                onClick={() => promoteTab(activeTab.id)}
                className="flex items-center gap-1.5 px-2.5 py-1 rounded text-xs text-secondary hover:text-primary bg-surface hover:bg-elevated transition-colors"
                title="Switch to edit mode"
              >
                <Pencil className="w-3 h-3 pointer-events-none" />
                Edit
              </button>
            ) : (
              <button
                onClick={() => handleSave(activeTab.id, activeTab.content)}
                disabled={!activeTab.isModified && saveFlash !== activeTab.id}
                className={`flex items-center gap-1.5 px-2.5 py-1 rounded text-xs transition-colors ${
                  saveFlash === activeTab.id
                    ? 'text-green-600 dark:text-green-400 bg-green-400/10'
                    : activeTab.isModified
                      ? 'text-blue-600 dark:text-blue-400 hover:text-blue-800 dark:hover:text-blue-700 bg-blue-500/10 hover:bg-blue-500/20'
                      : 'text-subtle bg-surface cursor-default'
                }`}
                title={`Save (${modSymbol}S)`}
              >
                {saveFlash === activeTab.id ? (
                  <Check className="w-3 h-3 pointer-events-none" />
                ) : (
                  <Save className="w-3 h-3 pointer-events-none" />
                )}
                {saveFlash === activeTab.id ? 'Saved' : 'Save'}
              </button>
            )}
          </div>
        </div>
      )}

      {/* Editor Content */}
      <div className="flex-1 overflow-hidden">
        {activeTab ? (
          activeTab.binaryType ? (
            <FileViewer
              key={activeTab.id}
              filePath={activeTab.filePath}
              base64Content={activeTab.content}
              mimeType={activeTab.mimeType || 'application/octet-stream'}
              binaryType={activeTab.binaryType}
            />
          ) : activeTab.mode === 'diff' ? (
            <DiffViewer
              filePath={activeTab.filePath}
              original={activeTab.diffOriginal || activeTab.originalContent}
              modified={activeTab.content}
              onAccept={() => handleSave(activeTab.id, activeTab.content)}
              onReject={() => closeDiff(activeTab.id)}
            />
          ) : (
            <CodeEditor
              key={activeTab.id}
              filePath={activeTab.filePath}
              content={activeTab.content}
              readOnly={activeTab.isPreview}
              onChange={(content) => updateTabContent(activeTab.id, content)}
              onSave={(content) => handleSave(activeTab.id, content)}
            />
          )
        ) : (
          <div className="h-full flex flex-col items-center justify-center text-subtle">
            <Code2 className="w-16 h-16 mb-4 text-subtle" />
            <p className="text-sm mb-1">No file open</p>
            <p className="text-xs text-subtle">
              Open a file from the sidebar or press{' '}
              <kbd className="px-1.5 py-0.5 bg-surface rounded text-secondary text-[11px] font-mono">
                {modSymbol}P
              </kbd>
            </p>
          </div>
        )}
      </div>
    </div>
  );
}
