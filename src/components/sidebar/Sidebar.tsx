import { useState, useEffect, useCallback, useRef } from 'react';
import {
  ChevronRight,
  ChevronDown,
  File,
  Folder,
  FolderOpen,
  RefreshCw,
  FolderInput,
  Monitor,
  Server,
  CornerDownRight,
  Pin,
  PinOff,
  Star,
  FolderPlus,
  Trash2,
  Copy,
  Pencil,
  MessageSquarePlus,
  Filter,
} from 'lucide-react';
import { SSHView } from './SSHView';
import { RemoteExplorer } from './RemoteExplorer';
import { ProtocolsView } from './ProtocolsView';
import { GitPanel } from './GitPanel';
import { ExtensionsView } from './ExtensionsView';
import { dockerExtension } from './DockerPanel';
import { singularityExtension } from './SingularityPanel';
import { invoke } from '@tauri-apps/api/core';
import { listen, emit } from '@tauri-apps/api/event';
import { useProject } from '../../context/ProjectContext';
import type { FileEntry } from '../../lib/files';

const BINARY_EXTENSIONS: Record<string, { binaryType: 'image' | 'pdf' | 'html'; mimeType: string }> = {
  png: { binaryType: 'image', mimeType: 'image/png' },
  jpg: { binaryType: 'image', mimeType: 'image/jpeg' },
  jpeg: { binaryType: 'image', mimeType: 'image/jpeg' },
  gif: { binaryType: 'image', mimeType: 'image/gif' },
  bmp: { binaryType: 'image', mimeType: 'image/bmp' },
  webp: { binaryType: 'image', mimeType: 'image/webp' },
  tiff: { binaryType: 'image', mimeType: 'image/tiff' },
  tif: { binaryType: 'image', mimeType: 'image/tiff' },
  svg: { binaryType: 'image', mimeType: 'image/svg+xml' },
  pdf: { binaryType: 'pdf', mimeType: 'application/pdf' },
  html: { binaryType: 'html', mimeType: 'text/html' },
  htm: { binaryType: 'html', mimeType: 'text/html' },
};

interface SidebarProps {
  activeView: string;
  onViewChange?: (view: string) => void;
}

// --- File Tree Node ---

interface TreeNodeProps {
  entry: FileEntry;
  depth: number;
  onNavigateDir?: (path: string) => void;
  isPinned?: boolean;
  onTogglePin?: (path: string, name: string, isDir: boolean) => void;
  onContextMenu?: (e: React.MouseEvent, entry: FileEntry) => void;
}

function TreeNode({ entry, depth, onNavigateDir, isPinned, onTogglePin, onContextMenu }: TreeNodeProps) {
  const [hovered, setHovered] = useState(false);
  const [expanded, setExpanded] = useState(false);
  const [children, setChildren] = useState<FileEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const { openFile, openBinaryFile } = useProject();

  const toggle = async () => {
    if (!entry.is_dir) return;
    if (expanded) {
      // Collapsing: clear children so next expand fetches fresh data
      setExpanded(false);
      setChildren([]);
      return;
    }
    // Expanding: always fetch fresh directory listing
    setLoading(true);
    try {
      const entries = await invoke<FileEntry[]>('list_directory', {
        path: entry.path,
      });
      setChildren(entries);
    } catch (err) {
      console.error('Failed to list directory:', err);
    }
    setLoading(false);
    setExpanded(true);
  };

  const openLocalFile = async (preview: boolean) => {
    // Guard: refuse to open files larger than 15 MB to avoid UI hangs
    if (entry.size > MAX_FILE_SIZE) {
      openFile(
        entry.path,
        `⚠ File too large to display\n\nThis file is ${formatSize(entry.size)}, which exceeds the 15 MB limit.\nOpening it in the editor could freeze the application.\n\nPath: ${entry.path}`,
        preview,
      );
      return;
    }

    try {
      const ext = entry.extension?.toLowerCase() || '';
      const binaryInfo = BINARY_EXTENSIONS[ext];

      if (binaryInfo) {
        const base64Content = await invoke<string>('read_file_base64', { path: entry.path });
        openBinaryFile(entry.path, base64Content, binaryInfo.mimeType, binaryInfo.binaryType, preview);
      } else {
        const content = await invoke<string>('read_file', { path: entry.path });
        openFile(entry.path, content, preview);
      }
    } catch (err) {
      console.error('Failed to read file:', err);
    }
  };

  const handleClick = () => {
    if (entry.is_dir) {
      toggle();
    } else {
      openLocalFile(true); // single click = preview
    }
  };

  const handleDoubleClick = () => {
    if (entry.is_dir) {
      onNavigateDir?.(entry.path); // double click on dir = navigate into it
    } else {
      openLocalFile(false); // double click on file = open permanently
    }
  };

  const getFileColor = (ext: string | null) => {
    const colorMap: Record<string, string> = {
      tsx: 'text-blue-400',
      ts: 'text-blue-400',
      jsx: 'text-yellow-400',
      js: 'text-yellow-400',
      rs: 'text-orange-400',
      py: 'text-green-400',
      json: 'text-yellow-400',
      css: 'text-purple-400',
      html: 'text-red-400',
      md: 'text-zinc-400',
      toml: 'text-red-400',
      yaml: 'text-pink-400',
      yml: 'text-pink-400',
    };
    return colorMap[ext || ''] || 'text-zinc-400';
  };

  const formatSize = (bytes: number): string => {
    if (bytes === 0) return '';
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
    return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
  };

  const MAX_FILE_SIZE = 15 * 1024 * 1024; // 15 MB

  return (
    <div>
      <div
        className="relative group"
        onMouseEnter={() => setHovered(true)}
        onMouseLeave={() => setHovered(false)}
      >
        <button
          className="w-full flex items-center gap-1 h-[26px] px-2 text-[13px] text-zinc-300 hover:bg-zinc-800/80 transition-colors group"
          style={{ paddingLeft: `${depth * 12 + 8}px` }}
          onClick={handleClick}
          onDoubleClick={handleDoubleClick}
          onContextMenu={(e) => onContextMenu?.(e, entry)}
        >
          {entry.is_dir ? (
            expanded ? (
              <ChevronDown className="w-3.5 h-3.5 text-zinc-500 shrink-0" />
            ) : (
              <ChevronRight className="w-3.5 h-3.5 text-zinc-500 shrink-0" />
            )
          ) : (
            <span className="w-3.5 shrink-0" />
          )}

          {entry.is_dir ? (
            expanded ? (
              <FolderOpen className="w-4 h-4 text-blue-400 shrink-0" />
            ) : (
              <Folder className="w-4 h-4 text-zinc-500 shrink-0" />
            )
          ) : (
            <File className={`w-4 h-4 shrink-0 ${getFileColor(entry.extension)}`} />
          )}

          <span className="truncate ml-1">{entry.name}</span>
          {isPinned && !hovered && (
            <Star className="w-3 h-3 text-amber-400 ml-auto shrink-0 fill-amber-400" />
          )}
          {!entry.is_dir && entry.size > 0 && !isPinned && !loading && (
            <span className="ml-auto text-[10px] text-zinc-600 opacity-0 group-hover:opacity-100 transition-opacity shrink-0">
              {formatSize(entry.size)}
            </span>
          )}
          {loading && <span className="ml-auto text-[10px] text-zinc-600 animate-pulse">...</span>}
        </button>

        {/* Pin button on hover */}
        {hovered && onTogglePin && (
          <button
            onClick={(e) => {
              e.stopPropagation();
              onTogglePin(entry.path, entry.name, entry.is_dir);
            }}
            className={`absolute right-1 top-1/2 -translate-y-1/2 p-0.5 rounded hover:bg-zinc-700 transition-colors ${
              isPinned ? 'text-amber-400' : 'text-zinc-600 hover:text-amber-400'
            }`}
            title={isPinned ? 'Unpin' : 'Pin to favorites'}
          >
            {isPinned ? (
              <PinOff className="w-3 h-3" />
            ) : (
              <Pin className="w-3 h-3" />
            )}
          </button>
        )}
      </div>

      {entry.is_dir &&
        expanded &&
        children.map((child) => (
          <TreeNode key={child.path} entry={child} depth={depth + 1} onNavigateDir={onNavigateDir} isPinned={onTogglePin ? false : undefined} onTogglePin={onTogglePin} onContextMenu={onContextMenu} />
        ))}
    </div>
  );
}

