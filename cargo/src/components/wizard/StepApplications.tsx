import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/tauri';
import { useWizard } from './WizardContext';
import { AppConfig, WingetPackage, ChocolateyPackage, CustomInstaller, PostInstallScript } from './types';
import { LocalPayloadItem, deriveLocalPayloadDisplayName } from '../../types/localPayload';
import {
  PackageOptionTiles,
  POPULAR_CHOCO_OPTIONS,
  POPULAR_WINGET_OPTIONS,
} from '../shared/PackageOptionTiles';
import { ShellLayoutCanvas, ShellLayoutSourceItem } from './ShellLayoutCanvas';

type TabType = 'winget' | 'chocolatey' | 'custom' | 'files' | 'scripts' | 'desktop-customisation';
type InstallerSourceType = CustomInstaller['sourceType'];

const DEFAULT_SCRIPT_CONTENT = [
  '# Runs after OS deployment completes.',
  '$timestamp = Get-Date -Format "yyyy-MM-dd HH:mm:ss"',
  'Write-Host "Custom script executed at $timestamp"',
].join('\n');

const INSTALLER_EXTENSIONS: Record<CustomInstaller['installerType'], string[]> = {
  Exe: ['exe'],
  Msi: ['msi'],
  Msix: ['msix'],
};

function hasExpectedInstallerExtension(path: string, installerType: CustomInstaller['installerType']) {
  const filename = path.split(/[\\/]/).pop() || '';
  const ext = filename.includes('.') ? filename.split('.').pop()?.toLowerCase() : '';
  if (!ext) {
    return false;
  }
  return INSTALLER_EXTENSIONS[installerType].includes(ext);
}

function sourceTypeLabel(sourceType: InstallerSourceType) {
  switch (sourceType) {
    case 'EmbeddedFile':
      return 'Embedded file';
    case 'NetworkDirectory':
      return 'UNC directory';
    default:
      return 'Direct path/URL';
  }
}

function prettifyPackageLabel(value: string) {
  const tail = value.split('.').filter(Boolean).pop() || value;
  return tail
    .replace(/([a-z])([A-Z])/g, '$1 $2')
    .replace(/[-_]/g, ' ')
    .trim();
}

function buildShellLayoutSourceItems(apps: AppConfig): ShellLayoutSourceItem[] {
  const items: ShellLayoutSourceItem[] = [];

  apps.wingetPackages
    .filter((pkg) => pkg.enabled)
    .forEach((pkg) => {
      items.push({
        id: `winget:${pkg.packageId}`,
        label: prettifyPackageLabel(pkg.packageId),
        itemType: 'winget',
        sourceRef: pkg.packageId,
      });
    });

  apps.chocolateyPackages
    .filter((pkg) => pkg.enabled)
    .forEach((pkg) => {
      items.push({
        id: `choco:${pkg.packageName}`,
        label: prettifyPackageLabel(pkg.packageName),
        itemType: 'chocolatey',
        sourceRef: pkg.packageName,
      });
    });

  apps.customInstallers
    .filter((installer) => installer.enabled)
    .forEach((installer, index) => {
      items.push({
        id: `custom:${index}:${installer.name}`,
        label: installer.name,
        itemType: 'custom',
        sourceRef: installer.name,
        sourcePath: installer.path,
      });
    });

  apps.copiedItems.forEach((item) => {
    items.push({
      id: `copied:${item.sourcePath}`,
      label: deriveLocalPayloadDisplayName(item),
      itemType: 'copied',
      sourceRef: item.sourcePath,
      sourcePath: item.sourcePath,
    });
  });

  return items.sort((left, right) => left.label.localeCompare(right.label));
}

function createPayloadItem(sourcePath: string, sourceKind: LocalPayloadItem['sourceKind']): LocalPayloadItem {
  const trimmed = sourcePath.trim();
  return {
    sourcePath: trimmed,
    sourceKind,
    displayName: trimmed.split(/[\\/]/).filter(Boolean).pop() || trimmed,
  };
}

