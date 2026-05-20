import { useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/tauri';
import { useWizard } from './WizardContext';
import { normalizeLocaleTag, validateLocaleTag } from './localeValidation';

interface OsVersionEntry {
  id: string;
  display_name: string;
  operating_system: string;
  release_id: string;
  build: string;
  architecture: string;
  language_code: string;
  license: string;
  size_bytes: number | null;
  download_url: string;
}

const EDITIONS = ['Home', 'Pro', 'Enterprise', 'Education'];
const CUSTOM_LANGUAGE_OPTION = '__custom__';

/** Map edition to expected license channel */
function editionToChannel(edition: string): string {
  const lower = edition.toLowerCase();
  if (lower === 'home' || lower === 'pro') return 'Retail';
  if (lower === 'enterprise' || lower === 'education') return 'Volume';
  return 'Retail';
}

function formatLanguageLabel(locale: string): string {
  return normalizeLocaleTag(locale) || locale;
}

function parseReleaseId(releaseId: string): { year: number; half: number } {
  const match = /^(\d+)\s*H(\d+)$/i.exec(releaseId.trim());
  if (!match) {
    return { year: Number.NEGATIVE_INFINITY, half: Number.NEGATIVE_INFINITY };
  }

  return {
    year: Number.parseInt(match[1], 10),
    half: Number.parseInt(match[2], 10),
  };
}

function parseBuildNumber(build: string): number {
  const digits = build.replace(/\D/g, '');
  return Number.parseInt(digits, 10) || 0;
}

function compareVersionsNewestFirst(left: OsVersionEntry, right: OsVersionEntry): number {
  const leftRelease = parseReleaseId(left.release_id);
  const rightRelease = parseReleaseId(right.release_id);

  if (leftRelease.year !== rightRelease.year) {
    return rightRelease.year - leftRelease.year;
  }

  if (leftRelease.half !== rightRelease.half) {
    return rightRelease.half - leftRelease.half;
  }

  return parseBuildNumber(right.build) - parseBuildNumber(left.build);
}

function getLatestSelectableVersion(
  versions: OsVersionEntry[],
  language: string,
  preferredOs: string,
): OsVersionEntry | null {
  if (versions.length === 0) {
    return null;
  }

  const normalizedLanguage = normalizeLocaleTag(language)?.toLowerCase();
  const languageMatches = normalizedLanguage
    ? versions.filter((version) => normalizeLocaleTag(version.language_code)?.toLowerCase() === normalizedLanguage)
    : versions;
  const preferredOsMatches = preferredOs
    ? languageMatches.filter((version) => version.operating_system === preferredOs)
    : [];

  return preferredOsMatches[0] || languageMatches[0] || versions[0];
}

export function StepWindowsSource() {
  const { state, dispatch } = useWizard();
  const { windowsVersion } = state;

  const initialLanguage = (windowsVersion.language || 'en-us').trim().toLowerCase() || 'en-us';

  const [osVersions, setOsVersions] = useState<OsVersionEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [syncing, setSyncing] = useState(false);
  const [lastSync, setLastSync] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [usingFallback, setUsingFallback] = useState(false);

  const [selectedLanguage, setSelectedLanguage] = useState(initialLanguage);
  const [customLanguage, setCustomLanguage] = useState(initialLanguage);
  const [useCustomLanguage, setUseCustomLanguage] = useState(false);
  const [selectedArch, setSelectedArch] = useState('amd64');

  const getBaseName = (path: string) => path.split(/[\\/]/).pop() || path;

  const discoveredLanguages = useMemo(() => {
    const unique = new Set<string>();
    osVersions.forEach((version) => {
      const canonical = normalizeLocaleTag(version.language_code);
      if (canonical) {
        unique.add(canonical.toLowerCase());
      }
    });
    return Array.from(unique).sort();
  }, [osVersions]);

  const effectiveLanguage = useCustomLanguage ? customLanguage : selectedLanguage;
  const normalizedEffectiveLanguage = normalizeLocaleTag(effectiveLanguage);
  const customLanguageError = useCustomLanguage ? validateLocaleTag(customLanguage) : null;

  const filteredVersions = useMemo(() => {
    if (!normalizedEffectiveLanguage) {
      return [];
    }

    return osVersions.filter((version) => {
      const versionLocale = normalizeLocaleTag(version.language_code);
      const languageMatch = versionLocale?.toLowerCase() === normalizedEffectiveLanguage.toLowerCase();
      return languageMatch;
    });
  }, [osVersions, normalizedEffectiveLanguage]);

  /** Filter versions by both language and license channel */
  const channelFilteredVersions = useMemo(() => {
    const expectedChannel = editionToChannel(windowsVersion.edition || 'Pro');
    return filteredVersions.filter((v) => {
      // Match the license channel: Retail editions for Home/Pro, Volume for Enterprise/Education
      return v.license === expectedChannel;
    });
  }, [filteredVersions, windowsVersion.edition]);

  useEffect(() => {
    void loadOsVersions();
  }, [selectedArch]);

  useEffect(() => {
    checkLastSync();
  }, []);

  useEffect(() => {
    if (useCustomLanguage || discoveredLanguages.length === 0) {
      return;
    }

    if (!discoveredLanguages.includes(selectedLanguage)) {
      const fallbackLanguage = discoveredLanguages.includes('en-us')
        ? 'en-us'
        : discoveredLanguages[0];
      setSelectedLanguage(fallbackLanguage);
    }
  }, [discoveredLanguages, selectedLanguage, useCustomLanguage]);

  useEffect(() => {
    dispatch({
      type: 'UPDATE_WINDOWS_VERSION',
      payload: { language: effectiveLanguage },
    });
  }, [dispatch, effectiveLanguage]);

  useEffect(() => {
    if (loading || windowsVersion.sourceType !== 'cloud') {
      return;
    }

    if (channelFilteredVersions.length === 0) {
      if (windowsVersion.osVersionId || windowsVersion.downloadUrl) {
        dispatch({
          type: 'UPDATE_WINDOWS_VERSION',
          payload: {
            osVersionId: undefined,
            downloadUrl: undefined,
          },
        });
      }
      return;
    }

    const selected = channelFilteredVersions.find((entry) => entry.id === windowsVersion.osVersionId);
    if (selected) {
      return;
    }

    const firstVersion = channelFilteredVersions[0];
    dispatch({
      type: 'UPDATE_WINDOWS_VERSION',
      payload: {
        name: firstVersion.operating_system,
        build: firstVersion.release_id,
        edition: windowsVersion.edition || 'Pro',
        language: firstVersion.language_code,
        osVersionId: firstVersion.id,
        downloadUrl: firstVersion.download_url,
        channel: firstVersion.license,
      },
    });
  }, [
    dispatch,
    channelFilteredVersions,
    loading,
    windowsVersion.sourceType,
    windowsVersion.osVersionId,
    windowsVersion.downloadUrl,
    windowsVersion.edition,
  ]);

  const checkLastSync = async () => {
    try {
      const lastSyncTime = await invoke<string | null>('get_last_catalog_sync');
      setLastSync(lastSyncTime);
    } catch (e) {
      console.error('Failed to get last sync time:', e);
    }
  };

  const loadOsVersions = async (): Promise<OsVersionEntry[]> => {
    try {
      setLoading(true);
      setError(null);

      const versions = await invoke<OsVersionEntry[]>('get_os_versions', {
        arch: selectedArch,
      });
      const sortedVersions = [...versions].sort(compareVersionsNewestFirst);

      setOsVersions(sortedVersions);
      setUsingFallback(sortedVersions.length === 0);
      return sortedVersions;
    } catch (err) {
      console.error('Failed to load OS versions:', err);
      setError('Failed to load OS catalog. Please try syncing.');
      setUsingFallback(true);
      setOsVersions([]);
      return [];
    } finally {
      setLoading(false);
    }
  };

  const handleSync = async () => {
    try {
      setSyncing(true);
      setError(null);

      const status = await invoke<{ last_sync_success: boolean; entry_count: number; error_message?: string }>('sync_os_catalog');

      if (status.last_sync_success) {
        const refreshedVersions = await loadOsVersions();
        await checkLastSync();

        if (windowsVersion.sourceType === 'cloud') {
          // Filter by expected channel
          const expectedChannel = editionToChannel(windowsVersion.edition || 'Pro');
          const channelMatched = refreshedVersions.filter((v) => v.license === expectedChannel);
          const latestVersion = getLatestSelectableVersion(
            channelMatched,
            effectiveLanguage,
            windowsVersion.name,
          );

          if (latestVersion) {
            dispatch({
              type: 'UPDATE_WINDOWS_VERSION',
              payload: {
                name: latestVersion.operating_system,
                build: latestVersion.release_id,
                edition: windowsVersion.edition || 'Pro',
                language: latestVersion.language_code,
                osVersionId: latestVersion.id,
                downloadUrl: latestVersion.download_url,
                channel: latestVersion.license,
              },
            });
          }
        }
      } else {
        setError(status.error_message || 'Sync failed. Using cached data.');
      }
    } catch (err) {
      console.error('Failed to sync catalog:', err);
      setError('Failed to sync catalog. Please check your internet connection.');
    } finally {
      setSyncing(false);
    }
  };

  const uniqueOsVersions = Array.from(
    new Map(channelFilteredVersions.map((v) => [v.operating_system, v])).values(),
  );

  const availableBuilds = Array.from(
    new Set(
      channelFilteredVersions
        .filter((v) => v.operating_system === windowsVersion.name)
        .map((v) => v.release_id),
    ),
  );

  const handleOsChange = (osName: string) => {
    const selectedOsVersions = channelFilteredVersions.filter((v) => v.operating_system === osName);
    const firstVersion = selectedOsVersions[0];

    if (!firstVersion) {
      return;
    }

    dispatch({
      type: 'UPDATE_WINDOWS_VERSION',
      payload: {
        name: osName,
        build: firstVersion.release_id,
        language: firstVersion.language_code || effectiveLanguage,
        osVersionId: firstVersion.id,
        downloadUrl: firstVersion.download_url,
        channel: firstVersion.license,
      },
    });
  };

  const handleBuildChange = (build: string) => {
    const selectedVersion = channelFilteredVersions.find(
      (v) => v.operating_system === windowsVersion.name && v.release_id === build,
    );

    if (!selectedVersion) {
      return;
    }

    dispatch({
      type: 'UPDATE_WINDOWS_VERSION',
      payload: {
        build,
        language: selectedVersion.language_code || effectiveLanguage,
        osVersionId: selectedVersion.id,
        downloadUrl: selectedVersion.download_url,
        channel: selectedVersion.license,
      },
    });
  };

  const formatLastSync = (isoString: string | null) => {
    if (!isoString) return 'Never';
    const date = new Date(isoString);
    const hoursAgo = Math.floor((Date.now() - date.getTime()) / (1000 * 60 * 60));
    if (hoursAgo < 1) return 'Just now';
    if (hoursAgo < 24) return `${hoursAgo} hours ago`;
    return `${Math.floor(hoursAgo / 24)} days ago`;
  };

  const showCloudLanguageWarning =
    windowsVersion.sourceType === 'cloud'
    && !loading
    && !!normalizedEffectiveLanguage
    && filteredVersions.length === 0;

  const expectedChannel = editionToChannel(windowsVersion.edition || 'Pro');
  const showChannelWarning =
    windowsVersion.sourceType === 'cloud'
    && !loading
    && filteredVersions.length > 0
    && channelFilteredVersions.length === 0;

  return (
    <div className="wizard-step space-y-6">
      <div className="flex justify-between items-start">
        <div>
          <h2 className="text-2xl font-bold text-gray-900 mb-2">Windows Source</h2>
          <p className="text-gray-600">
            Select the Windows version, build, and edition for your deployment image.
          </p>
        </div>
        <div className="flex items-center space-x-3">
          <span className="text-sm text-gray-500">
            Last synced: {formatLastSync(lastSync)}
          </span>
          <button
            onClick={handleSync}
            disabled={syncing || loading}
            className={`px-4 py-2 rounded-lg font-medium transition-colors ${
              syncing || loading
                ? 'bg-gray-300 text-gray-500 cursor-not-allowed'
                : 'bg-blue-600 text-white hover:bg-blue-700'
            }`}
          >
            {syncing ? 'Syncing...' : 'Refresh'}
          </button>
        </div>
      </div>

      {error && (
        <div className="bg-red-50 border border-red-200 rounded-lg p-4">
          <p className="text-red-800">{error}</p>
        </div>
      )}

      {usingFallback && (
        <div className="bg-yellow-50 border border-yellow-200 rounded-lg p-4">
          <p className="text-yellow-800">
            Using fallback catalog. Click Refresh to download the latest OS versions.
          </p>
        </div>
      )}

      {/* Filters */}
      <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
        <div>
          <label className="block text-sm font-medium text-gray-700 mb-2">
            Language
          </label>
          <select
            value={useCustomLanguage ? CUSTOM_LANGUAGE_OPTION : selectedLanguage}
            onChange={(e) => {
              const nextValue = e.target.value;
              if (nextValue === CUSTOM_LANGUAGE_OPTION) {
                setUseCustomLanguage(true);
                if (!customLanguage.trim()) {
                  setCustomLanguage(selectedLanguage);
                }
                return;
              }

              setUseCustomLanguage(false);
              setSelectedLanguage(nextValue);
            }}
            className="w-full px-4 py-2 border border-gray-300 rounded-lg focus:ring-2 focus:ring-blue-500 focus:border-blue-500 text-gray-900"
          >
            {discoveredLanguages.length === 0 && <option value="en-us">en-US</option>}
            {discoveredLanguages.map((languageCode) => (
              <option key={languageCode} value={languageCode}>
                {formatLanguageLabel(languageCode)}
              </option>
            ))}
            <option value={CUSTOM_LANGUAGE_OPTION}>Custom locale...</option>
          </select>
          {useCustomLanguage && (
            <div className="mt-3 space-y-2">
              <input
                value={customLanguage}
                onChange={(e) => setCustomLanguage(e.target.value)}
                placeholder="fr-FR"
                className="w-full px-4 py-2 border border-gray-300 rounded-lg focus:ring-2 focus:ring-blue-500 focus:border-blue-500 text-gray-900"
              />
              <p className="text-xs text-gray-500">
                Enter a BCP-47 locale tag, for example en-US, fr-FR, zh-Hant-TW.
              </p>
              {customLanguageError && (
                <p className="text-xs text-red-600">{customLanguageError}</p>
              )}
            </div>
          )}
        </div>

        <div>
          <label className="block text-sm font-medium text-gray-700 mb-2">
            Architecture
          </label>
          <select
            value={selectedArch}
            onChange={(e) => setSelectedArch(e.target.value)}
            className="w-full px-4 py-2 border border-gray-300 rounded-lg focus:ring-2 focus:ring-blue-500 focus:border-blue-500 text-gray-900"
          >
            <option value="amd64">x64 (64-bit)</option>
            <option value="x86">x86 (32-bit)</option>
            <option value="arm64">ARM64</option>
          </select>
        </div>
      </div>

      {showCloudLanguageWarning && (
        <div className="bg-yellow-50 border border-yellow-200 rounded-lg p-4">
          <p className="text-yellow-800">
            No cloud catalog entries were found for language {normalizedEffectiveLanguage}. You can still build from a local source using this locale.
          </p>
        </div>
      )}

      {showChannelWarning && (
        <div className="bg-yellow-50 border border-yellow-200 rounded-lg p-4">
          <p className="text-yellow-800">
            No cloud catalog entries were found for edition <strong>{windowsVersion.edition}</strong> (expected channel: <strong>{expectedChannel}</strong>).
            The catalog may not yet include {expectedChannel === 'Volume' ? 'Volume (Enterprise/Education)' : 'Retail (Consumer/Home/Pro)'} editions for this build.
          </p>
        </div>
      )}

      {loading ? (
        <div className="grid grid-cols-1 md:grid-cols-3 gap-6">
          {['Windows Version', 'Build', 'Edition'].map((label) => (
            <div key={label}>
              <label className="block text-sm font-medium text-gray-700 mb-2">
                {label}
              </label>
              <div className="w-full h-10 px-4 py-2 border border-gray-200 rounded-lg bg-gray-100 animate-pulse"></div>
            </div>
          ))}
        </div>
      ) : (
        <div className="grid grid-cols-1 md:grid-cols-3 gap-6">
          {/* Windows Version */}
          <div>
            <label className="block text-sm font-medium text-gray-700 mb-2">
              Windows Version
            </label>
            <select
              value={windowsVersion.name}
              onChange={(e) => handleOsChange(e.target.value)}
              disabled={uniqueOsVersions.length === 0}
              className="w-full px-4 py-2 border border-gray-300 rounded-lg focus:ring-2 focus:ring-blue-500 focus:border-blue-500 text-gray-900 disabled:bg-gray-100 disabled:text-gray-500"
            >
              {uniqueOsVersions.length === 0 && (
                <option value="">No versions available</option>
              )}
              {uniqueOsVersions.map((version) => (
                <option key={version.operating_system} value={version.operating_system}>
                  {version.operating_system}
                </option>
              ))}
            </select>
          </div>

          {/* Build */}
          <div>
            <label className="block text-sm font-medium text-gray-700 mb-2">
              Build
            </label>
            <select
              value={windowsVersion.build}
              onChange={(e) => handleBuildChange(e.target.value)}
              disabled={availableBuilds.length === 0}
              className="w-full px-4 py-2 border border-gray-300 rounded-lg focus:ring-2 focus:ring-blue-500 focus:border-blue-500 text-gray-900 disabled:bg-gray-100 disabled:text-gray-500"
            >
              {availableBuilds.length === 0 && (
                <option value="">No builds available</option>
              )}
              {availableBuilds.map((build) => (
                <option key={build} value={build}>
                  {build}
                </option>
              ))}
            </select>
          </div>

          {/* Edition */}
          <div>
            <label className="block text-sm font-medium text-gray-700 mb-2">
              Edition
            </label>
            <select
              value={windowsVersion.edition}
              onChange={(e) =>
                dispatch({
                  type: 'UPDATE_WINDOWS_VERSION',
                  payload: { edition: e.target.value },
                })
              }
              className="w-full px-4 py-2 border border-gray-300 rounded-lg focus:ring-2 focus:ring-blue-500 focus:border-blue-500 text-gray-900"
            >
              {EDITIONS.map((edition) => (
                <option key={edition} value={edition}>
                  {edition}
                </option>
              ))}
            </select>
            <p className="text-xs text-gray-500 mt-1">
              {expectedChannel === 'Retail'
                ? 'Retail channel: Consumer/Home/Pro editions'
                : 'Volume channel: Enterprise/Education editions'}
            </p>
          </div>
        </div>
      )}

      {/* Source Type Selection */}
      <div className="bg-gray-50 border border-gray-200 rounded-lg p-4">
        <h3 className="font-semibold text-gray-900 mb-3">Image Source</h3>
        <p className="text-sm text-gray-600 mb-4">
          Choose where to get the Windows installation files from.
        </p>
        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
          {/* Cloud Download Option */}
          <label
            className={`flex items-start p-4 rounded-lg border-2 cursor-pointer transition-all ${
              windowsVersion.sourceType === 'cloud'
                ? 'bg-blue-50 border-blue-500 ring-2 ring-blue-200'
                : 'bg-white border-gray-200 hover:border-blue-300'
            }`}
          >
            <input
              type="radio"
              name="sourceType"
              checked={windowsVersion.sourceType === 'cloud'}
              onChange={() => {
                dispatch({
                  type: 'UPDATE_WINDOWS_VERSION',
                  payload: { sourceType: 'cloud', sourcePath: undefined },
                });
              }}
              className="w-5 h-5 text-blue-600 mt-0.5"
            />
            <div className="ml-3">
              <div className="flex items-center space-x-2">
                <span className="font-semibold text-gray-900">Download from Cloud</span>
                <span className="text-xs bg-blue-100 text-blue-700 px-2 py-0.5 rounded">Recommended</span>
              </div>
              <p className="text-sm text-gray-500 mt-1">
                Download the latest Windows ESD directly from Microsoft's CDN during the build process.
              </p>
              {windowsVersion.sourceType === 'cloud' && windowsVersion.downloadUrl && (
                <p className="text-xs text-green-600 mt-2 font-medium">
                  Ready to download from catalog
                </p>
              )}
              {windowsVersion.sourceType === 'cloud' && !windowsVersion.downloadUrl && (
                <p className="text-xs text-yellow-600 mt-2">
                  Select a Windows version above to get the download URL
                </p>
              )}
            </div>
          </label>

          {/* Local File Option */}
          <label
            className={`flex items-start p-4 rounded-lg border-2 cursor-pointer transition-all ${
              windowsVersion.sourceType === 'local'
                ? 'bg-blue-50 border-blue-500 ring-2 ring-blue-200'
                : 'bg-white border-gray-200 hover:border-blue-300'
            }`}
          >
            <input
              type="radio"
              name="sourceType"
              checked={windowsVersion.sourceType === 'local'}
              onChange={() => {
                dispatch({
                  type: 'UPDATE_WINDOWS_VERSION',
                  payload: { sourceType: 'local' },
                });
              }}
              className="w-5 h-5 text-blue-600 mt-0.5"
            />
            <div className="ml-3">
              <div className="flex items-center space-x-2">
                <span className="font-semibold text-gray-900">Use Local File</span>
              </div>
              <p className="text-sm text-gray-500 mt-1">
                Use an existing Windows ISO, ESD, or WIM file from your computer.
              </p>
              {windowsVersion.sourceType === 'local' && windowsVersion.sourcePath && (
                <p className="text-xs text-green-600 mt-2 font-medium truncate max-w-xs">
                  {getBaseName(windowsVersion.sourcePath)}
                </p>
              )}
            </div>
          </label>
        </div>
      </div>

      {/* Local File Browser - Only shown when local is selected */}
      {windowsVersion.sourceType === 'local' && (
        <div className="bg-white border border-gray-200 rounded-lg p-4">
          <h3 className="font-semibold text-gray-900 mb-2">Select Local File</h3>
          <div className="flex space-x-3">
            <input
              type="text"
              value={windowsVersion.sourcePath || ''}
              readOnly
              placeholder="No file selected..."
              className="flex-1 px-4 py-2 border border-gray-300 rounded-lg bg-gray-50 text-gray-700"
            />
            <button
              onClick={async () => {
                try {
                  const result = await invoke<string | null>('show_open_dialog', {
                    title: 'Select Windows ISO or ESD',
                  });
                  if (result) {
                    dispatch({
                      type: 'UPDATE_WINDOWS_VERSION',
                      payload: { sourcePath: result },
                    });
                  }
                } catch (err) {
                  console.error('Failed to open file dialog:', err);
                }
              }}
              className="px-4 py-2 bg-blue-600 text-white rounded-lg hover:bg-blue-700 font-medium"
            >
              Browse...
            </button>
            {windowsVersion.sourcePath && (
              <button
                onClick={() => {
                  dispatch({
                    type: 'UPDATE_WINDOWS_VERSION',
                    payload: { sourcePath: undefined },
                  });
                }}
                className="px-4 py-2 border border-gray-300 rounded-lg hover:bg-gray-100"
              >
                Clear
              </button>
            )}
          </div>
          <p className="text-xs text-gray-500 mt-2">
            Supported formats: .iso, .esd, .wim
          </p>
        </div>
      )}

      {/* Source Preview */}
      <div className="bg-blue-50 border border-blue-200 rounded-lg p-4">
        <h3 className="font-semibold text-blue-900 mb-2">Selected Configuration</h3>
        <p className="text-blue-800">
          {windowsVersion.name} {windowsVersion.build} {windowsVersion.edition}
        </p>
        {windowsVersion.osVersionId && (
          <>
            <p className="text-sm text-blue-600 mt-2">
              ID: {windowsVersion.osVersionId}
            </p>
            <p className="text-sm text-blue-600">
              Language: {(normalizedEffectiveLanguage || effectiveLanguage).toUpperCase()} | Architecture: {selectedArch.toUpperCase()}
            </p>
          </>
        )}
        <div className="mt-3 pt-3 border-t border-blue-200">
          <p className="text-sm font-medium text-blue-900">Source:</p>
          {windowsVersion.sourceType === 'cloud' ? (
            windowsVersion.downloadUrl ? (
              <p className="text-sm text-green-600 font-medium">Will download from Microsoft CDN</p>
            ) : (
              <p className="text-sm text-yellow-600">No download URL available - select a version above</p>
            )
          ) : windowsVersion.sourcePath ? (
            <p className="text-sm text-green-600 font-medium truncate">{windowsVersion.sourcePath}</p>
          ) : (
            <p className="text-sm text-yellow-600">No local file selected</p>
          )}
        </div>
      </div>
    </div>
  );
}
