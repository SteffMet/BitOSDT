pub mod full_iso_builder;
pub mod image_preparer;
pub mod iso_creator;
pub mod lightweight_builder;
pub mod lightweight_host;
pub mod linux_support;
pub mod provisioning_ui;
pub mod publish;
pub mod runtime_drivers;
pub mod shell_layout;
pub mod startnet;
pub mod usb_writer;
pub mod winpe_builder;
pub mod winpe_ui;
pub mod workflow;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeDomainJoinConfig {
    pub enabled: bool,
    pub prompt_for_credentials_at_runtime: bool,
    pub default_domain: Option<String>,
    pub default_ou_path: Option<String>,
}

pub use full_iso_builder::{
    build_full_iso, DiskSelectionPolicy, FullIsoBuildConfig, FullIsoBuildResult, FullIsoProgress,
};
pub use image_preparer::{FileInjection, ImagePrepConfig, ImagePreparer};
pub use iso_creator::IsoCreator;
pub use lightweight_builder::{LightweightBuilder, LightweightConfig};
pub use lightweight_host::{
    default_lightweight_bind_address, resolve_default_lightweight_host_settings,
    serve_lightweight_tree,
};
pub use linux_support::{
    default_winpe_assets_path, ensure_linux_build_prerequisites, runtime_executable_from_assets,
    sync_winpe_asset_bundle, WinpeAssetBundle,
};
pub use provisioning_ui::{generate_provisioning_hta, generate_provisioning_kiosk_helper_ps1};
pub use publish::{stage_lightweight_media_tree, PublishResult};
pub use runtime_drivers::{stage_runtime_driver_assets, RuntimeDriverAssetConfig};
pub use shell_layout::{
    empty_shell_layout_value, generate_shell_layout_script, ShellLayoutConfig, ShellLayoutItem,
};
pub use startnet::{StartnetConfig, StartnetGenerator};
pub use usb_writer::{UsbDevice, UsbWriter};
pub use winpe_builder::WinPEBuilder;
pub use winpe_ui::{WinPEStatus, WinPEUiMode};
pub use workflow::{
    assess_sign_in_readiness, build_full_iso_with_context, build_image_with_context,
    build_lightweight_iso_with_context, build_manifest_json, build_task_sequence,
    build_unattend_config, default_simple_publish_path, default_simple_runtime_url,
    persist_built_image, prepare_full_build_source, resolve_cloud_catalog_entry,
    resolve_delivery_mode, validate_driver_paths, validate_driver_paths_with_network,
    validate_full_iso_remote_sources, validate_sign_in_readiness, BuildProgress, DeliveryModeKind,
    ImageBuildContext, ImageBuildRequest, ResolvedBuildSource, SignInReadiness,
    SignInReadinessLevel,
};
