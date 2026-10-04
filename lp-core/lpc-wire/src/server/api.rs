use crate::messages::ProjectReadEvent;
use crate::project::WireProjectHandle;
use crate::project_command::WireProjectCommandResponse;
use crate::server::fs_api::FsResponse;
use alloc::string::String;
use alloc::vec::Vec;
use lpc_model::LpPathBuf;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ServerMsgBody {
    /// Wire bootstrap: protocol version + build provenance + device uid.
    ///
    /// Sent unsolicited (id 0) as the first frame when the server loop
    /// starts serving, and as the response to [`crate::ClientRequest::Hello`].
    /// See [`crate::server::hello`] for the contract and version policy.
    Hello(crate::server::hello::ServerHello),
    /// Filesystem operation response
    Filesystem(FsResponse),
    /// Response to LoadProject
    LoadProject {
        handle: WireProjectHandle,
    },
    /// Response to UnloadProject
    UnloadProject,
    /// One batch of ordered project-read events.
    ///
    /// The transport batches events to a budget and the envelope sequences the
    /// batches (`seq`/`fin`). A read may span several `ProjectRead` messages
    /// under the same request id; the final one carries `fin == true` and (for a
    /// successful read) the `End`/`Error` event.
    ProjectRead {
        events: Vec<ProjectReadEvent>,
    },
    /// Response to ProjectCommand
    ProjectCommand {
        response: WireProjectCommandResponse,
    },
    /// Response to ListAvailableProjects
    ListAvailableProjects {
        projects: Vec<AvailableProject>,
    },
    /// Response to ListLoadedProjects
    ListLoadedProjects {
        projects: Vec<LoadedProject>,
    },
    /// Response to StopAllProjects
    StopAllProjects,
    /// Ack for SetLogLevel: the level has been applied globally.
    SetLogLevel,
    /// Ack for [`crate::ClientRequest::Reboot`]: the request was accepted and
    /// the device resets once this frame is on the wire.
    ///
    /// The ack is sent BEFORE the reset, not after it (there is no after):
    /// the embedder's reset hook fires only once the transport reports the
    /// frame written, so a client sees its answer and then the boot banner.
    /// An embedder with no reset hook answers [`ServerMsgBody::Error`]
    /// instead — a reboot that will not happen must never be acked.
    Reboot,
    /// Ack for [`crate::ClientRequest::ClearFaults`]: the engine's faulted
    /// nodes have been re-armed, and `ledger_cleared` says whether there was
    /// a crash-recovery ledger to forget as well.
    ///
    /// `false` is not a failure. A host or browser server installs no
    /// recovery region, so it has no quarantine to lift and never had one;
    /// the engine half still happened. Reporting the difference keeps the
    /// client from claiming a device forgot something it never recorded.
    ///
    /// Nothing resets and nothing is retried in the request path: the
    /// cleared state takes effect on the device's next tick.
    ClearFaults {
        ledger_cleared: bool,
    },
    /// Answer to [`crate::ClientRequest::SetEncoding`]: the encoding this
    /// link's replies are written in from the NEXT frame on.
    ///
    /// `json` when the host asked for JSON, named a different pack format, or
    /// the embedder cannot pack. This frame is always JSON; the transport
    /// switches after writing it, and a `packed` answer starts a new
    /// learned-table epoch.
    SetEncoding {
        encoding: crate::WireEncoding,
    },

    Log {
        level: LogLevel,
        message: String,
    },
    /// Heartbeat message with server status
    ///
    /// Sent periodically (typically every second) to provide server status information.
    /// These are unsolicited messages (not responses to client requests) and use `id: 0`
    /// to indicate they are not correlated with any specific request.
    ///
    /// Clients can subscribe to these messages to monitor server health, FPS, and loaded
    /// projects, or ignore them if not needed.
    ///
    /// # Prior Art
    ///
    /// This follows the pattern established in `fw-esp32c6/src/tests/test_usb.rs` which sends
    /// heartbeat messages for debugging. This implementation makes heartbeat messages part
    /// of the formal protocol using proper `ServerMessage` types with `M!` prefix.
    ///
    /// # Fields
    ///
    /// * `fps` - FPS statistics (avg, sdev, min, max) over a recent window (e.g. 5s)
    /// * `frame_count` - Total frame count since server startup
    /// * `loaded_projects` - List of currently loaded projects with handles and paths
    /// * `uptime_ms` - Server uptime in milliseconds since startup
    /// * `memory` - Optional memory statistics (platform-dependent; ESP32 reports heap)
    Heartbeat {
        /// FPS statistics over the configured window (e.g. 5 seconds)
        fps: SampleStats,
        /// Total frame count since startup
        frame_count: u64,
        /// List of loaded projects
        loaded_projects: Vec<LoadedProject>,
        /// Uptime in milliseconds since server startup
        uptime_ms: u64,
        /// Optional memory statistics (ESP32 reports heap; absent on other platforms)
        #[serde(default)]
        memory: Option<MemoryStats>,
        /// Crash-recovery state (level, last crash, gated paths); absent on
        /// targets without a recovery region.
        #[serde(default)]
        recovery: Option<crate::server::RecoveryStatus>,
        /// Per-output-wire transmission counters; absent on targets whose
        /// output drivers keep no per-wire attribution (host server,
        /// single-core fallback boots).
        #[serde(default)]
        outputs: Option<Vec<crate::server::OutputWireStatus>>,
        /// The device link's counters (lp-link's: resends, damaged frames,
        /// resets by reason, stalls, …); absent on targets whose host link
        /// is not an lp-link (host server, ws, fw-emu). Every recovery the
        /// link makes is counted here so a lossy edge stays visible
        /// (lp-link principle 2; `docs/adr/2026-09-27-lp-link-one-comms-layer.md`).
        #[serde(default)]
        link: Option<crate::server::LinkCounters>,
        /// Who this device is, repeated on every heartbeat.
        ///
        /// A client that attaches MID-STREAM never sees the boot hello, so
        /// without this it stays anonymous until its own `Hello` request is
        /// answered. Repeating identity on the unsolicited channel resolves
        /// such an attach passively, within one heartbeat period. Absent
        /// from embedders that know no identity (and from pre-R4 firmware,
        /// which is why it is optional rather than required).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        identity: Option<HeartbeatIdentity>,
    },
    /// Error response for any request type
    Error {
        error: String,
    },
    /// Answer to [`crate::ClientRequest::LoginBegin`]: a fresh challenge.
    /// MAC `nonce` under the key derived for each offer, and answer one MAC
    /// per offer, in this order. Offers carry salt and cost, never labels.
    LoginChallenge {
        #[serde(with = "lpc_access::base64_bytes")]
        nonce: [u8; lpc_access::NONCE_BYTES],
        offers: Vec<lpc_access::LoginOffer>,
    },
    /// The verdict on a login: `granted` (this link now holds `tier`, via the
    /// secret named `label`) or `refused` (wait `retry_after_ms`, possibly 0,
    /// before beginning again). Also the answer to a `LoginBegin` that could
    /// not start one.
    LoginResult(lpc_access::LoginOutcome),
    /// The request was refused because this link does not hold `needs`.
    ///
    /// A refusal is always a reply, never a dropped message, so a client can
    /// say "log in with an edit password" instead of timing out.
    NotPermitted {
        needs: lpc_access::Tier,
    },
    /// Who has access to this device: the device store's two settings and
    /// every secret in it, without `k`. The answer to
    /// [`crate::ClientRequest::AccessList`] and to each access change
    /// (`AccessAdd`, `AccessRemove`, `AccessSetSwitches`), which reply with
    /// the list as it now stands. Edit tier only.
    ///
    /// `ble_enabled` is the STORED value: a change applies at the next
    /// boot, so it can differ from whether the radio is up right now.
    /// Project sidecars are not listed; this is the device's own list.
    #[serde(rename_all = "camelCase")]
    AccessList {
        ble_enabled: bool,
        /// Who nearby gets in with no password.
        open: lpc_access::OpenTo,
        entries: Vec<crate::server::AccessEntryInfo>,
    },
    /// The device's network settings and what its station is doing: the
    /// saved network without its password, `cloudRelay`, and the station
    /// state. The answer to [`crate::ClientRequest::NetworkStatus`] and to
    /// each network change (`NetworkSet`, `NetworkForget`), which reply
    /// with the status as it now stands. Edit tier only.
    NetworkStatus(crate::server::NetworkStatus),
}

