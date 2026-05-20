import { LocalPayloadItem } from "../../types/localPayload";

export type DomainJoinMode = "SpecializeXml" | "PostRenameScript";
export type OobeTriggerMode =
  | "SetupUnattend"
  | "FirstLogonUsbScan"
  | "ProvisioningPackage";

export interface OobeUiConfig {
  skipMachineOobe: boolean;
  skipUserOobe: boolean;
  hideEula: boolean;
  hidePrivacySettings: boolean;
  hideWirelessSetup: boolean;
  hideLocalAccountScreen: boolean;
  hideOnlineAccountScreens: boolean;
  networkLocation: "Home" | "Work" | "Other";
  protectYourPc: "Recommended" | "Custom" | "Off";
  computerName?: string;
}

export interface DomainJoinUiConfig {
  enabled: boolean;
  domain: string;
  username: string;
  password: string;
  ouPath?: string;
}

export interface DefaultUserUiConfig {
  enabled: boolean;
  username: string;
  password: string;
  group: "Administrators" | "Users";
}

export type OobeWifiAuthentication = "Open" | "Wpa2Psk" | "Wpa3Sae";
export type OobeWifiEncryption = "None" | "Aes" | "Tkip";

export interface OobeWifiConfig {
  enabled: boolean;
  ssid: string;
  password: string;
  authentication: OobeWifiAuthentication;
  encryption: OobeWifiEncryption;
  autoConnect: boolean;
  hiddenNetwork: boolean;
  dnsServer1: string;
  dnsServer2: string;
}

export interface OobeWingetPackage {
  packageId: string;
  version?: string;
  customArgs?: string;
  enabled: boolean;
}

export interface OobeChocolateyPackage {
  packageName: string;
  version?: string;
  source?: string;
  customArgs?: string;
  enabled: boolean;
}

export type OobeInstallerSource =
  | "EmbeddedFile"
  | "NetworkDirectory"
  | "DirectPathOrUrl";
export type OobeInstallerType = "Exe" | "Msi" | "Msix" | "Msp";

export interface OobeCustomInstaller {
  name: string;
  path: string;
  sourceType?: OobeInstallerSource;
  sourceFileName?: string;
  dependencies: LocalPayloadItem[];
  dependencyDestination?: string;
  silentArgs: string;
  installerType: OobeInstallerType;
  enabled: boolean;
}

export interface OobeCustomScript {
  name: string;
  content: string;
  enabled: boolean;
  continueOnError: boolean;
}

export interface OobeAppsConfig {
  wingetPackages: OobeWingetPackage[];
  chocolateyPackages: OobeChocolateyPackage[];
  customInstallers: OobeCustomInstaller[];
  copiedItems: LocalPayloadItem[];
  copyDestination?: string;
  disableBitLocker: boolean;
  rebootAfterDisableBitLocker: boolean;
  autoInstallChocolatey: boolean;
  continueOnError: boolean;
  enableCustomScripts: boolean;
  customScripts: OobeCustomScript[];
}

export interface OobeProfileRequest {
  name: string;
  description: string;
  overwrite: boolean;
  triggerMode: OobeTriggerMode;
  oobeConfig: OobeUiConfig;
  domainJoin: DomainJoinUiConfig;
  domainJoinMode: DomainJoinMode;
  promptForComputerName: boolean;
  defaultUser: DefaultUserUiConfig;
  wifi: OobeWifiConfig;
  apps: OobeAppsConfig;
  language: string;
  inputLocale: string;
  timezone: string;
  enableDebloat: boolean;
  debloatScriptContent: string;
}

export interface OobeProfileSummary {
  name: string;
  description: string;
  path: string;
  updatedAt: string;
  hasManifest: boolean;
  preflightWarnings?: string[];
}

export interface OobeProfilePreflight {
  profileName: string;
  profilePath: string;
  warnings: string[];
}

export interface OobeProfileDetail {
  name: string;
  path: string;
  createdAt: string;
  updatedAt: string;
  request: OobeProfileRequest;
}

export interface PpkgSigningMetadata {
  pfxPath: string;
  password?: string;
  timestampUrl?: string;
}

export interface PpkgRequest {
  profileName?: string;
  profilePath?: string;
  outputPpkgPath: string;
  builderPath?: string;
  owner?: string;
  rank?: number;
  version?: string;
  signing?: PpkgSigningMetadata;
  localAdminUsername?: string;
  localAdminPassword?: string;
}

export interface PpkgResponse {
  outputPpkgPath: string;
  logsPath: string;
  warnings: string[];
}

export interface PpkgCapabilityStatus {
  nativeBuilderAvailable: boolean;
  localAdminCredentialsRequired: boolean;
}
