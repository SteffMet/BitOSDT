#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_error_conversion_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "test");
        let bit_err: BitOSDTError = io_err.into();

        match bit_err {
            BitOSDTError::Io(_) => (),
            _ => panic!("Expected Io error"),
        }
    }

    #[test]
    fn test_error_conversion_serde() {
        let json = "invalid json";
        let result: Result<serde_json::Value, _> = serde_json::from_str(json);

        if let Err(e) = result {
            let bit_err: BitOSDTError = e.into();
            match bit_err {
                BitOSDTError::Serialization(_) => (),
                _ => panic!("Expected Serialization error"),
            }
        }
    }

    #[test]
    fn test_database_error_display() {
        let err = DatabaseError::ConnectionFailed("test db".to_string());
        let msg = format!("{}", err);
        assert!(msg.contains("Connection failed"));
        assert!(msg.contains("test db"));
    }

    #[test]
    fn test_config_error_display() {
        let err = ConfigError::InvalidConfig("bad value".to_string());
        let msg = format!("{}", err);
        assert!(msg.contains("Invalid configuration"));
        assert!(msg.contains("bad value"));
    }

    #[test]
    fn test_bitosdt_error_variants() {
        let errors = vec![
            BitOSDTError::Network("timeout".to_string()),
            BitOSDTError::HardwareDetection("no wmi".to_string()),
            BitOSDTError::Driver("not found".to_string()),
            BitOSDTError::Deployment("failed".to_string()),
            BitOSDTError::WinPE("no adk".to_string()),
            BitOSDTError::CatalogSync("xml error".to_string()),
            BitOSDTError::NotImplemented("feature".to_string()),
            BitOSDTError::InvalidInput("bad data".to_string()),
            BitOSDTError::NotFound("resource".to_string()),
            BitOSDTError::PermissionDenied("access".to_string()),
            BitOSDTError::Cancelled,
            BitOSDTError::Unknown("error".to_string()),
        ];

        for err in errors {
            let msg = format!("{}", err);
            assert!(!msg.is_empty());
        }
    }
}
