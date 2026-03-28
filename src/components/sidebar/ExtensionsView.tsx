import { useState, useEffect, useCallback, useRef } from 'react';
import {
  Search,
  Download,
  Trash2,
  ToggleLeft,
  ToggleRight,
  RefreshCw,
  Star,
  CheckCircle,
  AlertCircle,
  Loader2,
  Package,
  ChevronLeft,
  FileUp,
  ExternalLink,
  Play,
} from 'lucide-react';
import { listen } from '@tauri-apps/api/event';
import {
  searchExtensions,
  listInstalledExtensions,
  installExtension,
  uninstallExtension,
  enableExtension,
  disableExtension,
  checkExtensionCompatibility,
  getExtensionReadme,
} from '../../lib/extensions';
import { getToolExtensions } from '../../lib/toolExtensions';
import type {
  ExtensionInfo,
  InstalledExtension,
  CompatibilityReport,
  InstallProgress,
  SearchResult,
} from '../../types/extensions';
import type { ToolExtension } from '../../types/toolExtension';

type Tab = 'marketplace' | 'installed';

interface DetailView {
  namespace: string;
  name: string;
  displayName: string;
}

interface ExtensionsViewState {
  openToolId?: string;
}

export function ExtensionsView() {
  const [tab, setTab] = useState<Tab>('marketplace');
  const [query, setQuery] = useState('');
  const [searchResults, setSearchResults] = useState<ExtensionInfo[]>([]);
  const [totalResults, setTotalResults] = useState(0);
  const [installed, setInstalled] = useState<InstalledExtension[]>([]);
  const [loading, setLoading] = useState(false);
  const [installing, setInstalling] = useState<Set<string>>(new Set());
  const [installProgress, setInstallProgress] = useState<Record<string, InstallProgress>>({});
  const [detailView, setDetailView] = useState<DetailView | null>(null);
  const [readme, setReadme] = useState<string | null>(null);
  const [compatibility, setCompatibility] = useState<Record<string, CompatibilityReport>>({});
  const [toolExtensions, setToolExtensions] = useState<ToolExtension[]>([]);
  const searchTimeout = useRef<ReturnType<typeof setTimeout> | null>(null);
  const installedIds = new Set(installed.map((e) => e.id));

  // Load installed extensions and tool extensions on mount
  useEffect(() => {
    loadInstalled();
    // Load available tool extensions
    const allTools = getToolExtensions();
    setToolExtensions(allTools);
  }, []);

  // Load popular extensions on first mount
  useEffect(() => {
    doSearch('', 'downloadCount');
  }, []);

  // Listen for install progress events
  useEffect(() => {
    const unlisten = listen<InstallProgress>('extension-install-progress', (event) => {
      const progress = event.payload;
      setInstallProgress((prev) => ({ ...prev, [progress.extension_id]: progress }));
      if (progress.stage === 'complete') {
        setInstalling((prev) => {
          const next = new Set(prev);
          next.delete(progress.extension_id);
          return next;
        });
        loadInstalled();
        // Clean up progress after a moment
        setTimeout(() => {
          setInstallProgress((prev) => {
            const next = { ...prev };
            delete next[progress.extension_id];
            return next;
          });
        }, 2000);
      }
    });
    return () => { unlisten.then((u) => u()); };
  }, []);

  const loadInstalled = async () => {
    try {
      const exts = await listInstalledExtensions();
      setInstalled(exts);
    } catch (err) {
      console.error('Failed to load installed extensions:', err);
    }
  };

  const doSearch = async (q: string, sortBy?: string) => {
    setLoading(true);
    try {
      const result: SearchResult = await searchExtensions(q, {
        size: 30,
        sortBy: sortBy || 'relevance',
        sortOrder: 'desc',
      });
      setSearchResults(result.extensions);
      setTotalResults(result.total_size);
    } catch (err) {
      console.error('Search failed:', err);
    }
    setLoading(false);
  };

  const handleSearchInput = useCallback((value: string) => {
    setQuery(value);
    if (searchTimeout.current) clearTimeout(searchTimeout.current);
    searchTimeout.current = setTimeout(() => {
      doSearch(value);
    }, 300);
  }, []);

  const handleInstall = async (namespace: string, name: string) => {
    const id = `${namespace}.${name}`;
    setInstalling((prev) => new Set(prev).add(id));
    try {
      await installExtension(namespace, name);
    } catch (err) {
      console.error('Install failed:', err);
      setInstalling((prev) => {
        const next = new Set(prev);
        next.delete(id);
        return next;
      });
    }
  };

  const handleUninstall = async (id: string) => {
    try {
      await uninstallExtension(id);
      await loadInstalled();
    } catch (err) {
      console.error('Uninstall failed:', err);
    }
  };

  const handleToggle = async (id: string, enabled: boolean) => {
    try {
      if (enabled) {
        await disableExtension(id);
      } else {
        await enableExtension(id);
      }
      await loadInstalled();
    } catch (err) {
      console.error('Toggle failed:', err);
    }
  };

  const handleCheckCompat = async (namespace: string, name: string) => {
    const id = `${namespace}.${name}`;
    if (compatibility[id]) return;
    try {
      const report = await checkExtensionCompatibility(namespace, name);
      setCompatibility((prev) => ({ ...prev, [id]: report }));
    } catch {
      // Ignore compatibility check failures
    }
  };

  const openDetail = async (namespace: string, name: string, displayName: string) => {
    setDetailView({ namespace, name, displayName });
    setReadme(null);
    handleCheckCompat(namespace, name);
    try {
      const text = await getExtensionReadme(namespace, name);
      setReadme(text);
    } catch {
      setReadme('*No README available.*');
    }
  };

  const openToolPanel = (toolId: string) => {
    // Emit event to switch to the tool panel in the sidebar
    const event = new CustomEvent('open-tool-panel', { detail: { toolId } });
    window.dispatchEvent(event);
  };

  // ── Detail View ──────────────────────────────────────────────────────

  if (detailView) {
    const { namespace, name, displayName } = detailView;
    const extId = `${namespace}.${name}`;
    const isInstalled = installedIds.has(extId);
    const isInstalling = installing.has(extId);
    const compat = compatibility[extId];

    return (
      <div className="flex flex-col h-full">
        {/* Header */}
        <div className="flex items-center gap-2 px-3 py-2 border-b border-zinc-800">
          <button
            onClick={() => setDetailView(null)}
            className="p-0.5 rounded hover:bg-zinc-800 text-zinc-400 hover:text-zinc-200"
          >
            <ChevronLeft className="w-4 h-4" />
          </button>
          <span className="text-[11px] font-semibold text-zinc-500 uppercase tracking-wider truncate">
            {displayName}
          </span>
        </div>

        {/* Detail content */}
        <div className="flex-1 overflow-y-auto px-3 py-3 space-y-3">
          {/* Title + actions */}
          <div className="flex items-start justify-between gap-2">
            <div>
              <h3 className="text-sm font-medium text-zinc-200">{displayName}</h3>
              <p className="text-[11px] text-zinc-500">{namespace}</p>
            </div>
            <div>
              {isInstalled ? (
                <button
                  onClick={() => handleUninstall(extId)}
                  className="px-2 py-1 text-[11px] rounded bg-red-500/10 text-red-400 hover:bg-red-500/20"
                >
                  Uninstall
                </button>
              ) : isInstalling ? (
                <span className="flex items-center gap-1 text-[11px] text-blue-400">
                  <Loader2 className="w-3 h-3 animate-spin" /> Installing...
                </span>
              ) : (
                <button
                  onClick={() => handleInstall(namespace, name)}
                  className="px-2 py-1 text-[11px] rounded bg-blue-500 text-white hover:bg-blue-600"
                >
                  Install
                </button>
              )}
            </div>
          </div>

          {/* Compatibility badge */}
          {compat && <CompatibilityBadge report={compat} />}

          {/* README */}
          {readme === null ? (
            <div className="flex items-center justify-center py-8">
              <Loader2 className="w-4 h-4 animate-spin text-zinc-500" />
            </div>
          ) : (
            <div className="text-xs text-zinc-400 leading-relaxed whitespace-pre-wrap break-words">
              {readme.slice(0, 5000)}
              {readme.length > 5000 && (
                <span className="text-zinc-600">... (truncated)</span>
              )}
            </div>
          )}
        </div>
      </div>
    );
  }

  // ── Main View ────────────────────────────────────────────────────────

  return (
    <div className="flex flex-col h-full">
      {/* Header */}
      <div className="flex items-center px-3 py-2 border-b border-zinc-800">
        <span className="text-[11px] font-semibold text-zinc-500 uppercase tracking-wider">
          Extensions
        </span>
        <div className="ml-auto flex items-center gap-1">
          <button
            onClick={loadInstalled}
            className="p-1 rounded hover:bg-zinc-800 text-zinc-500 hover:text-zinc-300"
            title="Refresh"
          >
            <RefreshCw className="w-3.5 h-3.5" />
          </button>
        </div>
      </div>

      {/* Search */}
      <div className="px-3 py-2 border-b border-zinc-800">
        <div className="flex items-center gap-2 px-2 py-1.5 rounded bg-zinc-800/50 border border-zinc-700/50">
          <Search className="w-3.5 h-3.5 text-zinc-500 shrink-0" />
          <input
            type="text"
            value={query}
            onChange={(e) => handleSearchInput(e.target.value)}
            placeholder="Search extensions..."
            className="flex-1 bg-transparent text-xs text-zinc-200 placeholder-zinc-600 outline-none"
          />
        </div>
      </div>

      {/* Tabs */}
      <div className="flex border-b border-zinc-800">
        <button
          onClick={() => setTab('marketplace')}
          className={`flex-1 px-3 py-1.5 text-[11px] font-medium transition-colors ${
            tab === 'marketplace'
              ? 'text-zinc-200 border-b-2 border-blue-500'
              : 'text-zinc-500 hover:text-zinc-300'
          }`}
        >
          Marketplace
        </button>
        <button
          onClick={() => setTab('installed')}
          className={`flex-1 px-3 py-1.5 text-[11px] font-medium transition-colors ${
            tab === 'installed'
              ? 'text-zinc-200 border-b-2 border-blue-500'
              : 'text-zinc-500 hover:text-zinc-300'
          }`}
        >
          Installed ({installed.length})
        </button>
      </div>

      {/* Content */}
      <div className="flex-1 overflow-y-auto">
        {tab === 'marketplace' ? (
          <MarketplaceList
            extensions={searchResults}
            totalResults={totalResults}
            loading={loading}
            installedIds={installedIds}
            installing={installing}
            installProgress={installProgress}
            compatibility={compatibility}
            onInstall={handleInstall}
            onCheckCompat={handleCheckCompat}
            onOpenDetail={openDetail}
          />
        ) : (
          <>
            <InstalledList
              extensions={installed}
              onUninstall={handleUninstall}
              onToggle={handleToggle}
              onOpenDetail={openDetail}
            />
            {toolExtensions.length > 0 && (
              <IntegratedToolsList
                tools={toolExtensions}
                onOpenTool={openToolPanel}
              />
            )}
          </>
        )}
      </div>
    </div>
  );
}

