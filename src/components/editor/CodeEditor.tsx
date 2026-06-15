import Editor, { type OnMount, type OnChange, type BeforeMount } from '@monaco-editor/react';
import { useRef, useCallback, useEffect } from 'react';
import type { editor } from 'monaco-editor';
import { useProject } from '../../context/ProjectContext';
import { useLspAutoStart } from '../../hooks/useLspAutoStart';
import { useTheme } from '../../context/ThemeContext';

interface CodeEditorProps {
  filePath: string;
  content: string;
  readOnly?: boolean;
  onChange?: (content: string) => void;
  onSave?: (content: string) => void;
}

const EXTENSION_MAP: Record<string, string> = {
  js: 'javascript',
  jsx: 'javascript',
  ts: 'typescript',
  tsx: 'typescriptreact',
  py: 'python',
  rs: 'rust',
  go: 'go',
  rb: 'ruby',
  java: 'java',
  kt: 'kotlin',
  swift: 'swift',
  c: 'c',
  cpp: 'cpp',
  h: 'c',
  cs: 'csharp',
  php: 'php',
  html: 'html',
  css: 'css',
  scss: 'scss',
  json: 'json',
  yaml: 'yaml',
  yml: 'yaml',
  toml: 'toml',
  xml: 'xml',
  md: 'markdown',
  sql: 'sql',
  sh: 'shell',
  bash: 'shell',
  zsh: 'shell',
  dockerfile: 'dockerfile',
  makefile: 'makefile',
  r: 'r',
  R: 'r',
};

export function detectLanguage(filePath: string): string {
  const ext = filePath.split('.').pop()?.toLowerCase() || '';
  const fileName = filePath.split('/').pop()?.toLowerCase() || '';
  if (EXTENSION_MAP[fileName]) return EXTENSION_MAP[fileName];
  return EXTENSION_MAP[ext] || 'plaintext';
}

// Define both themes inline so beforeMount can register them synchronously.
const OPERON_DARK_THEME: editor.IStandaloneThemeData = {
  base: 'vs-dark',
  inherit: true,
  rules: [
    { token: '', foreground: 'fafafa', background: '09090b' },
    { token: 'comment', foreground: '71717a', fontStyle: 'italic' },
    { token: 'keyword', foreground: 'c084fc' },
    { token: 'keyword.control', foreground: 'c084fc' },
    { token: 'string', foreground: '4ade80' },
    { token: 'string.escape', foreground: '22d3ee' },
    { token: 'number', foreground: 'fb923c' },
    { token: 'type', foreground: '38bdf8' },
    { token: 'type.identifier', foreground: '38bdf8' },
    { token: 'function', foreground: '60a5fa' },
    { token: 'function.declaration', foreground: '60a5fa' },
    { token: 'variable', foreground: 'fafafa' },
    { token: 'variable.predefined', foreground: 'f472b6' },
    { token: 'constant', foreground: 'fb923c' },
    { token: 'tag', foreground: 'f87171' },
    { token: 'attribute.name', foreground: 'facc15' },
    { token: 'attribute.value', foreground: '4ade80' },
    { token: 'delimiter', foreground: 'a1a1aa' },
    { token: 'operator', foreground: 'a1a1aa' },
  ],
  colors: {
    'editor.background': '#09090b',
    'editor.foreground': '#fafafa',
    'editor.lineHighlightBackground': '#18181b',
    'editor.selectionBackground': '#3f3f4680',
    'editor.inactiveSelectionBackground': '#3f3f4640',
    'editorLineNumber.foreground': '#52525b',
    'editorLineNumber.activeForeground': '#a1a1aa',
    'editorCursor.foreground': '#fafafa',
    'editorIndentGuide.background': '#27272a',
    'editorIndentGuide.activeBackground': '#3f3f46',
    'editorBracketMatch.background': '#3f3f4660',
    'editorBracketMatch.border': '#71717a',
    'editor.findMatchBackground': '#eab30840',
    'editor.findMatchHighlightBackground': '#eab30820',
    'editorWidget.background': '#18181b',
    'editorWidget.border': '#27272a',
    'editorSuggestWidget.background': '#18181b',
    'editorSuggestWidget.border': '#27272a',
    'editorSuggestWidget.selectedBackground': '#27272a',
    'editorHoverWidget.background': '#18181b',
    'editorHoverWidget.border': '#27272a',
    'minimap.background': '#09090b',
    'scrollbar.shadow': '#00000000',
    'scrollbarSlider.background': '#3f3f4640',
    'scrollbarSlider.hoverBackground': '#3f3f4680',
    'scrollbarSlider.activeBackground': '#3f3f46a0',
  },
};

