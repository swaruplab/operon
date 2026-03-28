import * as monaco from 'monaco-editor';
import { listen } from '@tauri-apps/api/event';
import { sendLspMessage, startLanguageServer, stopLanguageServer } from './extensions';
import type { LspServerInfo } from './extensions';

/**
 * OperonLanguageClient bridges Monaco editor to a language server
 * running as a child process on the Rust backend, communicating
 * via Tauri IPC events.
 *
 * Message flow:
 *   Monaco → OperonLanguageClient → sendLspMessage (Tauri command) → server stdin
 *   server stdout → lsp-message-{serverId} (Tauri event) → OperonLanguageClient → Monaco
 */
export class OperonLanguageClient {
  private serverId: string | null = null;
  private requestId = 0;
  private pendingRequests = new Map<number, { resolve: (v: any) => void; reject: (e: any) => void }>();
  private unlistenMessage: (() => void) | null = null;
  private unlistenExit: (() => void) | null = null;
  private disposables: monaco.IDisposable[] = [];
  private running = false;
  private restartCount = 0;
  private maxRestarts = 3;

  constructor(
    private extensionId: string,
    private serverCommand: string,
    private serverArgs: string[],
    private languages: string[],
    private workspacePath: string
  ) {}

  async start(): Promise<LspServerInfo> {
    // Start the language server process
    const info = await startLanguageServer(
      this.extensionId,
      this.serverCommand,
      this.serverArgs,
      this.workspacePath,
      this.languages
    );
    this.serverId = info.server_id;
    this.running = true;

    // Listen for LSP messages from the server
    const unlisten = await listen<string>(`lsp-message-${this.serverId}`, (event) => {
      this.handleServerMessage(event.payload);
    });
    this.unlistenMessage = unlisten;

    // Listen for LSP server exit events (crash detection)
    const sid = this.serverId;
    const unlistenExit = await listen<string>(`lsp-server-exit-${sid}`, () => {
      this.handleCrash();
    });
    this.unlistenExit = unlistenExit;

    // Send initialize request
    await this.initialize();

    // Register Monaco providers for each language
    for (const lang of this.languages) {
      this.registerProviders(lang);
    }

    return info;
  }

  async stop(): Promise<void> {
    if (!this.running || !this.serverId) return;
    this.running = false;

    // Send shutdown request
    try {
      await this.sendRequest('shutdown', null);
      await this.sendNotification('exit', null);
    } catch {
      // Server may have already exited
    }

    // Stop the process
    try {
      await stopLanguageServer(this.serverId);
    } catch {
      // Already stopped
    }

    // Clean up listeners and providers
    if (this.unlistenMessage) {
      this.unlistenMessage();
      this.unlistenMessage = null;
    }
    if (this.unlistenExit) {
      this.unlistenExit();
      this.unlistenExit = null;
    }
    this.disposables.forEach((d) => d.dispose());
    this.disposables = [];
    this.pendingRequests.clear();
    this.serverId = null;
  }

  isRunning(): boolean {
    return this.running;
  }

  getServerId(): string | null {
    return this.serverId;
  }

  getLanguages(): string[] {
    return this.languages;
  }

  // ── Crash Recovery ──────────────────────────────────────────────────

  private async handleCrash(): Promise<void> {
    if (this.restartCount >= this.maxRestarts) {
      console.error(`[LSP ${this.extensionId}] Server crashed ${this.maxRestarts} times, giving up`);
      this.running = false;
      return;
    }
    this.restartCount++;
    console.warn(`[LSP ${this.extensionId}] Server crashed, restarting (attempt ${this.restartCount}/${this.maxRestarts})`);

    // Clean up old listeners
    if (this.unlistenMessage) {
      this.unlistenMessage();
      this.unlistenMessage = null;
    }
    if (this.unlistenExit) {
      this.unlistenExit();
      this.unlistenExit = null;
    }
    this.disposables.forEach(d => d.dispose());
    this.disposables = [];
    this.pendingRequests.clear();

    // Restart
    try {
      await this.start();
    } catch (err) {
      console.error(`[LSP ${this.extensionId}] Restart failed:`, err);
    }
  }