// ── Sub-components ───────────────────────────────────────────────────────

function CompatibilityBadge({ report }: { report: CompatibilityReport }) {
  const config: Record<string, { color: string; label: string; icon: typeof CheckCircle }> = {
    full: { color: 'text-green-400 bg-green-500/10', label: 'Full Support', icon: CheckCircle },
    lsp: { color: 'text-blue-400 bg-blue-500/10', label: 'LSP Support', icon: CheckCircle },
    partial: { color: 'text-yellow-400 bg-yellow-500/10', label: 'Partial', icon: AlertCircle },
    not_compatible: { color: 'text-zinc-500 bg-zinc-700/30', label: 'Not Compatible', icon: AlertCircle },
  };
  const c = config[report.level] || config.not_compatible;
  const Icon = c.icon;

  return (
    <div className={`inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-[10px] font-medium ${c.color}`}>
      <Icon className="w-3 h-3" />
      {c.label}
      {report.unsupported.length > 0 && (
        <span className="text-zinc-600 ml-1" title={`Unsupported: ${report.unsupported.join(', ')}`}>
          ({report.unsupported.length} unsupported)
        </span>
      )}
    </div>
  );
}

function formatDownloads(count: number | null): string {
  if (!count) return '';
  if (count >= 1_000_000) return `${(count / 1_000_000).toFixed(1)}M`;
  if (count >= 1_000) return `${(count / 1_000).toFixed(0)}K`;
  return count.toString();
}

