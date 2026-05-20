import { getPolicySelectionDiagnostics } from './policyHelpers';
import { PolicyEditorBootstrap } from './policyTypes';
import { WizardState } from './types';
import { validateLocaleTag } from './localeValidation';

export interface WizardReadinessIssue {
  code: string;
  level: 'error' | 'warning';
  message: string;
}

export interface WizardReadiness {
  hasValidSource: boolean;
  hasOutputPath: boolean;
  includesLightweightOutput: boolean;
  usesAdvancedDelivery: boolean;
  normalizedComputerName: string;
  hasExplicitComputerName: boolean;
  computerNameValidationError: string | null;
  hasEnabledCustomScripts: boolean;
  configuredCustomScriptsCount: number;
  hasCustomInstallers: boolean;
  hasLocalPayloadCopyWork: boolean;
  hasBootDrivers: boolean;
  selectedPolicyCount: number;
  customRegistryEntryCount: number;
  unsupportedPolicySelectionsCount: number;
  blockingErrors: WizardReadinessIssue[];
  warnings: WizardReadinessIssue[];
  canStartBuild: boolean;
}

export const LIGHTWEIGHT_COMPUTER_NAME_ERROR =
  'Computer name customization is supported only for Full ISO in this release.';

export const LIGHTWEIGHT_CUSTOM_SCRIPT_ERROR =
  'Custom post-install scripts are supported only for Full ISO in this release.';

export function isValidUncFilePath(value: string): boolean {
  const trimmed = value.trim();
  if (!trimmed.startsWith('\\\\')) {
    return false;
  }

  const segments = trimmed.slice(2).split('\\');
  if (segments.length < 3) {
    return false;
  }

  const [server, share, ...relativePath] = segments;
  if (!server?.trim() || !share?.trim()) {
    return false;
  }

  return relativePath.every((segment) => segment.trim().length > 0);
}

export function isValidUncDirectoryPath(value: string): boolean {
  const trimmed = value.trim();
  if (!trimmed.startsWith('\\\\')) {
    return false;
  }

  const segments = trimmed.slice(2).split('\\');
  if (segments.length < 2) {
    return false;
  }

  const [server, share, ...relativePath] = segments;
  if (!server?.trim() || !share?.trim()) {
    return false;
  }

  return relativePath.every((segment) => segment.trim().length > 0);
}

export function validateComputerName(value: string): string | null {
  const trimmed = value.trim();
  if (!trimmed || trimmed === '*') {
    return null;
  }
  if (trimmed.length > 15) {
    return 'Computer name must be 1-15 characters.';
  }
  if (trimmed.startsWith('-') || trimmed.endsWith('-')) {
    return "Computer name cannot start or end with '-'.";
  }
  if (!/^[A-Za-z0-9-]+$/.test(trimmed)) {
    return "Computer name can only contain letters, numbers, and '-'.";
  }
  return null;
}

function hasLocalAdministratorUser(state: WizardState): boolean {
  return state.userAccounts.some((user) => user.group === 'Administrators');
}

