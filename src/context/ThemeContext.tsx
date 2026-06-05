import { createContext, useContext, useEffect, useState, useCallback, type ReactNode } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { getSettings, updateSettings } from '../lib/settings';

export type ThemeChoice = 'dark' | 'light' | 'system';
export type ResolvedTheme = 'dark' | 'light';

interface ThemeContextValue {
  /** What the user selected — may be 'system'. */
  theme: ThemeChoice;
  /** What actually applies right now — 'system' resolved against OS. */
  resolved: ResolvedTheme;
  /** Persist + apply a new theme. */
  setTheme: (next: ThemeChoice) => void;
  /** Convenience: toggle between dark ↔ light (treats 'system' as its current resolved value). */
  toggle: () => void;
}

const ThemeContext = createContext<ThemeContextValue | null>(null);

function applyToDocument(resolved: ResolvedTheme) {
  const root = document.documentElement;
  root.classList.toggle('dark', resolved === 'dark');
  root.classList.toggle('light', resolved === 'light');
  // colorScheme tells the browser to render native form controls + scrollbars
  // in the matching style — important for <select>, <input type="date">, etc.
  root.style.colorScheme = resolved;
  // Also tell Tauri's native window to switch appearance so the macOS title
  // bar (traffic-light area) matches. Without this, the title bar stays in
  // whatever appearance the window was created with.
  getCurrentWindow()
    .setTheme(resolved)
    .catch((err) => console.warn('[theme] setTheme failed:', err));
}

function detectSystem(): ResolvedTheme {
  if (typeof window === 'undefined' || !window.matchMedia) return 'dark';
  return window.matchMedia('(prefers-color-scheme: light)').matches ? 'light' : 'dark';
}

function resolve(choice: ThemeChoice): ResolvedTheme {
  return choice === 'system' ? detectSystem() : choice;
}

export function ThemeProvider({ children }: { children: ReactNode }) {
  // Default to dark — applied immediately so first paint isn't flash-of-white.
  // Once settings load we may swap to light or system.
  const [theme, setThemeState] = useState<ThemeChoice>('dark');
  const [resolved, setResolved] = useState<ResolvedTheme>('dark');

  // Apply the dark class on mount before settings even load (avoids FOUC).
  useEffect(() => { applyToDocument('dark'); }, []);

  // Hydrate from persisted settings.
  useEffect(() => {
    getSettings()
      .then((s) => {
        const choice = (s.theme === 'light' || s.theme === 'system') ? s.theme : 'dark';
        setThemeState(choice);
        const r = resolve(choice);
        setResolved(r);
        applyToDocument(r);
      })
      .catch(() => {});
  }, []);

  // If user picked 'system', follow OS changes.
  useEffect(() => {
    if (theme !== 'system' || typeof window === 'undefined' || !window.matchMedia) return;
    const mq = window.matchMedia('(prefers-color-scheme: light)');
    const onChange = () => {
      const r = mq.matches ? 'light' : 'dark';
      setResolved(r);
      applyToDocument(r);
    };
    mq.addEventListener('change', onChange);
    return () => mq.removeEventListener('change', onChange);
  }, [theme]);

  const setTheme = useCallback((next: ThemeChoice) => {
    setThemeState(next);
    const r = resolve(next);
    setResolved(r);
    applyToDocument(r);
    // Persist asynchronously; merge into existing settings so we don't clobber.
    getSettings()
      .then((s) => updateSettings({ ...s, theme: next }))
      .catch(() => {});
  }, []);

  const toggle = useCallback(() => {
    setTheme(resolved === 'dark' ? 'light' : 'dark');
  }, [resolved, setTheme]);

  return (
    <ThemeContext.Provider value={{ theme, resolved, setTheme, toggle }}>
      {children}
    </ThemeContext.Provider>
  );
}

export function useTheme(): ThemeContextValue {
  const ctx = useContext(ThemeContext);
  if (!ctx) throw new Error('useTheme must be used inside <ThemeProvider>');
  return ctx;
}
