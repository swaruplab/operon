import { useState, useEffect, useMemo } from 'react';
import { copyText } from '../../lib/clipboard';
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
  Microscope,
  Pill,
  Activity,
  Network,
  Image as ImageIcon,
  Workflow,
  Atom,
  Globe,
  Heart,
  Shield,
  Bug,
  Telescope,
  ScanLine,
  Bot,
  Copy,
  Clock,
} from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { emit } from '@tauri-apps/api/event';
import { useProject } from '../../context/ProjectContext';
import { scanProjectFiles, scanRemoteProjectFiles, batchReadFilePreviews, batchReadRemoteFilePreviews } from '../../lib/report';
import { ReportFileSelector } from '../report/ReportFileSelector';
import type { ProjectScan, ScanTreeNode, ScannedFile } from '../../types/report';
import {
  searchProtocols,
  loadRecentProtocolIds,
  pushRecentProtocolId,
  loadActiveCategories,
  saveActiveCategories,
} from '../../lib/protocolSearch';

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

interface SSHConnection {
  profileId: string;
  profileName: string;
  terminalId: string;
}

interface ProtocolsViewProps {
  activeProtocolIds: string[];
  onToggle: (protocol: { id: string; name: string }, allActive: { id: string; name: string }[]) => void;
  sshConnection?: SSHConnection | null;
  remotePath?: string;
}

// Max active protocols injected into the agent's system prompt at once.
// Each adds its SKILL.md to every turn (~3-10k tokens per protocol). Bumping
// this higher inflates context cost and dilutes the agent's attention —
// 4 is the recommended ceiling.
const MAX_ACTIVE_PROTOCOLS = 4;

type ViewMode = 'list' | 'create' | 'edit' | 'import';
type CreateTab = 'generate' | 'manual';
type FilterTab = 'all' | 'user' | 'bundled';

const CATEGORY_META: Record<string, { label: string; icon: typeof Database; color: string }> = {
  // Wet-lab data analysis (assay-specific) ─────────────────────────────
  single_cell:         { label: 'Single-cell',                  icon: Dna,          color: 'text-green-600 dark:text-green-400' },
  spatial:             { label: 'Spatial transcriptomics',      icon: Microscope,   color: 'text-emerald-600 dark:text-emerald-400' },
  chromatin:           { label: 'Chromatin (ATAC, ChIP, Hi-C)', icon: Network,      color: 'text-teal-600 dark:text-teal-400' },
  rna:                 { label: 'Bulk RNA & isoforms',          icon: Activity,     color: 'text-lime-600 dark:text-lime-400' },
  crispr:              { label: 'CRISPR & genome engineering',  icon: Workflow,     color: 'text-yellow-600 dark:text-yellow-400' },
  cytometry:           { label: 'Flow cytometry',               icon: ScanLine,     color: 'text-amber-600 dark:text-amber-400' },
  epigenetics:         { label: 'Epigenetics & RNA mod.',       icon: Layers,       color: 'text-orange-600 dark:text-orange-400' },
  immunology:          { label: 'Immunology (TCR/BCR, abx)',    icon: Shield,       color: 'text-rose-600 dark:text-rose-400' },
  microbiome:          { label: 'Microbiome & metagenomics',    icon: Bug,          color: 'text-fuchsia-600 dark:text-fuchsia-400' },
  liquid_biopsy:       { label: 'Liquid biopsy (cfDNA/ctDNA)',  icon: Heart,        color: 'text-pink-600 dark:text-pink-400' },
  // Genomics broader ───────────────────────────────────────────────────
  population:          { label: 'Variants & population gen.',   icon: Telescope,    color: 'text-sky-600 dark:text-sky-400' },
  copy_number:         { label: 'Copy number',                  icon: BarChart3,    color: 'text-blue-600 dark:text-blue-400' },
  genome_assembly:     { label: 'Genome assembly & long-read',  icon: GitBranch,    color: 'text-indigo-600 dark:text-indigo-400' },
  phylogenetics:       { label: 'Phylogenetics',                icon: Network,      color: 'text-violet-600 dark:text-violet-400' },
  bio_tools:           { label: 'Sequence I/O & alignment',     icon: Package,      color: 'text-purple-600 dark:text-purple-400' },
  // Structural / chemical / pathways ──────────────────────────────────
  proteomics_structure:{ label: 'Proteomics & structural bio',  icon: Atom,         color: 'text-cyan-600 dark:text-cyan-400' },
  drug_discovery:      { label: 'Drug discovery & chem',        icon: Pill,         color: 'text-pink-700 dark:text-pink-300' },
  metabolomics:        { label: 'Metabolomics',                 icon: FlaskConical, color: 'text-emerald-700 dark:text-emerald-300' },
  systems_biology:     { label: 'Systems biology & pathways',   icon: Network,      color: 'text-blue-700 dark:text-blue-300' },
  // Clinical ──────────────────────────────────────────────────────────
  clinical:            { label: 'Clinical & healthcare',        icon: Stethoscope,  color: 'text-red-600 dark:text-red-400' },
  medical_imaging:     { label: 'Medical imaging & pathology',  icon: ImageIcon,    color: 'text-red-700 dark:text-red-300' },
  // Lab & data infrastructure ─────────────────────────────────────────
  lab_automation:      { label: 'Lab automation & platforms',   icon: Plug,         color: 'text-indigo-700 dark:text-indigo-300' },
  databases:           { label: 'Databases & references',       icon: Database,     color: 'text-blue-600 dark:text-blue-400' },
  // Cross-cutting / agentic ───────────────────────────────────────────
  bio_agents:          { label: 'Bio agents & multi-tool',      icon: Bot,          color: 'text-purple-700 dark:text-purple-300' },
  ml_compute:          { label: 'ML & scientific computing',    icon: Brain,        color: 'text-purple-600 dark:text-purple-400' },
  statistics:          { label: 'Statistics & data science',    icon: TrendingUp,   color: 'text-orange-600 dark:text-orange-400' },
  visualization:       { label: 'Visualization & plotting',     icon: BarChart3,    color: 'text-amber-600 dark:text-amber-400' },
  writing:             { label: 'Writing & documents',          icon: PenTool,      color: 'text-cyan-700 dark:text-cyan-300' },
  research:            { label: 'Research & reasoning',         icon: Lightbulb,    color: 'text-yellow-600 dark:text-yellow-400' },
  // Legacy slots — for any pre-existing protocol still tagged with these
  finance:             { label: 'Finance & business',           icon: DollarSign,   color: 'text-emerald-600 dark:text-emerald-400' },
  integration:         { label: 'Lab platforms & integrations', icon: Plug,         color: 'text-indigo-600 dark:text-indigo-400' },
  genomics:            { label: 'Genomics & omics',             icon: Dna,          color: 'text-green-600 dark:text-green-400' },
  cheminformatics:     { label: 'Cheminformatics',              icon: FlaskConical, color: 'text-pink-600 dark:text-pink-400' },
  ml_ai:               { label: 'ML & AI',                      icon: Brain,        color: 'text-purple-600 dark:text-purple-400' },
  pipeline:            { label: 'Pipelines',                    icon: GitBranch,    color: 'text-teal-600 dark:text-teal-400' },
  tool:                { label: 'Tools & packages',             icon: Package,      color: 'text-violet-600 dark:text-violet-400' },
  database:            { label: 'Databases & references',       icon: Database,     color: 'text-blue-600 dark:text-blue-400' },
  other:               { label: 'Other',                        icon: Layers,       color: 'text-secondary' },
};