// --- Local File Explorer View ---

interface LocalFileExplorerProps {
  localTerminalId: string | null;
}

interface PinnedItem {
  path: string;
  name: string;
  isDir: boolean;
}

const PINNED_STORAGE_KEY = 'operon-pinned-items';

function loadPinnedItems(): PinnedItem[] {
  try {
    const stored = localStorage.getItem(PINNED_STORAGE_KEY);
    return stored ? JSON.parse(stored) : [];
  } catch { return []; }
}

function savePinnedItems(items: PinnedItem[]) {
  try {
    localStorage.setItem(PINNED_STORAGE_KEY, JSON.stringify(items));
  } catch { /* ignore */ }
}

function LocalFileExplorer({ localTerminalId }: LocalFileExplorerProps) {
  const { projectPath, setProjectPath, openFile, openBinaryFile } = useProject();
  const [entries, setEntries] = useState<FileEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const [pinnedItems, setPinnedItems] = useState<PinnedItem[]>(loadPinnedItems);
  const [refreshKey, setRefreshKey] = useState(0);
  const [creatingFolder, setCreatingFolder] = useState(false);
  const [newFolderName, setNewFolderName] = useState('');
  const newFolderRef = useRef<HTMLInputElement>(null);

  // Context menu for file operations
  const [contextMenu, setContextMenu] = useState<{ x: number; y: number; entry: FileEntry } | null>(null);
  const [deleteConfirm, setDeleteConfirm] = useState<FileEntry | null>(null);
  // Regex-add dialog — open from the context menu for folders
  const [regexDialogRoot, setRegexDialogRoot] = useState<FileEntry | null>(null);

  useEffect(() => {
    if (!contextMenu) return;
    const close = () => setContextMenu(null);
    window.addEventListener('click', close);
    return () => window.removeEventListener('click', close);
  }, [contextMenu]);

  const [renaming, setRenaming] = useState<FileEntry | null>(null);
  const [renameInput, setRenameInput] = useState('');
  const renameRef = useRef<HTMLInputElement>(null);

  const addToChat = useCallback((entry: FileEntry) => {
    window.dispatchEvent(new CustomEvent('chat-add-context', {
      detail: {
        kind: 'file',
        name: entry.name,
        path: entry.path,
        isDir: entry.is_dir,
        isRemote: false,
      },
    }));
  }, []);

  const handleContextMenu = useCallback((e: React.MouseEvent, entry: FileEntry) => {
    e.preventDefault();
    e.stopPropagation();
    setContextMenu({ x: e.clientX, y: e.clientY, entry });
  }, []);

  const handleRenameLocal = async () => {
    if (!renaming || !renameInput.trim()) {
      setRenaming(null);
      return;
    }
    const dir = renaming.path.replace(/\/[^/]+$/, '');
    const newPath = `${dir}/${renameInput.trim()}`;
    try {
      await invoke('rename_path', { oldPath: renaming.path, newPath });
      setRenaming(null);
      if (projectPath) loadDir(projectPath);
    } catch (err) {
      console.error('Failed to rename:', err);
      setRenaming(null);
    }
  };

  const handleDeleteLocalFile = async (entry: FileEntry) => {
    try {
      await invoke('delete_path', { path: entry.path });
      setDeleteConfirm(null);
      setRefreshKey(k => k + 1);
      if (projectPath) loadDir(projectPath);
    } catch (err) {
      console.error('Failed to delete file:', err);
      setDeleteConfirm(null);
    }
  };

  const handleCreateFolder = async () => {
    const name = newFolderName.trim();
    if (!name || !projectPath) {
      setCreatingFolder(false);
      setNewFolderName('');
      return;
    }
    try {
      await invoke('create_directory', { path: `${projectPath}/${name}` });
      setCreatingFolder(false);
      setNewFolderName('');
      setRefreshKey(k => k + 1);
    } catch (err) {
      console.error('Failed to create folder:', err);
    }
  };

  useEffect(() => {
    if (creatingFolder && newFolderRef.current) {
      newFolderRef.current.focus();
    }
  }, [creatingFolder]);

  const togglePin = useCallback((path: string, name: string, isDir: boolean) => {
    setPinnedItems(prev => {
      const exists = prev.some(p => p.path === path);
      const next = exists ? prev.filter(p => p.path !== path) : [...prev, { path, name, isDir }];
      savePinnedItems(next);
      return next;
    });
  }, []);

  const isPinned = useCallback((path: string) => {
    return pinnedItems.some(p => p.path === path);
  }, [pinnedItems]);

  const openPinnedItem = async (item: PinnedItem) => {
    if (item.isDir) {
      setProjectPath(item.path);
    } else {
      try {
        const ext = item.name.split('.').pop()?.toLowerCase() || '';
        const binaryExts: Record<string, { binaryType: 'image' | 'pdf' | 'html'; mimeType: string }> = {
          png: { binaryType: 'image', mimeType: 'image/png' },
          jpg: { binaryType: 'image', mimeType: 'image/jpeg' },
          jpeg: { binaryType: 'image', mimeType: 'image/jpeg' },
          gif: { binaryType: 'image', mimeType: 'image/gif' },
          pdf: { binaryType: 'pdf', mimeType: 'application/pdf' },
        };
        const binaryInfo = binaryExts[ext];
        if (binaryInfo) {
          const base64Content = await invoke<string>('read_file_base64', { path: item.path });
          openBinaryFile(item.path, base64Content, binaryInfo.mimeType, binaryInfo.binaryType, false);
        } else {
          const content = await invoke<string>('read_file', { path: item.path });
          openFile(item.path, content, false);
        }
      } catch (err) {
        console.error('Failed to open pinned file:', err);
      }
    }
  };

  const loadDir = useCallback(
    async (path: string) => {
      setLoading(true);
      try {
        const items = await invoke<FileEntry[]>('list_directory', { path });
        setEntries(items);
      } catch (err) {
        console.error('Failed to load directory:', err);
      }
      setLoading(false);
    },
    [],
  );

  // Track whether we've already attempted to restore from settings
  const restoredRef = useRef(false);

  useEffect(() => {
    if (projectPath) {
      loadDir(projectPath);
      // Persist the project path to settings so it's restored on next launch
      invoke('get_settings').then((settings: any) => {
        if (settings.last_project_path !== projectPath) {
          invoke('update_settings', { settings: { ...settings, last_project_path: projectPath } }).catch(() => {});
        }
      }).catch(() => {});
    } else if (!restoredRef.current) {
      // First load — try to restore last project path from settings.
      // Do NOT fall back to ~ (home directory) — listing ~ triggers macOS TCC
      // permission dialogs for Desktop/Downloads/Documents on every launch.
      restoredRef.current = true;
      invoke<any>('get_settings')
        .then((settings) => {
          if (settings.last_project_path) {
            setProjectPath(settings.last_project_path);
          }
          // If no last path, stay in "no project" state — user picks a folder
        })
        .catch(console.error);
    }
  }, [projectPath, loadDir, setProjectPath]);

  const refresh = () => {
    if (projectPath) {
      loadDir(projectPath);
      // Bump key to force all TreeNodes to remount with fresh data
      setRefreshKey((k) => k + 1);
    }
  };

  // Suppress auto-cd when navigation was triggered by terminal CWD sync
  const syncedFromTerminal = useRef(false);

  const cdToTerminalPath = (path: string) => {
    if (!path || !localTerminalId) return;
    const encoded = Array.from(
      new TextEncoder().encode(`cd '${path.replace(/'/g, "'\\''")}'\n`)
    );
    invoke('write_terminal', {
      terminalId: localTerminalId,
      data: encoded,
    }).catch((err) => console.error('Failed to cd in terminal:', err));
  };

  const navigateTo = (path: string) => {
    setProjectPath(path);
    // Auto-cd terminal unless this navigation was triggered by terminal sync
    if (!syncedFromTerminal.current) {
      cdToTerminalPath(path);
    }
    syncedFromTerminal.current = false;
  };

  const navigateUp = () => {
    if (!projectPath || projectPath === '/') return;
    const parent = projectPath.replace(/\/[^/]+\/?$/, '') || '/';
    navigateTo(parent);
  };

  const cdToTerminal = () => {
    if (!projectPath) return;
    cdToTerminalPath(projectPath);
  };

  // Listen for terminal CWD changes → update sidebar (terminal → sidebar sync)
  const projectPathRef = useRef(projectPath);
  projectPathRef.current = projectPath;
  useEffect(() => {
    const unlisten = listen<{ terminalId: string; cwd: string }>('terminal-cwd-changed', (event) => {
      const { cwd } = event.payload;
      if (cwd && cwd !== projectPathRef.current) {
        syncedFromTerminal.current = true;
        setProjectPath(cwd);
      }
    });
    return () => { unlisten.then((u) => u()); };
  }, [setProjectPath]);

  const folderName = projectPath?.split('/').pop() || 'Project';

  // Go-to-folder editable path bar
  const [isEditingPath, setIsEditingPath] = useState(false);
  const [pathInput, setPathInput] = useState(projectPath || '');
  const pathInputRef = useRef<HTMLInputElement>(null);

  // Sync pathInput when projectPath changes externally
  useEffect(() => {
    if (!isEditingPath) setPathInput(projectPath || '');
  }, [projectPath, isEditingPath]);

  // Focus the input when entering edit mode
  useEffect(() => {
    if (isEditingPath && pathInputRef.current) {
      pathInputRef.current.focus();
      pathInputRef.current.select();
    }
  }, [isEditingPath]);

  const commitPathInput = () => {
    const trimmed = pathInput.trim();
    if (trimmed && trimmed !== projectPath) {
      setProjectPath(trimmed);
    }
    setIsEditingPath(false);
  };

  return (
    <>
      <div className="flex items-center justify-between px-3 py-1.5 border-b border-zinc-800/50">
        <div className="flex items-center gap-1.5 text-xs text-zinc-400">
          <Folder className="w-3.5 h-3.5" />
          <span className="font-medium truncate">{folderName}</span>
        </div>
        <div className="flex items-center gap-0.5">
          <button
            onClick={navigateUp}
            className="p-1 rounded hover:bg-zinc-800 text-zinc-500 hover:text-zinc-300 text-xs"
            title="Go Up"
          >
            ..
          </button>
          <button
            onClick={() => { setCreatingFolder(true); setNewFolderName(''); }}
            className="p-1 rounded hover:bg-zinc-800 text-zinc-500 hover:text-zinc-300"
            title="New Folder"
          >
            <FolderPlus className="w-3.5 h-3.5" />
          </button>
          <button
            onClick={refresh}
            className="p-1 rounded hover:bg-zinc-800 text-zinc-500 hover:text-zinc-300"
            title="Refresh"
          >
            <RefreshCw className="w-3.5 h-3.5" />
          </button>
          <button
            onClick={cdToTerminal}
            className="p-1 rounded hover:bg-zinc-800 text-zinc-500 hover:text-zinc-300 transition-colors shrink-0"
            title="cd to this directory in terminal"
          >
            <CornerDownRight className="w-3.5 h-3.5" />
          </button>
        </div>
      </div>

      {/* Go-to-folder path bar — click to type a path, press Enter to navigate */}
      <div className="flex items-center gap-1 px-2 py-1 border-b border-zinc-800/30">
        {isEditingPath ? (
          <input
            ref={pathInputRef}
            type="text"
            value={pathInput}
            onChange={(e) => setPathInput(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') commitPathInput();
              if (e.key === 'Escape') {
                setPathInput(projectPath || '');
                setIsEditingPath(false);
              }
            }}
            onBlur={commitPathInput}
            className="flex-1 bg-zinc-900 border border-blue-700/50 rounded px-1.5 py-0.5 text-[11px] text-zinc-300 font-mono outline-none focus:border-blue-500 min-w-0"
            placeholder="/path/to/folder"
            spellCheck={false}
          />
        ) : (
          <button
            onClick={() => setIsEditingPath(true)}
            className="flex-1 text-left text-[11px] text-zinc-500 hover:text-zinc-300 truncate font-mono transition-colors rounded px-1.5 py-0.5 hover:bg-zinc-800/50 min-w-0"
            title="Click to type a path"
          >
            {projectPath || '~'}
          </button>
        )}
      </div>

      <div className="flex-1 overflow-y-auto py-1">
        {/* Inline new folder input */}
        {creatingFolder && (
          <div className="flex items-center gap-1 px-2 py-1 mx-1 mb-1 bg-zinc-800/80 rounded border border-blue-600/40">
            <FolderPlus className="w-3.5 h-3.5 text-blue-400 shrink-0" />
            <input
              ref={newFolderRef}
              type="text"
              value={newFolderName}
              onChange={(e) => setNewFolderName(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') handleCreateFolder();
                if (e.key === 'Escape') { setCreatingFolder(false); setNewFolderName(''); }
              }}
              onBlur={handleCreateFolder}
              className="flex-1 bg-transparent text-[13px] text-zinc-200 outline-none placeholder:text-zinc-600 min-w-0"
              placeholder="folder name"
              spellCheck={false}
            />
          </div>
        )}

        {/* Pinned/Favorites section */}
        {pinnedItems.length > 0 && (
          <div className="mb-2 border-b border-zinc-600/40 pb-2">
            <div className="flex items-center gap-1.5 px-3 py-1 text-[10px] text-amber-400/70 font-medium uppercase tracking-wider">
              <Star className="w-3 h-3 fill-amber-400/50" />
              Favorites
            </div>
            {pinnedItems.map((item) => (
              <div key={item.path} className="relative group">
                <button
                  className="w-full flex items-center gap-1.5 h-[26px] px-3 text-[13px] text-zinc-300 hover:bg-zinc-800/80 transition-colors"
                  onClick={() => openPinnedItem(item)}
                  title={item.path}
                >
                  {item.isDir ? (
                    <Folder className="w-4 h-4 text-amber-400/70 shrink-0" />
                  ) : (
                    <File className="w-4 h-4 text-amber-400/70 shrink-0" />
                  )}
                  <span className="truncate">{item.name}</span>
                </button>
                <button
                  onClick={(e) => {
                    e.stopPropagation();
                    togglePin(item.path, item.name, item.isDir);
                  }}
                  className="absolute right-1 top-1/2 -translate-y-1/2 p-0.5 rounded text-zinc-600 hover:text-red-400 hover:bg-zinc-700 transition-colors opacity-0 group-hover:opacity-100"
                  title="Unpin"
                >
                  <PinOff className="w-3 h-3" />
                </button>
              </div>
            ))}
          </div>
        )}

        {/* File tree */}
        {loading ? (
          <div className="px-4 py-8 text-center text-zinc-600 text-sm">Loading...</div>
        ) : entries.length === 0 ? (
          <div className="px-4 py-8 text-center text-zinc-600 text-sm">Empty folder</div>
        ) : (
          entries.map((entry) => (
            <TreeNode
              key={`${entry.path}-${refreshKey}`}
              entry={entry}
              depth={0}
              onNavigateDir={navigateTo}
              isPinned={isPinned(entry.path)}
              onTogglePin={togglePin}
              onContextMenu={handleContextMenu}
            />
          ))
        )}
      </div>

      {/* Context menu */}
      {contextMenu && (
        <div
          className="fixed z-[100] bg-zinc-800 border border-zinc-600 rounded-lg shadow-xl py-1 min-w-[180px]"
          style={{
            left: Math.min(contextMenu.x, window.innerWidth - 200),
            top: Math.min(contextMenu.y, window.innerHeight - 240),
          }}
        >
          <button
            onClick={() => {
              addToChat(contextMenu.entry);
              setContextMenu(null);
            }}
            className="w-full flex items-center gap-2 px-3 py-1.5 text-[12px] text-blue-300 hover:bg-zinc-700 transition-colors text-left"
          >
            <MessageSquarePlus className="w-3.5 h-3.5 text-blue-400 pointer-events-none" />
            Add {contextMenu.entry.is_dir ? 'folder' : 'file'} to chat
          </button>
          {contextMenu.entry.is_dir && (
            <button
              onClick={() => {
                setRegexDialogRoot(contextMenu.entry);
                setContextMenu(null);
              }}
              className="w-full flex items-center gap-2 px-3 py-1.5 text-[12px] text-purple-300 hover:bg-zinc-700 transition-colors text-left"
            >
              <Filter className="w-3.5 h-3.5 text-purple-400 pointer-events-none" />
              Add matching files to chat…
            </button>
          )}
          <div className="border-t border-zinc-700 my-1" />
          <button
            onClick={() => {
              setRenaming(contextMenu.entry);
              setRenameInput(contextMenu.entry.name);
              setContextMenu(null);
              setTimeout(() => renameRef.current?.select(), 50);
            }}
            className="w-full flex items-center gap-2 px-3 py-1.5 text-[12px] text-zinc-300 hover:bg-zinc-700 transition-colors text-left"
          >
            <Pencil className="w-3.5 h-3.5 text-zinc-500 pointer-events-none" />
            Rename
          </button>
          <button
            onClick={() => {
              navigator.clipboard.writeText(contextMenu.entry.path);
              setContextMenu(null);
            }}
            className="w-full flex items-center gap-2 px-3 py-1.5 text-[12px] text-zinc-300 hover:bg-zinc-700 transition-colors text-left"
          >
            <Copy className="w-3.5 h-3.5 text-zinc-500 pointer-events-none" />
            Copy path
          </button>
          <div className="border-t border-zinc-700 my-1" />
          <button
            onClick={() => {
              setDeleteConfirm(contextMenu.entry);
              setContextMenu(null);
            }}
            className="w-full flex items-center gap-2 px-3 py-1.5 text-[12px] text-red-400 hover:bg-zinc-700 transition-colors text-left"
          >
            <Trash2 className="w-3.5 h-3.5 pointer-events-none" />
            Delete
          </button>
        </div>
      )}

      {/* Regex bulk-add dialog */}
      {regexDialogRoot && (
        <RegexAddDialog
          rootPath={regexDialogRoot.path}
          rootName={regexDialogRoot.name}
          isRemote={false}
          onClose={() => setRegexDialogRoot(null)}
        />
      )}

      {/* Rename inline input */}
      {renaming && (
        <div className="absolute bottom-3 left-3 right-3 z-50 px-3 py-2.5 bg-zinc-800 border border-zinc-600 rounded-lg shadow-lg">
          <p className="text-[11px] text-zinc-400 mb-1.5">
            Rename <span className="font-medium text-zinc-200">{renaming.name}</span>
          </p>
          <input
            ref={renameRef}
            value={renameInput}
            onChange={(e) => setRenameInput(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') handleRenameLocal();
              if (e.key === 'Escape') setRenaming(null);
            }}
            className="w-full bg-zinc-900 border border-zinc-700 rounded px-2 py-1 text-xs text-zinc-200 outline-none focus:border-blue-500"
            autoFocus
          />
          <div className="flex items-center gap-2 mt-2">
            <button onClick={handleRenameLocal} className="px-2 py-0.5 text-[11px] rounded bg-blue-600 hover:bg-blue-500 text-white">Rename</button>
            <button onClick={() => setRenaming(null)} className="px-2 py-0.5 text-[11px] rounded bg-zinc-700 hover:bg-zinc-600 text-zinc-300">Cancel</button>
          </div>
        </div>
      )}

      {/* Delete confirmation */}
      {deleteConfirm && (
        <div className="absolute bottom-3 left-3 right-3 z-50 px-3 py-2.5 bg-red-950/90 border border-red-800/60 rounded-lg shadow-lg">
          <p className="text-[11px] text-red-300 mb-2">
            Delete <span className="font-medium text-red-200">{deleteConfirm.name}</span>?
          </p>
          <div className="flex items-center gap-2">
            <button
              onClick={() => handleDeleteLocalFile(deleteConfirm)}
              className="px-2.5 py-1 bg-red-600 hover:bg-red-500 text-white text-[10px] rounded transition-colors"
            >
              Delete
            </button>
            <button
              onClick={() => setDeleteConfirm(null)}
              className="px-2.5 py-1 bg-zinc-700 hover:bg-zinc-600 text-zinc-300 text-[10px] rounded transition-colors"
            >
              Cancel
            </button>
          </div>
        </div>
      )}
    </>
  );
}

