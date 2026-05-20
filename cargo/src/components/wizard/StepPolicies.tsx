import { invoke } from '@tauri-apps/api/tauri';
import {
  AlertTriangle,
  ChevronDown,
  ChevronRight,
  RefreshCcw,
  Save,
  Search,
  Trash2,
} from 'lucide-react';
import { useEffect, useState } from 'react';
import { useWizard } from './WizardContext';
import {
  createEmptyCustomRegistryEntry,
  filterPolicies,
  getPolicySelectionDiagnostics,
  mergePolicyEntries,
} from './policyHelpers';
import {
  CustomRegistryEntry,
  GroupPolicyState,
  PolicyCatalogEntry,
  PolicyCategory,
  PolicyImpact,
  PolicyPreset,
  PolicyRegistryValueType,
} from './policyTypes';

const POLICY_TABS: Array<{ key: PolicyCategory; label: string }> = [
  { key: 'security', label: 'Security' },
  { key: 'privacy', label: 'Privacy' },
  { key: 'performance', label: 'Performance' },
  { key: 'updates', label: 'Updates' },
  { key: 'network', label: 'Network' },
  { key: 'custom', label: 'Custom' },
];

const ROW_HEIGHT = 132;
const LIST_HEIGHT = 420;
const LIST_OVERSCAN = 4;
const REGISTRY_VALUE_TYPES: PolicyRegistryValueType[] = [
  'String',
  'DWord',
  'QWord',
  'ExpandString',
  'MultiString',
  'Binary',
];

function getImpactBadgeClass(impact: PolicyImpact): string {
  switch (impact) {
    case 'high':
      return 'border-red-200 bg-red-50 text-red-700';
    case 'low':
      return 'border-emerald-200 bg-emerald-50 text-emerald-700';
    default:
      return 'border-amber-200 bg-amber-50 text-amber-700';
  }
}

function getSupportBadgeClass(supported: boolean): string {
  return supported
    ? 'border-emerald-200 bg-emerald-50 text-emerald-700'
    : 'border-red-200 bg-red-50 text-red-700';
}

function sortPolicyEntries(entries: PolicyCatalogEntry[], selectedPolicyIds: string[]): PolicyCatalogEntry[] {
  const selected = new Set(selectedPolicyIds);
  return [...entries].sort((left, right) => {
    const leftScore = Number(selected.has(left.id));
    const rightScore = Number(selected.has(right.id));
    return rightScore - leftScore
      || Number(right.support.supported) - Number(left.support.supported)
      || left.displayName.localeCompare(right.displayName);
  });
}

function updateGroupPolicies(
  current: GroupPolicyState,
  dispatch: ReturnType<typeof useWizard>['dispatch'],
  payload: Partial<GroupPolicyState>,
) {
  dispatch({
    type: 'UPDATE_GROUP_POLICIES',
    payload: {
      ...current,
      ...payload,
    },
  });
}

