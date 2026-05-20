use crate::core::errors::{BitOSDTError, BitOSDTResult};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct PublishResult {
    pub destination: PathBuf,
    pub copied_files: usize,
}

pub fn stage_lightweight_media_tree(
    media_dir: &Path,
    destination: &Path,
    runtime_executable: Option<&Path>,
    manifest_json: Option<&str>,
) -> BitOSDTResult<PublishResult> {
    if !media_dir.is_dir() {
        return Err(BitOSDTError::NotFound(format!(
            "Lightweight media directory was not found: {}",
            media_dir.display()
        )));
    }

    if destination.exists() {
        if destination.is_dir() {
            fs::remove_dir_all(destination)?;
        } else {
            fs::remove_file(destination)?;
        }
    }

    fs::create_dir_all(destination)?;

    let mut copied_files = copy_directory_contents(media_dir, destination)?;

    if let Some(executable) = runtime_executable {
        if !executable.is_file() {
            return Err(BitOSDTError::NotFound(format!(
                "BitOSDT runtime executable was not found: {}",
                executable.display()
            )));
        }

        let download_dir = destination.join("download");
        fs::create_dir_all(&download_dir)?;
        fs::copy(executable, download_dir.join("bitosdt.exe"))?;
        copied_files += 1;
    }

    if let Some(manifest_json) = manifest_json {
        fs::write(destination.join("manifest.json"), manifest_json)?;
        copied_files += 1;
    }

    Ok(PublishResult {
        destination: destination.to_path_buf(),
        copied_files,
    })
}

fn copy_directory_contents(source_root: &Path, destination_root: &Path) -> BitOSDTResult<usize> {
    let mut copied_files = 0usize;
    for entry in fs::read_dir(source_root)? {
        let entry = entry?;
        let source_path = entry.path();
        let destination_path = destination_root.join(entry.file_name());

        if source_path.is_dir() {
            fs::create_dir_all(&destination_path)?;
            copied_files += copy_directory_contents(&source_path, &destination_path)?;
            continue;
        }

        if source_path.is_file() {
            if let Some(parent) = destination_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&source_path, &destination_path)?;
            copied_files += 1;
        }
    }

    Ok(copied_files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn stage_lightweight_media_tree_copies_media_and_runtime_files() {
        let temp = tempdir().expect("temp dir");
        let media_dir = temp.path().join("media");
        let destination = temp.path().join("publish");
        let runtime = temp.path().join("bitosdt.exe");

        fs::create_dir_all(media_dir.join("sources")).expect("create sources");
        fs::write(media_dir.join("bootmgr"), b"bootmgr").expect("write bootmgr");
        fs::write(media_dir.join("sources").join("boot.wim"), b"bootwim").expect("write boot.wim");
        fs::write(&runtime, b"exe").expect("write runtime");

        let result = stage_lightweight_media_tree(
            &media_dir,
            &destination,
            Some(&runtime),
            Some(r#"{"name":"BitOSDT"}"#),
        )
        .expect("stage media");

        assert_eq!(result.destination, destination);
        assert!(result.copied_files >= 4);
        assert!(destination.join("bootmgr").exists());
        assert!(destination.join("sources").join("boot.wim").exists());
        assert!(destination.join("download").join("bitosdt.exe").exists());
        assert_eq!(
            fs::read_to_string(destination.join("manifest.json")).expect("read manifest"),
            r#"{"name":"BitOSDT"}"#
        );
    }
}
