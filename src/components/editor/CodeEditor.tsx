import Editor, { type OnMount, type OnChange, type BeforeMount } from '@monaco-editor/react';
import { useRef, useCallback } from 'react';
import type { editor } from 'monaco-editor';
import { useProject } from '../../context/ProjectContext';
import { useLspAutoStart } from '../../hooks/useLspAutoStart';

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

// Define the theme inline so beforeMount can register it synchronously
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

export function CodeEditor({
  filePath,
  content,
  readOnly = false,
  onChange,
  onSave,
}: CodeEditorProps) {
  const editorRef = useRef<editor.IStandaloneCodeEditor | null>(null);
  const { projectPath } = useProject();
  const languageId = detectLanguage(filePath);

  // Auto-start LSP server for this file's language
  const { sendDidChange } = useLspAutoStart({
    filePath,
    languageId,
    content,
    workspacePath: projectPath,
  });

  // Register theme BEFORE Monaco creates the editor — this is synchronous
  // and guarantees the theme exists when the editor instance is created.
  const handleBeforeMount: BeforeMount = useCallback((monaco) => {
    monaco.editor.defineTheme('operon-dark', OPERON_DARK_THEME);
  }, []);

  const handleMount: OnMount = useCallback(
    (editor, monaco) => {
      editorRef.current = editor;

      // Ensure theme is applied (belt-and-suspenders)
      monaco.editor.setTheme('operon-dark');

      // Register Cmd+S to save
      editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS, () => {
        const value = editor.getValue();
        onSave?.(value);
      });

      editor.focus();
    },
    [onSave],
  );

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

  return (
    <Editor
      height="100%"
      path={filePath}
      value={content}
      language={detectLanguage(filePath)}
      theme="operon-dark"
      beforeMount={handleBeforeMount}
      onMount={handleMount}
      onChange={handleChange}
      loading={
        <div className="h-full flex items-center justify-center text-zinc-500 text-sm">
          Loading editor...
        </div>
      }
      options={{
        readOnly,
        minimap: { enabled: true, maxColumn: 80 },
        scrollBeyondLastLine: false,
        fontSize: 13,
        fontFamily: "'JetBrains Mono', 'SF Mono', Menlo, Monaco, monospace",
        fontLigatures: true,
        lineHeight: 20,
        tabSize: 2,
        insertSpaces: true,
        wordWrap: 'off',
        automaticLayout: true,
        bracketPairColorization: { enabled: true },
        guides: { bracketPairs: true, indentation: true },
        smoothScrolling: true,
        cursorSmoothCaretAnimation: 'on',
        cursorBlinking: 'smooth',
        renderLineHighlight: 'line',
        renderWhitespace: 'selection',
        padding: { top: 8 },
      }}
    />
  );
}