export function StepApplications() {
  const { state, dispatch } = useWizard();
  const { apps } = state;
  const shellLayoutSourceItems = buildShellLayoutSourceItems(apps);
  const shellLayoutItemCount = state.shellLayout.items.length;
  const isWindows11 = (state.windowsVersion.name || '').toLowerCase().includes('11');
  const selectedWingetIds = new Set(apps.wingetPackages.map((pkg) => pkg.packageId));
  const selectedChocolateyIds = new Set(
    apps.chocolateyPackages.map((pkg) => pkg.packageName),
  );
  const [activeTab, setActiveTab] = useState<TabType>('winget');
  const [customPackageId, setCustomPackageId] = useState('');
  const [customChocoName, setCustomChocoName] = useState('');
  const [customInstallerError, setCustomInstallerError] = useState<string | null>(null);
  const [customScriptError, setCustomScriptError] = useState<string | null>(null);
  const [newScriptName, setNewScriptName] = useState('');
  const [selectedScriptIndex, setSelectedScriptIndex] = useState(0);
  const [customInstaller, setCustomInstaller] = useState<CustomInstaller>({
    name: '',
    path: '',
    sourceType: 'DirectPathOrUrl',
    sourceFileName: '',
    dependencies: [],
    dependencyDestination: '',
    silentArgs: '',
    installerType: 'Exe',
    enabled: true,
  });

  useEffect(() => {
    if (apps.customScripts.length === 0) {
      setSelectedScriptIndex(0);
      return;
    }
    if (selectedScriptIndex > apps.customScripts.length - 1) {
      setSelectedScriptIndex(apps.customScripts.length - 1);
    }
  }, [apps.customScripts.length, selectedScriptIndex]);

  const updateCustomScripts = (customScripts: PostInstallScript[]) => {
    dispatch({ type: 'UPDATE_APPS', payload: { customScripts } });
  };

  const selectedScript = apps.customScripts[selectedScriptIndex];
  const selectedScriptLineCount = selectedScript
    ? Math.max(1, selectedScript.content.split('\n').length)
    : 1;

  const enabledScriptsWithIssues = apps.enableCustomScripts
    ? apps.customScripts.filter((script) => script.enabled && (!script.name.trim() || !script.content.trim()))
    : [];

  const addWingetPackage = (pkg: { packageId: string; name?: string }) => {
    const exists = apps.wingetPackages.some((p) => p.packageId === pkg.packageId);
    if (!exists) {
      const newPackage: WingetPackage = {
        packageId: pkg.packageId,
        enabled: true,
      };
      dispatch({
        type: 'UPDATE_APPS',
        payload: { wingetPackages: [...apps.wingetPackages, newPackage] },
      });
    }
  };

  const removeWingetPackage = (packageId: string) => {
    dispatch({
      type: 'UPDATE_APPS',
      payload: {
        wingetPackages: apps.wingetPackages.filter((p) => p.packageId !== packageId),
      },
    });
  };

  const addChocoPackage = (pkg: { packageName: string; name?: string }) => {
    const exists = apps.chocolateyPackages.some((p) => p.packageName === pkg.packageName);
    if (!exists) {
      const newPackage: ChocolateyPackage = {
        packageName: pkg.packageName,
        enabled: true,
      };
      dispatch({
        type: 'UPDATE_APPS',
        payload: { chocolateyPackages: [...apps.chocolateyPackages, newPackage] },
      });
    }
  };

  const removeChocoPackage = (packageName: string) => {
    dispatch({
      type: 'UPDATE_APPS',
      payload: {
        chocolateyPackages: apps.chocolateyPackages.filter((p) => p.packageName !== packageName),
      },
    });
  };

  const addCopiedItem = (item: LocalPayloadItem) => {
    if (apps.copiedItems.some((existing) => existing.sourcePath.toLowerCase() === item.sourcePath.toLowerCase())) {
      return;
    }

    dispatch({
      type: 'UPDATE_APPS',
      payload: {
        copiedItems: [...apps.copiedItems, item],
      },
    });
  };

  const removeCopiedItem = (sourcePath: string) => {
    dispatch({
      type: 'UPDATE_APPS',
      payload: {
        copiedItems: apps.copiedItems.filter((item) => item.sourcePath !== sourcePath),
      },
    });
  };

  const addInstallerDependency = (item: LocalPayloadItem) => {
    if (
      customInstaller.dependencies.some(
        (existing) => existing.sourcePath.toLowerCase() === item.sourcePath.toLowerCase(),
      )
    ) {
      return;
    }

    setCustomInstaller((prev) => ({
      ...prev,
      dependencies: [...prev.dependencies, item],
    }));
  };

  const removeInstallerDependency = (sourcePath: string) => {
    setCustomInstaller((prev) => ({
      ...prev,
      dependencies: prev.dependencies.filter((item) => item.sourcePath !== sourcePath),
    }));
  };

  const browseCopiedFile = async () => {
    try {
      const result = await invoke<string | null>('show_open_dialog', {
        title: 'Select file to copy to installed machine',
        filters: [['All Files', ['*']]],
      });
      if (result) {
        addCopiedItem(createPayloadItem(result, 'File'));
      }
    } catch (error) {
      console.error('Failed to open file picker:', error);
      setCustomInstallerError('Failed to open file picker.');
    }
  };

  const browseCopiedFolder = async () => {
    try {
      const result = await invoke<string | null>('show_folder_dialog', {
        title: 'Select folder to copy to installed machine',
      });
      if (result) {
        addCopiedItem(createPayloadItem(result, 'Directory'));
      }
    } catch (error) {
      console.error('Failed to open folder picker:', error);
      setCustomInstallerError('Failed to open folder picker.');
    }
  };

  const browseEmbeddedInstaller = async () => {
    try {
      const extensions = INSTALLER_EXTENSIONS[customInstaller.installerType];
      const filters: Array<[string, string[]]> = [
        [`${customInstaller.installerType} files`, extensions],
        ['All Files', ['*']],
      ];
      const result = await invoke<string | null>('show_open_dialog', {
        title: `Select ${customInstaller.installerType} installer`,
        filters,
      });

      if (result) {
        setCustomInstaller((prev) => ({
          ...prev,
          sourceType: 'EmbeddedFile',
          path: result,
        }));
        setCustomInstallerError(null);
      }
    } catch (error) {
      console.error('Failed to open installer picker:', error);
      setCustomInstallerError('Failed to open file picker.');
    }
  };

  const browseInstallerDependencyFile = async () => {
    try {
      const result = await invoke<string | null>('show_open_dialog', {
        title: 'Select installer dependency file',
        filters: [['All Files', ['*']]],
      });
      if (result) {
        addInstallerDependency(createPayloadItem(result, 'File'));
        setCustomInstallerError(null);
      }
    } catch (error) {
      console.error('Failed to open dependency file picker:', error);
      setCustomInstallerError('Failed to open file picker.');
    }
  };

  const browseInstallerDependencyFolder = async () => {
    try {
      const result = await invoke<string | null>('show_folder_dialog', {
        title: 'Select installer dependency folder',
      });
      if (result) {
        addInstallerDependency(createPayloadItem(result, 'Directory'));
        setCustomInstallerError(null);
      }
    } catch (error) {
      console.error('Failed to open dependency folder picker:', error);
      setCustomInstallerError('Failed to open folder picker.');
    }
  };

  const addCustomInstaller = () => {
    const name = customInstaller.name.trim();
    const path = customInstaller.path.trim();
    const sourceFileName = customInstaller.sourceFileName?.trim();

    if (!name) {
      setCustomInstallerError('Installer name is required.');
      return;
    }

    if (customInstaller.sourceType === 'EmbeddedFile') {
      if (!path) {
        setCustomInstallerError('Select a local installer file.');
        return;
      }
      if (!hasExpectedInstallerExtension(path, customInstaller.installerType)) {
        const expected = INSTALLER_EXTENSIONS[customInstaller.installerType]
          .map((ext) => `.${ext}`)
          .join(', ');
        setCustomInstallerError(`Selected file extension does not match installer type. Expected: ${expected}`);
        return;
      }
    } else if (customInstaller.sourceType === 'NetworkDirectory') {
      if (!path.startsWith('\\\\')) {
        setCustomInstallerError('UNC directory must start with \\\\.');
        return;
      }
      if (!sourceFileName) {
        setCustomInstallerError('Installer filename is required for UNC directory mode.');
        return;
      }
    } else if (!path) {
      setCustomInstallerError('Path or URL is required.');
      return;
    }

    const nextInstaller: CustomInstaller = {
      ...customInstaller,
      name,
      path,
      sourceFileName: customInstaller.sourceType === 'NetworkDirectory' ? sourceFileName : undefined,
      dependencyDestination: customInstaller.dependencyDestination?.trim() || undefined,
    };

    dispatch({
      type: 'UPDATE_APPS',
      payload: {
        customInstallers: [...apps.customInstallers, nextInstaller],
      },
    });
    setCustomInstallerError(null);
    setCustomInstaller({
      name: '',
      path: '',
      sourceType: 'DirectPathOrUrl',
      sourceFileName: '',
      dependencies: [],
      dependencyDestination: '',
      silentArgs: '',
      installerType: 'Exe',
      enabled: true,
    });
  };

  const removeCustomInstaller = (index: number) => {
    dispatch({
      type: 'UPDATE_APPS',
      payload: {
        customInstallers: apps.customInstallers.filter((_, i) => i !== index),
      },
    });
  };

  const addCustomScript = () => {
    const name = newScriptName.trim();
    if (!name) {
      setCustomScriptError('Script name is required.');
      return;
    }

    const script: PostInstallScript = {
      name,
      content: DEFAULT_SCRIPT_CONTENT,
      enabled: true,
      continueOnError: true,
    };

    const nextScripts = [...apps.customScripts, script];
    updateCustomScripts(nextScripts);
    dispatch({ type: 'UPDATE_APPS', payload: { enableCustomScripts: true } });
    setSelectedScriptIndex(nextScripts.length - 1);
    setNewScriptName('');
    setCustomScriptError(null);
  };

  const updateSelectedScript = (changes: Partial<PostInstallScript>) => {
    if (!selectedScript) {
      return;
    }

    const nextScripts = [...apps.customScripts];
    nextScripts[selectedScriptIndex] = {
      ...nextScripts[selectedScriptIndex],
      ...changes,
    };
    updateCustomScripts(nextScripts);
  };

  const removeCustomScript = (index: number) => {
    const nextScripts = apps.customScripts.filter((_, i) => i !== index);
    updateCustomScripts(nextScripts);
    setCustomScriptError(null);

    if (nextScripts.length === 0) {
      setSelectedScriptIndex(0);
      return;
    }

    if (selectedScriptIndex >= nextScripts.length) {
      setSelectedScriptIndex(nextScripts.length - 1);
    }
  };

  const moveCustomScript = (index: number, direction: -1 | 1) => {
    const target = index + direction;
    if (target < 0 || target >= apps.customScripts.length) {
      return;
    }

    const nextScripts = [...apps.customScripts];
    const [script] = nextScripts.splice(index, 1);
    nextScripts.splice(target, 0, script);
    updateCustomScripts(nextScripts);
    setSelectedScriptIndex(target);
  };

  const customInstallerReady =
    customInstaller.name.trim().length > 0 &&
    ((customInstaller.sourceType === 'NetworkDirectory'
      && customInstaller.path.trim().startsWith('\\\\')
      && (customInstaller.sourceFileName?.trim().length ?? 0) > 0)
      || (customInstaller.sourceType !== 'NetworkDirectory' && customInstaller.path.trim().length > 0));

  return (
    <div className="wizard-step space-y-6">
      <div>
        <h2 className="text-2xl font-bold text-gray-900 mb-2">Applications</h2>
        <p className="text-gray-600">
          Select applications and post-install scripts to run after deployment completes.
        </p>
      </div>

      {/* Tabs */}
      <div className="border-b border-gray-200">
        <nav className="flex flex-wrap gap-x-8">
          {[
            { id: 'winget', label: 'Winget', count: apps.wingetPackages.length },
            { id: 'chocolatey', label: 'Chocolatey', count: apps.chocolateyPackages.length },
            { id: 'custom', label: 'Custom', count: apps.customInstallers.length },
            { id: 'files', label: 'Files', count: apps.copiedItems.length },
            { id: 'scripts', label: 'Scripts', count: apps.customScripts.length },
            { id: 'desktop-customisation', label: 'Desktop Customisation', count: shellLayoutItemCount },
          ].map((tab) => (
            <button
              key={tab.id}
              onClick={() => setActiveTab(tab.id as TabType)}
              className={`py-4 px-1 border-b-2 font-medium text-sm ${
                activeTab === tab.id
                  ? 'border-blue-500 text-blue-600'
                  : 'border-transparent text-gray-500 hover:text-gray-700 hover:border-gray-300'
              }`}
            >
              {tab.label}
              {tab.count > 0 && (
                <span className="ml-2 bg-blue-100 text-blue-600 py-0.5 px-2 rounded-full text-xs">
                  {tab.count}
                </span>
              )}
            </button>
          ))}
        </nav>
      </div>

      {/* Winget Tab */}
      {activeTab === 'winget' && (
        <div className="space-y-6">
          {/* Popular Apps */}
          <div>
            <h3 className="text-lg font-semibold mb-3">Popular Applications</h3>
            <PackageOptionTiles
              items={POPULAR_WINGET_OPTIONS}
              selectedIds={selectedWingetIds}
              onToggle={(packageId) =>
                selectedWingetIds.has(packageId)
                  ? removeWingetPackage(packageId)
                  : addWingetPackage({ packageId })
              }
            />
          </div>

          {/* Custom ID */}
          <div>
            <h3 className="text-lg font-semibold mb-3">Add by Package ID</h3>
            <div className="flex space-x-3">
              <input
                type="text"
                value={customPackageId}
                onChange={(e) => setCustomPackageId(e.target.value)}
                placeholder="e.g., Microsoft.WindowsTerminal"
                className="flex-1 px-4 py-2 border border-gray-300 rounded-lg text-gray-900"
              />
              <button
                onClick={() => {
                  if (customPackageId) {
                    addWingetPackage({ packageId: customPackageId });
                    setCustomPackageId('');
                  }
                }}
                className="px-4 py-2 bg-blue-600 text-white rounded-lg hover:bg-blue-700"
              >
                Add
              </button>
            </div>
          </div>

          {/* Selected Apps */}
          {apps.wingetPackages.length > 0 && (
            <div>
              <h3 className="text-lg font-semibold mb-3">
                Selected ({apps.wingetPackages.length})
              </h3>
              <div className="space-y-2">
                {apps.wingetPackages.map((pkg) => (
                  <div
                    key={pkg.packageId}
                    className="flex items-center justify-between bg-gray-50 rounded-lg p-3"
                  >
                    <span className="font-mono text-sm text-gray-900">{pkg.packageId}</span>
                    <button
                      onClick={() => removeWingetPackage(pkg.packageId)}
                      className="text-red-600 hover:text-red-800 text-sm"
                    >
                      Remove
                    </button>
                  </div>
                ))}
              </div>
            </div>
          )}
        </div>
      )}

      {/* Chocolatey Tab */}
      {activeTab === 'chocolatey' && (
        <div className="space-y-6">
          <div className="bg-yellow-50 border border-yellow-200 rounded-lg p-4 mb-4">
            <label className="flex items-center space-x-3">
              <input
                type="checkbox"
                checked={apps.autoInstallChocolatey}
                onChange={(e) =>
                  dispatch({ type: 'UPDATE_APPS', payload: { autoInstallChocolatey: e.target.checked } })
                }
                className="w-5 h-5 text-blue-600 rounded"
              />
              <span className="text-gray-900">Automatically install Chocolatey if not present</span>
            </label>
          </div>

          {/* Popular Apps */}
          <div>
            <h3 className="text-lg font-semibold mb-3">Popular Applications</h3>
            <PackageOptionTiles
              items={POPULAR_CHOCO_OPTIONS}
              selectedIds={selectedChocolateyIds}
              onToggle={(packageName) =>
                selectedChocolateyIds.has(packageName)
                  ? removeChocoPackage(packageName)
                  : addChocoPackage({ packageName })
              }
            />
          </div>

          {/* Custom Name */}
          <div>
            <h3 className="text-lg font-semibold mb-3">Add by Package Name</h3>
            <div className="flex space-x-3">
              <input
                type="text"
                value={customChocoName}
                onChange={(e) => setCustomChocoName(e.target.value)}
                placeholder="e.g., git"
                className="flex-1 px-4 py-2 border border-gray-300 rounded-lg text-gray-900"
              />
              <button
                onClick={() => {
                  if (customChocoName) {
                    addChocoPackage({ packageName: customChocoName });
                    setCustomChocoName('');
                  }
                }}
                className="px-4 py-2 bg-blue-600 text-white rounded-lg hover:bg-blue-700"
              >
                Add
              </button>
            </div>
          </div>

          {/* Selected Apps */}
          {apps.chocolateyPackages.length > 0 && (
            <div>
              <h3 className="text-lg font-semibold mb-3">
                Selected ({apps.chocolateyPackages.length})
              </h3>
              <div className="space-y-2">
                {apps.chocolateyPackages.map((pkg) => (
                  <div
                    key={pkg.packageName}
                    className="flex items-center justify-between bg-gray-50 rounded-lg p-3"
                  >
                    <span className="font-mono text-sm text-gray-900">{pkg.packageName}</span>
                    <button
                      onClick={() => removeChocoPackage(pkg.packageName)}
                      className="text-red-600 hover:text-red-800 text-sm"
                    >
                      Remove
                    </button>
                  </div>
                ))}
              </div>
            </div>
          )}
        </div>
      )}

      {/* Custom Tab */}
      {activeTab === 'custom' && (
        <div className="space-y-6">
          <div className="bg-gray-50 rounded-lg p-6 space-y-4">
            <h3 className="text-lg font-semibold">Add Custom Installer</h3>
            <div className="grid grid-cols-1 md:grid-cols-3 gap-3">
              <input
                type="text"
                value={customInstaller.name}
                onChange={(e) => setCustomInstaller({ ...customInstaller, name: e.target.value })}
                placeholder="Installer Name"
                className="px-4 py-2 border border-gray-300 rounded-lg text-gray-900"
              />
              <select
                value={customInstaller.installerType}
                onChange={(e) =>
                  setCustomInstaller({
                    ...customInstaller,
                    installerType: e.target.value as 'Msi' | 'Exe' | 'Msix',
                  })
                }
                className="px-4 py-2 border border-gray-300 rounded-lg text-gray-900"
              >
                <option value="Exe">EXE</option>
                <option value="Msi">MSI</option>
                <option value="Msix">MSIX</option>
              </select>
              <select
                value={customInstaller.sourceType}
                onChange={(e) =>
                  setCustomInstaller({
                    ...customInstaller,
                    sourceType: e.target.value as InstallerSourceType,
                    sourceFileName:
                      e.target.value === 'NetworkDirectory' ? customInstaller.sourceFileName || '' : undefined,
                  })
                }
                className="px-4 py-2 border border-gray-300 rounded-lg text-gray-900"
              >
                <option value="EmbeddedFile">Embedded file</option>
                <option value="NetworkDirectory">UNC directory</option>
                <option value="DirectPathOrUrl">Direct path/URL</option>
              </select>
            </div>

            {customInstaller.sourceType === 'EmbeddedFile' && (
              <div className="space-y-3">
                <div className="flex space-x-3">
                  <input
                    type="text"
                    value={customInstaller.path}
                    onChange={(e) => setCustomInstaller({ ...customInstaller, path: e.target.value })}
                    placeholder={`Select local .${INSTALLER_EXTENSIONS[customInstaller.installerType].join(', .')} file`}
                    className="flex-1 px-4 py-2 border border-gray-300 rounded-lg text-gray-900"
                  />
                  <button
                    onClick={browseEmbeddedInstaller}
                    className="px-4 py-2 border border-gray-300 rounded-lg hover:bg-gray-100 text-gray-700"
                  >
                    Browse...
                  </button>
                </div>
                <p className="text-xs text-gray-500">
                  File will be embedded into Full ISO and installed from `C:\BitOSDT\Installers`.
                </p>
              </div>
            )}

            {customInstaller.sourceType === 'NetworkDirectory' && (
              <div className="space-y-3">
                <input
                  type="text"
                  value={customInstaller.path}
                  onChange={(e) => setCustomInstaller({ ...customInstaller, path: e.target.value })}
                  placeholder="UNC directory (e.g. \\\\server\\share\\apps)"
                  className="w-full px-4 py-2 border border-gray-300 rounded-lg text-gray-900"
                />
                <input
                  type="text"
                  value={customInstaller.sourceFileName || ''}
                  onChange={(e) => setCustomInstaller({ ...customInstaller, sourceFileName: e.target.value })}
                  placeholder="Installer filename (e.g. setup.msi)"
                  className="w-full px-4 py-2 border border-gray-300 rounded-lg text-gray-900"
                />
                <p className="text-xs text-gray-500">
                  Installer will run at first admin logon with credential prompt.
                </p>
              </div>
            )}

            {customInstaller.sourceType === 'DirectPathOrUrl' && (
              <input
                type="text"
                value={customInstaller.path}
                onChange={(e) => setCustomInstaller({ ...customInstaller, path: e.target.value })}
                placeholder="Path or URL to installer"
                className="w-full px-4 py-2 border border-gray-300 rounded-lg text-gray-900"
              />
            )}

            <input
              type="text"
              value={customInstaller.silentArgs}
              onChange={(e) => setCustomInstaller({ ...customInstaller, silentArgs: e.target.value })}
              placeholder="Silent arguments (e.g. /S, /quiet /norestart)"
              className="w-full px-4 py-2 border border-gray-300 rounded-lg text-gray-900"
            />

            <div className="space-y-3 border-t border-gray-200 pt-4">
              <div>
                <h4 className="text-sm font-semibold text-gray-900">Installer Dependencies</h4>
                <p className="text-xs text-gray-500 mt-1">
                  Attach local files or folders that must be copied before this installer runs.
                </p>
              </div>
              <input
                type="text"
                value={customInstaller.dependencyDestination || ''}
                onChange={(e) =>
                  setCustomInstaller({ ...customInstaller, dependencyDestination: e.target.value })
                }
                placeholder="Dependency destination (default C:\\BitOSDT\\Files\\)"
                className="w-full px-4 py-2 border border-gray-300 rounded-lg text-gray-900"
              />
              <div className="flex flex-wrap gap-3">
                <button
                  type="button"
                  onClick={browseInstallerDependencyFile}
                  className="px-4 py-2 border border-gray-300 rounded-lg hover:bg-gray-100 text-gray-700"
                >
                  Add Dependency File
                </button>
                <button
                  type="button"
                  onClick={browseInstallerDependencyFolder}
                  className="px-4 py-2 border border-gray-300 rounded-lg hover:bg-gray-100 text-gray-700"
                >
                  Add Dependency Folder
                </button>
              </div>
              {customInstaller.dependencies.length > 0 ? (
                <div className="space-y-2">
                  {customInstaller.dependencies.map((item) => (
                    <div
                      key={item.sourcePath}
                      className="flex items-start justify-between bg-white border border-gray-200 rounded-lg p-3"
                    >
                      <div className="min-w-0">
                        <p className="font-medium text-gray-900">
                          {deriveLocalPayloadDisplayName(item)}
                        </p>
                        <p className="text-xs text-gray-500">
                          {item.sourceKind} • {item.sourcePath}
                        </p>
                      </div>
                      <button
                        type="button"
                        onClick={() => removeInstallerDependency(item.sourcePath)}
                        className="text-red-600 hover:text-red-800 text-sm"
                      >
                        Remove
                      </button>
                    </div>
                  ))}
                </div>
              ) : (
                <p className="text-xs text-gray-500">No installer dependencies selected.</p>
              )}
            </div>
            {customInstallerError && (
              <div className="text-sm text-red-700 bg-red-50 border border-red-200 rounded-lg px-3 py-2">
                {customInstallerError}
              </div>
            )}
            <button
              onClick={addCustomInstaller}
              disabled={!customInstallerReady}
              className="px-4 py-2 bg-blue-600 text-white rounded-lg hover:bg-blue-700 disabled:opacity-50"
            >
              Add Installer
            </button>
          </div>

          <div>
            <h3 className="text-lg font-semibold mb-3">
              Selected ({apps.customInstallers.length})
            </h3>
            {apps.customInstallers.length === 0 ? (
              <div className="text-sm text-gray-500 bg-white border border-gray-200 rounded-lg p-4">
                No custom installers selected.
              </div>
            ) : (
              <div className="space-y-2">
                {apps.customInstallers.map((installer, index) => (
                  <div
                    key={`${installer.name}-${index}`}
                    className="flex items-start justify-between bg-gray-50 rounded-lg p-3"
                  >
                    <div className="min-w-0">
                      <p className="font-medium text-gray-900">{installer.name}</p>
                      <p className="text-xs text-gray-500">
                        {installer.installerType} • {sourceTypeLabel(installer.sourceType)} •{' '}
                        {installer.sourceType === 'NetworkDirectory'
                          ? `${installer.path}\\${installer.sourceFileName || ''}`
                          : installer.path}
                      </p>
                      {installer.silentArgs && (
                        <p className="text-xs text-gray-500 mt-1">Args: {installer.silentArgs}</p>
                      )}
                      {installer.dependencies.length > 0 && (
                        <p className="text-xs text-gray-500 mt-1">
                          Dependencies: {installer.dependencies.length} • Destination:{' '}
                          {installer.dependencyDestination || 'C:\\BitOSDT\\Files\\'}
                        </p>
                      )}
                    </div>
                    <button
                      onClick={() => removeCustomInstaller(index)}
                      className="text-red-600 hover:text-red-800 text-sm"
                    >
                      Remove
                    </button>
                  </div>
                ))}
              </div>
            )}
          </div>
        </div>
      )}

      {activeTab === 'files' && (
        <div className="space-y-6">
          <div className="bg-gray-50 rounded-lg p-6 space-y-4">
            <div>
              <h3 className="text-lg font-semibold">Files and Folders</h3>
              <p className="text-sm text-gray-600 mt-1">
                Copy local files or folders from the build machine to the installed Windows system.
              </p>
            </div>

            <div>
              <label className="block text-sm font-medium text-gray-700 mb-1">
                Destination on Installed Machine
              </label>
              <input
                type="text"
                value={apps.copyDestination || ''}
                onChange={(e) =>
                  dispatch({ type: 'UPDATE_APPS', payload: { copyDestination: e.target.value } })
                }
                placeholder="C:\\BitOSDT\\Files\\"
                className="w-full px-4 py-2 border border-gray-300 rounded-lg text-gray-900"
              />
              <p className="text-xs text-gray-500 mt-1">
                Leave blank to use <code>C:\BitOSDT\Files\</code>.
              </p>
            </div>

            <div className="flex flex-wrap gap-3">
              <button
                type="button"
                onClick={browseCopiedFile}
                className="px-4 py-2 border border-gray-300 rounded-lg hover:bg-gray-100 text-gray-700"
              >
                Add File
              </button>
              <button
                type="button"
                onClick={browseCopiedFolder}
                className="px-4 py-2 border border-gray-300 rounded-lg hover:bg-gray-100 text-gray-700"
              >
                Add Folder
              </button>
            </div>
          </div>

          <div>
            <h3 className="text-lg font-semibold mb-3">Selected ({apps.copiedItems.length})</h3>
            {apps.copiedItems.length === 0 ? (
              <div className="text-sm text-gray-500 bg-white border border-gray-200 rounded-lg p-4">
                No files or folders selected.
              </div>
            ) : (
              <div className="space-y-2">
                {apps.copiedItems.map((item) => (
                  <div
                    key={item.sourcePath}
                    className="flex items-start justify-between bg-gray-50 rounded-lg p-3"
                  >
                    <div className="min-w-0">
                      <p className="font-medium text-gray-900">
                        {deriveLocalPayloadDisplayName(item)}
                      </p>
                      <p className="text-xs text-gray-500">
                        {item.sourceKind} • {item.sourcePath}
                      </p>
                    </div>
                    <button
                      type="button"
                      onClick={() => removeCopiedItem(item.sourcePath)}
                      className="text-red-600 hover:text-red-800 text-sm"
                    >
                      Remove
                    </button>
                  </div>
                ))}
              </div>
            )}
          </div>
        </div>
      )}

      {/* Script Tab */}
      {activeTab === 'scripts' && (
        <div className="space-y-6">
          <div className="bg-gray-50 rounded-lg p-6 space-y-4">
            <div className="flex flex-col gap-3 md:flex-row md:items-center md:justify-between">
              <div>
                <h3 className="text-lg font-semibold text-gray-900">Post-Install PowerShell Scripts</h3>
                <p className="text-sm text-gray-600">
                  Scripts run after OS deployment completes via <code>SetupComplete.cmd</code>.
                </p>
              </div>
              <label className="flex items-center space-x-2">
                <input
                  type="checkbox"
                  checked={apps.enableCustomScripts}
                  onChange={(e) => {
                    dispatch({ type: 'UPDATE_APPS', payload: { enableCustomScripts: e.target.checked } });
                    setCustomScriptError(null);
                  }}
                  className="w-5 h-5 text-blue-600 rounded"
                />
                <span className="font-medium text-gray-900">Enable custom scripts</span>
              </label>
            </div>

            {apps.enableCustomScripts ? (
              <>
                <div className="flex flex-col gap-3 md:flex-row">
                  <input
                    type="text"
                    value={newScriptName}
                    onChange={(e) => setNewScriptName(e.target.value)}
                    placeholder="Script name (e.g. PostDeploy-Hardening)"
                    className="flex-1 px-4 py-2 border border-gray-300 rounded-lg text-gray-900"
                  />
                  <button
                    onClick={addCustomScript}
                    className="px-4 py-2 bg-blue-600 text-white rounded-lg hover:bg-blue-700"
                  >
                    Add Script
                  </button>
                </div>

                {customScriptError && (
                  <div className="text-sm text-red-700 bg-red-50 border border-red-200 rounded-lg px-3 py-2">
                    {customScriptError}
                  </div>
                )}

                {apps.customScripts.length === 0 ? (
                  <div className="text-sm text-gray-500 bg-white border border-gray-200 rounded-lg p-4">
                    No scripts added yet. Add a script name to start editing.
                  </div>
                ) : (
                  <div className="grid grid-cols-1 lg:grid-cols-3 gap-4">
                    <div className="space-y-2">
                      {apps.customScripts.map((script, index) => (
                        <div
                          key={`${script.name}-${index}`}
                          className={`rounded-lg border p-3 ${
                            selectedScriptIndex === index
                              ? 'border-blue-400 bg-blue-50'
                              : 'border-gray-200 bg-white'
                          }`}
                        >
                          <button
                            type="button"
                            onClick={() => setSelectedScriptIndex(index)}
                            className="w-full text-left"
                          >
                            <p className="font-medium text-gray-900 truncate">{script.name || `Script ${index + 1}`}</p>
                            <p className="text-xs text-gray-500">
                              {script.enabled ? 'Enabled' : 'Disabled'} •{' '}
                              {script.continueOnError ? 'Continue on error' : 'Stop on error'}
                            </p>
                          </button>
                          <div className="mt-2 flex items-center gap-2">
                            <button
                              type="button"
                              onClick={() => moveCustomScript(index, -1)}
                              disabled={index === 0}
                              className="px-2 py-1 text-xs border border-gray-300 rounded disabled:opacity-40"
                            >
                              Up
                            </button>
                            <button
                              type="button"
                              onClick={() => moveCustomScript(index, 1)}
                              disabled={index === apps.customScripts.length - 1}
                              className="px-2 py-1 text-xs border border-gray-300 rounded disabled:opacity-40"
                            >
                              Down
                            </button>
                            <button
                              type="button"
                              onClick={() => removeCustomScript(index)}
                              className="px-2 py-1 text-xs text-red-700 border border-red-300 rounded hover:bg-red-50"
                            >
                              Remove
                            </button>
                          </div>
                        </div>
                      ))}
                    </div>

                    <div className="lg:col-span-2 space-y-3">
                      {selectedScript ? (
                        <>
                          <input
                            type="text"
                            value={selectedScript.name}
                            onChange={(e) => updateSelectedScript({ name: e.target.value })}
                            placeholder="Script name"
                            className="w-full px-4 py-2 border border-gray-300 rounded-lg text-gray-900"
                          />
                          <div className="flex flex-wrap items-center gap-4 text-sm">
                            <label className="flex items-center space-x-2">
                              <input
                                type="checkbox"
                                checked={selectedScript.enabled}
                                onChange={(e) => updateSelectedScript({ enabled: e.target.checked })}
                                className="w-4 h-4 text-blue-600 rounded"
                              />
                              <span className="text-gray-900">Enabled</span>
                            </label>
                            <label className="flex items-center space-x-2">
                              <input
                                type="checkbox"
                                checked={selectedScript.continueOnError}
                                onChange={(e) => updateSelectedScript({ continueOnError: e.target.checked })}
                                className="w-4 h-4 text-blue-600 rounded"
                              />
                              <span className="text-gray-900">Continue on error</span>
                            </label>
                          </div>

                          <div className="rounded-lg border border-gray-700 overflow-hidden">
                            <div className="px-4 py-2 bg-[#252526] border-b border-gray-700 flex items-center justify-between">
                              <span className="text-xs tracking-wide uppercase text-gray-200">
                                PowerShell ISE
                              </span>
                              <span className="text-xs text-gray-400">
                                {selectedScript.name || `Script-${selectedScriptIndex + 1}`}.ps1
                              </span>
                            </div>
                            <div className="grid grid-cols-[3rem_1fr] min-h-[22rem]">
                              <div className="bg-[#2d2d30] text-gray-500 text-xs leading-6 py-3 px-2 text-right select-none">
                                {Array.from({ length: selectedScriptLineCount }, (_, line) => (
                                  <div key={line}>{line + 1}</div>
                                ))}
                              </div>
                              <textarea
                                value={selectedScript.content}
                                onChange={(e) => updateSelectedScript({ content: e.target.value })}
                                spellCheck={false}
                                className="w-full min-h-[22rem] bg-[#1e1e1e] text-[#d4d4d4] font-mono text-sm leading-6 px-3 py-3 outline-none resize-y"
                              />
                            </div>
                          </div>
                        </>
                      ) : (
                        <div className="text-sm text-gray-500 bg-white border border-gray-200 rounded-lg p-4">
                          Select a script to edit.
                        </div>
                      )}
                    </div>
                  </div>
                )}

                {enabledScriptsWithIssues.length > 0 && (
                  <div className="text-sm text-yellow-800 bg-yellow-50 border border-yellow-200 rounded-lg px-3 py-2">
                    Enabled scripts must have both a name and script content.
                  </div>
                )}
              </>
            ) : (
              <div className="text-sm text-gray-500 bg-white border border-gray-200 rounded-lg p-4">
                Enable custom scripts to add and edit post-install PowerShell scripts.
              </div>
            )}
          </div>
        </div>
      )}

      {activeTab === 'desktop-customisation' && (
        <ShellLayoutCanvas
          items={shellLayoutSourceItems}
          value={state.shellLayout}
          isWindows11={isWindows11}
          onChange={(shellLayout) => dispatch({ type: 'UPDATE_SHELL_LAYOUT', payload: shellLayout })}
        />
      )}

      {/* Options */}
      <div className="pt-4 border-t border-gray-200">
        <label className="flex items-center space-x-3">
          <input
            type="checkbox"
            checked={apps.continueOnError}
            onChange={(e) =>
              dispatch({ type: 'UPDATE_APPS', payload: { continueOnError: e.target.checked } })
            }
            className="w-5 h-5 text-blue-600 rounded"
          />
          <span className="text-gray-900">Continue if an application fails to install</span>
        </label>
      </div>
    </div>
  );
}
