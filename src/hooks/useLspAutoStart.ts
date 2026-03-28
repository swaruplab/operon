/**
 * useLspAutoStart — Phase 5.5
 *
 * Auto-starts LSP servers when files are opened, and manages the full
 * document lifecycle (didOpen / didChange / didClose) between Monaco
 * and running language servers.
 *
 * Strategy:
 *   1. On mount, query installed+enabled extensions for ones whose
 *      language contributions match the current file's language.
 *   2. If a matching extension has an LSP server entry point (package.json
 *      "main" or a well-known server binary), start it via OperonLanguageClient.
 *   3. Send textDocument/didOpen with the file content.
 *   4. On every content change, send textDocument/didChange.
 *   5. On unmount, send textDocument/didClose.
 *   6. Servers are kept alive across file switches (30s grace period with no
 *      open files of that language before stopping).
 */

import { useEffect, useRef, useCallback } from 'react';
import {
  OperonLanguageClient,
  getActiveClient,
  registerClient,
} from '../lib/lspClient';
import { listInstalledExtensions, getExtensionPackageJson, getExtensionSettings, getExtensionRecommendations, startRemoteLanguageServer } from '../lib/extensions';
import type { InstalledExtension } from '../types/extensions';

// Track grace-period timers: languageId → timeout handle
const stopTimers = new Map<string, ReturnType<typeof setTimeout>>();

// Track open document count per language so we know when to start the grace period
const openDocsByLanguage = new Map<string, number>();

// Track which extensions we already attempted (and failed) to avoid retry loops
const failedExtensions = new Set<string>();

/** Well-known server binaries for common languages (fallback when extension main isn't an LSP). */
const WELL_KNOWN_SERVERS: Record<string, { command: string; args: string[] }> = {
  python: { command: 'pylsp', args: [] },
  rust: { command: 'rust-analyzer', args: [] },
  go: { command: 'gopls', args: ['serve'] },
  yaml: { command: 'yaml-language-server', args: ['--stdio'] },
  json: { command: 'vscode-json-language-server', args: ['--stdio'] },
  html: { command: 'vscode-html-language-server', args: ['--stdio'] },
  css: { command: 'vscode-css-language-server', args: ['--stdio'] },
  r: { command: 'R', args: ['--slave', '-e', 'languageserver::run()'] },
};

interface LspAutoStartOptions {
  filePath: string;
  languageId: string;
  content: string;
  workspacePath: string | null;
  sshProfileId?: string; // Optional: if provided, start LSP on remote via SSH
}

/**
 * Find an installed extension that provides LSP for the given language.
 */
async function findLspExtension(
  languageId: string
): Promise<{ ext: InstalledExtension; serverCmd: string; serverArgs: string[] } | null> {
  try {
    const extensions = await listInstalledExtensions();
    for (const ext of extensions) {
      if (!ext.enabled) continue;
      if (failedExtensions.has(ext.id)) continue;

      // Check if this extension declares the language
      const matchesLanguage = ext.contributions.languages.some(
        (l) => l.id === languageId || l.aliases.some((a) => a.toLowerCase() === languageId)
      );
      if (!matchesLanguage) continue;

      // Try to resolve server entry point from package.json
      try {
        const pkg = await getExtensionPackageJson(ext.id);
        const main = (pkg as any)?.main as string | undefined;

        if (main) {
          // Many language extensions bundle a Node.js server at "main" or
          // reference a serverModule path. If main exists, run it with node --stdio.
          const serverPath = `${ext.path}/${main}`;
          return {
            ext,
            serverCmd: 'node',
            serverArgs: [serverPath, '--stdio'],
          };
        }
      } catch {
        // package.json unavailable, try well-known fallback
      }

      // Fallback: check well-known server binary
      if (WELL_KNOWN_SERVERS[languageId]) {
        const wk = WELL_KNOWN_SERVERS[languageId];
        return { ext, serverCmd: wk.command, serverArgs: wk.args };
      }
    }
  } catch (err) {
    console.warn('[LSP] Failed to query installed extensions:', err);
  }

  // Even without an extension, try a well-known server
  if (WELL_KNOWN_SERVERS[languageId]) {
    const wk = WELL_KNOWN_SERVERS[languageId];
    return {
      ext: { id: `builtin-${languageId}`, display_name: languageId, version: '', description: '', enabled: true, path: '', contributions: { themes: [], snippets: [], grammars: [], languages: [], configuration: null }, publisher: '', icon_path: null },
      serverCmd: wk.command,
      serverArgs: wk.args,
    };
  }

  return null;
}

/**
 * React hook that auto-starts/connects LSP for the file being edited.
 */
