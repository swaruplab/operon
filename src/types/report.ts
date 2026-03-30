// ── Report Mode Types ──

/** File categories recognized by the project scanner */
export type ScanFileType = 'pdf' | 'image' | 'csv' | 'doc' | 'code';

/** A single scanned file with metadata */
export interface ScannedFile {
  path: string;
  name: string;
  size: number;
  file_type: ScanFileType;
  /** For CSVs: column names */
  columns?: string[];
  /** For CSVs: row count */
  rows?: number;
  /** For images: dimensions "WxH" */
  dimensions?: string;
}

/** Directory node in the scan tree */
export interface ScanTreeNode {
  path: string;
  name: string;
  is_dir: boolean;
  /** Heuristic hint: 'results' | 'plots' | 'raw' | 'intermediate' | 'other' */
  hint?: string;
  files: ScannedFile[];
  children: ScanTreeNode[];
  total_file_count: number;
  total_size: number;
}

/** Full project scan result */
export interface ProjectScan {
  root: ScanTreeNode;
  total_pdfs: number;
  total_images: number;
  total_csvs: number;
  total_docs: number;
  total_code: number;
  total_size: number;
}

/** A tool/software entry for the Methods section */
export interface ToolEntry {
  name: string;
  version: string;
  language?: string; // "R 4.3.1", "Python 3.11.2", etc.
  category?: string; // "alignment", "variant calling", "visualization"
}

/** Extracted methods information from project files */
export interface MethodsInfo {
  tools: ToolEntry[];
  r_version?: string;
  python_version?: string;
  /** Raw text snippets where versions were found */
  evidence: string[];
}

/** PubMed citation for the report */
export interface ReportCitation {
  index: number; // [1], [2], etc.
  pmid: string;
  title: string;
  authors: string;
  journal: string;
  year: string;
  doi?: string;
  url: string;
}

/** The full report configuration sent to the PDF renderer */
export interface ReportConfig {
  /** Output filename (without path) */
  filename: string;
  /** Project path for output */
  output_dir: string;

  /** Report content sections */
  title: string;
  date: string;
  authors?: string;
  abstract_text: string;
  introduction?: string;
  results: string;
  discussion: string;

  /** Methods section */
  methods: {
    overview: string;
    tools: ToolEntry[];
    data_sources?: string;
  };

  /** Figures to embed (PNG paths) */
  figures: Array<{
    path: string;
    caption: string;
    label?: string; // "Figure 1", etc.
  }>;

  /** Tables from CSVs */
  tables: Array<{
    title: string;
    headers: string[];
    rows: string[][];
    caption?: string;
  }>;

  /** PubMed references */
  references: ReportCitation[];
}

/** Preview of a file's text content for report context */
export interface FilePreview {
  path: string;
  name: string;
  content: string;
  truncated: boolean;
  error?: string;
}

/** Report generation phases */
export type ReportPhase = 'idle' | 'scan' | 'select' | 'clarify' | 'draft' | 'render' | 'done';

/** Report generation state tracked in ChatPanel */
export interface ReportState {
  phase: ReportPhase;
  scan?: ProjectScan;
  selectedFiles: string[]; // paths of selected files
  methodsInfo?: MethodsInfo;
  draftConfig?: ReportConfig;
  outputPath?: string;
  error?: string;
}
