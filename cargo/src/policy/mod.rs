#[cfg(target_os = "windows")]
use crate::deploy::HardwareDetector;
use crate::tasks::{RegistryConfig, RegistryOperation, RegistryValueType};
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
#[cfg(target_os = "windows")]
use std::process::Command;

const POLICY_DEFINITIONS_DIR: &str = r"C:\Windows\PolicyDefinitions";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PolicyCategory {
    Security,
    Privacy,
    Performance,
    Updates,
    Network,
    Custom,
}

impl PolicyCategory {
    pub fn label(self) -> &'static str {
        match self {
            Self::Security => "Security",
            Self::Privacy => "Privacy",
            Self::Performance => "Performance",
            Self::Updates => "Updates",
            Self::Network => "Network",
            Self::Custom => "Custom",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PolicyImpact {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PolicySourceKind {
    Admx,
    Curated,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PolicySupportStatus {
    pub supported: bool,
    pub supported_on: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PolicyCatalogEntry {
    pub id: String,
    pub source_kind: PolicySourceKind,
    pub category: PolicyCategory,
    pub display_name: String,
    pub description: String,
    pub impact: PolicyImpact,
    #[serde(default)]
    pub starter: bool,
    #[serde(default)]
    pub selectable: bool,
    pub support: PolicySupportStatus,
    pub read_only_reason: Option<String>,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub category_label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CustomRegistryEntry {
    pub id: String,
    pub key_path: String,
    pub value_name: String,
    pub value_type: RegistryValueType,
    pub value_data: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct GroupPolicySelection {
    #[serde(default)]
    pub selected_policy_ids: Vec<String>,
    #[serde(default)]
    pub custom_registry_entries: Vec<CustomRegistryEntry>,
    pub last_applied_preset_id: Option<String>,
    pub last_applied_preset_name: Option<String>,
}

impl GroupPolicySelection {
    pub fn is_empty(&self) -> bool {
        self.selected_policy_ids.is_empty() && self.custom_registry_entries.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PolicyPreset {
    pub id: String,
    pub name: String,
    pub built_in: bool,
    #[serde(default)]
    pub selected_policy_ids: Vec<String>,
    #[serde(default)]
    pub custom_registry_entries: Vec<CustomRegistryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PolicyHostContext {
    pub available: bool,
    pub summary: String,
    pub product_name: String,
    pub edition_id: String,
    pub display_version: String,
    pub build_number: u32,
    pub installation_type: String,
    pub architecture: String,
    pub ui_language: String,
    pub policy_definitions_path: String,
    pub is_vm: bool,
    pub tpm_spec_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PolicyEditorBootstrap {
    pub available: bool,
    pub unavailable_reason: Option<String>,
    pub host: PolicyHostContext,
    #[serde(default)]
    pub starter_policies: Vec<PolicyCatalogEntry>,
    #[serde(default)]
    pub catalog: Vec<PolicyCatalogEntry>,
    #[serde(default)]
    pub built_in_presets: Vec<PolicyPreset>,
    #[serde(default)]
    pub saved_presets: Vec<PolicyPreset>,
}

#[derive(Debug, Clone)]
struct SelectablePolicyDefinition {
    entry: PolicyCatalogEntry,
    operations: Vec<RegistryOperation>,
}

#[derive(Debug, Clone)]
struct PolicyCatalogBundle {
    bootstrap: PolicyEditorBootstrap,
    selectable_definitions: HashMap<String, SelectablePolicyDefinition>,
}

#[cfg(target_os = "windows")]
#[derive(Debug, Clone)]
struct HostDetectionRaw {
    product_name: String,
    edition_id: String,
    display_version: String,
    build_number: u32,
    installation_type: String,
    ui_language: String,
}

#[derive(Debug, Clone)]
struct AdmxFileMetadata {
    target_prefix: Option<String>,
    categories: HashMap<String, String>,
    supported_definitions: HashMap<String, String>,
}

#[derive(Debug, Clone, Default)]
struct PartialPolicy {
    name: String,
    class_name: String,
    display_name: String,
    explain_text: String,
    key: Option<String>,
    value_name: Option<String>,
    parent_category_ref: Option<String>,
    supported_on_ref: Option<String>,
    enabled_operations: Vec<RegistryOperation>,
    has_enabled_config: bool,
}

#[derive(Debug, Clone, Copy)]
enum RegistryCaptureSection {
    EnabledValue,
    EnabledList,
}

#[derive(Debug, Clone)]
struct PendingListItem {
    key: String,
    value_name: String,
}

#[derive(Debug, Clone)]
struct CuratedPolicyDefinition {
    entry: PolicyCatalogEntry,
    operations: Vec<RegistryOperation>,
    min_client_rank: Option<i32>,
    min_server_rank: Option<i32>,
    max_client_rank: Option<i32>,
    max_server_rank: Option<i32>,
    allow_server: bool,
    allow_client: bool,
    allowed_editions: Vec<&'static str>,
}

#[derive(Debug, Clone)]
struct SupportEvaluation {
    supported: bool,
    reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PolicyPresetSetting {
    #[serde(default)]
    presets: Vec<PolicyPreset>,
}

pub fn empty_group_policy_selection_value() -> serde_json::Value {
    serde_json::to_value(GroupPolicySelection::default()).unwrap_or_else(|_| {
        serde_json::json!({
            "selectedPolicyIds": [],
            "customRegistryEntries": [],
            "lastAppliedPresetId": null,
            "lastAppliedPresetName": null
        })
    })
}

pub fn load_saved_policy_presets_from_json(raw: Option<&str>) -> Result<Vec<PolicyPreset>, String> {
    let Some(raw) = raw.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(Vec::new());
    };

    if let Ok(setting) = serde_json::from_str::<PolicyPresetSetting>(raw) {
        return Ok(setting.presets);
    }

    serde_json::from_str::<Vec<PolicyPreset>>(raw)
        .map_err(|e| format!("Failed to parse saved policy presets JSON: {}", e))
}

pub fn serialize_saved_policy_presets(presets: &[PolicyPreset]) -> Result<String, String> {
    serde_json::to_string(&PolicyPresetSetting {
        presets: presets.to_vec(),
    })
    .map_err(|e| format!("Failed to serialize policy presets JSON: {}", e))
}

pub fn built_in_policy_presets() -> Vec<PolicyPreset> {
    vec![
        PolicyPreset {
            id: "builtin-balanced-hardening".to_string(),
            name: "Balanced Hardening".to_string(),
            built_in: true,
            selected_policy_ids: vec![
                "curated-smartscreen-explorer".to_string(),
                "curated-smartscreen-reputation".to_string(),
                "curated-defender-pua-protection".to_string(),
                "curated-disable-consumer-features".to_string(),
                "curated-disable-tailored-experiences".to_string(),
                "curated-exclude-wu-drivers".to_string(),
                "curated-disable-llmnr".to_string(),
            ],
            custom_registry_entries: Vec::new(),
        },
        PolicyPreset {
            id: "builtin-privacy-hardened".to_string(),
            name: "Privacy Hardened".to_string(),
            built_in: true,
            selected_policy_ids: vec![
                "curated-disable-consumer-features".to_string(),
                "curated-disable-tailored-experiences".to_string(),
                "curated-disable-advertising-id".to_string(),
                "curated-disable-telemetry".to_string(),
                "curated-disable-web-search".to_string(),
            ],
            custom_registry_entries: Vec::new(),
        },
        PolicyPreset {
            id: "builtin-performance-focus".to_string(),
            name: "Performance Focus".to_string(),
            built_in: true,
            selected_policy_ids: vec![
                "curated-disable-delivery-optimization".to_string(),
                "curated-disable-web-search".to_string(),
                "curated-disable-tailored-experiences".to_string(),
            ],
            custom_registry_entries: Vec::new(),
        },
        PolicyPreset {
            id: "builtin-update-control".to_string(),
            name: "Update Control".to_string(),
            built_in: true,
            selected_policy_ids: vec![
                "curated-exclude-wu-drivers".to_string(),
                "curated-no-auto-reboot-with-logged-on-users".to_string(),
                "curated-au-notify-download".to_string(),
            ],
            custom_registry_entries: Vec::new(),
        },
    ]
}

pub fn load_policy_editor_bootstrap() -> Result<PolicyEditorBootstrap, String> {
    Ok(load_policy_catalog_bundle()?.bootstrap)
}

pub fn resolve_policy_registry_config(
    selection: &GroupPolicySelection,
) -> Result<Option<RegistryConfig>, String> {
    if selection.is_empty() {
        return Ok(None);
    }

    let bundle = load_policy_catalog_bundle()?;
    if !bundle.bootstrap.available {
        return Err(bundle
            .bootstrap
            .unavailable_reason
            .unwrap_or_else(|| "Policy editor is unavailable on this host.".to_string()));
    }

    let mut operations = Vec::new();
    for policy_id in &selection.selected_policy_ids {
        let definition = bundle
            .selectable_definitions
            .get(policy_id)
            .ok_or_else(|| {
                format!(
                    "Selected policy '{}' is not available on this host.",
                    policy_id
                )
            })?;

        if !definition.entry.support.supported {
            return Err(format!(
                "Selected policy '{}' is unsupported on this host: {}",
                definition.entry.display_name, definition.entry.support.reason
            ));
        }
        if !definition.entry.selectable {
            return Err(format!(
                "Selected policy '{}' cannot be applied because it is read-only in this release.",
                definition.entry.display_name
            ));
        }

        operations.extend(definition.operations.clone());
    }

    for entry in &selection.custom_registry_entries {
        operations.push(custom_entry_to_operation(entry)?);
    }

    if operations.is_empty() {
        return Ok(None);
    }

    let mut deduped = BTreeMap::<(String, String), RegistryOperation>::new();
    for operation in operations {
        deduped.insert((operation.key.clone(), operation.name.clone()), operation);
    }

    Ok(Some(RegistryConfig {
        operations: deduped.into_values().collect(),
    }))
}

fn load_policy_catalog_bundle() -> Result<PolicyCatalogBundle, String> {
    let host = detect_policy_host_context()?;
    if !host.available {
        return Ok(PolicyCatalogBundle {
            bootstrap: PolicyEditorBootstrap {
                available: false,
                unavailable_reason: Some(host.summary.clone()),
                host,
                starter_policies: Vec::new(),
                catalog: Vec::new(),
                built_in_presets: built_in_policy_presets(),
                saved_presets: Vec::new(),
            },
            selectable_definitions: HashMap::new(),
        });
    }

    let policy_definitions_dir = Path::new(&host.policy_definitions_path);
    if !policy_definitions_dir.is_dir() {
        let summary = format!(
            "Policy definitions directory was not found at {}.",
            host.policy_definitions_path
        );
        return Ok(PolicyCatalogBundle {
            bootstrap: PolicyEditorBootstrap {
                available: false,
                unavailable_reason: Some(summary.clone()),
                host: PolicyHostContext { summary, ..host },
                starter_policies: Vec::new(),
                catalog: Vec::new(),
                built_in_presets: built_in_policy_presets(),
                saved_presets: Vec::new(),
            },
            selectable_definitions: HashMap::new(),
        });
    }

    let ui_language = host.ui_language.clone();
    let metadata = collect_admx_file_metadata(policy_definitions_dir, &ui_language)?;
    let mut category_display = HashMap::new();
    let mut supported_display = HashMap::new();
    for file in metadata.values() {
        category_display.extend(file.categories.clone());
        supported_display.extend(file.supported_definitions.clone());
    }

    let mut catalog = Vec::new();
    let mut selectable_definitions = HashMap::new();
    for admx_path in iter_admx_paths(policy_definitions_dir)? {
        let file_stem = admx_path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_string();
        let strings = load_adml_strings(policy_definitions_dir, &file_stem, &ui_language)?;
        let target_prefix = metadata
            .get(&file_stem)
            .and_then(|value| value.target_prefix.clone());

        for policy in parse_admx_policies(&admx_path, &strings, target_prefix.as_deref())? {
            if !policy.class_name.eq_ignore_ascii_case("machine")
                && !policy.class_name.eq_ignore_ascii_case("both")
            {
                continue;
            }

            let parent_ref = policy.parent_category_ref.as_deref().unwrap_or_default();
            let resolved_category_display = resolve_lookup_value(&category_display, parent_ref)
                .unwrap_or_else(|| "Windows Components".to_string());
            let supported_ref = policy.supported_on_ref.clone();
            let supported_display_name = supported_ref
                .as_deref()
                .and_then(|value| resolve_lookup_value(&supported_display, value));
            let support = evaluate_admx_support(
                &host,
                supported_ref.as_deref(),
                supported_display_name.as_deref(),
            );
            let display_name = if policy.display_name.trim().is_empty() {
                policy.name.clone()
            } else {
                policy.display_name.clone()
            };
            let description = if policy.explain_text.trim().is_empty() {
                "Parsed from local Windows PolicyDefinitions.".to_string()
            } else {
                policy.explain_text.clone()
            };
            let category = infer_policy_category(
                &display_name,
                &description,
                Some(&resolved_category_display),
            );
            let selectable = policy.has_enabled_config && !policy.enabled_operations.is_empty();
            let read_only_reason = if selectable {
                None
            } else {
                Some(
                    "This ADMX policy needs structured input beyond a simple checkbox and is read-only in v1."
                        .to_string(),
                )
            };
            let entry = PolicyCatalogEntry {
                id: format!("admx:{}:{}", file_stem.to_ascii_lowercase(), policy.name),
                source_kind: PolicySourceKind::Admx,
                category,
                display_name,
                description,
                impact: PolicyImpact::Medium,
                starter: false,
                selectable,
                support: PolicySupportStatus {
                    supported: support.supported,
                    supported_on: supported_display_name.clone(),
                    reason: support.reason,
                },
                read_only_reason,
                aliases: vec![
                    resolved_category_display.clone(),
                    file_stem.clone(),
                    policy.name,
                ],
                category_label: resolved_category_display,
            };

            if selectable {
                selectable_definitions.insert(
                    entry.id.clone(),
                    SelectablePolicyDefinition {
                        entry: entry.clone(),
                        operations: policy.enabled_operations.clone(),
                    },
                );
            }
            catalog.push(entry);
        }
    }

    for curated in curated_policy_definitions() {
        let support = evaluate_curated_support(&host, &curated);
        let mut entry = curated.entry.clone();
        entry.support = PolicySupportStatus {
            supported: support.supported,
            supported_on: entry.support.supported_on.clone(),
            reason: support.reason,
        };
        selectable_definitions.insert(
            entry.id.clone(),
            SelectablePolicyDefinition {
                entry: entry.clone(),
                operations: curated.operations.clone(),
            },
        );
        catalog.push(entry);
    }

    catalog.sort_by(|left, right| {
        left.category_label
            .cmp(&right.category_label)
            .then_with(|| left.display_name.cmp(&right.display_name))
    });

    let starter_policies = catalog
        .iter()
        .filter(|entry| entry.starter)
        .cloned()
        .collect::<Vec<_>>();

    Ok(PolicyCatalogBundle {
        bootstrap: PolicyEditorBootstrap {
            available: true,
            unavailable_reason: None,
            host,
            starter_policies,
            catalog,
            built_in_presets: built_in_policy_presets(),
            saved_presets: Vec::new(),
        },
        selectable_definitions,
    })
}

fn curated_policy_definitions() -> Vec<CuratedPolicyDefinition> {
    vec![
        curated_policy(
            "curated-defender-pua-protection",
            PolicyCategory::Security,
            "Enable Microsoft Defender PUA protection",
            "Blocks potentially unwanted applications during deployment.",
            PolicyImpact::Medium,
            true,
            "At least Windows 10",
            vec![reg_dword(
                r"HKLM:\SOFTWARE\Policies\Microsoft\Windows Defender\MpEngine",
                "MpEnablePus",
                1,
            )],
            CuratedSupportRule::client_10_plus(),
            vec!["defender".into(), "pua".into(), "potentially unwanted".into()],
        ),
        curated_policy(
            "curated-smartscreen-explorer",
            PolicyCategory::Security,
            "Require SmartScreen for Explorer",
            "Forces Microsoft Defender SmartScreen checks for downloaded content opened from Explorer.",
            PolicyImpact::Low,
            true,
            "At least Windows 10",
            vec![reg_dword(
                r"HKLM:\SOFTWARE\Policies\Microsoft\Windows\System",
                "EnableSmartScreen",
                1,
            )],
            CuratedSupportRule::client_10_plus(),
            vec!["smartscreen".into(), "explorer".into(), "defender reputation".into()],
        ),
        curated_policy(
            "curated-smartscreen-reputation",
            PolicyCategory::Security,
            "Set SmartScreen shell level to Warn",
            "Uses the default warning shell experience instead of allowing unknown files silently.",
            PolicyImpact::Low,
            true,
            "At least Windows 10",
            vec![reg_string(
                r"HKLM:\SOFTWARE\Policies\Microsoft\Windows\System",
                "ShellSmartScreenLevel",
                "Warn",
            )],
            CuratedSupportRule::client_10_plus(),
            vec!["smartscreen".into(), "warn".into(), "reputation".into()],
        ),
        curated_policy(
            "curated-lsa-protection",
            PolicyCategory::Security,
            "Enable LSA protection",
            "Runs Local Security Authority as a protected process. This can affect older security tooling.",
            PolicyImpact::High,
            true,
            "At least Windows 10",
            vec![reg_dword(
                r"HKLM:\SYSTEM\CurrentControlSet\Control\Lsa",
                "RunAsPPL",
                1,
            )],
            CuratedSupportRule::client_10_plus(),
            vec!["lsa".into(), "credential".into(), "protected process".into()],
        ),
        curated_policy(
            "curated-disable-consumer-features",
            PolicyCategory::Privacy,
            "Disable Windows consumer features",
            "Stops Microsoft consumer experiences and suggested app provisioning on first sign-in.",
            PolicyImpact::Low,
            true,
            "At least Windows 10",
            vec![reg_dword(
                r"HKLM:\SOFTWARE\Policies\Microsoft\Windows\CloudContent",
                "DisableWindowsConsumerFeatures",
                1,
            )],
            CuratedSupportRule::client_10_plus(),
            vec!["consumer".into(), "suggested apps".into(), "cloud content".into()],
        ),
        curated_policy(
            "curated-disable-tailored-experiences",
            PolicyCategory::Privacy,
            "Disable tailored experiences with diagnostic data",
            "Prevents Windows from using diagnostic data for personalized recommendations and tips.",
            PolicyImpact::Low,
            true,
            "At least Windows 10",
            vec![reg_dword(
                r"HKLM:\SOFTWARE\Policies\Microsoft\Windows\CloudContent",
                "DisableTailoredExperiencesWithDiagnosticData",
                1,
            )],
            CuratedSupportRule::client_10_plus(),
            vec!["tailored experiences".into(), "diagnostic".into(), "recommendations".into()],
        ),
        curated_policy(
            "curated-disable-advertising-id",
            PolicyCategory::Privacy,
            "Disable the advertising ID",
            "Turns off the per-user advertising ID experience at the machine policy level.",
            PolicyImpact::Low,
            true,
            "At least Windows 10",
            vec![reg_dword(
                r"HKLM:\SOFTWARE\Policies\Microsoft\Windows\AdvertisingInfo",
                "DisabledByGroupPolicy",
                1,
            )],
            CuratedSupportRule::client_10_plus(),
            vec!["advertising".into(), "ads".into(), "privacy".into()],
        ),
        curated_policy(
            "curated-disable-telemetry",
            PolicyCategory::Privacy,
            "Set diagnostic data to Security/Required only",
            "Configures the machine telemetry level to the lowest supported value for managed devices.",
            PolicyImpact::High,
            true,
            "At least Windows 10",
            vec![reg_dword(
                r"HKLM:\SOFTWARE\Policies\Microsoft\Windows\DataCollection",
                "AllowTelemetry",
                0,
            )],
            CuratedSupportRule::client_10_plus(),
            vec!["telemetry".into(), "diagnostic data".into(), "data collection".into()],
        ),
        curated_policy(
            "curated-disable-web-search",
            PolicyCategory::Performance,
            "Disable web search in Start/Search",
            "Keeps the shell search experience local-only and reduces background web lookups.",
            PolicyImpact::Low,
            true,
            "At least Windows 10",
            vec![
                reg_dword(
                    r"HKLM:\SOFTWARE\Policies\Microsoft\Windows\Windows Search",
                    "DisableWebSearch",
                    1,
                ),
                reg_dword(
                    r"HKLM:\SOFTWARE\Policies\Microsoft\Windows\Windows Search",
                    "ConnectedSearchUseWeb",
                    0,
                ),
            ],
            CuratedSupportRule::client_10_plus(),
            vec!["search".into(), "bing".into(), "start menu".into()],
        ),
        curated_policy(
            "curated-disable-delivery-optimization",
            PolicyCategory::Performance,
            "Disable Delivery Optimization peering",
            "Forces Delivery Optimization into HTTP-only mode instead of peer sharing.",
            PolicyImpact::Low,
            true,
            "At least Windows 10",
            vec![reg_dword(
                r"HKLM:\SOFTWARE\Policies\Microsoft\Windows\DeliveryOptimization",
                "DODownloadMode",
                0,
            )],
            CuratedSupportRule::client_10_plus(),
            vec!["delivery optimization".into(), "peer cache".into(), "bandwidth".into()],
        ),
        curated_policy(
            "curated-exclude-wu-drivers",
            PolicyCategory::Updates,
            "Exclude driver updates from quality update scans",
            "Prevents Windows Update from pulling new drivers during post-deployment servicing.",
            PolicyImpact::Medium,
            true,
            "At least Windows 10",
            vec![reg_dword(
                r"HKLM:\SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate",
                "ExcludeWUDriversInQualityUpdate",
                1,
            )],
            CuratedSupportRule::client_10_plus(),
            vec!["windows update".into(), "driver updates".into(), "drivers".into()],
        ),
        curated_policy(
            "curated-no-auto-reboot-with-logged-on-users",
            PolicyCategory::Updates,
            "Do not auto-restart while a user is signed in",
            "Prevents automatic reboots after update installation when a user session is active.",
            PolicyImpact::Medium,
            true,
            "At least Windows 10",
            vec![
                reg_dword(
                    r"HKLM:\SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate\AU",
                    "NoAutoRebootWithLoggedOnUsers",
                    1,
                ),
                reg_dword(
                    r"HKLM:\SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate\AU",
                    "NoAutoUpdate",
                    0,
                ),
            ],
            CuratedSupportRule::client_10_plus(),
            vec!["auto reboot".into(), "logged on users".into(), "restart".into()],
        ),
        curated_policy(
            "curated-au-notify-download",
            PolicyCategory::Updates,
            "Set Automatic Updates to notify before download",
            "Leaves Windows Update available but changes AU behavior to notify before download and install.",
            PolicyImpact::High,
            true,
            "At least Windows 10",
            vec![
                reg_dword(
                    r"HKLM:\SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate\AU",
                    "NoAutoUpdate",
                    0,
                ),
                reg_dword(
                    r"HKLM:\SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate\AU",
                    "AUOptions",
                    2,
                ),
            ],
            CuratedSupportRule::client_10_plus(),
            vec!["windows update".into(), "notify".into(), "au options".into()],
        ),
        curated_policy(
            "curated-disable-llmnr",
            PolicyCategory::Network,
            "Disable LLMNR",
            "Turns off multicast name resolution to reduce broadcast-based spoofing opportunities.",
            PolicyImpact::Medium,
            true,
            "At least Windows 10",
            vec![reg_dword(
                r"HKLM:\SOFTWARE\Policies\Microsoft\Windows NT\DNSClient",
                "EnableMulticast",
                0,
            )],
            CuratedSupportRule::client_10_plus(),
            vec!["llmnr".into(), "dns".into(), "multicast".into()],
        ),
        curated_policy(
            "curated-require-smb-client-signing",
            PolicyCategory::Network,
            "Require SMB client signing",
            "Requires SMB signing for outbound client connections. Can break very old or misconfigured shares.",
            PolicyImpact::High,
            true,
            "At least Windows 10",
            vec![reg_dword(
                r"HKLM:\SYSTEM\CurrentControlSet\Services\LanmanWorkstation\Parameters",
                "RequireSecuritySignature",
                1,
            )],
            CuratedSupportRule::client_10_plus(),
            vec!["smb".into(), "signing".into(), "lanmanworkstation".into()],
        ),
        curated_policy(
            "curated-require-smb-server-signing",
            PolicyCategory::Network,
            "Require SMB server signing",
            "Requires SMB signing for inbound file sharing traffic on the deployed machine.",
            PolicyImpact::High,
            true,
            "At least Windows 10",
            vec![reg_dword(
                r"HKLM:\SYSTEM\CurrentControlSet\Services\LanmanServer\Parameters",
                "RequireSecuritySignature",
                1,
            )],
            CuratedSupportRule::client_10_plus(),
            vec!["smb".into(), "server signing".into(), "lanmanserver".into()],
        ),
    ]
}

#[derive(Debug, Clone)]
struct CuratedSupportRule {
    min_client_rank: Option<i32>,
    min_server_rank: Option<i32>,
    max_client_rank: Option<i32>,
    max_server_rank: Option<i32>,
    allow_server: bool,
    allow_client: bool,
    allowed_editions: Vec<&'static str>,
}

impl CuratedSupportRule {
    fn client_10_plus() -> Self {
        Self {
            min_client_rank: Some(10_1507),
            min_server_rank: None,
            max_client_rank: None,
            max_server_rank: None,
            allow_server: false,
            allow_client: true,
            allowed_editions: Vec::new(),
        }
    }
}

fn curated_policy(
    id: &'static str,
    category: PolicyCategory,
    display_name: &'static str,
    description: &'static str,
    impact: PolicyImpact,
    starter: bool,
    supported_on: &'static str,
    operations: Vec<RegistryOperation>,
    support_rule: CuratedSupportRule,
    aliases: Vec<String>,
) -> CuratedPolicyDefinition {
    CuratedPolicyDefinition {
        entry: PolicyCatalogEntry {
            id: id.to_string(),
            source_kind: PolicySourceKind::Curated,
            category,
            display_name: display_name.to_string(),
            description: description.to_string(),
            impact,
            starter,
            selectable: true,
            support: PolicySupportStatus {
                supported: true,
                supported_on: Some(supported_on.to_string()),
                reason: "Supported on this host.".to_string(),
            },
            read_only_reason: None,
            aliases,
            category_label: category.label().to_string(),
        },
        operations,
        min_client_rank: support_rule.min_client_rank,
        min_server_rank: support_rule.min_server_rank,
        max_client_rank: support_rule.max_client_rank,
        max_server_rank: support_rule.max_server_rank,
        allow_server: support_rule.allow_server,
        allow_client: support_rule.allow_client,
        allowed_editions: support_rule.allowed_editions,
    }
}

fn reg_dword(key: &str, name: &str, value: i64) -> RegistryOperation {
    RegistryOperation {
        key: key.to_string(),
        name: name.to_string(),
        data: value.to_string(),
        value_type: RegistryValueType::DWord,
    }
}

fn reg_string(key: &str, name: &str, value: &str) -> RegistryOperation {
    RegistryOperation {
        key: key.to_string(),
        name: name.to_string(),
        data: value.to_string(),
        value_type: RegistryValueType::String,
    }
}

fn custom_entry_to_operation(entry: &CustomRegistryEntry) -> Result<RegistryOperation, String> {
    let key = normalize_hklm_path(&entry.key_path)?;
    let value_name = entry.value_name.trim();
    if value_name.is_empty() {
        return Err("Custom registry entries require a value name.".to_string());
    }
    Ok(RegistryOperation {
        key,
        name: value_name.to_string(),
        data: entry.value_data.clone(),
        value_type: entry.value_type.clone(),
    })
}

fn normalize_hklm_path(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("Registry key path is required.".to_string());
    }

    let upper = trimmed.to_ascii_uppercase();
    if upper.starts_with("HKLM:\\") || upper.starts_with("HKLM\\") {
        return Ok(trimmed.replace("HKLM\\", "HKLM:\\"));
    }
    if upper.starts_with("HKEY_LOCAL_MACHINE\\") {
        return Ok(trimmed.replacen("HKEY_LOCAL_MACHINE\\", "HKLM:\\", 1));
    }
    Err(format!(
        "Only HKLM machine-scope registry paths are supported in v1: {}",
        raw
    ))
}

fn detect_policy_host_context() -> Result<PolicyHostContext, String> {
    #[cfg(not(target_os = "windows"))]
    {
        return Ok(PolicyHostContext {
            available: false,
            summary: "Policy inspection requires a Windows host because it reads local PolicyDefinitions.".to_string(),
            product_name: "Unavailable".to_string(),
            edition_id: "Unavailable".to_string(),
            display_version: String::new(),
            build_number: 0,
            installation_type: "non-windows".to_string(),
            architecture: std::env::consts::ARCH.to_string(),
            ui_language: "en-US".to_string(),
            policy_definitions_path: POLICY_DEFINITIONS_DIR.to_string(),
            is_vm: false,
            tpm_spec_version: None,
        });
    }

    #[cfg(target_os = "windows")]
    {
        let raw = detect_windows_host_raw()?;
        let architecture = raw.architecture().to_string();
        let hardware = HardwareDetector::new().detect_all().ok();
        let is_vm = hardware.as_ref().map(|value| value.is_vm).unwrap_or(false);
        let tpm_spec_version = hardware
            .as_ref()
            .and_then(|value| value.tpm.as_ref().map(|tpm| tpm.spec_version.clone()));
        let summary = format!(
            "{} {} {} ({})",
            raw.product_name,
            raw.display_version,
            raw.edition_id,
            raw.architecture()
        );

        Ok(PolicyHostContext {
            available: true,
            summary,
            product_name: raw.product_name,
            edition_id: raw.edition_id,
            display_version: raw.display_version,
            build_number: raw.build_number,
            installation_type: raw.installation_type,
            architecture,
            ui_language: raw.ui_language,
            policy_definitions_path: POLICY_DEFINITIONS_DIR.to_string(),
            is_vm,
            tpm_spec_version,
        })
    }
}

#[cfg(target_os = "windows")]
impl HostDetectionRaw {
    fn architecture(&self) -> &'static str {
        match std::env::consts::ARCH {
            "x86_64" => "x64",
            "aarch64" => "arm64",
            other => other,
        }
    }
}

#[cfg(target_os = "windows")]
fn detect_windows_host_raw() -> Result<HostDetectionRaw, String> {
    let script = r#"
$os = Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion'
[pscustomobject]@{
  ProductName = [string]$os.ProductName
  EditionID = [string]$os.EditionID
  DisplayVersion = [string]($(if ($os.DisplayVersion) { $os.DisplayVersion } else { $os.ReleaseId }))
  CurrentBuild = [string]$os.CurrentBuildNumber
  InstallationType = [string]$os.InstallationType
  UICulture = [string](Get-UICulture).Name
} | ConvertTo-Json -Compress
"#;

    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .output()
        .map_err(|e| format!("Failed to inspect Windows host for policy support: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(format!(
            "Failed to inspect Windows host for policy support: {}",
            stderr
        ));
    }

    #[derive(Deserialize)]
    struct HostPayload {
        #[serde(rename = "ProductName")]
        product_name: String,
        #[serde(rename = "EditionID")]
        edition_id: String,
        #[serde(rename = "DisplayVersion")]
        display_version: String,
        #[serde(rename = "CurrentBuild")]
        current_build: String,
        #[serde(rename = "InstallationType")]
        installation_type: String,
        #[serde(rename = "UICulture")]
        ui_culture: String,
    }

    let payload: HostPayload = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("Failed to parse Windows host policy inspection JSON: {}", e))?;

    Ok(HostDetectionRaw {
        product_name: payload.product_name,
        edition_id: payload.edition_id,
        display_version: payload.display_version,
        build_number: payload
            .current_build
            .trim()
            .parse::<u32>()
            .unwrap_or_default(),
        installation_type: payload.installation_type,
        ui_language: if payload.ui_culture.trim().is_empty() {
            "en-US".to_string()
        } else {
            payload.ui_culture
        },
    })
}

fn iter_admx_paths(policy_definitions_dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut paths = fs::read_dir(policy_definitions_dir)
        .map_err(|e| {
            format!(
                "Failed to read policy definitions directory {}: {}",
                policy_definitions_dir.display(),
                e
            )
        })?
        .filter_map(|entry| entry.ok().map(|value| value.path()))
        .filter(|path| {
            path.extension()
                .and_then(|value| value.to_str())
                .map(|value| value.eq_ignore_ascii_case("admx"))
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    paths.sort();
    Ok(paths)
}

fn collect_admx_file_metadata(
    policy_definitions_dir: &Path,
    ui_language: &str,
) -> Result<HashMap<String, AdmxFileMetadata>, String> {
    let mut metadata = HashMap::new();
    for admx_path in iter_admx_paths(policy_definitions_dir)? {
        let file_stem = admx_path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_string();
        let strings = load_adml_strings(policy_definitions_dir, &file_stem, ui_language)?;
        metadata.insert(file_stem, parse_admx_metadata(&admx_path, &strings)?);
    }
    Ok(metadata)
}

fn load_adml_strings(
    policy_definitions_dir: &Path,
    file_stem: &str,
    ui_language: &str,
) -> Result<HashMap<String, String>, String> {
    let language_candidates = [ui_language, "en-US"];
    let mut last_error = None;
    for language in language_candidates {
        let adml_path = policy_definitions_dir
            .join(language)
            .join(format!("{}.adml", file_stem));
        if !adml_path.is_file() {
            continue;
        }

        match read_xml_document(&adml_path) {
            Ok(content) => return parse_adml_strings(&content),
            Err(err) => {
                last_error = Some(err);
            }
        }
    }

    if let Some(error) = last_error {
        return Err(error);
    }

    Ok(HashMap::new())
}

fn parse_adml_strings(content: &str) -> Result<HashMap<String, String>, String> {
    let mut reader = Reader::from_str(content);
    reader.trim_text(false);
    let mut buf = Vec::new();
    let mut current_id: Option<String> = None;
    let mut strings = HashMap::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(event)) if local_name(&event) == "string" => {
                current_id = attr_value(&event, b"id");
            }
            Ok(Event::Text(text)) => {
                if let Some(id) = current_id.as_ref() {
                    let value = text
                        .unescape()
                        .map_err(|e| format!("Failed to decode ADML text: {}", e))?
                        .trim()
                        .to_string();
                    strings.insert(id.clone(), value);
                }
            }
            Ok(Event::End(event)) if local_end_name(&event) == "string" => {
                current_id = None;
            }
            Ok(Event::Eof) => break,
            Err(err) => {
                return Err(format!("Failed to parse ADML strings: {}", err));
            }
            _ => {}
        }
        buf.clear();
    }

    Ok(strings)
}

fn parse_admx_metadata(
    admx_path: &Path,
    strings: &HashMap<String, String>,
) -> Result<AdmxFileMetadata, String> {
    let content = read_xml_document(admx_path)?;
    let mut reader = Reader::from_str(&content);
    reader.trim_text(true);
    let mut buf = Vec::new();
    let mut target_prefix = None;
    let mut categories = HashMap::new();
    let mut supported_definitions = HashMap::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Empty(event)) | Ok(Event::Start(event)) => {
                let name = local_name(&event);
                match name.as_str() {
                    "target" => {
                        target_prefix = attr_value(&event, b"prefix");
                    }
                    "category" => {
                        if let Some(category_name) = attr_value(&event, b"name") {
                            let display_name = attr_value(&event, b"displayName")
                                .map(|value| resolve_resource_reference(&value, strings))
                                .unwrap_or_else(|| category_name.clone());
                            categories.insert(category_name.clone(), display_name.clone());
                            if let Some(prefix) = target_prefix.as_ref() {
                                categories
                                    .insert(format!("{}:{}", prefix, category_name), display_name);
                            }
                        }
                    }
                    "definition" => {
                        if let Some(definition_name) = attr_value(&event, b"name") {
                            let display_name = attr_value(&event, b"displayName")
                                .map(|value| resolve_resource_reference(&value, strings))
                                .unwrap_or_else(|| definition_name.clone());
                            supported_definitions
                                .insert(definition_name.clone(), display_name.clone());
                            if let Some(prefix) = target_prefix.as_ref() {
                                supported_definitions.insert(
                                    format!("{}:{}", prefix, definition_name),
                                    display_name,
                                );
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(err) => {
                return Err(format!(
                    "Failed to parse ADMX metadata from {}: {}",
                    admx_path.display(),
                    err
                ));
            }
            _ => {}
        }
        buf.clear();
    }

    Ok(AdmxFileMetadata {
        target_prefix,
        categories,
        supported_definitions,
    })
}

fn parse_admx_policies(
    admx_path: &Path,
    strings: &HashMap<String, String>,
    _target_prefix: Option<&str>,
) -> Result<Vec<PartialPolicy>, String> {
    let content = read_xml_document(admx_path)?;
    let mut reader = Reader::from_str(&content);
    reader.trim_text(true);
    let mut buf = Vec::new();
    let mut policies = Vec::new();
    let mut current_policy: Option<PartialPolicy> = None;
    let mut current_section: Option<RegistryCaptureSection> = None;
    let mut current_list_item: Option<PendingListItem> = None;
    let mut current_value: Option<RegistryOperation> = None;
    let mut expecting_string_value = false;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(event)) | Ok(Event::Empty(event)) => {
                let name = local_name(&event);
                match name.as_str() {
                    "policy" => {
                        current_policy = Some(PartialPolicy {
                            name: attr_value(&event, b"name").unwrap_or_default(),
                            class_name: attr_value(&event, b"class")
                                .unwrap_or_else(|| "Machine".to_string()),
                            display_name: attr_value(&event, b"displayName")
                                .map(|value| resolve_resource_reference(&value, strings))
                                .unwrap_or_default(),
                            explain_text: attr_value(&event, b"explainText")
                                .map(|value| resolve_resource_reference(&value, strings))
                                .unwrap_or_default(),
                            key: attr_value(&event, b"key"),
                            value_name: attr_value(&event, b"valueName"),
                            parent_category_ref: None,
                            supported_on_ref: None,
                            enabled_operations: Vec::new(),
                            has_enabled_config: false,
                        });
                    }
                    "parentCategory" => {
                        if let Some(policy) = current_policy.as_mut() {
                            policy.parent_category_ref = attr_value(&event, b"ref");
                        }
                    }
                    "supportedOn" => {
                        if let Some(policy) = current_policy.as_mut() {
                            policy.supported_on_ref = attr_value(&event, b"ref");
                        }
                    }
                    "enabledValue" => current_section = Some(RegistryCaptureSection::EnabledValue),
                    "enabledList" => current_section = Some(RegistryCaptureSection::EnabledList),
                    "item" => {
                        if matches!(current_section, Some(RegistryCaptureSection::EnabledList)) {
                            current_list_item = Some(PendingListItem {
                                key: attr_value(&event, b"key").unwrap_or_default(),
                                value_name: attr_value(&event, b"valueName").unwrap_or_default(),
                            });
                        }
                    }
                    "decimal" => {
                        current_value = capture_numeric_value(
                            &event,
                            current_policy.as_ref(),
                            current_list_item.as_ref(),
                            RegistryValueType::DWord,
                        );
                    }
                    "longDecimal" => {
                        current_value = capture_numeric_value(
                            &event,
                            current_policy.as_ref(),
                            current_list_item.as_ref(),
                            RegistryValueType::QWord,
                        );
                    }
                    "string" => {
                        expecting_string_value = true;
                        current_value = capture_string_container(
                            current_policy.as_ref(),
                            current_list_item.as_ref(),
                        );
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(text)) => {
                if expecting_string_value {
                    if let Some(operation) = current_value.as_mut() {
                        operation.data = text
                            .unescape()
                            .map_err(|e| format!("Failed to decode ADMX string value: {}", e))?
                            .to_string();
                    }
                }
            }
            Ok(Event::End(event)) => match local_end_name(&event).as_str() {
                "policy" => {
                    if let Some(policy) = current_policy.take() {
                        policies.push(policy);
                    }
                }
                "enabledValue" | "enabledList" => {
                    current_section = None;
                }
                "item" => {
                    current_list_item = None;
                }
                "decimal" | "longDecimal" | "string" => {
                    if let Some(operation) = current_value.take() {
                        if let Some(policy) = current_policy.as_mut() {
                            policy.has_enabled_config = true;
                            if !operation.key.is_empty() && !operation.name.is_empty() {
                                policy.enabled_operations.push(operation);
                            }
                        }
                    }
                    expecting_string_value = false;
                }
                _ => {}
            },
            Ok(Event::Eof) => break,
            Err(err) => {
                return Err(format!(
                    "Failed to parse ADMX policies from {}: {}",
                    admx_path.display(),
                    err
                ));
            }
            _ => {}
        }
        buf.clear();
    }

    Ok(policies)
}

fn read_xml_document(path: &Path) -> Result<String, String> {
    let bytes =
        fs::read(path).map_err(|e| format!("Failed to read XML file {}: {}", path.display(), e))?;
    decode_xml_bytes(&bytes)
        .map_err(|e| format!("Failed to decode XML file {}: {}", path.display(), e))
}

fn decode_xml_bytes(bytes: &[u8]) -> Result<String, String> {
    if bytes.starts_with(&[0xFF, 0xFE]) {
        return decode_utf16_bytes(&bytes[2..], true);
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        return decode_utf16_bytes(&bytes[2..], false);
    }
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8(bytes[3..].to_vec())
            .map_err(|e| format!("UTF-8 BOM decode failed: {}", e));
    }

    if let Ok(text) = String::from_utf8(bytes.to_vec()) {
        return Ok(text);
    }

    if bytes.len() % 2 == 0 {
        return decode_utf16_bytes(bytes, true);
    }

    Err("Unsupported XML encoding".to_string())
}

fn decode_utf16_bytes(bytes: &[u8], little_endian: bool) -> Result<String, String> {
    if bytes.len() % 2 != 0 {
        return Err("UTF-16 payload length was not even".to_string());
    }

    let words = bytes
        .chunks_exact(2)
        .map(|chunk| {
            if little_endian {
                u16::from_le_bytes([chunk[0], chunk[1]])
            } else {
                u16::from_be_bytes([chunk[0], chunk[1]])
            }
        })
        .collect::<Vec<_>>();

    String::from_utf16(&words).map_err(|e| format!("UTF-16 decode failed: {}", e))
}

fn capture_numeric_value(
    event: &BytesStart<'_>,
    current_policy: Option<&PartialPolicy>,
    current_list_item: Option<&PendingListItem>,
    value_type: RegistryValueType,
) -> Option<RegistryOperation> {
    let data = attr_value(event, b"value")?;
    build_operation_for_capture(current_policy, current_list_item, value_type, data)
}

fn capture_string_container(
    current_policy: Option<&PartialPolicy>,
    current_list_item: Option<&PendingListItem>,
) -> Option<RegistryOperation> {
    build_operation_for_capture(
        current_policy,
        current_list_item,
        RegistryValueType::String,
        String::new(),
    )
}

fn build_operation_for_capture(
    current_policy: Option<&PartialPolicy>,
    current_list_item: Option<&PendingListItem>,
    value_type: RegistryValueType,
    data: String,
) -> Option<RegistryOperation> {
    if let Some(item) = current_list_item {
        return Some(RegistryOperation {
            key: normalize_policy_key_path(&item.key),
            name: item.value_name.clone(),
            data,
            value_type,
        });
    }

    let policy = current_policy?;
    Some(RegistryOperation {
        key: normalize_policy_key_path(policy.key.as_deref().unwrap_or_default()),
        name: policy.value_name.clone().unwrap_or_default(),
        data,
        value_type,
    })
}

fn normalize_policy_key_path(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.to_ascii_uppercase().starts_with("HKLM:") {
        return trimmed.to_string();
    }
    format!("HKLM:\\{}", trimmed.trim_start_matches('\\'))
}

fn local_name(event: &BytesStart<'_>) -> String {
    String::from_utf8_lossy(event.local_name().as_ref()).to_string()
}

fn local_end_name(event: &quick_xml::events::BytesEnd<'_>) -> String {
    String::from_utf8_lossy(event.local_name().as_ref()).to_string()
}

fn attr_value(event: &BytesStart<'_>, key: &[u8]) -> Option<String> {
    event
        .attributes()
        .flatten()
        .find(|attribute| attribute.key.as_ref() == key)
        .and_then(|attribute| String::from_utf8(attribute.value.into_owned()).ok())
}

fn resolve_resource_reference(raw: &str, strings: &HashMap<String, String>) -> String {
    let trimmed = raw.trim();
    if let Some(id) = trimmed
        .strip_prefix("$(string.")
        .and_then(|value| value.strip_suffix(')'))
    {
        return strings.get(id).cloned().unwrap_or_else(|| id.to_string());
    }
    trimmed.to_string()
}

fn resolve_lookup_value(map: &HashMap<String, String>, raw_ref: &str) -> Option<String> {
    let trimmed = raw_ref.trim();
    if trimmed.is_empty() {
        return None;
    }

    map.get(trimmed).cloned().or_else(|| {
        trimmed
            .split(':')
            .next_back()
            .and_then(|value| map.get(value).cloned())
    })
}

fn infer_policy_category(
    display_name: &str,
    description: &str,
    category_label: Option<&str>,
) -> PolicyCategory {
    let haystack = format!(
        "{} {} {}",
        display_name.to_ascii_lowercase(),
        description.to_ascii_lowercase(),
        category_label.unwrap_or_default().to_ascii_lowercase()
    );

    if contains_any(
        &haystack,
        &[
            "windows update",
            "quality update",
            "feature update",
            "wu",
            "delivery optimization",
            "microsoft update",
            "automatic updates",
            "wsus",
            "update compliance",
        ],
    ) {
        return PolicyCategory::Updates;
    }

    if contains_any(
        &haystack,
        &[
            "telemetry",
            "diagnostic",
            "advertising",
            "privacy",
            "tailored experiences",
            "consumer features",
            "web search",
            "cloud content",
            "feedback",
            "data collection",
        ],
    ) {
        return PolicyCategory::Privacy;
    }

    if contains_any(
        &haystack,
        &[
            "network",
            "dns",
            "tcp",
            "smb",
            "lanman",
            "winsock",
            "proxy",
            "internet communication",
            "wireless",
            "802.1x",
        ],
    ) {
        return PolicyCategory::Network;
    }

    if contains_any(
        &haystack,
        &[
            "performance",
            "search index",
            "indexing",
            "graphics",
            "gpu",
            "memory",
            "power",
            "visual effects",
            "game",
            "background apps",
        ],
    ) {
        return PolicyCategory::Performance;
    }

    if contains_any(
        &haystack,
        &[
            "security",
            "defender",
            "smartscreen",
            "firewall",
            "credential",
            "app control",
            "uac",
            "bitlocker",
            "lsa",
            "exploit",
        ],
    ) {
        return PolicyCategory::Security;
    }

    PolicyCategory::Security
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

fn evaluate_curated_support(
    host: &PolicyHostContext,
    curated: &CuratedPolicyDefinition,
) -> SupportEvaluation {
    if !host.available {
        return SupportEvaluation {
            supported: false,
            reason: host.summary.clone(),
        };
    }

    let is_server = host
        .installation_type
        .to_ascii_lowercase()
        .contains("server")
        || host.product_name.to_ascii_lowercase().contains("server");
    let host_edition = normalize_edition(&host.edition_id);
    let host_rank = host_rank(host);

    if is_server && !curated.allow_server {
        return SupportEvaluation {
            supported: false,
            reason: "This policy is only available for client Windows editions in v1.".to_string(),
        };
    }

    if !is_server && !curated.allow_client {
        return SupportEvaluation {
            supported: false,
            reason: "This policy is only available for Windows Server builds in v1.".to_string(),
        };
    }

    if !curated.allowed_editions.is_empty() {
        let normalized_allowed = curated
            .allowed_editions
            .iter()
            .map(|value| normalize_edition(value))
            .collect::<Vec<_>>();
        if !normalized_allowed
            .iter()
            .any(|value| value == &host_edition)
        {
            return SupportEvaluation {
                supported: false,
                reason: format!(
                    "This policy is limited to these editions: {}.",
                    curated.allowed_editions.join(", ")
                ),
            };
        }
    }

    let min_rank = if is_server {
        curated.min_server_rank
    } else {
        curated.min_client_rank
    };
    let max_rank = if is_server {
        curated.max_server_rank
    } else {
        curated.max_client_rank
    };

    if let Some(min_rank) = min_rank {
        if host_rank < min_rank {
            return SupportEvaluation {
                supported: false,
                reason: format!(
                    "This policy requires a newer Windows build on the build host. Current host: {}.",
                    host.summary
                ),
            };
        }
    }

    if let Some(max_rank) = max_rank {
        if host_rank > max_rank {
            return SupportEvaluation {
                supported: false,
                reason: format!(
                    "This policy targets older Windows builds than the current host: {}.",
                    host.summary
                ),
            };
        }
    }

    SupportEvaluation {
        supported: true,
        reason: "Supported on this host.".to_string(),
    }
}

fn evaluate_admx_support(
    host: &PolicyHostContext,
    supported_ref: Option<&str>,
    supported_display_name: Option<&str>,
) -> SupportEvaluation {
    if !host.available {
        return SupportEvaluation {
            supported: false,
            reason: host.summary.clone(),
        };
    }

    let support_text = supported_display_name
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            supported_ref
                .map(str::trim)
                .filter(|value| !value.is_empty())
        })
        .unwrap_or("");

    if support_text.is_empty() {
        return SupportEvaluation {
            supported: true,
            reason: "Supported on this host.".to_string(),
        };
    }

    let normalized = support_text.to_ascii_lowercase();
    let is_server = host
        .installation_type
        .to_ascii_lowercase()
        .contains("server")
        || host.product_name.to_ascii_lowercase().contains("server");
    let mentions_server = normalized.contains("server");
    let mentions_client = contains_any(
        &normalized,
        &[
            "windows 11",
            "windows11",
            "windows 10",
            "windows10",
            "windows 8.1",
            "windows81",
            "windows 8",
            "windows8",
            "windows 7",
            "windows7",
            "client",
        ],
    );

    if is_server && mentions_client && !mentions_server {
        return SupportEvaluation {
            supported: false,
            reason: format!(
                "This policy targets client Windows releases, not {}.",
                host.summary
            ),
        };
    }

    if !is_server && mentions_server && !mentions_client {
        return SupportEvaluation {
            supported: false,
            reason: format!(
                "This policy targets Windows Server releases, not {}.",
                host.summary
            ),
        };
    }

    let edition_restrictions = extract_edition_restrictions(support_text);
    if !edition_restrictions.is_empty() {
        let host_edition = normalize_edition(&host.edition_id);
        if !edition_restrictions
            .iter()
            .any(|value| value == &host_edition)
        {
            return SupportEvaluation {
                supported: false,
                reason: format!(
                    "This policy is limited to these editions: {}.",
                    edition_restrictions.join(", ")
                ),
            };
        }
    }

    let min_rank = infer_min_rank(support_text, is_server);
    if let Some(min_rank) = min_rank {
        if host_rank(host) < min_rank {
            return SupportEvaluation {
                supported: false,
                reason: format!(
                    "The current host ({}) is older than the minimum supported target for this policy.",
                    host.summary
                ),
            };
        }
    }

    let max_rank = infer_max_rank(support_text, is_server);
    if let Some(max_rank) = max_rank {
        if host_rank(host) > max_rank {
            return SupportEvaluation {
                supported: false,
                reason: format!(
                    "The current host ({}) is newer than the supported range described by this policy.",
                    host.summary
                ),
            };
        }
    }

    SupportEvaluation {
        supported: true,
        reason: "Supported on this host.".to_string(),
    }
}

fn normalize_edition(raw: &str) -> String {
    let normalized = raw
        .trim()
        .to_ascii_lowercase()
        .replace(" edition", "")
        .replace("windows ", "")
        .replace(' ', "");

    if normalized.contains("professional") || normalized == "pro" {
        return "pro".to_string();
    }
    if normalized.contains("enterprise") {
        return "enterprise".to_string();
    }
    if normalized.contains("education") {
        return "education".to_string();
    }
    if normalized.contains("home") || normalized == "core" {
        return "home".to_string();
    }
    if normalized.contains("iot") {
        return "iot".to_string();
    }
    if normalized.contains("datacenter") {
        return "server-datacenter".to_string();
    }
    if normalized.contains("standard") {
        return "server-standard".to_string();
    }

    normalized
}

fn extract_edition_restrictions(text: &str) -> Vec<String> {
    let normalized = text.to_ascii_lowercase();
    let mut editions = Vec::new();

    for (needle, edition) in [
        ("professional", "pro"),
        (" pro", "pro"),
        ("pro,", "pro"),
        (" enterprise", "enterprise"),
        ("education", "education"),
        ("home", "home"),
        ("core", "home"),
        ("iot", "iot"),
    ] {
        if normalized.contains(needle) && !editions.iter().any(|value| value == edition) {
            editions.push(edition.to_string());
        }
    }

    editions
}

fn host_rank(host: &PolicyHostContext) -> i32 {
    let lower_product = host.product_name.to_ascii_lowercase();
    let display = host.display_version.to_ascii_lowercase();
    let is_server = host
        .installation_type
        .to_ascii_lowercase()
        .contains("server")
        || lower_product.contains("server");

    if is_server {
        if lower_product.contains("2025") || host.build_number >= 26_100 {
            return 2025;
        }
        if lower_product.contains("2022") || host.build_number >= 20_348 {
            return 2022;
        }
        if lower_product.contains("2019") || host.build_number >= 17_763 {
            return 2019;
        }
        if lower_product.contains("2016") || host.build_number >= 14_393 {
            return 2016;
        }
        if lower_product.contains("2012 r2") || host.build_number >= 9_600 {
            return 2013;
        }
        if lower_product.contains("2012") || host.build_number >= 9_200 {
            return 2012;
        }
        return host.build_number as i32;
    }

    if lower_product.contains("windows 11") || host.build_number >= 22_000 {
        if let Some(release_rank) = infer_rank_from_text(&display, false) {
            return release_rank;
        }
        if host.build_number >= 26_100 {
            return 11_2402;
        }
        if host.build_number >= 22_631 {
            return 11_2302;
        }
        if host.build_number >= 22_621 {
            return 11_2202;
        }
        return 11_2102;
    }

    if let Some(release_rank) = infer_rank_from_text(&display, false) {
        return release_rank.max(10_1507);
    }

    match host.build_number {
        build if build >= 19_045 => 10_2202,
        build if build >= 19_044 => 10_2102,
        build if build >= 19_043 => 10_2101,
        build if build >= 19_042 => 10_2002,
        build if build >= 19_041 => 10_2004,
        build if build >= 18_363 => 10_1909,
        build if build >= 18_362 => 10_1903,
        build if build >= 17_763 => 10_1809,
        build if build >= 17_134 => 10_1803,
        build if build >= 16_299 => 10_1709,
        build if build >= 15_063 => 10_1703,
        build if build >= 14_393 => 10_1607,
        build if build >= 10_586 => 10_1511,
        build if build >= 10_240 => 10_1507,
        _ => host.build_number as i32,
    }
}

fn infer_min_rank(text: &str, is_server: bool) -> Option<i32> {
    let ranks = if is_server {
        extract_server_ranks(text)
    } else {
        extract_client_ranks(text)
    };
    ranks.into_iter().min()
}

fn infer_max_rank(text: &str, is_server: bool) -> Option<i32> {
    let lower = text.to_ascii_lowercase();
    let looks_like_range = contains_any(
        &lower,
        &[
            " to ",
            "through",
            " up to ",
            "until ",
            "and earlier",
            "or earlier",
        ],
    ) || lower.contains("to")
        || lower.contains("through")
        || lower.contains("upto");

    if !looks_like_range {
        return None;
    }

    let ranks = if is_server {
        extract_server_ranks(text)
    } else {
        extract_client_ranks(text)
    };
    ranks.into_iter().max()
}

fn infer_rank_from_text(text: &str, is_server: bool) -> Option<i32> {
    if is_server {
        extract_server_ranks(text).into_iter().max()
    } else {
        extract_client_ranks(text).into_iter().max()
    }
}

fn extract_client_ranks(text: &str) -> Vec<i32> {
    let sanitized = text
        .to_ascii_lowercase()
        .chars()
        .filter(|value| value.is_ascii_alphanumeric())
        .collect::<String>();
    let mut ranks = Vec::new();

    for (marker, rank) in [
        ("24h2", 2402),
        ("23h2", 2302),
        ("22h2", 2202),
        ("22h1", 2201),
        ("21h2", 2102),
        ("21h1", 2101),
        ("20h2", 2002),
        ("2004", 2004),
        ("1909", 1909),
        ("1903", 1903),
        ("1809", 1809),
        ("1803", 1803),
        ("1709", 1709),
        ("1703", 1703),
        ("1607", 1607),
        ("1511", 1511),
        ("1507", 1507),
    ] {
        if sanitized.contains(&format!("windows11version{}", marker))
            || sanitized.contains(&format!("windows11{}", marker))
            || sanitized.contains(&format!("11{}", marker))
        {
            ranks.push(11 * 10_000 + rank);
        }
        if sanitized.contains(&format!("windows10version{}", marker))
            || sanitized.contains(&format!("windows10{}", marker))
            || sanitized.contains(&format!("10{}", marker))
        {
            ranks.push(10 * 10_000 + rank);
        }
    }

    if sanitized.contains("windows11") && !ranks.iter().any(|value| *value >= 11 * 10_000) {
        ranks.push(11 * 10_000);
    }
    if sanitized.contains("windows10") && !ranks.iter().any(|value| *value / 10_000 == 10) {
        ranks.push(10 * 10_000);
    }
    if sanitized.contains("windows81") {
        ranks.push(81_000);
    }
    if sanitized.contains("windows8") && !sanitized.contains("windows81") {
        ranks.push(80_000);
    }
    if sanitized.contains("windows7") {
        ranks.push(70_000);
    }

    ranks.sort_unstable();
    ranks.dedup();
    ranks
}

fn extract_server_ranks(text: &str) -> Vec<i32> {
    let sanitized = text
        .to_ascii_lowercase()
        .chars()
        .filter(|value| value.is_ascii_alphanumeric())
        .collect::<String>();
    let mut ranks = Vec::new();

    for (marker, rank) in [
        ("windowsserver2025", 2025),
        ("server2025", 2025),
        ("windowsserver2022", 2022),
        ("server2022", 2022),
        ("windowsserver2019", 2019),
        ("server2019", 2019),
        ("windowsserver2016", 2016),
        ("server2016", 2016),
        ("windowsserver2012r2", 2013),
        ("server2012r2", 2013),
        ("windowsserver2012", 2012),
        ("server2012", 2012),
    ] {
        if sanitized.contains(marker) {
            ranks.push(rank);
        }
    }

    ranks.sort_unstable();
    ranks.dedup();
    ranks
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host(
        product_name: &str,
        edition_id: &str,
        display_version: &str,
        build_number: u32,
        installation_type: &str,
    ) -> PolicyHostContext {
        PolicyHostContext {
            available: true,
            summary: format!(
                "{} {} {} ({})",
                product_name, display_version, build_number, installation_type
            ),
            product_name: product_name.to_string(),
            edition_id: edition_id.to_string(),
            display_version: display_version.to_string(),
            build_number,
            installation_type: installation_type.to_string(),
            architecture: "x64".to_string(),
            ui_language: "en-US".to_string(),
            policy_definitions_path: POLICY_DEFINITIONS_DIR.to_string(),
            is_vm: false,
            tpm_spec_version: Some("2.0".to_string()),
        }
    }

    #[test]
    fn normalizes_hklm_paths() {
        assert_eq!(
            normalize_hklm_path(r"HKEY_LOCAL_MACHINE\SOFTWARE\Policies\Test").unwrap(),
            r"HKLM:\SOFTWARE\Policies\Test"
        );
        assert!(normalize_hklm_path(r"HKCU:\Software\Test").is_err());
    }

    #[test]
    fn serializes_and_loads_saved_presets() {
        let presets = vec![PolicyPreset {
            id: "custom-1".to_string(),
            name: "Custom".to_string(),
            built_in: false,
            selected_policy_ids: vec!["curated-disable-telemetry".to_string()],
            custom_registry_entries: vec![CustomRegistryEntry {
                id: "entry-1".to_string(),
                key_path: r"HKLM:\SOFTWARE\Policies\Test".to_string(),
                value_name: "Enabled".to_string(),
                value_type: RegistryValueType::DWord,
                value_data: "1".to_string(),
            }],
        }];

        let raw = serialize_saved_policy_presets(&presets).unwrap();
        let restored = load_saved_policy_presets_from_json(Some(&raw)).unwrap();
        assert_eq!(restored, presets);
    }

    #[test]
    fn infers_category_from_text() {
        let category = infer_policy_category(
            "Disable web search in Start",
            "Stops cloud search suggestions and telemetry-backed results.",
            Some("Search"),
        );
        assert_eq!(category, PolicyCategory::Privacy);
    }

    #[test]
    fn evaluates_curated_client_support() {
        let curated = curated_policy_definitions()
            .into_iter()
            .find(|entry| entry.entry.id == "curated-disable-telemetry")
            .unwrap();
        let supported = evaluate_curated_support(
            &host("Windows 11 Pro", "Professional", "24H2", 26100, "Client"),
            &curated,
        );
        assert!(supported.supported);

        let unsupported = evaluate_curated_support(
            &host("Windows Server 2022", "ServerStandard", "", 20348, "Server"),
            &curated,
        );
        assert!(!unsupported.supported);
    }

    #[test]
    fn evaluates_admx_support_for_minimum_release() {
        let old_host = host("Windows 10 Pro", "Professional", "22H2", 19045, "Client");
        let new_host = host("Windows 11 Pro", "Professional", "24H2", 26100, "Client");

        let old_result = evaluate_admx_support(
            &old_host,
            Some("SUPPORTED_Windows_11_0_24H2"),
            Some("At least Windows 11, version 24H2"),
        );
        let new_result = evaluate_admx_support(
            &new_host,
            Some("SUPPORTED_Windows_11_0_24H2"),
            Some("At least Windows 11, version 24H2"),
        );

        assert!(!old_result.supported);
        assert!(new_result.supported);
    }

    #[test]
    fn infers_version_ranges_from_supported_text() {
        assert_eq!(
            infer_min_rank("Windows 7 to Windows 11 22H2", false),
            Some(70_000)
        );
        assert_eq!(
            infer_max_rank("Windows 7 to Windows 11 22H2", false),
            Some(11_2202)
        );
        assert_eq!(
            infer_min_rank("Windows Server 2016 through Windows Server 2022", true),
            Some(2016)
        );
        assert_eq!(
            infer_max_rank("Windows Server 2016 through Windows Server 2022", true),
            Some(2022)
        );
    }
}
