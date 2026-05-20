// Types for Image Creation Wizard
import { LocalPayloadItem } from '../../types/localPayload';
import { GroupPolicyState, defaultGroupPolicyState } from './policyTypes';

export type SourceType = 'cloud' | 'local';

export interface WindowsVersion {
  name: string;
  build: string;
  edition: string;
  language?: string;
  osVersionId?: string;
  downloadUrl?: string;
  sourcePath?: string;
  sourceType: SourceType;
  /** The license channel type: 'Retail' (Consumer/Home/Pro) or 'Volume' (Enterprise/Education) */
  channel?: 'Retail' | 'Volume' | string;
}

export interface OobeConfig {
  skipMachineOobe: boolean;
  skipUserOobe: boolean;
  hideEula: boolean;
  hideWirelessSetup: boolean;
  hideLocalAccountScreen: boolean;
  hideOnlineAccountScreens: boolean;
  networkLocation: 'Home' | 'Work' | 'Other';
  protectYourPc: 'Recommended' | 'Custom' | 'Off';
  computerName: string;
}

export interface UserAccount {
  username: string;
  password: string;
  displayName?: string;
  group: 'Administrators' | 'Users';
  passwordNeverExpires: boolean;
  requirePasswordChange: boolean;
}

export interface DomainJoinConfig {
  enabled: boolean;
  domain: string;
  username: string;
  password: string;
  ouPath?: string;
  promptForDomainCredentialsAtRuntime?: boolean;
}

export interface AutopilotConfig {
  enabled: boolean;
  tenantId: string;
  deploymentMode: 'UserDriven' | 'SelfDeploying' | 'PreProvisioned';
  skipUserOobe: boolean;
  skipDeviceOobe: boolean;
  allowWhiteglove: boolean;
  groupTag?: string;
}

export interface WingetPackage {
  packageId: string;
  version?: string;
  customArgs?: string;
  enabled: boolean;
}

export interface ChocolateyPackage {
  packageName: string;
  version?: string;
  source?: string;
  customArgs?: string;
  enabled: boolean;
}

export interface CustomInstaller {
  name: string;
  path: string;
  sourceType: 'EmbeddedFile' | 'NetworkDirectory' | 'DirectPathOrUrl';
  sourceFileName?: string;
  dependencies: LocalPayloadItem[];
  dependencyDestination?: string;
  silentArgs: string;
  installerType: 'Msi' | 'Exe' | 'Msix';
  enabled: boolean;
}

export interface PostInstallScript {
  name: string;
  content: string;
  enabled: boolean;
  continueOnError: boolean;
}

export interface AppConfig {
  wingetPackages: WingetPackage[];
  chocolateyPackages: ChocolateyPackage[];
  customInstallers: CustomInstaller[];
  copiedItems: LocalPayloadItem[];
  copyDestination?: string;
  enableCustomScripts: boolean;
  customScripts: PostInstallScript[];
  autoInstallChocolatey: boolean;
  continueOnError: boolean;
}

export interface WindowsUpdateConfig {
  enabled: boolean;
  installSecurityUpdates: boolean;
  installCriticalUpdates: boolean;
  installDriverUpdates: boolean;
  excludePreview: boolean;
  excludeOptional: boolean;
  rebootBehavior: 'AutoReboot' | 'ScheduleReboot' | 'NoReboot';
}

export interface ShellLayoutItem {
  id: string;
  label: string;
  itemType: 'winget' | 'chocolatey' | 'custom' | 'copied' | 'shortcut';
  sourceRef?: string;
  sourcePath?: string;
  shortcutTargetPath?: string;
  shortcutArguments?: string;
  shortcutWorkingDirectory?: string;
  shortcutIconPath?: string;
  desktop: boolean;
  start: boolean;
  taskbar: boolean;
}

export interface ShellLayoutState {
  enabled: boolean;
  items: ShellLayoutItem[];
}