// Light counterpart — darker hues for syntax (better contrast against white),
// neutral grays for chrome that match the app's --canvas/--panel/--surface vars.
const OPERON_LIGHT_THEME: editor.IStandaloneThemeData = {
  base: 'vs',
  inherit: true,
  rules: [
    { token: '', foreground: '18181b', background: 'fafafa' },
    { token: 'comment', foreground: '71717a', fontStyle: 'italic' },
    { token: 'keyword', foreground: '7c3aed' },           // violet-600
    { token: 'keyword.control', foreground: '7c3aed' },
    { token: 'string', foreground: '15803d' },            // green-700
    { token: 'string.escape', foreground: '0e7490' },     // cyan-700
    { token: 'number', foreground: 'c2410c' },            // orange-700
    { token: 'type', foreground: '0369a1' },              // sky-700
    { token: 'type.identifier', foreground: '0369a1' },
    { token: 'function', foreground: '1d4ed8' },          // blue-700
    { token: 'function.declaration', foreground: '1d4ed8' },
    { token: 'variable', foreground: '18181b' },
    { token: 'variable.predefined', foreground: 'be185d' }, // pink-700
    { token: 'constant', foreground: 'c2410c' },
    { token: 'tag', foreground: 'b91c1c' },               // red-700
    { token: 'attribute.name', foreground: 'a16207' },    // yellow-700
    { token: 'attribute.value', foreground: '15803d' },
    { token: 'delimiter', foreground: '52525b' },
    { token: 'operator', foreground: '52525b' },
  ],
  colors: {
    'editor.background': '#fafafa',
    'editor.foreground': '#18181b',
    'editor.lineHighlightBackground': '#f4f4f5',
    'editor.selectionBackground': '#d4d4d880',
    'editor.inactiveSelectionBackground': '#d4d4d840',
    'editorLineNumber.foreground': '#a1a1aa',
    'editorLineNumber.activeForeground': '#52525b',
    'editorCursor.foreground': '#18181b',
    'editorIndentGuide.background': '#e4e4e7',
    'editorIndentGuide.activeBackground': '#d4d4d8',
    'editorBracketMatch.background': '#d4d4d860',
    'editorBracketMatch.border': '#71717a',
    'editor.findMatchBackground': '#eab30840',
    'editor.findMatchHighlightBackground': '#eab30820',
    'editorWidget.background': '#ffffff',
    'editorWidget.border': '#e4e4e7',
    'editorSuggestWidget.background': '#ffffff',
    'editorSuggestWidget.border': '#e4e4e7',
    'editorSuggestWidget.selectedBackground': '#e4e4e7',
    'editorHoverWidget.background': '#ffffff',
    'editorHoverWidget.border': '#e4e4e7',
    'minimap.background': '#fafafa',
    'scrollbar.shadow': '#00000000',
    'scrollbarSlider.background': '#d4d4d880',
    'scrollbarSlider.hoverBackground': '#a1a1aa80',
    'scrollbarSlider.activeBackground': '#71717a80',
  },
};

/** Public name of the currently-active Monaco theme — used by DiffViewer too. */
export function monacoThemeNameFor(resolved: 'dark' | 'light'): string {
  return resolved === 'light' ? 'operon-light' : 'operon-dark';
}

