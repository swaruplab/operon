import { useState, useEffect } from 'react';
import {
  Box,
  Play,
  Terminal,
  Download,
  AlertCircle,
  Loader2,
  RefreshCw,
  HardDrive,
} from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import type { ToolExtension } from '../../types/toolExtension';

interface SingularityImage {
  name: string;
  path: string;
  size: string;
  modified: string;
}

interface SingularityInstance {
  name: string;
  pid: string;
  image: string;
}

type SingularityTab = 'images' | 'instances';

function SingularityPanelContent() {
  const [activeTab, setActiveTab] = useState<SingularityTab>('images');
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [singularityInstalled, setSingularityInstalled] = useState(false);
  const [images, setImages] = useState<SingularityImage[]>([]);
  const [instances, setInstances] = useState<SingularityInstance[]>([]);
  const [searchDir, setSearchDir] = useState<string>('$HOME/.sif');
  const [actionLoading, setActionLoading] = useState<Set<string>>(new Set());
  const [pullUri, setPullUri] = useState('');
  const [showPullInput, setShowPullInput] = useState(false);

  // Check if Singularity/Apptainer is installed on component mount
  useEffect(() => {
    checkSingularity();
  }, []);

  const checkSingularity = async () => {
    try {
      setLoading(true);
      // Try to list instances to check if singularity/apptainer is available
      await invoke<SingularityInstance[]>('singularity_list_instances');
      setSingularityInstalled(true);
      setError(null);
      // Also load initial data
      loadInstances();
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : String(err);
      if (errorMsg.includes('not available')) {
        setSingularityInstalled(false);
      } else {
        setError(errorMsg);
      }
    } finally {
      setLoading(false);
    }
  };

  const loadImages = async () => {
    try {
      setLoading(true);
      setError(null);
      // Expand $HOME in the search directory
      const expandedDir = searchDir.replace('$HOME', '~');
      const data = await invoke<SingularityImage[]>('singularity_list_images', {
        search_dir: expandedDir,
      });
      setImages(data);
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : String(err);
      setError(errorMsg);
    } finally {
      setLoading(false);
    }
  };

  const loadInstances = async () => {
    try {
      setLoading(true);
      setError(null);
      const data = await invoke<SingularityInstance[]>('singularity_list_instances');
      setInstances(data);
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : String(err);
      setError(errorMsg);
    } finally {
      setLoading(false);
    }
  };

  // Load data when tab changes
  useEffect(() => {
    if (!singularityInstalled) return;
    if (activeTab === 'images') loadImages();
    if (activeTab === 'instances') loadInstances();
  }, [activeTab, singularityInstalled]);

  const performImageAction = async (imagePath: string, action: string) => {
    try {
      const key = `${imagePath}-${action}`;
      setActionLoading((prev) => new Set(prev).add(key));
      await invoke<string>('singularity_action', {
        action,
        image_path: imagePath,
        instance_name: undefined,
      });
      setError(null);
      // Reload instances after action
      loadInstances();
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : String(err);
      setError(errorMsg);
    } finally {
      const key = `${imagePath}-${action}`;
      setActionLoading((prev) => {
        const next = new Set(prev);
        next.delete(key);
        return next;
      });
    }
  };

  const handlePullImage = async () => {
    if (!pullUri.trim()) {
      setError('Please enter a Singularity image URI (e.g., docker://ubuntu:latest)');
      return;
    }

    try {
      const key = `pull-${pullUri}`;
      setActionLoading((prev) => new Set(prev).add(key));
      await invoke<string>('singularity_action', {
        action: 'pull',
        image_path: pullUri,
        instance_name: undefined,
      });
      setError(null);
      setPullUri('');
      setShowPullInput(false);
      loadImages();
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : String(err);
      setError(errorMsg);
    } finally {
      const key = `pull-${pullUri}`;
      setActionLoading((prev) => {
        const next = new Set(prev);
        next.delete(key);
        return next;
      });
    }
  };

  const performInstanceAction = async (instanceName: string, action: string) => {
    try {
      const key = `${instanceName}-${action}`;
      setActionLoading((prev) => new Set(prev).add(key));
      await invoke<string>('singularity_action', {
        action,
        image_path: '',
        instance_name: instanceName,
      });
      setError(null);
      loadInstances();
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : String(err);
      setError(errorMsg);
    } finally {
      const key = `${instanceName}-${action}`;
      setActionLoading((prev) => {
        const next = new Set(prev);
        next.delete(key);
        return next;
      });
    }
  };

  if (!singularityInstalled) {
    return (
      <div className="flex flex-col h-full">
        <div className="flex items-center justify-between px-3 py-2 border-b border-zinc-800">
          <div className="flex items-center gap-1.5 text-xs text-zinc-400">
            <HardDrive className="w-4 h-4" />
            <span className="font-medium">Singularity/Apptainer</span>
          </div>
        </div>
        <div className="flex-1 flex flex-col items-center justify-center gap-3 px-4 text-center">
          <AlertCircle className="w-8 h-8 text-zinc-600" />
          <div>
            <p className="text-sm font-medium text-zinc-300 mb-1">Singularity/Apptainer Not Installed</p>
            <p className="text-xs text-zinc-500 mb-3">
              Install Singularity or Apptainer to use this HPC container tool.
            </p>
            <p className="text-xs text-zinc-500 mb-3">
              On HPC clusters, load the module:
            </p>
            <code className="block text-xs bg-zinc-800/50 p-2 rounded text-zinc-400 mb-3 font-mono">
              module load singularity
            </code>
            <p className="text-xs text-zinc-600 mb-3">Or install locally:</p>
            <code className="block text-xs bg-zinc-800/50 p-2 rounded text-zinc-400 mb-3 font-mono">
              brew install apptainer
            </code>
            <button
              onClick={checkSingularity}
              className="px-3 py-1.5 bg-zinc-800 hover:bg-zinc-700 text-xs text-zinc-300 rounded transition-colors"
            >
              Retry
            </button>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full">
      <div className="flex items-center justify-between px-3 py-2 border-b border-zinc-800">
        <div className="flex items-center gap-1.5 text-xs text-zinc-400">
          <HardDrive className="w-4 h-4 text-purple-400" />
          <span className="font-medium">Singularity/Apptainer</span>
        </div>
        <button
          onClick={() => {
            if (activeTab === 'images') loadImages();
            if (activeTab === 'instances') loadInstances();
          }}
          className="p-1 rounded hover:bg-zinc-800 text-zinc-500 hover:text-zinc-300 transition-colors"
          title="Refresh"
        >
          <RefreshCw className={`w-3.5 h-3.5 ${loading ? 'animate-spin' : ''}`} />
        </button>
      </div>

      {error && (
        <div className="mx-2 mt-2 p-2 bg-red-900/20 border border-red-700/30 rounded text-xs text-red-300 flex items-start gap-2">
          <AlertCircle className="w-3.5 h-3.5 shrink-0 mt-0.5" />
          <span>{error}</span>
        </div>
      )}

      {/* Tab Buttons */}
      <div className="flex border-b border-zinc-800 px-2 py-1 gap-1">
        <button
          onClick={() => setActiveTab('images')}
          className={`flex items-center gap-1 px-2 py-1 text-xs font-medium rounded transition-colors ${
            activeTab === 'images'
              ? 'bg-purple-900/40 text-purple-300'
              : 'text-zinc-500 hover:text-zinc-400'
          }`}
        >
          <Box className="w-3.5 h-3.5" />
          Images
        </button>
        <button
          onClick={() => setActiveTab('instances')}
          className={`flex items-center gap-1 px-2 py-1 text-xs font-medium rounded transition-colors ${
            activeTab === 'instances'
              ? 'bg-purple-900/40 text-purple-300'
              : 'text-zinc-500 hover:text-zinc-400'
          }`}
        >
          <Terminal className="w-3.5 h-3.5" />
          Instances
        </button>
      </div>

      {/* Content Area */}
      <div className="flex-1 overflow-y-auto">
        {loading && (
          <div className="flex items-center justify-center h-32 text-zinc-600">
            <Loader2 className="w-4 h-4 animate-spin mr-2" />
            Loading...
          </div>
        )}

        {/* Images Tab */}
        {activeTab === 'images' && !loading && (
          <div className="p-2 space-y-2">
            {/* Search directory and pull controls */}
            <div className="bg-zinc-800 rounded p-2 space-y-1">
              <label className="block text-xs font-medium text-zinc-400">Search Directory:</label>
              <input
                type="text"
                value={searchDir}
                onChange={(e) => setSearchDir(e.target.value)}
                onKeyDown={(e) => e.key === 'Enter' && loadImages()}
                placeholder="$HOME/.sif"
                className="w-full px-2 py-1 bg-zinc-900 border border-zinc-700 rounded text-xs text-zinc-300 placeholder:text-zinc-600"
              />
              <button
                onClick={loadImages}
                className="w-full px-2 py-1 bg-zinc-700 hover:bg-zinc-600 text-xs text-zinc-300 rounded transition-colors"
              >
                Search
              </button>
            </div>

            {/* Pull new image */}
            {!showPullInput ? (
              <button
                onClick={() => setShowPullInput(true)}
                className="w-full flex items-center justify-center gap-2 px-2 py-2 bg-purple-900/30 hover:bg-purple-900/50 text-xs text-purple-300 rounded transition-colors"
              >
                <Download className="w-3.5 h-3.5" />
                Pull Image
              </button>
            ) : (
              <div className="bg-zinc-800 rounded p-2 space-y-1">
                <input
                  type="text"
                  value={pullUri}
                  onChange={(e) => setPullUri(e.target.value)}
                  placeholder="docker://ubuntu:latest"
                  className="w-full px-2 py-1 bg-zinc-900 border border-zinc-700 rounded text-xs text-zinc-300 placeholder:text-zinc-600"
                />
                <div className="flex gap-1">
                  <button
                    onClick={handlePullImage}
                    disabled={actionLoading.has(`pull-${pullUri}`)}
                    className="flex-1 px-2 py-1 bg-purple-900/40 hover:bg-purple-900/60 text-xs text-purple-300 rounded transition-colors disabled:opacity-50"
                  >
                    {actionLoading.has(`pull-${pullUri}`) ? (
                      <Loader2 className="w-3 h-3 animate-spin mx-auto" />
                    ) : (
                      'Pull'
                    )}
                  </button>
                  <button
                    onClick={() => {
                      setShowPullInput(false);
                      setPullUri('');
                    }}
                    className="flex-1 px-2 py-1 bg-zinc-700 hover:bg-zinc-600 text-xs text-zinc-300 rounded transition-colors"
                  >
                    Cancel
                  </button>
                </div>
              </div>
            )}

            {/* Images list */}
            {images.length === 0 ? (
              <div className="text-center py-8 text-xs text-zinc-600">No .sif images found</div>
            ) : (
              images.map((image) => (
                <div key={image.path} className="bg-zinc-800 rounded p-2 space-y-1">
                  <div className="flex items-start justify-between gap-2">
                    <div className="flex-1 min-w-0">
                      <p className="text-xs font-medium text-zinc-200 truncate">{image.name}</p>
                      <p className="text-[10px] text-zinc-500">
                        {image.size} · {image.modified}
                      </p>
                      <p className="text-[10px] text-zinc-600 truncate font-mono">{image.path}</p>
                    </div>
                  </div>
                  <div className="flex gap-1 pt-1">
                    <button
                      onClick={() => performImageAction(image.path, 'shell')}
                      disabled={actionLoading.has(`${image.path}-shell`)}
                      className="flex-1 flex items-center justify-center gap-1 px-2 py-1 bg-zinc-700 hover:bg-zinc-600 text-xs text-zinc-300 rounded transition-colors disabled:opacity-50"
                      title="Shell"
                    >
                      {actionLoading.has(`${image.path}-shell`) ? (
                        <Loader2 className="w-3 h-3 animate-spin" />
                      ) : (
                        <Terminal className="w-3 h-3" />
                      )}
                      Shell
                    </button>
                    <button
                      onClick={() => performImageAction(image.path, 'run')}
                      disabled={actionLoading.has(`${image.path}-run`)}
                      className="flex-1 flex items-center justify-center gap-1 px-2 py-1 bg-green-900/40 hover:bg-green-900/60 text-xs text-green-300 rounded transition-colors disabled:opacity-50"
                      title="Run"
                    >
                      {actionLoading.has(`${image.path}-run`) ? (
                        <Loader2 className="w-3 h-3 animate-spin" />
                      ) : (
                        <Play className="w-3 h-3" />
                      )}
                      Run
                    </button>
                  </div>
                </div>
              ))
            )}
          </div>
        )}

        {/* Instances Tab */}
        {activeTab === 'instances' && !loading && (
          <div className="p-2 space-y-1">
            {instances.length === 0 ? (
              <div className="text-center py-8 text-xs text-zinc-600">No running instances</div>
            ) : (
              instances.map((instance) => (
                <div key={instance.name} className="bg-zinc-800 rounded p-2">
                  <div className="flex items-start justify-between gap-2 mb-1">
                    <div className="flex-1 min-w-0">
                      <p className="text-xs font-medium text-zinc-200 truncate">{instance.name}</p>
                      <p className="text-[10px] text-zinc-500">PID: {instance.pid}</p>
                      <p className="text-[10px] text-zinc-600 truncate font-mono">{instance.image}</p>
                    </div>
                  </div>
                  <div className="flex gap-1 pt-1">
                    <button
                      onClick={() => performInstanceAction(instance.name, 'instance_stop')}
                      disabled={actionLoading.has(`${instance.name}-instance_stop`)}
                      className="flex-1 flex items-center justify-center gap-1 px-2 py-1 bg-red-900/30 hover:bg-red-900/50 text-xs text-red-400 rounded transition-colors disabled:opacity-50"
                      title="Stop"
                    >
                      {actionLoading.has(`${instance.name}-instance_stop`) ? (
                        <Loader2 className="w-3 h-3 animate-spin" />
                      ) : (
                        'Stop'
                      )}
                    </button>
                  </div>
                </div>
              ))
            )}
          </div>
        )}
      </div>
    </div>
  );
}

// Export the extension definition
export const singularityExtension: ToolExtension = {
  id: 'singularity',
  name: 'Singularity/Apptainer',
  icon: HardDrive,
  description: 'Manage Singularity/Apptainer containers for HPC',
  checkInstalled: async () => {
    try {
      await invoke<SingularityInstance[]>('singularity_list_instances');
      return true;
    } catch {
      return false;
    }
  },
  SidebarPanel: SingularityPanelContent,
};
