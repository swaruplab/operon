# Settings

Open the settings panel with ++cmd++/`,` (macOS) or ++ctrl++/`,` (Windows /
Linux), or click the gear icon in the top bar.

![Settings panel](../img/settings-panel.png){ width=600 }

## Sections

### Editor

| Setting | Default | Notes |
|---|---|---|
| Font size | 14 | Applies to Monaco + diff viewer |
| Font family | system monospace | Any installed mono font |
| Tab size | 4 | Display-only (file bytes unchanged) |
| Word wrap | off | Soft-wrap long lines |
| Line numbers | on | |
| Minimap | on | Right-edge overview strip |
| Auto-save | off | Save on every change |
| Theme | follows system | `operon-dark` or `operon-light` |

### Terminal

| Setting | Default | Notes |
|---|---|---|
| Font size | 13 | |
| Cursor style | block | block / underline / bar |
| Cursor blink | on | |
| Scrollback | 10000 | Lines kept in history |

### Claude

| Setting | Default | Notes |
|---|---|---|
| Provider | Anthropic | See [Providers](../ai/providers.md) |
| Model | Claude Opus 5 | Auto-loads when provider catalog changes |
| Effort | High | Reasoning level; skipped for models without it (e.g. Haiku 4.5) |
| Max turns (Agent mode) | 30 | How many tool-call rounds before stopping |
| Max turns (Plan mode) | 3 | Plan mode is intentionally brief |
| Max turns (Report mode) | 6 | Default report depth |
| Disallowed tools (Report mode) | `Read,Bash,Glob,Grep` | Forces writeup-focused output |
| Use translation proxy | on (Custom provider only) | Routes through bundled `anthropic-proxy` |

### Auth

Where Operon stores credentials. Per-provider:

- **Anthropic** — OAuth refresh token, or API key
- **Portkey** — virtual key + base URL
- **Custom** — base URL + optional bearer token

All credentials live in the OS keychain:

- **macOS** — Keychain (`com.operon.app`)
- **Windows** — Credential Manager
- **Linux** — libsecret / GNOME Keyring / KWallet

Never in plain-text config files.

### Appearance

| Setting | What |
|---|---|
| Theme | light / dark / system (also via top-bar toggle) |
| Reduce motion | Disables panel transitions for accessibility |

### MCP servers (advanced)

Toggle which Model Context Protocol servers Claude has access to. See
[MCP catalog](../ai/mcp.md) for what each does.

## Config file

Settings are persisted to `~/.operon/settings.json` (or
`%USERPROFILE%\.operon\settings.json` on Windows). Safe to commit a subset
into a dotfiles repo if you want the same setup on multiple machines —
secrets stay out (they live in the keychain).

Example:

```json
{
  "ai_provider": "anthropic",
  "editor_font_size": 14,
  "editor_theme": "operon-dark",
  "terminal_font_size": 13,
  "max_turns": 30,
  "appearance_theme": "dark",
  "setup_completed": true
}
```
