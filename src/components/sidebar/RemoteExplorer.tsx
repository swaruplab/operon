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
  Trash2,
  Pencil,
  MessageSquarePlus,
  Filter,
} from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { emit, listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { useProject } from '../../context/ProjectContext';
import type { FileEntry } from '../../lib/files';
import { listRemoteDirectoryCached, invalidateRemotePath, clearRemoteCache, batchDeleteRemoteFiles } from '../../lib/ssh';
import { RegexAddDialog } from './Sidebar';
import { Copy } from 'lucide-react';

const BINARY_EXTENSIONS: Record<string, { binaryType: 'image' | 'pdf' | 'html' | 'xlsx' | 'pptx' | 'docx'; mimeType: string }> = {
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
  xlsx: { binaryType: 'xlsx', mimeType: 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet' },
  xlsm: { binaryType: 'xlsx', mimeType: 'application/vnd.ms-excel.sheet.macroEnabled.12' },
  xls: { binaryType: 'xlsx', mimeType: 'application/vnd.ms-excel' },
  pptx: { binaryType: 'pptx', mimeType: 'application/vnd.openxmlformats-officedocument.presentationml.presentation' },
  pptm: { binaryType: 'pptx', mimeType: 'application/vnd.ms-powerpoint.presentation.macroEnabled.12' },
  ppt: { binaryType: 'pptx', mimeType: 'application/vnd.ms-powerpoint' },
  docx: { binaryType: 'docx', mimeType: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document' },
  docm: { binaryType: 'docx', mimeType: 'application/vnd.ms-word.document.macroEnabled.12' },
  doc: { binaryType: 'docx', mimeType: 'application/msword' },
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
  isSelected?: boolean;
  selectionMode?: boolean;
  onSelect?: (e: React.MouseEvent, entry: FileEntry) => void;
}

interface ContextMenuState {
  x: number;
  y: number;
  entry: FileEntry;
}

function RemoteTreeNode({ entry, depth, profileId, showHidden, onNavigate, isPinned, onTogglePin, onContextMenu, isSelected, selectionMode, onSelect }: RemoteTreeNodeProps) {
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
      const entries = await listRemoteDirectoryCached(profileId, entry.path, showHidden);
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

  const handleClick = (e: React.MouseEvent) => {
    const modified = e.shiftKey || e.metaKey || e.ctrlKey;
    if (modified || (selectionMode && onSelect)) {
      onSelect?.(e, entry);
      return;
    }
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
      tsx: 'text-blue-600 dark:text-blue-400',
      ts: 'text-blue-600 dark:text-blue-400',
      jsx: 'text-yellow-600 dark:text-yellow-400',
      js: 'text-yellow-600 dark:text-yellow-400',
      rs: 'text-orange-600 dark:text-orange-400',
      py: 'text-green-600 dark:text-green-400',
      json: 'text-yellow-600 dark:text-yellow-400',
      css: 'text-purple-600 dark:text-purple-400',
      html: 'text-red-600 dark:text-red-400',
      md: 'text-secondary',
      toml: 'text-red-600 dark:text-red-400',
      yaml: 'text-pink-600 dark:text-pink-400',
      yml: 'text-pink-600 dark:text-pink-400',
      sh: 'text-green-600 dark:text-green-400',
      bash: 'text-green-600 dark:text-green-400',
      c: 'text-blue-700 dark:text-blue-300',
      cpp: 'text-blue-700 dark:text-blue-300',
      h: 'text-blue-700 dark:text-blue-300',
      java: 'text-red-700 dark:text-red-300',
      go: 'text-cyan-600 dark:text-cyan-400',
      rb: 'text-red-600 dark:text-red-400',
      php: 'text-purple-700 dark:text-purple-300',
      log: 'text-muted',
      txt: 'text-secondary',
      cfg: 'text-secondary',
      conf: 'text-secondary',
    };
    return colorMap[ext || ''] || 'text-secondary';
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
          className={`w-full flex items-center gap-1 h-[26px] px-2 text-[13px] text-secondary hover:bg-hover/80 transition-colors group ${
            isSelected ? 'bg-blue-500/15' : ''
          }`}
          style={{ paddingLeft: `${depth * 12 + 8}px` }}
          onClick={handleClick}
          onDoubleClick={handleDoubleClick}
          onContextMenu={(e) => onContextMenu?.(e, entry)}
        >
          {entry.is_dir ? (
            expanded ? (
              <ChevronDown className="w-3.5 h-3.5 text-muted shrink-0" />
            ) : (
              <ChevronRight className="w-3.5 h-3.5 text-muted shrink-0" />
            )
          ) : (
            <span className="w-3.5 shrink-0" />
          )}

          {entry.is_dir ? (
            expanded ? (
              <FolderOpen className="w-4 h-4 text-blue-600 dark:text-blue-400 shrink-0" />
            ) : (
              <Folder className="w-4 h-4 text-muted shrink-0" />
            )
          ) : (
            <File className={`w-4 h-4 shrink-0 ${getFileColor(entry.extension)}`} />
          )}

          <span className="truncate ml-1">{entry.name}</span>

          {isPinned && !hovered && (
            <Star className="w-3 h-3 text-amber-600 dark:text-amber-400 ml-auto shrink-0 fill-amber-400" />
          )}

          {!entry.is_dir && entry.size > 0 && !isPinned && (
            <span className="ml-auto text-[10px] text-subtle opacity-0 group-hover:opacity-100 transition-opacity shrink-0">
              {formatSize(entry.size)}
            </span>
          )}

          {loading && (
            <Loader2 className="ml-auto w-3 h-3 text-subtle animate-spin shrink-0" />
          )}
        </button>

        {/* Pin button on hover */}
        {hovered && onTogglePin && (
          <button
            onClick={(e) => {
              e.stopPropagation();
              onTogglePin(entry.path, entry.name, entry.is_dir);
            }}
            className={`absolute right-1 top-1/2 -translate-y-1/2 p-0.5 rounded hover:bg-elevated transition-colors ${
              isPinned ? 'text-amber-600 dark:text-amber-400' : 'text-subtle hover:text-amber-700 dark:hover:text-amber-600'
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
          <RemoteTreeNode key={child.path} entry={child} depth={depth + 1} profileId={profileId} showHidden={showHidden} onNavigate={onNavigate} isPinned={onTogglePin ? false : undefined} onTogglePin={onTogglePin} onContextMenu={onContextMenu} isSelected={onSelect ? false : undefined} selectionMode={selectionMode} onSelect={onSelect} />
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

function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / (1024 * 1024)).toFixed(1)} MB`;
  return `${(n / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

export function RemoteExplorer({ profileId, profileName, terminalId }: RemoteExplorerProps) {
  const [remotePath, setRemotePath] = useState<string>('');
  const [entries, setEntries] = useState<FileEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [showHidden, setShowHidden] = useState(false);
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
  // Regex bulk-add dialog (Part C) — open from the context menu for folders
  const [regexDialogRoot, setRegexDialogRoot] = useState<FileEntry | null>(null);

  // Bulk selection state — anchor tracks the last clicked row for shift-range.
  // The flat list used for shift-range is just the visible top-level entries;
  // we don't try to flatten expanded children since the user typically selects
  // siblings at one level anyway.
  const [selectedPaths, setSelectedPaths] = useState<Set<string>>(new Set());
  const [bulkDeleteConfirm, setBulkDeleteConfirm] = useState(false);
  const lastAnchorRef = useRef<string | null>(null);

  const handleSelect = useCallback((e: React.MouseEvent, entry: FileEntry) => {
    setSelectedPaths(prev => {
      const next = new Set(prev);
      if (e.shiftKey && lastAnchorRef.current) {
        const anchor = lastAnchorRef.current;
        const flat = entries.map(x => x.path);
        const i = flat.indexOf(anchor);
        const j = flat.indexOf(entry.path);
        if (i >= 0 && j >= 0) {
          const [lo, hi] = i < j ? [i, j] : [j, i];
          for (let k = lo; k <= hi; k++) next.add(flat[k]);
          return next;
        }
      }
      if (e.metaKey || e.ctrlKey) {
        if (next.has(entry.path)) next.delete(entry.path);
        else next.add(entry.path);
      } else {
        next.clear();
        next.add(entry.path);
      }
      lastAnchorRef.current = entry.path;
      return next;
    });
  }, [entries]);

  const clearSelection = useCallback(() => {
    setSelectedPaths(new Set());
    lastAnchorRef.current = null;
  }, []);

  const copySelectedPaths = useCallback(() => {
    const paths = Array.from(selectedPaths).join('\n');
    navigator.clipboard.writeText(paths).catch(() => {});
  }, [selectedPaths]);

  const addToChat = useCallback((entry: FileEntry) => {
    window.dispatchEvent(new CustomEvent('chat-add-context', {
      detail: {
        kind: 'file',
        name: entry.name,
        path: entry.path,
        isDir: entry.is_dir,
        isRemote: true,
      },
    }));
  }, []);

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
      invalidateRemotePath(profileId, remotePath);
      setCreatingFolder(false);
      setNewFolderName('');
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
        const items = await listRemoteDirectoryCached(profileId, path, showHidden);
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

  // On mount, open the server's configured Working Directory (server_config
  // work_dir, with $USER etc. expanded) if set, otherwise the remote $HOME.
  // get_remote_initial_dir handles the work_dir-or-home decision + expansion;
  // because the session CWD and remote-search root both derive from the Remote
  // Explorer's path, opening here directs all of them to the working directory.
  useEffect(() => {
    if (remotePath) {
      loadDir(remotePath);
    } else {
      invoke<string>('get_remote_initial_dir', { profileId })
        .then((dir) => {
          loadDir(dir);
        })
        .catch((err) => {
          console.error('Failed to get remote initial dir:', err);
          loadDir('/');
        });
    }
  }, [profileId, loadDir]); // loadDir already depends on showHidden

  const refresh = async () => {
    try { await clearRemoteCache(); } catch { /* ignore */ }
    if (remotePath) {
      loadDir(remotePath);
    }
  };

  const cdToTerminalPath = (path: string) => {
    if (!path || !terminalId) return;
    const encoded = Array.from(new TextEncoder().encode(`cd '${path.replace(/'/g, "'\\''")}'\n`));
    invoke('write_terminal', {
      terminalId,
      data: encoded,
    }).catch((err) => console.error('Failed to cd in terminal:', err));
  };

  const cdToTerminal = () => {
    if (!remotePath) return;
    cdToTerminalPath(remotePath);
  };

  // Suppress auto-cd when navigation was triggered by terminal CWD sync
  const syncedFromTerminal = useRef(false);

  // Auto-cd remote terminal when navigating in the sidebar
  const prevRemotePath = useRef(remotePath);
  useEffect(() => {
    if (remotePath && remotePath !== prevRemotePath.current) {
      if (!syncedFromTerminal.current) {
        cdToTerminalPath(remotePath);
      }
      syncedFromTerminal.current = false;
      prevRemotePath.current = remotePath;
    }
  }, [remotePath, terminalId]);

  // Listen for remote terminal CWD changes → update explorer
  const remotePathRef = useRef(remotePath);
  remotePathRef.current = remotePath;
  useEffect(() => {
    const unlisten = listen<{ terminalId: string; cwd: string }>('remote-terminal-cwd-changed', (event) => {
      const { cwd } = event.payload;
      if (cwd && cwd !== remotePathRef.current) {
        syncedFromTerminal.current = true;
        navigateTo(cwd);
      }
    });
    return () => { unlisten.then((u) => u()); };
  }, []);

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

  const [deleteConfirm, setDeleteConfirm] = useState<FileEntry | null>(null);
  const [renaming, setRenaming] = useState<FileEntry | null>(null);
  const [renameInput, setRenameInput] = useState('');
  const renameRef = useRef<HTMLInputElement>(null);

  const handleRenameRemote = async () => {
    if (!renaming || !renameInput.trim()) {
      setRenaming(null);
      return;
    }
    const dir = renaming.path.replace(/\/[^/]+$/, '');
    const newPath = `${dir}/${renameInput.trim()}`;
    try {
      await invoke('rename_remote_path', { profileId, oldPath: renaming.path, newPath });
      invalidateRemotePath(profileId, renaming.path);
      invalidateRemotePath(profileId, newPath);
      setRenaming(null);
      if (remotePath) {
        const items = await listRemoteDirectoryCached(profileId, remotePath, showHidden);
        setEntries(items);
      }
    } catch (err) {
      console.error('Failed to rename remote path:', err);
      setRenaming(null);
    }
  };

  const handleDeleteRemoteFile = async (entry: FileEntry) => {
    try {
      await invoke('delete_remote_file', { profileId, path: entry.path });
      invalidateRemotePath(profileId, entry.path);
      setDeleteConfirm(null);
      // Refresh the current directory
      const items = await listRemoteDirectoryCached(profileId, remotePath, showHidden);
      setEntries(items);
    } catch (err) {
      console.error('Failed to delete remote file:', err);
      setDeleteConfirm(null);
    }
  };

  const downloadToLocal = async (entry: FileEntry) => {
    setContextMenu(null);

    const homeDir = await invoke<string>('get_home_dir');
    const downloadsDir = `${homeDir}/Downloads`;
    const localDest = `${downloadsDir}/${entry.name}`;

    setTransfer({
      completed: 0,
      total: 0,
      current_file: entry.name,
      errors: 0,
      status: 'downloading',
    });

    try {
      const cmd = entry.is_dir
        ? 'sftp_dir_download_with_progress'
        : 'sftp_download_with_progress';
      await invoke(cmd, {
        profileId,
        remotePath: entry.path,
        localPath: localDest,
      });
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

  const performBulkDelete = async () => {
    const paths = Array.from(selectedPaths);
    if (paths.length === 0) return;
    try {
      const succeeded = await batchDeleteRemoteFiles(profileId, paths);
      for (const p of paths) invalidateRemotePath(profileId, p);
      clearSelection();
      setBulkDeleteConfirm(false);
      if (remotePath) {
        const items = await listRemoteDirectoryCached(profileId, remotePath, showHidden);
        setEntries(items);
      }
      setTransfer({
        completed: succeeded,
        total: paths.length,
        current_file: '',
        errors: paths.length - succeeded,
        status: succeeded === paths.length ? 'done' : 'error',
        message: succeeded === paths.length
          ? `${succeeded} item${succeeded === 1 ? '' : 's'} deleted`
          : `${succeeded}/${paths.length} deleted (${paths.length - succeeded} failed)`,
      });
    } catch (err) {
      setBulkDeleteConfirm(false);
      setTransfer({
        completed: 0,
        total: paths.length,
        current_file: '',
        errors: paths.length,
        status: 'error',
        message: `Delete failed: ${err}`,
      });
    }
  };

  const downloadSelectedToLocal = async () => {
    const paths = Array.from(selectedPaths);
    if (paths.length === 0) return;
    const homeDir = await invoke<string>('get_home_dir');
    const downloadsDir = `${homeDir}/Downloads`;
    let done = 0;
    let errors = 0;
    setTransfer({
      completed: 0,
      total: paths.length,
      current_file: paths[0].split('/').pop() || '',
      errors: 0,
      status: 'downloading',
    });
    for (const p of paths) {
      const name = p.split('/').pop() || 'file';
      const localDest = `${downloadsDir}/${name}`;
      // Look up the entry to decide file vs dir; default to file.
      const ent = entries.find(e => e.path === p);
      const cmd = ent?.is_dir ? 'sftp_dir_download_with_progress' : 'sftp_download_with_progress';
      try {
        await invoke(cmd, { profileId, remotePath: p, localPath: localDest });
        done += 1;
      } catch {
        errors += 1;
      }
      setTransfer(prev => prev ? { ...prev, completed: done, errors, current_file: name } : prev);
    }
    setTransfer({
      completed: done,
      total: paths.length,
      current_file: '',
      errors,
      status: errors === 0 ? 'done' : 'error',
      message: errors === 0
        ? `Downloaded ${done} item${done === 1 ? '' : 's'} to ~/Downloads`
        : `${done}/${paths.length} downloaded (${errors} failed)`,
    });
    clearSelection();
  };

  return (
    <div
      ref={containerRef}
      className="flex flex-col h-full relative"
    >
      {/* Drag overlay */}
      {isDragOver && (
        <div className="absolute inset-0 z-50 bg-blue-500/10 border-2 border-dashed border-blue-400 rounded-lg flex flex-col items-center justify-center pointer-events-none">
          <Upload className="w-8 h-8 text-blue-600 dark:text-blue-400 mb-2" />
          <p className="text-sm font-medium text-blue-700 dark:text-blue-300">Drop files to upload</p>
          <p className="text-xs text-blue-600 dark:text-blue-400/70 mt-1">
            to {remotePath || '~'}
          </p>
        </div>
      )}

      {/* Bulk-selection action bar — takes the toast slot when ≥1 row is selected. */}
      {selectedPaths.size > 0 && (
        <div className="absolute bottom-3 left-3 right-3 z-50 px-3 py-2 rounded-lg text-xs bg-surface/95 text-secondary border border-border-strong/60 shadow-lg">
          {bulkDeleteConfirm ? (
            <div className="flex items-center gap-2">
              <span className="text-red-700 dark:text-red-300 flex-1 truncate">
                Delete {selectedPaths.size} item{selectedPaths.size === 1 ? '' : 's'}?
              </span>
              <button
                onClick={performBulkDelete}
                className="px-2 py-0.5 bg-red-600 hover:bg-red-500 text-white text-[10px] rounded transition-colors"
              >
                Delete
              </button>
              <button
                onClick={() => setBulkDeleteConfirm(false)}
                className="px-2 py-0.5 bg-elevated hover:bg-elevated text-secondary text-[10px] rounded transition-colors"
              >
                Cancel
              </button>
            </div>
          ) : (
            <div className="flex items-center gap-2">
              <span className="font-medium text-primary shrink-0">
                {selectedPaths.size} selected
              </span>
              <div className="flex-1" />
              <button
                onClick={() => setBulkDeleteConfirm(true)}
                className="flex items-center gap-1 px-2 py-0.5 text-[11px] rounded bg-red-600/90 hover:bg-red-600 text-white transition-colors"
                title="Delete selected"
              >
                <Trash2 className="w-3 h-3 pointer-events-none" />
                Delete
              </button>
              <button
                onClick={copySelectedPaths}
                className="flex items-center gap-1 px-2 py-0.5 text-[11px] rounded bg-elevated hover:bg-hover text-secondary transition-colors"
                title="Copy remote paths"
              >
                <Copy className="w-3 h-3 pointer-events-none" />
                Copy Paths
              </button>
              <button
                onClick={downloadSelectedToLocal}
                className="flex items-center gap-1 px-2 py-0.5 text-[11px] rounded bg-blue-600/90 hover:bg-blue-600 text-white transition-colors"
                title="Download selected to ~/Downloads"
              >
                <Download className="w-3 h-3 pointer-events-none" />
                Download
              </button>
              <button
                onClick={clearSelection}
                className="p-0.5 text-muted hover:text-secondary transition-colors"
                title="Clear selection"
              >
                <X className="w-3 h-3" />
              </button>
            </div>
          )}
        </div>
      )}

      {/* Transfer progress toast */}
      {transfer && selectedPaths.size === 0 && (
        <div className={`absolute bottom-3 left-3 right-3 z-50 px-3 py-2.5 rounded-lg text-xs border shadow-lg ${
          transfer.status === 'done'
            ? 'bg-green-900/80 text-green-700 dark:text-green-300 border-green-800/60'
            : transfer.status === 'error'
              ? 'bg-red-900/80 text-red-700 dark:text-red-300 border-red-800/60'
              : 'bg-surface/95 text-secondary border-border-strong/60'
        }`}>
          <div className="flex items-center gap-2">
            {transfer.status === 'uploading' && <Upload className="w-3.5 h-3.5 text-blue-600 dark:text-blue-400 animate-pulse shrink-0" />}
            {transfer.status === 'downloading' && <Download className="w-3.5 h-3.5 text-blue-600 dark:text-blue-400 animate-pulse shrink-0" />}
            {transfer.status === 'done' && <CheckCircle2 className="w-3.5 h-3.5 text-green-600 dark:text-green-400 shrink-0" />}
            {transfer.status === 'error' && <XCircle className="w-3.5 h-3.5 text-red-600 dark:text-red-400 shrink-0" />}
            <div className="flex-1 min-w-0">
              {transfer.message ? (
                <p className="truncate">{transfer.message}</p>
              ) : (
                <p className="truncate">
                  {transfer.status === 'uploading' ? 'Uploading' : 'Downloading'} {transfer.current_file}
                  {transfer.status === 'downloading' && transfer.total > 0 && (
                    ` (${formatBytes(transfer.completed)} / ${formatBytes(transfer.total)})`
                  )}
                  {transfer.status === 'uploading' && transfer.total > 1 && (
                    ` (${transfer.completed}/${transfer.total})`
                  )}
                </p>
              )}
            </div>
            <button
              onClick={() => setTransfer(null)}
              className="p-0.5 text-muted hover:text-secondary transition-colors shrink-0"
            >
              <X className="w-3 h-3" />
            </button>
          </div>
          {/* Progress bar — bytes for downloads, file counts for uploads */}
          {(transfer.status === 'uploading' || transfer.status === 'downloading') && transfer.total > 0 && (
            <div className="mt-1.5 h-1 bg-elevated rounded-full overflow-hidden">
              <div
                className="h-full bg-blue-500 rounded-full transition-all duration-300"
                style={{ width: `${Math.min(100, (transfer.completed / transfer.total) * 100)}%` }}
              />
            </div>
          )}
        </div>
      )}

      {/* Context menu */}
      {contextMenu && (
        <div
          className="fixed z-[100] bg-surface border border-border-strong rounded-lg shadow-xl py-1 min-w-[180px]"
          style={{
            left: Math.min(contextMenu.x, window.innerWidth - 200),
            top: Math.min(contextMenu.y, window.innerHeight - 220),
          }}
        >
          <button
            onClick={() => {
              addToChat(contextMenu.entry);
              setContextMenu(null);
            }}
            className="w-full flex items-center gap-2 px-3 py-1.5 text-[12px] text-blue-700 dark:text-blue-300 hover:bg-elevated transition-colors text-left"
          >
            <MessageSquarePlus className="w-3.5 h-3.5 text-blue-600 dark:text-blue-400 pointer-events-none" />
            Add {contextMenu.entry.is_dir ? 'folder' : 'file'} to chat
          </button>
          {contextMenu.entry.is_dir && (
            <button
              onClick={() => {
                setRegexDialogRoot(contextMenu.entry);
                setContextMenu(null);
              }}
              className="w-full flex items-center gap-2 px-3 py-1.5 text-[12px] text-purple-700 dark:text-purple-300 hover:bg-elevated transition-colors text-left"
            >
              <Filter className="w-3.5 h-3.5 text-purple-600 dark:text-purple-400 pointer-events-none" />
              Add matching files to chat…
            </button>
          )}
          <div className="border-t border-border-strong my-1" />
          <button
            onClick={() => {
              setRenaming(contextMenu.entry);
              setRenameInput(contextMenu.entry.name);
              setContextMenu(null);
              setTimeout(() => renameRef.current?.select(), 50);
            }}
            className="w-full flex items-center gap-2 px-3 py-1.5 text-[12px] text-secondary hover:bg-elevated transition-colors text-left"
          >
            <Pencil className="w-3.5 h-3.5 text-muted pointer-events-none" />
            Rename
          </button>
          <button
            onClick={() => downloadToLocal(contextMenu.entry)}
            className="w-full flex items-center gap-2 px-3 py-1.5 text-[12px] text-secondary hover:bg-elevated transition-colors text-left"
          >
            <Download className="w-3.5 h-3.5 text-blue-600 dark:text-blue-400" />
            Download to local
          </button>
          <button
            onClick={() => {
              navigator.clipboard.writeText(contextMenu.entry.path);
              setContextMenu(null);
            }}
            className="w-full flex items-center gap-2 px-3 py-1.5 text-[12px] text-secondary hover:bg-elevated transition-colors text-left"
          >
            <File className="w-3.5 h-3.5 text-muted" />
            Copy remote path
          </button>
          <div className="border-t border-border-strong my-1" />
          <button
            onClick={() => {
              setDeleteConfirm(contextMenu.entry);
              setContextMenu(null);
            }}
            className="w-full flex items-center gap-2 px-3 py-1.5 text-[12px] text-red-600 dark:text-red-400 hover:bg-elevated transition-colors text-left"
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
          isRemote={true}
          profileId={profileId}
          onClose={() => setRegexDialogRoot(null)}
        />
      )}

      {/* Rename inline input */}
      {renaming && (
        <div className="absolute top-0 left-0 right-0 z-50 mx-2 mt-2 px-3 py-2.5 bg-surface border border-border-strong rounded-lg shadow-lg">
          <p className="text-[11px] text-secondary mb-1.5">
            Rename <span className="font-medium text-primary">{renaming.name}</span>
          </p>
          <input
            ref={renameRef}
            value={renameInput}
            onChange={(e) => setRenameInput(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') handleRenameRemote();
              if (e.key === 'Escape') setRenaming(null);
            }}
            className="w-full bg-panel border border-border-strong rounded px-2 py-1 text-xs text-primary outline-none focus:border-blue-500"
            autoFocus
          />
          <div className="flex items-center gap-2 mt-2">
            <button onClick={handleRenameRemote} className="px-2 py-0.5 text-[11px] rounded bg-blue-600 hover:bg-blue-500 text-white">Rename</button>
            <button onClick={() => setRenaming(null)} className="px-2 py-0.5 text-[11px] rounded bg-elevated hover:bg-elevated text-secondary">Cancel</button>
          </div>
        </div>
      )}

      {/* Delete confirmation */}
      {deleteConfirm && (
        <div className="absolute top-0 left-0 right-0 z-50 mx-2 mt-2 px-3 py-2.5 bg-red-950/90 border border-red-800/60 rounded-lg shadow-lg">
          <p className="text-[11px] text-red-200 dark:text-red-300 mb-2">
            Delete <span className="font-medium text-red-100 dark:text-red-200">{deleteConfirm.name}</span>?
          </p>
          <div className="flex items-center gap-2">
            <button
              onClick={() => handleDeleteRemoteFile(deleteConfirm)}
              className="px-2.5 py-1 bg-red-600 hover:bg-red-500 text-white text-[10px] rounded transition-colors"
            >
              Delete
            </button>
            <button
              onClick={() => setDeleteConfirm(null)}
              className="px-2.5 py-1 bg-elevated hover:bg-elevated text-secondary text-[10px] rounded transition-colors"
            >
              Cancel
            </button>
          </div>
        </div>
      )}

      {/* Header */}
      <div className="flex items-center justify-between px-3 py-2 border-b border-border-default">
        <div className="flex items-center gap-1.5">
          <Server className="w-3.5 h-3.5 text-green-600 dark:text-green-400" />
          <span className="text-[11px] font-semibold text-muted uppercase tracking-wider">
            Remote
          </span>
        </div>
        <div className="flex items-center gap-0.5">
          <button
            onClick={navigateUp}
            className="p-1 rounded hover:bg-hover text-muted hover:text-secondary text-xs"
            title="Go Up"
          >
            ..
          </button>
          <button
            onClick={() => setShowHidden((v) => !v)}
            className={`p-1 rounded hover:bg-hover transition-colors ${
              showHidden ? 'text-secondary' : 'text-muted hover:text-secondary'
            }`}
            title={showHidden ? 'Hide hidden files' : 'Show hidden files'}
          >
            {showHidden ? <Eye className="w-3.5 h-3.5" /> : <EyeOff className="w-3.5 h-3.5" />}
          </button>
          <button
            onClick={() => { setCreatingFolder(true); setNewFolderName(''); }}
            className="p-1 rounded hover:bg-hover text-muted hover:text-secondary"
            title="New Folder"
          >
            <FolderPlus className="w-3.5 h-3.5" />
          </button>
          <button
            onClick={refresh}
            className="p-1 rounded hover:bg-hover text-muted hover:text-secondary"
            title="Refresh"
          >
            <RefreshCw className="w-3.5 h-3.5" />
          </button>
          <button
            onClick={cdToTerminal}
            className="p-1 rounded hover:bg-hover text-muted hover:text-secondary transition-colors shrink-0"
            title="cd to this directory in terminal"
          >
            <CornerDownRight className="w-3.5 h-3.5" />
          </button>
        </div>
      </div>

      {/* Connection info */}
      <div className="flex items-center gap-1.5 px-3 py-1.5 text-xs border-b border-border-default/50">
        <span className="w-1.5 h-1.5 rounded-full bg-green-400 shrink-0" />
        <span className="text-secondary font-medium truncate">{profileName}</span>
      </div>

      {/* Go-to-folder path bar — click to type a path, press Enter to navigate */}
      <div className="flex items-center gap-1 px-2 py-1 border-b border-border-default/30">
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
            className="flex-1 bg-panel border border-green-700/50 rounded px-1.5 py-0.5 text-[11px] text-secondary font-mono outline-none focus:border-green-500 min-w-0"
            placeholder="/remote/path/to/folder"
            spellCheck={false}
          />
        ) : (
          <button
            onClick={() => setIsEditingPath(true)}
            className="flex-1 text-left text-[11px] text-muted hover:text-secondary truncate font-mono transition-colors rounded px-1.5 py-0.5 hover:bg-hover/50 min-w-0"
            title="Click to type a path"
          >
            {remotePath || '~'}
          </button>
        )}
      </div>

      {/* File tree */}
      <div className="flex-1 min-h-0 overflow-y-auto py-1 pb-16">
        {/* Inline new folder input */}
        {creatingFolder && (
          <div className="flex items-center gap-1 px-2 py-1 mx-1 mb-1 bg-surface/80 rounded border border-green-600/40">
            <FolderPlus className="w-3.5 h-3.5 text-green-600 dark:text-green-400 shrink-0" />
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
              className="flex-1 bg-transparent text-[13px] text-primary outline-none placeholder:text-subtle min-w-0"
              placeholder="folder name"
              spellCheck={false}
            />
          </div>
        )}

        {/* Pinned/Favorites section */}
        {pinnedItems.length > 0 && (
          <div className="mb-2 border-b border-border-strong/40 pb-2">
            <div className="flex items-center gap-1.5 px-3 py-1 text-[10px] text-amber-600 dark:text-amber-400/70 font-medium uppercase tracking-wider">
              <Star className="w-3 h-3 fill-amber-400/50" />
              Favorites
            </div>
            {pinnedItems.map((item) => (
              <div key={item.path} className="relative group">
                <button
                  className="w-full flex items-center gap-1.5 h-[26px] px-3 text-[13px] text-secondary hover:bg-hover/80 transition-colors"
                  onClick={() => openPinnedItem(item)}
                  title={item.path}
                >
                  {item.isDir ? (
                    <Folder className="w-4 h-4 text-amber-600 dark:text-amber-400/70 shrink-0" />
                  ) : (
                    <File className="w-4 h-4 text-amber-600 dark:text-amber-400/70 shrink-0" />
                  )}
                  <span className="truncate">{item.name}</span>
                </button>
                <button
                  onClick={(e) => {
                    e.stopPropagation();
                    togglePin(item.path, item.name, item.isDir);
                  }}
                  className="absolute right-1 top-1/2 -translate-y-1/2 p-0.5 rounded text-subtle hover:text-red-700 dark:hover:text-red-600 hover:bg-elevated transition-colors opacity-0 group-hover:opacity-100"
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
            <Loader2 className="w-5 h-5 text-subtle animate-spin" />
            <span className="text-xs text-subtle">Connecting to {profileName}...</span>
          </div>
        ) : error ? (
          <div className="px-3 py-4">
            <div className="px-3 py-2 bg-red-900/20 border border-red-800/30 rounded text-xs text-red-600 dark:text-red-400">
              {error}
            </div>
            <button
              onClick={refresh}
              className="mt-2 w-full px-3 py-1.5 bg-surface hover:bg-elevated rounded text-xs text-secondary transition-colors"
            >
              Retry
            </button>
          </div>
        ) : entries.length === 0 ? (
          <div className="px-4 py-8 text-center text-subtle text-sm">Empty directory</div>
        ) : (
          entries.map((entry) => (
            <RemoteTreeNode key={entry.path} entry={entry} depth={0} profileId={profileId} showHidden={showHidden} onNavigate={navigateTo} isPinned={isPinned(entry.path)} onTogglePin={togglePin} onContextMenu={handleContextMenu} isSelected={selectedPaths.has(entry.path)} selectionMode={selectedPaths.size > 0} onSelect={handleSelect} />
          ))
        )}
      </div>
    </div>
  );
}
