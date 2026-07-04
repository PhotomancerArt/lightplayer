//! JSON serialization/deserialization helpers for the wire protocol.
//!
//! This module is a small facade over `serde_json`. Keeping the facade lets
//! transports and tests share one import path while leaving room for protocol
//! framing and message-size policy to live here later.

use serde::{Deserialize, Serialize};

#[path = "json/json_write.rs"]
pub mod json_write;
#[path = "json/json_writer.rs"]
pub mod json_writer;
#[path = "json/streaming_base64.rs"]
pub mod streaming_base64;

pub use serde_json::Error;

/// Serialize a value to a JSON string.
pub fn to_string<T: Serialize>(value: &T) -> Result<alloc::string::String, Error> {
    serde_json::to_string(value)
}

/// Deserialize a value from a JSON string.
pub fn from_str<T>(s: &str) -> Result<T, Error>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_str(s)
}

/// Deserialize a value from a JSON byte slice.
pub fn from_slice<T>(bytes: &[u8]) -> Result<T, Error>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_slice(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::{String, ToString};
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct TestStruct {
        test: u32,
        name: String,
    }

    #[test]
    fn to_string_serializes_json() {
        let value = TestStruct {
            test: 42,
            name: "test".to_string(),
        };

        let json = to_string(&value).unwrap();

        assert!(json.contains("\"test\":42"));
        assert!(json.contains("\"name\":\"test\""));
    }

    #[test]
    fn from_str_deserializes_json() {
        let json = r#"{"test":42,"name":"test"}"#;

        let value: TestStruct = from_str(json).unwrap();

        assert_eq!(value.test, 42);
        assert_eq!(value.name, "test");
    }

    #[test]
    fn round_trips_json_string() {
        let original = TestStruct {
            test: 42,
            name: "test".to_string(),
        };

        let json = to_string(&original).unwrap();
        let deserialized: TestStruct = from_str(&json).unwrap();

        assert_eq!(original, deserialized);
    }

    #[test]
    fn project_config_round_trips() {
        use lpc_model::project::ProjectConfig;

        let original = ProjectConfig {
            uid: "test".to_string(),
            name: "Test Project".to_string(),
        };

        let json = to_string(&original).unwrap();
        let deserialized: ProjectConfig = from_str(&json).unwrap();

        assert_eq!(original.uid, deserialized.uid);
        assert_eq!(original.name, deserialized.name);
    }

    #[test]
    fn project_config_deserializes_from_slice() {
        use lpc_model::project::ProjectConfig;

        let original = ProjectConfig {
            uid: "test".to_string(),
            name: "Test Project".to_string(),
        };
        let json = to_string(&original).unwrap();

        let deserialized: ProjectConfig = from_slice(json.as_bytes()).unwrap();

        assert_eq!(original.uid, deserialized.uid);
        assert_eq!(original.name, deserialized.name);
    }
}

/// Compatibility tests for the ESP32 streaming serializer.
///
/// `fw-esp32` writes outbound messages with `ser-write-json` so it can stream
/// directly to serial without allocating a full message string. These tests
/// confirm those bytes remain normal JSON for the shared parser.
#[cfg(all(test, feature = "ser-write-json"))]
mod ser_write_json_tests {
    use super::*;
    use crate::ServerMessage;
    use crate::project::WireProjectHandle;
    use crate::server::{FsResponse, LoadedProject, MemoryStats, SampleStats, ServerMsgBody};
    use alloc::string::{String, ToString};
    use alloc::vec;
    use alloc::vec::Vec;
    use core::convert::Infallible;
    use lpc_model::AsLpPathBuf;
    use ser_write_json::SerWrite;
    use ser_write_json::ser::to_writer;
    use serde::Serialize;

