import { useState, useEffect, useCallback, useRef } from 'react';
import {
  ChevronRight,
  ChevronDown,
  File,
  Folder,
  FolderOpen,
  RefreshCw,
  Server,
  Loader2,
  Eye,
  EyeOff,
  CornerDownRight,
  Star,
  Pin,
  PinOff,
  FolderPlus,
  Upload,
  Download,
  X,
  CheckCircle2,
  XCircle,
  AlertCircle,
} from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { emit, listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
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

interface RemoteExplorerProps {
  profileId: string;
  profileName: string;
  terminalId: string;
}

// --- Remote Tree Node ---

interface RemoteTreeNodeProps {
  entry: FileEntry;
  depth: number;
  profileId: string;
  showHidden: boolean;
  onNavigate: (path: string) => void;
  isPinned?: boolean;
  onTogglePin?: (path: string, name: string, isDir: boolean) => void;
  onContextMenu?: (e: React.MouseEvent, entry: FileEntry) => void;
}

interface ContextMenuState {
  x: number;
  y: number;
  entry: FileEntry;
}

function RemoteTreeNode({ entry, depth, profileId, showHidden, onNavigate, isPinned, onTogglePin, onContextMenu }: RemoteTreeNodeProps) {
  const [expanded, setExpanded] = useState(false);
  const [children, setChildren] = useState<FileEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const [hovered, setHovered] = useState(false);
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
      const entries = await invoke<FileEntry[]>('list_remote_directory', {
        profileId,
        path: entry.path,
        showHidden,
      });
      setChildren(entries);
    } catch (err) {
      console.error('Failed to list remote directory:', err);
    }
    setLoading(false);
    setExpanded(true);
  };

  const MAX_FILE_SIZE = 15 * 1024 * 1024; // 15 MB

  const openRemoteFile = async (preview: boolean) => {
    // Guard: refuse to open files larger than 15 MB to avoid UI hangs
    if (entry.size > MAX_FILE_SIZE) {
      openFile(
        entry.path,
        `⚠ File too large to display\n\nThis file is ${formatSize(entry.size)}, which exceeds the 15 MB limit.\nOpening it in the editor could freeze the application.\n\nPath: ${entry.path}`,
        preview,
        profileId,
      );
      return;
    }

    setLoading(true);
    try {
      const ext = entry.extension?.toLowerCase() || '';
      const binaryInfo = BINARY_EXTENSIONS[ext];

      if (binaryInfo) {
        // Fetch as base64 for binary files
        const base64Content = await invoke<string>('read_remote_file_base64', {
          profileId,
          path: entry.path,
        });
        openBinaryFile(entry.path, base64Content, binaryInfo.mimeType, binaryInfo.binaryType, preview, profileId);
      } else {
        // Fetch as text for code/text files
        const content = await invoke<string>('read_remote_file', {
          profileId,
          path: entry.path,
        });
        openFile(entry.path, content, preview, profileId);
      }
    } catch (err) {
      console.error('Failed to read remote file:', err);
    }
    setLoading(false);
  };

  const handleClick = () => {
    if (entry.is_dir) {
      toggle();
    } else {
      openRemoteFile(true); // single click = preview
    }
  };

  const handleDoubleClick = () => {
    if (entry.is_dir) {
      onNavigate(entry.path);
    } else {
      openRemoteFile(false); // double click = open for editing
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
      sh: 'text-green-400',
      bash: 'text-green-400',
      c: 'text-blue-300',
      cpp: 'text-blue-300',
      h: 'text-blue-300',
      java: 'text-red-300',
      go: 'text-cyan-400',
      rb: 'text-red-400',
      php: 'text-purple-300',
      log: 'text-zinc-500',
      txt: 'text-zinc-400',
      cfg: 'text-zinc-400',
      conf: 'text-zinc-400',
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

  return (
    <div>
      <div
        className="relative"
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

          {!entry.is_dir && entry.size > 0 && !isPinned && (
            <span className="ml-auto text-[10px] text-zinc-600 opacity-0 group-hover:opacity-100 transition-opacity shrink-0">
              {formatSize(entry.size)}
            </span>
          )}

          {loading && (
            <Loader2 className="ml-auto w-3 h-3 text-zinc-600 animate-spin shrink-0" />
          )}
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
          <RemoteTreeNode key={child.path} entry={child} depth={depth + 1} profileId={profileId} showHidden={showHidden} onNavigate={onNavigate} isPinned={onTogglePin ? false : undefined} onTogglePin={onTogglePin} onContextMenu={onContextMenu} />
        ))}
    </div>
  );
}

