/** @type {import('tailwindcss').Config} */
export default {
  darkMode: 'class',   // we toggle 'light' class on <html>; dark is default
  content: [
    "./index.html",
    "./src/**/*.{js,ts,jsx,tsx}",
  ],
  theme: {
    extend: {
      fontFamily: {
        sans: ['system-ui', '-apple-system', 'BlinkMacSystemFont', 'Segoe UI', 'sans-serif'],
        mono: ["'JetBrains Mono'", "'SF Mono'", 'Menlo', 'Monaco', "'Courier New'", 'monospace'],
      },
      // Semantic palette wired to CSS variables in styles.css. Use these
      // (bg-canvas, text-primary, border-default, …) instead of hard zinc-*
      // classes so light/dark theming works automatically.
      colors: {
        canvas:    'rgb(var(--canvas) / <alpha-value>)',
        panel:     'rgb(var(--panel) / <alpha-value>)',
        surface:   'rgb(var(--surface) / <alpha-value>)',
        elevated:  'rgb(var(--elevated) / <alpha-value>)',
        hover:     'rgb(var(--hover) / <alpha-value>)',
        primary:   'rgb(var(--text-primary) / <alpha-value>)',
        secondary: 'rgb(var(--text-secondary) / <alpha-value>)',
        muted:     'rgb(var(--text-muted) / <alpha-value>)',
        subtle:    'rgb(var(--text-subtle) / <alpha-value>)',
        'border-default': 'rgb(var(--border-default) / <alpha-value>)',
        'border-strong':  'rgb(var(--border-strong) / <alpha-value>)',
      },
      // Make bare `border`, `border-t`, `divide-x`, etc. (without an explicit
      // color suffix) inherit the themed border color instead of Tailwind's
      // default gray-200. Otherwise panel dividers vanish.
      borderColor: {
        DEFAULT: 'rgb(var(--border-default) / <alpha-value>)',
      },
      divideColor: {
        DEFAULT: 'rgb(var(--border-default) / <alpha-value>)',
      },
      ringColor: {
        DEFAULT: 'rgb(var(--border-strong) / <alpha-value>)',
      },
    },
  },
  plugins: [],
}