    struct VecWriter<'a>(&'a mut Vec<u8>);

    impl SerWrite for VecWriter<'_> {
        type Error = Infallible;

        fn write(&mut self, buf: &[u8]) -> Result<(), Self::Error> {
            self.0.extend_from_slice(buf);
            Ok(())
        }
    }

    fn serialize_with_ser_write_json<T: Serialize>(
        value: &T,
    ) -> Result<String, ser_write_json::ser::Error<Infallible>> {
        let mut buffer = Vec::new();
        let mut writer = VecWriter(&mut buffer);
        to_writer(&mut writer, value)?;
        Ok(core::str::from_utf8(&buffer)
            .expect("JSON output is valid UTF-8")
            .to_string())
    }

    #[test]
    fn ser_write_json_server_message_round_trips() {
        let msg = ServerMessage::<()> {
            id: 1,
            msg: ServerMsgBody::UnloadProject,
        };

        let json = serialize_with_ser_write_json(&msg).expect("ser-write-json serialize");
        let deserialized: ServerMessage<()> =
            from_str(&json).expect("from_str(ser-write-json output)");

        assert_eq!(msg.id, deserialized.id);
        assert!(matches!(deserialized.msg, ServerMsgBody::UnloadProject));
    }

    #[test]
    fn ser_write_json_fs_response_read_round_trips() {
        let resp = FsResponse::Read {
            path: "/project.json".as_path_buf(),
            data: Some(b"{\"uid\":\"test\"}".to_vec()),
            error: None,
        };

        let json = serialize_with_ser_write_json(&resp).expect("ser-write-json serialize");
        let deserialized: FsResponse = from_str(&json).expect("from_str(ser-write-json output)");

        match (&resp, &deserialized) {
            (
                FsResponse::Read {
                    path: expected_path,
                    data: expected_data,
                    error: expected_error,
                },
                FsResponse::Read { path, data, error },
            ) => {
                assert_eq!(expected_path.as_str(), path.as_str());
                assert_eq!(expected_data, data);
                assert_eq!(expected_error, error);
            }
            _ => panic!("variant mismatch"),
        }
    }

    #[test]
    fn ser_write_json_heartbeat_round_trips() {
        let msg = ServerMessage::<()> {
            id: 0,
            msg: ServerMsgBody::Heartbeat {
                fps: SampleStats {
                    avg: 60.0,
                    sdev: 1.0,
                    min: 58.0,
                    max: 62.0,
                },
                frame_count: 1000,
                loaded_projects: vec![LoadedProject {
                    handle: WireProjectHandle::new(1),
                    path: "projects/test".as_path_buf(),
                }],
                uptime_ms: 5000,
                memory: Some(MemoryStats {
                    free_bytes: 100000,
                    used_bytes: 200000,
                    total_bytes: 300000,
                }),
                recovery: Some(crate::server::RecoveryStatus {
                    level: crate::server::RecoveryLevelWire::Yellow,
                    reset_reason: "watchdog-reset".to_string(),
                    boot_count: 4,
                    safe_mode: false,
                    last_crash: Some(crate::server::CrashSummaryWire {
                        cause: "watchdog".to_string(),
                        path: "boot/node:nodes/fire".to_string(),
                        message: String::new(),
                        boots_ago: 1,
                    }),
                    paths: vec![crate::server::RecoveryPathWire {
                        path: "node:nodes/fire".to_string(),
                        state: "yellow".to_string(),
                        crash_count: 1,
                    }],
                }),
            },
        };

        let json = serialize_with_ser_write_json(&msg).expect("ser-write-json serialize");
        let deserialized: ServerMessage<()> =
            from_str(&json).expect("from_str(ser-write-json output)");

        assert_eq!(msg.id, deserialized.id);
        match (&msg.msg, &deserialized.msg) {
            (
                ServerMsgBody::Heartbeat {
                    frame_count: expected,
                    ..
                },
                ServerMsgBody::Heartbeat { frame_count, .. },
            ) => assert_eq!(expected, frame_count),
            _ => panic!("variant mismatch"),
        }
    }
}
