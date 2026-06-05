# Light & dark theme

Operon ships with both **light** and **dark** themes. Toggle from the top
bar or from **Settings → Appearance**.

The switch propagates everywhere:

- **UI chrome** — sidebar, top bar, panels, dialogs
- **Monaco editor** — `operon-dark` ↔ `operon-light`
- **xterm terminal** — palette + WebGL atlas refresh (so glyphs re-render
  cleanly at the new color)
- **Native title bar** — on macOS, Operon sets the window appearance so the
  system-drawn title bar matches the theme; same on Windows
- **Code blocks in the chat panel**
- **Diff viewer colors**

## Picking a theme

| Where | What |
|---|---|
| **Top bar** | One-click sun/moon toggle — switches immediately |
| **Settings → Appearance** | Light / Dark / Follow system |
| **`~/.operon/settings.json`** | `"appearance_theme": "light"` / `"dark"` / `"system"` |

"Follow system" reads your OS preference and updates live when you flip it
in Settings (macOS), Personalization (Windows), or the GNOME / KDE
appearance panel.

## Why a runtime toggle matters

Wet-lab biologists often work in bright tissue-culture rooms with overhead
lights and need a high-contrast light theme. Bioinformatics analysts
hammering at code at 11pm want the dark theme. Operon doesn't make you
choose at install time — flip it whenever.

## CSS-variable themes

If you want to fork the palette, the entire theme is driven by CSS variables
in `src/styles.css`:

```css
:root.dark {
  --canvas: 9 9 11;       /* zinc-950 — workspace background */
  --panel: 24 24 27;      /* zinc-900 — panel backgrounds */
  --border-default: 39 39 42;
  --text-primary: 250 250 250;
  --accent: 59 130 246;   /* blue-500 */
  /* ... */
}

:root.light {
  --canvas: 250 250 250;
  --panel: 255 255 255;
  --border-default: 228 228 231;
  --text-primary: 24 24 27;
  /* ... */
}
```

Tailwind utilities consume them via `bg-canvas`, `text-primary`,
`border-default`, etc. Override either block and rebuild.

## Known quirks

- **First terminal tab after toggle** — the xterm WebGL atlas is rebuilt
  asynchronously; you may see a 100-200ms flash while glyphs re-render.
- **Monaco diff in a fresh tab** — extremely rare flicker on the first
  diff render after a toggle while the new theme styles register.

Both are cosmetic and resolve on the next interaction.
