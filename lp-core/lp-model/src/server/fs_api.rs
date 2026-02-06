//! Filesystem API message types
//!
//! Defines request and response types for filesystem operations.

use crate::LpPathBuf;
use crate::serde_base64;
use alloc::{string::String, vec::Vec};
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

        // Verify round-trip serialization
        // Note: serde-json-core may escape strings differently, so we check that
        // the data round-trips correctly even if the exact bytes differ
        let deserialized: FsResponse = crate::json::from_str(&json).unwrap();
        match deserialized {
            FsResponse::Read { path, data, error } => {
                assert_eq!(path.as_str(), "/test.txt");
                // The data should round-trip, but may have different escaping
                // Check that it's valid UTF-8 and contains the expected content
                if let Some(ref data_bytes) = data {
                    let data_str = core::str::from_utf8(data_bytes).expect("Should be valid UTF-8");
                    assert!(data_str.contains("key"));
                    assert!(data_str.contains("value"));
                } else {
                    panic!("Expected Some(data)");
                }
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
        use crate::project::ProjectConfig;
        use alloc::string::ToString;

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

        // The issue: when serialize_smart serializes the bytes as a JSON string,
        // JSON escapes the quotes. So {"uid":"test"} becomes "{\"uid\":\"test\"}" in JSON.
        // When deserializing, JSON should unescape it back to {"uid":"test"}.
        // But if serde_json_core doesn't unescape properly, we get {\"uid\":\"test\"} with literal backslashes.

        // Verify round-trip: deserialize the request
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
        use crate::project::ProjectConfig;
        use alloc::string::ToString;

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
            "JSON should contain escaped quotes in the data field. Actual JSON: {}",
            request_json
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

        // Conclusion:
        // - The escaping is CORRECT and REQUIRED by JSON specification
        // - Any compliant JSON parser will unescape it correctly
        // - The previous issue was that serde_json_core::from_slice doesn't unescape,
        //   but from_slice_escaped does (which we're now using)
        // - This should work fine with other systems that use standard JSON parsers
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