export function evaluateWizardReadiness(
  state: WizardState,
  options?: {
    isBuilding?: boolean;
    policyEditorBootstrap?: PolicyEditorBootstrap | null;
    policyEditorLoading?: boolean;
    policyEditorError?: string | null;
  },
): WizardReadiness {
  const isBuilding = options?.isBuilding === true;
  const policyEditorBootstrap = options?.policyEditorBootstrap;
  const policyEditorLoading = options?.policyEditorLoading === true;
  const policyEditorError = options?.policyEditorError;
  const { output, windowsVersion } = state;

  const hasValidSource = windowsVersion.sourceType === 'cloud'
    ? !!windowsVersion.downloadUrl
    : !!windowsVersion.sourcePath;
  const isWdsPxeOutput = output.outputType === 'WDSPXE';
  const hasOutputPath = isWdsPxeOutput || output.outputPath.trim().length > 0;
  const hasCustomInstallers = state.apps.customInstallers.length > 0;
  const hasLocalPayloadCopyWork =
    state.apps.copiedItems.length > 0
    || state.apps.customInstallers.some((installer) => installer.dependencies.length > 0);
  const includesLightweightOutput =
    output.outputType === 'LightweightISO' || output.outputType === 'Both';
  const usesAdvancedDelivery = output.deliveryMode === 'Advanced';
  const hasBootDrivers = output.driverPaths.length > 0;
  const normalizedComputerName = state.oobeConfig.computerName.trim();
  const computerNameValidationError = validateComputerName(state.oobeConfig.computerName);
  const hasExplicitComputerName =
    normalizedComputerName.length > 0 && normalizedComputerName !== '*';
  const hasEnabledCustomScripts =
    state.apps.enableCustomScripts && state.apps.customScripts.some((script) => script.enabled);
  const configuredCustomScriptsCount = state.apps.customScripts.filter((script) => script.enabled).length;
  const languageValidationError = validateLocaleTag(windowsVersion.language || 'en-us');
  const policyDiagnostics = getPolicySelectionDiagnostics(state.groupPolicies, policyEditorBootstrap);
  const selectedPolicyCount = state.groupPolicies.selectedPolicyIds.length;
  const customRegistryEntryCount = state.groupPolicies.customRegistryEntries.length;
  const hasShellLayoutWork =
    state.shellLayout.enabled
    && state.shellLayout.items.some((item) => item.desktop || item.start || item.taskbar);
  const unsupportedPolicySelectionsCount =
    policyDiagnostics.unsupportedEntries.length
    + policyDiagnostics.missingPolicyIds.length
    + policyDiagnostics.readOnlySelectedEntries.length;

  const blockingErrors: WizardReadinessIssue[] = [];
  const warnings: WizardReadinessIssue[] = [];

  if (!hasValidSource) {
    blockingErrors.push({
      code: 'missing-source',
      level: 'error',
      message: windowsVersion.sourceType === 'cloud'
        ? 'No download URL available. Select a Windows version in Windows Source.'
        : 'No local source file selected. Select an ISO, ESD, or WIM file.',
    });
  }

  if (!hasOutputPath) {
    blockingErrors.push({
      code: 'missing-output-path',
      level: 'error',
      message: 'Output path is required before starting a build.',
    });
  }

  if (computerNameValidationError) {
    blockingErrors.push({
      code: 'invalid-computer-name',
      level: 'error',
      message: computerNameValidationError,
    });
  }

  if (state.oobeConfig.skipUserOobe && !hasLocalAdministratorUser(state)) {
    blockingErrors.push({
      code: 'skip-user-oobe-requires-admin',
      level: 'error',
      message:
        'Skip User OOBE requires at least one local administrator account so deployed Windows still has a usable sign-in path.',
    });
  }

  if (languageValidationError) {
    blockingErrors.push({
      code: 'invalid-language-locale',
      level: 'error',
      message: languageValidationError,
    });
  }

  if (includesLightweightOutput && usesAdvancedDelivery && !output.serverUrl?.trim()) {
    blockingErrors.push({
      code: 'advanced-delivery-server-url',
      level: 'error',
      message: 'Runtime server URL is required in Advanced PXE delivery mode.',
    });
  }

  if (includesLightweightOutput && usesAdvancedDelivery && !output.pxeExportPath?.trim()) {
    blockingErrors.push({
      code: 'advanced-delivery-export-path',
      level: 'error',
      message: 'PXE/WDS export path is required in Advanced PXE delivery mode.',
    });
  }

  for (const driver of output.driverPaths) {
    if (!driver.sourcePath.trim()) {
      blockingErrors.push({
        code: 'driver-path-missing',
        level: 'error',
        message: 'Boot driver entries must include a local folder path.',
      });
      break;
    }

    if (driver.sourceKind !== 'Directory') {
      blockingErrors.push({
        code: 'driver-path-kind',
        level: 'error',
        message: 'Boot drivers must be selected as local folders.',
      });
      break;
    }

    if (driver.sourcePath.trim().startsWith('\\\\')) {
      blockingErrors.push({
        code: 'driver-path-network',
        level: 'error',
        message: 'Boot driver folders must be local paths, not network shares.',
      });
      break;
    }
  }

  if (isWdsPxeOutput && output.bootDriverUncPath?.trim() && !isValidUncDirectoryPath(output.bootDriverUncPath)) {
    blockingErrors.push({
      code: 'boot-driver-unc-path',
      level: 'error',
      message: 'WDS/PXE boot-driver UNC path must be a full UNC folder path like \\\\server\\share\\drivers\\winpe.',
    });
  }

  if (includesLightweightOutput && hasExplicitComputerName) {
    blockingErrors.push({
      code: 'lightweight-computer-name',
      level: 'error',
      message: LIGHTWEIGHT_COMPUTER_NAME_ERROR,
    });
  }

  if (hasShellLayoutWork && output.outputType === 'LightweightISO') {
    blockingErrors.push({
      code: 'lightweight-shell-layout',
      level: 'error',
      message: 'Live shell layout canvas is supported only for Windows 11 Full ISO or WDS/PXE builds in this release.',
    });
  }

  if (hasShellLayoutWork && !(windowsVersion.name || '').toLowerCase().includes('11')) {
    blockingErrors.push({
      code: 'shell-layout-windows-version',
      level: 'error',
      message: 'Live shell layout canvas is currently supported only for Windows 11 builds.',
    });
  }

  if (
    isWdsPxeOutput
    && !hasLocalAdministratorUser(state)
    && (state.domainJoin.enabled || state.autopilot.enabled)
  ) {
    warnings.push({
      code: 'wds-sign-in-external-identity',
      level: 'warning',
      message: 'WDS/PXE first sign-in depends on domain join or Autopilot because no local administrator account is configured.',
    });
  } else if (
    isWdsPxeOutput
    && !hasLocalAdministratorUser(state)
    && !state.domainJoin.enabled
    && !state.autopilot.enabled
  ) {
    warnings.push({
      code: 'wds-sign-in-oobe-only',
      level: 'warning',
      message: 'WDS/PXE first sign-in depends on default Windows OOBE because no local administrator account is configured.',
    });
  }

  if (includesLightweightOutput && hasEnabledCustomScripts) {
    blockingErrors.push({
      code: 'lightweight-custom-scripts',
      level: 'error',
      message: LIGHTWEIGHT_CUSTOM_SCRIPT_ERROR,
    });
  }

  if (output.fullIsoUncPath?.trim() && !isValidUncFilePath(output.fullIsoUncPath)) {
    blockingErrors.push({
      code: 'fulliso-unc-path',
      level: 'error',
      message: isWdsPxeOutput
        ? 'WDS/PXE UNC runtime path must be a full UNC file path like \\\\server\\share\\install.wim.'
        : 'Full ISO UNC fallback path must be a full UNC file path like \\\\server\\share\\install.wim.',
    });
  }

  if (policyDiagnostics.invalidCustomEntries.length > 0) {
    for (const issue of policyDiagnostics.invalidCustomEntries) {
      blockingErrors.push({
        code: `invalid-custom-policy-${issue}`,
        level: 'error',
        message: issue,
      });
    }
  }

  if ((selectedPolicyCount > 0 || customRegistryEntryCount > 0) && !policyEditorLoading) {
    if (policyEditorBootstrap && !policyEditorBootstrap.available) {
      blockingErrors.push({
        code: 'policy-editor-unavailable',
        level: 'error',
        message: policyEditorBootstrap.unavailableReason || 'Policy inspection is unavailable on this host.',
      });
    } else if (policyEditorError) {
      blockingErrors.push({
        code: 'policy-editor-error',
        level: 'error',
        message: policyEditorError,
      });
    }
  }

  for (const entry of policyDiagnostics.unsupportedEntries) {
    blockingErrors.push({
      code: `unsupported-policy-${entry.id}`,
      level: 'error',
      message: `${entry.displayName}: ${entry.support.reason}`,
    });
  }

  for (const entry of policyDiagnostics.readOnlySelectedEntries) {
    blockingErrors.push({
      code: `readonly-policy-${entry.id}`,
      level: 'error',
      message: `${entry.displayName}: ${entry.readOnlyReason || 'This policy is read-only in the current release.'}`,
    });
  }

  for (const policyId of policyDiagnostics.missingPolicyIds) {
    blockingErrors.push({
      code: `missing-policy-${policyId}`,
      level: 'error',
      message: `Previously saved policy "${policyId}" is not available on the current build host. Remove it before building.`,
    });
  }

  if (output.fullIsoHttpUrl?.trim()) {
    const normalized = output.fullIsoHttpUrl.trim().toLowerCase();
    if (!(normalized.startsWith('http://') || normalized.startsWith('https://'))) {
      blockingErrors.push({
        code: 'fulliso-http-url',
        level: 'error',
        message: isWdsPxeOutput
          ? 'WDS/PXE HTTP runtime URL must start with http:// or https://.'
          : 'Full ISO HTTP fallback URL must start with http:// or https://.',
      });
    }
  }

  const hasUncRuntimePath = !!output.fullIsoUncPath?.trim();
  const hasHttpRuntimePath = !!output.fullIsoHttpUrl?.trim();
  const hasUncUsername = !!output.fullIsoUncUsername?.trim();
  const hasUncPassword = !!output.fullIsoUncPassword;
  const promptsDomainCredentialsAtRuntime = !!state.domainJoin.promptForDomainCredentialsAtRuntime;
  const hasDomainName = !!state.domainJoin.domain.trim();
  const hasDomainUsername = !!state.domainJoin.username.trim();
  const hasDomainPassword = !!state.domainJoin.password;

  if (state.domainJoin.enabled && !promptsDomainCredentialsAtRuntime && (!hasDomainName || !hasDomainUsername || !hasDomainPassword)) {
    blockingErrors.push({
      code: 'domain-join-required-fields',
      level: 'error',
      message: 'Domain Join is enabled but domain, username, or password is missing.',
    });
  }

  if (hasUncRuntimePath && (!hasUncUsername || !hasUncPassword) && !output.promptUncCredentialsAtRuntime) {
    blockingErrors.push({
      code: 'unc-runtime-credentials',
      level: 'error',
      message: isWdsPxeOutput
        ? 'WDS/PXE UNC runtime path requires both a username and password.'
        : 'Full ISO UNC fallback path requires both a username and password.',
    });
  }

  if (!hasUncRuntimePath && (hasUncUsername || hasUncPassword)) {
    blockingErrors.push({
      code: 'unc-runtime-credentials-without-path',
      level: 'error',
      message: 'Clear the UNC credentials or configure a UNC runtime path.',
    });
  }

  if (isWdsPxeOutput) {
    if (hasUncRuntimePath === hasHttpRuntimePath) {
      blockingErrors.push({
        code: 'wdspxe-runtime-path',
        level: 'error',
        message: 'WDS/PXE output requires exactly one final runtime path: either UNC or HTTP.',
      });
    }
  }

  if (hasCustomInstallers && includesLightweightOutput) {
    warnings.push({
      code: 'lightweight-custom-installers',
      level: 'warning',
      message: output.outputType === 'Both'
        ? 'Custom installer payloads apply only to Full ISO. Lightweight output skips installer execution.'
        : 'Custom installer payloads are skipped in Lightweight ISO mode.',
    });
  }

  if (hasLocalPayloadCopyWork && includesLightweightOutput) {
    warnings.push({
      code: 'lightweight-local-payloads',
      level: 'warning',
      message: output.outputType === 'Both'
        ? 'Configured files, folders, and installer dependencies apply only to Full ISO. Lightweight output skips them.'
        : 'Configured files, folders, and installer dependencies are skipped in Lightweight ISO mode.',
    });
  }

  return {
    hasValidSource,
    hasOutputPath,
    includesLightweightOutput,
    usesAdvancedDelivery,
    normalizedComputerName,
    hasExplicitComputerName,
    computerNameValidationError,
    hasEnabledCustomScripts,
    configuredCustomScriptsCount,
    hasCustomInstallers,
    hasLocalPayloadCopyWork,
    hasBootDrivers,
    selectedPolicyCount,
    customRegistryEntryCount,
    unsupportedPolicySelectionsCount,
    blockingErrors,
    warnings,
    canStartBuild: !isBuilding && blockingErrors.length === 0,
  };
}
