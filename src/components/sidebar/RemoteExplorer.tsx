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
} from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { emit } from '@tauri-apps/api/event';
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
}

function RemoteTreeNode({ entry, depth, profileId, showHidden, onNavigate }: RemoteTreeNodeProps) {
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

  const openRemoteFile = async (preview: boolean) => {
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
        openBinaryFile(entry.path, base64Content, binaryInfo.mimeType, binaryInfo.binaryType, preview);
      } else {
        // Fetch as text for code/text files
        const content = await invoke<string>('read_remote_file', {
          profileId,
          path: entry.path,
        });
        openFile(entry.path, content, preview);
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

        {!entry.is_dir && entry.size > 0 && (
          <span className="ml-auto text-[10px] text-zinc-600 opacity-0 group-hover:opacity-100 transition-opacity shrink-0">
            {formatSize(entry.size)}
          </span>
        )}

        {loading && (
          <Loader2 className="ml-auto w-3 h-3 text-zinc-600 animate-spin shrink-0" />
        )}
      </button>

      {entry.is_dir &&
        expanded &&
        children.map((child) => (
          <RemoteTreeNode key={child.path} entry={child} depth={depth + 1} profileId={profileId} showHidden={showHidden} onNavigate={onNavigate} />
        ))}
    </div>
  );
}

// --- Main Remote Explorer View ---

export function RemoteExplorer({ profileId, profileName, terminalId }: RemoteExplorerProps) {
  const [remotePath, setRemotePath] = useState<string>('');
  const [entries, setEntries] = useState<FileEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [showHidden, setShowHidden] = useState(false);
  const [refreshKey, setRefreshKey] = useState(0);

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

  const refresh = () => {
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

  return (
    <div className="flex flex-col h-full">
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
            onClick={refresh}
            className="p-1 rounded hover:bg-zinc-800 text-zinc-500 hover:text-zinc-300"
            title="Refresh"
          >
            <RefreshCw className="w-3.5 h-3.5" />
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
        <button
          onClick={cdToTerminal}
          className="p-0.5 rounded hover:bg-zinc-800 text-zinc-600 hover:text-zinc-300 transition-colors shrink-0"
          title="cd to this directory in terminal"
        >
          <CornerDownRight className="w-3.5 h-3.5" />
        </button>
      </div>

      {/* File tree */}
      <div className="flex-1 overflow-y-auto py-1">
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
            <RemoteTreeNode key={`${entry.path}-${refreshKey}`} entry={entry} depth={0} profileId={profileId} showHidden={showHidden} onNavigate={navigateTo} />
          ))
        )}
      </div>
    </div>
  );
}