interface MarketplaceListProps {
  extensions: ExtensionInfo[];
  totalResults: number;
  loading: boolean;
  installedIds: Set<string>;
  installing: Set<string>;
  installProgress: Record<string, InstallProgress>;
  compatibility: Record<string, CompatibilityReport>;
  onInstall: (namespace: string, name: string) => void;
  onCheckCompat: (namespace: string, name: string) => void;
  onOpenDetail: (namespace: string, name: string, displayName: string) => void;
}

function MarketplaceList({
  extensions,
  totalResults,
  loading,
  installedIds,
  installing,
  installProgress,
  compatibility,
  onInstall,
  onCheckCompat,
  onOpenDetail,
}: MarketplaceListProps) {
  if (loading && extensions.length === 0) {
    return (
      <div className="flex items-center justify-center py-12">
        <Loader2 className="w-5 h-5 animate-spin text-zinc-500" />
      </div>
    );
  }

  if (extensions.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center py-12 text-zinc-600 text-xs">
        <Package className="w-8 h-8 mb-2" />
        No extensions found
      </div>
    );
  }

  return (
    <div className="divide-y divide-zinc-800/50">
      {totalResults > 0 && (
        <div className="px-3 py-1.5 text-[10px] text-zinc-600">
          {totalResults.toLocaleString()} results
        </div>
      )}
      {extensions.map((ext) => {
        const extId = `${ext.namespace}.${ext.name}`;
        const isInstalled = installedIds.has(extId);
        const isInstalling = installing.has(extId);
        const progress = installProgress[extId];
        const compat = compatibility[extId];
        const displayName = ext.display_name || ext.name;

        return (
          <div
            key={extId}
            className="px-3 py-2 hover:bg-zinc-800/30 cursor-pointer group"
            onClick={() => onOpenDetail(ext.namespace, ext.name, displayName)}
            onMouseEnter={() => onCheckCompat(ext.namespace, ext.name)}
          >
            <div className="flex items-start gap-2">
              {/* Icon placeholder */}
              <div className="w-8 h-8 rounded bg-zinc-800 flex items-center justify-center shrink-0 mt-0.5">
                {ext.files?.icon ? (
                  <img
                    src={ext.files.icon}
                    alt=""
                    className="w-8 h-8 rounded"
                    onError={(e) => {
                      (e.target as HTMLImageElement).style.display = 'none';
                    }}
                  />
                ) : (
                  <Package className="w-4 h-4 text-zinc-600" />
                )}
              </div>

              <div className="flex-1 min-w-0">
                <div className="flex items-center gap-1.5">
                  <span className="text-xs font-medium text-zinc-200 truncate">
                    {displayName}
                  </span>
                  {ext.verified && (
                    <CheckCircle className="w-3 h-3 text-blue-400 shrink-0" />
                  )}
                </div>
                <div className="flex items-center gap-1.5 mt-0.5">
                  <span className="text-[10px] text-zinc-500">{ext.namespace}</span>
                  {ext.download_count ? (
                    <>
                      <span className="text-[10px] text-zinc-700">{'\u00B7'}</span>
                      <span className="text-[10px] text-zinc-500">
                        <Download className="w-2.5 h-2.5 inline mr-0.5" />
                        {formatDownloads(ext.download_count)}
                      </span>
                    </>
                  ) : null}
                  {ext.average_rating ? (
                    <>
                      <span className="text-[10px] text-zinc-700">{'\u00B7'}</span>
                      <span className="text-[10px] text-zinc-500">
                        <Star className="w-2.5 h-2.5 inline mr-0.5 text-yellow-500" />
                        {ext.average_rating.toFixed(1)}
                      </span>
                    </>
                  ) : null}
                </div>
                {ext.description && (
                  <p className="text-[10px] text-zinc-500 mt-1 line-clamp-2">
                    {ext.description}
                  </p>
                )}

                {/* Compat badge + install button */}
                <div className="flex items-center justify-between mt-1.5">
                  <div>{compat && <CompatibilityBadge report={compat} />}</div>
                  <div
                    onClick={(e) => e.stopPropagation()}
                    className="opacity-0 group-hover:opacity-100 transition-opacity"
                  >
                    {isInstalled ? (
                      <span className="text-[10px] text-green-400">Installed</span>
                    ) : isInstalling ? (
                      <span className="flex items-center gap-1 text-[10px] text-blue-400">
                        <Loader2 className="w-3 h-3 animate-spin" />
                        {progress?.stage || 'Installing'}
                      </span>
                    ) : (
                      <button
                        onClick={() => onInstall(ext.namespace, ext.name)}
                        className="px-2 py-0.5 text-[10px] rounded bg-blue-500 text-white hover:bg-blue-600"
                      >
                        Install
                      </button>
                    )}
                  </div>
                </div>
              </div>
            </div>
          </div>
        );
      })}
    </div>
  );
}

