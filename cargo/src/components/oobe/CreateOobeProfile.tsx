import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/tauri";
import { Plus, Save, Trash2, FolderOpen, RefreshCw } from "lucide-react";
import { OpsPageShell } from "../layout/OpsPageShell";
import { useToast } from "../../contexts/ToastContext";
import { AppModal } from "../shared/AppModal";
import {
  PackageOptionTiles,
  POPULAR_CHOCO_OPTIONS,
  POPULAR_WINGET_OPTIONS,
} from "../shared/PackageOptionTiles";
import {
  LocalPayloadItem,
  deriveLocalPayloadDisplayName,
} from "../../types/localPayload";
import { createDefaultOobeProfileRequest } from "./oobeDefaults";
import {
  OobeCustomInstaller,
  OobeCustomScript,
  OobeProfileDetail,
  OobeProfileRequest,
  OobeProfileSummary,
  OobeTriggerMode,
  PpkgCapabilityStatus,
  PpkgRequest,
  PpkgResponse,
} from "./oobeTypes";

const INSTALLER_EXTENSIONS: Record<
  OobeCustomInstaller["installerType"],
  string[]
> = {
  Exe: ["exe"],
  Msi: ["msi"],
  Msix: ["msix"],
  Msp: ["msp"],
};

const LANGUAGE_OPTIONS = [
  { value: "en-US", label: "English (United States)" },
  { value: "en-GB", label: "English (United Kingdom)" },
  { value: "fr-FR", label: "French (France)" },
  { value: "de-DE", label: "German (Germany)" },
  { value: "ja-JP", label: "Japanese (Japan)" },
  { value: "zh-Hant-TW", label: "Chinese (Traditional, Taiwan)" },
];

const TIMEZONE_OPTIONS = [
  "Pacific Standard Time",
  "Mountain Standard Time",
  "Central Standard Time",
  "Eastern Standard Time",
  "GMT Standard Time",
  "W. Europe Standard Time",
  "Tokyo Standard Time",
  "Taipei Standard Time",
];

function hasExpectedInstallerExtension(
  path: string,
  installerType: OobeCustomInstaller["installerType"],
) {
  const filename = path.split(/[\\/]/).pop() || "";
  const ext = filename.includes(".")
    ? filename.split(".").pop()?.toLowerCase()
    : "";
  if (!ext) {
    return false;
  }
  return INSTALLER_EXTENSIONS[installerType].includes(ext);
}

function parsePackageInput(value: string) {
  return value
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean);
}

function addPackageLine(current: string, entry: string) {
  const normalized = entry.trim();
  if (!normalized) {
    return current;
  }
  const existing = parsePackageInput(current);
  if (
    existing.some((item) => item.toLowerCase() === normalized.toLowerCase())
  ) {
    return current;
  }
  return [...existing, normalized].join("\n");
}

function removePackageLine(current: string, entry: string) {
  const normalized = entry.trim().toLowerCase();
  const next = parsePackageInput(current).filter(
    (item) => item.toLowerCase() !== normalized,
  );
  return next.join("\n");
}

function buildAutoPpkgPath(profilePath: string, profileName: string) {
  const cleaned = profilePath.replace(/[\\/]+$/, "");
  return `${cleaned}\\${profileName}.ppkg`;
}

function isValidDnsServer(value: string) {
  const trimmed = value.trim();
  if (!trimmed) {
    return true;
  }

  if (/^\d{1,3}(\.\d{1,3}){3}$/.test(trimmed)) {
    return trimmed
      .split(".")
      .every((segment) => Number(segment) >= 0 && Number(segment) <= 255);
  }

  return trimmed.includes(":") && /^[0-9a-f:]+$/i.test(trimmed);
}

function createPayloadItem(
  sourcePath: string,
  sourceKind: LocalPayloadItem["sourceKind"],
): LocalPayloadItem {
  const trimmed = sourcePath.trim();
  return {
    sourcePath: trimmed,
    sourceKind,
    displayName: trimmed.split(/[\\/]/).filter(Boolean).pop() || trimmed,
  };
}

interface CreateOobeProfileProps {
  onBack: () => void;
  onOpenManage: () => void;
  editingProfileName?: string | null;
  onClearEditing?: () => void;
}

function blankInstaller(): OobeCustomInstaller {
  return {
    name: "",
    path: "",
    sourceType: "DirectPathOrUrl",
    sourceFileName: "",
    dependencies: [],
    dependencyDestination: "",
    silentArgs: "",
    installerType: "Exe",
    enabled: true,
  };
}

function blankScript(): OobeCustomScript {
  return {
    name: "",
    content: "Write-Host 'Custom script executed'",
    enabled: true,
    continueOnError: true,
  };
}

