import {
  AppWindow,
  ArrowRight,
  Folder,
  Monitor,
  Package,
  Plus,
  Trash2,
} from 'lucide-react';
import { useEffect, useMemo, useState } from 'react';
import type { LucideIcon } from 'lucide-react';
import type { ShellLayoutItem, ShellLayoutState } from './types';

export interface ShellLayoutSourceItem {
  id: string;
  label: string;
  itemType: ShellLayoutItem['itemType'];
  sourceRef?: string;
  sourcePath?: string;
  shortcutTargetPath?: string;
  shortcutArguments?: string;
  shortcutWorkingDirectory?: string;
  shortcutIconPath?: string;
}

interface ShellLayoutCanvasProps {
  items: ShellLayoutSourceItem[];
  value: ShellLayoutState;
  onChange: (next: ShellLayoutState) => void;
  isWindows11: boolean;
}

interface CustomShortcutDraft {
  label: string;
  targetPath: string;
  arguments: string;
  workingDirectory: string;
  iconPath: string;
}

type ShellLayoutZone = 'desktop' | 'start' | 'taskbar';

const EMPTY_SHORTCUT_DRAFT: CustomShortcutDraft = {
  label: '',
  targetPath: '',
  arguments: '',
  workingDirectory: '',
  iconPath: '',
};

function sortItems<T extends { label: string }>(items: T[]) {
  return [...items].sort((left, right) => left.label.localeCompare(right.label));
}

function hasPlacement(item: Pick<ShellLayoutItem, ShellLayoutZone>) {
  return item.desktop || item.start || item.taskbar;
}

function buildShortcutId() {
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
    return `shortcut:${crypto.randomUUID()}`;
  }
  return `shortcut:${Date.now()}`;
}

function itemTypeLabel(itemType: ShellLayoutItem['itemType']) {
  switch (itemType) {
    case 'winget':
      return 'Winget';
    case 'chocolatey':
      return 'Chocolatey';
    case 'custom':
      return 'Custom Installer';
    case 'copied':
      return 'Copied Item';
    case 'shortcut':
      return 'Custom Shortcut';
    default:
      return itemType;
  }
}

function itemTypeIcon(itemType: ShellLayoutItem['itemType']): LucideIcon {
  switch (itemType) {
    case 'winget':
    case 'chocolatey':
      return Package;
    case 'custom':
      return AppWindow;
    case 'copied':
      return Folder;
    case 'shortcut':
      return ArrowRight;
    default:
      return Monitor;
  }
}

function iconPalette(itemType: ShellLayoutItem['itemType']) {
  switch (itemType) {
    case 'winget':
      return {
        background: 'rgba(56, 189, 248, 0.16)',
        color: '#38bdf8',
      };
    case 'chocolatey':
      return {
        background: 'rgba(99, 102, 241, 0.16)',
        color: '#818cf8',
      };
    case 'custom':
      return {
        background: 'rgba(52, 211, 153, 0.16)',
        color: '#34d399',
      };
    case 'copied':
      return {
        background: 'rgba(251, 191, 36, 0.16)',
        color: '#fbbf24',
      };
    case 'shortcut':
      return {
        background: 'rgba(244, 114, 182, 0.16)',
        color: '#f472b6',
      };
    default:
      return {
        background: 'rgba(148, 163, 184, 0.16)',
        color: '#cbd5e1',
      };
  }
}

function createCustomShortcutItem(draft: CustomShortcutDraft): ShellLayoutItem {
  return {
    id: buildShortcutId(),
    label: draft.label.trim(),
    itemType: 'shortcut',
    shortcutTargetPath: draft.targetPath.trim(),
    shortcutArguments: draft.arguments.trim() || undefined,
    shortcutWorkingDirectory: draft.workingDirectory.trim() || undefined,
    shortcutIconPath: draft.iconPath.trim() || undefined,
    desktop: true,
    start: false,
    taskbar: false,
  };
}

function createSourceFromItem(item: ShellLayoutItem): ShellLayoutSourceItem {
  return {
    id: item.id,
    label: item.label,
    itemType: item.itemType,
    sourceRef: item.sourceRef,
    sourcePath: item.sourcePath,
    shortcutTargetPath: item.shortcutTargetPath,
    shortcutArguments: item.shortcutArguments,
    shortcutWorkingDirectory: item.shortcutWorkingDirectory,
    shortcutIconPath: item.shortcutIconPath,
  };
}

