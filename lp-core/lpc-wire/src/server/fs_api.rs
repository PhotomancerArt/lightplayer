//! Filesystem API message types
//!
//! Defines request and response types for filesystem operations.

use crate::serde_base64;
use crate::server::file_chunk::FileChunk;
use alloc::{string::String, vec::Vec};
use lpc_model::{FsVersion, LpPathBuf};
use serde::{Deserialize, Serialize};

/// Filesystem operation request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
    /// Files changed under `prefix` since an fs revision (paginated).
    ///
    /// `since` is the last revision the client already has; the server
    /// enumerates strictly newer changes. `since = FsVersion(0)` means
    /// "everything": a full recursive enumeration, since the change log
    /// need not cover files that predate tracking (the initial-pull path).
    /// `cursor` resumes a paginated enumeration (`None` starts one); pass
    /// the previous page's `next` after fully receiving it.
    ChangesSince {
        prefix: LpPathBuf,
        since: FsVersion,
        cursor: Option<crate::server::file_chunk::FileCursor>,
    },
    /// Append one chunk of a file (chunked upload for files larger than a
    /// frame).
    ///
    /// Stateless per chunk: `offset == 0` creates/truncates; `offset > 0`
    /// must equal the file's current length (appended via
    /// `LpFs::append_file`), otherwise the response carries an error.
    /// Completion is the sender's knowledge; verification is
    /// [`FsRequest::HashPackage`].
    WriteChunk {
        path: LpPathBuf,
        offset: u32,
        #[serde(
            serialize_with = "serde_base64::serialize_smart",
            deserialize_with = "serde_base64::deserialize_smart"
        )]
        data: Vec<u8>,
    },
    /// Canonical package hash (lpc-history `lph1` spec) of the directory at
    /// `prefix` — end-to-end verification for pushes and pulls.
    HashPackage { prefix: LpPathBuf },
}

