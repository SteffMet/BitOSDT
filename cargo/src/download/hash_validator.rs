use crate::core::errors::{BitOSDTError, BitOSDTResult};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use tracing::{info, warn};

/// Hash validation utilities for downloaded files
pub struct HashValidator;

impl HashValidator {
    /// Calculate SHA256 hash of a file
    pub fn calculate_sha256(file_path: &Path) -> BitOSDTResult<String> {
        let file = File::open(file_path)?;

        let mut reader = BufReader::with_capacity(8 * 1024 * 1024, file); // 8MB buffer
        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 65536]; // 64KB chunks

        loop {
            let bytes_read = reader.read(&mut buffer)?;

            if bytes_read == 0 {
                break;
            }

            hasher.update(&buffer[..bytes_read]);
        }

        let hash = hasher.finalize();
        Ok(format!("{:x}", hash))
    }

    /// Calculate SHA256 with progress callback
    pub fn calculate_sha256_with_progress<F>(
        file_path: &Path,
        total_size: u64,
        mut progress_callback: F,
    ) -> BitOSDTResult<String>
    where
        F: FnMut(u64, u64),
    {
        let file = File::open(file_path)?;

        let mut reader = BufReader::with_capacity(8 * 1024 * 1024, file);
        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 65536];
        let mut bytes_processed: u64 = 0;

        loop {
            let bytes_read = reader.read(&mut buffer)?;

            if bytes_read == 0 {
                break;
            }

            hasher.update(&buffer[..bytes_read]);
            bytes_processed += bytes_read as u64;
            progress_callback(bytes_processed, total_size);
        }

        let hash = hasher.finalize();
        Ok(format!("{:x}", hash))
    }

    /// Validate file against expected SHA256 hash
    pub fn validate_sha256(file_path: &Path, expected_hash: &str) -> BitOSDTResult<bool> {
        info!("Validating SHA256 hash for {:?}", file_path);

        let calculated = Self::calculate_sha256(file_path)?;
        let expected_lower = expected_hash.to_lowercase();

        if calculated == expected_lower {
            info!("Hash validation successful");
            Ok(true)
        } else {
            warn!(
                "Hash mismatch: expected {}, calculated {}",
                expected_lower, calculated
            );
            Ok(false)
        }
    }

    /// Validate with detailed error on mismatch
    pub fn validate_sha256_strict(file_path: &Path, expected_hash: &str) -> BitOSDTResult<()> {
        if !Self::validate_sha256(file_path, expected_hash)? {
            return Err(BitOSDTError::Validation(format!(
                "SHA256 hash mismatch for {:?}",
                file_path
            )));
        }
        Ok(())
    }

    /// Calculate MD5 hash (for legacy OSDCloud catalog compatibility)
    pub fn calculate_md5(file_path: &Path) -> BitOSDTResult<String> {
        let file = File::open(file_path)?;

        let mut reader = BufReader::with_capacity(8 * 1024 * 1024, file);
        let mut hasher = md5::Context::new();
        let mut buffer = [0u8; 65536];

        loop {
            let bytes_read = reader.read(&mut buffer)?;

            if bytes_read == 0 {
                break;
            }

            hasher.consume(&buffer[..bytes_read]);
        }

        let result = hasher.compute();
        Ok(format!("{:x}", result))
    }

    /// Validate MD5 hash
    pub fn validate_md5(file_path: &Path, expected_hash: &str) -> BitOSDTResult<bool> {
        let calculated = Self::calculate_md5(file_path)?;
        let expected_lower = expected_hash.to_lowercase();
        Ok(calculated == expected_lower)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_sha256_calculation() {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"Hello, World!").unwrap();
        file.flush().unwrap();

        let hash = HashValidator::calculate_sha256(file.path()).unwrap();
        // SHA256 of "Hello, World!" is known
        assert_eq!(
            hash,
            "dffd6021bb2bd5b0af676290809ec3a53191dd81c7f70a4b28688a362182986f"
        );
    }

    #[test]
    fn test_sha256_validation() {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"test data").unwrap();
        file.flush().unwrap();

        let hash = HashValidator::calculate_sha256(file.path()).unwrap();
        assert!(HashValidator::validate_sha256(file.path(), &hash).unwrap());
        assert!(!HashValidator::validate_sha256(file.path(), "invalid_hash").unwrap());
    }
}
