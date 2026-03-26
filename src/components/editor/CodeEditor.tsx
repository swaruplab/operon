import Editor, { type OnMount, type OnChange } from '@monaco-editor/react';
import { useRef, useCallback } from 'react';
import type { editor } from 'monaco-editor';

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
};

export function detectLanguage(filePath: string): string {
  const ext = filePath.split('.').pop()?.toLowerCase() || '';
  const fileName = filePath.split('/').pop()?.toLowerCase() || '';
  if (EXTENSION_MAP[fileName]) return EXTENSION_MAP[fileName];
  return EXTENSION_MAP[ext] || 'plaintext';
}

export function CodeEditor({
  filePath,
  content,
  readOnly = false,
  onChange,
  onSave,
}: CodeEditorProps) {
  const editorRef = useRef<editor.IStandaloneCodeEditor | null>(null);

  const handleMount: OnMount = useCallback(
    (editor, monaco) => {
      editorRef.current = editor;

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
      }
    },
    [onChange],
  );

  return (
    <Editor
      height="100%"
      path={filePath}
      value={content}
      language={detectLanguage(filePath)}
      theme="operon-dark"
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