// --- File Explorer View with Local/Remote toggle ---

interface SSHConnection {
  profileId: string;
  profileName: string;
  terminalId: string;
}

interface FileExplorerViewProps {
  sshConnection: SSHConnection | null;
  localTerminalId: string | null;
}

function FileExplorerView({ sshConnection, localTerminalId }: FileExplorerViewProps) {
  const [explorerMode, setExplorerMode] = useState<'local' | 'remote'>('local');

  // Auto-switch to remote when a new SSH connection arrives
  useEffect(() => {
    if (sshConnection) {
      setExplorerMode('remote');
    }
  }, [sshConnection]);

  const hasRemote = sshConnection !== null;

  return (
    <div className="flex flex-col h-full">
      {/* Header with toggle */}
      <div className="flex items-center justify-between px-3 py-2 border-b border-zinc-800">
        <span className="text-[11px] font-semibold text-zinc-500 uppercase tracking-wider">
          Explorer
        </span>

        {hasRemote && (
          <div className="flex items-center bg-zinc-800 rounded-md p-0.5">
            <button
              onClick={() => setExplorerMode('local')}
              className={`flex items-center gap-1 px-2 py-0.5 rounded text-[10px] font-medium transition-colors ${
                explorerMode === 'local'
                  ? 'bg-zinc-700 text-zinc-200'
                  : 'text-zinc-500 hover:text-zinc-400'
              }`}
              title="Local files"
            >
              <Monitor className="w-3 h-3" />
              Local
            </button>
            <button
              onClick={() => setExplorerMode('remote')}
              className={`flex items-center gap-1 px-2 py-0.5 rounded text-[10px] font-medium transition-colors ${
                explorerMode === 'remote'
                  ? 'bg-green-900/60 text-green-300'
                  : 'text-zinc-500 hover:text-zinc-400'
              }`}
              title={`Remote: ${sshConnection?.profileName}`}
            >
              <Server className="w-3 h-3" />
              Remote
            </button>
          </div>
        )}
      </div>

      {/* Content */}
      {explorerMode === 'local' || !sshConnection ? (
        <LocalFileExplorer localTerminalId={localTerminalId} />
      ) : (
        <RemoteExplorer
          profileId={sshConnection.profileId}
          profileName={sshConnection.profileName}
          terminalId={sshConnection.terminalId}
        />
      )}
    </div>
  );
}

