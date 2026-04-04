import { useCallback } from 'react';
import { X, FileText, Code2, Image as ImageIcon, Globe } from 'lucide-react';
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
    closeDiff,
  } = useProject();

  const activeTab = tabs.find((t) => t.id === activeTabId);

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
      } catch (err) {
        console.error('Failed to save file:', err);
      }
    },
    [tabs, saveTab],
  );

  const getFileColor = (fileName: string) => {
    const ext = fileName.split('.').pop()?.toLowerCase();
    switch (ext) {
      case 'tsx':
      case 'ts':
        return 'text-blue-400';
      case 'rs':
        return 'text-orange-400';
      case 'json':
        return 'text-yellow-400';
      case 'css':
        return 'text-purple-400';
      case 'py':
        return 'text-green-400';
      case 'js':
      case 'jsx':
        return 'text-yellow-400';
      case 'pdf':
        return 'text-red-400';
      case 'png':
      case 'jpg':
      case 'jpeg':
      case 'tiff':
      case 'tif':
      case 'gif':
      case 'bmp':
      case 'svg':
      case 'webp':
        return 'text-green-400';
      default:
        return 'text-zinc-400';
    }
  };

  const getTabIcon = (tab: typeof activeTab) => {
    if (!tab) return <FileText className="w-3.5 h-3.5 text-zinc-400" />;
    if (tab.binaryType === 'image') return <ImageIcon className={`w-3.5 h-3.5 ${getFileColor(tab.fileName)}`} />;
    if (tab.binaryType === 'pdf') return <FileText className={`w-3.5 h-3.5 ${getFileColor(tab.fileName)}`} />;
    if (tab.binaryType === 'html') return <Globe className={`w-3.5 h-3.5 text-orange-400`} />;
    return <FileText className={`w-3.5 h-3.5 ${getFileColor(tab.fileName)}`} />;
  };

  return (
    <div className="flex flex-col h-full bg-zinc-950">
      {/* Tab Bar */}
      {tabs.length > 0 && (
        <div className="flex items-center h-[35px] bg-zinc-900 border-b border-zinc-800 overflow-x-auto shrink-0">
          {tabs.map((tab) => (
            <button
              key={tab.id}
              onClick={() => setActiveTabId(tab.id)}
              className={`
                group flex items-center gap-1.5 px-3 h-full text-[13px] border-r border-zinc-800 shrink-0 transition-colors
                ${tab.isPreview ? 'italic' : ''}
                ${
                  activeTabId === tab.id
                    ? 'bg-zinc-950 text-zinc-100 border-t-2 border-t-blue-500'
                    : 'text-zinc-500 hover:text-zinc-300 bg-zinc-900 border-t-2 border-t-transparent'
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
                className="ml-1 p-0.5 rounded hover:bg-zinc-700 text-zinc-600 hover:text-zinc-300 opacity-0 group-hover:opacity-100 transition-opacity"
              >
                <X className="w-3 h-3" />
              </span>
            </button>
          ))}
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
          <div className="h-full flex flex-col items-center justify-center text-zinc-600">
            <Code2 className="w-16 h-16 mb-4 text-zinc-800" />
            <p className="text-sm mb-1">No file open</p>
            <p className="text-xs text-zinc-700">
              Open a file from the sidebar or press{' '}
              <kbd className="px-1.5 py-0.5 bg-zinc-800 rounded text-zinc-400 text-[11px] font-mono">
                {modSymbol}P
              </kbd>
            </p>
          </div>
        )}
      </div>
    </div>
  );
}