const CATEGORY_ORDER = [
  // Wet-lab assays (most common starting points)
  'single_cell', 'spatial', 'chromatin', 'rna', 'crispr',
  'cytometry', 'epigenetics', 'immunology', 'microbiome', 'liquid_biopsy',
  // Broader genomics
  'population', 'copy_number', 'genome_assembly', 'phylogenetics', 'bio_tools',
  // Structural & chemical
  'proteomics_structure', 'drug_discovery', 'metabolomics', 'systems_biology',
  // Clinical
  'clinical', 'medical_imaging',
  // Lab & data infrastructure
  'lab_automation', 'databases',
  // Cross-cutting / agentic
  'bio_agents', 'ml_compute', 'statistics', 'visualization', 'writing', 'research',
  // Legacy buckets (shown if any pre-existing protocol still uses them)
  'genomics', 'cheminformatics', 'ml_ai', 'integration', 'pipeline', 'tool', 'database', 'finance',
  // Fallback
  'other',
];

export function ProtocolsView({ activeProtocolIds, onToggle, sshConnection, remotePath: remotePathProp }: ProtocolsViewProps) {
  const [protocols, setProtocols] = useState<ProtocolEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const [previewContent, setPreviewContent] = useState<string | null>(null);

  // Search & filter
  const [searchQuery, setSearchQuery] = useState('');
  const [filterTab, setFilterTab] = useState<FilterTab>('all');
  const [collapsedCategories, setCollapsedCategories] = useState<Set<string>>(new Set());
  const [activeCategories, setActiveCategories] = useState<Set<string>>(() => loadActiveCategories());
  const [recentIds, setRecentIds] = useState<string[]>(() => loadRecentProtocolIds());

  useEffect(() => {
    saveActiveCategories(activeCategories);
  }, [activeCategories]);

  const toggleActiveCategory = (cat: string) => {
    setActiveCategories(prev => {
      const next = new Set(prev);
      if (next.has(cat)) next.delete(cat);
      else next.add(cat);
      return next;
    });
  };

  const recordRecent = (id: string) => {
    setRecentIds(pushRecentProtocolId(id));
  };

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

  // Import from project state
  const { projectPath } = useProject();
  const [importPhase, setImportPhase] = useState<'scan' | 'select' | 'context' | 'generate' | 'review'>('scan');
  const [importScan, setImportScan] = useState<ProjectScan | null>(null);
  const [importSelectedFiles, setImportSelectedFiles] = useState<string[]>([]);
  const [importContext, setImportContext] = useState('');
  const [importGenerating, setImportGenerating] = useState(false);
  const [importError, setImportError] = useState<string | null>(null);
  const remotePath = remotePathProp || '';

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
      await copyText(content);
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

  // Distinct categories present in the loaded catalog, in canonical order.
  const availableCategories = useMemo(() => {
    const present = new Set<string>();
    for (const p of protocols) present.add(p.category || 'other');
    const ordered = CATEGORY_ORDER.filter(c => present.has(c));
    for (const c of present) if (!ordered.includes(c)) ordered.push(c);
    return ordered;
  }, [protocols]);

  // --- Filtered & grouped protocols ---
  const filteredProtocols = useMemo(() => {
    let list = protocols;

    if (filterTab === 'user') {
      list = list.filter(p => p.source === 'user');
    } else if (filterTab === 'bundled') {
      list = list.filter(p => p.source === 'bundled');
    }

    // A search query searches the WHOLE catalog — the category chips are a
    // browse filter, not a search filter. Honoring them during search silently
    // hides matching protocols (e.g. searching "spatial" while a different chip
    // is selected returns nothing), which reads as "search is broken".
    if (searchQuery.trim()) {
      // Attach the human-readable category label so a query like "spatial"
      // surfaces every protocol in that category, not just name/desc matches.
      const withLabel = list.map((p) => ({
        ...p,
        categoryLabel: CATEGORY_META[p.category || 'other']?.label,
      }));
      return searchProtocols(withLabel, searchQuery);
    }

    if (activeCategories.size > 0) {
      list = list.filter(p => activeCategories.has(p.category || 'other'));
    }

    return list;
  }, [protocols, filterTab, searchQuery, activeCategories]);

  const hasActiveSearch = searchQuery.trim().length > 0;

  const recentProtocols = useMemo(() => {
    if (hasActiveSearch) return [];
    const byId = new Map(protocols.map(p => [p.id, p] as const));
    return recentIds
      .map(id => byId.get(id))
      .filter((p): p is ProtocolEntry => Boolean(p));
  }, [recentIds, protocols, hasActiveSearch]);

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
    recordRecent(p.id);
    try {
      const content = await invoke<string>('read_protocol', { protocolId: p.id });
      setPreviewContent(content);
    } catch {
      setPreviewContent('(Could not read protocol file)');
    }
  };

  const handleActivate = (p: ProtocolEntry) => {
    const isActive = activeProtocolIds.includes(p.id);
    if (!isActive) recordRecent(p.id);
    let newActive: { id: string; name: string }[];
    if (isActive) {
      // Deactivate this protocol
      newActive = protocols
        .filter((proto) => activeProtocolIds.includes(proto.id) && proto.id !== p.id)
        .map((proto) => ({ id: proto.id, name: proto.name }));
    } else {
      if (activeProtocolIds.length >= MAX_ACTIVE_PROTOCOLS) return; // Enforce limit
      // Activate this protocol alongside existing ones
      const existing = protocols
        .filter((proto) => activeProtocolIds.includes(proto.id))
        .map((proto) => ({ id: proto.id, name: proto.name }));
      newActive = [...existing, { id: p.id, name: p.name }];
    }
    onToggle({ id: p.id, name: p.name }, newActive);
  };

  // Auto-select pipeline-relevant files from a scan tree
  const PIPELINE_EXTENSIONS = new Set([
    'py', 'r', 'R', 'sh', 'bash', 'pl', 'slurm', 'sbatch', 'pbs',
    'smk', 'nf', 'wdl', 'cwl',
    'yaml', 'yml', 'json', 'toml', 'cfg', 'ini', 'env', 'config',
    'md', 'txt', 'rst',
    'def', 'dockerfile',
  ]);
  const PIPELINE_FILENAMES = new Set([
    'snakefile', 'makefile', 'dockerfile', 'nextflow.config',
    'readme', 'readme.md', 'readme.txt', 'methods', 'methods.md',
  ]);
  const DATA_EXTENSIONS = new Set([
    'fastq', 'fq', 'bam', 'sam', 'cram', 'h5', 'hdf5', 'h5ad',
    'csv', 'tsv', 'bed', 'vcf', 'bcf', 'bigwig', 'bw', 'gz', 'zip',
    'tar', 'png', 'jpg', 'jpeg', 'gif', 'pdf', 'svg', 'tif', 'tiff',
  ]);

  const getAllScanFiles = (node: ScanTreeNode): ScannedFile[] => {
    const files = [...node.files];
    for (const child of node.children) files.push(...getAllScanFiles(child));
    return files;
  };

  const autoSelectPipelineFiles = (scan: ProjectScan): string[] => {
    const allFiles = getAllScanFiles(scan.root);
    return allFiles
      .filter((f) => {
        const ext = f.name.split('.').pop()?.toLowerCase() || '';
        const nameLower = f.name.toLowerCase();
        if (DATA_EXTENSIONS.has(ext)) return false;
        if (f.size > 1024 * 1024) return false; // Skip files > 1MB
        return PIPELINE_EXTENSIONS.has(ext) || PIPELINE_FILENAMES.has(nameLower);
      })
      .map((f) => f.path);
  };

  const isRemote = !!sshConnection;

  const handleImport = async () => {
    const scanPath = isRemote ? remotePath : projectPath;
    if (!scanPath) {
      setImportError(isRemote ? 'No remote path set. Navigate to a folder on the server first.' : 'No project folder open. Open a folder first.');
      setViewMode('import');
      setImportPhase('select');
      return;
    }
    setViewMode('import');
    setImportPhase('scan');
    setImportError(null);
    setImportContext('');
    setProtocolName('');
    setProtocolContent('');

    try {
      const scan = isRemote
        ? await scanRemoteProjectFiles(sshConnection!.profileId, scanPath)
        : await scanProjectFiles(scanPath);
      setImportScan(scan);
      setImportSelectedFiles(autoSelectPipelineFiles(scan));
      setImportPhase('select');
    } catch (err) {
      setImportError(`Scan failed: ${err}`);
    }
  };

  const handleImportGenerate = async () => {
    if (importSelectedFiles.length === 0) return;
    setImportPhase('generate');
    setImportGenerating(true);
    setImportError(null);

    try {
      // Read file contents (local or remote)
      const previews = isRemote
        ? await batchReadRemoteFilePreviews(sshConnection!.profileId, importSelectedFiles)
        : await batchReadFilePreviews(importSelectedFiles);
      const fileContents: [string, string][] = previews
        .filter((p) => !p.error && p.content)
        .map((p) => [p.name, p.content]);

      // Generate protocol via Claude
      const result = await invoke<string>('generate_protocol_from_files', {
        fileContents,
        context: importContext || null,
      });

      // Extract name from first H1 header
      const nameMatch = result.match(/^#\s+(.+)/m);
      setProtocolName(nameMatch ? nameMatch[1].trim() : 'Imported Pipeline');
      setProtocolContent(result);
      setImportPhase('review');
    } catch (err) {
      setImportError(`Generation failed: ${err}`);
      setImportPhase('select');
    }
    setImportGenerating(false);
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
      if (activeProtocolIds.includes(protocolId)) {
        // Deactivate the deleted protocol
        const remaining = protocols
          .filter((p) => activeProtocolIds.includes(p.id) && p.id !== protocolId)
          .map((p) => ({ id: p.id, name: p.name }));
        onToggle({ id: protocolId, name: '' }, remaining);
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

  // Import from Project view
  if (viewMode === 'import') {
    return (
      <div className="flex flex-col h-full">
        {/* Header */}
        <div className="flex items-center gap-2 px-3 py-2 border-b border-border-default">
          <button
            onClick={() => setViewMode('list')}
            className="p-1 rounded hover:bg-hover text-secondary hover:text-primary"
          >
            <ArrowLeft className="w-3.5 h-3.5" />
          </button>
          <span className="text-[11px] font-semibold text-secondary">Import Pipeline as Protocol</span>
          {isRemote && (
            <span className="text-[9px] bg-teal-900/40 text-teal-600 dark:text-teal-400 px-1.5 py-0.5 rounded-full ml-auto">
              {sshConnection?.profileName}
            </span>
          )}
        </div>

        <div className="flex-1 overflow-y-auto p-3 space-y-3">
          {importPhase === 'scan' && (
            <div className="flex flex-col items-center justify-center py-8">
              <Loader2 className="w-5 h-5 text-teal-600 dark:text-teal-400 animate-spin mb-2" />
              <p className="text-xs text-secondary">Scanning project files...</p>
            </div>
          )}

          {importPhase === 'select' && importScan && (
            <>
              <p className="text-[11px] text-secondary leading-relaxed">
                Select the scripts, configs, and workflow files that define your pipeline.
                Data files (FASTQ, BAM, H5, etc.) are dimmed — they are not needed for the protocol.
              </p>
              <ReportFileSelector
                scan={importScan}
                selectedFiles={importSelectedFiles}
                onSelectionChange={setImportSelectedFiles}
                maxFiles={50}
                maxSize={20 * 1024 * 1024}
                headerLabel="Select pipeline files"
                headerStats={`${importSelectedFiles.length} selected · ${importScan.total_code} code files in project`}
                tipText="Scripts, configs, and docs are auto-selected. Deselect any that aren't relevant."
              />
              {importError && (
                <p className="text-[10px] text-red-600 dark:text-red-400">{importError}</p>
              )}
              <button
                onClick={() => setImportPhase('context')}
                disabled={importSelectedFiles.length === 0}
                className="w-full py-2 bg-teal-600 hover:bg-teal-500 disabled:bg-elevated disabled:text-muted text-white text-xs font-medium rounded transition-colors"
              >
                Continue ({importSelectedFiles.length} files selected)
              </button>
            </>
          )}

          {importPhase === 'context' && (
            <>
              <p className="text-[11px] text-secondary leading-relaxed">
                Optionally describe what this pipeline does. This helps Claude generate a better protocol.
              </p>
              <textarea
                value={importContext}
                onChange={(e) => setImportContext(e.target.value)}
                placeholder="e.g., Bulk RNA-seq pipeline from FASTQ to differential expression, using STAR + DESeq2 on a SLURM cluster..."
                className="w-full h-24 bg-surface border border-border-strong rounded px-3 py-2 text-xs text-primary placeholder:text-subtle resize-none focus:outline-none focus:border-teal-600"
              />
              <div className="flex gap-2">
                <button
                  onClick={() => setImportPhase('select')}
                  className="flex-1 py-2 bg-surface hover:bg-elevated text-secondary text-xs font-medium rounded transition-colors"
                >
                  Back
                </button>
                <button
                  onClick={handleImportGenerate}
                  className="flex-1 py-2 bg-teal-600 hover:bg-teal-500 text-white text-xs font-medium rounded transition-colors flex items-center justify-center gap-1.5"
                >
                  <Sparkles className="w-3 h-3" />
                  Generate Protocol
                </button>
              </div>
            </>
          )}

          {importPhase === 'generate' && (
            <div className="flex flex-col items-center justify-center py-8">
              <Loader2 className="w-5 h-5 text-teal-600 dark:text-teal-400 animate-spin mb-2" />
              <p className="text-xs text-secondary">Claude is analyzing your pipeline...</p>
              <p className="text-[10px] text-subtle mt-1">This may take 30–60 seconds</p>
            </div>
          )}

          {importPhase === 'review' && (
            <>
              <p className="text-[11px] text-green-600 dark:text-green-400 font-medium">Protocol generated! Review and save:</p>
              {importError && (
                <p className="text-[10px] text-red-600 dark:text-red-400">{importError}</p>
              )}
              <div>
                <label className="text-[10px] text-muted block mb-1">Protocol name</label>
                <input
                  type="text"
                  value={protocolName}
                  onChange={(e) => setProtocolName(e.target.value)}
                  className="w-full bg-surface border border-border-strong rounded px-3 py-1.5 text-xs text-primary focus:outline-none focus:border-teal-600"
                />
              </div>
              <div>
                <label className="text-[10px] text-muted block mb-1">Protocol content</label>
                <textarea
                  value={protocolContent}
                  onChange={(e) => setProtocolContent(e.target.value)}
                  className="w-full h-64 bg-surface border border-border-strong rounded px-3 py-2 text-xs text-primary font-mono resize-none focus:outline-none focus:border-teal-600"
                />
              </div>
              <div className="flex gap-2">
                <button
                  onClick={handleImportGenerate}
                  disabled={importGenerating}
                  className="flex-1 py-2 bg-surface hover:bg-elevated text-secondary text-xs font-medium rounded transition-colors"
                >
                  Regenerate
                </button>
                <button
                  onClick={async () => {
                    if (!protocolName.trim() || !protocolContent.trim()) return;
                    setSaving(true);
                    setSaveError(null);
                    try {
                      const id = protocolName.trim().toLowerCase().replace(/[^a-z0-9]+/g, '-');
                      await invoke('save_protocol', { protocolId: id, content: protocolContent });
                      await loadProtocols();
                      setViewMode('list');
                    } catch (e) {
                      setSaveError(String(e));
                    }
                    setSaving(false);
                  }}
                  disabled={saving || !protocolName.trim() || !protocolContent.trim()}
                  className="flex-1 py-2 bg-teal-600 hover:bg-teal-500 disabled:bg-elevated disabled:text-muted text-white text-xs font-medium rounded transition-colors flex items-center justify-center gap-1.5"
                >
                  {saving ? <Loader2 className="w-3 h-3 animate-spin" /> : <Save className="w-3 h-3" />}
                  Save Protocol
                </button>
              </div>
              {saveError && (
                <p className="text-[10px] text-red-600 dark:text-red-400">{saveError}</p>
              )}
            </>
          )}
        </div>
      </div>
    );
  }

  // Create / Edit view
  if (viewMode === 'create' || viewMode === 'edit') {
    return (
      <div className="flex flex-col h-full">
        {/* Header */}
        <div className="flex items-center gap-2 px-3 py-2 border-b border-border-default">
          <button
            onClick={handleBack}
            className="p-1 rounded hover:bg-hover text-muted hover:text-secondary transition-colors"
            title="Back to list"
          >
            <ArrowLeft className="w-3.5 h-3.5" />
          </button>
          <span className="text-sm font-medium text-secondary">
            {viewMode === 'edit' ? 'Edit Protocol' : 'New Protocol'}
          </span>
        </div>

        {/* Tab selector (only in create mode) */}
        {viewMode === 'create' && (
          <div className="flex border-b border-border-default">
            <button
              onClick={() => setCreateTab('generate')}
              className={`flex-1 flex items-center justify-center gap-1.5 px-3 py-2 text-[11px] font-medium transition-colors ${
                createTab === 'generate'
                  ? 'text-purple-700 dark:text-purple-300 border-b-2 border-purple-500 bg-purple-950/20'
                  : 'text-muted hover:text-secondary'
              }`}
            >
              <Wand2 className="w-3 h-3" />
              Generate with AI
            </button>
            <button
              onClick={() => setCreateTab('manual')}
              className={`flex-1 flex items-center justify-center gap-1.5 px-3 py-2 text-[11px] font-medium transition-colors ${
                createTab === 'manual'
                  ? 'text-teal-700 dark:text-teal-300 border-b-2 border-teal-500 bg-teal-950/20'
                  : 'text-muted hover:text-secondary'
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
                <label className="text-[10px] text-muted font-medium uppercase tracking-wider block mb-1.5">
                  Describe the protocol you need
                </label>
                <textarea
                  value={aiDescription}
                  onChange={(e) => setAiDescription(e.target.value)}
                  placeholder="e.g., Single-cell RNA-seq analysis using Scanpy with SLURM job submission for a cluster with GPU and CPU partitions..."
                  className="w-full h-28 bg-panel border border-border-strong rounded-lg px-3 py-2 text-xs text-primary placeholder:text-subtle outline-none focus:border-purple-600 resize-none leading-relaxed"
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
                <p className="text-[10px] text-subtle text-center">
                  Claude is writing your protocol. This may take 15-30 seconds.
                </p>
              )}

              {generateError && (
                <div className="p-2 bg-red-950/20 border border-red-900/30 rounded-lg">
                  <p className="text-[10px] text-red-700 dark:text-red-300">{generateError}</p>
                </div>
              )}
            </div>
          )}

          {/* Manual / Review tab */}
          {(createTab === 'manual' || viewMode === 'edit') && (
            <div className="space-y-3">
              <div>
                <label className="text-[10px] text-muted font-medium uppercase tracking-wider block mb-1.5">
                  Protocol Name
                </label>
                <input
                  type="text"
                  value={protocolName}
                  onChange={(e) => setProtocolName(e.target.value)}
                  placeholder="e.g., scRNA-seq Scanpy Pipeline"
                  className="w-full bg-panel border border-border-strong rounded-lg px-3 py-2 text-xs text-primary placeholder:text-subtle outline-none focus:border-teal-600"
                  disabled={viewMode === 'edit'}
                />
              </div>

              <div>
                <label className="text-[10px] text-muted font-medium uppercase tracking-wider block mb-1.5">
                  Protocol Content (Markdown)
                </label>
                <textarea
                  value={protocolContent}
                  onChange={(e) => setProtocolContent(e.target.value)}
                  placeholder={"# My Protocol\n\nDescribe the rules, tools, and patterns Claude should follow...\n\n## Tools & Packages\n- ...\n\n## Workflow\n1. ..."}
                  className="w-full h-64 bg-panel border border-border-strong rounded-lg px-3 py-2 text-[11px] text-primary placeholder:text-subtle outline-none focus:border-teal-600 resize-none font-mono leading-relaxed"
                  autoFocus={viewMode === 'edit'}
                />
              </div>

              {protocolContent && viewMode === 'create' && createTab === 'manual' && aiDescription && (
                <p className="text-[9px] text-subtle">
                  Generated by Claude — review and edit before saving.
                </p>
              )}

              {saveError && (
                <div className="p-2 bg-red-950/20 border border-red-900/30 rounded-lg">
                  <p className="text-[10px] text-red-700 dark:text-red-300">{saveError}</p>
                </div>
              )}
            </div>
          )}
        </div>

        {/* Footer with save/cancel */}
        {(createTab === 'manual' || viewMode === 'edit') && (
          <div className="flex items-center gap-2 px-3 py-2 border-t border-border-default">
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
              className="px-3 py-1.5 bg-surface hover:bg-elevated text-secondary rounded-lg text-xs transition-colors"
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
      <div className="flex items-center justify-between px-3 py-2 border-b border-border-default">
        <div className="flex items-center gap-2">
          <BookOpen className="w-4 h-4 text-teal-600 dark:text-teal-400" />
          <span className="text-sm font-medium text-secondary">Protocols</span>
          <span className="text-[10px] text-subtle">{protocols.length}</span>
        </div>
        <div className="flex items-center gap-1">
          <button
            onClick={handleImport}
            className="p-1 rounded hover:bg-hover text-teal-600 dark:text-teal-400 hover:text-teal-800 dark:hover:text-teal-700 transition-colors"
            title="Import pipeline as protocol"
          >
            <FolderOpen className="w-3.5 h-3.5" />
          </button>
          <button
            onClick={handleNew}
            className="p-1 rounded hover:bg-hover text-teal-600 dark:text-teal-400 hover:text-teal-800 dark:hover:text-teal-700 transition-colors"
            title="New protocol"
          >
            <Plus className="w-3.5 h-3.5" />
          </button>
          <button
            onClick={loadProtocols}
            className="p-1 rounded hover:bg-hover text-muted hover:text-secondary transition-colors"
            title="Refresh protocols"
          >
            <RefreshCw className="w-3.5 h-3.5" />
          </button>
          <button
            onClick={openProtocolsFolder}
            className="p-1 rounded hover:bg-hover text-muted hover:text-secondary transition-colors"
            title="Open protocols folder"
          >
            <FolderOpen className="w-3.5 h-3.5" />
          </button>
        </div>
      </div>

      {/* Search bar */}
      <div className="px-3 py-2 border-b border-border-default/50">
        <div className="relative">
          <Search className="absolute left-2 top-1/2 -translate-y-1/2 w-3 h-3 text-subtle" />
          <input
            type="text"
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            placeholder="Search protocols..."
            className="w-full bg-panel border border-border-default rounded-md pl-7 pr-7 py-1.5 text-[11px] text-secondary placeholder:text-subtle outline-none focus:border-border-strong transition-colors"
          />
          {searchQuery && (
            <button
              onClick={() => setSearchQuery('')}
              className="absolute right-2 top-1/2 -translate-y-1/2 text-subtle hover:text-secondary"
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
                  ? 'bg-elevated text-primary'
                  : 'text-muted hover:text-secondary hover:bg-hover/50'
              }`}
            >
              {tab.key === 'user' && <User className="w-2.5 h-2.5" />}
              {tab.label}
              <span className={`${filterTab === tab.key ? 'text-secondary' : 'text-subtle'}`}>
                {tab.count}
              </span>
            </button>
          ))}
        </div>

        {/* Category filter chips */}
        {availableCategories.length > 0 && (
          <div className="flex flex-wrap gap-1 mt-2">
            {availableCategories.map(cat => {
              const meta = CATEGORY_META[cat] ?? CATEGORY_META.other;
              const isOn = activeCategories.has(cat);
              return (
                <button
                  key={cat}
                  onClick={() => toggleActiveCategory(cat)}
                  className={`px-1.5 py-0.5 rounded-full text-[9px] font-medium transition-colors ${
                    isOn
                      ? 'bg-blue-600 text-white'
                      : 'bg-surface text-muted hover:bg-elevated hover:text-secondary'
                  }`}
                  title={meta.label}
                >
                  {meta.label}
                </button>
              );
            })}
            {activeCategories.size > 0 && (
              <button
                onClick={() => setActiveCategories(new Set())}
                className="px-1.5 py-0.5 rounded-full text-[9px] font-medium text-subtle hover:text-secondary"
              >
                clear
              </button>
            )}
          </div>
        )}
      </div>

      {/* Limit warning */}
      {activeProtocolIds.length >= MAX_ACTIVE_PROTOCOLS && (
        <div className="px-3 py-1.5 bg-amber-950/30 border-b border-amber-800/30">
          <p className="text-[10px] text-amber-600 dark:text-amber-400">
            Maximum {MAX_ACTIVE_PROTOCOLS} protocols active. Deactivate one to select another.
          </p>
          <p className="text-[9px] text-muted mt-0.5">
            Using too many protocols at once consumes context and may reduce response quality.
          </p>
        </div>
      )}

      {/* Content */}
      <div className="flex-1 overflow-y-auto">
        {loading ? (
          <div className="flex items-center justify-center py-8">
            <Loader2 className="w-5 h-5 text-muted animate-spin" />
          </div>
        ) : filteredProtocols.length === 0 ? (
          <div className="px-3 py-6 text-center">
            {searchQuery ? (
              <>
                <Search className="w-8 h-8 text-subtle mx-auto mb-2" />
                <p className="text-xs text-muted mb-1">No protocols match "{searchQuery}"</p>
                <button
                  onClick={() => setSearchQuery('')}
                  className="text-[10px] text-teal-600 dark:text-teal-400 hover:text-teal-800 dark:hover:text-teal-700"
                >
                  Clear search
                </button>
              </>
            ) : filterTab === 'user' ? (
              <>
                <User className="w-8 h-8 text-subtle mx-auto mb-2" />
                <p className="text-xs text-muted mb-3">No custom protocols yet</p>
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
                <BookOpen className="w-8 h-8 text-subtle mx-auto mb-2" />
                <p className="text-xs text-muted mb-3">No protocols available</p>
              </>
            )}
          </div>
        ) : (
          (() => {
            const renderItem = (p: ProtocolEntry) => {
              const isActive = activeProtocolIds.includes(p.id);
              const atLimit = activeProtocolIds.length >= MAX_ACTIVE_PROTOCOLS && !isActive;
              const isExpanded = expandedId === p.id;
              const isDeleting = deletingId === p.id;

              return (
                <div key={p.id} className="border-b border-border-default/30">
                  {isDeleting ? (
                    <div className="flex items-center gap-2 px-3 py-2 bg-red-950/20">
                      <p className="text-[10px] text-red-700 dark:text-red-300 flex-1">Delete "{p.name}"?</p>
                      <button
                        onClick={() => handleDelete(p.id)}
                        className="px-2 py-0.5 bg-red-600 hover:bg-red-500 text-white text-[10px] rounded transition-colors"
                      >
                        Delete
                      </button>
                      <button
                        onClick={() => setDeletingId(null)}
                        className="px-2 py-0.5 bg-elevated hover:bg-elevated text-secondary text-[10px] rounded transition-colors"
                      >
                        Cancel
                      </button>
                    </div>
                  ) : (
                    <>
                      <div
                        className={`flex items-start gap-2 px-3 py-1.5 ml-2 hover:bg-hover/50 transition-colors cursor-pointer ${
                          isActive ? 'bg-teal-950/30' : ''
                        }`}
                        onContextMenu={(e) => {
                          e.preventDefault();
                          e.stopPropagation();
                          setContextMenu({ x: e.clientX, y: e.clientY, protocol: p });
                        }}
                      >
                        <button
                          onClick={() => handleActivate(p)}
                          disabled={atLimit}
                          className={`mt-0.5 w-4.5 h-4.5 rounded flex items-center justify-center shrink-0 transition-colors ${
                            isActive
                              ? 'bg-teal-600 text-white'
                              : atLimit
                                ? 'bg-surface/50 text-subtle cursor-not-allowed'
                                : 'bg-surface text-muted hover:bg-elevated hover:text-secondary'
                          }`}
                          style={{ width: '18px', height: '18px' }}
                          title={isActive ? 'Remove from chat' : 'Add to chat'}
                        >
                          {isActive ? <Check className="w-2.5 h-2.5" /> : null}
                        </button>

                        <div className="flex-1 min-w-0" onClick={() => handleTogglePreview(p)}>
                          <div className="flex items-center gap-1.5">
                            <span className={`text-[11px] font-medium ${isActive ? 'text-teal-700 dark:text-teal-300' : 'text-secondary'}`}>
                              {p.name}
                            </span>
                            {isActive && (
                              <span className="text-[8px] bg-teal-800/50 text-teal-700 dark:text-teal-300 px-1 py-0 rounded-full">
                                active
                              </span>
                            )}
                            {p.source === 'user' && (
                              <span className="text-[8px] bg-surface text-muted px-1 py-0 rounded-full">
                                custom
                              </span>
                            )}
                          </div>
                          <p className="text-[10px] text-muted truncate mt-0.5">
                            {p.description}
                            {p.is_folder && (
                              <span className="text-subtle ml-1">({p.file_count} files)</span>
                            )}
                          </p>
                        </div>

                        <div className="flex items-center gap-0.5 shrink-0 mt-0.5">
                          {p.source === 'user' && !p.is_folder && (
                            <button
                              onClick={() => handleEdit(p)}
                              className="p-0.5 rounded hover:bg-elevated text-subtle hover:text-secondary transition-colors"
                              title="Edit protocol"
                            >
                              <Pencil className="w-3 h-3" />
                            </button>
                          )}
                          {p.source === 'user' && (
                            <button
                              onClick={() => setDeletingId(p.id)}
                              className="p-0.5 rounded hover:bg-elevated text-subtle hover:text-red-700 dark:hover:text-red-600 transition-colors"
                              title="Delete protocol"
                            >
                              <Trash2 className="w-3 h-3" />
                            </button>
                          )}
                          <button
                            onClick={() => handleDownload(p)}
                            className="p-0.5 rounded hover:bg-elevated text-subtle hover:text-secondary transition-colors"
                            title="Download protocol"
                          >
                            <Download className="w-3 h-3" />
                          </button>
                          <button
                            onClick={() => handleTogglePreview(p)}
                            className="p-0.5 rounded hover:bg-elevated text-subtle hover:text-secondary transition-colors"
                            title="Preview protocol"
                          >
                            <Info className="w-3 h-3" />
                          </button>
                        </div>
                      </div>

                      {isExpanded && (
                        <div className="px-3 pb-2 ml-2 space-y-2">
                          {previewContent && (
                            <div className="bg-canvas rounded border border-border-default p-2 max-h-48 overflow-y-auto">
                              <pre className="text-[10px] text-secondary whitespace-pre-wrap leading-relaxed font-mono">
                                {previewContent.slice(0, 2000)}
                                {previewContent.length > 2000 ? '\n...' : ''}
                              </pre>
                            </div>
                          )}
                        </div>
                      )}
                    </>
                  )}
                </div>
              );
            };

            return (
              <div className="py-1">
                {recentProtocols.length > 0 && (
                  <div>
                    <div className="flex items-center gap-1.5 px-3 py-1.5">
                      <Clock className="w-3 h-3 text-amber-600 dark:text-amber-400" />
                      <span className="text-[10px] font-semibold uppercase tracking-wider text-amber-600 dark:text-amber-400">
                        Recently used
                      </span>
                      <span className="text-[9px] text-subtle ml-auto">
                        {recentProtocols.length}
                      </span>
                    </div>
                    {recentProtocols.map(renderItem)}
                  </div>
                )}

                {hasActiveSearch ? (
                  filteredProtocols.map(renderItem)
                ) : (
                  CATEGORY_ORDER
                    .filter(cat => groupedProtocols[cat]?.length)
                    .map(cat => {
                      const meta = CATEGORY_META[cat] ?? CATEGORY_META.other;
                      const items = groupedProtocols[cat];
                      const isCollapsed = collapsedCategories.has(cat);
                      const CatIcon = meta.icon;

                      return (
                        <div key={cat}>
                          <button
                            onClick={() => toggleCategory(cat)}
                            className="w-full flex items-center gap-1.5 px-3 py-1.5 hover:bg-hover/30 transition-colors group"
                          >
                            <ChevronRight
                              className={`w-3 h-3 text-subtle transition-transform ${isCollapsed ? '' : 'rotate-90'}`}
                            />
                            <CatIcon className={`w-3 h-3 ${meta.color}`} />
                            <span className={`text-[10px] font-semibold uppercase tracking-wider ${meta.color}`}>
                              {meta.label}
                            </span>
                            <span className="text-[9px] text-subtle ml-auto">
                              {items.length}
                            </span>
                          </button>
                          {!isCollapsed && items.map(renderItem)}
                        </div>
                      );
                    })
                )}
              </div>
            );
          })()
        )}
      </div>

      {/* Right-click context menu */}
      {contextMenu && (
        <div
          className="fixed z-[100] bg-surface border border-border-strong rounded-lg shadow-xl py-1 min-w-[160px]"
          style={{
            left: Math.min(contextMenu.x, window.innerWidth - 180),
            top: Math.min(contextMenu.y, window.innerHeight - 200),
          }}
        >
          <button
            onClick={() => { handleDownload(contextMenu.protocol); setContextMenu(null); }}
            className="w-full flex items-center gap-2 px-3 py-1.5 text-[12px] text-secondary hover:bg-elevated transition-colors text-left"
          >
            <Download className="w-3.5 h-3.5 pointer-events-none" />
            Download to Local
          </button>
          <button
            onClick={() => { handleCopyContent(contextMenu.protocol); setContextMenu(null); }}
            className="w-full flex items-center gap-2 px-3 py-1.5 text-[12px] text-secondary hover:bg-elevated transition-colors text-left"
          >
            <Copy className="w-3.5 h-3.5 pointer-events-none" />
            Copy to Clipboard
          </button>
          <button
            onClick={() => { handleTogglePreview(contextMenu.protocol); setContextMenu(null); }}
            className="w-full flex items-center gap-2 px-3 py-1.5 text-[12px] text-secondary hover:bg-elevated transition-colors text-left"
          >
            <Info className="w-3.5 h-3.5 pointer-events-none" />
            Preview
          </button>
          {contextMenu.protocol.source === 'user' && !contextMenu.protocol.is_folder && (
            <>
              <div className="border-t border-border-strong my-1" />
              <button
                onClick={() => { handleEdit(contextMenu.protocol); setContextMenu(null); }}
                className="w-full flex items-center gap-2 px-3 py-1.5 text-[12px] text-secondary hover:bg-elevated transition-colors text-left"
              >
                <Pencil className="w-3.5 h-3.5 pointer-events-none" />
                Edit
              </button>
              <button
                onClick={() => { setDeletingId(contextMenu.protocol.id); setContextMenu(null); }}
                className="w-full flex items-center gap-2 px-3 py-1.5 text-[12px] text-red-600 dark:text-red-400 hover:bg-elevated transition-colors text-left"
              >
                <Trash2 className="w-3.5 h-3.5 pointer-events-none" />
                Delete
              </button>
            </>
          )}
        </div>
      )}

      {/* Footer info */}
      <div className="px-3 py-2 border-t border-border-default">
        <p className="text-[9px] text-subtle leading-relaxed">
          Create protocols with AI or import from an existing pipeline.
        </p>
      </div>
    </div>
  );
}