// --- Search View ---

interface SearchHit {
  path: string;   // relative path from the search root
  line: number;
  text: string;
}

interface SearchResult {
  hits: SearchHit[];
  backend: string; // "ripgrep-sidecar" | "ripgrep-system" | "ripgrep-remote" | "grep-remote" | "rust-walker" | "noop"
}

interface SearchViewProps {
  sshConnection: SSHConnection | null;
  remotePath: string;
}

function SearchView({ sshConnection, remotePath }: SearchViewProps) {
  const [query, setQuery] = useState('');
  const [caseSensitive, setCaseSensitive] = useState(false);
  const [useRegex, setUseRegex] = useState(false);
  const [mode, setMode] = useState<'local' | 'remote'>('local');
  const [results, setResults] = useState<SearchHit[]>([]);
  const [backend, setBackend] = useState<string>('');
  const [searching, setSearching] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [ran, setRan] = useState(false);
  const [installingRg, setInstallingRg] = useState(false);
  const [installMsg, setInstallMsg] = useState<string | null>(null);
  const { projectPath, openFile } = useProject();

  // Auto-switch mode to remote when an SSH connection is active, local otherwise
  useEffect(() => {
    setMode(sshConnection ? 'remote' : 'local');
  }, [sshConnection]);

  const rootPath = mode === 'remote' ? remotePath : projectPath || '';
  const canSearch =
    query.trim().length > 0 &&
    rootPath.length > 0 &&
    (mode === 'local' || !!sshConnection);

  const handleSearch = async () => {
    if (!canSearch || searching) return;
    setSearching(true);
    setError(null);
    setRan(true);
    try {
      const result = await (mode === 'remote' && sshConnection
        ? invoke<SearchResult>('search_in_remote_directory', {
            profileId: sshConnection.profileId,
            rootPath,
            query,
            caseSensitive,
            useRegex,
            maxResults: 200,
          })
        : invoke<SearchResult>('search_in_directory', {
            rootPath,
            query,
            caseSensitive,
            useRegex,
            maxResults: 200,
          }));
      setResults(result.hits);
      setBackend(result.backend);
    } catch (err) {
      setResults([]);
      setBackend('');
      setError(typeof err === 'string' ? err : String(err));
    } finally {
      setSearching(false);
    }
  };

  const handleInstallRemoteRg = async () => {
    if (!sshConnection || installingRg) return;
    setInstallingRg(true);
    setInstallMsg(null);
    try {
      const msg = await invoke<string>('install_remote_ripgrep', {
        profileId: sshConnection.profileId,
      });
      setInstallMsg(msg);
      // Retry the search to pick up ripgrep
      handleSearch();
    } catch (err) {
      setInstallMsg(`Install failed: ${err}`);
    } finally {
      setInstallingRg(false);
    }
  };

  const openHit = async (hit: SearchHit) => {
    // `hit.path` is relative to rootPath — resolve to absolute
    const sep = rootPath.endsWith('/') || rootPath.endsWith('\\') ? '' : '/';
    const absPath = `${rootPath}${sep}${hit.path}`;
    try {
      if (mode === 'remote' && sshConnection) {
        const content = await invoke<string>('read_remote_file', {
          profileId: sshConnection.profileId,
          path: absPath,
        });
        openFile(absPath, content, false, sshConnection.profileId);
      } else {
        const content = await invoke<string>('read_file', { path: absPath });
        openFile(absPath, content, false);
      }
      // Ask the editor to scroll the matched line into view.
      // Small delay so the tab is mounted before we reveal.
      setTimeout(() => {
        window.dispatchEvent(
          new CustomEvent('reveal-editor-line', {
            detail: { filePath: absPath, line: hit.line },
          }),
        );
      }, 80);
    } catch (err) {
      console.error('Failed to open search hit:', err);
    }
  };

  const rootLabel = mode === 'remote'
    ? (remotePath || '(no remote path — browse a remote folder in Files first)')
    : (projectPath || '(no project folder opened)');

  return (
    <div className="flex flex-col h-full">
      <div className="flex items-center justify-between px-3 py-2 border-b border-zinc-800">
        <span className="text-[11px] font-semibold text-zinc-500 uppercase tracking-wider">
          Search
        </span>
        {sshConnection && (
          <div className="flex items-center gap-1 text-[10px]">
            <button
              onClick={() => setMode('local')}
              className={`px-1.5 py-[1px] rounded ${
                mode === 'local'
                  ? 'bg-blue-600 text-white'
                  : 'text-zinc-500 hover:text-zinc-300'
              }`}
            >
              Local
            </button>
            <button
              onClick={() => setMode('remote')}
              className={`px-1.5 py-[1px] rounded ${
                mode === 'remote'
                  ? 'bg-blue-600 text-white'
                  : 'text-zinc-500 hover:text-zinc-300'
              }`}
            >
              Remote
            </button>
          </div>
        )}
      </div>

      <div className="px-3 pt-3 pb-2 space-y-1.5">
        <input
          type="text"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => e.key === 'Enter' && handleSearch()}
          placeholder={mode === 'remote' ? 'Search remote files…' : 'Search files…'}
          className="w-full px-2.5 py-1.5 bg-zinc-800 border border-zinc-700 rounded text-sm text-zinc-100 placeholder:text-zinc-600 outline-none focus:border-blue-500"
        />
        <div className="flex items-center justify-between text-[10px]">
          <div className="flex items-center gap-3">
            <label className="flex items-center gap-1 text-zinc-500 cursor-pointer select-none">
              <input
                type="checkbox"
                checked={caseSensitive}
                onChange={(e) => setCaseSensitive(e.target.checked)}
                className="accent-blue-500"
              />
              Aa
            </label>
            <label className="flex items-center gap-1 text-zinc-500 cursor-pointer select-none" title="Treat query as a regular expression">
              <input
                type="checkbox"
                checked={useRegex}
                onChange={(e) => setUseRegex(e.target.checked)}
                className="accent-blue-500"
              />
              .*
            </label>
          </div>
          <button
            onClick={handleSearch}
            disabled={!canSearch || searching}
            className="px-2 py-[2px] bg-blue-600 hover:bg-blue-500 disabled:bg-zinc-700 disabled:text-zinc-500 text-white rounded text-[10px]"
          >
            {searching ? 'Searching…' : 'Search'}
          </button>
        </div>
        <div className="text-[9px] text-zinc-600 truncate" title={rootLabel}>
          in: {rootLabel}
        </div>
        {backend && ran && !error && (
          <div className="flex items-center justify-between text-[9px] text-zinc-600">
            <span title="Search engine that handled this query">engine: {backend}</span>
            {backend === 'grep-remote' && sshConnection && (
              <button
                onClick={handleInstallRemoteRg}
                disabled={installingRg}
                className="px-1.5 py-[1px] rounded bg-zinc-800 hover:bg-zinc-700 disabled:opacity-50 text-zinc-300 text-[9px]"
                title="Upload Operon's bundled ripgrep binary to ~/.operon/bin/rg on this server"
              >
                {installingRg ? 'Installing…' : 'Install ripgrep on server'}
              </button>
            )}
          </div>
        )}
        {installMsg && (
          <div className="text-[9px] text-zinc-500 break-words">{installMsg}</div>
        )}
      </div>

      <div className="flex-1 overflow-y-auto border-t border-zinc-800/60">
        {searching && results.length === 0 && (
          <div className="px-4 py-8 text-center text-zinc-600 text-xs">
            Searching…
          </div>
        )}
        {!searching && error && (
          <div className="px-4 py-3 text-[11px] text-red-400 break-all">
            {error}
          </div>
        )}
        {!searching && !error && results.length === 0 && ran && (
          <div className="px-4 py-8 text-center text-zinc-600 text-xs">
            No results found
          </div>
        )}
        {!searching && !error && results.length === 0 && !ran && (
          <div className="px-4 py-8 text-center text-zinc-600 text-xs">
            {mode === 'remote' && !remotePath
              ? 'Open a remote folder in the Files view first.'
              : 'Type a query and press Enter.'}
          </div>
        )}
        {results.length > 0 && (
          <>
            <div className="px-3 py-1.5 text-[10px] text-zinc-500 sticky top-0 bg-zinc-900 border-b border-zinc-800/60">
              {results.length} match{results.length === 1 ? '' : 'es'}
              {results.length >= 200 ? ' (capped at 200)' : ''}
            </div>
            {results.map((r, i) => (
              <button
                key={`${r.path}:${r.line}:${i}`}
                className="w-full text-left px-3 py-1 hover:bg-zinc-800 text-xs border-b border-zinc-800/30"
                onClick={() => openHit(r)}
                title={r.path}
              >
                <div className="text-zinc-300 truncate">
                  {r.path.split('/').pop()}
                  <span className="text-zinc-600"> · {r.path}</span>
                </div>
                <div className="text-zinc-500 truncate font-mono text-[11px]">
                  <span className="text-zinc-600">L{r.line}:</span> {r.text}
                </div>
              </button>
            ))}
          </>
        )}
      </div>
    </div>
  );
}