  // ── LSP Protocol ─────────────────────────────────────────────────────

  private async initialize(): Promise<void> {
    const result = await this.sendRequest('initialize', {
      processId: null,
      capabilities: {
        textDocument: {
          synchronization: {
            dynamicRegistration: false,
            willSave: false,
            willSaveWaitUntil: false,
            didSave: true,
          },
          completion: {
            dynamicRegistration: false,
            completionItem: {
              snippetSupport: true,
              commitCharactersSupport: true,
              documentationFormat: ['markdown', 'plaintext'],
              deprecatedSupport: true,
              preselectSupport: true,
            },
          },
          hover: {
            dynamicRegistration: false,
            contentFormat: ['markdown', 'plaintext'],
          },
          definition: { dynamicRegistration: false },
          references: { dynamicRegistration: false },
          documentSymbol: { dynamicRegistration: false },
          formatting: { dynamicRegistration: false },
          publishDiagnostics: {
            relatedInformation: true,
          },
        },
        workspace: {
          workspaceFolders: true,
          configuration: true,
        },
      },
      rootUri: `file://${this.workspacePath}`,
      workspaceFolders: [
        {
          uri: `file://${this.workspacePath}`,
          name: this.workspacePath.split('/').pop() || 'workspace',
        },
      ],
    });

    // Send initialized notification
    await this.sendNotification('initialized', {});
  }

  async didOpen(uri: string, languageId: string, version: number, text: string): Promise<void> {
    await this.sendNotification('textDocument/didOpen', {
      textDocument: { uri, languageId, version, text },
    });
  }

  async didChange(uri: string, version: number, text: string): Promise<void> {
    await this.sendNotification('textDocument/didChange', {
      textDocument: { uri, version },
      contentChanges: [{ text }],
    });
  }

  async didClose(uri: string): Promise<void> {
    await this.sendNotification('textDocument/didClose', {
      textDocument: { uri },
    });
  }

  async didChangeConfiguration(settings: Record<string, unknown>): Promise<void> {
    await this.sendNotification('workspace/didChangeConfiguration', { settings });
  }

  // ── Message Handling ─────────────────────────────────────────────────

  private async sendRequest(method: string, params: any): Promise<any> {
    if (!this.serverId) throw new Error('Language server not started');
    const id = ++this.requestId;
    const message = JSON.stringify({ jsonrpc: '2.0', id, method, params });

    return new Promise((resolve, reject) => {
      this.pendingRequests.set(id, { resolve, reject });
      sendLspMessage(this.serverId!, message).catch(reject);

      // Timeout after 30s
      setTimeout(() => {
        if (this.pendingRequests.has(id)) {
          this.pendingRequests.delete(id);
          reject(new Error(`LSP request '${method}' timed out`));
        }
      }, 30000);
    });
  }

  private async sendNotification(method: string, params: any): Promise<void> {
    if (!this.serverId) return;
    const message = JSON.stringify({ jsonrpc: '2.0', method, params });
    await sendLspMessage(this.serverId, message);
  }

  private handleServerMessage(raw: string): void {
    let msg: any;
    try {
      msg = JSON.parse(raw);
    } catch {
      return;
    }

    // Response to a request
    if (msg.id !== undefined && (msg.result !== undefined || msg.error !== undefined)) {
      const pending = this.pendingRequests.get(msg.id);
      if (pending) {
        this.pendingRequests.delete(msg.id);
        if (msg.error) {
          pending.reject(new Error(msg.error.message || 'LSP error'));
        } else {
          pending.resolve(msg.result);
        }
      }
      return;
    }

    // Server notification
    if (msg.method) {
      this.handleNotification(msg.method, msg.params);
    }
  }

  private handleNotification(method: string, params: any): void {
    switch (method) {
      case 'textDocument/publishDiagnostics':
        this.handleDiagnostics(params);
        break;
      case 'window/logMessage':
      case 'window/showMessage':
        console.log(`[LSP ${this.extensionId}] ${params?.message}`);
        break;
      // workspace/configuration requests from server
    }
  }