export function CodeEditor({
  filePath,
  content,
  readOnly = false,
  onChange,
  onSave,
}: CodeEditorProps) {
  const editorRef = useRef<editor.IStandaloneCodeEditor | null>(null);
  const monacoRef = useRef<typeof import('monaco-editor') | null>(null);
  const onSaveRef = useRef(onSave);
  onSaveRef.current = onSave;
  const { projectPath } = useProject();
  const { resolved: themeMode } = useTheme();
  const languageId = detectLanguage(filePath);
  const isMarkdown = languageId === 'markdown';
  const activeTheme = monacoThemeNameFor(themeMode);

  // Auto-start LSP server for this file's language
  const { sendDidChange } = useLspAutoStart({
    filePath,
    languageId,
    content,
    workspacePath: projectPath,
  });

  // Register BOTH themes BEFORE Monaco creates the editor — synchronous, and
  // guarantees whichever the user toggles to is already defined.
  const handleBeforeMount: BeforeMount = useCallback((monaco) => {
    monaco.editor.defineTheme('operon-dark', OPERON_DARK_THEME);
    monaco.editor.defineTheme('operon-light', OPERON_LIGHT_THEME);
  }, []);

  const handleMount: OnMount = useCallback(
    (editor, monaco) => {
      editorRef.current = editor;
      monacoRef.current = monaco;

      monaco.editor.setTheme(activeTheme);

      // Register Cmd+S to save — uses ref so the handler always calls the latest onSave
      editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS, () => {
        const value = editor.getValue();
        onSaveRef.current?.(value);
      });

      editor.focus();
    },
    [activeTheme],
  );

  // Re-apply theme when the global app theme flips. Monaco's setTheme is a
  // module-level switch (affects all editor instances), but we still call
  // here on each mount/change so a freshly-opened editor picks it up.
  useEffect(() => {
    monacoRef.current?.editor.setTheme(activeTheme);
  }, [activeTheme]);

  const handleChange: OnChange = useCallback(
    (value) => {
      if (value !== undefined) {
        onChange?.(value);
        // Notify LSP of content change
        sendDidChange(value);
      }
    },
    [onChange, sendDidChange],
  );

  // Listen for "reveal-editor-line" events (dispatched by the Search view)
  // and scroll / select the target line when this editor's file matches.
  useEffect(() => {
    const handler = (event: Event) => {
      const custom = event as CustomEvent<{ filePath: string; line: number }>;
      const { filePath: target, line } = custom.detail || ({} as { filePath: string; line: number });
      if (!target || !line || target !== filePath) return;
      const ed = editorRef.current;
      if (!ed) return;
      try {
        ed.revealLineInCenter(line);
        ed.setPosition({ lineNumber: line, column: 1 });
        ed.focus();
      } catch {
        // ignore
      }
    };
    window.addEventListener('reveal-editor-line', handler);
    return () => window.removeEventListener('reveal-editor-line', handler);
  }, [filePath]);

  return (
    <Editor
      height="100%"
      path={filePath}
      value={content}
      language={detectLanguage(filePath)}
      theme={activeTheme}
      beforeMount={handleBeforeMount}
      onMount={handleMount}
      onChange={handleChange}
      loading={
        <div className="h-full flex items-center justify-center text-muted text-sm">
          Loading editor...
        </div>
      }
      options={{
        readOnly,
        minimap: { enabled: false },
        scrollBeyondLastLine: false,
        fontSize: 13,
        fontFamily: isMarkdown
          ? "'Inter', 'SF Pro Text', -apple-system, 'Ubuntu', 'Cantarell', 'Noto Sans', 'Liberation Sans', sans-serif"
          : "'JetBrains Mono', 'SF Mono', Menlo, Monaco, Consolas, 'Cascadia Code', 'DejaVu Sans Mono', 'Liberation Mono', 'Ubuntu Mono', 'Courier New', monospace",
        fontLigatures: !isMarkdown,
        lineHeight: isMarkdown ? 24 : 20,
        tabSize: 2,
        insertSpaces: true,
        wordWrap: isMarkdown ? 'on' : 'off',
        automaticLayout: true,
        bracketPairColorization: { enabled: !isMarkdown },
        guides: { bracketPairs: !isMarkdown, indentation: !isMarkdown },
        smoothScrolling: true,
        cursorSmoothCaretAnimation: 'on',
        cursorBlinking: 'smooth',
        renderLineHighlight: isMarkdown ? 'none' : 'line',
        renderWhitespace: isMarkdown ? 'none' : 'selection',
        lineNumbers: isMarkdown ? 'off' : 'on',
        padding: { top: isMarkdown ? 16 : 8 },
        scrollbar: {
          horizontal: 'auto',
          vertical: 'auto',
          useShadows: false,
        },
      }}
    />
  );
}
