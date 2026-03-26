import * as monaco from 'monaco-editor';
import { readExtensionTheme, readExtensionSnippets, listInstalledExtensions } from './extensions';
import { convertVSCodeTheme } from './themeConverter';
import type { InstalledExtension, ThemeContribution, SnippetContribution } from '../types/extensions';

// Track loaded contributions for cleanup
const loadedThemes = new Map<string, string[]>(); // extId → theme names
const loadedSnippetProviders = new Map<string, monaco.IDisposable[]>(); // extId → disposables

/**
 * Load all contributions from an installed, enabled extension.
 */
export async function loadExtensionContributions(ext: InstalledExtension): Promise<void> {
  if (!ext.enabled) return;

  // Load themes
  if (ext.contributions.themes.length > 0) {
    const names: string[] = [];
    for (const theme of ext.contributions.themes) {
      try {
        const themeJson = await readExtensionTheme(ext.id, theme.path);
        const monacoTheme = convertVSCodeTheme(themeJson as any);
        const themeName = `ext-${ext.id}-${theme.label}`.replace(/[^a-zA-Z0-9-_]/g, '-');
        monaco.editor.defineTheme(themeName, monacoTheme);
        names.push(themeName);
      } catch (err) {
        console.warn(`Failed to load theme "${theme.label}" from ${ext.id}:`, err);
      }
    }
    loadedThemes.set(ext.id, names);
  }

  // Load snippets
  if (ext.contributions.snippets.length > 0) {
    const disposables: monaco.IDisposable[] = [];
    for (const snippet of ext.contributions.snippets) {
      try {
        const snippetJson = await readExtensionSnippets(ext.id, snippet.path);
        const provider = registerSnippets(snippet.language, snippetJson);
        if (provider) disposables.push(provider);
      } catch (err) {
        console.warn(`Failed to load snippets for ${snippet.language} from ${ext.id}:`, err);
      }
    }
    loadedSnippetProviders.set(ext.id, disposables);
  }
}

/**
 * Unload all contributions from an extension.
 */
export function unloadExtensionContributions(extId: string): void {
  // Themes can't truly be "unloaded" from Monaco, but we remove tracking
  loadedThemes.delete(extId);

  // Dispose snippet providers
  const providers = loadedSnippetProviders.get(extId);
  if (providers) {
    providers.forEach((p) => p.dispose());
    loadedSnippetProviders.delete(extId);
  }
}

/**
 * Load contributions from all enabled installed extensions.
 * Call on app startup.
 */
export async function loadAllExtensionContributions(): Promise<void> {
  try {
    const extensions = await listInstalledExtensions();
    for (const ext of extensions) {
      if (ext.enabled) {
        await loadExtensionContributions(ext);
      }
    }
  } catch (err) {
    console.warn('Failed to load extension contributions:', err);
  }
}

/**
 * Get all extension theme names that have been loaded.
 */
export function getExtensionThemeNames(): { extId: string; name: string; label: string }[] {
  const themes: { extId: string; name: string; label: string }[] = [];
  for (const [extId, names] of loadedThemes) {
    for (const name of names) {
      // Extract label from theme name
      const label = name.replace(`ext-${extId}-`, '').replace(/-/g, ' ');
      themes.push({ extId, name, label });
    }
  }
  return themes;
}

// ── Internal helpers ──────────────────────────────────────────────────

interface VSCodeSnippet {
  prefix: string | string[];
  body: string | string[];
  description?: string;
}

function registerSnippets(
  language: string,
  snippetsJson: Record<string, unknown>
): monaco.IDisposable | null {
  const suggestions: monaco.languages.CompletionItem[] = [];

  for (const [name, raw] of Object.entries(snippetsJson)) {
    const snippet = raw as VSCodeSnippet;
    if (!snippet.prefix || !snippet.body) continue;

    const prefixes = Array.isArray(snippet.prefix) ? snippet.prefix : [snippet.prefix];
    const body = Array.isArray(snippet.body) ? snippet.body.join('\n') : snippet.body;

    for (const prefix of prefixes) {
      suggestions.push({
        label: prefix,
        kind: monaco.languages.CompletionItemKind.Snippet,
        insertText: body,
        insertTextRules: monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet,
        detail: name,
        documentation: snippet.description || undefined,
        range: undefined as any, // Will be set by Monaco
      });
    }
  }

  if (suggestions.length === 0) return null;

  return monaco.languages.registerCompletionItemProvider(language, {
    provideCompletionItems: (_model, position) => {
      const word = _model.getWordUntilPosition(position);
      const range = {
        startLineNumber: position.lineNumber,
        endLineNumber: position.lineNumber,
        startColumn: word.startColumn,
        endColumn: word.endColumn,
      };
      return {
        suggestions: suggestions.map((s) => ({ ...s, range })),
      };
    },
  });
}
