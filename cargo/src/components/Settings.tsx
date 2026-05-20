import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/tauri';
import { open } from '@tauri-apps/api/shell';
import {
  RefreshCw,
  Database,
  FolderCog,
  FolderOpen,
  Globe,
  HardDrive,
  Save,
  Settings2,
  Trash2,
} from 'lucide-react';
import { useTheme } from '../contexts/ThemeContext';
import { THEME_OPTIONS, Theme } from '../contexts/theme';
import { useToast } from '../contexts/ToastContext';
import { OpsPageShell } from './layout/OpsPageShell';
import { AppModal } from './shared/AppModal';

interface Setting {
  key: string;
  value: string;
  value_type: string;
}

interface SettingsProps {
  onBack: () => void;
}

interface AppReleaseMetadata {
  version: string;
  channel: 'release' | 'experimental';
}

interface CachePathSummary {
  key: string;
  label: string;
  path: string;
  exists: boolean;
  file_count: number;
  directory_count: number;
  total_bytes: number;
}

interface CacheClearSummary {
  removable_paths: CachePathSummary[];
  preserved_paths: CachePathSummary[];
  os_catalog_entries: number;
  driver_catalog_entries: number;
  driver_cache_records: number;
}

interface CacheClearResult {
  summary: CacheClearSummary;
  deleted_files: number;
  deleted_directories: number;
  deleted_bytes: number;
  warnings: string[];
}

