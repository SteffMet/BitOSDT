import { createContext, useContext, useEffect, useState, ReactNode } from 'react';
import { invoke } from '@tauri-apps/api/tauri';
import {
  EffectiveTheme,
  Theme,
  SelectableTheme,
  isTheme,
  normalizeTheme,
  resolveEffectiveTheme,
} from './theme';
type Setting = {
  key: string;
  value: string;
};

interface ThemeContextType {
  theme: SelectableTheme;
  setTheme: (theme: Theme) => void;
  effectiveTheme: EffectiveTheme;
}

const ThemeContext = createContext<ThemeContextType | undefined>(undefined);
const THEME_STORAGE_KEY = 'theme';

function getStoredTheme(): SelectableTheme {
  const stored = localStorage.getItem(THEME_STORAGE_KEY);
  return normalizeTheme(stored);
}

function getSystemTheme(): EffectiveTheme {
  return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
}

function applyThemeToDocument(theme: Theme, effectiveTheme: EffectiveTheme) {
  const normalizedTheme = normalizeTheme(theme);
  document.documentElement.classList.toggle('dark', effectiveTheme === 'dark');
  document.documentElement.dataset.theme = normalizedTheme;
}

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [theme, setThemeState] = useState<SelectableTheme>(getStoredTheme);
  const [effectiveTheme, setEffectiveTheme] = useState<EffectiveTheme>('light');
  const [hasLoadedPersistedTheme, setHasLoadedPersistedTheme] = useState(false);

  useEffect(() => {
    let cancelled = false;

    const loadPersistedTheme = async () => {
      try {
        const settings = await invoke<Setting[]>('get_settings');
        const persistedTheme = settings.find((setting) => setting.key === 'theme')?.value ?? null;
        if (!cancelled && isTheme(persistedTheme)) {
          setThemeState(normalizeTheme(persistedTheme));
        }
      } catch (error) {
        console.warn('Failed to load persisted theme setting:', error);
      } finally {
        if (!cancelled) {
          setHasLoadedPersistedTheme(true);
        }
      }
    };

    loadPersistedTheme();

    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    localStorage.setItem(THEME_STORAGE_KEY, theme);
    const effective = resolveEffectiveTheme(theme, getSystemTheme());
    setEffectiveTheme(effective);
    applyThemeToDocument(theme, effective);
  }, [theme]);

  useEffect(() => {
    if (!hasLoadedPersistedTheme) {
      return;
    }

    invoke('set_setting', { key: 'theme', value: theme, valueType: 'string' }).catch((error) => {
      console.warn('Failed to persist theme setting:', error);
    });
  }, [hasLoadedPersistedTheme, theme]);

  useEffect(() => {
    if (theme !== 'system') return;

    const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)');
    const handleSystemThemeChange = (e: MediaQueryListEvent) => {
      const newEffective: EffectiveTheme = e.matches ? 'dark' : 'light';
      setEffectiveTheme(newEffective);
      applyThemeToDocument(theme, newEffective);
    };

    mediaQuery.addEventListener('change', handleSystemThemeChange);
    return () => mediaQuery.removeEventListener('change', handleSystemThemeChange);
  }, [theme]);

  const setTheme = (nextTheme: Theme) => {
    setThemeState(normalizeTheme(nextTheme));
  };

  return (
    <ThemeContext.Provider value={{ theme, setTheme, effectiveTheme }}>
      {children}
    </ThemeContext.Provider>
  );
}

export function useTheme() {
  const context = useContext(ThemeContext);
  if (!context) {
    throw new Error('useTheme must be used within a ThemeProvider');
  }
  return context;
}