// SSHView is now imported from ./SSHView.tsx

// --- Main Sidebar ---

export function Sidebar({ activeView, onViewChange }: SidebarProps) {
  const [sshConnection, setSSHConnection] = useState<SSHConnection | null>(null);
  const [localTerminalId, setLocalTerminalId] = useState<string | null>(null);
  const [activeProtocolIds, setActiveProtocolIds] = useState<string[]>([]);
  const [currentRemotePath, setCurrentRemotePath] = useState<string>('');

  // Listen for remote path changes at Sidebar level (always mounted)
  useEffect(() => {
    const unlisten = listen<{ profileId: string; remotePath: string }>('remote-path-changed', (event) => {
      setCurrentRemotePath(event.payload.remotePath);
    });
    return () => { unlisten.then((u) => u()); };
  }, []);

  // Listen for local terminal active events
  useEffect(() => {
    const unlisten = listen<{ terminalId: string }>('local-terminal-active', (event) => {
      setLocalTerminalId(event.payload.terminalId);
    });
    return () => { unlisten.then((u) => u()); };
  }, []);

  // Listen for SSH connections at the Sidebar level (always mounted)
  // so we capture the event regardless of which view is active
  useEffect(() => {
    const unlisten = listen<{
      terminalId: string;
      title: string;
      sshCommand?: string;
      profileId?: string;
      profileName?: string;
    }>('open-ssh-terminal', (event) => {
      const { profileId, profileName, terminalId } = event.payload;
      if (profileId && profileName) {
        setSSHConnection({ profileId, profileName, terminalId });
        // Auto-switch to the files view to show the remote explorer
        onViewChange?.('files');
      }
    });

    return () => { unlisten.then((u) => u()); };
  }, [onViewChange]);

  // Listen for tool panel events from ExtensionsView
  useEffect(() => {
    const handleOpenToolPanel = (event: Event) => {
      const customEvent = event as CustomEvent<{ toolId: string }>;
      const toolId = customEvent.detail?.toolId;
      if (toolId) {
        onViewChange?.(toolId);
      }
    };

    window.addEventListener('open-tool-panel', handleOpenToolPanel);
    return () => {
      window.removeEventListener('open-tool-panel', handleOpenToolPanel);
    };
  }, [onViewChange]);

  return (
    <div className="h-full bg-zinc-900 overflow-hidden">
      {/* FileExplorerView is always mounted but hidden when inactive.
          This preserves directory listing, scroll position, and expanded state
          across sidebar tab switches (prevents reset to home directory). */}
      <div className={activeView === 'files' ? 'h-full' : 'hidden'}>
        <FileExplorerView sshConnection={sshConnection} localTerminalId={localTerminalId} />
      </div>
      {activeView === 'search' && (
        <SearchView sshConnection={sshConnection} remotePath={currentRemotePath} />
      )}
      {activeView === 'git' && <GitPanel />}
      {activeView === 'extensions' && <ExtensionsView />}
      {activeView === 'ssh' && <SSHView onConnectSSH={() => {}} />}
      {activeView === 'protocols' && (
        <ProtocolsView
          activeProtocolIds={activeProtocolIds}
          onToggle={(protocol, allActive) => {
            setActiveProtocolIds(allActive.map((p) => p.id));
            emit('protocols-changed', allActive.length > 0 ? allActive : null);
          }}
          sshConnection={sshConnection}
          remotePath={currentRemotePath}
        />
      )}
      {activeView === 'docker' && <dockerExtension.SidebarPanel />}
      {activeView === 'singularity' && <singularityExtension.SidebarPanel />}
      {activeView === 'settings' && (
        <div className="flex flex-col h-full">
          <div className="flex items-center px-3 py-2 border-b border-zinc-800">
            <span className="text-[11px] font-semibold text-zinc-500 uppercase tracking-wider">
              Settings
            </span>
          </div>
          <div className="flex-1 flex items-center justify-center text-zinc-600 text-sm">
            Settings panel (Phase 7)
          </div>
        </div>
      )}
    </div>
  );
}