export function Settings({ onBack }: SettingsProps) {
  const { theme, setTheme } = useTheme();
  const { showToast } = useToast();
  const [version, setVersion] = useState('2.0.6');
  const [releaseChannel, setReleaseChannel] = useState<'release' | 'experimental'>('release');
  const [settings, setSettings] = useState<Record<string, string>>({});
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [syncing, setSyncing] = useState(false);
  const [cacheSummary, setCacheSummary] = useState<CacheClearSummary | null>(null);
  const [cacheModalOpen, setCacheModalOpen] = useState(false);
  const [clearingCache, setClearingCache] = useState(false);

  useEffect(() => {
    loadSettings();
    invoke<string>('get_app_version').then(setVersion).catch(() => {});
    invoke<AppReleaseMetadata>('get_app_release_metadata')
      .then((metadata) => setReleaseChannel(metadata.channel))
      .catch(() => {});
  }, []);

  const loadSettings = async () => {
    try {
      setLoading(true);
      const data = await invoke<Setting[]>('get_settings');
      const settingsMap: Record<string, string> = {};
      data.forEach((setting) => {
        settingsMap[setting.key] = setting.value;
      });
      setSettings(settingsMap);
    } catch (err) {
      console.error('Failed to load settings:', err);
      showToast('Failed to load settings', 'error');
    } finally {
      setLoading(false);
    }
  };

  const handleSave = async () => {
    try {
      setSaving(true);

      const promises = Object.entries(settings).map(([key, value]) => {
        const valueType = getValueType(key);
        return invoke('set_setting', { key, value, valueType });
      });

      await Promise.all(promises);
      showToast('Settings saved successfully', 'success');
    } catch (err) {
      console.error('Failed to save settings:', err);
      showToast('Failed to save settings', 'error');
    } finally {
      setSaving(false);
    }
  };

  const handleSyncOsCatalog = async () => {
    try {
      setSyncing(true);
      await invoke('sync_os_catalog');
      showToast('OS catalog synced successfully', 'success');
    } catch (err) {
      console.error('Failed to sync OS catalog:', err);
      showToast('Failed to sync OS catalog', 'error');
    } finally {
      setSyncing(false);
    }
  };

  const handleSyncDriverCatalog = async () => {
    try {
      setSyncing(true);
      const result = await invoke<{ started: boolean; synced_sources: number; errors: string[] }>('sync_driver_catalog');
      if (result.errors.length > 0) {
        showToast(
          `Driver sync completed with ${result.errors.length} issue(s). Synced ${result.synced_sources} source(s).`,
          'warning'
        );
      } else {
        showToast(`Driver catalog synced (${result.synced_sources} sources)`, 'success');
      }
    } catch (err) {
      console.error('Failed to sync driver catalog:', err);
      showToast('Failed to sync driver catalog', 'error');
    } finally {
      setSyncing(false);
    }
  };

  const handleBrowseFolder = async (settingKey: string, title: string) => {
    try {
      const result = await invoke<string | null>('show_folder_dialog', { title });
      if (result) {
        updateSetting(settingKey, result);
      }
    } catch (err) {
      console.error('Failed to open folder dialog:', err);
      showToast('Failed to open folder picker', 'error');
    }
  };

  const getValueType = (key: string): string => {
    switch (key) {
      case 'auto_sync_catalogs':
        return 'bool';
      case 'sync_frequency_hours':
        return 'int';
      default:
        return 'string';
    }
  };

  const updateSetting = (key: string, value: string) => {
    setSettings((prev) => ({ ...prev, [key]: value }));
  };

  const formatBytes = (value: number) => {
    if (value <= 0) {
      return '0 B';
    }
    const units = ['B', 'KB', 'MB', 'GB', 'TB'];
    let size = value;
    let unitIndex = 0;
    while (size >= 1024 && unitIndex < units.length - 1) {
      size /= 1024;
      unitIndex += 1;
    }
    return `${size.toFixed(size >= 10 || unitIndex === 0 ? 0 : 1)} ${units[unitIndex]}`;
  };

  const handleOpenCacheModal = async () => {
    try {
      const summary = await invoke<CacheClearSummary>('get_cache_clear_summary');
      setCacheSummary(summary);
      setCacheModalOpen(true);
    } catch (err) {
      console.error('Failed to load cache clear summary:', err);
      showToast('Failed to inspect cache contents', 'error');
    }
  };

  const handleConfirmCacheClear = async () => {
    try {
      setClearingCache(true);
      const result = await invoke<CacheClearResult>('clear_download_cache');
      setCacheSummary(result.summary);
      if (result.warnings.length > 0) {
        showToast(
          `Cache clear finished with ${result.warnings.length} issue(s). Removed ${formatBytes(result.deleted_bytes)}.`,
          'warning'
        );
      } else {
        showToast(`Removed ${formatBytes(result.deleted_bytes)} of cached content`, 'success');
      }
      setCacheModalOpen(false);
    } catch (err) {
      console.error('Failed to clear cache:', err);
      showToast('Failed to clear downloaded cache', 'error');
    } finally {
      setClearingCache(false);
    }
  };

  const autoSyncEnabled = settings.auto_sync_catalogs === 'true';

  if (loading) {
    return (
      <div className="ops-loading-screen">
        <div className="ops-spinner" />
        <p>Loading settings...</p>
      </div>
    );
  }

  return (
    <OpsPageShell
      kicker="System Configuration"
      title="Settings"
      subtitle="Control themes, workspace paths, and catalog synchronization behavior."
      onBack={onBack}
    >
      <div className="ops-layout-stack">
        <section className="ops-card">
          <div className="ops-card-heading">
            <span className="ops-card-icon">
              <Settings2 size={16} />
            </span>
            <div>
              <h2 className="ops-card-title">General</h2>
              <p className="ops-card-subtitle">Core personalization and localization options.</p>
            </div>
          </div>

          <div className="ops-form-grid">
            <div className="ops-field">
              <label className="ops-label">Theme</label>
              <select
                value={theme}
                onChange={(event) => setTheme(event.target.value as Theme)}
                className="ops-select"
              >
                {THEME_OPTIONS.map((option) => (
                  <option key={option.value} value={option.value}>
                    {option.label}
                  </option>
                ))}
              </select>
            </div>

            <div className="ops-field">
              <label className="ops-label">Language</label>
              <select
                value={settings.language || 'en-US'}
                onChange={(event) => updateSetting('language', event.target.value)}
                className="ops-select"
              >
                <option value="en-US">English (US)</option>
                <option value="en-GB">English (UK)</option>
                <option value="de-DE">German</option>
                <option value="fr-FR">French</option>
                <option value="es-ES">Spanish</option>
              </select>
            </div>
          </div>
        </section>

        <section className="ops-card">
          <div className="ops-card-heading">
            <span className="ops-card-icon">
              <FolderCog size={16} />
            </span>
            <div>
              <h2 className="ops-card-title">Paths and Directories</h2>
              <p className="ops-card-subtitle">Storage locations for downloads and build workspaces.</p>
            </div>
          </div>

          <div className="ops-layout-stack ops-compact-stack">
            <div className="ops-field">
              <label className="ops-label">Download Path</label>
              <div className="ops-input-row">
                <input
                  type="text"
                  value={settings.download_path || 'C:\\BitOSDT\\Downloads'}
                  onChange={(event) => updateSetting('download_path', event.target.value)}
                  className="ops-input"
                  placeholder="C:\\BitOSDT\\Downloads"
                />
                <button
                  type="button"
                  onClick={() => handleBrowseFolder('download_path', 'Select Download Folder')}
                  className="ops-btn ops-btn-secondary"
                >
                  <FolderOpen size={15} />
                  <span>Browse</span>
                </button>
              </div>
              <p className="ops-hint">Location where Windows images and drivers are downloaded.</p>
            </div>

            <div className="ops-field">
              <label className="ops-label">Workspace Path</label>
              <div className="ops-input-row">
                <input
                  type="text"
                  value={settings.workspace_path || 'C:\\BitOSDT\\Workspace'}
                  onChange={(event) => updateSetting('workspace_path', event.target.value)}
                  className="ops-input"
                  placeholder="C:\\BitOSDT\\Workspace"
                />
                <button
                  type="button"
                  onClick={() => handleBrowseFolder('workspace_path', 'Select Workspace Folder')}
                  className="ops-btn ops-btn-secondary"
                >
                  <FolderOpen size={15} />
                  <span>Browse</span>
                </button>
              </div>
              <p className="ops-hint">Temporary directory for image preparation and packaging.</p>
            </div>
          </div>
        </section>

        <section className="ops-card">
          <div className="ops-card-heading">
            <span className="ops-card-icon">
              <HardDrive size={16} />
            </span>
            <div>
              <h2 className="ops-card-title">Windows ADK</h2>
              <p className="ops-card-subtitle">Optional override for ADK detection.</p>
            </div>
          </div>

          <div className="ops-field">
            <label className="ops-label">ADK Installation Path</label>
            <div className="ops-input-row">
              <input
                type="text"
                value={settings.adk_path || ''}
                onChange={(event) => updateSetting('adk_path', event.target.value)}
                className="ops-input"
                placeholder="C:\\Program Files (x86)\\Windows Kits\\10"
              />
              <button
                type="button"
                onClick={() => handleBrowseFolder('adk_path', 'Select ADK Installation Folder')}
                className="ops-btn ops-btn-secondary"
              >
                <FolderOpen size={15} />
                <span>Browse</span>
              </button>
            </div>
            <p className="ops-hint">Path to the Windows Assessment and Deployment Kit.</p>
          </div>
        </section>

        <section className="ops-card">
          <div className="ops-card-heading">
            <span className="ops-card-icon">
              <Database size={16} />
            </span>
            <div>
              <h2 className="ops-card-title">Catalog Management</h2>
              <p className="ops-card-subtitle">OS and driver catalog sync strategy.</p>
            </div>
          </div>

          <div className="ops-form-grid">
            <div className="ops-field">
              <label className="ops-label">Auto-sync Catalogs</label>
              <button
                type="button"
                className={`ops-toggle ${autoSyncEnabled ? 'is-on' : ''}`}
                onClick={() => updateSetting('auto_sync_catalogs', (!autoSyncEnabled).toString())}
                aria-pressed={autoSyncEnabled}
              >
                <span className="ops-toggle-track">
                  <span className="ops-toggle-knob" />
                </span>
                <span className="ops-toggle-text">{autoSyncEnabled ? 'Enabled' : 'Disabled'}</span>
              </button>
              <p className="ops-hint">Automatically sync OS and driver catalogs on app startup.</p>
            </div>

            <div className="ops-field">
              <label className="ops-label">Sync Frequency (hours)</label>
              <input
                type="number"
                min="1"
                max="168"
                value={settings.sync_frequency_hours || '24'}
                onChange={(event) => updateSetting('sync_frequency_hours', event.target.value)}
                className="ops-input ops-input-short"
              />
            </div>
          </div>

          <div className="ops-cluster">
            <button
              type="button"
              onClick={handleSyncOsCatalog}
              disabled={syncing}
              className="ops-btn ops-btn-primary"
            >
              <RefreshCw size={15} />
              <span>{syncing ? 'Syncing...' : 'Sync OS Catalog'}</span>
            </button>

            <button
              type="button"
              onClick={handleSyncDriverCatalog}
              disabled={syncing}
              className="ops-btn ops-btn-secondary"
            >
              <RefreshCw size={15} />
              <span>{syncing ? 'Syncing...' : 'Sync Driver Catalog'}</span>
            </button>

            <button
              type="button"
              onClick={handleOpenCacheModal}
              className="ops-btn ops-btn-secondary"
            >
              <Trash2 size={15} />
              <span>Clear Download Cache</span>
            </button>
          </div>
        </section>

        <section className="ops-card">
          <div className="ops-card-heading">
            <span className="ops-card-icon">
              <Globe size={16} />
            </span>
            <div>
              <h2 className="ops-card-title">About</h2>
              <p className="ops-card-subtitle">BitOSDT release and support links.</p>
            </div>
          </div>

          <div className="ops-about-list">
            <p>
              <strong>{`BitOSDT ${version}`}</strong> - Windows Deployment Solution
            </p>
            <p>Version: {version}</p>
            <p>Channel: {releaseChannel}</p>
            <p>Create and automate custom Windows deployment media workflows.</p>
          </div>

          <div className="ops-cluster">
            <button type="button" onClick={() => open('https://bitosdt.com/docs')} className="ops-link-btn">
              Documentation
            </button>
            <button type="button" onClick={() => open('https://bitosdt.com/forum')} className="ops-link-btn">
              Community Forum
            </button>
            <button type="button" onClick={() => open('https://bitosdt.com/forum/')} className="ops-link-btn">
              Report Issue
            </button>
          </div>
        </section>

        <section className="ops-action-bar">
          <span className="ops-action-note">Review and save changes before starting image builds.</span>
          <button type="button" onClick={handleSave} disabled={saving} className="ops-btn ops-btn-primary ops-btn-save">
            <Save size={15} />
            <span>{saving ? 'Saving...' : 'Save Settings'}</span>
          </button>
        </section>
      </div>
      {cacheModalOpen && cacheSummary && (
        <AppModal open onClose={() => setCacheModalOpen(false)} labelledBy="clear-cache-title">
          <>
            <div className="ops-modal-head">
              <div>
                <h2 id="clear-cache-title" className="ops-card-title">Clear Download Cache</h2>
                <p className="ops-card-subtitle">Review what BitOSDT will remove before continuing.</p>
              </div>
            </div>
            <div className="ops-modal-body space-y-4">
              <div className="ops-modal-alert ops-modal-alert-danger">
                This clears downloaded images, driver-cache content, and synced catalog rows. Workspace builds,
                saved image profiles, and app settings stay intact.
              </div>

              <div className="space-y-3">
                <h3 className="ops-modal-section-title">Will Remove</h3>
                {cacheSummary.removable_paths.map((entry) => (
                  <div key={entry.key} className="ops-path-card">
                    <p className="ops-path-card-title">{entry.label}</p>
                    <p className="ops-path-card-path ops-break">{entry.path}</p>
                    <p className="ops-path-card-meta">
                      {entry.exists
                        ? `${entry.file_count} files, ${entry.directory_count} folders, ${formatBytes(entry.total_bytes)}`
                        : 'Path does not exist yet'}
                    </p>
                  </div>
                ))}
                <div className="ops-path-card">
                  <p>Synced OS catalog rows: {cacheSummary.os_catalog_entries}</p>
                  <p>Synced driver catalog rows: {cacheSummary.driver_catalog_entries}</p>
                  <p>Driver cache records: {cacheSummary.driver_cache_records}</p>
                </div>
              </div>

              <div className="space-y-3">
                <h3 className="ops-modal-section-title">Will Preserve</h3>
                {cacheSummary.preserved_paths.map((entry) => (
                  <div key={entry.key} className="ops-path-card ops-path-card-preserve">
                    <p className="ops-path-card-title">{entry.label}</p>
                    <p className="ops-path-card-path ops-break">{entry.path}</p>
                    <p className="ops-path-card-meta">
                      {entry.exists
                        ? `${entry.file_count} files, ${entry.directory_count} folders, ${formatBytes(entry.total_bytes)}`
                        : 'Path does not exist yet'}
                    </p>
                  </div>
                ))}
              </div>
            </div>
            <div className="ops-modal-foot">
              <button
                type="button"
                onClick={() => setCacheModalOpen(false)}
                disabled={clearingCache}
                className="ops-btn ops-btn-secondary"
              >
                Cancel
              </button>
              <button
                type="button"
                onClick={handleConfirmCacheClear}
                disabled={clearingCache}
                className="ops-btn ops-btn-primary"
              >
                <Trash2 size={15} />
                <span>{clearingCache ? 'Clearing...' : 'Clear Cache'}</span>
              </button>
            </div>
          </>
        </AppModal>
      )}
    </OpsPageShell>
  );
}