/// Log severity carried by [`ServerMsgBody::Log`] frames and
/// [`crate::ClientRequest::SetLogLevel`] requests, lowest to highest.
///
/// There is deliberately no `Off` variant: the runtime log-level command can
/// lower output to `Error` but never fully silence the device.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AvailableProject {
    pub path: LpPathBuf,
}

/// Sample statistics over a time window (e.g. FPS over 5s).
///
/// Reusable for any scalar metric: avg, population standard deviation, min, max.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SampleStats {
    pub avg: f32,
    pub sdev: f32,
    pub min: f32,
    pub max: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadedProject {
    pub handle: WireProjectHandle,
    pub path: LpPathBuf,
    /// The project's runtime fault verdict, when it has one (any node in
    /// `NodeRuntimeStatus::Fault`). Absent = no node is faulted, and absent
    /// from firmware built before the fault policy — which is why it is
    /// additive and optional rather than a required empty record.
    ///
    /// This is what stops the device card saying "Running" over a board
    /// whose show is a red breathe (`docs/adr/` fault-is-never-black).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fault: Option<ProjectFaultWire>,
}

impl LoadedProject {
    /// A loaded project with no fault — the shape every non-heartbeat
    /// listing wants.
    pub fn new(handle: WireProjectHandle, path: LpPathBuf) -> Self {
        Self {
            handle,
            path,
            fault: None,
        }
    }
}

