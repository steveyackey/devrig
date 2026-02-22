/**
 * Dark / light mode management using localStorage.
 *
 * Persists the user's preference in localStorage under the key "devrig-theme".
 * Falls back to the system preference (prefers-color-scheme) when no stored
 * preference exists.
 *
 * The theme is applied by toggling a "dark" class on the <html> element
 * (registered via `@custom-variant` in index.css) and setting `data-theme`
 * for CSS variable overrides (Tailwind v4 CSS-based config).
 *
 * Usage:
 *   import { theme, setTheme, toggleTheme, initTheme } from '../lib/theme';
 *
 *   // Call once at app startup
 *   initTheme();
 *
 *   // Read current theme reactively
 *   const current = theme(); // 'dark' | 'light'
 *
 *   // Toggle
 *   toggleTheme();
 *
 *   // Set explicitly
 *   setTheme('light');
 */

import { createSignal } from 'solid-js';

export type Theme = 'dark' | 'light';

const STORAGE_KEY = 'devrig-theme';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function getSystemPreference(): Theme {
  if (typeof window === 'undefined') return 'dark';
  return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
}

function getStoredPreference(): Theme | null {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (stored === 'dark' || stored === 'light') return stored;
  } catch {
    // localStorage may be unavailable (SSR, privacy mode, etc.)
  }
  return null;
}

function storePreference(theme: Theme) {
  try {
    localStorage.setItem(STORAGE_KEY, theme);
  } catch {
    // Silently ignore storage errors
  }
}

function applyThemeToDocument(theme: Theme) {
  if (typeof document === 'undefined') return;
  const root = document.documentElement;
  const body = document.body;
  // Set data-theme attribute for CSS variable overrides
  root.setAttribute('data-theme', theme);
  body.setAttribute('data-theme', theme);
  // Set class on both html and body (tests check body class)
  if (theme === 'dark') {
    root.classList.add('dark');
    root.classList.remove('light');
    body.classList.add('dark');
    body.classList.remove('light');
  } else {
    root.classList.add('light');
    root.classList.remove('dark');
    body.classList.add('light');
    body.classList.remove('dark');
  }
}

// ---------------------------------------------------------------------------
// Reactive state
// ---------------------------------------------------------------------------

const [_theme, _setTheme] = createSignal<Theme>('dark');

/**
 * Reactive signal returning the current theme ('dark' | 'light').
 */
export const theme = _theme;

/**
 * Set the theme explicitly. Persists the choice and updates the DOM.
 */
export function setTheme(value: Theme) {
  _setTheme(value);
  storePreference(value);
  applyThemeToDocument(value);
}

/**
 * Toggle between dark and light mode.
 */
export function toggleTheme() {
  setTheme(_theme() === 'dark' ? 'light' : 'dark');
}

/**
 * Initialize the theme from localStorage or system preference.
 * Should be called once at application startup (e.g., in index.tsx or App.tsx).
 */
export function initTheme() {
  const initial = getStoredPreference() ?? getSystemPreference();
  _setTheme(initial);
  applyThemeToDocument(initial);

  // Listen for OS-level preference changes
  if (typeof window !== 'undefined') {
    const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)');
    const handler = (e: MediaQueryListEvent) => {
      // Only follow system preference if the user hasn't explicitly chosen
      if (!getStoredPreference()) {
        const next = e.matches ? 'dark' : 'light';
        _setTheme(next);
        applyThemeToDocument(next);
      }
    };
    mediaQuery.addEventListener('change', handler);
    // No cleanup needed -- this is a singleton intended to live for the app's lifetime
  }
}

/**
 * Remove the stored preference so the app falls back to the system preference.
 */
export function clearThemePreference() {
  try {
    localStorage.removeItem(STORAGE_KEY);
  } catch {
    // Silently ignore
  }
  const system = getSystemPreference();
  _setTheme(system);
  applyThemeToDocument(system);
}
