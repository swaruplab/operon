import { invoke } from '@tauri-apps/api/core';
import type {
  SearchResult,
  ExtensionDetail,
  NamespaceDetail,
  Review,
  CompatibilityReport,
  InstalledExtension,
} from '../types/extensions';

// ── Open VSX API ─────────────────────────────────────────────────────────

export async function searchExtensions(
  query: string,
  options?: {
    category?: string;
    offset?: number;
    size?: number;
    sortBy?: string;
    sortOrder?: string;
  }
): Promise<SearchResult> {
  return invoke('search_extensions', {
    query,
    category: options?.category ?? null,
    offset: options?.offset ?? null,
    size: options?.size ?? null,
    sortBy: options?.sortBy ?? null,
    sortOrder: options?.sortOrder ?? null,
  });
}

export async function getExtensionDetails(
  namespace: string,
  name: string
): Promise<ExtensionDetail> {
  return invoke('get_extension_details', { namespace, name });
}

export async function getExtensionManifest(
  namespace: string,
  name: string
): Promise<Record<string, unknown>> {
  return invoke('get_extension_manifest', { namespace, name });
}

export async function getExtensionReadme(
  namespace: string,
  name: string
): Promise<string> {
  return invoke('get_extension_readme', { namespace, name });
}

export async function getNamespaceExtensions(
  namespace: string
): Promise<NamespaceDetail> {
  return invoke('get_namespace_extensions', { namespace });
}

export async function getExtensionReviews(
  namespace: string,
  name: string
): Promise<Review[]> {
  return invoke('get_extension_reviews', { namespace, name });
}

export async function checkExtensionCompatibility(
  namespace: string,
  name: string
): Promise<CompatibilityReport> {
  return invoke('check_extension_compatibility', { namespace, name });
}

export async function browseExtensionsByCategory(
  category: string,
  options?: { offset?: number; size?: number; sortBy?: string }
): Promise<SearchResult> {
  return invoke('browse_extensions_by_category', {
    category,
    offset: options?.offset ?? null,
    size: options?.size ?? null,
    sortBy: options?.sortBy ?? null,
  });
}

// ── Extension Management ─────────────────────────────────────────────────

export async function listInstalledExtensions(): Promise<InstalledExtension[]> {
  return invoke('list_installed_extensions');
}

export async function installExtension(
  namespace: string,
  name: string
): Promise<InstalledExtension> {
  return invoke('install_extension_from_registry', { namespace, name });
}

export async function uninstallExtension(id: string): Promise<void> {
  return invoke('uninstall_extension', { id });
}

export async function enableExtension(id: string): Promise<void> {
  return invoke('enable_extension', { id });
}

export async function disableExtension(id: string): Promise<void> {
  return invoke('disable_extension', { id });
}

export async function sideloadVsix(path: string): Promise<InstalledExtension> {
  return invoke('sideload_vsix', { path });
}

export async function getExtensionPackageJson(
  id: string
): Promise<Record<string, unknown>> {
  return invoke('get_extension_package_json', { id });
}

// ── Extension Content ────────────────────────────────────────────────────

export async function readExtensionTheme(
  extensionId: string,
  themePath: string
): Promise<Record<string, unknown>> {
  return invoke('read_extension_theme', {
    extensionId,
    themePath,
  });
}

export async function readExtensionSnippets(
  extensionId: string,
  snippetPath: string
): Promise<Record<string, unknown>> {
  return invoke('read_extension_snippets', {
    extensionId,
    snippetPath,
  });
}

// ── LSP Server Management ────────────────────────────────────────────────

export interface LspServerInfo {
  server_id: string;
  extension_id: string;
  languages: string[];
}

export async function startLanguageServer(
  extensionId: string,
  serverCommand: string,
  serverArgs: string[],
  workspacePath: string,
  languages: string[]
): Promise<LspServerInfo> {
  return invoke('start_language_server', {
    extensionId,
    serverCommand,
    serverArgs,
    workspacePath,
    languages,
  });
}

export async function sendLspMessage(
  serverId: string,
  message: string
): Promise<void> {
  return invoke('send_lsp_message', { serverId, message });
}

export async function stopLanguageServer(serverId: string): Promise<void> {
  return invoke('stop_language_server', { serverId });
}

export async function listLanguageServers(): Promise<LspServerInfo[]> {
  return invoke('list_language_servers');
}

// ── Extension Settings ───────────────────────────────────────────────────

export async function getExtensionConfigSchema(id: string): Promise<Record<string, unknown> | null> {
  return invoke('get_extension_config_schema', { id });
}

export async function getExtensionSettings(id: string): Promise<Record<string, unknown>> {
  return invoke('get_extension_settings', { id });
}

export async function updateExtensionSettings(id: string, values: Record<string, unknown>): Promise<void> {
  return invoke('update_extension_settings', { id, values });
}

// ── Phase 9: Polish & Reliability ────────────────────────────────────────

export interface UpdateAvailable {
  extension_id: string;
  current_version: string;
  latest_version: string;
  display_name: string;
}

export async function checkExtensionUpdates(): Promise<UpdateAvailable[]> {
  return invoke('check_extension_updates');
}

export interface ExtensionRecommendation {
  language_id: string;
  namespace: string;
  name: string;
  display_name: string;
  description: string;
}

export async function getExtensionRecommendations(
  languageId: string
): Promise<ExtensionRecommendation[]> {
  return invoke('get_extension_recommendations', { language_id: languageId });
}

export async function validateExtensionInstall(
  namespace: string,
  name: string
): Promise<{ can_install: boolean; warnings: string[] }> {
  return invoke('validate_extension_install', { namespace, name });
}

// ── Remote LSP Server Management ─────────────────────────────────────

export async function startRemoteLanguageServer(
  extensionId: string,
  serverCommand: string,
  serverArgs: string[],
  workspacePath: string,
  languages: string[],
  sshProfileId: string,
): Promise<LspServerInfo> {
  return invoke('start_remote_language_server', {
    extensionId,
    serverCommand,
    serverArgs,
    workspacePath,
    languages,
    sshProfileId,
  });
}

// ── Remote Extension Installation ───────────────────────────────────

export async function installRemoteExtension(
  sshProfileId: string,
  namespace: string,
  name: string,
): Promise<void> {
  return invoke('install_remote_extension', { sshProfileId, namespace, name });
}
