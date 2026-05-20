import { Monitor } from 'lucide-react';
import { useTheme } from '../../contexts/ThemeContext';
import { THEME_OPTIONS, Theme } from '../../contexts/theme';

export function WizardThemeToggle() {
  const { theme, setTheme } = useTheme();

  return (
    <label className="wizard-theme-picker">
      <span className="wizard-theme-picker-label">
        <Monitor size={13} />
        Theme
      </span>
      <select
        value={theme}
        onChange={(event) => setTheme(event.target.value as Theme)}
        className="wizard-theme-picker-select"
        aria-label="Wizard theme mode"
      >
        {THEME_OPTIONS.map((option) => (
          <option key={option.value} value={option.value}>
            {option.label}
          </option>
        ))}
      </select>
    </label>
  );
}
