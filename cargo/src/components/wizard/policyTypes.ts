export type PolicyCategory = 'security' | 'privacy' | 'performance' | 'updates' | 'network' | 'custom';

export type PolicyImpact = 'low' | 'medium' | 'high';

export type PolicySourceKind = 'admx' | 'curated';

export type PolicyRegistryValueType =
  | 'String'
  | 'DWord'
  | 'QWord'
  | 'ExpandString'
  | 'MultiString'
  | 'Binary';

export interface PolicySupportStatus {
  supported: boolean;
  supportedOn?: string | null;
  reason: string;
}

export interface PolicyCatalogEntry {
  id: string;
  sourceKind: PolicySourceKind;
  category: PolicyCategory;
  displayName: string;
  description: string;
  impact: PolicyImpact;
  starter: boolean;
  selectable: boolean;
  support: PolicySupportStatus;
  readOnlyReason?: string | null;
  aliases: string[];
  categoryLabel: string;
}

export interface CustomRegistryEntry {
  id: string;
  keyPath: string;
  valueName: string;
  valueType: PolicyRegistryValueType;
  valueData: string;
}

export interface GroupPolicyState {
  selectedPolicyIds: string[];
  customRegistryEntries: CustomRegistryEntry[];
  lastAppliedPresetId?: string | null;
  lastAppliedPresetName?: string | null;
}

export interface PolicyPreset {
  id: string;
  name: string;
  builtIn: boolean;
  selectedPolicyIds: string[];
  customRegistryEntries: CustomRegistryEntry[];
}

export interface PolicyHostContext {
  available: boolean;
  summary: string;
  productName: string;
  editionId: string;
  displayVersion: string;
  buildNumber: number;
  installationType: string;
  architecture: string;
  uiLanguage: string;
  policyDefinitionsPath: string;
  isVm: boolean;
  tpmSpecVersion?: string | null;
}

export interface PolicyEditorBootstrap {
  available: boolean;
  unavailableReason?: string | null;
  host: PolicyHostContext;
  starterPolicies: PolicyCatalogEntry[];
  catalog: PolicyCatalogEntry[];
  builtInPresets: PolicyPreset[];
  savedPresets: PolicyPreset[];
}

export const defaultGroupPolicyState: GroupPolicyState = {
  selectedPolicyIds: [],
  customRegistryEntries: [],
  lastAppliedPresetId: null,
  lastAppliedPresetName: null,
};
