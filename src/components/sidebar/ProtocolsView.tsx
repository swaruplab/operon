import { useState, useEffect, useMemo } from 'react';
import {
  BookOpen,
  FolderOpen,
  RefreshCw,
  Loader2,
  Check,
  Info,
  Plus,
  Sparkles,
  Pencil,
  Trash2,
  ArrowLeft,
  Save,
  X,
  FileText,
  Wand2,
  Search,
  ChevronRight,
  Database,
  GitBranch,
  Package,
  BarChart3,
  Layers,
  User,
  Plug,
  Dna,
  FlaskConical,
  Brain,
  TrendingUp,
  PenTool,
  Stethoscope,
  DollarSign,
  Lightbulb,
  Download,
  Copy,
} from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { emit } from '@tauri-apps/api/event';

interface ProtocolEntry {
  id: string;
  name: string;
  description: string;
  path: string;
  is_folder: boolean;
  file_count: number;
  source: string;   // "bundled" | "user"
  category: string;  // auto-detected category for grouping
}

interface ProtocolsViewProps {
  activeProtocolId: string | null;
  onActivate: (protocol: { id: string; name: string } | null) => void;
}

type ViewMode = 'list' | 'create' | 'edit';
type CreateTab = 'generate' | 'manual';
type FilterTab = 'all' | 'user' | 'bundled';

const CATEGORY_META: Record<string, { label: string; icon: typeof Database; color: string }> = {
  genomics:        { label: 'Genomics & Omics',          icon: Dna,           color: 'text-green-400' },
  database:        { label: 'Databases & References',     icon: Database,      color: 'text-blue-400' },
  cheminformatics: { label: 'Cheminformatics & Drug Discovery', icon: FlaskConical, color: 'text-pink-400' },
  ml_ai:           { label: 'ML, AI & Quantum',           icon: Brain,         color: 'text-purple-400' },
  visualization:   { label: 'Visualization & Plotting',   icon: BarChart3,     color: 'text-amber-400' },
  writing:         { label: 'Writing & Documents',        icon: PenTool,       color: 'text-cyan-400' },
  statistics:      { label: 'Statistics & Data Science',  icon: TrendingUp,    color: 'text-orange-400' },
  integration:     { label: 'Lab Platforms & Integrations', icon: Plug,         color: 'text-indigo-400' },
  research:        { label: 'Research & Reasoning',       icon: Lightbulb,     color: 'text-yellow-400' },
  clinical:        { label: 'Clinical & Healthcare',      icon: Stethoscope,   color: 'text-red-400' },
  finance:         { label: 'Finance & Business',         icon: DollarSign,    color: 'text-emerald-400' },
  pipeline:        { label: 'Pipelines',                  icon: GitBranch,     color: 'text-teal-400' },
  tool:            { label: 'Tools & Packages',           icon: Package,       color: 'text-violet-400' },
  other:           { label: 'Other',                      icon: Layers,        color: 'text-zinc-400' },
};

const CATEGORY_ORDER = [
  'genomics', 'database', 'cheminformatics', 'ml_ai', 'visualization',
  'writing', 'statistics', 'integration', 'research', 'clinical',
  'finance', 'pipeline', 'tool', 'other',
];

