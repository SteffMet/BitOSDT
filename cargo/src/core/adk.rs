use std::path::{Path, PathBuf};
#[cfg(target_os = "windows")]
use std::process::Command;

#[derive(Debug, Clone)]
pub struct AdkPaths {
    pub root: PathBuf,
    pub deployment_tools: PathBuf,
    pub winpe_root: PathBuf,
    pub winpe_ocs: PathBuf,
    pub copype_cmd: PathBuf,
    pub oscdimg_exe: PathBuf,
    pub dism_exe: PathBuf,
}

#[cfg(any(test, target_os = "windows"))]
pub(crate) fn normalize_arch(arch: &str) -> String {
    match arch.to_lowercase().as_str() {
        "x64" | "amd64" | "x86_64" => "amd64".to_string(),
        "x86" | "i686" | "i386" => "x86".to_string(),
        "arm64" | "aarch64" => "arm64".to_string(),
        other => other.to_string(),
    }
}

#[cfg(any(test, target_os = "windows"))]
pub(crate) fn is_supported_copype_arch(arch: &str) -> bool {
    matches!(
        normalize_arch(arch).as_str(),
        "amd64" | "x86" | "arm" | "arm64"
    )
}

#[cfg(any(test, target_os = "windows"))]
fn find_adk_root_from_path(path: &Path) -> Option<PathBuf> {
    let mut current = path;
    loop {
        if current
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.eq_ignore_ascii_case("Assessment and Deployment Kit"))
            .unwrap_or(false)
        {
            return Some(current.to_path_buf());
        }
        match current.parent() {
            Some(parent) => current = parent,
            None => break,
        }
    }
    None
}

#[cfg(any(test, target_os = "windows"))]
fn normalize_adk_root(path: &Path) -> PathBuf {
    if let Some(root) = find_adk_root_from_path(path) {
        return root;
    }

    if path
        .file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.eq_ignore_ascii_case("10"))
        .unwrap_or(false)
    {
        return path.join("Assessment and Deployment Kit");
    }

    path.to_path_buf()
}

#[cfg(target_os = "windows")]
fn pick_existing(candidates: &[PathBuf]) -> PathBuf {
    candidates
        .iter()
        .find(|p| p.exists())
        .cloned()
        .unwrap_or_else(|| candidates[0].clone())
}

#[cfg(target_os = "windows")]
pub fn candidate_copype_paths(root: &Path, arch: &str) -> Vec<PathBuf> {
    let arch = normalize_arch(arch);
    let deployment_tools_root = root.join("Deployment Tools");
    let winpe_root = root.join("Windows Preinstallation Environment");
    vec![
        // Older ADK layout: under Deployment Tools
        deployment_tools_root.join(&arch).join("copype.cmd"),
        deployment_tools_root.join("copype.cmd"),
        deployment_tools_root.join(&arch).join("copype.ps1"),
        deployment_tools_root.join("copype.ps1"),
        // Newer ADK layout: under Windows Preinstallation Environment
        winpe_root.join("copype.cmd"),
        winpe_root.join(&arch).join("copype.cmd"),
        winpe_root.join("copype.ps1"),
        winpe_root.join(&arch).join("copype.ps1"),
    ]
}

#[cfg(target_os = "windows")]
fn candidate_oscdimg_paths(root: &Path, arch: &str) -> Vec<PathBuf> {
    let arch = normalize_arch(arch);
    let deployment_tools_root = root.join("Deployment Tools");
    vec![
        deployment_tools_root
            .join(&arch)
            .join("Oscdimg")
            .join("oscdimg.exe"),
        deployment_tools_root.join("Oscdimg").join("oscdimg.exe"),
        root.join("Oscdimg").join("oscdimg.exe"),
    ]
}

#[cfg(target_os = "windows")]
fn candidate_dism_paths(root: &Path, arch: &str) -> Vec<PathBuf> {
    let arch = normalize_arch(arch);
    let deployment_tools_root = root.join("Deployment Tools");
    vec![
        deployment_tools_root
            .join(&arch)
            .join("DISM")
            .join("dism.exe"),
        deployment_tools_root.join("DISM").join("dism.exe"),
    ]
}

#[cfg(target_os = "windows")]
fn push_unique_path(paths: &mut Vec<PathBuf>, candidate: PathBuf) {
    let needle = candidate.to_string_lossy().to_lowercase();
    let exists = paths
        .iter()
        .any(|existing| existing.to_string_lossy().to_lowercase() == needle);
    if !exists {
        paths.push(candidate);
    }
}

#[cfg(target_os = "windows")]
fn registry_value_to_adk_root(output: &str) -> Option<PathBuf> {
    output
        .lines()
        .find_map(|line| {
            if !line.contains("REG_SZ") {
                return None;
            }
            line.split_once("REG_SZ")
                .map(|(_, value)| value.trim())
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
        })
        .map(|kits_root| normalize_adk_root(&kits_root))
}

