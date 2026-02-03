//! Filesystem API message types
//!
//! Defines request and response types for filesystem operations.

use crate::LpPathBuf;
use crate::serde_base64;
use alloc::{string::String, vec::Vec};
use serde::{Deserialize, Serialize};

/// Filesystem operation request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "fsType", rename_all = "camelCase")]
pub enum FsRequest {
    /// Read a file
    Read { path: LpPathBuf },
    /// Write a file
    Write {
        path: LpPathBuf,
        #[serde(
            serialize_with = "serde_base64::serialize_smart",
            deserialize_with = "serde_base64::deserialize_smart"
        )]
        data: Vec<u8>,
    },
    /// Delete a file
    DeleteFile { path: LpPathBuf },
    /// Delete a directory (always recursive)
    DeleteDir { path: LpPathBuf },
    /// List directory contents
    ListDir { path: LpPathBuf, recursive: bool },
}

/// Filesystem operation response
///
/// All response variants include an optional error field.
/// If `error` is `Some`, the operation failed and other fields may be empty/default.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "fsType", rename_all = "camelCase")]
pub enum FsResponse {
    /// Response to Read request
    Read {
        path: LpPathBuf,
        #[serde(
            serialize_with = "serde_base64::serialize_option_smart",
            deserialize_with = "serde_base64::deserialize_option_smart"
        )]
        data: Option<Vec<u8>>,
        error: Option<String>,
    },
    /// Response to Write request
    Write {
        path: LpPathBuf,
        error: Option<String>,
    },
    /// Response to DeleteFile request
    DeleteFile {
        path: LpPathBuf,
        error: Option<String>,
    },
    /// Response to DeleteDir request
    DeleteDir {
        path: LpPathBuf,
        error: Option<String>,
    },
    /// Response to ListDir request
    ListDir {
        path: LpPathBuf,
        entries: Vec<LpPathBuf>,
        error: Option<String>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AsLpPathBuf;
    use alloc::{string::ToString, vec};

    #[test]
    fn test_fs_request_serialization() {
        let req = FsRequest::Read {
            path: "/project.json".as_path_buf(),
        };
        let json = serde_json::to_string(&req).unwrap();
        // Verify round-trip serialization
        let deserialized: FsRequest = serde_json::from_str(&json).unwrap();
        match deserialized {
            FsRequest::Read { path } => assert_eq!(path.as_str(), "/project.json"),
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_fs_response_serialization() {
        let resp = FsResponse::Read {
            path: "/project.json".as_path_buf(),
            data: Some(b"{}".to_vec()),
            error: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        // With tag="type" and rename_all="camelCase", JSON uses lowercase "read"
        assert!(json.contains("read") || json.contains("Read"));
        assert!(json.contains("/project.json"));

        let deserialized: FsResponse = serde_json::from_str(&json).unwrap();
        match deserialized {
            FsResponse::Read { path, data, error } => {
                assert_eq!(path.as_str(), "/project.json");
                assert_eq!(data, Some(b"{}".to_vec()));
                assert_eq!(error, None);
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_fs_response_with_error() {
        let resp = FsResponse::Write {
            path: "/test.txt".as_path_buf(),
            error: Some("Permission denied".to_string()),
        };
        let json = serde_json::to_string(&resp).unwrap();
        // Verify round-trip serialization
        let deserialized: FsResponse = serde_json::from_str(&json).unwrap();
        match deserialized {
            FsResponse::Write { path, error } => {
                assert_eq!(path.as_str(), "/test.txt");
                assert_eq!(error, Some("Permission denied".to_string()));
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_fs_request_write_text() {
        // Text data should be serialized as plain string
        let req = FsRequest::Write {
            path: "/test.txt".as_path_buf(),
            data: b"hello world".to_vec(),
        };
        let json = serde_json::to_string(&req).unwrap();
        // Verify data is NOT base64 encoded (should be plain string)
        assert!(!json.contains("aGVsbG8gd29ybGQ")); // Not base64
        assert!(!json.contains("[104,101,108,108,111")); // Not array of bytes
        assert!(json.contains("hello world")); // Plain text string

        // Verify round-trip serialization
        let deserialized: FsRequest = serde_json::from_str(&json).unwrap();
        match deserialized {
            FsRequest::Write { path, data } => {
                assert_eq!(path.as_str(), "/test.txt");
                assert_eq!(data, b"hello world".to_vec());
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_fs_request_write_binary() {
        // Binary data should be serialized as base64
        let binary_data = vec![0xFF, 0xFE, 0xFD, 0x00, 0x01, 0x02];
        let req = FsRequest::Write {
            path: "/test.bin".as_path_buf(),
            data: binary_data.clone(),
        };
        let json = serde_json::to_string(&req).unwrap();
        // Verify data is base64 encoded (not an array, not plain text)
        assert!(!json.contains("[255,254,253")); // Not array of bytes
        // Should contain base64 encoding
        use base64::Engine;
        let expected_base64 = base64::engine::general_purpose::STANDARD.encode(&binary_data);
        assert!(json.contains(&expected_base64));

        // Verify round-trip serialization
        let deserialized: FsRequest = serde_json::from_str(&json).unwrap();
        match deserialized {
            FsRequest::Write { path, data } => {
                assert_eq!(path.as_str(), "/test.bin");
                assert_eq!(data, binary_data);
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_fs_response_read_text() {
        // Text data should be serialized as plain string
        let resp = FsResponse::Read {
            path: "/test.txt".as_path_buf(),
            data: Some(b"{\"key\": \"value\"}".to_vec()),
            error: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        // Verify data is NOT base64 encoded
        assert!(json.contains("key")); // Plain text in JSON
        assert!(!json.contains("eyJrZXkiOiAidmFsdWUifQ")); // Not base64

        // Verify round-trip serialization
        let deserialized: FsResponse = serde_json::from_str(&json).unwrap();
        match deserialized {
            FsResponse::Read { path, data, error } => {
                assert_eq!(path.as_str(), "/test.txt");
                assert_eq!(data, Some(b"{\"key\": \"value\"}".to_vec()));
                assert_eq!(error, None);
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_fs_response_read_binary() {
        // Binary data should be serialized as base64
        let binary_data = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A]; // PNG header start
        let resp = FsResponse::Read {
            path: "/test.png".as_path_buf(),
            data: Some(binary_data.clone()),
            error: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        // Verify data is base64 encoded
        use base64::Engine;
        let expected_base64 = base64::engine::general_purpose::STANDARD.encode(&binary_data);
        assert!(json.contains(&expected_base64));

        // Verify round-trip serialization
        let deserialized: FsResponse = serde_json::from_str(&json).unwrap();
        match deserialized {
            FsResponse::Read { path, data, error } => {
                assert_eq!(path.as_str(), "/test.png");
                assert_eq!(data, Some(binary_data));
                assert_eq!(error, None);
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_fs_request_write_empty() {
        // Empty data should serialize as empty string
        let req = FsRequest::Write {
            path: "/empty.txt".as_path_buf(),
            data: vec![],
        };
        let json = serde_json::to_string(&req).unwrap();
        // Empty string should be serialized as ""
        assert!(json.contains("\"\""));

        // Verify round-trip serialization
        let deserialized: FsRequest = serde_json::from_str(&json).unwrap();
        match deserialized {
            FsRequest::Write { path, data } => {
                assert_eq!(path.as_str(), "/empty.txt");
                assert_eq!(data, Vec::<u8>::new());
            }
            _ => panic!("Wrong variant"),
        }
    }
}