// --- Remote Pinned Items ---

interface RemotePinnedItem {
  path: string;
  name: string;
  isDir: boolean;
}

function getRemotePinnedKey(profileId: string) {
  return `operon-remote-pinned-${profileId}`;
}

function loadRemotePinnedItems(profileId: string): RemotePinnedItem[] {
  try {
    const stored = localStorage.getItem(getRemotePinnedKey(profileId));
    return stored ? JSON.parse(stored) : [];
  } catch { return []; }
}

function saveRemotePinnedItems(profileId: string, items: RemotePinnedItem[]) {
  try {
    localStorage.setItem(getRemotePinnedKey(profileId), JSON.stringify(items));
  } catch { /* ignore */ }
}

// --- Main Remote Explorer View ---

// ── Transfer progress state ──

interface TransferProgress {
  completed: number;
  total: number;
  current_file: string;
  errors: number;
  status: 'uploading' | 'downloading' | 'done' | 'error';
  message?: string;
}

export function RemoteExplorer({ profileId, profileName, terminalId }: RemoteExplorerProps) {
  const [remotePath, setRemotePath] = useState<string>('');
  const [entries, setEntries] = useState<FileEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [showHidden, setShowHidden] = useState(false);
  const [refreshKey, setRefreshKey] = useState(0);
  const [pinnedItems, setPinnedItems] = useState<RemotePinnedItem[]>(() => loadRemotePinnedItems(profileId));
  const [creatingFolder, setCreatingFolder] = useState(false);
  const [newFolderName, setNewFolderName] = useState('');
  const newFolderRef = useRef<HTMLInputElement>(null);
  const { openFile, openBinaryFile } = useProject();

  // Drag-and-drop state (Tauri 2 window-level events)
  const [isDragOver, setIsDragOver] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);

  // Transfer progress
  const [transfer, setTransfer] = useState<TransferProgress | null>(null);

  // Context menu
  const [contextMenu, setContextMenu] = useState<ContextMenuState | null>(null);

  const handleCreateFolder = async () => {
    const name = newFolderName.trim();
    if (!name || !remotePath) {
      setCreatingFolder(false);
      setNewFolderName('');
      return;
    }
    try {
      await invoke('create_remote_directory', {
        profileId,
        path: `${remotePath}/${name}`,
      });
      setCreatingFolder(false);
      setNewFolderName('');
      setRefreshKey(k => k + 1);
    } catch (err) {
      console.error('Failed to create remote folder:', err);
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
      saveRemotePinnedItems(profileId, next);
      return next;
    });
  }, [profileId]);

  const isPinned = useCallback((path: string) => {
    return pinnedItems.some(p => p.path === path);
  }, [pinnedItems]);

  const loadDir = useCallback(
    async (path: string) => {
      setLoading(true);
      setError(null);
      try {
        const items = await invoke<FileEntry[]>('list_remote_directory', {
          profileId,
          path,
          showHidden,
        });
        setEntries(items);
        setRemotePath(path);
        // Notify other components (e.g. ChatPanel) about the current remote path
        emit('remote-path-changed', { profileId, profileName, remotePath: path });
      } catch (err) {
        console.error('Failed to load remote directory:', err);
        setError(`${err}`);
      }
      setLoading(false);
    },
    [profileId, showHidden],
  );

  // Navigate to a directory in the explorer
  const navigateTo = useCallback(
    (path: string) => {
      loadDir(path);
    },
    [loadDir],
  );

  const openPinnedItem = async (item: RemotePinnedItem) => {
    if (item.isDir) {
      navigateTo(item.path);
    } else {
      try {
        const ext = item.name.split('.').pop()?.toLowerCase() || '';
        const binaryInfo = BINARY_EXTENSIONS[ext];
        if (binaryInfo) {
          const base64Content = await invoke<string>('read_remote_file_base64', { profileId, path: item.path });
          openBinaryFile(item.path, base64Content, binaryInfo.mimeType, binaryInfo.binaryType, false, profileId);
        } else {
          const content = await invoke<string>('read_remote_file', { profileId, path: item.path });
          openFile(item.path, content, false, profileId);
        }
      } catch (err) {
        console.error('Failed to open pinned remote file:', err);
      }
    }
  };

  // On mount, fetch remote home directory and list it
  useEffect(() => {
    if (remotePath) {
      loadDir(remotePath);
    } else {
      invoke<string>('get_remote_home', { profileId })
        .then((home) => {
          loadDir(home);
        })
        .catch((err) => {
          console.error('Failed to get remote home:', err);
          loadDir('/');
        });
    }
  }, [profileId, loadDir]); // loadDir already depends on showHidden

  const refresh = async () => {
    // Clear the backend SSH cache first so we get truly fresh data
    try { await invoke('clear_ssh_cache'); } catch { /* ignore */ }
    if (remotePath) {
      loadDir(remotePath);
      setRefreshKey((k) => k + 1);
    }
  };

  const cdToTerminal = () => {
    if (!remotePath || !terminalId) return;
    const encoded = Array.from(new TextEncoder().encode(`cd '${remotePath.replace(/'/g, "'\\''")}'\n`));
    invoke('write_terminal', {
      terminalId,
      data: encoded,
    }).catch((err) => console.error('Failed to cd in terminal:', err));
  };

  const navigateUp = () => {
    if (!remotePath || remotePath === '/') return;
    const parent = remotePath.replace(/\/[^/]+\/?$/, '') || '/';
    navigateTo(parent);
  };

  const folderName = remotePath === '/' ? '/' : remotePath?.split('/').pop() || 'Remote';

  // Go-to-folder editable path bar
  const [isEditingPath, setIsEditingPath] = useState(false);
  const [pathInput, setPathInput] = useState(remotePath || '');
  const pathInputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (!isEditingPath) setPathInput(remotePath || '');
  }, [remotePath, isEditingPath]);

  useEffect(() => {
    if (isEditingPath && pathInputRef.current) {
      pathInputRef.current.focus();
      pathInputRef.current.select();
    }
  }, [isEditingPath]);

  const commitPathInput = () => {
    const trimmed = pathInput.trim();
    if (trimmed && trimmed !== remotePath) {
      navigateTo(trimmed);
    }
    setIsEditingPath(false);
  };

  // ── Drag & Drop: Local → Remote Upload (Tauri 2 window-level events) ──
  // Tauri 2 does NOT populate File.path on dataTransfer — it fires its own
  // window drag-drop events with native file paths.

  const uploadFiles = useCallback(async (localPaths: string[]) => {
    if (!remotePath || localPaths.length === 0) return;

    setTransfer({
      completed: 0,
      total: localPaths.length,
      current_file: localPaths[0].split('/').pop() || 'file',
      errors: 0,
      status: 'uploading',
    });

    try {
      const completed = await invoke<number>('scp_batch_upload', {
        profileId,
        localPaths,
        remoteDir: remotePath,
      });
      setTransfer({
        completed,
        total: localPaths.length,
        current_file: '',
        errors: localPaths.length - completed,
        status: completed > 0 ? 'done' : 'error',
        message: completed === localPaths.length
          ? `${completed} file${completed > 1 ? 's' : ''} uploaded`
          : `${completed}/${localPaths.length} uploaded (${localPaths.length - completed} failed)`,
      });
      refresh();
    } catch (err) {
      setTransfer({
        completed: 0,
        total: localPaths.length,
        current_file: '',
        errors: localPaths.length,
        status: 'error',
        message: `Upload failed: ${err}`,
      });
    }
  }, [profileId, remotePath, refresh]);

  // Tauri 2 window-level drag-drop listener.
  // PhysicalPosition is in physical pixels; getBoundingClientRect() returns
  // CSS (logical) pixels — scale by devicePixelRatio for hit-testing.
  const isInsideContainer = useCallback((physX: number, physY: number): boolean => {
    if (!containerRef.current) return false;
    const dpr = window.devicePixelRatio || 1;
    const x = physX / dpr;
    const y = physY / dpr;
    const rect = containerRef.current.getBoundingClientRect();
    return x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom;
  }, []);

  useEffect(() => {
    const win = getCurrentWindow();
    const unlisten = win.onDragDropEvent((event) => {
      if (event.payload.type === 'over') {
        const { position } = event.payload;
        setIsDragOver(isInsideContainer(position.x, position.y));
      } else if (event.payload.type === 'drop') {
        setIsDragOver(false);
        const { position, paths } = event.payload;
        if (isInsideContainer(position.x, position.y) && paths && paths.length > 0) {
          uploadFiles(paths);
        }
      } else if (event.payload.type === 'leave') {
        setIsDragOver(false);
      }
    });
    return () => { unlisten.then((u) => u()); };
  }, [uploadFiles, isInsideContainer]);

  // Listen for progress events from batch upload
  useEffect(() => {
    const unlisten = listen<{ completed: number; total: number; current_file: string; errors: number }>(
      'scp-transfer-progress',
      (event) => {
        setTransfer((prev) => prev ? {
          ...prev,
          completed: event.payload.completed,
          current_file: event.payload.current_file,
          errors: event.payload.errors,
        } : null);
      },
    );
    return () => { unlisten.then((u) => u()); };
  }, []);

  // Auto-dismiss transfer toast
  useEffect(() => {
    if (transfer && (transfer.status === 'done' || transfer.status === 'error')) {
      const timer = setTimeout(() => setTransfer(null), 5000);
      return () => clearTimeout(timer);
    }
  }, [transfer]);

  // ── Context Menu: Download to Local ──

  const handleContextMenu = (e: React.MouseEvent, entry: FileEntry) => {
    e.preventDefault();
    e.stopPropagation();
    setContextMenu({ x: e.clientX, y: e.clientY, entry });
  };

  // Close context menu on any click
  useEffect(() => {
    if (!contextMenu) return;
    const close = () => setContextMenu(null);
    window.addEventListener('click', close);
    return () => window.removeEventListener('click', close);
  }, [contextMenu]);

  const downloadToLocal = async (entry: FileEntry) => {
    setContextMenu(null);

    // Use the Tauri save dialog to pick a local destination
    // For simplicity, download to ~/Downloads/
    const homeDir = await invoke<string>('get_home_dir');
    const downloadsDir = `${homeDir}/Downloads`;
    const localDest = `${downloadsDir}/${entry.name}`;

    setTransfer({
      completed: 0,
      total: 1,
      current_file: entry.name,
      errors: 0,
      status: 'downloading',
    });

    try {
      if (entry.is_dir) {
        await invoke('scp_dir_from_remote', {
          profileId,
          remotePath: entry.path,
          localPath: localDest,
        });
      } else {
        await invoke('scp_from_remote', {
          profileId,
          remotePath: entry.path,
          localPath: localDest,
        });
      }
      setTransfer({
        completed: 1,
        total: 1,
        current_file: entry.name,
        errors: 0,
        status: 'done',
        message: `Downloaded to ~/Downloads/${entry.name}`,
      });
    } catch (err) {
      setTransfer({
        completed: 0,
        total: 1,
        current_file: entry.name,
        errors: 1,
        status: 'error',
        message: `Download failed: ${err}`,
      });
    }
  };

  return (
    <div
      ref={containerRef}
      className="flex flex-col h-full relative"
    >
      {/* Drag overlay */}
      {isDragOver && (
        <div className="absolute inset-0 z-50 bg-blue-500/10 border-2 border-dashed border-blue-400 rounded-lg flex flex-col items-center justify-center pointer-events-none">
          <Upload className="w-8 h-8 text-blue-400 mb-2" />
          <p className="text-sm font-medium text-blue-300">Drop files to upload</p>
          <p className="text-xs text-blue-400/70 mt-1">
            to {remotePath || '~'}
          </p>
        </div>
      )}

      {/* Transfer progress toast */}
      {transfer && (
        <div className={`absolute bottom-3 left-3 right-3 z-50 px-3 py-2.5 rounded-lg text-xs border shadow-lg ${
          transfer.status === 'done'
            ? 'bg-green-900/80 text-green-300 border-green-800/60'
            : transfer.status === 'error'
              ? 'bg-red-900/80 text-red-300 border-red-800/60'
              : 'bg-zinc-800/95 text-zinc-300 border-zinc-700/60'
        }`}>
          <div className="flex items-center gap-2">
            {transfer.status === 'uploading' && <Upload className="w-3.5 h-3.5 text-blue-400 animate-pulse shrink-0" />}
            {transfer.status === 'downloading' && <Download className="w-3.5 h-3.5 text-blue-400 animate-pulse shrink-0" />}
            {transfer.status === 'done' && <CheckCircle2 className="w-3.5 h-3.5 text-green-400 shrink-0" />}
            {transfer.status === 'error' && <XCircle className="w-3.5 h-3.5 text-red-400 shrink-0" />}
            <div className="flex-1 min-w-0">
              {transfer.message ? (
                <p className="truncate">{transfer.message}</p>
              ) : (
                <p className="truncate">
                  {transfer.status === 'uploading' ? 'Uploading' : 'Downloading'} {transfer.current_file}
                  {transfer.total > 1 && ` (${transfer.completed}/${transfer.total})`}
                </p>
              )}
            </div>
            <button
              onClick={() => setTransfer(null)}
              className="p-0.5 text-zinc-500 hover:text-zinc-300 transition-colors shrink-0"
            >
              <X className="w-3 h-3" />
            </button>
          </div>
          {/* Progress bar for multi-file transfers */}
          {(transfer.status === 'uploading' || transfer.status === 'downloading') && transfer.total > 1 && (
            <div className="mt-1.5 h-1 bg-zinc-700 rounded-full overflow-hidden">
              <div
                className="h-full bg-blue-500 rounded-full transition-all duration-300"
                style={{ width: `${(transfer.completed / transfer.total) * 100}%` }}
              />
            </div>
          )}
        </div>
      )}

      {/* Context menu */}
      {contextMenu && (
        <div
          className="fixed z-[100] bg-zinc-800 border border-zinc-600 rounded-lg shadow-xl py-1 min-w-[160px]"
          style={{
            left: Math.min(contextMenu.x, window.innerWidth - 180),
            top: Math.min(contextMenu.y, window.innerHeight - 120),
          }}
        >
          <button
            onClick={() => downloadToLocal(contextMenu.entry)}
            className="w-full flex items-center gap-2 px-3 py-1.5 text-[12px] text-zinc-300 hover:bg-zinc-700 transition-colors text-left"
          >
            <Download className="w-3.5 h-3.5 text-blue-400" />
            Download to local
          </button>
          {!contextMenu.entry.is_dir && (
            <button
              onClick={() => {
                navigator.clipboard.writeText(contextMenu.entry.path);
                setContextMenu(null);
              }}
              className="w-full flex items-center gap-2 px-3 py-1.5 text-[12px] text-zinc-300 hover:bg-zinc-700 transition-colors text-left"
            >
              <File className="w-3.5 h-3.5 text-zinc-500" />
              Copy remote path
            </button>
          )}
        </div>
      )}

      {/* Header */}
      <div className="flex items-center justify-between px-3 py-2 border-b border-zinc-800">
        <div className="flex items-center gap-1.5">
          <Server className="w-3.5 h-3.5 text-green-400" />
          <span className="text-[11px] font-semibold text-zinc-500 uppercase tracking-wider">
            Remote
          </span>
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
            onClick={() => setShowHidden((v) => !v)}
            className={`p-1 rounded hover:bg-zinc-800 transition-colors ${
              showHidden ? 'text-zinc-300' : 'text-zinc-500 hover:text-zinc-300'
            }`}
            title={showHidden ? 'Hide hidden files' : 'Show hidden files'}
          >
            {showHidden ? <Eye className="w-3.5 h-3.5" /> : <EyeOff className="w-3.5 h-3.5" />}
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

      {/* Connection info */}
      <div className="flex items-center gap-1.5 px-3 py-1.5 text-xs border-b border-zinc-800/50">
        <span className="w-1.5 h-1.5 rounded-full bg-green-400 shrink-0" />
        <span className="text-zinc-400 font-medium truncate">{profileName}</span>
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
                setPathInput(remotePath || '');
                setIsEditingPath(false);
              }
            }}
            onBlur={commitPathInput}
            className="flex-1 bg-zinc-900 border border-green-700/50 rounded px-1.5 py-0.5 text-[11px] text-zinc-300 font-mono outline-none focus:border-green-500 min-w-0"
            placeholder="/remote/path/to/folder"
            spellCheck={false}
          />
        ) : (
          <button
            onClick={() => setIsEditingPath(true)}
            className="flex-1 text-left text-[11px] text-zinc-500 hover:text-zinc-300 truncate font-mono transition-colors rounded px-1.5 py-0.5 hover:bg-zinc-800/50 min-w-0"
            title="Click to type a path"
          >
            {remotePath || '~'}
          </button>
        )}
      </div>

      {/* File tree */}
      <div className="flex-1 overflow-y-auto py-1">
        {/* Inline new folder input */}
        {creatingFolder && (
          <div className="flex items-center gap-1 px-2 py-1 mx-1 mb-1 bg-zinc-800/80 rounded border border-green-600/40">
            <FolderPlus className="w-3.5 h-3.5 text-green-400 shrink-0" />
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

        {loading ? (
          <div className="flex flex-col items-center justify-center py-8 gap-2">
            <Loader2 className="w-5 h-5 text-zinc-600 animate-spin" />
            <span className="text-xs text-zinc-600">Connecting to {profileName}...</span>
          </div>
        ) : error ? (
          <div className="px-3 py-4">
            <div className="px-3 py-2 bg-red-900/20 border border-red-800/30 rounded text-xs text-red-400">
              {error}
            </div>
            <button
              onClick={refresh}
              className="mt-2 w-full px-3 py-1.5 bg-zinc-800 hover:bg-zinc-700 rounded text-xs text-zinc-300 transition-colors"
            >
              Retry
            </button>
          </div>
        ) : entries.length === 0 ? (
          <div className="px-4 py-8 text-center text-zinc-600 text-sm">Empty directory</div>
        ) : (
          entries.map((entry) => (
            <RemoteTreeNode key={`${entry.path}-${refreshKey}`} entry={entry} depth={0} profileId={profileId} showHidden={showHidden} onNavigate={navigateTo} isPinned={isPinned(entry.path)} onTogglePin={togglePin} onContextMenu={handleContextMenu} />
          ))
        )}
      </div>
    </div>
  );
}