interface InstalledListProps {
  extensions: InstalledExtension[];
  onUninstall: (id: string) => void;
  onToggle: (id: string, enabled: boolean) => void;
  onOpenDetail: (namespace: string, name: string, displayName: string) => void;
}

function InstalledList({ extensions, onUninstall, onToggle, onOpenDetail }: InstalledListProps) {
  if (extensions.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center py-12 text-zinc-600 text-xs">
        <Package className="w-8 h-8 mb-2" />
        No extensions installed
      </div>
    );
  }

  return (
    <div className="divide-y divide-zinc-800/50">
      {extensions.map((ext) => {
        const [namespace, name] = ext.id.split('.');
        return (
          <div
            key={ext.id}
            className="px-3 py-2 hover:bg-zinc-800/30 cursor-pointer group"
            onClick={() => onOpenDetail(namespace, name, ext.display_name)}
          >
            <div className="flex items-start gap-2">
              <div className="w-8 h-8 rounded bg-zinc-800 flex items-center justify-center shrink-0 mt-0.5">
                <Package className="w-4 h-4 text-zinc-600" />
              </div>
              <div className="flex-1 min-w-0">
                <div className="flex items-center gap-1.5">
                  <span className={`text-xs font-medium truncate ${ext.enabled ? 'text-zinc-200' : 'text-zinc-500'}`}>
                    {ext.display_name}
                  </span>
                  <span className="text-[10px] text-zinc-600">v{ext.version}</span>
                </div>
                <p className="text-[10px] text-zinc-500 mt-0.5">{ext.publisher}</p>
                {ext.description && (
                  <p className="text-[10px] text-zinc-500 mt-0.5 line-clamp-1">{ext.description}</p>
                )}

                {/* Contribution summary */}
                <div className="flex items-center gap-1.5 mt-1">
                  {ext.contributions.themes.length > 0 && (
                    <span className="text-[9px] px-1 py-0.5 rounded bg-zinc-800 text-zinc-400">
                      {ext.contributions.themes.length} theme{ext.contributions.themes.length !== 1 ? 's' : ''}
                    </span>
                  )}
                  {ext.contributions.snippets.length > 0 && (
                    <span className="text-[9px] px-1 py-0.5 rounded bg-zinc-800 text-zinc-400">
                      {ext.contributions.snippets.length} snippet{ext.contributions.snippets.length !== 1 ? 's' : ''}
                    </span>
                  )}
                  {ext.contributions.languages.length > 0 && (
                    <span className="text-[9px] px-1 py-0.5 rounded bg-zinc-800 text-zinc-400">
                      {ext.contributions.languages.length} lang{ext.contributions.languages.length !== 1 ? 's' : ''}
                    </span>
                  )}
                </div>

                {/* Actions */}
                <div
                  className="flex items-center gap-2 mt-1.5 opacity-0 group-hover:opacity-100 transition-opacity"
                  onClick={(e) => e.stopPropagation()}
                >
                  <button
                    onClick={() => onToggle(ext.id, ext.enabled)}
                    className={`p-0.5 rounded hover:bg-zinc-700 ${ext.enabled ? 'text-green-400' : 'text-zinc-600'}`}
                    title={ext.enabled ? 'Disable' : 'Enable'}
                  >
                    {ext.enabled ? (
                      <ToggleRight className="w-4 h-4" />
                    ) : (
                      <ToggleLeft className="w-4 h-4" />
                    )}
                  </button>
                  <button
                    onClick={() => onUninstall(ext.id)}
                    className="p-0.5 rounded hover:bg-zinc-700 text-zinc-500 hover:text-red-400"
                    title="Uninstall"
                  >
                    <Trash2 className="w-3.5 h-3.5" />
                  </button>
                </div>
              </div>
            </div>
          </div>
        );
      })}
    </div>
  );
}

