//! JSON serialization/deserialization helpers for the wire protocol.
//!
//! This module is a small facade over `serde_json`. Keeping the facade lets
//! transports and tests share one import path while leaving room for protocol
//! framing and message-size policy to live here later.

use serde::{Deserialize, Serialize};

pub use serde_json::Error;

/// Prefix that marks a protocol message on a serial link.
///
/// Every line on the device link is either an `M!{json}` message or free-form
/// log output. Both directions parse by this prefix: a line without it is
/// treated as a log line and dropped by the message parser (see
/// `SerialTransport::receive` in fw-core), so any writer that omits it is
/// silently ignored rather than rejected.
pub const SERIAL_LINE_PREFIX: &str = "M!";

/// Frame a wire message as one serial line: `M!{json}\n`.
///
/// The single framer for every writer on the device link, in both directions.
/// The line is complete (prefix, JSON, terminating newline) and ready to write
/// to the wire as bytes.
pub fn to_serial_line<T: Serialize>(value: &T) -> Result<alloc::string::String, Error> {
    let json = serde_json::to_string(value)?;
    let mut line = alloc::string::String::with_capacity(SERIAL_LINE_PREFIX.len() + json.len() + 1);
    line.push_str(SERIAL_LINE_PREFIX);
    line.push_str(&json);
    line.push('\n');
    Ok(line)
}

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
    fn to_serial_line_frames_a_client_message() {
        use crate::{ClientMessage, ClientRequest};

        let msg = ClientMessage {
            id: 7,
            msg: ClientRequest::ListLoadedProjects,
        };

        let line = to_serial_line(&msg).unwrap();

        let body = line
            .strip_prefix(SERIAL_LINE_PREFIX)
            .expect("line carries the message prefix")
            .strip_suffix('\n')
            .expect("line is newline-terminated");
        assert!(!body.contains('\n'), "one message is one line");
        let decoded: ClientMessage = from_str(body).unwrap();
        assert_eq!(decoded.id, 7);
        assert!(matches!(decoded.msg, ClientRequest::ListLoadedProjects));
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
/// `fw-esp32c6` writes outbound messages with `ser-write-json` so it can stream
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
        let msg = ServerMessage::new(1, ServerMsgBody::UnloadProject);

        let json = serialize_with_ser_write_json(&msg).expect("ser-write-json serialize");
        let deserialized: ServerMessage = from_str(&json).expect("from_str(ser-write-json output)");

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
    fn pre_link_counters_heartbeat_still_decodes() {
        // Bytes from firmware built before `link` / the MemoryStats
        // fragmentation fields existed: the additive fields must decode as
        // absent (the additive-FIELD rule the wire posture depends on).
        let old = r#"{"id":0,"msg":{"heartbeat":{"fps":{"avg":60.0,"sdev":1.0,"min":58.0,"max":62.0},"frame_count":7,"loaded_projects":[{"handle":1,"path":"/projects/demo"}],"uptime_ms":5000,"memory":{"freeBytes":1,"usedBytes":2,"totalBytes":3}}}}"#;
        let msg: ServerMessage = from_str(old).expect("old heartbeat decodes");
        let ServerMsgBody::Heartbeat {
            memory,
            link,
            identity,
            loaded_projects,
            ..
        } = msg.msg
        else {
            panic!("expected heartbeat");
        };
        assert!(link.is_none(), "link absent on old frames");
        assert!(identity.is_none(), "identity absent on pre-R4 frames");
        let memory = memory.expect("memory present");
        assert!(memory.largest_free_block.is_none());
        assert!(memory.oom_retry_saves.is_none());
        // The fault policy's additive field: firmware that predates it says
        // nothing, and absent must decode as "no fault reported" rather
        // than failing the whole frame.
        let [project] = loaded_projects.as_slice() else {
            panic!("expected one loaded project");
        };
        assert!(
            project.fault.is_none(),
            "fault absent on pre-fault-policy frames"
        );
    }

    /// The connection-monitor stamps are additive on an object that already
    /// shipped: firmware built before M6 P1b sends the four loss counters and
    /// nothing else, and that frame must still decode — with the stamps read
    /// as "this device never told us", not as a parse failure.
    #[test]
    fn a_link_object_without_the_connection_stamps_still_decodes() {
        let old = r#"{"id":0,"msg":{"heartbeat":{"fps":{"avg":60.0,"sdev":0.0,"min":60.0,"max":60.0},"frame_count":7,"loaded_projects":[],"uptime_ms":5000,"link":{"parseFailures":0,"rxErrors":0,"queueFullDrops":0,"stalePartialFlushes":0}}}}"#;
        let msg: ServerMessage = from_str(old).expect("a pre-P1b link object decodes");
        let ServerMsgBody::Heartbeat { link, .. } = msg.msg else {
            panic!("expected heartbeat");
        };
        let link = link.expect("link present");
        assert_eq!(link.parse_failures, 0);
        assert_eq!(link.host_not_draining_ms, None, "never told us");
        assert_eq!(link.host_draining_again_ms, None);
        assert_eq!(link.not_draining_count, 0);

        // And the other direction: a link that never latched sends no stamp
        // keys at all, so the four-counter frame is exactly what a C6 with a
        // host reading it from boot still puts on the wire.
        let quiet = crate::server::LinkCounters::default();
        assert_eq!(
            to_string(&quiet).unwrap(),
            r#"{"parseFailures":0,"rxErrors":0,"queueFullDrops":0,"stalePartialFlushes":0,"notDrainingCount":0}"#
        );
    }

    #[test]
    fn a_heartbeat_carrying_a_project_fault_decodes() {
        // The other half of the additive guard: a fault-reporting frame
        // decodes into the record the device card reads.
        let json = r#"{"id":0,"msg":{"heartbeat":{"fps":{"avg":43.0,"sdev":1.0,"min":42.0,"max":44.0},"frame_count":7,"loaded_projects":[{"handle":1,"path":"/projects/meteor","fault":{"sinceMs":12000,"nodes":[{"path":"/studio.show/s","message":"recovery: node 'nodes/meteor' (disabled after 3 crashes)"}]}}],"uptime_ms":5000}}}"#;
        let msg: ServerMessage = from_str(json).expect("fault heartbeat decodes");
        let ServerMsgBody::Heartbeat {
            loaded_projects, ..
        } = msg.msg
        else {
            panic!("expected heartbeat");
        };
        let fault = loaded_projects[0]
            .fault
            .as_ref()
            .expect("fault present on the reporting frame");
        assert_eq!(fault.since_ms, 12_000);
        assert_eq!(fault.nodes.len(), 1);
        assert_eq!(fault.nodes[0].path, "/studio.show/s");
        assert!(fault.nodes[0].message.contains("disabled after 3 crashes"));
    }

    #[test]
    fn a_faulted_loaded_project_round_trips_through_ser_write_json() {
        // The C6 writes heartbeats with ser-write-json; a fault that only
        // survived serde_json would never reach a card.
        let msg = ServerMessage::new(
            0,
            ServerMsgBody::Heartbeat {
                fps: SampleStats {
                    avg: 43.0,
                    sdev: 1.0,
                    min: 42.0,
                    max: 44.0,
                },
                frame_count: 7,
                loaded_projects: vec![LoadedProject {
                    handle: WireProjectHandle::new(1),
                    path: "projects/meteor".as_path_buf(),
                    fault: Some(crate::server::ProjectFaultWire::new(
                        12_000,
                        [(
                            "/studio.show/s".to_string(),
                            "recovery: node 'nodes/meteor' (disabled after 3 crashes)".to_string(),
                        )],
                    )),
                }],
                uptime_ms: 5000,
                memory: None,
                recovery: None,
                outputs: None,
                link: None,
                identity: None,
            },
        );

        let json = serialize_with_ser_write_json(&msg).expect("ser-write-json serialize");
        let decoded: ServerMessage = from_str(&json).expect("from_str(ser-write-json output)");
        let ServerMsgBody::Heartbeat {
            loaded_projects, ..
        } = decoded.msg
        else {
            panic!("expected heartbeat");
        };
        let fault = loaded_projects[0].fault.as_ref().expect("fault survived");
        assert_eq!(fault.since_ms, 12_000);
        assert_eq!(fault.nodes[0].path, "/studio.show/s");
    }

    #[test]
    fn a_long_fault_message_is_capped_on_a_char_boundary() {
        // The C6 rebuilds these strings every heartbeat out of engine
        // status text that carries no length promise.
        let long = "é".repeat(200);
        let fault = crate::server::ProjectFaultWire::new(
            0,
            core::iter::repeat_n(("/n".to_string(), long), crate::server::FAULT_NODES_CAP + 3),
        );
        assert_eq!(fault.nodes.len(), crate::server::FAULT_NODES_CAP);
        for node in &fault.nodes {
            assert!(node.message.len() <= crate::server::FAULT_MESSAGE_CAP_BYTES);
            // A cut message SAYS it was cut (G1 bench: "exceeded 10" read as a
            // lie for "exceeded 100000 iterations"), and what precedes the
            // mark is valid UTF-8 whose char count is exactly half the byte
            // count for this 2-byte char.
            let body = node
                .message
                .strip_suffix('…')
                .expect("a capped message ends in an ellipsis");
            assert_eq!(body.chars().count() * 2, body.len());
        }
    }

    /// The ack the device writes with its own serializer, not the host's:
    /// `ledger_cleared` is the only thing it carries and the only thing the
    /// client reads, so a boolean that failed to survive the firmware's
    /// encoder would be silent.
    #[test]
    fn ser_write_json_clear_faults_ack_round_trips() {
        for ledger_cleared in [true, false] {
            let msg = ServerMessage::new(7, ServerMsgBody::ClearFaults { ledger_cleared });

            let json = serialize_with_ser_write_json(&msg).expect("ser-write-json serialize");
            let deserialized: ServerMessage = from_str(&json).expect("decode the ack");

            assert_eq!(deserialized.id, 7);
            match deserialized.msg {
                ServerMsgBody::ClearFaults {
                    ledger_cleared: got,
                } => {
                    assert_eq!(got, ledger_cleared, "{json}");
                }
                other => panic!("expected the ClearFaults ack, got {other:?}"),
            }
        }
    }

    #[test]
    fn ser_write_json_heartbeat_round_trips() {
        let msg = ServerMessage::new(
            0,
            ServerMsgBody::Heartbeat {
                fps: SampleStats {
                    avg: 60.0,
                    sdev: 1.0,
                    min: 58.0,
                    max: 62.0,
                },
                frame_count: 1000,
                loaded_projects: vec![LoadedProject::new(
                    WireProjectHandle::new(1),
                    "projects/test".as_path_buf(),
                )],
                uptime_ms: 5000,
                memory: Some(MemoryStats {
                    free_bytes: 100000,
                    used_bytes: 200000,
                    total_bytes: 300000,
                    largest_free_block: Some(40000),
                    oom_retry_saves: Some(2),
                }),
                recovery: Some(crate::server::RecoveryStatus {
                    level: crate::server::RecoveryLevelWire::Yellow,
                    reset_reason: "watchdog-reset".to_string(),
                    boot_count: 4,
                    safe_mode: false,
                    output_clamp: None,
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
                outputs: Some(vec![crate::server::OutputWireStatus {
                    wire: 4,
                    gpio: 13,
                    posted: 100,
                    sent: 99,
                    torn: 1,
                    waved: 99,
                    mux: 99,
                    queue_wait_max_us: 9_800,
                }]),
                link: Some(crate::server::LinkCounters {
                    parse_failures: 3,
                    rx_errors: 1,
                    queue_full_drops: 0,
                    stale_partial_flushes: 2,
                    host_not_draining_ms: Some(1_402),
                    host_draining_again_ms: Some(8_137),
                    not_draining_count: 1,
                }),
                identity: Some(crate::server::HeartbeatIdentity {
                    device_uid: Some("dev0000000000000001".to_string()),
                    base_mac: Some("60:55:f9:0a:0b:0c".to_string()),
                }),
            },
        );

        let json = serialize_with_ser_write_json(&msg).expect("ser-write-json serialize");
        let deserialized: ServerMessage = from_str(&json).expect("from_str(ser-write-json output)");

        assert_eq!(msg.id, deserialized.id);
        match (&msg.msg, &deserialized.msg) {
            (
                ServerMsgBody::Heartbeat {
                    frame_count: expected,
                    identity: expected_identity,
                    link: expected_link,
                    ..
                },
                ServerMsgBody::Heartbeat {
                    frame_count,
                    identity,
                    link,
                    ..
                },
            ) => {
                assert_eq!(expected, frame_count);
                // The firmware's serializer is the one that must carry
                // identity: heartbeats leave a device through ser-write-json.
                assert_eq!(expected_identity, identity);
                // …and the connection-monitor stamps, which are `Option`s:
                // ser-write-json has to emit `serialize_some` for them or the
                // negative control has no vehicle at all.
                assert_eq!(expected_link, link);
                assert!(
                    json.contains(r#""hostNotDrainingMs":1402"#),
                    "the stamps must ride the wire in camelCase: {json}"
                );
                assert!(json.contains(r#""notDrainingCount":1"#), "{json}");
            }
            _ => panic!("variant mismatch"),
        }
    }
}
