export type Theme = 'light' | 'dark' | 'space' | 'system' | 'light-experimental' | 'dark-experimental';
export type EffectiveTheme = 'light' | 'dark';
export type SelectableTheme = Exclude<Theme, 'light-experimental' | 'dark-experimental'>;

export const THEME_OPTIONS: Array<{ value: SelectableTheme; label: string }> = [
  { value: 'light', label: 'Light' },
  { value: 'dark', label: 'Dark' },
  { value: 'space', label: 'Space' },
  { value: 'system', label: 'Auto (System)' },
];

export function normalizeTheme(value: string | null | undefined): SelectableTheme {
  switch (value) {
    case 'light':
    case 'light-experimental':
      return 'light';
    case 'dark':
      return 'dark';
    case 'space':
    case 'dark-experimental':
      return 'space';
    case 'system':
      return 'system';
    default:
      return 'system';
  }
}

export function isTheme(value: string | null): value is Theme {
  return (
    value === 'light' ||
    value === 'dark' ||
    value === 'space' ||
    value === 'system' ||
    value === 'light-experimental' ||
    value === 'dark-experimental'
  );
}

export function resolveEffectiveTheme(theme: Theme, systemTheme: EffectiveTheme): EffectiveTheme {
  const normalizedTheme = normalizeTheme(theme);

  if (normalizedTheme === 'system') {
    return systemTheme;
  }

  if (normalizedTheme === 'light') {
    return 'light';
  }

  return 'dark';
}

export function getThemeLabel(theme: Theme, effectiveTheme?: EffectiveTheme): string {
  switch (normalizeTheme(theme)) {
    case 'dark':
      return 'Dark';
    case 'light':
      return 'Light';
    case 'space':
      return 'Space';
    case 'system':
      if (!effectiveTheme) {
        return 'Auto (System)';
      }
      return `System (${effectiveTheme === 'dark' ? 'Dark' : 'Light'})`;
    default:
      return 'Auto (System)';
  }
}