#[cfg(target_os = "windows")]
fn query_registry_adk_roots() -> Vec<PathBuf> {
    let queries = [
        (
            r"HKLM\SOFTWARE\Microsoft\Windows Kits\Installed Roots",
            "KitsRoot10",
        ),
        (
            r"HKLM\SOFTWARE\Microsoft\Windows Kits\Installed Roots",
            "KitsRoot11",
        ),
        (
            r"HKLM\SOFTWARE\WOW6432Node\Microsoft\Windows Kits\Installed Roots",
            "KitsRoot10",
        ),
        (
            r"HKLM\SOFTWARE\WOW6432Node\Microsoft\Windows Kits\Installed Roots",
            "KitsRoot11",
        ),
    ];

    let mut roots = Vec::new();

    for (key, value) in queries {
        let output = Command::new("reg")
            .args(["query", key, "/v", value])
            .output();

        let Ok(output) = output else {
            continue;
        };
        if !output.status.success() {
            continue;
        }

        if let Some(root) = registry_value_to_adk_root(&String::from_utf8_lossy(&output.stdout)) {
            push_unique_path(&mut roots, root);
        }
    }

    roots
}

#[cfg(target_os = "windows")]
fn resolve_from_root(root: &Path, arch: &str) -> Option<AdkPaths> {
    let arch = normalize_arch(arch);
    let deployment_tools_arch = root.join("Deployment Tools").join(&arch);
    let deployment_tools_root = root.join("Deployment Tools");
    let winpe_root_arch = root.join("Windows Preinstallation Environment").join(&arch);
    let winpe_root_shared = root.join("Windows Preinstallation Environment");

    let copype_candidates = candidate_copype_paths(root, &arch);
    let oscdimg_candidates = candidate_oscdimg_paths(root, &arch);
    let dism_candidates = candidate_dism_paths(root, &arch);
    let winpe_ocs_candidates = vec![
        winpe_root_arch.join("WinPE_OCs"),
        winpe_root_shared.join("WinPE_OCs"),
    ];

    let copype_cmd = pick_existing(&copype_candidates);
    let oscdimg_exe = pick_existing(&oscdimg_candidates);
    let dism_exe = pick_existing(&dism_candidates);
    let winpe_root = if winpe_root_arch.exists() {
        winpe_root_arch.clone()
    } else {
        winpe_root_shared.clone()
    };
    let winpe_ocs = pick_existing(&winpe_ocs_candidates);
    let deployment_tools = if deployment_tools_arch.exists() {
        deployment_tools_arch
    } else {
        deployment_tools_root
    };

    if !root.exists() {
        return None;
    }

    if copype_cmd.exists() || dism_exe.exists() || oscdimg_exe.exists() || winpe_root.exists() {
        return Some(AdkPaths {
            root: root.to_path_buf(),
            deployment_tools,
            winpe_root,
            winpe_ocs,
            copype_cmd,
            oscdimg_exe,
            dism_exe,
        });
    }

    None
}

#[cfg(target_os = "windows")]
pub fn resolve_adk_paths(override_path: Option<&Path>, arch: &str) -> Option<AdkPaths> {
    let mut candidates = query_registry_adk_roots();

    if let Some(path) = override_path {
        push_unique_path(&mut candidates, normalize_adk_root(path));
    }

    push_unique_path(
        &mut candidates,
        PathBuf::from(r"C:\Program Files (x86)\Windows Kits\10\Assessment and Deployment Kit"),
    );
    push_unique_path(
        &mut candidates,
        PathBuf::from(r"C:\Program Files\Windows Kits\10\Assessment and Deployment Kit"),
    );
    push_unique_path(
        &mut candidates,
        PathBuf::from(r"C:\Program Files (x86)\Windows Kits\11\Assessment and Deployment Kit"),
    );
    push_unique_path(
        &mut candidates,
        PathBuf::from(r"C:\Program Files\Windows Kits\11\Assessment and Deployment Kit"),
    );

    for candidate in candidates {
        if let Some(paths) = resolve_from_root(&candidate, arch) {
            return Some(paths);
        }
    }

    None
}

#[cfg(not(target_os = "windows"))]
pub fn resolve_adk_paths(_override_path: Option<&Path>, _arch: &str) -> Option<AdkPaths> {
    None
}

pub fn resolve_adk_paths_from_env(arch: &str) -> Option<AdkPaths> {
    // Deliberately use autodetection only so all command paths are resolved
    // from the same source of truth regardless of ambient environment variables.
    resolve_adk_paths(None, arch)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_arch() {
        assert_eq!(normalize_arch("x64"), "amd64");
        assert_eq!(normalize_arch("x86_64"), "amd64");
        assert_eq!(normalize_arch("arm64"), "arm64");
        assert_eq!(normalize_arch("x86"), "x86");
    }

    #[test]
    fn test_supported_copype_arch() {
        assert!(is_supported_copype_arch("x64"));
        assert!(is_supported_copype_arch("amd64"));
        assert!(is_supported_copype_arch("x86"));
        assert!(is_supported_copype_arch("arm64"));
        assert!(is_supported_copype_arch("arm"));
        assert!(!is_supported_copype_arch("sparc"));
    }

    #[test]
    fn test_normalize_adk_root_from_windows_kits() {
        let path = PathBuf::from("/opt/Windows Kits/10");
        let normalized = normalize_adk_root(&path);
        assert_eq!(
            normalized,
            PathBuf::from("/opt/Windows Kits/10/Assessment and Deployment Kit")
        );
    }

    #[test]
    fn test_find_adk_root_from_subpath() {
        let path = PathBuf::from(
            "/opt/Windows Kits/10/Assessment and Deployment Kit/Deployment Tools/amd64/DISM",
        );
        let root = find_adk_root_from_path(&path);
        assert_eq!(
            root,
            Some(PathBuf::from(
                "/opt/Windows Kits/10/Assessment and Deployment Kit"
            ))
        );
    }
}