/// Byte cap for one faulted node's message on the wire.
///
/// The C6 rebuilds this record every heartbeat out of engine status strings
/// that carry no length promise, so the cap lives with the type rather than
/// at any one fill site.
pub const FAULT_MESSAGE_CAP_BYTES: usize = 120;

/// Cap on faulted nodes reported per project per heartbeat. A project with
/// more faulted nodes than this is already unambiguously degraded; the card
/// needs the first few, not all of them.
pub const FAULT_NODES_CAP: usize = 8;

/// A project-level fault verdict as the heartbeat carries it.
///
/// Project-level rather than per-output by policy (D1): a fault anywhere
/// means every output of the project is showing the fault pattern, so the
/// card never has to know which strand hangs off which broken node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectFaultWire {
    /// Frame time in milliseconds at which the project first had a node in
    /// fault, CONTINUOUSLY until now. Same clock as the engine's frame
    /// time, so it is comparable only against itself — a client wanting
    /// "how long" subtracts it from the current frame time, never from
    /// uptime.
    pub since_ms: u64,
    /// The faulted nodes, in tree order (steady frame over frame for status
    /// diffing), capped at [`FAULT_NODES_CAP`].
    pub nodes: Vec<FaultedNodeWire>,
}

impl ProjectFaultWire {
    /// Build a capped record from a `(tree path, message)` list.
    pub fn new(since_ms: u64, nodes: impl IntoIterator<Item = (String, String)>) -> Self {
        Self {
            since_ms,
            nodes: nodes
                .into_iter()
                .take(FAULT_NODES_CAP)
                .map(|(path, message)| FaultedNodeWire::new(path, message))
                .collect(),
        }
    }
}

/// One node in fault: where it is and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FaultedNodeWire {
    /// The node's tree path, e.g. `/studio.show/s`.
    pub path: String,
    /// The runtime's own reason, truncated to [`FAULT_MESSAGE_CAP_BYTES`].
    pub message: String,
}

impl FaultedNodeWire {
    /// Build one entry, truncating the message on a char boundary.
    pub fn new(path: String, message: String) -> Self {
        Self {
            path,
            message: truncate_on_char_boundary(message, FAULT_MESSAGE_CAP_BYTES),
        }
    }
}

/// Truncate to at most `cap` bytes without splitting a char, and SAY so:
/// a cut message ends in `…` so a card never reads "exceeded 10" for
/// "exceeded 100000 iterations" (G1 bench, 2026-09-02).
fn truncate_on_char_boundary(mut text: String, cap: usize) -> String {
    const ELLIPSIS: &str = "…";
    if text.len() <= cap {
        return text;
    }
    let mut end = cap.saturating_sub(ELLIPSIS.len());
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
    text.push_str(ELLIPSIS);
    text
}

/// Optional memory statistics (platform-dependent; ESP32 reports heap).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct MemoryStats {
    pub free_bytes: u32,
    pub used_bytes: u32,
    pub total_bytes: u32,
    /// Largest single allocatable block — the number that matters on a
    /// small fragmented arena (total-free can look healthy while every
    /// allocation over a few hundred bytes fails). Absent on targets that
    /// cannot probe it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub largest_free_block: Option<u32>,
    /// Times the retrying allocator saved an allocation that first failed
    /// (fragmentation pressure evidence). Absent where unsupported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oom_retry_saves: Option<u32>,
}

