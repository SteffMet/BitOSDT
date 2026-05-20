fn main() {
    let channel = read_release_channel().unwrap_or("release".to_string());
    println!("cargo:rustc-env=BITOSDT_RELEASE_CHANNEL={channel}");
    println!("cargo:rerun-if-changed=Cargo.toml");

    #[cfg(target_os = "windows")]
    {
        println!("cargo:rerun-if-changed=bitosdt.manifest");

        let windows =
            tauri_build::WindowsAttributes::new().app_manifest(include_str!("bitosdt.manifest"));
        let attrs = tauri_build::Attributes::new().windows_attributes(windows);
        tauri_build::try_build(attrs).expect("failed to run tauri build script");
    }

    #[cfg(not(target_os = "windows"))]
    tauri_build::build();
}

fn read_release_channel() -> Option<String> {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").ok()?;
    let cargo_toml =
        std::fs::read_to_string(std::path::Path::new(&manifest_dir).join("Cargo.toml")).ok()?;
    let mut in_section = false;

    for line in cargo_toml.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_section = trimmed == "[package.metadata.bitosdt]";
            continue;
        }

        if !in_section || !trimmed.starts_with("release_channel") {
            continue;
        }

        let (_, value) = trimmed.split_once('=')?;
        let channel = value.trim().trim_matches('"').to_ascii_lowercase();
        if channel == "release" || channel == "experimental" {
            return Some(channel);
        }
    }

    None
}