// --- Regex bulk-add dialog ---
//
// Lets users select many files under a folder without expanding it. Local
// uses the Rust `regex` crate (RE2); remote uses GNU `grep -E` (ERE).
// Matched paths are dispatched as a single "group" mention so the chat
// context stays compact.

interface RegexAddDialogProps {
  rootPath: string;
  rootName: string;
  isRemote: boolean;
  profileId?: string;
  onClose: () => void;
}

interface RegexMatchResult {
  paths: string[];
  total_matched: number;
  truncated: boolean;
}

export function RegexAddDialog({
  rootPath,
  rootName,
  isRemote,
  profileId,
  onClose,
}: RegexAddDialogProps) {
  const [pattern, setPattern] = useState('');
  const [recursive, setRecursive] = useState(true);
  const [caseSensitive, setCaseSensitive] = useState(false);
  const [matchFullPath, setMatchFullPath] = useState(false);
  const [preview, setPreview] = useState<RegexMatchResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    if (debounceRef.current) clearTimeout(debounceRef.current);
    const p = pattern.trim();
    if (!p) {
      setPreview(null);
      setError(null);
      return;
    }
    debounceRef.current = setTimeout(async () => {
      setLoading(true);
      setError(null);
      try {
        const cmd = isRemote
          ? 'list_remote_files_matching_regex'
          : 'list_files_matching_regex';
        const args: Record<string, unknown> = {
          rootPath,
          pattern: p,
          recursive,
          caseSensitive,
          matchFullPath,
          maxResults: 500,
        };
        if (isRemote && profileId) args.profileId = profileId;
        const result = await invoke<RegexMatchResult>(cmd, args);
        setPreview(result);
      } catch (err) {
        setPreview(null);
        setError(typeof err === 'string' ? err : String(err));
      } finally {
        setLoading(false);
      }
    }, 300);
    return () => {
      if (debounceRef.current) clearTimeout(debounceRef.current);
    };
  }, [pattern, recursive, caseSensitive, matchFullPath, rootPath, isRemote, profileId]);

  const handleAdd = () => {
    if (!preview || preview.paths.length === 0) return;
    const groupName =
      preview.paths.length === 1
        ? preview.paths[0].split('/').pop() || preview.paths[0]
        : `${rootName} · ${preview.paths.length} files`;
    window.dispatchEvent(new CustomEvent('chat-add-context', {
      detail: {
        kind: 'group',
        name: groupName,
        path: rootPath,
        isDir: true,
        pattern: pattern.trim(),
        paths: preview.paths,
        isRemote,
      },
    }));
    onClose();
  };

  return (
    <div
      className="fixed inset-0 z-[200] flex items-center justify-center bg-black/60"
      onClick={onClose}
    >
      <div
        className="w-[520px] max-w-[90vw] max-h-[80vh] flex flex-col bg-zinc-900 border border-zinc-700 rounded-lg shadow-2xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="px-4 py-3 border-b border-zinc-800">
          <div className="flex items-center gap-2">
            <Filter className="w-4 h-4 text-purple-400" />
            <h3 className="text-sm font-semibold text-zinc-200">
              Add matching files to chat
            </h3>
          </div>
          <div className="mt-1 text-[11px] text-zinc-500 truncate" title={rootPath}>
            under: {rootPath}
          </div>
        </div>

        <div className="px-4 py-3 space-y-2">
          <input
            type="text"
            value={pattern}
            onChange={(e) => setPattern(e.target.value)}
            placeholder={isRemote
              ? 'Regex (ERE) — e.g. \\.csv$ or sample_0[0-9]+'
              : 'Regex (RE2) — e.g. \\.csv$ or sample_0[0-9]+'}
            className="w-full px-2.5 py-1.5 bg-zinc-800 border border-zinc-700 rounded text-sm text-zinc-100 placeholder:text-zinc-600 outline-none focus:border-blue-500 font-mono"
            autoFocus
          />
          <div className="flex items-center gap-4 text-[11px] text-zinc-400">
            <label className="flex items-center gap-1 cursor-pointer select-none">
              <input
                type="checkbox"
                checked={recursive}
                onChange={(e) => setRecursive(e.target.checked)}
                className="accent-blue-500"
              />
              Recursive
            </label>
            <label className="flex items-center gap-1 cursor-pointer select-none">
              <input
                type="checkbox"
                checked={caseSensitive}
                onChange={(e) => setCaseSensitive(e.target.checked)}
                className="accent-blue-500"
              />
              Case sensitive
            </label>
            <label className="flex items-center gap-1 cursor-pointer select-none" title="Match against relative path instead of filename only">
              <input
                type="checkbox"
                checked={matchFullPath}
                onChange={(e) => setMatchFullPath(e.target.checked)}
                className="accent-blue-500"
              />
              Match full path
            </label>
          </div>
          <div className="text-[10px] text-zinc-600">
            {isRemote
              ? 'Remote uses GNU grep -E (ERE). No lookaround.'
              : 'Local uses Rust RE2. No lookaround / backrefs.'}
          </div>
        </div>

        <div className="flex-1 overflow-y-auto border-t border-zinc-800 bg-zinc-950/40">
          {loading && (
            <div className="px-4 py-6 text-center text-zinc-600 text-xs">Matching…</div>
          )}
          {error && !loading && (
            <div className="px-4 py-3 text-[11px] text-red-400 break-all">{error}</div>
          )}
          {!loading && !error && preview && (
            <>
              <div className="px-3 py-1.5 text-[10px] text-zinc-500 sticky top-0 bg-zinc-900/80 border-b border-zinc-800/60">
                {preview.total_matched} match{preview.total_matched === 1 ? '' : 'es'}
                {preview.truncated ? ` (showing first ${preview.paths.length})` : ''}
              </div>
              {preview.paths.length === 0 ? (
                <div className="px-4 py-4 text-center text-zinc-600 text-xs">
                  No files match this regex
                </div>
              ) : (
                <ul className="py-1">
                  {preview.paths.slice(0, 200).map((p, i) => (
                    <li
                      key={`${p}-${i}`}
                      className="px-3 py-0.5 text-[11px] text-zinc-400 font-mono truncate"
                      title={p}
                    >
                      {p}
                    </li>
                  ))}
                  {preview.paths.length > 200 && (
                    <li className="px-3 py-1 text-[10px] text-zinc-600">
                      … and {preview.paths.length - 200} more
                    </li>
                  )}
                </ul>
              )}
            </>
          )}
          {!loading && !error && !preview && (
            <div className="px-4 py-6 text-center text-zinc-600 text-xs">
              Type a regex above to preview matching files.
            </div>
          )}
        </div>

        <div className="px-4 py-3 border-t border-zinc-800 flex items-center justify-end gap-2">
          <button
            onClick={onClose}
            className="px-3 py-1 text-[11px] rounded bg-zinc-800 hover:bg-zinc-700 text-zinc-300"
          >
            Cancel
          </button>
          <button
            onClick={handleAdd}
            disabled={!preview || preview.paths.length === 0}
            className="px-3 py-1 text-[11px] rounded bg-purple-600 hover:bg-purple-500 disabled:bg-zinc-800 disabled:text-zinc-600 text-white"
          >
            {preview && preview.paths.length > 0
              ? `Add ${preview.paths.length} file${preview.paths.length === 1 ? '' : 's'} to chat`
              : 'Add to chat'}
          </button>
        </div>
      </div>
    </div>
  );
}