export function ProtocolsView({ activeProtocolId, onActivate }: ProtocolsViewProps) {
  const [protocols, setProtocols] = useState<ProtocolEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const [previewContent, setPreviewContent] = useState<string | null>(null);

  // Search & filter
  const [searchQuery, setSearchQuery] = useState('');
  const [filterTab, setFilterTab] = useState<FilterTab>('all');
  const [collapsedCategories, setCollapsedCategories] = useState<Set<string>>(new Set());

  // View mode
  const [viewMode, setViewMode] = useState<ViewMode>('list');
  const [createTab, setCreateTab] = useState<CreateTab>('generate');

  // Create / Edit state
  const [protocolName, setProtocolName] = useState('');
  const [protocolContent, setProtocolContent] = useState('');
  const [editingId, setEditingId] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);

  // AI generation state
  const [aiDescription, setAiDescription] = useState('');
  const [generating, setGenerating] = useState(false);
  const [generateError, setGenerateError] = useState<string | null>(null);

  // Delete confirmation
  const [deletingId, setDeletingId] = useState<string | null>(null);

  // Right-click context menu
  const [contextMenu, setContextMenu] = useState<{ x: number; y: number; protocol: ProtocolEntry } | null>(null);

  // Auto-close context menu on any click
  useEffect(() => {
    if (!contextMenu) return;
    const close = () => setContextMenu(null);
    window.addEventListener('click', close);
    return () => window.removeEventListener('click', close);
  }, [contextMenu]);

  const handleDownload = async (p: ProtocolEntry) => {
    try {
      const content = await invoke<string>('read_protocol', { protocolId: p.id });
      const blob = new Blob([content], { type: 'text/markdown' });
      const url = URL.createObjectURL(blob);
      const link = document.createElement('a');
      link.href = url;
      link.download = `${p.id}.md`;
      document.body.appendChild(link);
      link.click();
      document.body.removeChild(link);
      setTimeout(() => URL.revokeObjectURL(url), 1000);
    } catch (e) {
      emit('show-notification', { message: `Failed to download: ${e}` });
    }
  };

  const handleCopyContent = async (p: ProtocolEntry) => {
    try {
      const content = await invoke<string>('read_protocol', { protocolId: p.id });
      await navigator.clipboard.writeText(content);
      emit('show-notification', { message: `Copied "${p.name}" to clipboard` });
    } catch (e) {
      emit('show-notification', { message: `Failed to copy: ${e}` });
    }
  };

  const loadProtocols = async () => {
    setLoading(true);
    try {
      const items = await invoke<ProtocolEntry[]>('list_protocols');
      setProtocols(items);
    } catch {
      setProtocols([]);
    }
    setLoading(false);
  };

  useEffect(() => {
    loadProtocols();
  }, []);

  // --- Filtered & grouped protocols ---
  const filteredProtocols = useMemo(() => {
    let list = protocols;

    // Source filter
    if (filterTab === 'user') {
      list = list.filter(p => p.source === 'user');
    } else if (filterTab === 'bundled') {
      list = list.filter(p => p.source === 'bundled');
    }

    // Search filter
    if (searchQuery.trim()) {
      const q = searchQuery.toLowerCase();
      list = list.filter(p =>
        p.name.toLowerCase().includes(q) ||
        p.id.toLowerCase().includes(q) ||
        p.description.toLowerCase().includes(q) ||
        p.category.toLowerCase().includes(q)
      );
    }

    return list;
  }, [protocols, filterTab, searchQuery]);

  const groupedProtocols = useMemo(() => {
    const groups: Record<string, ProtocolEntry[]> = {};
    for (const p of filteredProtocols) {
      const cat = p.category || 'other';
      if (!groups[cat]) groups[cat] = [];
      groups[cat].push(p);
    }
    return groups;
  }, [filteredProtocols]);

  const userCount = useMemo(() => protocols.filter(p => p.source === 'user').length, [protocols]);
  const bundledCount = useMemo(() => protocols.filter(p => p.source === 'bundled').length, [protocols]);

  const toggleCategory = (cat: string) => {
    setCollapsedCategories(prev => {
      const next = new Set(prev);
      if (next.has(cat)) next.delete(cat);
      else next.add(cat);
      return next;
    });
  };

  const handleTogglePreview = async (p: ProtocolEntry) => {
    if (expandedId === p.id) {
      setExpandedId(null);
      setPreviewContent(null);
      return;
    }
    setExpandedId(p.id);
    try {
      const content = await invoke<string>('read_protocol', { protocolId: p.id });
      setPreviewContent(content);
    } catch {
      setPreviewContent('(Could not read protocol file)');
    }
  };

  const handleActivate = (p: ProtocolEntry) => {
    if (activeProtocolId === p.id) {
      onActivate(null);
    } else {
      onActivate({ id: p.id, name: p.name });
      emit('protocol-activated', { id: p.id, name: p.name });
    }
  };

  const openProtocolsFolder = async () => {
    try {
      const dir = await invoke<string>('get_protocols_dir');
      emit('show-notification', { message: `Protocols folder: ${dir}` });
    } catch {
      // ignore
    }
  };

  // --- Create / Edit handlers ---

  const resetCreateState = () => {
    setProtocolName('');
    setProtocolContent('');
    setAiDescription('');
    setEditingId(null);
    setSaveError(null);
    setGenerateError(null);
    setGenerating(false);
    setSaving(false);
    setCreateTab('generate');
  };

  const handleNew = () => {
    resetCreateState();
    setViewMode('create');
  };

  const handleEdit = async (p: ProtocolEntry) => {
    resetCreateState();
    setEditingId(p.id);
    setProtocolName(p.name);
    setViewMode('edit');
    setCreateTab('manual');
    try {
      const content = await invoke<string>('read_protocol', { protocolId: p.id });
      setProtocolContent(content);
    } catch {
      setProtocolContent('');
    }
  };

  const handleGenerate = async () => {
    if (!aiDescription.trim()) return;
    setGenerating(true);
    setGenerateError(null);
    try {
      const content = await invoke<string>('generate_protocol', { description: aiDescription.trim() });
      setProtocolContent(content);
      const h1Match = content.match(/^#\s+(.+)$/m);
      if (h1Match && !protocolName) {
        setProtocolName(h1Match[1].trim());
      }
      setCreateTab('manual');
    } catch (e) {
      setGenerateError(String(e));
    }
    setGenerating(false);
  };

  const handleSave = async () => {
    if (!protocolContent.trim()) {
      setSaveError('Protocol content cannot be empty');
      return;
    }
    setSaving(true);
    setSaveError(null);

    const id = editingId || protocolName
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, '-')
      .replace(/^-+|-+$/g, '')
      || `protocol-${Date.now()}`;

    try {
      await invoke('save_protocol', { protocolId: id, content: protocolContent });
      await loadProtocols();
      setViewMode('list');
      resetCreateState();
    } catch (e) {
      setSaveError(String(e));
    }
    setSaving(false);
  };

  const handleDelete = async (protocolId: string) => {
    try {
      if (activeProtocolId === protocolId) {
        onActivate(null);
      }
      await invoke('delete_protocol', { protocolId });
      setDeletingId(null);
      await loadProtocols();
    } catch (e) {
      emit('show-notification', { message: `Failed to delete: ${e}` });
      setDeletingId(null);
    }
  };

  const handleBack = () => {
    setViewMode('list');
    resetCreateState();
  };

  // --- Render ---

  // Create / Edit view
  if (viewMode === 'create' || viewMode === 'edit') {
    return (
      <div className="flex flex-col h-full">
        {/* Header */}
        <div className="flex items-center gap-2 px-3 py-2 border-b border-zinc-800">
          <button
            onClick={handleBack}
            className="p-1 rounded hover:bg-zinc-800 text-zinc-500 hover:text-zinc-300 transition-colors"
            title="Back to list"
          >
            <ArrowLeft className="w-3.5 h-3.5" />
          </button>
          <span className="text-sm font-medium text-zinc-300">
            {viewMode === 'edit' ? 'Edit Protocol' : 'New Protocol'}
          </span>
        </div>

        {/* Tab selector (only in create mode) */}
        {viewMode === 'create' && (
          <div className="flex border-b border-zinc-800">
            <button
              onClick={() => setCreateTab('generate')}
              className={`flex-1 flex items-center justify-center gap-1.5 px-3 py-2 text-[11px] font-medium transition-colors ${
                createTab === 'generate'
                  ? 'text-purple-300 border-b-2 border-purple-500 bg-purple-950/20'
                  : 'text-zinc-500 hover:text-zinc-300'
              }`}
            >
              <Wand2 className="w-3 h-3" />
              Generate with AI
            </button>
            <button
              onClick={() => setCreateTab('manual')}
              className={`flex-1 flex items-center justify-center gap-1.5 px-3 py-2 text-[11px] font-medium transition-colors ${
                createTab === 'manual'
                  ? 'text-teal-300 border-b-2 border-teal-500 bg-teal-950/20'
                  : 'text-zinc-500 hover:text-zinc-300'
              }`}
            >
              <FileText className="w-3 h-3" />
              Write Manually
            </button>
          </div>
        )}

        {/* Content area */}
        <div className="flex-1 overflow-y-auto px-3 py-3 space-y-3">
          {/* AI Generate tab */}
          {createTab === 'generate' && viewMode === 'create' && (
            <div className="space-y-3">
              <div>
                <label className="text-[10px] text-zinc-500 font-medium uppercase tracking-wider block mb-1.5">
                  Describe the protocol you need
                </label>
                <textarea
                  value={aiDescription}
                  onChange={(e) => setAiDescription(e.target.value)}
                  placeholder="e.g., Single-cell RNA-seq analysis using Scanpy with SLURM job submission for a cluster with GPU and CPU partitions..."
                  className="w-full h-28 bg-zinc-900 border border-zinc-700 rounded-lg px-3 py-2 text-xs text-zinc-200 placeholder:text-zinc-600 outline-none focus:border-purple-600 resize-none leading-relaxed"
                  autoFocus
                />
              </div>

              <button
                onClick={handleGenerate}
                disabled={generating || !aiDescription.trim()}
                className="w-full flex items-center justify-center gap-2 px-3 py-2 bg-purple-600 hover:bg-purple-500 text-white rounded-lg text-xs font-medium transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
              >
                {generating ? (
                  <>
                    <Loader2 className="w-3.5 h-3.5 animate-spin" />
                    Generating protocol...
                  </>
                ) : (
                  <>
                    <Sparkles className="w-3.5 h-3.5" />
                    Generate Protocol
                  </>
                )}
              </button>

              {generating && (
                <p className="text-[10px] text-zinc-600 text-center">
                  Claude is writing your protocol. This may take 15-30 seconds.
                </p>
              )}

              {generateError && (
                <div className="p-2 bg-red-950/20 border border-red-900/30 rounded-lg">
                  <p className="text-[10px] text-red-300">{generateError}</p>
                </div>
              )}
            </div>
          )}

          {/* Manual / Review tab */}
          {(createTab === 'manual' || viewMode === 'edit') && (
            <div className="space-y-3">
              <div>
                <label className="text-[10px] text-zinc-500 font-medium uppercase tracking-wider block mb-1.5">
                  Protocol Name
                </label>
                <input
                  type="text"
                  value={protocolName}
                  onChange={(e) => setProtocolName(e.target.value)}
                  placeholder="e.g., scRNA-seq Scanpy Pipeline"
                  className="w-full bg-zinc-900 border border-zinc-700 rounded-lg px-3 py-2 text-xs text-zinc-200 placeholder:text-zinc-600 outline-none focus:border-teal-600"
                  disabled={viewMode === 'edit'}
                />
              </div>

              <div>
                <label className="text-[10px] text-zinc-500 font-medium uppercase tracking-wider block mb-1.5">
                  Protocol Content (Markdown)
                </label>
                <textarea
                  value={protocolContent}
                  onChange={(e) => setProtocolContent(e.target.value)}
                  placeholder={"# My Protocol\n\nDescribe the rules, tools, and patterns Claude should follow...\n\n## Tools & Packages\n- ...\n\n## Workflow\n1. ..."}
                  className="w-full h-64 bg-zinc-900 border border-zinc-700 rounded-lg px-3 py-2 text-[11px] text-zinc-200 placeholder:text-zinc-600 outline-none focus:border-teal-600 resize-none font-mono leading-relaxed"
                  autoFocus={viewMode === 'edit'}
                />
              </div>

              {protocolContent && viewMode === 'create' && createTab === 'manual' && aiDescription && (
                <p className="text-[9px] text-zinc-600">
                  Generated by Claude — review and edit before saving.
                </p>
              )}

              {saveError && (
                <div className="p-2 bg-red-950/20 border border-red-900/30 rounded-lg">
                  <p className="text-[10px] text-red-300">{saveError}</p>
                </div>
              )}
            </div>
          )}
        </div>

        {/* Footer with save/cancel */}
        {(createTab === 'manual' || viewMode === 'edit') && (
          <div className="flex items-center gap-2 px-3 py-2 border-t border-zinc-800">
            <button
              onClick={handleSave}
              disabled={saving || !protocolContent.trim()}
              className="flex-1 flex items-center justify-center gap-1.5 px-3 py-1.5 bg-teal-600 hover:bg-teal-500 text-white rounded-lg text-xs font-medium transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
            >
              {saving ? (
                <Loader2 className="w-3 h-3 animate-spin" />
              ) : (
                <Save className="w-3 h-3" />
              )}
              {saving ? 'Saving...' : 'Save Protocol'}
            </button>
            <button
              onClick={handleBack}
              className="px-3 py-1.5 bg-zinc-800 hover:bg-zinc-700 text-zinc-400 rounded-lg text-xs transition-colors"
            >
              Cancel
            </button>
          </div>
        )}
      </div>
    );
  }

  // --- List view (default) ---
  return (
    <div className="flex flex-col h-full">
      {/* Header */}
      <div className="flex items-center justify-between px-3 py-2 border-b border-zinc-800">
        <div className="flex items-center gap-2">
          <BookOpen className="w-4 h-4 text-teal-400" />
          <span className="text-sm font-medium text-zinc-300">Protocols</span>
          <span className="text-[10px] text-zinc-600">{protocols.length}</span>
        </div>
        <div className="flex items-center gap-1">
          <button
            onClick={handleNew}
            className="p-1 rounded hover:bg-zinc-800 text-teal-400 hover:text-teal-300 transition-colors"
            title="New protocol"
          >
            <Plus className="w-3.5 h-3.5" />
          </button>
          <button
            onClick={loadProtocols}
            className="p-1 rounded hover:bg-zinc-800 text-zinc-500 hover:text-zinc-300 transition-colors"
            title="Refresh protocols"
          >
            <RefreshCw className="w-3.5 h-3.5" />
          </button>
          <button
            onClick={openProtocolsFolder}
            className="p-1 rounded hover:bg-zinc-800 text-zinc-500 hover:text-zinc-300 transition-colors"
            title="Open protocols folder"
          >
            <FolderOpen className="w-3.5 h-3.5" />
          </button>
        </div>
      </div>

      {/* Search bar */}
      <div className="px-3 py-2 border-b border-zinc-800/50">
        <div className="relative">
          <Search className="absolute left-2 top-1/2 -translate-y-1/2 w-3 h-3 text-zinc-600" />
          <input
            type="text"
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            placeholder="Search protocols..."
            className="w-full bg-zinc-900 border border-zinc-800 rounded-md pl-7 pr-7 py-1.5 text-[11px] text-zinc-300 placeholder:text-zinc-600 outline-none focus:border-zinc-600 transition-colors"
          />
          {searchQuery && (
            <button
              onClick={() => setSearchQuery('')}
              className="absolute right-2 top-1/2 -translate-y-1/2 text-zinc-600 hover:text-zinc-400"
            >
              <X className="w-3 h-3" />
            </button>
          )}
        </div>

        {/* Source filter tabs */}
        <div className="flex gap-1 mt-2">
          {([
            { key: 'all' as FilterTab, label: 'All', count: protocols.length },
            { key: 'bundled' as FilterTab, label: 'Pre-configured', count: bundledCount },
            { key: 'user' as FilterTab, label: 'My Protocols', count: userCount },
          ]).map(tab => (
            <button
              key={tab.key}
              onClick={() => setFilterTab(tab.key)}
              className={`flex items-center gap-1 px-2 py-1 rounded-md text-[10px] font-medium transition-colors ${
                filterTab === tab.key
                  ? 'bg-zinc-700 text-zinc-200'
                  : 'text-zinc-500 hover:text-zinc-300 hover:bg-zinc-800/50'
              }`}
            >
              {tab.key === 'user' && <User className="w-2.5 h-2.5" />}
              {tab.label}
              <span className={`${filterTab === tab.key ? 'text-zinc-400' : 'text-zinc-600'}`}>
                {tab.count}
              </span>
            </button>
          ))}
        </div>
      </div>

      {/* Content */}
      <div className="flex-1 overflow-y-auto">
        {loading ? (
          <div className="flex items-center justify-center py-8">
            <Loader2 className="w-5 h-5 text-zinc-500 animate-spin" />
          </div>
        ) : filteredProtocols.length === 0 ? (
          <div className="px-3 py-6 text-center">
            {searchQuery ? (
              <>
                <Search className="w-8 h-8 text-zinc-700 mx-auto mb-2" />
                <p className="text-xs text-zinc-500 mb-1">No protocols match "{searchQuery}"</p>
                <button
                  onClick={() => setSearchQuery('')}
                  className="text-[10px] text-teal-400 hover:text-teal-300"
                >
                  Clear search
                </button>
              </>
            ) : filterTab === 'user' ? (
              <>
                <User className="w-8 h-8 text-zinc-700 mx-auto mb-2" />
                <p className="text-xs text-zinc-500 mb-3">No custom protocols yet</p>
                <button
                  onClick={handleNew}
                  className="inline-flex items-center gap-1.5 px-3 py-1.5 bg-purple-600 hover:bg-purple-500 text-white rounded-lg text-xs font-medium transition-colors"
                >
                  <Sparkles className="w-3 h-3" />
                  Create Your First Protocol
                </button>
              </>
            ) : (
              <>
                <BookOpen className="w-8 h-8 text-zinc-700 mx-auto mb-2" />
                <p className="text-xs text-zinc-500 mb-3">No protocols available</p>
              </>
            )}
          </div>
        ) : (
          <div className="py-1">
            {CATEGORY_ORDER
              .filter(cat => groupedProtocols[cat]?.length)
              .map(cat => {
                const meta = CATEGORY_META[cat];
                const items = groupedProtocols[cat];
                const isCollapsed = collapsedCategories.has(cat);
                const CatIcon = meta.icon;

                return (
                  <div key={cat}>
                    {/* Category header */}
                    <button
                      onClick={() => toggleCategory(cat)}
                      className="w-full flex items-center gap-1.5 px-3 py-1.5 hover:bg-zinc-800/30 transition-colors group"
                    >
                      <ChevronRight
                        className={`w-3 h-3 text-zinc-600 transition-transform ${isCollapsed ? '' : 'rotate-90'}`}
                      />
                      <CatIcon className={`w-3 h-3 ${meta.color}`} />
                      <span className={`text-[10px] font-semibold uppercase tracking-wider ${meta.color}`}>
                        {meta.label}
                      </span>
                      <span className="text-[9px] text-zinc-600 ml-auto">
                        {items.length}
                      </span>
                    </button>

                    {/* Protocol items */}
                    {!isCollapsed && items.map(p => {
                      const isActive = activeProtocolId === p.id;
                      const isExpanded = expandedId === p.id;
                      const isDeleting = deletingId === p.id;

                      return (
                        <div key={p.id} className="border-b border-zinc-800/30">
                          {isDeleting ? (
                            <div className="flex items-center gap-2 px-3 py-2 bg-red-950/20">
                              <p className="text-[10px] text-red-300 flex-1">Delete "{p.name}"?</p>
                              <button
                                onClick={() => handleDelete(p.id)}
                                className="px-2 py-0.5 bg-red-600 hover:bg-red-500 text-white text-[10px] rounded transition-colors"
                              >
                                Delete
                              </button>
                              <button
                                onClick={() => setDeletingId(null)}
                                className="px-2 py-0.5 bg-zinc-700 hover:bg-zinc-600 text-zinc-300 text-[10px] rounded transition-colors"
                              >
                                Cancel
                              </button>
                            </div>
                          ) : (
                            <>
                              <div
                                className={`flex items-start gap-2 px-3 py-1.5 ml-2 hover:bg-zinc-800/50 transition-colors cursor-pointer ${
                                  isActive ? 'bg-teal-950/30' : ''
                                }`}
                                onContextMenu={(e) => {
                                  e.preventDefault();
                                  e.stopPropagation();
                                  setContextMenu({ x: e.clientX, y: e.clientY, protocol: p });
                                }}
                              >
                                {/* Activate button */}
                                <button
                                  onClick={() => handleActivate(p)}
                                  className={`mt-0.5 w-4.5 h-4.5 rounded flex items-center justify-center shrink-0 transition-colors ${
                                    isActive
                                      ? 'bg-teal-600 text-white'
                                      : 'bg-zinc-800 text-zinc-500 hover:bg-zinc-700 hover:text-zinc-300'
                                  }`}
                                  style={{ width: '18px', height: '18px' }}
                                  title={isActive ? 'Deactivate protocol' : 'Activate protocol'}
                                >
                                  {isActive ? <Check className="w-2.5 h-2.5" /> : null}
                                </button>

                                {/* Content */}
                                <div className="flex-1 min-w-0" onClick={() => handleTogglePreview(p)}>
                                  <div className="flex items-center gap-1.5">
                                    <span className={`text-[11px] font-medium ${isActive ? 'text-teal-300' : 'text-zinc-300'}`}>
                                      {p.name}
                                    </span>
                                    {isActive && (
                                      <span className="text-[8px] bg-teal-800/50 text-teal-300 px-1 py-0 rounded-full">
                                        active
                                      </span>
                                    )}
                                    {p.source === 'user' && (
                                      <span className="text-[8px] bg-zinc-800 text-zinc-500 px-1 py-0 rounded-full">
                                        custom
                                      </span>
                                    )}
                                  </div>
                                  <p className="text-[10px] text-zinc-500 truncate mt-0.5">
                                    {p.description}
                                    {p.is_folder && (
                                      <span className="text-zinc-600 ml-1">({p.file_count} files)</span>
                                    )}
                                  </p>
                                </div>

                                {/* Action buttons */}
                                <div className="flex items-center gap-0.5 shrink-0 mt-0.5">
                                  {p.source === 'user' && !p.is_folder && (
                                    <button
                                      onClick={() => handleEdit(p)}
                                      className="p-0.5 rounded hover:bg-zinc-700 text-zinc-600 hover:text-zinc-400 transition-colors"
                                      title="Edit protocol"
                                    >
                                      <Pencil className="w-3 h-3" />
                                    </button>
                                  )}
                                  {p.source === 'user' && (
                                    <button
                                      onClick={() => setDeletingId(p.id)}
                                      className="p-0.5 rounded hover:bg-zinc-700 text-zinc-600 hover:text-red-400 transition-colors"
                                      title="Delete protocol"
                                    >
                                      <Trash2 className="w-3 h-3" />
                                    </button>
                                  )}
                                  <button
                                    onClick={() => handleDownload(p)}
                                    className="p-0.5 rounded hover:bg-zinc-700 text-zinc-600 hover:text-zinc-400 transition-colors"
                                    title="Download protocol"
                                  >
                                    <Download className="w-3 h-3" />
                                  </button>
                                  <button
                                    onClick={() => handleTogglePreview(p)}
                                    className="p-0.5 rounded hover:bg-zinc-700 text-zinc-600 hover:text-zinc-400 transition-colors"
                                    title="Preview protocol"
                                  >
                                    <Info className="w-3 h-3" />
                                  </button>
                                </div>
                              </div>

                              {/* Expanded preview */}
                              {isExpanded && previewContent && (
                                <div className="px-3 pb-2 ml-2">
                                  <div className="bg-zinc-950 rounded border border-zinc-800 p-2 max-h-48 overflow-y-auto">
                                    <pre className="text-[10px] text-zinc-400 whitespace-pre-wrap leading-relaxed font-mono">
                                      {previewContent.slice(0, 2000)}
                                      {previewContent.length > 2000 ? '\n...' : ''}
                                    </pre>
                                  </div>
                                </div>
                              )}
                            </>
                          )}
                        </div>
                      );
                    })}
                  </div>
                );
              })}
          </div>
        )}
      </div>

      {/* Right-click context menu */}
      {contextMenu && (
        <div
          className="fixed z-[100] bg-zinc-800 border border-zinc-600 rounded-lg shadow-xl py-1 min-w-[160px]"
          style={{
            left: Math.min(contextMenu.x, window.innerWidth - 180),
            top: Math.min(contextMenu.y, window.innerHeight - 200),
          }}
        >
          <button
            onClick={() => { handleDownload(contextMenu.protocol); setContextMenu(null); }}
            className="w-full flex items-center gap-2 px-3 py-1.5 text-[12px] text-zinc-300 hover:bg-zinc-700 transition-colors text-left"
          >
            <Download className="w-3.5 h-3.5 pointer-events-none" />
            Download to Local
          </button>
          <button
            onClick={() => { handleCopyContent(contextMenu.protocol); setContextMenu(null); }}
            className="w-full flex items-center gap-2 px-3 py-1.5 text-[12px] text-zinc-300 hover:bg-zinc-700 transition-colors text-left"
          >
            <Copy className="w-3.5 h-3.5 pointer-events-none" />
            Copy to Clipboard
          </button>
          <button
            onClick={() => { handleTogglePreview(contextMenu.protocol); setContextMenu(null); }}
            className="w-full flex items-center gap-2 px-3 py-1.5 text-[12px] text-zinc-300 hover:bg-zinc-700 transition-colors text-left"
          >
            <Info className="w-3.5 h-3.5 pointer-events-none" />
            Preview
          </button>
          {contextMenu.protocol.source === 'user' && !contextMenu.protocol.is_folder && (
            <>
              <div className="border-t border-zinc-700 my-1" />
              <button
                onClick={() => { handleEdit(contextMenu.protocol); setContextMenu(null); }}
                className="w-full flex items-center gap-2 px-3 py-1.5 text-[12px] text-zinc-300 hover:bg-zinc-700 transition-colors text-left"
              >
                <Pencil className="w-3.5 h-3.5 pointer-events-none" />
                Edit
              </button>
              <button
                onClick={() => { setDeletingId(contextMenu.protocol.id); setContextMenu(null); }}
                className="w-full flex items-center gap-2 px-3 py-1.5 text-[12px] text-red-400 hover:bg-zinc-700 transition-colors text-left"
              >
                <Trash2 className="w-3.5 h-3.5 pointer-events-none" />
                Delete
              </button>
            </>
          )}
        </div>
      )}

      {/* Footer info */}
      <div className="px-3 py-2 border-t border-zinc-800">
        <p className="text-[9px] text-zinc-600 leading-relaxed">
          Custom protocols are saved to <code className="bg-zinc-800 px-1 rounded">~/.operon/protocols/</code>.
          You can also place <code className="bg-zinc-800 px-1 rounded">.md</code> files there directly.
        </p>
      </div>
    </div>
  );
}