// ── Integrated Tools List ──────────────────────────────────────────────

interface IntegratedToolsListProps {
  tools: ToolExtension[];
  onOpenTool: (toolId: string) => void;
}

function IntegratedToolsList({ tools, onOpenTool }: IntegratedToolsListProps) {
  const [installedStatus, setInstalledStatus] = useState<Record<string, boolean>>({});
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    // Check which tools are installed
    const checkTools = async () => {
      const status: Record<string, boolean> = {};
      for (const tool of tools) {
        try {
          status[tool.id] = await tool.checkInstalled();
        } catch {
          status[tool.id] = false;
        }
      }
      setInstalledStatus(status);
      setLoading(false);
    };
    checkTools();
  }, [tools]);

  if (loading) {
    return null;
  }

  // Filter to only show installed tools
  const installedTools = tools.filter((t) => installedStatus[t.id]);
  if (installedTools.length === 0) {
    return null;
  }

  return (
    <>
      <div className="border-t border-zinc-800/50 mt-2 pt-2">
        <div className="px-3 py-1.5">
          <div className="text-[10px] text-zinc-500 font-medium uppercase tracking-wider mb-2">
            Integrated Tools
          </div>
          <div className="space-y-1">
            {installedTools.map((tool) => {
              const Icon = tool.icon;
              return (
                <div
                  key={tool.id}
                  className="flex items-start gap-2 px-2 py-1.5 rounded bg-zinc-800/40 hover:bg-zinc-800/60 transition-colors cursor-pointer group"
                  onClick={() => onOpenTool(tool.id)}
                >
                  <div className="w-8 h-8 rounded bg-zinc-800 flex items-center justify-center shrink-0 mt-0">
                    <Icon className="w-4 h-4 text-zinc-400" />
                  </div>
                  <div className="flex-1 min-w-0">
                    <div className="text-xs font-medium text-zinc-200">{tool.name}</div>
                    <p className="text-[10px] text-zinc-500">{tool.description}</p>
                  </div>
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      onOpenTool(tool.id);
                    }}
                    className="p-1 rounded hover:bg-zinc-700 text-zinc-500 hover:text-zinc-300 shrink-0 opacity-0 group-hover:opacity-100 transition-opacity"
                    title="Open"
                  >
                    <Play className="w-3.5 h-3.5" />
                  </button>
                </div>
              );
            })}
          </div>
        </div>
      </div>
    </>
  );
}
