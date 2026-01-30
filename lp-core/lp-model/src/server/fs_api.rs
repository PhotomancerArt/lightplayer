//! Filesystem API message types
//!
//! Defines request and response types for filesystem operations.

use crate::LpPathBuf;
use alloc::{string::String, vec::Vec};
use serde::{Deserialize, Serialize};

/// Filesystem operation request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "fsType", rename_all = "camelCase")]
pub enum FsRequest {
    /// Read a file
    Read { path: LpPathBuf },
    /// Write a file
    Write { path: LpPathBuf, data: Vec<u8> },
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
    use alloc::string::ToString;

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
}
