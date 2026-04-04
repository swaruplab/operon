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
}

function TreeNode({ entry, depth, onNavigateDir, isPinned, onTogglePin }: TreeNodeProps) {
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
          <TreeNode key={child.path} entry={child} depth={depth + 1} onNavigateDir={onNavigateDir} isPinned={onTogglePin ? false : undefined} onTogglePin={onTogglePin} />
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

  useEffect(() => {
    if (projectPath) {
      loadDir(projectPath);
    } else {
      invoke<string>('get_home_dir')
        .then((home) => {
          setProjectPath(home);
          loadDir(home);
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

  const navigateTo = (path: string) => {
    setProjectPath(path);
  };

  const navigateUp = () => {
    if (!projectPath || projectPath === '/') return;
    const parent = projectPath.replace(/\/[^/]+\/?$/, '') || '/';
    navigateTo(parent);
  };

  const cdToTerminal = () => {
    if (!projectPath || !localTerminalId) return;
    const encoded = Array.from(
      new TextEncoder().encode(`cd '${projectPath.replace(/'/g, "'\\''")}'\n`)
    );
    invoke('write_terminal', {
      terminalId: localTerminalId,
      data: encoded,
    }).catch((err) => console.error('Failed to cd in terminal:', err));
  };

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
            />
          ))
        )}
      </div>
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

function SearchView() {
  const [query, setQuery] = useState('');
  const [results, setResults] = useState<{ path: string; line: number; text: string }[]>([]);
  const { projectPath, openFile } = useProject();

  const handleSearch = async () => {
    if (!query.trim() || !projectPath) return;
    // Simple grep-like search using Rust
    try {
      const files = await invoke<FileEntry[]>('list_directory', { path: projectPath });
      const found: { path: string; line: number; text: string }[] = [];
      for (const file of files.filter((f) => !f.is_dir)) {
        try {
          const content = await invoke<string>('read_file', { path: file.path });
          content.split('\n').forEach((lineText, i) => {
            if (lineText.toLowerCase().includes(query.toLowerCase())) {
              found.push({ path: file.path, line: i + 1, text: lineText.trim() });
            }
          });
        } catch {
          // skip unreadable files
        }
        if (found.length >= 50) break;
      }
      setResults(found);
    } catch (err) {
      console.error('Search failed:', err);
    }
  };

  return (
    <div className="flex flex-col h-full">
      <div className="flex items-center justify-between px-3 py-2 border-b border-zinc-800">
        <span className="text-[11px] font-semibold text-zinc-500 uppercase tracking-wider">
          Search
        </span>
      </div>
      <div className="p-3">
        <input
          type="text"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => e.key === 'Enter' && handleSearch()}
          placeholder="Search files..."
          className="w-full px-2.5 py-1.5 bg-zinc-800 border border-zinc-700 rounded text-sm text-zinc-100 placeholder:text-zinc-600 outline-none focus:border-blue-500"
        />
      </div>
      <div className="flex-1 overflow-y-auto">
        {results.length === 0 ? (
          <div className="flex-1 flex items-center justify-center text-zinc-600 text-sm px-4 py-8">
            {query ? 'No results found' : 'Type to search across files'}
          </div>
        ) : (
          results.map((r, i) => (
            <button
              key={i}
              className="w-full text-left px-3 py-1 hover:bg-zinc-800 text-xs"
              onClick={async () => {
                const content = await invoke<string>('read_file', { path: r.path });
                openFile(r.path, content, false);
              }}
            >
              <div className="text-zinc-300 truncate">{r.path.split('/').pop()}</div>
              <div className="text-zinc-600 truncate">
                L{r.line}: {r.text}
              </div>
            </button>
          ))
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
  const [activeProtocolId, setActiveProtocolId] = useState<string | null>(null);

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
      {activeView === 'files' && <FileExplorerView sshConnection={sshConnection} localTerminalId={localTerminalId} />}
      {activeView === 'search' && <SearchView />}
      {activeView === 'git' && <GitPanel />}
      {activeView === 'extensions' && <ExtensionsView />}
      {activeView === 'ssh' && <SSHView onConnectSSH={() => {}} />}
      {activeView === 'protocols' && (
        <ProtocolsView
          activeProtocolId={activeProtocolId}
          onActivate={(protocol) => {
            setActiveProtocolId(protocol?.id ?? null);
            emit('protocol-changed', protocol ? { id: protocol.id, name: protocol.name } : null);
          }}
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
