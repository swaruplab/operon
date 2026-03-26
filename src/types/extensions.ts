// ── Installed Extension Types ─────────────────────────────────────────────

export interface InstalledExtension {
  id: string;
  display_name: string;
  version: string;
  description: string;
  enabled: boolean;
  path: string;
  contributions: ExtContributions;
  publisher: string;
  icon_path: string | null;
}

export interface ExtContributions {
  themes: ThemeContribution[];
  snippets: SnippetContribution[];
  grammars: GrammarContribution[];
  languages: LanguageContribution[];
  configuration: Record<string, unknown> | null;
}

export interface ThemeContribution {
  label: string;
  ui_theme: string;
  path: string;
}

export interface SnippetContribution {
  language: string;
  path: string;
}

export interface GrammarContribution {
  language: string;
  scope_name: string;
  path: string;
}

export interface LanguageContribution {
  id: string;
  extensions: string[];
  aliases: string[];
}

// ── Open VSX API Types ───────────────────────────────────────────────────

export interface SearchResult {
  offset: number;
  total_size: number;
  extensions: ExtensionInfo[];
}

export interface ExtensionInfo {
  url: string | null;
  name: string;
  namespace: string;
  version: string;
  timestamp: string | null;
  display_name: string | null;
  description: string | null;
  verified: boolean | null;
  deprecated: boolean | null;
  download_count: number | null;
  average_rating: number | null;
  review_count: number | null;
  files: ExtensionFiles | null;
}

export interface ExtensionFiles {
  download: string | null;
  icon: string | null;
  manifest: string | null;
  readme: string | null;
  changelog: string | null;
  license: string | null;
  signature: string | null;
  sha256: string | null;
  public_key: string | null;
}

export interface ExtensionDetail extends ExtensionInfo {
  license: string | null;
  homepage: string | null;
  repository: string | null;
  bugs: string | null;
  engines: Record<string, string> | null;
  categories: string[] | null;
  tags: string[] | null;
  extension_kind: string[] | null;
  preview: boolean | null;
  pre_release: boolean | null;
  published_by: Publisher | null;
  dependencies: ExtensionRef[] | null;
  bundled_extensions: ExtensionRef[] | null;
  all_versions: Record<string, string> | null;
  reviews_url: string | null;
}

export interface Publisher {
  login_name: string | null;
  full_name: string | null;
  avatar_url: string | null;
}

export interface ExtensionRef {
  namespace: string | null;
  extension: string | null;
}

export interface Review {
  user: Publisher | null;
  timestamp: string | null;
  rating: number | null;
  comment: string | null;
}

export interface NamespaceDetail {
  name: string;
  extensions: Record<string, string> | null;
  verified: boolean | null;
}

export interface CompatibilityReport {
  level: 'full' | 'lsp' | 'partial' | 'not_compatible';
  supported: string[];
  unsupported: string[];
}

export interface InstallProgress {
  extension_id: string;
  stage: 'fetching' | 'downloading' | 'extracting' | 'parsing' | 'complete';
  percent: number;
}
