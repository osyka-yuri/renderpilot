export type ThemeMode = 'system' | 'dark' | 'light';
export type ResolvedThemeMode = Exclude<ThemeMode, 'system'>;

const STORAGE_KEY = 'renderpilot.theme-mode';

const DEFAULT_THEME_MODE: ThemeMode = 'system';
const DEFAULT_RESOLVED_THEME: ResolvedThemeMode = 'dark';

const SYSTEM_THEME_QUERY = '(prefers-color-scheme: light)';

const THEME_MODES = ['system', 'dark', 'light'] as const;

export function readStoredThemeMode(): ThemeMode {
  const storage = getLocalStorage();

  if (!storage) {
    return DEFAULT_THEME_MODE;
  }

  try {
    return normalizeThemeMode(storage.getItem(STORAGE_KEY));
  } catch {
    return DEFAULT_THEME_MODE;
  }
}

export function persistThemeMode(mode: ThemeMode): void {
  const storage = getLocalStorage();

  if (!storage) {
    return;
  }

  try {
    storage.setItem(STORAGE_KEY, mode);
  } catch {
    // Ignore storage errors: private mode, quota exceeded, disabled storage, etc.
  }
}

export function applyThemeMode(mode: ThemeMode): void {
  const root = getDocumentRoot();

  if (!root) {
    return;
  }

  root.dataset.themeMode = mode;
  root.dataset.theme = resolveThemeMode(mode);
}

export function observeSystemTheme(listener: () => void): () => void {
  const mediaQuery = getSystemThemeMediaQuery();

  if (!mediaQuery) {
    return noop;
  }

  mediaQuery.addEventListener('change', listener);

  return () => {
    mediaQuery.removeEventListener('change', listener);
  };
}

function resolveThemeMode(mode: ThemeMode): ResolvedThemeMode {
  if (mode !== 'system') {
    return mode;
  }

  return getSystemThemeMode();
}

function getSystemThemeMode(): ResolvedThemeMode {
  const mediaQuery = getSystemThemeMediaQuery();

  if (!mediaQuery) {
    return DEFAULT_RESOLVED_THEME;
  }

  return mediaQuery.matches ? 'light' : 'dark';
}

function getSystemThemeMediaQuery(): MediaQueryList | null {
  if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') {
    return null;
  }

  return window.matchMedia(SYSTEM_THEME_QUERY);
}

function getLocalStorage(): Storage | null {
  if (typeof window === 'undefined') {
    return null;
  }

  try {
    return window.localStorage;
  } catch {
    return null;
  }
}

function getDocumentRoot(): HTMLElement | null {
  if (typeof document === 'undefined') {
    return null;
  }

  return document.documentElement;
}

function normalizeThemeMode(value: string | null): ThemeMode {
  return isThemeMode(value) ? value : DEFAULT_THEME_MODE;
}

function isThemeMode(value: string | null): value is ThemeMode {
  return value !== null && THEME_MODES.includes(value as ThemeMode);
}

function noop(): void {
  return;
}