  private handleDiagnostics(params: { uri: string; diagnostics: any[] }): void {
    const uri = monaco.Uri.parse(params.uri);
    const model = monaco.editor.getModels().find((m) => m.uri.toString() === uri.toString());
    if (!model) return;

    const markers: monaco.editor.IMarkerData[] = params.diagnostics.map((d) => ({
      severity: this.mapSeverity(d.severity),
      startLineNumber: (d.range?.start?.line ?? 0) + 1,
      startColumn: (d.range?.start?.character ?? 0) + 1,
      endLineNumber: (d.range?.end?.line ?? 0) + 1,
      endColumn: (d.range?.end?.character ?? 0) + 1,
      message: d.message || '',
      source: d.source || this.extensionId,
      code: d.code?.toString(),
    }));

    monaco.editor.setModelMarkers(model, this.extensionId, markers);
  }

  private mapSeverity(severity?: number): monaco.MarkerSeverity {
    switch (severity) {
      case 1: return monaco.MarkerSeverity.Error;
      case 2: return monaco.MarkerSeverity.Warning;
      case 3: return monaco.MarkerSeverity.Info;
      case 4: return monaco.MarkerSeverity.Hint;
      default: return monaco.MarkerSeverity.Info;
    }
  }

  // ── Monaco Provider Registration ─────────────────────────────────────

  private registerProviders(languageId: string): void {
    // Completion provider
    this.disposables.push(
      monaco.languages.registerCompletionItemProvider(languageId, {
        triggerCharacters: ['.', ':', '<', '"', "'", '/', '@', '#'],
        provideCompletionItems: async (model, position) => {
          if (!this.running) return { suggestions: [] };
          try {
            const result = await this.sendRequest('textDocument/completion', {
              textDocument: { uri: model.uri.toString() },
              position: { line: position.lineNumber - 1, character: position.column - 1 },
            });

            const items = Array.isArray(result) ? result : result?.items || [];
            const word = model.getWordUntilPosition(position);
            const range = {
              startLineNumber: position.lineNumber,
              endLineNumber: position.lineNumber,
              startColumn: word.startColumn,
              endColumn: word.endColumn,
            };

            return {
              suggestions: items.map((item: any) => ({
                label: item.label,
                kind: this.mapCompletionKind(item.kind),
                insertText: item.insertText || item.label,
                insertTextRules: item.insertTextFormat === 2
                  ? monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet
                  : undefined,
                detail: item.detail,
                documentation: item.documentation,
                sortText: item.sortText,
                filterText: item.filterText,
                range,
              })),
            };
          } catch {
            return { suggestions: [] };
          }
        },
      })
    );

    // Hover provider
    this.disposables.push(
      monaco.languages.registerHoverProvider(languageId, {
        provideHover: async (model, position) => {
          if (!this.running) return null;
          try {
            const result = await this.sendRequest('textDocument/hover', {
              textDocument: { uri: model.uri.toString() },
              position: { line: position.lineNumber - 1, character: position.column - 1 },
            });
            if (!result?.contents) return null;

            const contents = Array.isArray(result.contents)
              ? result.contents.map((c: any) =>
                  typeof c === 'string' ? { value: c } : { value: c.value || '' }
                )
              : [{ value: typeof result.contents === 'string' ? result.contents : result.contents.value || '' }];

            return {
              contents,
              range: result.range
                ? {
                    startLineNumber: result.range.start.line + 1,
                    startColumn: result.range.start.character + 1,
                    endLineNumber: result.range.end.line + 1,
                    endColumn: result.range.end.character + 1,
                  }
                : undefined,
            };
          } catch {
            return null;
          }
        },
      })
    );

    // Definition provider
    this.disposables.push(
      monaco.languages.registerDefinitionProvider(languageId, {
        provideDefinition: async (model, position) => {
          if (!this.running) return null;
          try {
            const result = await this.sendRequest('textDocument/definition', {
              textDocument: { uri: model.uri.toString() },
              position: { line: position.lineNumber - 1, character: position.column - 1 },
            });
            if (!result) return null;

            const locations = Array.isArray(result) ? result : [result];
            return locations.map((loc: any) => ({
              uri: monaco.Uri.parse(loc.uri),
              range: {
                startLineNumber: loc.range.start.line + 1,
                startColumn: loc.range.start.character + 1,
                endLineNumber: loc.range.end.line + 1,
                endColumn: loc.range.end.character + 1,
              },
            }));
          } catch {
            return null;
          }
        },
      })
    );

    // Document formatting provider
    this.disposables.push(
      monaco.languages.registerDocumentFormattingEditProvider(languageId, {
        provideDocumentFormattingEdits: async (model) => {
          if (!this.running) return [];
          try {
            const result = await this.sendRequest('textDocument/formatting', {
              textDocument: { uri: model.uri.toString() },
              options: {
                tabSize: 2,
                insertSpaces: true,
              },
            });
            if (!result) return [];

            return result.map((edit: any) => ({
              range: {
                startLineNumber: edit.range.start.line + 1,
                startColumn: edit.range.start.character + 1,
                endLineNumber: edit.range.end.line + 1,
                endColumn: edit.range.end.character + 1,
              },
              text: edit.newText,
            }));
          } catch {
            return [];
          }
        },
      })
    );
  }