export function StepPolicies() {
  const {
    state,
    dispatch,
    policyEditorBootstrap,
    policyEditorLoading,
    policyEditorError,
    reloadPolicyEditorBootstrap,
  } = useWizard();
  const [isExpanded, setIsExpanded] = useState(false);
  const [activeTab, setActiveTab] = useState<PolicyCategory>('security');
  const [search, setSearch] = useState('');
  const [scrollTop, setScrollTop] = useState(0);

  const diagnostics = getPolicySelectionDiagnostics(state.groupPolicies, policyEditorBootstrap);
  const selectedCount = state.groupPolicies.selectedPolicyIds.length + state.groupPolicies.customRegistryEntries.length;
  const blockedSelectionCount =
    diagnostics.unsupportedEntries.length
    + diagnostics.readOnlySelectedEntries.length
    + diagnostics.missingPolicyIds.length;
  const searchActive = search.trim().length > 0;
  const catalog = policyEditorBootstrap?.catalog ?? [];
  const starterPolicies = policyEditorBootstrap?.starterPolicies ?? [];
  const selectedEntriesForTab = diagnostics.selectedEntries.filter((entry) => entry.category === activeTab);
  const starterEntriesForTab = starterPolicies.filter((entry) => entry.category === activeTab);
  const visiblePolicies = searchActive
    ? sortPolicyEntries(filterPolicies(catalog, search), state.groupPolicies.selectedPolicyIds)
    : activeTab === 'custom'
    ? []
    : sortPolicyEntries(
        mergePolicyEntries(starterEntriesForTab, selectedEntriesForTab),
        state.groupPolicies.selectedPolicyIds,
      );

  const totalHeight = visiblePolicies.length * ROW_HEIGHT;
  const startIndex = Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - LIST_OVERSCAN);
  const visibleCount = Math.ceil(LIST_HEIGHT / ROW_HEIGHT) + LIST_OVERSCAN * 2;
  const endIndex = Math.min(visiblePolicies.length, startIndex + visibleCount);
  const renderedPolicies = visiblePolicies.slice(startIndex, endIndex);
  const hostSummary = policyEditorBootstrap?.host.summary || 'Host inspection unavailable.';

  useEffect(() => {
    setScrollTop(0);
  }, [activeTab, search]);

  const applySelection = (nextState: Partial<GroupPolicyState>) => {
    updateGroupPolicies(state.groupPolicies, dispatch, nextState);
  };

  const togglePolicy = (entry: PolicyCatalogEntry) => {
    const selectedIds = new Set(state.groupPolicies.selectedPolicyIds);
    if (selectedIds.has(entry.id)) {
      selectedIds.delete(entry.id);
    } else {
      selectedIds.add(entry.id);
    }

    applySelection({
      selectedPolicyIds: Array.from(selectedIds),
    });
  };

  const handleApplyPreset = (preset: PolicyPreset) => {
    if (selectedCount > 0) {
      const confirmed = window.confirm(
        `Apply preset "${preset.name}" and replace the current policy selection?`,
      );
      if (!confirmed) {
        return;
      }
    }

    const skipped: string[] = [];
    const selectedPolicyIds = preset.selectedPolicyIds.filter((policyId) => {
      const entry = catalog.find((candidate) => candidate.id === policyId);
      if (!entry) {
        skipped.push(policyId);
        return false;
      }
      if (!entry.support.supported || !entry.selectable) {
        skipped.push(entry.displayName);
        return false;
      }
      return true;
    });

    applySelection({
      selectedPolicyIds,
      customRegistryEntries: preset.customRegistryEntries,
      lastAppliedPresetId: preset.id,
      lastAppliedPresetName: preset.name,
    });

    if (skipped.length > 0) {
      window.alert(
        `Preset "${preset.name}" was applied, but some items were skipped on this host:\n\n${skipped.join('\n')}`,
      );
    }
  };

  const handleSavePreset = async () => {
    if (selectedCount === 0) {
      window.alert('Select at least one policy or custom registry entry before saving a preset.');
      return;
    }

    const suggestedName = state.groupPolicies.lastAppliedPresetName || '';
    const name = window.prompt('Preset name', suggestedName);
    if (!name?.trim()) {
      return;
    }

    try {
      const preset = await invoke<PolicyPreset>('save_policy_preset', {
        name: name.trim(),
        selection: state.groupPolicies,
      });
      applySelection({
        lastAppliedPresetId: preset.id,
        lastAppliedPresetName: preset.name,
      });
      await reloadPolicyEditorBootstrap(true);
    } catch (error) {
      console.error('Failed to save policy preset:', error);
      window.alert(`Failed to save preset: ${String(error)}`);
    }
  };

  const handleDeletePreset = async (preset: PolicyPreset) => {
    const confirmed = window.confirm(`Delete preset "${preset.name}"?`);
    if (!confirmed) {
      return;
    }

    try {
      await invoke('delete_policy_preset', { presetId: preset.id });
      if (state.groupPolicies.lastAppliedPresetId === preset.id) {
        applySelection({
          lastAppliedPresetId: null,
          lastAppliedPresetName: null,
        });
      }
      await reloadPolicyEditorBootstrap(true);
    } catch (error) {
      console.error('Failed to delete policy preset:', error);
      window.alert(`Failed to delete preset: ${String(error)}`);
    }
  };

  const updateCustomEntry = (entryId: string, payload: Partial<CustomRegistryEntry>) => {
    applySelection({
      customRegistryEntries: state.groupPolicies.customRegistryEntries.map((entry) =>
        entry.id === entryId ? { ...entry, ...payload } : entry,
      ),
    });
  };

  const addCustomEntry = () => {
    applySelection({
      customRegistryEntries: [
        ...state.groupPolicies.customRegistryEntries,
        createEmptyCustomRegistryEntry(),
      ],
    });
  };

  const removeCustomEntry = (entryId: string) => {
    applySelection({
      customRegistryEntries: state.groupPolicies.customRegistryEntries.filter((entry) => entry.id !== entryId),
    });
  };

  const builtInPresets = policyEditorBootstrap?.builtInPresets ?? [];
  const savedPresets = policyEditorBootstrap?.savedPresets ?? [];

  return (
    <div className="wizard-step space-y-6">
      <div className="space-y-3">
        <div>
          <h2 className="mb-2 text-2xl font-bold text-gray-900">Local Policy Baseline</h2>
          <p className="text-gray-600">
            Review supported machine policies on this build host, then stamp the selected registry-backed settings into the final deployed image.
          </p>
        </div>

        <button
          type="button"
          onClick={() => setIsExpanded((current) => !current)}
          className="flex w-full items-start justify-between rounded-xl border border-[var(--wiz-border)] bg-[var(--wiz-surface-muted)] px-4 py-4 text-left transition-colors hover:bg-white"
        >
          <div className="space-y-3">
            <div className="flex items-center gap-2 text-sm font-semibold text-gray-900">
              {isExpanded ? <ChevronDown size={18} /> : <ChevronRight size={18} />}
              Policy Editor
            </div>
            <div className="flex flex-wrap gap-2 text-xs">
              <span className="rounded-full border border-gray-200 bg-white px-2.5 py-1 font-medium text-gray-700">
                {selectedCount} selected
              </span>
              <span
                className={`rounded-full border px-2.5 py-1 font-medium ${
                  blockedSelectionCount > 0
                    ? 'border-red-200 bg-red-50 text-red-700'
                    : 'border-emerald-200 bg-emerald-50 text-emerald-700'
                }`}
              >
                {blockedSelectionCount > 0 ? `${blockedSelectionCount} need attention` : 'Host-compatible'}
              </span>
              {state.groupPolicies.lastAppliedPresetName && (
                <span className="rounded-full border border-blue-200 bg-blue-50 px-2.5 py-1 font-medium text-blue-700">
                  Preset: {state.groupPolicies.lastAppliedPresetName}
                </span>
              )}
            </div>
            <p className="text-sm text-gray-500">{hostSummary}</p>
          </div>
          <span className="rounded-full border border-gray-200 bg-white px-3 py-1 text-xs font-semibold uppercase tracking-wide text-gray-500">
            {isExpanded ? 'Collapse' : 'Expand'}
          </span>
        </button>
      </div>

      {!isExpanded && (
        <div className="rounded-xl border border-dashed border-gray-300 bg-white px-4 py-5 text-sm text-gray-500">
          Expand the editor to review supported policies, search the local ADMX catalog, apply presets, or add HKLM registry entries.
        </div>
      )}

      {isExpanded && (
        <div className="space-y-5">
          <div className="rounded-xl border border-[var(--wiz-border)] bg-white p-4">
            <div className="flex flex-wrap items-center justify-between gap-3">
              <div>
                <h3 className="text-sm font-semibold text-gray-900">Presets</h3>
                <p className="text-sm text-gray-500">Replace the current selection with a curated baseline or save the current one.</p>
              </div>
              <div className="flex flex-wrap gap-2">
                <button
                  type="button"
                  onClick={() => void reloadPolicyEditorBootstrap(true)}
                  className="inline-flex items-center gap-2 rounded-lg border border-gray-300 px-3 py-2 text-sm font-medium text-gray-700 hover:bg-gray-50"
                >
                  <RefreshCcw size={15} />
                  Refresh host scan
                </button>
                <button
                  type="button"
                  onClick={() => void handleSavePreset()}
                  className="inline-flex items-center gap-2 rounded-lg border border-gray-300 px-3 py-2 text-sm font-medium text-gray-700 hover:bg-gray-50"
                >
                  <Save size={15} />
                  Save preset
                </button>
              </div>
            </div>

            <div className="mt-4 space-y-3">
              <div className="flex flex-wrap gap-2">
                {builtInPresets.map((preset) => (
                  <button
                    key={preset.id}
                    type="button"
                    onClick={() => handleApplyPreset(preset)}
                    disabled={!policyEditorBootstrap?.available}
                    className="rounded-lg border border-gray-300 px-3 py-2 text-sm font-medium text-gray-700 hover:bg-gray-50 disabled:cursor-not-allowed disabled:opacity-50"
                  >
                    {preset.name}
                  </button>
                ))}
              </div>

              {savedPresets.length > 0 && (
                <div className="space-y-2">
                  <p className="text-xs font-semibold uppercase tracking-wide text-gray-500">Saved Presets</p>
                  <div className="flex flex-wrap gap-2">
                    {savedPresets.map((preset) => (
                      <div key={preset.id} className="flex items-center gap-1 rounded-lg border border-gray-300 bg-gray-50 p-1">
                        <button
                          type="button"
                          onClick={() => handleApplyPreset(preset)}
                          disabled={!policyEditorBootstrap?.available}
                          className="rounded-md px-3 py-1.5 text-sm font-medium text-gray-700 hover:bg-white disabled:cursor-not-allowed disabled:opacity-50"
                        >
                          {preset.name}
                        </button>
                        <button
                          type="button"
                          onClick={() => void handleDeletePreset(preset)}
                          className="rounded-md p-1.5 text-gray-500 hover:bg-white hover:text-red-600"
                          aria-label={`Delete ${preset.name}`}
                        >
                          <Trash2 size={15} />
                        </button>
                      </div>
                    ))}
                  </div>
                </div>
              )}
            </div>
          </div>

          {(policyEditorError || !policyEditorBootstrap?.available) && (
            <div className="rounded-xl border border-red-200 bg-red-50 p-4 text-sm text-red-700">
              <div className="flex items-start gap-2">
                <AlertTriangle size={16} className="mt-[2px] shrink-0" />
                <div>
                  <p className="font-semibold">Policy inspection is unavailable on this host.</p>
                  <p>{policyEditorError || policyEditorBootstrap?.unavailableReason || 'The local PolicyDefinitions catalog could not be inspected.'}</p>
                </div>
              </div>
            </div>
          )}

          {(diagnostics.missingPolicyIds.length > 0 || diagnostics.invalidCustomEntries.length > 0) && (
            <div className="rounded-xl border border-amber-200 bg-amber-50 p-4 text-sm text-amber-800">
              {diagnostics.missingPolicyIds.length > 0 && (
                <p>Saved selections could not be resolved on this host: {diagnostics.missingPolicyIds.join(', ')}</p>
              )}
              {diagnostics.invalidCustomEntries.map((message) => (
                <p key={message}>{message}</p>
              ))}
            </div>
          )}

          <div className="rounded-xl border border-[var(--wiz-border)] bg-white p-4">
            <div className="flex flex-col gap-3 lg:flex-row lg:items-center lg:justify-between">
              <div className="flex flex-wrap gap-2">
                {POLICY_TABS.map((tab) => (
                  <button
                    key={tab.key}
                    type="button"
                    onClick={() => setActiveTab(tab.key)}
                    className={`rounded-lg border px-3 py-2 text-sm font-medium ${
                      activeTab === tab.key
                        ? 'border-slate-900 bg-slate-900 text-white'
                        : 'border-gray-300 bg-white text-gray-700 hover:bg-gray-50'
                    }`}
                  >
                    {tab.label}
                  </button>
                ))}
              </div>

              <label className="flex items-center gap-2 rounded-lg border border-gray-300 bg-white px-3 py-2 text-sm text-gray-600 lg:min-w-[320px]">
                <Search size={16} />
                <input
                  value={search}
                  onChange={(event) => setSearch(event.target.value)}
                  placeholder="Search supported policies, categories, aliases..."
                  className="w-full bg-transparent text-sm text-gray-900 outline-none"
                />
              </label>
            </div>

            {(searchActive || activeTab !== 'custom') && (
              <div className="mt-4 space-y-3">
                <div className="flex items-center justify-between text-sm text-gray-500">
                  <span>
                    {searchActive
                      ? `${visiblePolicies.length} result${visiblePolicies.length === 1 ? '' : 's'} from the local PolicyDefinitions catalog`
                      : `Starter policies for ${POLICY_TABS.find((tab) => tab.key === activeTab)?.label}`}
                  </span>
                  {policyEditorLoading && <span>Refreshing policy catalog...</span>}
                </div>

                {visiblePolicies.length === 0 ? (
                  <div className="rounded-lg border border-dashed border-gray-300 bg-gray-50 p-6 text-sm text-gray-500">
                    {searchActive
                      ? 'No matching policies were found in the local ADMX catalog.'
                      : 'No starter policies are defined for this category on the current host.'}
                  </div>
                ) : (
                  <div
                    onScroll={(event) => setScrollTop(event.currentTarget.scrollTop)}
                    className="overflow-y-auto rounded-xl border border-gray-200 bg-gray-50"
                    style={{ height: LIST_HEIGHT }}
                  >
                    <div style={{ height: totalHeight, position: 'relative' }}>
                      {renderedPolicies.map((entry, index) => {
                        const checked = state.groupPolicies.selectedPolicyIds.includes(entry.id);
                        const canEnable = entry.support.supported && entry.selectable;
                        const top = (startIndex + index) * ROW_HEIGHT;

                        return (
                          <div
                            key={entry.id}
                            style={{ position: 'absolute', top, left: 0, right: 0, height: ROW_HEIGHT }}
                            className="px-3 py-2"
                          >
                            <label
                              className={`flex h-full cursor-pointer gap-3 rounded-xl border p-4 ${
                                checked
                                  ? 'border-slate-900 bg-white shadow-sm'
                                  : 'border-gray-200 bg-white hover:border-gray-300'
                              } ${!canEnable && !checked ? 'cursor-not-allowed opacity-80' : ''}`}
                            >
                              <input
                                type="checkbox"
                                checked={checked}
                                disabled={!canEnable && !checked}
                                onChange={() => togglePolicy(entry)}
                                className="mt-1 h-4 w-4 rounded border-gray-300 text-slate-900"
                              />
                              <div className="min-w-0 flex-1 space-y-2">
                                <div className="flex flex-wrap items-start justify-between gap-2">
                                  <div className="min-w-0">
                                    <p className="font-semibold text-gray-900">{entry.displayName}</p>
                                    <p className="mt-1 overflow-hidden text-sm text-gray-500" style={{ maxHeight: 42 }}>
                                      {entry.description}
                                    </p>
                                  </div>
                                  <div className="flex flex-wrap gap-2">
                                    {searchActive && (
                                      <span className="rounded-full border border-gray-200 bg-gray-50 px-2.5 py-1 text-xs font-medium text-gray-600">
                                        {POLICY_TABS.find((tab) => tab.key === entry.category)?.label || entry.categoryLabel}
                                      </span>
                                    )}
                                    <span className={`rounded-full border px-2.5 py-1 text-xs font-semibold ${getImpactBadgeClass(entry.impact)}`}>
                                      Impact {entry.impact}
                                    </span>
                                    <span className={`rounded-full border px-2.5 py-1 text-xs font-semibold ${getSupportBadgeClass(entry.support.supported)}`}>
                                      {entry.support.supported ? 'Supported' : 'Unsupported'}
                                    </span>
                                    {!entry.selectable && (
                                      <span className="rounded-full border border-gray-200 bg-gray-50 px-2.5 py-1 text-xs font-semibold text-gray-600">
                                        Read-only
                                      </span>
                                    )}
                                  </div>
                                </div>

                                <div className="flex flex-wrap gap-3 text-xs text-gray-500">
                                  {entry.support.supportedOn && <span>Supported on: {entry.support.supportedOn}</span>}
                                  <span>{entry.categoryLabel}</span>
                                </div>

                                {!entry.support.supported && (
                                  <p className="text-sm font-medium text-red-700">{entry.support.reason}</p>
                                )}
                                {entry.selectable === false && entry.readOnlyReason && (
                                  <p className="text-sm text-gray-600">{entry.readOnlyReason}</p>
                                )}
                              </div>
                            </label>
                          </div>
                        );
                      })}
                    </div>
                  </div>
                )}
              </div>
            )}

            {!searchActive && activeTab === 'custom' && (
              <div className="mt-4 space-y-4">
                <div className="flex items-start justify-between gap-4 rounded-lg border border-gray-200 bg-gray-50 p-4">
                  <div>
                    <h4 className="font-semibold text-gray-900">Custom HKLM Registry Entries</h4>
                    <p className="text-sm text-gray-500">
                      These entries are written only into the final deployed machine. BitOSDT does not change the build host registry.
                    </p>
                  </div>
                  <button
                    type="button"
                    onClick={addCustomEntry}
                    className="rounded-lg border border-gray-300 px-3 py-2 text-sm font-medium text-gray-700 hover:bg-white"
                  >
                    Add entry
                  </button>
                </div>

                {state.groupPolicies.customRegistryEntries.length === 0 ? (
                  <div className="rounded-lg border border-dashed border-gray-300 bg-gray-50 p-6 text-sm text-gray-500">
                    No custom HKLM registry entries added.
                  </div>
                ) : (
                  <div className="space-y-3">
                    {state.groupPolicies.customRegistryEntries.map((entry) => (
                      <div key={entry.id} className="rounded-xl border border-gray-200 bg-white p-4">
                        <div className="mb-4 flex items-center justify-between gap-3">
                          <p className="text-sm font-semibold text-gray-900">
                            {entry.valueName.trim() || 'New registry entry'}
                          </p>
                          <button
                            type="button"
                            onClick={() => removeCustomEntry(entry.id)}
                            className="inline-flex items-center gap-2 rounded-lg border border-gray-300 px-3 py-2 text-sm text-gray-700 hover:bg-gray-50"
                          >
                            <Trash2 size={15} />
                            Remove
                          </button>
                        </div>

                        <div className="grid gap-4 lg:grid-cols-2">
                          <div className="space-y-2">
                            <label className="block text-sm font-medium text-gray-700">HKLM key path</label>
                            <input
                              value={entry.keyPath}
                              onChange={(event) => updateCustomEntry(entry.id, { keyPath: event.target.value })}
                              className="w-full rounded-lg border border-gray-300 px-3 py-2 text-sm text-gray-900"
                              placeholder="HKLM:\SOFTWARE\Policies\..."
                            />
                          </div>

                          <div className="space-y-2">
                            <label className="block text-sm font-medium text-gray-700">Value name</label>
                            <input
                              value={entry.valueName}
                              onChange={(event) => updateCustomEntry(entry.id, { valueName: event.target.value })}
                              className="w-full rounded-lg border border-gray-300 px-3 py-2 text-sm text-gray-900"
                              placeholder="ValueName"
                            />
                          </div>

                          <div className="space-y-2">
                            <label className="block text-sm font-medium text-gray-700">Value type</label>
                            <select
                              value={entry.valueType}
                              onChange={(event) => updateCustomEntry(entry.id, { valueType: event.target.value as PolicyRegistryValueType })}
                              className="w-full rounded-lg border border-gray-300 px-3 py-2 text-sm text-gray-900"
                            >
                              {REGISTRY_VALUE_TYPES.map((valueType) => (
                                <option key={valueType} value={valueType}>
                                  {valueType}
                                </option>
                              ))}
                            </select>
                          </div>

                          <div className="space-y-2 lg:col-span-2">
                            <label className="block text-sm font-medium text-gray-700">Value data</label>
                            <textarea
                              value={entry.valueData}
                              onChange={(event) => updateCustomEntry(entry.id, { valueData: event.target.value })}
                              rows={3}
                              className="w-full rounded-lg border border-gray-300 px-3 py-2 text-sm text-gray-900"
                              placeholder={
                                entry.valueType === 'Binary'
                                  ? 'DE AD BE EF'
                                  : entry.valueType === 'MultiString'
                                  ? 'One value per line or JSON array'
                                  : 'Registry value data'
                              }
                            />
                          </div>
                        </div>
                      </div>
                    ))}
                  </div>
                )}
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
