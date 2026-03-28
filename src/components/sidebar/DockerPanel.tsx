import { useState, useEffect } from 'react';
import {
  Container,
  Box,
  HardDrive,
  Play,
  Square,
  Trash2,
  ScrollText,
  RefreshCw,
  AlertCircle,
  Loader2,
} from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import type { ToolExtension } from '../../types/toolExtension';

interface DockerContainer {
  id: string;
  names: string;
  image: string;
  status: string;
  state: string;
  ports: string;
}

interface DockerImage {
  id: string;
  repository: string;
  tag: string;
  size: string;
  created: string;
}

interface DockerVolume {
  name: string;
  driver: string;
  mountpoint: string;
}

type DockerTab = 'containers' | 'images' | 'volumes';

function DockerPanelContent() {
  const [activeTab, setActiveTab] = useState<DockerTab>('containers');
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [dockerInstalled, setDockerInstalled] = useState(false);
  const [containers, setContainers] = useState<DockerContainer[]>([]);
  const [images, setImages] = useState<DockerImage[]>([]);
  const [volumes, setVolumes] = useState<DockerVolume[]>([]);
  const [actionLoading, setActionLoading] = useState<Set<string>>(new Set());

  // Check if Docker is installed on component mount
  useEffect(() => {
    checkDocker();
  }, []);

  const checkDocker = async () => {
    try {
      setLoading(true);
      // Try to list containers to check if Docker is available
      await invoke<DockerContainer[]>('docker_list_containers');
      setDockerInstalled(true);
      setError(null);
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : String(err);
      if (errorMsg.includes('Docker not available')) {
        setDockerInstalled(false);
      } else {
        setError(errorMsg);
      }
    } finally {
      setLoading(false);
    }
  };

  const loadContainers = async () => {
    try {
      setLoading(true);
      setError(null);
      const data = await invoke<DockerContainer[]>('docker_list_containers');
      setContainers(data);
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : String(err);
      setError(errorMsg);
    } finally {
      setLoading(false);
    }
  };

  const loadImages = async () => {
    try {
      setLoading(true);
      setError(null);
      const data = await invoke<DockerImage[]>('docker_list_images');
      setImages(data);
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : String(err);
      setError(errorMsg);
    } finally {
      setLoading(false);
    }
  };

  const loadVolumes = async () => {
    try {
      setLoading(true);
      setError(null);
      const data = await invoke<DockerVolume[]>('docker_list_volumes');
      setVolumes(data);
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : String(err);
      setError(errorMsg);
    } finally {
      setLoading(false);
    }
  };

  // Load data when tab changes
  useEffect(() => {
    if (!dockerInstalled) return;
    if (activeTab === 'containers') loadContainers();
    if (activeTab === 'images') loadImages();
    if (activeTab === 'volumes') loadVolumes();
  }, [activeTab, dockerInstalled]);

  const performContainerAction = async (containerId: string, action: string) => {
    try {
      const key = `${containerId}-${action}`;
      setActionLoading((prev) => new Set(prev).add(key));
      await invoke<string>('docker_container_action', { container_id: containerId, action });
      setError(null);
      // Reload containers after action
      loadContainers();
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : String(err);
      setError(errorMsg);
    } finally {
      const key = `${containerId}-${action}`;
      setActionLoading((prev) => {
        const next = new Set(prev);
        next.delete(key);
        return next;
      });
    }
  };

  if (!dockerInstalled) {
    return (
      <div className="flex flex-col h-full">
        <div className="flex items-center justify-between px-3 py-2 border-b border-zinc-800">
          <div className="flex items-center gap-1.5 text-xs text-zinc-400">
            <Container className="w-4 h-4" />
            <span className="font-medium">Docker</span>
          </div>
        </div>
        <div className="flex-1 flex flex-col items-center justify-center gap-3 px-4 text-center">
          <AlertCircle className="w-8 h-8 text-zinc-600" />
          <div>
            <p className="text-sm font-medium text-zinc-300 mb-1">Docker Not Installed</p>
            <p className="text-xs text-zinc-500 mb-3">
              Install Docker Desktop or Docker Engine to use this tool.
            </p>
            <code className="block text-xs bg-zinc-800/50 p-2 rounded text-zinc-400 mb-3 font-mono">
              brew install docker
            </code>
            <button
              onClick={checkDocker}
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
          <Container className="w-4 h-4 text-blue-400" />
          <span className="font-medium">Docker</span>
        </div>
        <button
          onClick={() => {
            if (activeTab === 'containers') loadContainers();
            if (activeTab === 'images') loadImages();
            if (activeTab === 'volumes') loadVolumes();
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
          onClick={() => setActiveTab('containers')}
          className={`flex items-center gap-1 px-2 py-1 text-xs font-medium rounded transition-colors ${
            activeTab === 'containers'
              ? 'bg-blue-900/40 text-blue-300'
              : 'text-zinc-500 hover:text-zinc-400'
          }`}
        >
          <Container className="w-3.5 h-3.5" />
          Containers
        </button>
        <button
          onClick={() => setActiveTab('images')}
          className={`flex items-center gap-1 px-2 py-1 text-xs font-medium rounded transition-colors ${
            activeTab === 'images'
              ? 'bg-blue-900/40 text-blue-300'
              : 'text-zinc-500 hover:text-zinc-400'
          }`}
        >
          <Box className="w-3.5 h-3.5" />
          Images
        </button>
        <button
          onClick={() => setActiveTab('volumes')}
          className={`flex items-center gap-1 px-2 py-1 text-xs font-medium rounded transition-colors ${
            activeTab === 'volumes'
              ? 'bg-blue-900/40 text-blue-300'
              : 'text-zinc-500 hover:text-zinc-400'
          }`}
        >
          <HardDrive className="w-3.5 h-3.5" />
          Volumes
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

        {/* Containers Tab */}
        {activeTab === 'containers' && !loading && (
          <div className="p-2 space-y-1">
            {containers.length === 0 ? (
              <div className="text-center py-8 text-xs text-zinc-600">No containers</div>
            ) : (
              containers.map((container) => (
                <div key={container.id} className="bg-zinc-800 rounded p-2 space-y-1">
                  <div className="flex items-start justify-between gap-2">
                    <div className="flex-1 min-w-0">
                      <p className="text-xs font-medium text-zinc-200 truncate">{container.names}</p>
                      <p className="text-[10px] text-zinc-500 truncate">{container.image}</p>
                    </div>
                    <span
                      className={`text-[10px] font-medium px-1.5 py-0.5 rounded whitespace-nowrap shrink-0 ${
                        container.state === 'running'
                          ? 'bg-green-900/40 text-green-300'
                          : 'bg-zinc-700 text-zinc-400'
                      }`}
                    >
                      {container.state}
                    </span>
                  </div>
                  {container.ports && (
                    <p className="text-[10px] text-zinc-500 truncate">Ports: {container.ports}</p>
                  )}
                  <div className="flex gap-1 pt-1">
                    {container.state === 'running' ? (
                      <>
                        <button
                          onClick={() => performContainerAction(container.id, 'stop')}
                          disabled={actionLoading.has(`${container.id}-stop`)}
                          className="flex-1 flex items-center justify-center gap-1 px-2 py-1 bg-zinc-700 hover:bg-zinc-600 text-xs text-zinc-300 rounded transition-colors disabled:opacity-50"
                          title="Stop"
                        >
                          {actionLoading.has(`${container.id}-stop`) ? (
                            <Loader2 className="w-3 h-3 animate-spin" />
                          ) : (
                            <Square className="w-3 h-3" />
                          )}
                          Stop
                        </button>
                        <button
                          onClick={() => performContainerAction(container.id, 'logs')}
                          disabled={actionLoading.has(`${container.id}-logs`)}
                          className="flex items-center justify-center px-2 py-1 bg-zinc-700 hover:bg-zinc-600 text-xs text-zinc-300 rounded transition-colors disabled:opacity-50"
                          title="Logs"
                        >
                          {actionLoading.has(`${container.id}-logs`) ? (
                            <Loader2 className="w-3 h-3 animate-spin" />
                          ) : (
                            <ScrollText className="w-3 h-3" />
                          )}
                        </button>
                      </>
                    ) : (
                      <button
                        onClick={() => performContainerAction(container.id, 'start')}
                        disabled={actionLoading.has(`${container.id}-start`)}
                        className="flex-1 flex items-center justify-center gap-1 px-2 py-1 bg-green-900/40 hover:bg-green-900/60 text-xs text-green-300 rounded transition-colors disabled:opacity-50"
                        title="Start"
                      >
                        {actionLoading.has(`${container.id}-start`) ? (
                          <Loader2 className="w-3 h-3 animate-spin" />
                        ) : (
                          <Play className="w-3 h-3" />
                        )}
                        Start
                      </button>
                    )}
                    <button
                      onClick={() => performContainerAction(container.id, 'remove')}
                      disabled={actionLoading.has(`${container.id}-remove`)}
                      className="flex items-center justify-center px-2 py-1 bg-red-900/30 hover:bg-red-900/50 text-xs text-red-400 rounded transition-colors disabled:opacity-50"
                      title="Remove"
                    >
                      {actionLoading.has(`${container.id}-remove`) ? (
                        <Loader2 className="w-3 h-3 animate-spin" />
                      ) : (
                        <Trash2 className="w-3 h-3" />
                      )}
                    </button>
                  </div>
                </div>
              ))
            )}
          </div>
        )}

        {/* Images Tab */}
        {activeTab === 'images' && !loading && (
          <div className="p-2 space-y-1">
            {images.length === 0 ? (
              <div className="text-center py-8 text-xs text-zinc-600">No images</div>
            ) : (
              images.map((image) => (
                <div key={image.id} className="bg-zinc-800 rounded p-2">
                  <div className="flex items-start justify-between gap-2">
                    <div className="flex-1 min-w-0">
                      <p className="text-xs font-medium text-zinc-200 truncate">
                        {image.repository}:{image.tag}
                      </p>
                      <p className="text-[10px] text-zinc-500">
                        {image.size} · {image.created}
                      </p>
                    </div>
                  </div>
                </div>
              ))
            )}
          </div>
        )}

        {/* Volumes Tab */}
        {activeTab === 'volumes' && !loading && (
          <div className="p-2 space-y-1">
            {volumes.length === 0 ? (
              <div className="text-center py-8 text-xs text-zinc-600">No volumes</div>
            ) : (
              volumes.map((volume) => (
                <div key={volume.name} className="bg-zinc-800 rounded p-2">
                  <p className="text-xs font-medium text-zinc-200 truncate">{volume.name}</p>
                  <p className="text-[10px] text-zinc-500">{volume.driver}</p>
                  <p className="text-[10px] text-zinc-600 truncate font-mono">{volume.mountpoint}</p>
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
export const dockerExtension: ToolExtension = {
  id: 'docker',
  name: 'Docker',
  icon: Container,
  description: 'Manage Docker containers, images, and volumes',
  checkInstalled: async () => {
    try {
      await invoke<DockerContainer[]>('docker_list_containers');
      return true;
    } catch {
      return false;
    }
  },
  SidebarPanel: DockerPanelContent,
};