  private mapCompletionKind(kind?: number): monaco.languages.CompletionItemKind {
    const map: Record<number, monaco.languages.CompletionItemKind> = {
      1: monaco.languages.CompletionItemKind.Text,
      2: monaco.languages.CompletionItemKind.Method,
      3: monaco.languages.CompletionItemKind.Function,
      4: monaco.languages.CompletionItemKind.Constructor,
      5: monaco.languages.CompletionItemKind.Field,
      6: monaco.languages.CompletionItemKind.Variable,
      7: monaco.languages.CompletionItemKind.Class,
      8: monaco.languages.CompletionItemKind.Interface,
      9: monaco.languages.CompletionItemKind.Module,
      10: monaco.languages.CompletionItemKind.Property,
      11: monaco.languages.CompletionItemKind.Unit,
      12: monaco.languages.CompletionItemKind.Value,
      13: monaco.languages.CompletionItemKind.Enum,
      14: monaco.languages.CompletionItemKind.Keyword,
      15: monaco.languages.CompletionItemKind.Snippet,
      16: monaco.languages.CompletionItemKind.Color,
      17: monaco.languages.CompletionItemKind.File,
      18: monaco.languages.CompletionItemKind.Reference,
      19: monaco.languages.CompletionItemKind.Folder,
      20: monaco.languages.CompletionItemKind.EnumMember,
      21: monaco.languages.CompletionItemKind.Constant,
      22: monaco.languages.CompletionItemKind.Struct,
      23: monaco.languages.CompletionItemKind.Event,
      24: monaco.languages.CompletionItemKind.Operator,
      25: monaco.languages.CompletionItemKind.TypeParameter,
    };
    return map[kind ?? 1] ?? monaco.languages.CompletionItemKind.Text;
  }
}

// ── Active LSP Client Manager ───────────────────────────────────────────

const activeClients = new Map<string, OperonLanguageClient>();

/**
 * Get or create an LSP client for the given language.
 */
export function getActiveClient(languageId: string): OperonLanguageClient | undefined {
  for (const client of activeClients.values()) {
    if (client.getLanguages().includes(languageId)) {
      return client;
    }
  }
  return undefined;
}

/**
 * Register an active client.
 */
export function registerClient(client: OperonLanguageClient): void {
  const id = client.getServerId();
  if (id) activeClients.set(id, client);
}

/**
 * Stop and remove a client.
 */
export async function removeClient(serverId: string): Promise<void> {
  const client = activeClients.get(serverId);
  if (client) {
    await client.stop();
    activeClients.delete(serverId);
  }
}

/**
 * Stop all active clients.
 */
export async function stopAllClients(): Promise<void> {
  for (const client of activeClients.values()) {
    await client.stop();
  }
  activeClients.clear();
}