export function ShellLayoutCanvas({ items, value, onChange, isWindows11 }: ShellLayoutCanvasProps) {
  const [draggingId, setDraggingId] = useState<string | null>(null);
  const [shortcutDraft, setShortcutDraft] = useState<CustomShortcutDraft>(EMPTY_SHORTCUT_DRAFT);
  const [shortcutError, setShortcutError] = useState<string | null>(null);

  const customShortcutItems = useMemo(() => {
    return sortItems(
      value.items
        .filter((item) => item.itemType === 'shortcut')
        .map((item) => createSourceFromItem(item)),
    );
  }, [value.items]);

  const availableItems = useMemo(() => {
    const merged = new Map<string, ShellLayoutSourceItem>();
    [...items, ...customShortcutItems].forEach((item) => {
      merged.set(item.id, item);
    });
    return sortItems(Array.from(merged.values()));
  }, [customShortcutItems, items]);

  const itemMap = useMemo(() => new Map(availableItems.map((item) => [item.id, item])), [availableItems]);

  useEffect(() => {
    const nextItems = value.items
      .filter((item) => itemMap.has(item.id))
      .map((item) => {
        const latest = itemMap.get(item.id)!;
        return {
          ...item,
          label: latest.label,
          itemType: latest.itemType,
          sourceRef: latest.sourceRef,
          sourcePath: latest.sourcePath,
          shortcutTargetPath: latest.shortcutTargetPath,
          shortcutArguments: latest.shortcutArguments,
          shortcutWorkingDirectory: latest.shortcutWorkingDirectory,
          shortcutIconPath: latest.shortcutIconPath,
        };
      });

    const changed =
      nextItems.length !== value.items.length
      || nextItems.some((item, index) => JSON.stringify(item) !== JSON.stringify(value.items[index]));

    if (changed) {
      onChange({ ...value, items: nextItems });
    }
  }, [itemMap, onChange, value]);

  const updateCanvas = (nextItems: ShellLayoutItem[]) => {
    onChange({
      enabled: nextItems.some((item) => hasPlacement(item)) ? value.enabled : false,
      items: sortItems(nextItems),
    });
  };

  const placeItem = (itemId: string, zone: ShellLayoutZone) => {
    const source = itemMap.get(itemId);
    if (!source) {
      return;
    }

    const existing = value.items.find((item) => item.id === itemId);
    const base: ShellLayoutItem = existing ?? {
      id: source.id,
      label: source.label,
      itemType: source.itemType,
      sourceRef: source.sourceRef,
      sourcePath: source.sourcePath,
      shortcutTargetPath: source.shortcutTargetPath,
      shortcutArguments: source.shortcutArguments,
      shortcutWorkingDirectory: source.shortcutWorkingDirectory,
      shortcutIconPath: source.shortcutIconPath,
      desktop: false,
      start: false,
      taskbar: false,
    };

    const nextItem: ShellLayoutItem = { ...base, [zone]: true };
    onChange({
      enabled: true,
      items: sortItems([
        ...value.items.filter((item) => item.id !== itemId),
        nextItem,
      ]),
    });
  };

  const removePlacement = (itemId: string, zone: ShellLayoutZone) => {
    const existing = value.items.find((item) => item.id === itemId);
    if (!existing) {
      return;
    }

    const nextItem = { ...existing, [zone]: false };
    updateCanvas(
      [
        ...value.items.filter((item) => item.id !== itemId),
        nextItem.itemType === 'shortcut' || hasPlacement(nextItem) ? nextItem : null,
      ].filter(Boolean) as ShellLayoutItem[],
    );
  };

  const removeCustomShortcut = (itemId: string) => {
    updateCanvas(value.items.filter((item) => item.id !== itemId));
  };

  const addCustomShortcut = () => {
    if (!shortcutDraft.label.trim()) {
      setShortcutError('Shortcut label is required.');
      return;
    }
    if (!shortcutDraft.targetPath.trim()) {
      setShortcutError('Shortcut target path is required.');
      return;
    }

    const nextItem = createCustomShortcutItem(shortcutDraft);
    onChange({
      enabled: true,
      items: sortItems([...value.items, nextItem]),
    });
    setShortcutDraft(EMPTY_SHORTCUT_DRAFT);
    setShortcutError(null);
  };

  const zoneItems = (zone: ShellLayoutZone) => sortItems(value.items.filter((item) => item[zone]));

  const renderItemIcon = (item: ShellLayoutSourceItem | ShellLayoutItem, compact = false) => {
    const Icon = itemTypeIcon(item.itemType);
    const palette = iconPalette(item.itemType);
    return (
      <span
        className={`inline-flex items-center justify-center rounded-xl border border-white/10 ${compact ? 'h-9 w-9' : 'h-11 w-11'}`}
        style={{ background: palette.background, color: palette.color }}
      >
        <Icon size={compact ? 16 : 18} />
      </span>
    );
  };

  const renderZone = (zone: ShellLayoutZone, title: string, subtitle: string) => (
    <div
      onDragOver={(event) => event.preventDefault()}
      onDrop={(event) => {
        event.preventDefault();
        const itemId = event.dataTransfer.getData('text/plain') || draggingId;
        if (itemId) {
          placeItem(itemId, zone);
        }
        setDraggingId(null);
      }}
      className="min-h-[170px] rounded-2xl border border-dashed border-gray-300 bg-gray-50 p-4"
    >
      <div className="mb-3">
        <h4 className="text-sm font-semibold text-gray-900">{title}</h4>
        <p className="text-xs text-gray-600">{subtitle}</p>
      </div>
      <div className="space-y-2">
        {zoneItems(zone).length === 0 && (
          <p className="text-xs text-gray-500">Drag items here or use the quick add buttons below.</p>
        )}
        {zoneItems(zone).map((item) => (
          <div
            key={`${zone}-${item.id}`}
            className="flex items-center justify-between rounded-xl border border-gray-200 bg-white px-3 py-3 shadow-sm"
          >
            <div className="flex items-center gap-3">
              {renderItemIcon(item, true)}
              <div>
                <p className="text-sm font-medium text-gray-900">{item.label}</p>
                <p className="text-[11px] uppercase tracking-wide text-gray-500">{itemTypeLabel(item.itemType)}</p>
              </div>
            </div>
            <button
              type="button"
              onClick={() => removePlacement(item.id, zone)}
              className="text-xs font-semibold text-gray-500 hover:text-gray-900"
            >
              Remove
            </button>
          </div>
        ))}
      </div>
    </div>
  );

  return (
    <section className="space-y-5 rounded-2xl border border-gray-200 bg-white p-6 shadow-sm">
      <div className="flex flex-col gap-4 xl:flex-row xl:items-start xl:justify-between">
        <div className="space-y-2">
          <h3 className="text-xl font-semibold text-gray-900">Live Image Customisation Canvas</h3>
          <p className="max-w-3xl text-sm text-gray-600">
            Arrange Desktop, Start Menu, and Taskbar entries for the first sign-in experience. BitOSDT
            turns these placements into shell layout artefacts after deployment.
          </p>
        </div>
        <button
          type="button"
          onClick={() => onChange({ ...value, enabled: !value.enabled })}
          className={`ops-toggle ${value.enabled ? 'is-on' : ''}`}
          aria-pressed={value.enabled}
        >
          <span className="ops-toggle-track">
            <span className="ops-toggle-knob" />
          </span>
          <span className="ops-toggle-text">{value.enabled ? 'Enabled' : 'Disabled'}</span>
        </button>
      </div>

      {!isWindows11 && (
        <div className="rounded-xl border border-amber-200 bg-amber-50 p-4 text-sm text-amber-900">
          Desktop customisation is currently supported only for Windows 11 builds.
        </div>
      )}

      <div
        className="rounded-2xl border border-gray-200 p-4"
        style={{
          background:
            'radial-gradient(circle at top, rgba(96, 165, 250, 0.14), transparent 34%), var(--wiz-surface-terminal)',
          borderColor: 'var(--wiz-border-strong)',
          boxShadow: 'inset 0 1px 0 rgba(255, 255, 255, 0.05)',
        }}
      >
        <div className="grid gap-4 lg:grid-cols-[1.3fr_1fr]">
          <div
            className="rounded-2xl border p-4"
            style={{
              background:
                'radial-gradient(circle at top, rgba(148, 163, 184, 0.18), transparent 40%), rgba(15, 23, 42, 0.55)',
              borderColor: 'rgba(148, 163, 184, 0.28)',
            }}
          >
            <div className="mb-4 flex items-center justify-between text-xs uppercase tracking-[0.2em] text-slate-300">
              <span>Desktop Preview</span>
              <span>First Login</span>
            </div>
            <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
              {zoneItems('desktop').map((item) => (
                <div
                  key={`preview-desktop-${item.id}`}
                  className="rounded-xl border border-white/10 bg-slate-950/35 p-3 text-center"
                >
                  <div className="mx-auto mb-2 w-fit">{renderItemIcon(item)}</div>
                  <p className="text-xs font-medium text-slate-100">{item.label}</p>
                </div>
              ))}
              {zoneItems('desktop').length === 0 && (
                <div className="col-span-full rounded-xl border border-dashed border-slate-600 p-4 text-xs text-slate-400">
                  Desktop shortcuts appear here once you place them on the canvas.
                </div>
              )}
            </div>
          </div>

          <div className="space-y-4">
            <div
              className="rounded-2xl border p-4"
              style={{
                background: 'rgba(15, 23, 42, 0.55)',
                borderColor: 'rgba(148, 163, 184, 0.28)',
              }}
            >
              <div className="mb-3 text-xs uppercase tracking-[0.2em] text-slate-300">Start Menu</div>
              <div className="grid grid-cols-2 gap-2">
                {zoneItems('start').map((item) => (
                  <div
                    key={`preview-start-${item.id}`}
                    className="rounded-xl border border-white/10 bg-slate-950/35 px-3 py-4 text-xs font-medium text-slate-100"
                  >
                    <div className="mb-2">{renderItemIcon(item, true)}</div>
                    {item.label}
                  </div>
                ))}
                {zoneItems('start').length === 0 && (
                  <div className="col-span-full rounded-xl border border-dashed border-slate-700 p-3 text-xs text-slate-400">
                    Start Menu pins appear here.
                  </div>
                )}
              </div>
            </div>

            <div
              className="rounded-2xl border p-4"
              style={{
                background: 'rgba(15, 23, 42, 0.55)',
                borderColor: 'rgba(148, 163, 184, 0.28)',
              }}
            >
              <div className="mb-3 text-xs uppercase tracking-[0.2em] text-slate-300">Taskbar</div>
              <div className="flex flex-wrap gap-2">
                {zoneItems('taskbar').map((item) => (
                  <div
                    key={`preview-taskbar-${item.id}`}
                    className="flex items-center gap-2 rounded-full border border-white/10 bg-slate-950/35 px-3 py-2 text-xs font-medium text-slate-100"
                  >
                    {renderItemIcon(item, true)}
                    <span>{item.label}</span>
                  </div>
                ))}
                {zoneItems('taskbar').length === 0 && (
                  <div className="rounded-full border border-dashed border-slate-700 px-3 py-2 text-xs text-slate-400">
                    Taskbar pins appear here.
                  </div>
                )}
              </div>
            </div>
          </div>
        </div>
      </div>

      <div className="rounded-2xl border border-gray-200 bg-gray-50 p-4">
        <div className="mb-4 flex flex-col gap-2 md:flex-row md:items-start md:justify-between">
          <div>
            <h4 className="text-sm font-semibold text-gray-900">Custom Shortcuts</h4>
            <p className="text-xs text-gray-500">
              Add a shortcut manually when you want to pin a known executable or script path instead of waiting
              for an app installer to expose a shortcut.
            </p>
          </div>
          <button
            type="button"
            onClick={addCustomShortcut}
            className="inline-flex items-center justify-center gap-2 rounded-lg bg-blue-600 px-3 py-2 text-sm font-medium text-white hover:bg-blue-700"
          >
            <Plus size={16} />
            Add Shortcut
          </button>
        </div>

        <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-5">
          <input
            type="text"
            value={shortcutDraft.label}
            onChange={(event) => setShortcutDraft((current) => ({ ...current, label: event.target.value }))}
            placeholder="Label"
            className="w-full rounded-lg border border-gray-300 px-3 py-2 text-sm text-gray-900"
          />
          <input
            type="text"
            value={shortcutDraft.targetPath}
            onChange={(event) => setShortcutDraft((current) => ({ ...current, targetPath: event.target.value }))}
            placeholder="Target path or .lnk file"
            className="w-full rounded-lg border border-gray-300 px-3 py-2 text-sm text-gray-900 xl:col-span-2"
          />
          <input
            type="text"
            value={shortcutDraft.arguments}
            onChange={(event) => setShortcutDraft((current) => ({ ...current, arguments: event.target.value }))}
            placeholder="Arguments (optional)"
            className="w-full rounded-lg border border-gray-300 px-3 py-2 text-sm text-gray-900"
          />
          <input
            type="text"
            value={shortcutDraft.workingDirectory}
            onChange={(event) =>
              setShortcutDraft((current) => ({ ...current, workingDirectory: event.target.value }))
            }
            placeholder="Working directory (optional)"
            className="w-full rounded-lg border border-gray-300 px-3 py-2 text-sm text-gray-900"
          />
        </div>
        <div className="mt-3 grid gap-3 md:grid-cols-[minmax(0,1fr)_auto]">
          <input
            type="text"
            value={shortcutDraft.iconPath}
            onChange={(event) => setShortcutDraft((current) => ({ ...current, iconPath: event.target.value }))}
            placeholder="Icon path (optional)"
            className="w-full rounded-lg border border-gray-300 px-3 py-2 text-sm text-gray-900"
          />
          <div className="rounded-lg border border-blue-200 bg-blue-50 px-3 py-2 text-xs text-blue-900">
            New shortcuts are added to Desktop by default and can then be pinned to Start Menu or Taskbar.
          </div>
        </div>
        {shortcutError && (
          <div className="mt-3 rounded-lg border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-700">
            {shortcutError}
          </div>
        )}
      </div>

      <div className="rounded-2xl border border-gray-200 bg-gray-50 p-4">
        <div className="mb-4">
          <h4 className="text-sm font-semibold text-gray-900">Available Items</h4>
          <p className="text-xs text-gray-500">
            Use quick add actions if drag and drop is awkward in the current environment. Drag and drop still works
            when the canvas is enabled.
          </p>
        </div>

        {availableItems.length === 0 ? (
          <p className="text-sm text-gray-500">
            Add applications, copied payload items, or custom shortcuts above to start building the desktop layout.
          </p>
        ) : (
          <div className="grid gap-3 lg:grid-cols-2">
            {availableItems.map((item) => (
              <div
                key={item.id}
                draggable={value.enabled}
                onDragStart={(event) => {
                  event.dataTransfer.setData('text/plain', item.id);
                  setDraggingId(item.id);
                }}
                onDragEnd={() => setDraggingId(null)}
                className={`rounded-2xl border p-4 transition ${
                  value.enabled
                    ? 'border-gray-200 bg-white shadow-sm hover:border-blue-300'
                    : 'border-gray-200 bg-gray-100'
                }`}
              >
                <div className="flex items-start justify-between gap-3">
                  <div className="flex min-w-0 items-start gap-3">
                    {renderItemIcon(item)}
                    <div className="min-w-0">
                      <p className="truncate text-sm font-semibold text-gray-900">{item.label}</p>
                      <p className="text-[11px] uppercase tracking-wide text-gray-500">{itemTypeLabel(item.itemType)}</p>
                      {(item.shortcutTargetPath || item.sourcePath) && (
                        <p className="mt-1 truncate text-xs text-gray-500">
                          {item.shortcutTargetPath || item.sourcePath}
                        </p>
                      )}
                    </div>
                  </div>
                  {item.itemType === 'shortcut' && (
                    <button
                      type="button"
                      onClick={() => removeCustomShortcut(item.id)}
                      className="inline-flex h-9 w-9 items-center justify-center rounded-full border border-gray-200 text-gray-500 hover:border-red-300 hover:text-red-600"
                      aria-label={`Remove ${item.label}`}
                    >
                      <Trash2 size={16} />
                    </button>
                  )}
                </div>

                <div className="mt-4 flex flex-wrap gap-2">
                  <button
                    type="button"
                    onClick={() => placeItem(item.id, 'desktop')}
                    className="rounded-full border border-gray-300 bg-white px-3 py-2 text-xs font-semibold text-gray-900 hover:border-blue-400 hover:text-blue-700"
                  >
                    Desktop
                  </button>
                  <button
                    type="button"
                    onClick={() => placeItem(item.id, 'start')}
                    className="rounded-full border border-gray-300 bg-white px-3 py-2 text-xs font-semibold text-gray-900 hover:border-blue-400 hover:text-blue-700"
                  >
                    Start Menu
                  </button>
                  <button
                    type="button"
                    onClick={() => placeItem(item.id, 'taskbar')}
                    className="rounded-full border border-gray-300 bg-white px-3 py-2 text-xs font-semibold text-gray-900 hover:border-blue-400 hover:text-blue-700"
                  >
                    Taskbar
                  </button>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>

      {value.enabled && (
        <div className="grid gap-4 xl:grid-cols-3">
          {renderZone('desktop', 'Desktop', 'Creates public desktop shortcuts for first sign-in.')}
          {renderZone('start', 'Start Menu', 'Generates Start Menu layout XML for Windows 11.')}
          {renderZone('taskbar', 'Taskbar', 'Generates taskbar pin entries in the same layout XML.')}
        </div>
      )}

      <div className="rounded-xl border border-blue-200 bg-blue-50 px-4 py-3 text-sm text-blue-900">
        BitOSDT now stores manual shortcut targets in the canvas state, so Desktop, Start Menu, and Taskbar
        placement does not depend purely on drag and drop or label-only shortcut guessing.
      </div>
    </section>
  );
}
