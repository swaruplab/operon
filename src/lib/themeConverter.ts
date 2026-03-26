import type { editor } from 'monaco-editor';

interface VSCodeTokenColor {
  name?: string;
  scope?: string | string[];
  settings: {
    foreground?: string;
    background?: string;
    fontStyle?: string;
  };
}

interface VSCodeTheme {
  name?: string;
  type?: string;
  colors?: Record<string, string>;
  tokenColors?: VSCodeTokenColor[];
}

/**
 * Convert a VS Code theme JSON to a Monaco IStandaloneThemeData.
 *
 * VS Code themes use TextMate scopes in tokenColors and a colors object for UI.
 * Monaco themes use a different format: rules array and colors object.
 */
export function convertVSCodeTheme(
  vscodeTheme: VSCodeTheme
): editor.IStandaloneThemeData {
  // Determine base theme
  const type = (vscodeTheme.type || 'dark').toLowerCase();
  let base: 'vs' | 'vs-dark' | 'hc-black' = 'vs-dark';
  if (type === 'light' || type === 'vs') base = 'vs';
  else if (type === 'hc' || type === 'hc-black') base = 'hc-black';

  // Convert token colors to Monaco rules
  const rules: editor.ITokenThemeRule[] = [];
  if (vscodeTheme.tokenColors) {
    for (const tc of vscodeTheme.tokenColors) {
      if (!tc.scope) continue;
      const scopes = Array.isArray(tc.scope)
        ? tc.scope
        : tc.scope.split(',').map((s) => s.trim());

      for (const scope of scopes) {
        if (!scope) continue;
        const rule: editor.ITokenThemeRule = { token: scope };
        if (tc.settings.foreground) {
          rule.foreground = tc.settings.foreground.replace('#', '');
        }
        if (tc.settings.background) {
          rule.background = tc.settings.background.replace('#', '');
        }
        if (tc.settings.fontStyle) {
          rule.fontStyle = tc.settings.fontStyle;
        }
        rules.push(rule);
      }
    }
  }

  // Convert UI colors
  const colors: Record<string, string> = {};
  if (vscodeTheme.colors) {
    for (const [key, value] of Object.entries(vscodeTheme.colors)) {
      if (typeof value === 'string') {
        colors[key] = value;
      }
    }
  }

  return { base, inherit: true, rules, colors };
}

/**
 * Map VS Code uiTheme string to Monaco base theme.
 */
export function mapUiTheme(uiTheme: string): 'vs' | 'vs-dark' | 'hc-black' {
  switch (uiTheme) {
    case 'vs':
    case 'vs-light':
      return 'vs';
    case 'hc-black':
    case 'hc':
      return 'hc-black';
    default:
      return 'vs-dark';
  }
}