export interface OutputConfig {
  outputType: 'FullISO' | 'LightweightISO' | 'Both' | 'WDSPXE';
  outputPath: string;
  volumeLabel: string;
  deliveryMode: 'Simple' | 'Advanced';
  wdsRuntimeSource?: 'UNC' | 'HTTP';
  serverUrl?: string;
  pxeExportPath?: string;
  driverPaths: LocalPayloadItem[];
  bootDriverUncPath?: string;
  applyDriversToOfflineWindows: boolean;
  includeGui: boolean;
  fullIsoUncPath?: string;
  fullIsoUncUsername?: string;
  fullIsoUncPassword?: string;
  fullIsoHttpUrl?: string;
  promptUncCredentialsAtRuntime?: boolean;
}

export interface WizardState {
  currentStep: number;
  windowsVersion: WindowsVersion;
  oobeConfig: OobeConfig;
  userAccounts: UserAccount[];
  domainJoin: DomainJoinConfig;
  autopilot: AutopilotConfig;
  apps: AppConfig;
  windowsUpdate: WindowsUpdateConfig;
  groupPolicies: GroupPolicyState;
  shellLayout: ShellLayoutState;
  output: OutputConfig;
}

export const defaultWizardState: WizardState = {
  currentStep: 0,
  windowsVersion: {
    name: 'Windows 11',
    build: '23H2',
    edition: 'Pro',
    language: 'en-us',
    sourceType: 'cloud',
  },
  oobeConfig: {
    skipMachineOobe: false,
    skipUserOobe: false,
    hideEula: true,
    hideWirelessSetup: true,
    hideLocalAccountScreen: false,
    hideOnlineAccountScreens: true,
    networkLocation: 'Work',
    protectYourPc: 'Recommended',
    computerName: '',
  },
  userAccounts: [],
  domainJoin: {
    enabled: false,
    domain: '',
    username: '',
    password: '',
    ouPath: '',
    promptForDomainCredentialsAtRuntime: false,
  },
  autopilot: {
    enabled: false,
    tenantId: '',
    deploymentMode: 'UserDriven',
    skipUserOobe: true,
    skipDeviceOobe: true,
    allowWhiteglove: false,
    groupTag: '',
  },
  apps: {
    wingetPackages: [],
    chocolateyPackages: [],
    customInstallers: [],
    copiedItems: [],
    copyDestination: '',
    enableCustomScripts: false,
    customScripts: [],
    autoInstallChocolatey: true,
    continueOnError: true,
  },
  windowsUpdate: {
    enabled: true,
    installSecurityUpdates: true,
    installCriticalUpdates: true,
    installDriverUpdates: false,
    excludePreview: true,
    excludeOptional: true,
    rebootBehavior: 'NoReboot',
  },
  groupPolicies: defaultGroupPolicyState,
  shellLayout: {
    enabled: false,
    items: [],
  },
  output: {
    outputType: 'FullISO',
    outputPath: '',
    volumeLabel: 'BITOSDT',
    deliveryMode: 'Simple',
    wdsRuntimeSource: 'UNC',
    serverUrl: 'http://deploy.local:8080',
    pxeExportPath: '',
    driverPaths: [],
    bootDriverUncPath: '',
    applyDriversToOfflineWindows: false,
    includeGui: true,
    fullIsoUncPath: '',
    fullIsoUncUsername: '',
    fullIsoUncPassword: '',
    fullIsoHttpUrl: '',
    promptUncCredentialsAtRuntime: false,
  },
};

export const WIZARD_STEPS = [
  { id: 0, title: 'Windows Source', description: 'Select Windows version and source' },
  { id: 1, title: 'OOBE & Users', description: 'Configure Out-of-Box Experience and user accounts' },
  { id: 2, title: 'Domain & Autopilot', description: 'Set up domain join or Autopilot' },
  { id: 3, title: 'Applications', description: 'Select apps to install' },
  { id: 4, title: 'Windows Update', description: 'Configure updates to apply' },
  { id: 5, title: 'Policies', description: 'Configure local policy and registry defaults' },
  { id: 6, title: 'Output', description: 'Choose output type and build' },
] as const;
