import { useEffect, useState } from 'react';
import type { MouseEvent } from 'react';
import { appWindow } from '@tauri-apps/api/window';
import { Minus, Square, SquareStack, X } from 'lucide-react';
import { useTheme } from '../contexts/ThemeContext';
import { THEME_OPTIONS, Theme } from '../contexts/theme';

export function AppTitleBar() {
  const [isMaximized, setIsMaximized] = useState(false);
  const { theme, setTheme } = useTheme();

  useEffect(() => {
    let unlisten: (() => void) | null = null;

    const bind = async () => {
      try {
        setIsMaximized(await appWindow.isMaximized());
      } catch {
        setIsMaximized(false);
      }

      try {
        unlisten = await appWindow.onResized(async () => {
          try {
            setIsMaximized(await appWindow.isMaximized());
          } catch {
            setIsMaximized(false);
          }
        });
      } catch {
        unlisten = null;
      }
    };

    bind();

    return () => {
      if (unlisten) {
        unlisten();
      }
    };
  }, []);

  const handleMinimize = async () => {
    try {
      await appWindow.minimize();
    } catch (error) {
      console.error('Minimize failed:', error);
    }
  };

  const handleMaximizeToggle = async () => {
    try {
      await appWindow.toggleMaximize();
      setIsMaximized(await appWindow.isMaximized());
    } catch (error) {
      console.error('Toggle maximize failed:', error);
    }
  };

  const handleClose = async () => {
    try {
      await appWindow.close();
    } catch (error) {
      console.error('Close failed:', error);
    }
  };

  const handleStartDrag = async (event: MouseEvent<HTMLDivElement>) => {
    if (event.button !== 0) {
      return;
    }

    const target = event.target as HTMLElement;
    if (target.closest('button, a, input, select, textarea, [role="button"], [data-no-drag="true"]')) {
      return;
    }

    try {
      await appWindow.startDragging();
    } catch {
      // Ignore: data-tauri-drag-region still handles drag on supported runtimes.
    }
  };

  return (
    <header className="app-titlebar">
      <div
        className="app-titlebar-drag"
        data-tauri-drag-region
        onMouseDown={handleStartDrag}
        onDoubleClick={handleMaximizeToggle}
      >
        <span className="app-titlebar-dot" />
        <span className="app-titlebar-label">BitOSDT</span>
        <span className="app-titlebar-sub">Deployment Console</span>
      </div>

      <div className="app-titlebar-right" data-no-drag="true">
        <div className="app-titlebar-theme">
          <label htmlFor="app-theme-select" className="app-theme-label">
            Theme
          </label>
          <select
            id="app-theme-select"
            className="app-theme-select"
            value={theme}
            onChange={(event) => setTheme(event.target.value as Theme)}
            aria-label="Theme mode"
          >
            {THEME_OPTIONS.map((option) => (
              <option key={option.value} value={option.value}>
                {option.label}
              </option>
            ))}
          </select>
        </div>

        <div className="app-titlebar-actions" role="group" aria-label="Window controls">
          <button type="button" onClick={handleMinimize} className="app-window-btn" aria-label="Minimize">
            <Minus size={14} />
          </button>
          <button type="button" onClick={handleMaximizeToggle} className="app-window-btn" aria-label="Maximize or restore">
            {isMaximized ? <SquareStack size={12} /> : <Square size={12} />}
          </button>
          <button type="button" onClick={handleClose} className="app-window-btn app-window-btn-close" aria-label="Close">
            <X size={14} />
          </button>
        </div>
      </div>
    </header>
  );
}