/// The identity a heartbeat announces: the same two facts the hello carries,
/// and no more.
///
/// Deliberately a SUBSET of [`crate::ServerHello`] rather than a second
/// identity vocabulary — `device_uid` is the stamped `dev…` uid
/// ([`crate::ServerHello::device_uid`]) and `base_mac` is the efuse address
/// ([`crate::HardwareFacts::base_mac`]). Build provenance and capabilities
/// stay off the heartbeat: they never change while a device runs, so
/// repeating them every second would only cost frame bytes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct HeartbeatIdentity {
    /// See [`crate::ServerHello::device_uid`]. `None` = unstamped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_uid: Option<String>,
    /// See [`crate::HardwareFacts::base_mac`]. `None` from embedders with
    /// no efuse to read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_mac: Option<String>,
}

impl HeartbeatIdentity {
    /// Whether this announcement carries anything at all — an embedder that
    /// knows neither fact sends no identity rather than an empty record.
    pub fn is_empty(&self) -> bool {
        self.device_uid.is_none() && self.base_mac.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The three access answers, spelled as a client decodes them:
    /// externally tagged, binary as base64, tiers by name.
    #[test]
    fn access_answers_round_trip() {
        let challenge = ServerMsgBody::LoginChallenge {
            nonce: [7; 32],
            offers: alloc::vec![lpc_access::LoginOffer {
                salt: [1; 16],
                iterations: 120_000,
            }],
        };
        let json = crate::json::to_string(&challenge).unwrap();
        assert_eq!(
            json,
            r#"{"loginChallenge":{"nonce":"BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc=","offers":[{"salt":"AQEBAQEBAQEBAQEBAQEBAQ==","iterations":120000}]}}"#
        );
        match crate::json::from_str::<ServerMsgBody>(&json).unwrap() {
            ServerMsgBody::LoginChallenge { nonce, offers } => {
                assert_eq!(nonce, [7; 32]);
                assert_eq!(offers.len(), 1);
            }
            other => panic!("expected a challenge, got {other:?}"),
        }

        let granted = ServerMsgBody::LoginResult(lpc_access::LoginOutcome::Granted {
            tier: lpc_access::Tier::Play,
            label: "camp".into(),
        });
        let json = crate::json::to_string(&granted).unwrap();
        assert_eq!(
            json,
            r#"{"loginResult":{"granted":{"tier":"play","label":"camp"}}}"#
        );
        assert!(matches!(
            crate::json::from_str::<ServerMsgBody>(&json).unwrap(),
            ServerMsgBody::LoginResult(lpc_access::LoginOutcome::Granted { .. })
        ));

        let refused = ServerMsgBody::NotPermitted {
            needs: lpc_access::Tier::Edit,
        };
        let json = crate::json::to_string(&refused).unwrap();
        assert_eq!(json, r#"{"notPermitted":{"needs":"edit"}}"#);
        assert!(matches!(
            crate::json::from_str::<ServerMsgBody>(&json).unwrap(),
            ServerMsgBody::NotPermitted {
                needs: lpc_access::Tier::Edit
            }
        ));
    }

    /// The firmware writes frames with `ser-write-json`, not `serde_json`:
    /// the access answers must come out byte-identical through both.
    #[cfg(feature = "ser-write-json")]
    #[test]
    fn access_answers_encode_identically_through_the_device_serializer() {
        let body = ServerMsgBody::LoginChallenge {
            nonce: [9; 32],
            offers: alloc::vec![lpc_access::LoginOffer {
                salt: [2; 16],
                iterations: 1,
            }],
        };
        let mut out = alloc::vec::Vec::new();
        ser_write_json::ser::to_writer(&mut out, &body).unwrap();
        assert_eq!(
            core::str::from_utf8(&out).unwrap(),
            crate::json::to_string(&body).unwrap()
        );
    }

    /// The same, for the access list the board sends.
    #[cfg(feature = "ser-write-json")]
    #[test]
    fn access_list_encodes_identically_through_the_device_serializer() {
        let body = access_list_sample();
        let mut out = alloc::vec::Vec::new();
        ser_write_json::ser::to_writer(&mut out, &body).unwrap();
        assert_eq!(
            core::str::from_utf8(&out).unwrap(),
            crate::json::to_string(&body).unwrap()
        );
    }

    /// The network status the board sends, through the device serializer.
    #[cfg(feature = "ser-write-json")]
    #[test]
    fn network_status_encodes_identically_through_the_device_serializer() {
        for body in network_status_samples() {
            let mut out = alloc::vec::Vec::new();
            ser_write_json::ser::to_writer(&mut out, &body).unwrap();
            assert_eq!(
                core::str::from_utf8(&out).unwrap(),
                crate::json::to_string(&body).unwrap()
            );
        }
    }

    /// The network status: camelCase, the saved network without a password,
    /// `wifi` omitted when none is saved.
    #[test]
    fn network_status_round_trips() {
        let [saved, none, joined] = network_status_samples();
        let json = crate::json::to_string(&saved).unwrap();
        assert_eq!(
            json,
            r#"{"networkStatus":{"wifi":{"ssid":"lp-walk-net","hasPassword":true,"enabled":true},"cloudRelay":true,"station":"unsupported"}}"#
        );
        assert!(!json.contains("password\":"), "{json}");
        assert_eq!(
            crate::json::to_string(&none).unwrap(),
            r#"{"networkStatus":{"cloudRelay":false,"station":"off"}}"#
        );
        let json = crate::json::to_string(&joined).unwrap();
        match crate::json::from_str::<ServerMsgBody>(&json).unwrap() {
            ServerMsgBody::NetworkStatus(status) => {
                assert_eq!(
                    status.station,
                    crate::server::StationState::Joined {
                        ip: String::from("10.0.0.7"),
                        rssi: -48
                    }
                );
            }
            other => panic!("expected a network status, got {other:?}"),
        }
    }

    /// Who has access: camelCase switches, one entry per secret, no key.
    #[test]
    fn access_list_round_trips_without_a_key() {
        let json = crate::json::to_string(&access_list_sample()).unwrap();
        assert_eq!(
            json,
            r#"{"accessList":{"bleEnabled":true,"open":"play","entries":[{"label":"Yona's MacBook","kind":"browser","tier":"edit","salt":"BQUFBQUFBQUFBQUFBQUFBQ==","addedAt":1790000000}]}}"#
        );
        match crate::json::from_str::<ServerMsgBody>(&json).unwrap() {
            ServerMsgBody::AccessList {
                ble_enabled,
                open,
                entries,
            } => {
                assert!(ble_enabled);
                assert_eq!(open, lpc_access::OpenTo::Play);
                assert_eq!(entries[0].kind, lpc_access::SecretKind::Browser);
            }
            other => panic!("expected an access list, got {other:?}"),
        }
    }

    #[test]
    fn log_level_trace_round_trips() {
        let json = crate::json::to_string(&LogLevel::Trace).unwrap();
        assert_eq!(json, "\"Trace\"");
        let level: LogLevel = crate::json::from_str(&json).unwrap();
        assert_eq!(level, LogLevel::Trace);
    }

    #[test]
    fn set_log_level_request_round_trips() {
        let request = crate::ClientRequest::SetLogLevel {
            level: LogLevel::Debug,
        };
        let json = crate::json::to_string(&request).unwrap();
        let deserialized: crate::ClientRequest = crate::json::from_str(&json).unwrap();
        assert!(matches!(
            deserialized,
            crate::ClientRequest::SetLogLevel {
                level: LogLevel::Debug
            }
        ));
    }

    #[test]
    fn set_log_level_ack_round_trips() {
        let json = crate::json::to_string(&ServerMsgBody::SetLogLevel).unwrap();
        let deserialized: ServerMsgBody = crate::json::from_str(&json).unwrap();
        assert!(matches!(deserialized, ServerMsgBody::SetLogLevel));
    }

    fn network_status_samples() -> [ServerMsgBody; 3] {
        use crate::server::{NetworkStatus, StationState, WifiInfo};
        let wifi = WifiInfo {
            ssid: String::from("lp-walk-net"),
            has_password: true,
            enabled: true,
        };
        [
            ServerMsgBody::NetworkStatus(NetworkStatus {
                wifi: Some(wifi.clone()),
                cloud_relay: true,
                station: StationState::Unsupported,
            }),
            ServerMsgBody::NetworkStatus(NetworkStatus {
                wifi: None,
                cloud_relay: false,
                station: StationState::Off,
            }),
            ServerMsgBody::NetworkStatus(NetworkStatus {
                wifi: Some(wifi),
                cloud_relay: true,
                station: StationState::Joined {
                    ip: String::from("10.0.0.7"),
                    rssi: -48,
                },
            }),
        ]
    }

    fn access_list_sample() -> ServerMsgBody {
        let entry = lpc_access::SecretEntry::from_password(
            "Yona's MacBook",
            lpc_access::Tier::Edit,
            b"k",
            [5; 16],
            1,
        )
        .with_kind(lpc_access::SecretKind::Browser)
        .with_added_at(1_790_000_000);
        ServerMsgBody::AccessList {
            ble_enabled: true,
            open: lpc_access::OpenTo::Play,
            entries: alloc::vec![crate::server::AccessEntryInfo::from(&entry)],
        }
    }
}