export function CreateOobeProfile({
  onBack,
  onOpenManage,
  editingProfileName,
  onClearEditing,
}: CreateOobeProfileProps) {
  const { showToast } = useToast();
  const [request, setRequest] = useState<OobeProfileRequest>(
    createDefaultOobeProfileRequest(),
  );
  const [wingetInput, setWingetInput] = useState("");
  const [chocoInput, setChocoInput] = useState("");
  const [customWingetInput, setCustomWingetInput] = useState("");
  const [customChocoInput, setCustomChocoInput] = useState("");
  const [newInstaller, setNewInstaller] =
    useState<OobeCustomInstaller>(blankInstaller());
  const [newScript, setNewScript] = useState<OobeCustomScript>(blankScript());
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [installerError, setInstallerError] = useState<string | null>(null);
  const [showCredentialPrompt, setShowCredentialPrompt] = useState(false);
  const [localAdminUsername, setLocalAdminUsername] = useState("");
  const [localAdminPassword, setLocalAdminPassword] = useState("");
  const credentialResolverRef = useRef<
    ((value: { username: string; password: string } | null) => void) | null
  >(null);

  const promptForLocalAdminCredentials = () =>
    new Promise<{ username: string; password: string } | null>((resolve) => {
      setLocalAdminUsername("");
      setLocalAdminPassword("");
      credentialResolverRef.current = resolve;
      setShowCredentialPrompt(true);
    });

  const getPpkgCapabilityStatus = async () => {
    try {
      return await invoke<PpkgCapabilityStatus>("get_ppkg_capability_status", {
        builderPath: null,
      });
    } catch {
      const fallback: PpkgCapabilityStatus = {
        nativeBuilderAvailable: false,
        localAdminCredentialsRequired: true,
      };
      return fallback;
    }
  };

  const closeCredentialPrompt = (
    value: { username: string; password: string } | null,
  ) => {
    if (credentialResolverRef.current) {
      credentialResolverRef.current(value);
      credentialResolverRef.current = null;
    }
    setShowCredentialPrompt(false);
    setLocalAdminUsername("");
    setLocalAdminPassword("");
  };

  useEffect(() => {
    if (!editingProfileName) {
      setRequest(createDefaultOobeProfileRequest());
      setWingetInput("");
      setChocoInput("");
      setCustomWingetInput("");
      setCustomChocoInput("");
      setError(null);
      return;
    }

    setLoading(true);
    invoke<OobeProfileDetail>("get_oobe_profile", { name: editingProfileName })
      .then((detail) => {
        setRequest({ ...detail.request, overwrite: true });
        setWingetInput(
          detail.request.apps.wingetPackages
            .filter((pkg) => pkg.enabled)
            .map((pkg) => pkg.packageId)
            .join("\n"),
        );
        setChocoInput(
          detail.request.apps.chocolateyPackages
            .filter((pkg) => pkg.enabled)
            .map((pkg) => pkg.packageName)
            .join("\n"),
        );
        setError(null);
      })
      .catch((loadErr) => {
        console.error("Failed to load provisioning package:", loadErr);
        setError(String(loadErr));
        showToast("Failed to load provisioning package", "error");
      })
      .finally(() => setLoading(false));
  }, [editingProfileName, showToast]);

  const wingetPackages = useMemo(
    () => parsePackageInput(wingetInput),
    [wingetInput],
  );
  const chocoPackages = useMemo(
    () => parsePackageInput(chocoInput),
    [chocoInput],
  );
  const wingetPackageSet = useMemo(
    () => new Set(wingetPackages.map((pkg) => pkg.toLowerCase())),
    [wingetPackages],
  );
  const chocoPackageSet = useMemo(
    () => new Set(chocoPackages.map((pkg) => pkg.toLowerCase())),
    [chocoPackages],
  );

  const configuredAppCount = useMemo(() => {
    const copiedItems = request.apps.copiedItems.length;
    const wingetCount = wingetPackages.length;
    const chocoCount = chocoPackages.length;
    const customInstallers = request.apps.customInstallers.filter(
      (item) => item.enabled,
    ).length;
    return copiedItems + wingetCount + chocoCount + customInstallers;
  }, [
    request.apps.copiedItems.length,
    wingetPackages,
    chocoPackages,
    request.apps.customInstallers,
  ]);

  const configuredScriptCount = useMemo(
    () =>
      request.apps.customScripts.filter(
        (item) => item.enabled && item.content.trim(),
      ).length,
    [request.apps.customScripts],
  );
  const configuredWifiDns = useMemo(
    () =>
      [request.wifi.dnsServer1, request.wifi.dnsServer2]
        .map((value) => value.trim())
        .filter(Boolean),
    [request.wifi.dnsServer1, request.wifi.dnsServer2],
  );
  const provisioningSupport = useMemo(() => {
    const fixedComputerName = (request.oobeConfig.computerName || "").trim();
    const hasFixedComputerName = fixedComputerName.length > 0;
    const nativeHideOobe =
      request.oobeConfig.skipMachineOobe && request.oobeConfig.skipUserOobe;
    const nativeWifi =
      request.wifi.enabled &&
      !request.wifi.hiddenNetwork &&
      configuredWifiDns.length === 0 &&
      (request.wifi.authentication === "Open" ||
        request.wifi.authentication === "Wpa2Psk");
    const postSignInWifi = request.wifi.enabled && !nativeWifi;
    const nativeDomainJoin =
      request.domainJoin.enabled &&
      request.domainJoinMode === "SpecializeXml" &&
      hasFixedComputerName;
    const postSignInDomainJoin = request.domainJoin.enabled && !nativeDomainJoin;

    return {
      hasFixedComputerName,
      nativeHideOobe,
      nativeWifi,
      postSignInWifi,
      nativeDomainJoin,
      postSignInDomainJoin,
    };
  }, [configuredWifiDns.length, request]);

  const validateRequest = (draft: OobeProfileRequest): string | null => {
    if (!draft.name.trim()) {
      return "Profile name is required.";
    }

    if (!draft.language.trim()) {
      return "Language is required.";
    }

    if (!draft.inputLocale.trim()) {
      return "Input locale is required. Use a locale tag (example: fr-FR) or keyboard ID (example: 0409:00000409).";
    }

    if (!draft.timezone.trim()) {
      return "Timezone is required.";
    }

    if (draft.defaultUser.enabled) {
      if (
        !draft.defaultUser.username.trim() ||
        !draft.defaultUser.password.trim()
      ) {
        return "Default user requires both username and password.";
      }
    }

    if (draft.triggerMode === "FirstLogonUsbScan") {
      if (!draft.defaultUser.enabled) {
        return "USB media mode requires a default local administrator account for fallback sign-in.";
      }
      if (draft.defaultUser.group !== "Administrators") {
        return "USB media mode requires the default local user to be in the Administrators group.";
      }
    }

    if (draft.domainJoin.enabled) {
      if (
        !draft.domainJoin.domain.trim() ||
        !draft.domainJoin.username.trim() ||
        !draft.domainJoin.password.trim()
      ) {
        return "Domain join requires domain, username, and password.";
      }
    }

    if (draft.wifi.enabled) {
      if (!draft.wifi.ssid.trim()) {
        return "Wi-Fi profile requires an SSID.";
      }
      if (draft.wifi.authentication !== "Open") {
        if (!draft.wifi.password.trim()) {
          return "Wi-Fi profile requires a password for secured networks.";
        }
        if (draft.wifi.password.length < 8 || draft.wifi.password.length > 63) {
          return "Wi-Fi password must be 8-63 characters for secured networks.";
        }
      }
      if (!isValidDnsServer(draft.wifi.dnsServer1)) {
        return "Primary Wi-Fi DNS must be a valid IPv4 or IPv6 address.";
      }
      if (!isValidDnsServer(draft.wifi.dnsServer2)) {
        return "Secondary Wi-Fi DNS must be a valid IPv4 or IPv6 address.";
      }
    }

    return null;
  };

  const toggleWingetPackage = (packageId: string) => {
    if (wingetPackageSet.has(packageId.toLowerCase())) {
      setWingetInput((prev) => removePackageLine(prev, packageId));
    } else {
      setWingetInput((prev) => addPackageLine(prev, packageId));
    }
  };

  const toggleChocolateyPackage = (packageName: string) => {
    if (chocoPackageSet.has(packageName.toLowerCase())) {
      setChocoInput((prev) => removePackageLine(prev, packageName));
    } else {
      setChocoInput((prev) => addPackageLine(prev, packageName));
    }
  };

  const addCopiedItem = (item: LocalPayloadItem) => {
    setRequest((prev) => {
      if (
        prev.apps.copiedItems.some(
          (existing) =>
            existing.sourcePath.toLowerCase() === item.sourcePath.toLowerCase(),
        )
      ) {
        return prev;
      }

      return {
        ...prev,
        apps: {
          ...prev.apps,
          copiedItems: [...prev.apps.copiedItems, item],
        },
      };
    });
  };

  const removeCopiedItem = (sourcePath: string) => {
    setRequest((prev) => ({
      ...prev,
      apps: {
        ...prev.apps,
        copiedItems: prev.apps.copiedItems.filter(
          (item) => item.sourcePath !== sourcePath,
        ),
      },
    }));
  };

  const addInstallerDependency = (item: LocalPayloadItem) => {
    setNewInstaller((prev) => {
      if (
        prev.dependencies.some(
          (existing) =>
            existing.sourcePath.toLowerCase() === item.sourcePath.toLowerCase(),
        )
      ) {
        return prev;
      }

      return {
        ...prev,
        dependencies: [...prev.dependencies, item],
      };
    });
  };

  const removeInstallerDependency = (sourcePath: string) => {
    setNewInstaller((prev) => ({
      ...prev,
      dependencies: prev.dependencies.filter(
        (item) => item.sourcePath !== sourcePath,
      ),
    }));
  };

  const addInstaller = () => {
    const name = newInstaller.name.trim();
    const sourceType = newInstaller.sourceType || "DirectPathOrUrl";
    const path = newInstaller.path.trim();
    const sourceFileName = newInstaller.sourceFileName?.trim();

    if (!name) {
      setInstallerError("Installer name is required.");
      return;
    }

    if (sourceType === "EmbeddedFile") {
      if (!path) {
        setInstallerError("Select a local installer file.");
        return;
      }
      if (!hasExpectedInstallerExtension(path, newInstaller.installerType)) {
        const expected = INSTALLER_EXTENSIONS[newInstaller.installerType]
          .map((ext) => `.${ext}`)
          .join(", ");
        setInstallerError(
          `Selected file extension does not match installer type. Expected: ${expected}`,
        );
        return;
      }
    } else if (sourceType === "NetworkDirectory") {
      if (!path.startsWith("\\\\")) {
        setInstallerError("UNC directory must start with \\\\.");
        return;
      }
      if (!sourceFileName) {
        setInstallerError("UNC filename is required for UNC directory mode.");
        return;
      }
    } else if (!path) {
      setInstallerError("Path or URL is required.");
      return;
    }

    setRequest((prev) => ({
      ...prev,
      apps: {
        ...prev.apps,
        customInstallers: [
          ...prev.apps.customInstallers,
          {
            ...newInstaller,
            name,
            path,
            sourceType,
            sourceFileName:
              sourceType === "NetworkDirectory" ? sourceFileName : undefined,
            dependencyDestination:
              newInstaller.dependencyDestination?.trim() || undefined,
          },
        ],
      },
    }));
    setInstallerError(null);
    setNewInstaller(blankInstaller());
  };

  const removeInstaller = (index: number) => {
    setRequest((prev) => ({
      ...prev,
      apps: {
        ...prev.apps,
        customInstallers: prev.apps.customInstallers.filter(
          (_, i) => i !== index,
        ),
      },
    }));
  };

  const browseInstallerFile = async () => {
    try {
      const extensions = INSTALLER_EXTENSIONS[newInstaller.installerType];
      const filters: Array<[string, string[]]> = [
        [`${newInstaller.installerType} files`, extensions],
        ["All Files", ["*"]],
      ];
      const selectedPath = await invoke<string | null>("show_open_dialog", {
        title: `Select ${newInstaller.installerType} installer`,
        filters,
      });

      if (!selectedPath) {
        return;
      }

      setNewInstaller((prev) => ({
        ...prev,
        sourceType: "EmbeddedFile",
        path: selectedPath,
      }));
      setInstallerError(null);
    } catch (browseErr) {
      console.error("Failed to open installer picker:", browseErr);
      setInstallerError("Failed to open file picker.");
    }
  };

  const browseCopiedFile = async () => {
    try {
      const selectedPath = await invoke<string | null>("show_open_dialog", {
        title: "Select file to copy to installed machine",
        filters: [["All Files", ["*"]]],
      });
      if (selectedPath) {
        addCopiedItem(createPayloadItem(selectedPath, "File"));
      }
    } catch (browseErr) {
      console.error("Failed to open payload file picker:", browseErr);
      showToast("Failed to open file picker", "error");
    }
  };

  const browseCopiedFolder = async () => {
    try {
      const selectedPath = await invoke<string | null>("show_folder_dialog", {
        title: "Select folder to copy to installed machine",
      });
      if (selectedPath) {
        addCopiedItem(createPayloadItem(selectedPath, "Directory"));
      }
    } catch (browseErr) {
      console.error("Failed to open payload folder picker:", browseErr);
      showToast("Failed to open folder picker", "error");
    }
  };

  const browseInstallerDependencyFile = async () => {
    try {
      const selectedPath = await invoke<string | null>("show_open_dialog", {
        title: "Select installer dependency file",
        filters: [["All Files", ["*"]]],
      });
      if (selectedPath) {
        addInstallerDependency(createPayloadItem(selectedPath, "File"));
        setInstallerError(null);
      }
    } catch (browseErr) {
      console.error("Failed to open dependency file picker:", browseErr);
      setInstallerError("Failed to open file picker.");
    }
  };

  const browseInstallerDependencyFolder = async () => {
    try {
      const selectedPath = await invoke<string | null>("show_folder_dialog", {
        title: "Select installer dependency folder",
      });
      if (selectedPath) {
        addInstallerDependency(createPayloadItem(selectedPath, "Directory"));
        setInstallerError(null);
      }
    } catch (browseErr) {
      console.error("Failed to open dependency folder picker:", browseErr);
      setInstallerError("Failed to open folder picker.");
    }
  };

  const addScript = () => {
    if (!newScript.name.trim() || !newScript.content.trim()) {
      showToast("Script name and content are required", "warning");
      return;
    }

    setRequest((prev) => ({
      ...prev,
      apps: {
        ...prev.apps,
        enableCustomScripts: true,
        customScripts: [...prev.apps.customScripts, newScript],
      },
    }));
    setNewScript(blankScript());
  };

  const removeScript = (index: number) => {
    setRequest((prev) => ({
      ...prev,
      apps: {
        ...prev.apps,
        customScripts: prev.apps.customScripts.filter((_, i) => i !== index),
      },
    }));
  };

  const handleSave = async () => {
    const wingetPackagePayload = wingetPackages.map((packageId) => ({
      packageId,
      enabled: true,
    }));
    const chocolateyPackagePayload = chocoPackages.map((packageName) => ({
      packageName,
      enabled: true,
    }));

    const payload: OobeProfileRequest = {
      ...request,
      overwrite: Boolean(editingProfileName) || request.overwrite,
      apps: {
        ...request.apps,
        wingetPackages: wingetPackagePayload,
        chocolateyPackages: chocolateyPackagePayload,
      },
    };

    const validationError = validateRequest(payload);
    if (validationError) {
      setError(validationError);
      return;
    }

    try {
      setSaving(true);
      setError(null);
      const summary = await invoke<OobeProfileSummary>("create_oobe_profile", {
        request: payload,
      });
      setRequest((prev) => ({ ...prev, name: summary.name, overwrite: true }));
      showToast(`Provisioning package '${summary.name}' generated`, "success");
      const autoPpkgPath = buildAutoPpkgPath(summary.path, summary.name);
      const capability = await getPpkgCapabilityStatus();
      const credentials = capability.localAdminCredentialsRequired
        ? await promptForLocalAdminCredentials()
        : null;
      if (capability.localAdminCredentialsRequired && !credentials) {
        showToast(
          "Profile saved. PPKG generation canceled because local admin credentials were not provided for fallback mode.",
          "warning",
        );
        if (onClearEditing) {
          onClearEditing();
        }
        return;
      }
      const ppkgRequest: PpkgRequest = {
        profileName: summary.name,
        outputPpkgPath: autoPpkgPath,
        localAdminUsername: credentials?.username,
        localAdminPassword: credentials?.password,
      };
      try {
        const ppkgResult = await invoke<PpkgResponse>("generate_oobe_ppkg", {
          request: ppkgRequest,
        });
        if (ppkgResult.warnings.length > 0) {
          showToast(
            `PPKG generated with warnings. Keep the .ppkg with sibling Scripts, Apps, and Files folders. Logs: ${ppkgResult.logsPath}`,
            "warning",
          );
        } else {
          showToast(
            `PPKG generated: ${ppkgResult.outputPpkgPath}. Keep the .ppkg with sibling Scripts, Apps, and Files folders.`,
            "success",
          );
        }
      } catch (ppkgErr) {
        showToast(
          `Profile saved, but PPKG generation failed: ${String(ppkgErr)}`,
          "warning",
        );
      }
      if (onClearEditing) {
        onClearEditing();
      }
    } catch (saveErr) {
      console.error("Failed to create provisioning package:", saveErr);
      setError(String(saveErr));
      showToast("Failed to create provisioning package", "error");
    } finally {
      setSaving(false);
    }
  };

  if (loading) {
    return (
      <div className="ops-loading-screen">
        <div className="ops-spinner" />
        <p>Loading provisioning package...</p>
      </div>
    );
  }

  return (
    <>
      <OpsPageShell
        kicker="Provisioning Package Builder"
        title={
          editingProfileName
            ? `Edit Provisioning Package: ${editingProfileName}`
            : "Create Provisioning Package"
        }
        subtitle="Configure the post-logon provisioning flow, then choose whether payloads come from USB media or a provisioning package."
        onBack={onBack}
        headerActions={
          <button
            type="button"
            className="ops-btn ops-btn-secondary"
            onClick={onOpenManage}
          >
            <FolderOpen size={15} />
            <span>Manage Provisioning Package</span>
          </button>
        }
      >
        <div className="ops-layout-stack">
          <section className="ops-card">
            <h2 className="ops-card-title">Profile Details</h2>
            <div className="ops-oobe-grid">
              <label className="ops-field">
                <span className="ops-label">Profile Name *</span>
                <input
                  className="ops-input"
                  value={request.name}
                  onChange={(event) =>
                    setRequest((prev) => ({
                      ...prev,
                      name: event.target.value,
                    }))
                  }
                  placeholder="Example: Win11-BranchOffice-OOBE"
                />
              </label>
              <label className="ops-field">
                <span className="ops-label">Description</span>
                <input
                  className="ops-input"
                  value={request.description}
                  onChange={(event) =>
                    setRequest((prev) => ({
                      ...prev,
                      description: event.target.value,
                    }))
                  }
                  placeholder="Optional profile description"
                />
              </label>
            </div>
          </section>

          <section className="ops-card">
            <h2 className="ops-card-title">Trigger Semantics</h2>
            <label className="ops-field">
              <span className="ops-label">Execution Model</span>
              <select
                className="ops-select"
                value={
                  request.triggerMode === "SetupUnattend"
                    ? "LegacySetupUnattend"
                    : "PostLogonOrchestrated"
                }
                onChange={(event) =>
                  setRequest((prev) => ({
                    ...prev,
                    triggerMode:
                      event.target.value === "LegacySetupUnattend"
                        ? "SetupUnattend"
                        : prev.triggerMode === "ProvisioningPackage"
                          ? "ProvisioningPackage"
                          : "FirstLogonUsbScan",
                  }))
                }
              >
                <option value="PostLogonOrchestrated">
                  Post-Logon Orchestrated
                </option>
                {request.triggerMode === "SetupUnattend" ? (
                  <option value="LegacySetupUnattend">
                    SetupUnattend (legacy/hidden mode)
                  </option>
                ) : null}
              </select>
            </label>
            {request.triggerMode !== "SetupUnattend" ? (
              <label className="ops-field">
                <span className="ops-label">Deployment Source</span>
                <select
                  className="ops-select"
                  value={
                    request.triggerMode === "ProvisioningPackage"
                      ? "ProvisioningPackage"
                      : "FirstLogonUsbScan"
                  }
                  onChange={(event) =>
                    setRequest((prev) => ({
                      ...prev,
                      triggerMode: event.target.value as OobeTriggerMode,
                    }))
                  }
                >
                  <option value="FirstLogonUsbScan">
                    USB Media (Autounattend + first-logon scan)
                  </option>
                  <option value="ProvisioningPackage">
                    Provisioning Package (.ppkg)
                  </option>
                </select>
              </label>
            ) : null}
            <p className="ops-card-subtitle">
              {request.triggerMode === "SetupUnattend"
                ? "SetupUnattend is a legacy setup-media flow and is hidden for new profiles."
                : request.triggerMode === "FirstLogonUsbScan"
                  ? "Post-logon orchestration is enabled. BitOSDT uses temporary built-in Administrator autologon to stage the USB payload, resume across reboots, and then hand off to the default local administrator account."
                  : "Post-logon orchestration is enabled. Payloads are delivered through .ppkg bootstrap, then orchestrator phases run at first admin sign-in."}
            </p>
            {request.triggerMode === "ProvisioningPackage" ? (
              <div className="ops-layout-stack ops-compact-stack">
                <div className="wizard-alert wizard-alert-info">
                  Provisioning package mode is hybrid. BitOSDT applies supported
                  native `.ppkg` settings during package installation, then
                  resumes the remaining work at first administrator sign-in.
                </div>
                <div className="ops-detail-grid">
                  <div>
                    <span>Native During Apply</span>
                    <strong>
                      {[
                        request.defaultUser.enabled
                          ? "Default user"
                          : null,
                        provisioningSupport.hasFixedComputerName
                          ? "Fixed computer name"
                          : null,
                        provisioningSupport.nativeDomainJoin
                          ? "Domain join"
                          : null,
                        provisioningSupport.nativeWifi ? "Wi-Fi profile" : null,
                        provisioningSupport.nativeHideOobe
                          ? "HideOobe"
                          : null,
                      ]
                        .filter(Boolean)
                        .join(", ") || "None selected"}
                    </strong>
                  </div>
                  <div>
                    <span>Post-Sign-In</span>
                    <strong>
                      {[
                        !provisioningSupport.hasFixedComputerName &&
                        request.promptForComputerName
                          ? "Prompted computer name"
                          : null,
                        provisioningSupport.postSignInDomainJoin
                          ? "Domain join"
                          : null,
                        provisioningSupport.postSignInWifi
                          ? "Wi-Fi profile"
                          : null,
                        configuredAppCount > 0 ? "Apps / copied files" : null,
                        request.enableDebloat ? "Debloat" : null,
                        configuredScriptCount > 0 ? "Custom scripts" : null,
                      ]
                        .filter(Boolean)
                        .join(", ") || "None pending"}
                    </strong>
                  </div>
                </div>
                <p className="ops-card-subtitle">
                  Fine-grained privacy, wireless, and online-account OOBE
                  toggles are not emitted as native `.ppkg` settings here.
                  BitOSDT only applies broad HideOobe natively when both skip
                  toggles are enabled.
                </p>
              </div>
            ) : null}
          </section>

          <section className="ops-card">
            <h2 className="ops-card-title">OOBE & Computer Name</h2>
            <div className="ops-oobe-grid">
              <label className="ops-field">
                <span className="ops-label">Computer Name</span>
                <input
                  className="ops-input"
                  value={request.oobeConfig.computerName || ""}
                  onChange={(event) =>
                    setRequest((prev) => ({
                      ...prev,
                      oobeConfig: {
                        ...prev.oobeConfig,
                        computerName: event.target.value,
                      },
                    }))
                  }
                  placeholder="Leave blank for <ComputerName>*</ComputerName>"
                />
              </label>

              <label className="ops-field">
                <span className="ops-label">Network Location</span>
                <select
                  className="ops-select"
                  value={request.oobeConfig.networkLocation}
                  onChange={(event) =>
                    setRequest((prev) => ({
                      ...prev,
                      oobeConfig: {
                        ...prev.oobeConfig,
                        networkLocation: event.target.value as
                          | "Home"
                          | "Work"
                          | "Other",
                      },
                    }))
                  }
                >
                  <option value="Work">Work</option>
                  <option value="Home">Home</option>
                  <option value="Other">Other</option>
                </select>
              </label>
              <label className="ops-field">
                <span className="ops-label">Language / Region</span>
                <select
                  className="ops-select"
                  value={request.language}
                  onChange={(event) =>
                    setRequest((prev) => ({
                      ...prev,
                      language: event.target.value,
                    }))
                  }
                >
                  {LANGUAGE_OPTIONS.map((option) => (
                    <option key={option.value} value={option.value}>
                      {option.label}
                    </option>
                  ))}
                </select>
              </label>

              <label className="ops-field">
                <span className="ops-label">Input Locale</span>
                <input
                  className="ops-input"
                  value={request.inputLocale}
                  onChange={(event) =>
                    setRequest((prev) => ({
                      ...prev,
                      inputLocale: event.target.value,
                    }))
                  }
                  placeholder="Example: 0409:00000409 or fr-FR"
                />
              </label>

              <label className="ops-field">
                <span className="ops-label">Timezone</span>
                <select
                  className="ops-select"
                  value={request.timezone}
                  onChange={(event) =>
                    setRequest((prev) => ({
                      ...prev,
                      timezone: event.target.value,
                    }))
                  }
                >
                  {TIMEZONE_OPTIONS.map((tz) => (
                    <option key={tz} value={tz}>
                      {tz}
                    </option>
                  ))}
                </select>
              </label>
            </div>
            {request.triggerMode === "ProvisioningPackage" ? (
              <p className="ops-card-subtitle">
                Provisioning package mode applies a fixed computer name during
                package installation. If you leave Computer Name blank, BitOSDT
                keeps naming as a post-sign-in prompt instead.
              </p>
            ) : null}

            <div className="ops-oobe-checks">
              <label>
                <input
                  type="checkbox"
                  checked={request.oobeConfig.skipMachineOobe}
                  onChange={(event) =>
                    setRequest((prev) => ({
                      ...prev,
                      oobeConfig: {
                        ...prev.oobeConfig,
                        skipMachineOobe: event.target.checked,
                      },
                    }))
                  }
                />{" "}
                Skip Machine OOBE
              </label>
              <label>
                <input
                  type="checkbox"
                  checked={request.oobeConfig.skipUserOobe}
                  onChange={(event) =>
                    setRequest((prev) => ({
                      ...prev,
                      oobeConfig: {
                        ...prev.oobeConfig,
                        skipUserOobe: event.target.checked,
                      },
                    }))
                  }
                />{" "}
                Skip User OOBE
              </label>
              <label>
                <input
                  type="checkbox"
                  checked={request.oobeConfig.hideEula}
                  onChange={(event) =>
                    setRequest((prev) => ({
                      ...prev,
                      oobeConfig: {
                        ...prev.oobeConfig,
                        hideEula: event.target.checked,
                      },
                    }))
                  }
                />{" "}
                Accept EULA Automatically
              </label>
              <label>
                <input
                  type="checkbox"
                  checked={request.oobeConfig.hidePrivacySettings}
                  onChange={(event) =>
                    setRequest((prev) => ({
                      ...prev,
                      oobeConfig: {
                        ...prev.oobeConfig,
                        hidePrivacySettings: event.target.checked,
                      },
                    }))
                  }
                />{" "}
                Skip Privacy Settings Screen
              </label>
              <label>
                <input
                  type="checkbox"
                  checked={request.oobeConfig.hideWirelessSetup}
                  onChange={(event) =>
                    setRequest((prev) => ({
                      ...prev,
                      oobeConfig: {
                        ...prev.oobeConfig,
                        hideWirelessSetup: event.target.checked,
                      },
                    }))
                  }
                />{" "}
                Hide Wireless Setup
              </label>
              <label>
                <input
                  type="checkbox"
                  checked={request.oobeConfig.hideOnlineAccountScreens}
                  onChange={(event) =>
                    setRequest((prev) => ({
                      ...prev,
                      oobeConfig: {
                        ...prev.oobeConfig,
                        hideOnlineAccountScreens: event.target.checked,
                      },
                    }))
                  }
                />{" "}
                Hide Online Account Screens
              </label>
              <label>
                <input
                  type="checkbox"
                  checked={request.promptForComputerName}
                  onChange={(event) =>
                    setRequest((prev) => ({
                      ...prev,
                      promptForComputerName: event.target.checked,
                    }))
                  }
                />{" "}
                Prompt for PC name if no Computer Name is set
              </label>
            </div>
          </section>

          <section className="ops-card">
            <h2 className="ops-card-title">Domain Join</h2>
            <label className="ops-inline-toggle">
              <input
                type="checkbox"
                checked={request.domainJoin.enabled}
                onChange={(event) =>
                  setRequest((prev) => ({
                    ...prev,
                    domainJoin: {
                      ...prev.domainJoin,
                      enabled: event.target.checked,
                    },
                  }))
                }
              />
              <span>Enable domain join</span>
            </label>

            {request.domainJoin.enabled ? (
              <div className="ops-layout-stack ops-compact-stack">
                <div className="ops-oobe-radio-row">
                  <label>
                    <input
                      type="radio"
                      checked={request.domainJoinMode === "SpecializeXml"}
                      onChange={() =>
                        setRequest((prev) => ({
                          ...prev,
                          domainJoinMode: "SpecializeXml",
                        }))
                      }
                    />
                    <span>Specialize XML Mode</span>
                  </label>
                  <label>
                    <input
                      type="radio"
                      checked={request.domainJoinMode === "PostRenameScript"}
                      onChange={() =>
                        setRequest((prev) => ({
                          ...prev,
                          domainJoinMode: "PostRenameScript",
                        }))
                      }
                    />
                    <span>Post-Rename Script Mode</span>
                  </label>
                </div>
                {request.triggerMode === "ProvisioningPackage" ? (
                  <p className="ops-card-subtitle">
                    ProvisioningPackage applies domain join natively only when
                    you use Specialize XML mode with a fixed Computer Name.
                    Post-Rename Script mode, or leaving the name to be prompted
                    later, keeps domain join in the post-sign-in orchestrator.
                  </p>
                ) : null}

                <div className="ops-oobe-grid">
                  <label className="ops-field">
                    <span className="ops-label">Domain *</span>
                    <input
                      className="ops-input"
                      value={request.domainJoin.domain}
                      onChange={(event) =>
                        setRequest((prev) => ({
                          ...prev,
                          domainJoin: {
                            ...prev.domainJoin,
                            domain: event.target.value,
                          },
                        }))
                      }
                      placeholder="company.local"
                    />
                  </label>
                  <label className="ops-field">
                    <span className="ops-label">OU Path</span>
                    <input
                      className="ops-input"
                      value={request.domainJoin.ouPath || ""}
                      onChange={(event) =>
                        setRequest((prev) => ({
                          ...prev,
                          domainJoin: {
                            ...prev.domainJoin,
                            ouPath: event.target.value,
                          },
                        }))
                      }
                      placeholder="OU=Computers,DC=company,DC=local"
                    />
                  </label>
                  <label className="ops-field">
                    <span className="ops-label">Username *</span>
                    <input
                      className="ops-input"
                      value={request.domainJoin.username}
                      onChange={(event) =>
                        setRequest((prev) => ({
                          ...prev,
                          domainJoin: {
                            ...prev.domainJoin,
                            username: event.target.value,
                          },
                        }))
                      }
                      placeholder="company\\domainjoin"
                    />
                  </label>
                  <label className="ops-field">
                    <span className="ops-label">Password *</span>
                    <input
                      type="password"
                      className="ops-input"
                      value={request.domainJoin.password}
                      onChange={(event) =>
                        setRequest((prev) => ({
                          ...prev,
                          domainJoin: {
                            ...prev.domainJoin,
                            password: event.target.value,
                          },
                        }))
                      }
                    />
                  </label>
                </div>
              </div>
            ) : null}
          </section>

          <section className="ops-card">
            <h2 className="ops-card-title">Default User</h2>
            {request.triggerMode === "FirstLogonUsbScan" ? (
              <p className="ops-card-subtitle">
                USB media mode requires this local administrator account.
                BitOSDT falls back to it after the temporary bootstrap
                Administrator session is cleaned up.
              </p>
            ) : request.triggerMode === "ProvisioningPackage" ? (
              <p className="ops-card-subtitle">
                Provisioning package mode creates this local user during
                package installation rather than waiting for the post-sign-in
                orchestrator.
              </p>
            ) : null}
            <label className="ops-inline-toggle">
              <input
                type="checkbox"
                checked={request.defaultUser.enabled}
                onChange={(event) =>
                  setRequest((prev) => ({
                    ...prev,
                    defaultUser: {
                      ...prev.defaultUser,
                      enabled: event.target.checked,
                    },
                  }))
                }
              />
              <span>Create default local user</span>
            </label>

            {request.defaultUser.enabled ? (
              <div className="ops-oobe-grid">
                <label className="ops-field">
                  <span className="ops-label">Username *</span>
                  <input
                    className="ops-input"
                    value={request.defaultUser.username}
                    onChange={(event) =>
                      setRequest((prev) => ({
                        ...prev,
                        defaultUser: {
                          ...prev.defaultUser,
                          username: event.target.value,
                        },
                      }))
                    }
                  />
                </label>
                <label className="ops-field">
                  <span className="ops-label">Password *</span>
                  <input
                    type="password"
                    className="ops-input"
                    value={request.defaultUser.password}
                    onChange={(event) =>
                      setRequest((prev) => ({
                        ...prev,
                        defaultUser: {
                          ...prev.defaultUser,
                          password: event.target.value,
                        },
                      }))
                    }
                  />
                </label>
                <label className="ops-field">
                  <span className="ops-label">Group</span>
                  <select
                    className="ops-select"
                    value={request.defaultUser.group}
                    onChange={(event) =>
                      setRequest((prev) => ({
                        ...prev,
                        defaultUser: {
                          ...prev.defaultUser,
                          group: event.target.value as
                            | "Administrators"
                            | "Users",
                        },
                      }))
                    }
                  >
                    <option value="Administrators">Administrator</option>
                    <option value="Users">Standard</option>
                  </select>
                </label>
              </div>
            ) : null}
            {request.triggerMode === "FirstLogonUsbScan" &&
            request.defaultUser.enabled &&
            request.defaultUser.group !== "Administrators" ? (
              <p className="ops-card-subtitle" style={{ color: "#b45309" }}>
                USB media mode will not generate until this user is set to
                Administrator.
              </p>
            ) : null}
          </section>

          <section className="ops-card">
            <h2 className="ops-card-title">Wi-Fi Profile</h2>
            <label className="ops-inline-toggle">
              <input
                type="checkbox"
                checked={request.wifi.enabled}
                onChange={(event) =>
                  setRequest((prev) => ({
                    ...prev,
                    wifi: { ...prev.wifi, enabled: event.target.checked },
                  }))
                }
              />
              <span>Configure default Wi-Fi for auto-connect</span>
            </label>

            {request.wifi.enabled ? (
              <div className="ops-oobe-grid">
                <label className="ops-field">
                  <span className="ops-label">SSID *</span>
                  <input
                    className="ops-input"
                    value={request.wifi.ssid}
                    onChange={(event) =>
                      setRequest((prev) => ({
                        ...prev,
                        wifi: { ...prev.wifi, ssid: event.target.value },
                      }))
                    }
                    placeholder="Company-WiFi"
                  />
                </label>

                <label className="ops-field">
                  <span className="ops-label">Authentication</span>
                  <select
                    className="ops-select"
                    value={request.wifi.authentication}
                    onChange={(event) =>
                      setRequest((prev) => {
                        const authentication = event.target.value as
                          | "Open"
                          | "Wpa2Psk"
                          | "Wpa3Sae";
                        return {
                          ...prev,
                          wifi: {
                            ...prev.wifi,
                            authentication,
                            encryption:
                              authentication === "Open"
                                ? "None"
                                : prev.wifi.encryption === "None"
                                  ? "Aes"
                                  : prev.wifi.encryption,
                            password:
                              authentication === "Open"
                                ? ""
                                : prev.wifi.password,
                          },
                        };
                      })
                    }
                  >
                    <option value="Wpa2Psk">WPA2-PSK</option>
                    <option value="Wpa3Sae">WPA3-SAE</option>
                    <option value="Open">Open</option>
                  </select>
                </label>

                <label className="ops-field">
                  <span className="ops-label">Encryption</span>
                  <select
                    className="ops-select"
                    value={request.wifi.encryption}
                    onChange={(event) =>
                      setRequest((prev) => ({
                        ...prev,
                        wifi: {
                          ...prev.wifi,
                          encryption: event.target.value as
                            | "None"
                            | "Aes"
                            | "Tkip",
                        },
                      }))
                    }
                    disabled={request.wifi.authentication === "Open"}
                  >
                    <option value="Aes">AES</option>
                    <option value="Tkip">TKIP</option>
                    <option value="None">None</option>
                  </select>
                </label>

                <label className="ops-field">
                  <span className="ops-label">
                    Password{" "}
                    {request.wifi.authentication === "Open"
                      ? "(not required)"
                      : "*"}
                  </span>
                  <input
                    type="password"
                    className="ops-input"
                    value={request.wifi.password}
                    onChange={(event) =>
                      setRequest((prev) => ({
                        ...prev,
                        wifi: { ...prev.wifi, password: event.target.value },
                      }))
                    }
                    disabled={request.wifi.authentication === "Open"}
                  />
                </label>

                <label className="ops-field">
                  <span className="ops-label">Primary DNS</span>
                  <input
                    className="ops-input"
                    value={request.wifi.dnsServer1}
                    onChange={(event) =>
                      setRequest((prev) => ({
                        ...prev,
                        wifi: { ...prev.wifi, dnsServer1: event.target.value },
                      }))
                    }
                    placeholder="192.168.1.10"
                  />
                </label>

                <label className="ops-field">
                  <span className="ops-label">Secondary DNS</span>
                  <input
                    className="ops-input"
                    value={request.wifi.dnsServer2}
                    onChange={(event) =>
                      setRequest((prev) => ({
                        ...prev,
                        wifi: { ...prev.wifi, dnsServer2: event.target.value },
                      }))
                    }
                    placeholder="192.168.1.11"
                  />
                </label>

                <label className="ops-inline-toggle">
                  <input
                    type="checkbox"
                    checked={request.wifi.autoConnect}
                    onChange={(event) =>
                      setRequest((prev) => ({
                        ...prev,
                        wifi: {
                          ...prev.wifi,
                          autoConnect: event.target.checked,
                        },
                      }))
                    }
                  />
                  <span>Auto-connect</span>
                </label>

                <label className="ops-inline-toggle">
                  <input
                    type="checkbox"
                    checked={request.wifi.hiddenNetwork}
                    onChange={(event) =>
                      setRequest((prev) => ({
                        ...prev,
                        wifi: {
                          ...prev.wifi,
                          hiddenNetwork: event.target.checked,
                        },
                      }))
                    }
                  />
                  <span>Hidden network</span>
                </label>
              </div>
            ) : null}
            {request.wifi.enabled ? (
              <p className="ops-card-subtitle">
                {request.triggerMode === "ProvisioningPackage"
                  ? "Provisioning package mode applies Wi-Fi natively only for Open or WPA2-PSK networks that are not hidden and keep DNS on DHCP. Hidden SSIDs, WPA3, or manual DNS stay in the post-sign-in orchestrator."
                  : "Leave DNS blank to keep DHCP values. If set, BitOSDT applies up to two DNS servers to the active Wi-Fi adapter after it connects."}
              </p>
            ) : null}
          </section>

          <section className="ops-card">
            <h2 className="ops-card-title">Software / Packages</h2>
            {request.triggerMode === "ProvisioningPackage" ? (
              <div className="ops-layout-stack ops-compact-stack">
                <h3 className="ops-card-title">BitLocker</h3>
                <label className="ops-inline-toggle">
                  <input
                    type="checkbox"
                    checked={request.apps.disableBitLocker}
                    onChange={(event) =>
                      setRequest((prev) => ({
                        ...prev,
                        apps: {
                          ...prev.apps,
                          disableBitLocker: event.target.checked,
                          rebootAfterDisableBitLocker: event.target.checked
                            ? prev.apps.rebootAfterDisableBitLocker
                            : false,
                        },
                      }))
                    }
                  />
                  <span>Disable BitLocker on C: before app installs</span>
                </label>
                <p className="text-xs text-gray-500">
                  BitOSDT runs <code>manage-bde -off C:</code> as a dedicated
                  provisioning step before the applications phase.
                </p>
                {request.apps.disableBitLocker ? (
                  <label className="ops-inline-toggle">
                    <input
                      type="checkbox"
                      checked={request.apps.rebootAfterDisableBitLocker}
                      onChange={(event) =>
                        setRequest((prev) => ({
                          ...prev,
                          apps: {
                            ...prev.apps,
                            rebootAfterDisableBitLocker:
                              event.target.checked,
                          },
                        }))
                      }
                    />
                    <span>Restart after disabling BitLocker</span>
                  </label>
                ) : null}
              </div>
            ) : null}

            <div className="ops-layout-stack ops-compact-stack">
              <h3 className="ops-card-title">Winget Packages</h3>
              <p className="text-xs text-gray-500">
                BitOSDT auto-detects Winget. If OOBE runs as <code>SYSTEM</code>
                , installs are deferred to first admin logon and App Installer
                registration is attempted if Winget is missing.
              </p>
              <PackageOptionTiles
                items={POPULAR_WINGET_OPTIONS}
                selectedIds={wingetPackageSet}
                onToggle={toggleWingetPackage}
              />
              <div style={{ display: "flex", gap: "0.55rem" }}>
                <input
                  className="ops-input"
                  style={{ flex: 1 }}
                  value={customWingetInput}
                  onChange={(event) => setCustomWingetInput(event.target.value)}
                  placeholder="Add Winget package ID (e.g. Microsoft.WindowsTerminal)"
                />
                <button
                  type="button"
                  className="ops-btn ops-btn-secondary"
                  onClick={() => {
                    if (!customWingetInput.trim()) {
                      return;
                    }
                    setWingetInput((prev) =>
                      addPackageLine(prev, customWingetInput),
                    );
                    setCustomWingetInput("");
                  }}
                >
                  <Plus size={15} />
                  <span>Add</span>
                </button>
              </div>
              <label className="ops-field">
                <span className="ops-label">
                  Winget Package IDs (one per line)
                </span>
                <textarea
                  className="ops-input ops-oobe-textarea"
                  value={wingetInput}
                  onChange={(event) => setWingetInput(event.target.value)}
                />
              </label>
              {wingetPackages.length > 0 ? (
                <div className="ops-oobe-list">
                  {wingetPackages.map((pkg) => (
                    <article key={pkg} className="ops-oobe-list-item">
                      <div>
                        <strong>{pkg}</strong>
                      </div>
                      <button
                        type="button"
                        className="ops-icon-btn ops-icon-btn-danger"
                        onClick={() =>
                          setWingetInput((prev) => removePackageLine(prev, pkg))
                        }
                      >
                        <Trash2 size={14} />
                      </button>
                    </article>
                  ))}
                </div>
              ) : null}
            </div>

            <div className="ops-layout-stack ops-compact-stack">
              <h3 className="ops-card-title">Chocolatey Packages</h3>
              <label className="ops-inline-toggle">
                <input
                  type="checkbox"
                  checked={request.apps.autoInstallChocolatey}
                  onChange={(event) =>
                    setRequest((prev) => ({
                      ...prev,
                      apps: {
                        ...prev.apps,
                        autoInstallChocolatey: event.target.checked,
                      },
                    }))
                  }
                />
                <span>
                  Automatically install Chocolatey if it is not present
                </span>
              </label>
              <PackageOptionTiles
                items={POPULAR_CHOCO_OPTIONS}
                selectedIds={chocoPackageSet}
                onToggle={toggleChocolateyPackage}
              />
              <div style={{ display: "flex", gap: "0.55rem" }}>
                <input
                  className="ops-input"
                  style={{ flex: 1 }}
                  value={customChocoInput}
                  onChange={(event) => setCustomChocoInput(event.target.value)}
                  placeholder="Add Chocolatey package (e.g. git)"
                />
                <button
                  type="button"
                  className="ops-btn ops-btn-secondary"
                  onClick={() => {
                    if (!customChocoInput.trim()) {
                      return;
                    }
                    setChocoInput((prev) =>
                      addPackageLine(prev, customChocoInput),
                    );
                    setCustomChocoInput("");
                  }}
                >
                  <Plus size={15} />
                  <span>Add</span>
                </button>
              </div>
              <label className="ops-field">
                <span className="ops-label">
                  Chocolatey Package Names (one per line)
                </span>
                <textarea
                  className="ops-input ops-oobe-textarea"
                  value={chocoInput}
                  onChange={(event) => setChocoInput(event.target.value)}
                />
              </label>
              {chocoPackages.length > 0 ? (
                <div className="ops-oobe-list">
                  {chocoPackages.map((pkg) => (
                    <article key={pkg} className="ops-oobe-list-item">
                      <div>
                        <strong>{pkg}</strong>
                      </div>
                      <button
                        type="button"
                        className="ops-icon-btn ops-icon-btn-danger"
                        onClick={() =>
                          setChocoInput((prev) => removePackageLine(prev, pkg))
                        }
                      >
                        <Trash2 size={14} />
                      </button>
                    </article>
                  ))}
                </div>
              ) : null}
            </div>

            <div className="ops-layout-stack ops-compact-stack">
              <h3 className="ops-card-title">Files and Folders</h3>
              <div className="ops-field">
                <span className="ops-label">
                  Destination on Installed Machine
                </span>
                <input
                  className="ops-input"
                  placeholder="C:\\BitOSDT\\Files\\"
                  value={request.apps.copyDestination || ""}
                  onChange={(event) =>
                    setRequest((prev) => ({
                      ...prev,
                      apps: {
                        ...prev.apps,
                        copyDestination: event.target.value,
                      },
                    }))
                  }
                />
                <p className="text-xs text-gray-500 mt-1">
                  Leave blank to use <code>C:\BitOSDT\Files\</code>.
                </p>
              </div>
              <div
                style={{ display: "flex", gap: "0.55rem", flexWrap: "wrap" }}
              >
                <button
                  type="button"
                  className="ops-btn ops-btn-secondary"
                  onClick={browseCopiedFile}
                >
                  <FolderOpen size={15} />
                  <span>Add File</span>
                </button>
                <button
                  type="button"
                  className="ops-btn ops-btn-secondary"
                  onClick={browseCopiedFolder}
                >
                  <FolderOpen size={15} />
                  <span>Add Folder</span>
                </button>
              </div>
              {request.apps.copiedItems.length > 0 ? (
                <div className="ops-oobe-list">
                  {request.apps.copiedItems.map((item) => (
                    <article
                      key={item.sourcePath}
                      className="ops-oobe-list-item"
                    >
                      <div>
                        <strong>{deriveLocalPayloadDisplayName(item)}</strong>
                        <p>
                          {item.sourceKind} - {item.sourcePath}
                        </p>
                      </div>
                      <button
                        type="button"
                        className="ops-icon-btn ops-icon-btn-danger"
                        onClick={() => removeCopiedItem(item.sourcePath)}
                      >
                        <Trash2 size={14} />
                      </button>
                    </article>
                  ))}
                </div>
              ) : (
                <p className="text-sm text-gray-500">
                  No files or folders selected.
                </p>
              )}
            </div>

            <div className="ops-layout-stack ops-compact-stack">
              <h3 className="ops-card-title">Custom Installers</h3>
              <div className="ops-oobe-grid">
                <input
                  className="ops-input"
                  placeholder="Name"
                  value={newInstaller.name}
                  onChange={(event) =>
                    setNewInstaller((prev) => ({
                      ...prev,
                      name: event.target.value,
                    }))
                  }
                />
                <select
                  className="ops-select"
                  value={newInstaller.installerType}
                  onChange={(event) =>
                    setNewInstaller((prev) => ({
                      ...prev,
                      installerType: event.target
                        .value as OobeCustomInstaller["installerType"],
                    }))
                  }
                >
                  <option value="Exe">EXE</option>
                  <option value="Msi">MSI</option>
                  <option value="Msix">MSIX</option>
                  <option value="Msp">MSP</option>
                </select>
                <select
                  className="ops-select"
                  value={newInstaller.sourceType || "DirectPathOrUrl"}
                  onChange={(event) =>
                    setNewInstaller((prev) => ({
                      ...prev,
                      sourceType: event.target
                        .value as OobeCustomInstaller["sourceType"],
                    }))
                  }
                >
                  <option value="DirectPathOrUrl">Direct path/URL</option>
                  <option value="EmbeddedFile">Embedded file</option>
                  <option value="NetworkDirectory">UNC directory</option>
                </select>
                {newInstaller.sourceType === "EmbeddedFile" ? (
                  <div
                    style={{
                      display: "flex",
                      gap: "0.55rem",
                      gridColumn: "1 / -1",
                    }}
                  >
                    <input
                      className="ops-input"
                      style={{ flex: 1 }}
                      placeholder={`Select .${INSTALLER_EXTENSIONS[newInstaller.installerType].join(", .")} file`}
                      value={newInstaller.path}
                      onChange={(event) =>
                        setNewInstaller((prev) => ({
                          ...prev,
                          path: event.target.value,
                        }))
                      }
                    />
                    <button
                      type="button"
                      className="ops-btn ops-btn-secondary"
                      onClick={browseInstallerFile}
                    >
                      <FolderOpen size={15} />
                      <span>Browse</span>
                    </button>
                  </div>
                ) : null}
                {newInstaller.sourceType === "NetworkDirectory" ? (
                  <>
                    <input
                      className="ops-input"
                      placeholder="UNC directory (e.g. \\\\server\\share\\apps)"
                      value={newInstaller.path}
                      onChange={(event) =>
                        setNewInstaller((prev) => ({
                          ...prev,
                          path: event.target.value,
                        }))
                      }
                    />
                    <input
                      className="ops-input"
                      placeholder="UNC filename (e.g. setup.msi)"
                      value={newInstaller.sourceFileName || ""}
                      onChange={(event) =>
                        setNewInstaller((prev) => ({
                          ...prev,
                          sourceFileName: event.target.value,
                        }))
                      }
                    />
                  </>
                ) : null}
                {newInstaller.sourceType === "DirectPathOrUrl" ? (
                  <input
                    className="ops-input"
                    placeholder="Path / URL"
                    value={newInstaller.path}
                    onChange={(event) =>
                      setNewInstaller((prev) => ({
                        ...prev,
                        path: event.target.value,
                      }))
                    }
                  />
                ) : null}
                <input
                  className="ops-input"
                  placeholder="Silent args"
                  value={newInstaller.silentArgs}
                  onChange={(event) =>
                    setNewInstaller((prev) => ({
                      ...prev,
                      silentArgs: event.target.value,
                    }))
                  }
                />
              </div>
              <div className="ops-layout-stack ops-compact-stack">
                <div className="ops-field">
                  <span className="ops-label">Dependency Destination</span>
                  <input
                    className="ops-input"
                    placeholder="C:\\BitOSDT\\Files\\"
                    value={newInstaller.dependencyDestination || ""}
                    onChange={(event) =>
                      setNewInstaller((prev) => ({
                        ...prev,
                        dependencyDestination: event.target.value,
                      }))
                    }
                  />
                  <p className="text-xs text-gray-500 mt-1">
                    Leave blank to use <code>C:\BitOSDT\Files\</code>.
                  </p>
                </div>
                <div
                  style={{ display: "flex", gap: "0.55rem", flexWrap: "wrap" }}
                >
                  <button
                    type="button"
                    className="ops-btn ops-btn-secondary"
                    onClick={browseInstallerDependencyFile}
                  >
                    <FolderOpen size={15} />
                    <span>Add Dependency File</span>
                  </button>
                  <button
                    type="button"
                    className="ops-btn ops-btn-secondary"
                    onClick={browseInstallerDependencyFolder}
                  >
                    <FolderOpen size={15} />
                    <span>Add Dependency Folder</span>
                  </button>
                </div>
                {newInstaller.dependencies.length > 0 ? (
                  <div className="ops-oobe-list">
                    {newInstaller.dependencies.map((item) => (
                      <article
                        key={item.sourcePath}
                        className="ops-oobe-list-item"
                      >
                        <div>
                          <strong>{deriveLocalPayloadDisplayName(item)}</strong>
                          <p>
                            {item.sourceKind} - {item.sourcePath}
                          </p>
                        </div>
                        <button
                          type="button"
                          className="ops-icon-btn ops-icon-btn-danger"
                          onClick={() =>
                            removeInstallerDependency(item.sourcePath)
                          }
                        >
                          <Trash2 size={14} />
                        </button>
                      </article>
                    ))}
                  </div>
                ) : (
                  <p className="text-sm text-gray-500">
                    No dependencies selected.
                  </p>
                )}
              </div>
              {newInstaller.sourceType === "EmbeddedFile" ? (
                <p className="text-xs text-gray-500">
                  Embedded installers are copied into the profile{" "}
                  <code>Apps</code> folder and run from <code>C:\Apps</code>{" "}
                  during USB OOBE.
                </p>
              ) : null}
              {installerError ? (
                <div className="wizard-alert wizard-alert-error">
                  {installerError}
                </div>
              ) : null}
              <button
                type="button"
                className="ops-btn ops-btn-secondary"
                onClick={addInstaller}
              >
                <Plus size={15} />
                <span>Add Installer</span>
              </button>
              {request.apps.customInstallers.length > 0 ? (
                <div className="ops-oobe-list">
                  {request.apps.customInstallers.map((installer, index) => (
                    <article
                      key={`${installer.name}-${index}`}
                      className="ops-oobe-list-item"
                    >
                      <div>
                        <strong>{installer.name}</strong>
                        <p>
                          {installer.installerType} -{" "}
                          {installer.sourceType || "DirectPathOrUrl"} -{" "}
                          {installer.path}
                        </p>
                        {installer.dependencies.length > 0 ? (
                          <p>
                            Dependencies: {installer.dependencies.length} -{" "}
                            {installer.dependencyDestination ||
                              "C:\\BitOSDT\\Files\\"}
                          </p>
                        ) : null}
                      </div>
                      <button
                        type="button"
                        className="ops-icon-btn ops-icon-btn-danger"
                        onClick={() => removeInstaller(index)}
                      >
                        <Trash2 size={14} />
                      </button>
                    </article>
                  ))}
                </div>
              ) : null}
            </div>
          </section>

          <section className="ops-card">
            <h2 className="ops-card-title">Debloat + Custom Scripts</h2>
            <label className="ops-inline-toggle">
              <input
                type="checkbox"
                checked={request.enableDebloat}
                onChange={(event) =>
                  setRequest((prev) => ({
                    ...prev,
                    enableDebloat: event.target.checked,
                  }))
                }
              />
              <span>Enable debloat script</span>
            </label>
            {request.enableDebloat ? (
              <label className="ops-field">
                <span className="ops-label">
                  Debloat Script Override (optional)
                </span>
                <textarea
                  className="ops-input ops-oobe-textarea"
                  value={request.debloatScriptContent}
                  onChange={(event) =>
                    setRequest((prev) => ({
                      ...prev,
                      debloatScriptContent: event.target.value,
                    }))
                  }
                  placeholder="Leave blank to use default debloat script"
                />
              </label>
            ) : null}

            <label className="ops-inline-toggle">
              <input
                type="checkbox"
                checked={request.apps.enableCustomScripts}
                onChange={(event) =>
                  setRequest((prev) => ({
                    ...prev,
                    apps: {
                      ...prev.apps,
                      enableCustomScripts: event.target.checked,
                    },
                  }))
                }
              />
              <span>Enable custom scripts</span>
            </label>

            {request.apps.enableCustomScripts ? (
              <div className="ops-layout-stack ops-compact-stack">
                <input
                  className="ops-input"
                  placeholder="Script name"
                  value={newScript.name}
                  onChange={(event) =>
                    setNewScript((prev) => ({
                      ...prev,
                      name: event.target.value,
                    }))
                  }
                />
                <textarea
                  className="ops-input ops-oobe-textarea"
                  placeholder="Script content"
                  value={newScript.content}
                  onChange={(event) =>
                    setNewScript((prev) => ({
                      ...prev,
                      content: event.target.value,
                    }))
                  }
                />
                <button
                  type="button"
                  className="ops-btn ops-btn-secondary"
                  onClick={addScript}
                >
                  <Plus size={15} />
                  <span>Add Script</span>
                </button>

                {request.apps.customScripts.length > 0 ? (
                  <div className="ops-oobe-list">
                    {request.apps.customScripts.map((script, index) => (
                      <article
                        key={`${script.name}-${index}`}
                        className="ops-oobe-list-item"
                      >
                        <div>
                          <strong>{script.name}</strong>
                          <p>
                            {script.enabled ? "Enabled" : "Disabled"} -{" "}
                            {script.continueOnError
                              ? "Continue on error"
                              : "Stop on error"}
                          </p>
                        </div>
                        <button
                          type="button"
                          className="ops-icon-btn ops-icon-btn-danger"
                          onClick={() => removeScript(index)}
                        >
                          <Trash2 size={14} />
                        </button>
                      </article>
                    ))}
                  </div>
                ) : null}
              </div>
            ) : null}
          </section>

          <section className="ops-card">
            <h2 className="ops-card-title">Review</h2>
            <div className="ops-detail-grid">
              <div>
                <span>Profile</span>
                <strong>{request.name || "Not set"}</strong>
              </div>
              <div>
                <span>Domain Join</span>
                <strong>
                  {request.domainJoin.enabled
                    ? request.domainJoinMode
                    : "Disabled"}
                </strong>
              </div>
              <div>
                <span>Default User</span>
                <strong>
                  {request.defaultUser.enabled
                    ? `${request.defaultUser.username} (${request.defaultUser.group})`
                    : "Disabled"}
                </strong>
              </div>
              <div>
                <span>Wi-Fi</span>
                <strong>
                  {request.wifi.enabled
                    ? `${request.wifi.ssid} (${request.wifi.authentication}${configuredWifiDns.length > 0 ? ` / DNS ${configuredWifiDns.join(", ")}` : ""})`
                    : "Disabled"}
                </strong>
              </div>
              <div>
                <span>App Tasks</span>
                <strong>{configuredAppCount}</strong>
              </div>
              <div>
                <span>Custom Scripts</span>
                <strong>{configuredScriptCount}</strong>
              </div>
              <div>
                <span>Debloat</span>
                <strong>
                  {request.enableDebloat ? "Enabled" : "Disabled"}
                </strong>
              </div>
            </div>

            {error ? (
              <div
                className="wizard-alert wizard-alert-error"
                style={{ marginTop: "0.9rem" }}
              >
                {error}
              </div>
            ) : null}

            <div className="ops-cluster" style={{ marginTop: "0.9rem" }}>
              <button
                type="button"
                className="ops-btn ops-btn-primary"
                onClick={handleSave}
                disabled={saving}
              >
                {saving ? (
                  <RefreshCw size={15} className="animate-spin" />
                ) : (
                  <Save size={15} />
                )}
                <span>
                  {saving ? "Generating..." : "Generate Provisioning Package"}
                </span>
              </button>
              <button
                type="button"
                className="ops-btn ops-btn-secondary"
                onClick={onOpenManage}
              >
                <FolderOpen size={15} />
                <span>Open Provisioning Package Library</span>
              </button>
            </div>
          </section>
        </div>
      </OpsPageShell>
      {showCredentialPrompt ? (
        <AppModal
          open
          onClose={() => closeCredentialPrompt(null)}
          size="compact"
          labelledBy="ppkg-local-admin-title"
        >
          <>
            <div className="ops-modal-head">
              <div>
                <h2 id="ppkg-local-admin-title">
                  Local Admin Credential Required
                </h2>
                <p>
                  Provide local admin credentials for environments that need
                  ProvisioningTools fallback instead of native ICD.
                </p>
              </div>
            </div>
            <div className="ops-modal-body">
              <label className="ops-field">
                <span className="ops-label">Local admin username</span>
                <input
                  className="ops-input"
                  value={localAdminUsername}
                  onChange={(event) =>
                    setLocalAdminUsername(event.target.value)
                  }
                  placeholder="Administrator"
                />
              </label>
              <label className="ops-field">
                <span className="ops-label">Local admin password</span>
                <input
                  type="password"
                  className="ops-input"
                  value={localAdminPassword}
                  onChange={(event) =>
                    setLocalAdminPassword(event.target.value)
                  }
                  placeholder="Enter password"
                />
              </label>
            </div>
            <div className="ops-modal-foot">
              <button
                type="button"
                className="ops-btn ops-btn-ghost"
                onClick={() => closeCredentialPrompt(null)}
              >
                Cancel
              </button>
              <button
                type="button"
                className="ops-btn ops-btn-primary"
                onClick={() =>
                  closeCredentialPrompt({
                    username: localAdminUsername.trim(),
                    password: localAdminPassword,
                  })
                }
                disabled={!localAdminUsername.trim() || !localAdminPassword}
              >
                Continue
              </button>
            </div>
          </>
        </AppModal>
      ) : null}
    </>
  );
}

export default CreateOobeProfile;
