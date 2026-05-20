import {
  CustomRegistryEntry,
  GroupPolicyState,
  PolicyCatalogEntry,
  PolicyEditorBootstrap,
} from './policyTypes';

export interface PolicySelectionDiagnostics {
  selectedEntries: PolicyCatalogEntry[];
  unsupportedEntries: PolicyCatalogEntry[];
  missingPolicyIds: string[];
  readOnlySelectedEntries: PolicyCatalogEntry[];
  invalidCustomEntries: string[];
}

export function buildPolicyCatalogIndex(entries: PolicyCatalogEntry[]): Map<string, PolicyCatalogEntry> {
  return new Map(entries.map((entry) => [entry.id, entry]));
}

export function getPolicySelectionDiagnostics(
  groupPolicies: GroupPolicyState,
  bootstrap?: PolicyEditorBootstrap | null,
): PolicySelectionDiagnostics {
  if (!bootstrap) {
    return {
      selectedEntries: [],
      unsupportedEntries: [],
      missingPolicyIds: [],
      readOnlySelectedEntries: [],
      invalidCustomEntries: validateCustomRegistryEntries(groupPolicies.customRegistryEntries),
    };
  }

  const index = buildPolicyCatalogIndex(bootstrap?.catalog ?? []);
  const selectedEntries: PolicyCatalogEntry[] = [];
  const unsupportedEntries: PolicyCatalogEntry[] = [];
  const readOnlySelectedEntries: PolicyCatalogEntry[] = [];
  const missingPolicyIds: string[] = [];

  for (const policyId of groupPolicies.selectedPolicyIds) {
    const entry = index.get(policyId);
    if (!entry) {
      missingPolicyIds.push(policyId);
      continue;
    }

    selectedEntries.push(entry);
    if (!entry.support.supported) {
      unsupportedEntries.push(entry);
    }
    if (!entry.selectable) {
      readOnlySelectedEntries.push(entry);
    }
  }

  return {
    selectedEntries,
    unsupportedEntries,
    missingPolicyIds,
    readOnlySelectedEntries,
    invalidCustomEntries: validateCustomRegistryEntries(groupPolicies.customRegistryEntries),
  };
}

export function validateCustomRegistryEntries(entries: CustomRegistryEntry[]): string[] {
  const issues: string[] = [];
  const seenIds = new Set<string>();

  for (const entry of entries) {
    if (!entry.id || seenIds.has(entry.id)) {
      issues.push('Custom registry entries must have unique identifiers.');
      continue;
    }
    seenIds.add(entry.id);

    if (!entry.keyPath.trim()) {
      issues.push('Custom registry entries require an HKLM key path.');
    } else if (!/^((HKLM:\\)|(HKEY_LOCAL_MACHINE\\))/i.test(entry.keyPath.trim())) {
      issues.push(`Custom registry entry "${entry.valueName || entry.id}" must use an HKLM path.`);
    }

    if (!entry.valueName.trim()) {
      issues.push(`Custom registry entry "${entry.id}" requires a value name.`);
    }
  }

  return Array.from(new Set(issues));
}

export function filterPolicies(entries: PolicyCatalogEntry[], search: string): PolicyCatalogEntry[] {
  const normalizedSearch = search.trim().toLowerCase();
  if (!normalizedSearch) {
    return entries;
  }

  return entries.filter((entry) => {
    const haystack = [
      entry.displayName,
      entry.description,
      entry.categoryLabel,
      entry.support.supportedOn ?? '',
      ...entry.aliases,
    ]
      .join(' ')
      .toLowerCase();

    return haystack.includes(normalizedSearch);
  });
}

export function mergePolicyEntries(primary: PolicyCatalogEntry[], extra: PolicyCatalogEntry[]): PolicyCatalogEntry[] {
  const deduped = new Map<string, PolicyCatalogEntry>();
  for (const entry of [...primary, ...extra]) {
    deduped.set(entry.id, entry);
  }
  return Array.from(deduped.values());
}

export function createEmptyCustomRegistryEntry(): CustomRegistryEntry {
  return {
    id: `custom-${Math.random().toString(36).slice(2, 10)}`,
    keyPath: 'HKLM:\\SOFTWARE\\Policies\\',
    valueName: '',
    valueType: 'DWord',
    valueData: '',
  };
}