export function useLspAutoStart({ filePath, languageId, content, workspacePath, sshProfileId }: LspAutoStartOptions) {
  const versionRef = useRef(0);
  const clientRef = useRef<OperonLanguageClient | null>(null);
  const fileUri = `file://${filePath}`;

  // On mount: find or start the LSP server, send didOpen
  useEffect(() => {
    if (!workspacePath || languageId === 'plaintext') return;

    let cancelled = false;

    // Cancel any pending stop timer for this language
    const timer = stopTimers.get(languageId);
    if (timer) {
      clearTimeout(timer);
      stopTimers.delete(languageId);
    }

    // Track open document
    openDocsByLanguage.set(languageId, (openDocsByLanguage.get(languageId) || 0) + 1);

    (async () => {
      // Check if there's already a running client for this language
      let client = getActiveClient(languageId);

      if (!client) {
        // Try to start one
        const found = await findLspExtension(languageId);

        if (!found) {
          // No LSP extension found — check for recommendations
          try {
            const recommendations = await getExtensionRecommendations(languageId);
            if (recommendations.length > 0) {
              console.log(`[LSP] No extension found for ${languageId}, but found recommendations:`, recommendations);
            }
          } catch (err) {
            console.warn('[LSP] Failed to fetch recommendations:', err);
          }
          return;
        }

        if (cancelled) return;

        // If sshProfileId is provided, start on remote via SSH
        if (sshProfileId) {
          try {
            const serverInfo = await startRemoteLanguageServer(
              found.ext.id,
              found.serverCmd,
              found.serverArgs,
              workspacePath,
              [languageId],
              sshProfileId
            );
            console.log(`[LSP] Started remote server for ${languageId} via SSH profile ${sshProfileId}: ${serverInfo.server_id}`);
            // The remote server emits lsp-message-{serverId} events that OperonLanguageClient handles
            const newClient = new OperonLanguageClient(
              found.ext.id,
              found.serverCmd,
              found.serverArgs,
              [languageId],
              workspacePath
            );
            registerClient(newClient);
            client = newClient;
          } catch (err) {
            console.warn(`[LSP] Failed to start remote server for ${languageId}:`, err);
            failedExtensions.add(found.ext.id);
            return;
          }
        } else {
          // Local LSP
          const newClient = new OperonLanguageClient(
            found.ext.id,
            found.serverCmd,
            found.serverArgs,
            [languageId],
            workspacePath
          );

          try {
            await newClient.start();
            registerClient(newClient);
            client = newClient;
            console.log(`[LSP] Started server for ${languageId} (${found.serverCmd})`);
          } catch (err) {
            console.warn(`[LSP] Failed to start server for ${languageId}:`, err);
            failedExtensions.add(found.ext.id);
            return;
          }
        }
      }

      if (cancelled) return;
      clientRef.current = client;

      // Send didOpen
      versionRef.current = 1;
      try {
        await client.didOpen(fileUri, languageId, versionRef.current, content);
      } catch (err) {
        console.warn('[LSP] didOpen failed:', err);
      }

      // Load extension settings and send them to the server
      try {
        const found = await findLspExtension(languageId);
        if (found) {
          const extSettings = await getExtensionSettings(found.ext.id);
          if (extSettings && Object.keys(extSettings).length > 0) {
            await client.didChangeConfiguration(extSettings);
          }
        }
      } catch {
        // Settings not available, continue anyway
      }
    })();

    // Cleanup: didClose + maybe schedule server shutdown
    return () => {
      cancelled = true;
      const client = clientRef.current;
      if (client?.isRunning()) {
        client.didClose(fileUri).catch(() => {});
      }
      clientRef.current = null;

      // Decrement open count
      const count = (openDocsByLanguage.get(languageId) || 1) - 1;
      openDocsByLanguage.set(languageId, count);

      // If no more files of this language are open, schedule a 30s grace stop
      if (count <= 0) {
        openDocsByLanguage.delete(languageId);
        const stopTimer = setTimeout(async () => {
          stopTimers.delete(languageId);
          const c = getActiveClient(languageId);
          // Re-check: if count is still 0, stop the server
          if (c && !openDocsByLanguage.has(languageId)) {
            console.log(`[LSP] Stopping idle server for ${languageId}`);
            try {
              await c.stop();
            } catch { /* already stopped */ }
          }
        }, 30000);
        stopTimers.set(languageId, stopTimer);
      }
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [filePath, languageId, workspacePath, sshProfileId]);

  // didChange callback — called by the editor on content changes
  const sendDidChange = useCallback(
    (newContent: string) => {
      const client = clientRef.current;
      if (!client?.isRunning()) return;
      versionRef.current += 1;
      client.didChange(fileUri, versionRef.current, newContent).catch(() => {});
    },
    [fileUri]
  );

  return { sendDidChange };
}