/// Filesystem operation response
///
/// All response variants include an optional error field.
/// If `error` is `Some`, the operation failed and other fields may be empty/default.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
    /// One page of a ChangesSince enumeration.
    ///
    /// `next` present → more pages remain (request again with `cursor =
    /// next`); `next` absent → final page. `version` is the fs revision the
    /// page's enumeration was current to. Clients must adopt the **first**
    /// page's version as the next pull's `since`: changes landing during
    /// pagination then re-surface on that pull (convergent, never lost).
    Changes {
        entries: Vec<FileChunk>,
        next: Option<crate::server::file_chunk::FileCursor>,
        version: Option<FsVersion>,
        error: Option<String>,
    },
    /// Response to WriteChunk request (ack with the resulting extent).
    WriteChunk {
        path: LpPathBuf,
        offset: u32,
        written: u32,
        error: Option<String>,
    },
    /// Response to HashPackage request.
    ///
    /// `hash` is the 64-char lowercase-hex canonical package hash (a plain
    /// string so lpc-wire stays independent of lpc-history).
    PackageHash {
        prefix: LpPathBuf,
        hash: String,
        error: Option<String>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::{string::ToString, vec};
    use lpc_model::AsLpPathBuf;

    #[test]
    fn test_fs_request_serialization() {
        let req = FsRequest::Read {
            path: "/project.json".as_path_buf(),
        };
        let json = crate::json::to_string(&req).unwrap();
        // Verify round-trip serialization
        let deserialized: FsRequest = crate::json::from_str(&json).unwrap();
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
        let json = crate::json::to_string(&resp).unwrap();
        // With tag="type" and rename_all="camelCase", JSON uses lowercase "read"
        assert!(json.contains("read") || json.contains("Read"));
        assert!(json.contains("/project.json"));

        let deserialized: FsResponse = crate::json::from_str(&json).unwrap();
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
    fn test_changes_since_request_committed_sample() {
        let req = FsRequest::ChangesSince {
            prefix: "/projects/x".as_path_buf(),
            since: lpc_model::FsVersion::new(7),
            cursor: None,
        };
        let json = crate::json::to_string(&req).unwrap();
        // committed wire sample — changing this breaks peers; must be deliberate
        assert_eq!(
            json,
            "{\"changesSince\":{\"prefix\":\"/projects/x\",\"since\":7,\"cursor\":null}}"
        );
        let back: FsRequest = crate::json::from_str(&json).unwrap();
        match back {
            FsRequest::ChangesSince {
                prefix,
                since,
                cursor,
            } => {
                assert_eq!(prefix.as_str(), "/projects/x");
                assert_eq!(since.as_i64(), 7);
                assert_eq!(cursor, None);
            }
            _ => panic!("Wrong variant"),
        }

        // resuming cursor round-trips
        let req = FsRequest::ChangesSince {
            prefix: "/projects/x".as_path_buf(),
            since: lpc_model::FsVersion::new(7),
            cursor: Some(crate::server::file_chunk::FileCursor {
                path: "/big.svg".as_path_buf(),
                offset: 8192,
            }),
        };
        let json = crate::json::to_string(&req).unwrap();
        let back: FsRequest = crate::json::from_str(&json).unwrap();
        match back {
            FsRequest::ChangesSince {
                cursor: Some(c), ..
            } => {
                assert_eq!(c.path.as_str(), "/big.svg");
                assert_eq!(c.offset, 8192);
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_write_chunk_and_hash_package_round_trip() {
        let req = FsRequest::WriteChunk {
            path: "/projects/x/big.svg".as_path_buf(),
            offset: 4096,
            data: b"<svg/>".to_vec(),
        };
        let json = crate::json::to_string(&req).unwrap();
        let back: FsRequest = crate::json::from_str(&json).unwrap();
        match back {
            FsRequest::WriteChunk { path, offset, data } => {
                assert_eq!(path.as_str(), "/projects/x/big.svg");
                assert_eq!(offset, 4096);
                assert_eq!(data, b"<svg/>".to_vec());
            }
            _ => panic!("Wrong variant"),
        }

        let req = FsRequest::HashPackage {
            prefix: "/projects/x".as_path_buf(),
        };
        let json = crate::json::to_string(&req).unwrap();
        assert_eq!(json, "{\"hashPackage\":{\"prefix\":\"/projects/x\"}}");
    }

    #[test]
    fn test_changes_response_terminal_frame_carries_version() {
        use crate::server::file_chunk::{FileChangeKind, FileChunk};
        let resp = FsResponse::Changes {
            entries: vec![FileChunk {
                path: "/a.json".as_path_buf(),
                kind: FileChangeKind::Upsert,
                offset: 0,
                total: 2,
                data: b"{}".to_vec(),
            }],
            next: None,
            version: Some(lpc_model::FsVersion::new(42)),
            error: None,
        };
        let json = crate::json::to_string(&resp).unwrap();
        let back: FsResponse = crate::json::from_str(&json).unwrap();
        match back {
            FsResponse::Changes {
                entries,
                next,
                version,
                error,
            } => {
                assert_eq!(entries.len(), 1);
                assert_eq!(next, None);
                assert_eq!(version, Some(lpc_model::FsVersion::new(42)));
                assert_eq!(error, None);
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[cfg(feature = "ser-write-json")]
    #[test]
    fn test_maximal_changes_frame_fits_budget() {
        use crate::budget::{
            FILE_SYNC_PAGE_MAX_ENTRIES, FILE_SYNC_PAGE_RAW_BYTES, PROJECT_READ_FRAME_MAX_BYTES,
        };
        use crate::server::file_chunk::{FileChangeKind, FileChunk, FileCursor};
        let long_path =
            "/modules/plasma-mod_h7Kq9xY2mQ4tB8Wz/deeply/nested/dir/fixture-mapping-large.svg";
        // worst case A: max entries, raw budget spread across them, long paths
        let per_entry = FILE_SYNC_PAGE_RAW_BYTES / FILE_SYNC_PAGE_MAX_ENTRIES;
        let resp = FsResponse::Changes {
            entries: (0..FILE_SYNC_PAGE_MAX_ENTRIES)
                .map(|i| FileChunk {
                    path: long_path.as_path_buf(),
                    kind: FileChangeKind::Upsert,
                    offset: u32::MAX - i as u32,
                    total: u32::MAX,
                    data: alloc::vec![0xA5u8; per_entry],
                })
                .collect(),
            next: Some(FileCursor {
                path: long_path.as_path_buf(),
                offset: u32::MAX,
            }),
            version: Some(lpc_model::FsVersion::new(i64::MAX)),
            error: None,
        };
        let len = crate::ser_write::ser_write_json_len(&resp);
        assert!(
            len <= PROJECT_READ_FRAME_MAX_BYTES,
            "maximal Changes page (many entries) is {len} bytes, budget is {PROJECT_READ_FRAME_MAX_BYTES}"
        );

        // worst case B: one entry carrying the whole raw budget
        let resp = FsResponse::Changes {
            entries: vec![FileChunk {
                path: long_path.as_path_buf(),
                kind: FileChangeKind::Upsert,
                offset: u32::MAX - FILE_SYNC_PAGE_RAW_BYTES as u32,
                total: u32::MAX,
                data: alloc::vec![0xA5u8; FILE_SYNC_PAGE_RAW_BYTES],
            }],
            next: Some(FileCursor {
                path: long_path.as_path_buf(),
                offset: u32::MAX,
            }),
            version: Some(lpc_model::FsVersion::new(i64::MAX)),
            error: None,
        };
        let len = crate::ser_write::ser_write_json_len(&resp);
        assert!(
            len <= PROJECT_READ_FRAME_MAX_BYTES,
            "maximal Changes page (single big chunk) is {len} bytes, budget is {PROJECT_READ_FRAME_MAX_BYTES}"
        );
    }

    #[test]
    fn test_fs_response_with_error() {
        let resp = FsResponse::Write {
            path: "/test.txt".as_path_buf(),
            error: Some("Permission denied".to_string()),
        };
        let json = crate::json::to_string(&resp).unwrap();
        // Verify round-trip serialization
        let deserialized: FsResponse = crate::json::from_str(&json).unwrap();
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
        let json = crate::json::to_string(&req).unwrap();
        // Verify data is NOT base64 encoded (should be plain string)
        assert!(!json.contains("aGVsbG8gd29ybGQ")); // Not base64
        assert!(!json.contains("[104,101,108,108,111")); // Not array of bytes
        assert!(json.contains("hello world")); // Plain text string

        // Verify round-trip serialization
        let deserialized: FsRequest = crate::json::from_str(&json).unwrap();
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
        let json = crate::json::to_string(&req).unwrap();
        // Verify data is base64 encoded (not an array, not plain text)
        assert!(!json.contains("[255,254,253")); // Not array of bytes
        // Should contain base64 encoding
        use base64::Engine;
        let expected_base64 = base64::engine::general_purpose::STANDARD.encode(&binary_data);
        assert!(json.contains(&expected_base64));

        // Verify round-trip serialization
        let deserialized: FsRequest = crate::json::from_str(&json).unwrap();
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
        let original_data = b"{\"key\": \"value\"}".to_vec();
        let resp = FsResponse::Read {
            path: "/test.txt".as_path_buf(),
            data: Some(original_data.clone()),
            error: None,
        };
        let json = crate::json::to_string(&resp).unwrap();
        // Verify data is NOT base64 encoded
        assert!(json.contains("key")); // Plain text in JSON
        assert!(!json.contains("eyJrZXkiOiAidmFsdWUifQ")); // Not base64

        let deserialized: FsResponse = crate::json::from_str(&json).unwrap();
        match deserialized {
            FsResponse::Read { path, data, error } => {
                assert_eq!(path.as_str(), "/test.txt");
                assert_eq!(data, Some(original_data));
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
        let json = crate::json::to_string(&resp).unwrap();
        // Verify data is base64 encoded
        use base64::Engine;
        let expected_base64 = base64::engine::general_purpose::STANDARD.encode(&binary_data);
        assert!(json.contains(&expected_base64));

        // Verify round-trip serialization
        let deserialized: FsResponse = crate::json::from_str(&json).unwrap();
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
        let json = crate::json::to_string(&req).unwrap();
        // Empty string should be serialized as ""
        assert!(json.contains("\"\""));

        // Verify round-trip serialization
        let deserialized: FsRequest = crate::json::from_str(&json).unwrap();
        match deserialized {
            FsRequest::Write { path, data } => {
                assert_eq!(path.as_str(), "/empty.txt");
                assert_eq!(data, Vec::<u8>::new());
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_fs_request_write_project_json() {
        // Test the specific case: writing project.json content
        // This simulates what happens when ProjectBuilder writes project.json
        use alloc::string::ToString;
        use lpc_model::project::ProjectConfig;

        let config = ProjectConfig {
            uid: "test".to_string(),
            name: "Test Project".to_string(),
        };
        let project_json =
            crate::json::to_string(&config).expect("Failed to serialize project config");
        let project_json_bytes = project_json.as_bytes().to_vec();

        // Create FsRequest::Write with the JSON bytes
        let req = FsRequest::Write {
            path: "/project.json".as_path_buf(),
            data: project_json_bytes.clone(),
        };

        // Serialize the request
        let request_json = crate::json::to_string(&req).expect("Failed to serialize FsRequest");

        let deserialized: FsRequest =
            crate::json::from_str(&request_json).expect("Failed to deserialize FsRequest");

        match deserialized {
            FsRequest::Write { path, data } => {
                assert_eq!(path.as_str(), "/project.json");
                // The data should match exactly
                assert_eq!(
                    data,
                    project_json_bytes,
                    "Data should round-trip correctly. Original: {:?}, Deserialized: {:?}. Original string: {}, Deserialized string: {}",
                    project_json_bytes,
                    data,
                    core::str::from_utf8(&project_json_bytes).unwrap_or("<invalid>"),
                    core::str::from_utf8(&data).unwrap_or("<invalid>")
                );

                // Verify we can deserialize the project.json content
                let deserialized_config: ProjectConfig = crate::json::from_slice(&data)
                    .expect("Failed to deserialize ProjectConfig from round-trip data");
                assert_eq!(deserialized_config.uid, "test");
                assert_eq!(deserialized_config.name, "Test Project");
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn investigate_json_string_serialization_output() {
        // Investigate: What does the actual JSON output look like?
        // This test helps understand if the escaping will cause problems with other systems.
        use alloc::string::ToString;
        use lpc_model::project::ProjectConfig;

        let config = ProjectConfig {
            uid: "test".to_string(),
            name: "Test Project".to_string(),
        };
        let project_json =
            crate::json::to_string(&config).expect("Failed to serialize project config");

        // project_json is: {"uid":"test","name":"Test Project"}
        assert_eq!(project_json, r#"{"uid":"test","name":"Test Project"}"#);

        // Convert to bytes and serialize in FsRequest
        let project_json_bytes = project_json.as_bytes().to_vec();
        let req = FsRequest::Write {
            path: "/project.json".as_path_buf(),
            data: project_json_bytes.clone(),
        };

        let request_json = crate::json::to_string(&req).expect("Failed to serialize FsRequest");

        // Analysis of what happens:
        // 1. serialize_smart sees valid UTF-8 bytes: {"uid":"test","name":"Test Project"}
        // 2. It calls serializer.serialize_str() with that string
        // 3. The JSON serializer MUST escape quotes in string values (JSON spec requirement)
        // 4. So it produces: "{\"uid\":\"test\",\"name\":\"Test Project\"}"
        //    (the outer quotes are JSON string delimiters, inner \" are escaped quotes)
        //
        // The request_json will look like:
        // {"Write":{"path":"/project.json","data":"{\"uid\":\"test\",\"name\":\"Test Project\"}"}}
        //
        // This is VALID JSON. Any JSON parser will:
        // - Parse the outer structure correctly
        // - Unescape the "data" field to get: {"uid":"test","name":"Test Project"}
        //
        // The escaping is NOT a problem - it's required by the JSON specification.
        // When you put a string containing quotes into JSON, those quotes MUST be escaped.

        // Verify the JSON contains escaped quotes (this is correct JSON!)
        assert!(
            request_json.contains(r#""data":"{\"uid\""#),
            "JSON should contain escaped quotes in the data field. Actual JSON: {request_json}"
        );

        // Verify round-trip works
        let deserialized: FsRequest =
            crate::json::from_str(&request_json).expect("Failed to deserialize FsRequest");

        match deserialized {
            FsRequest::Write { path: _, data } => {
                // The data should be the original bytes (unescaped by JSON parser)
                assert_eq!(
                    data, project_json_bytes,
                    "Data should round-trip correctly. This confirms JSON unescaping works."
                );

                // And we should be able to parse it as ProjectConfig
                let parsed_config: ProjectConfig =
                    crate::json::from_slice(&data).expect("Should parse as ProjectConfig");
                assert_eq!(parsed_config.uid, "test");
            }
            _ => panic!("Wrong variant"),
        }

        // Conclusion: quoted data inside a JSON string is normal JSON escaping,
        // and the standard parser restores the original bytes on deserialize.
    }

    #[test]
    fn test_serialize_smart_round_trip() {
        // Test serialize_smart/deserialize_smart directly
        use crate::serde_base64;
        use serde::{Deserialize, Serialize};

        #[derive(Serialize, Deserialize)]
        struct Test {
            #[serde(
                serialize_with = "serde_base64::serialize_smart",
                deserialize_with = "serde_base64::deserialize_smart"
            )]
            data: Vec<u8>,
        }

        // Test with JSON-like content
        let original_bytes = b"{\"uid\":\"test\"}".to_vec();
        let test = Test {
            data: original_bytes.clone(),
        };

        let json = crate::json::to_string(&test).expect("Failed to serialize");
        // json should be: {"data":"{\"uid\":\"test\"}"}

        let deserialized: Test = crate::json::from_str(&json).expect("Failed to deserialize");

        assert_eq!(
            deserialized.data,
            original_bytes,
            "Bytes should round-trip. Original: {:?}, Deserialized: {:?}. Original str: {}, Deserialized str: {}",
            original_bytes,
            deserialized.data,
            core::str::from_utf8(&original_bytes).unwrap(),
            core::str::from_utf8(&deserialized.data).unwrap()
        );
    }
}
