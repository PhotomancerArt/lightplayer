//! Client → server payloads.

use crate::messages::ProjectReadRequest;
use crate::project::WireProjectHandle;
use crate::project_command::WireProjectCommand;
use crate::server::FsRequest;
use alloc::string::String;
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

/// Client message with request id for correlation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientMessage {
    pub id: u64,
    pub msg: ClientRequest,
}

/// Client request variants.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ClientRequest {
    /// Ask the server for its [`crate::server::hello::ServerHello`] (also
    /// sent unsolicited at boot); answered with
    /// [`crate::server::ServerMsgBody::Hello`].
    Hello,
    Filesystem(FsRequest),
    LoadProject {
        path: String,
    },
    UnloadProject {
        handle: WireProjectHandle,
    },
    ProjectRead {
        handle: WireProjectHandle,
        request: ProjectReadRequest,
    },
    ProjectCommand {
        handle: WireProjectHandle,
        command: WireProjectCommand,
    },
    ListAvailableProjects,
    ListLoadedProjects,
    StopAllProjects,
    /// Set the server/device global log level at runtime (see
    /// [`ClientRequest::SetLogLevel`] for semantics: global,
    /// non-persistent, no `Off`).
    SetLogLevel {
        level: crate::server::api::LogLevel,
    },
    /// Restart the device, bridge-independently: no DTR/RTS dance, no
    /// physical access, nothing the USB bridge chip has to support.
    ///
    /// Answered with [`crate::server::ServerMsgBody::Reboot`] and THEN
    /// reset — the embedder's reset hook fires once the answer is on the
    /// wire. An embedder with no way to reset itself (host, browser)
    /// answers an error instead: an unhonored ack would make the recovery
    /// ladder wait for a boot that never comes.
    Reboot,
    /// Forget what the device is holding against itself: clear the
    /// crash-recovery ledger and re-arm every faulted node.
    ///
    /// The escape from a quarantine that used to need a power cycle. A node
    /// that crashed twice is gated until the region is invalidated, so a
    /// board on a ceiling kept rendering a default input — black — with
    /// every log looking healthy (2026-09-01 bench). This is the retry.
    ///
    /// Answered with [`crate::server::ServerMsgBody::ClearFaults`] and then
    /// NOTHING resets: unlike [`ClientRequest::Reboot`] this changes only
    /// bookkeeping, and the cleared ledger takes effect on the next tick.
    /// If the failure is still there the node faults again and the device
    /// re-degrades within a heartbeat — the honest answer, not a bug.
    ClearFaults,
    /// Begin a login on this link: answered with
    /// [`crate::server::ServerMsgBody::LoginChallenge`] (a fresh nonce and
    /// every installed secret's salt and cost, with no labels), or with
    /// [`crate::server::ServerMsgBody::LoginResult`] `Refused` while another
    /// login is in flight on the device or the device is in backoff.
    ///
    /// Answered on every link at every tier — like [`Self::Hello`], it is
    /// how an untrusted link gets a tier at all.
    LoginBegin,
    /// Answer the outstanding challenge: one
    /// `HMAC-SHA256(K_i, nonce)` per offer, in offer order (base64). The
    /// link is granted the highest tier among the entries that verify; the
    /// verdict comes back as [`crate::server::ServerMsgBody::LoginResult`].
    LoginAnswer {
        macs: Vec<lpc_access::LoginMac>,
    },
    /// Who has access: answered with
    /// [`crate::server::ServerMsgBody::AccessList`] — the device store's
    /// switches and every secret in it, without its key. Edit tier.
    AccessList,
    /// Add `entry` to the device store, or replace the entry with the same
    /// salt (one holder, one salt: this is how a rename re-labels). The
    /// board merges, so two browsers adding keys never erase each other's.
    /// A new entry past the store's cap is refused with an error. Answered
    /// with the list as it now stands. Edit tier.
    AccessAdd {
        entry: lpc_access::SecretEntry,
    },
    /// Drop the device-store entry with `salt` (base64); nothing happens
    /// when there is none. Answered with the list. Edit tier.
    AccessRemove {
        #[serde(with = "lpc_access::base64_bytes")]
        salt: [u8; lpc_access::SALT_BYTES],
    },
    /// Set either or both device-store switches; an absent one is left as
    /// it is. `bleEnabled` applies at the next boot (the client restarts
    /// the device). Answered with the list. Edit tier.
    #[serde(rename_all = "camelCase")]
    AccessSetSwitches {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ble_enabled: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        open: Option<bool>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_model::AsLpPathBuf;

    #[test]
    fn test_nested_filesystem_request() {
        let req = ClientRequest::Filesystem(FsRequest::Write {
            path: "/test.txt".as_path_buf(),
            data: b"hello".to_vec(),
        });
        let json = crate::json::to_string(&req).unwrap();
        let deserialized: ClientRequest = crate::json::from_str(&json).unwrap();
        match deserialized {
            ClientRequest::Filesystem(FsRequest::Write { path, data }) => {
                assert_eq!(path.as_str(), "/test.txt");
                assert_eq!(data, b"hello");
            }
            _ => panic!("Wrong request type"),
        }
    }

    #[test]
    fn test_load_project_request() {
        use alloc::string::ToString;
        let req = ClientRequest::LoadProject {
            path: "projects/my-project".to_string(),
        };
        let json = crate::json::to_string(&req).unwrap();
        let deserialized: ClientRequest = crate::json::from_str(&json).unwrap();
        match deserialized {
            ClientRequest::LoadProject { path } => {
                assert_eq!(path, "projects/my-project");
            }
            _ => panic!("Wrong request type"),
        }
    }

    #[test]
    fn test_unload_project_request() {
        let req = ClientRequest::UnloadProject {
            handle: WireProjectHandle::new(1),
        };
        let json = crate::json::to_string(&req).unwrap();
        let deserialized: ClientRequest = crate::json::from_str(&json).unwrap();
        match deserialized {
            ClientRequest::UnloadProject { handle } => {
                assert_eq!(handle.id(), 1);
            }
            _ => panic!("Wrong request type"),
        }
    }

    #[test]
    fn test_project_read_request() {
        let req = ClientRequest::ProjectRead {
            handle: WireProjectHandle::new(1),
            request: crate::messages::ProjectReadRequest::default_debug(None),
        };
        let json = crate::json::to_string(&req).unwrap();
        let deserialized: ClientRequest = crate::json::from_str(&json).unwrap();
        match deserialized {
            ClientRequest::ProjectRead { handle, request } => {
                assert_eq!(handle.id(), 1);
                assert_eq!(
                    request,
                    crate::messages::ProjectReadRequest::default_debug(None)
                );
            }
            _ => panic!("Wrong request type"),
        }
    }

    #[test]
    fn test_project_command() {
        let req = ClientRequest::ProjectCommand {
            handle: WireProjectHandle::new(1),
            command: crate::WireProjectCommand::ReadOverlay {
                request: crate::WireOverlayReadRequest,
            },
        };
        let json = crate::json::to_string(&req).unwrap();
        let deserialized: ClientRequest = crate::json::from_str(&json).unwrap();
        match deserialized {
            ClientRequest::ProjectCommand { handle, command } => {
                assert_eq!(handle.id(), 1);
                assert!(matches!(
                    command,
                    crate::WireProjectCommand::ReadOverlay { .. }
                ));
            }
            _ => panic!("Wrong request type"),
        }
    }

    #[test]
    fn test_list_available_projects_request() {
        let req = ClientRequest::ListAvailableProjects;
        let json = crate::json::to_string(&req).unwrap();
        let deserialized: ClientRequest = crate::json::from_str(&json).unwrap();
        match deserialized {
            ClientRequest::ListAvailableProjects => {}
            _ => panic!("Wrong request type"),
        }
    }

    #[test]
    fn test_list_loaded_projects_request() {
        let req = ClientRequest::ListLoadedProjects;
        let json = crate::json::to_string(&req).unwrap();
        let deserialized: ClientRequest = crate::json::from_str(&json).unwrap();
        match deserialized {
            ClientRequest::ListLoadedProjects => {}
            _ => panic!("Wrong request type"),
        }
    }

    #[test]
    fn test_set_log_level_request() {
        use crate::server::api::LogLevel;
        let req = ClientRequest::SetLogLevel {
            level: LogLevel::Trace,
        };
        let json = crate::json::to_string(&req).unwrap();
        assert!(json.contains("setLogLevel"));
        let deserialized: ClientRequest = crate::json::from_str(&json).unwrap();
        match deserialized {
            ClientRequest::SetLogLevel { level } => assert_eq!(level, LogLevel::Trace),
            _ => panic!("Wrong request type"),
        }
    }

    /// The unit-variant spelling matters: `Reboot` rides the wire as the
    /// bare string `"reboot"`, like `Hello`, not as a tagged object.
    #[test]
    fn test_reboot_request() {
        let req = ClientRequest::Reboot;
        let json = crate::json::to_string(&req).unwrap();
        assert_eq!(json, "\"reboot\"");
        let deserialized: ClientRequest = crate::json::from_str(&json).unwrap();
        assert!(matches!(deserialized, ClientRequest::Reboot));
    }

    /// Same unit-variant spelling as `Reboot`: `"clearFaults"`, bare.
    #[test]
    fn test_clear_faults_request() {
        let req = ClientRequest::ClearFaults;
        let json = crate::json::to_string(&req).unwrap();
        assert_eq!(json, "\"clearFaults\"");
        let deserialized: ClientRequest = crate::json::from_str(&json).unwrap();
        assert!(matches!(deserialized, ClientRequest::ClearFaults));
    }

    /// Login requests: the bare unit spelling for begin, and MACs as
    /// base64 strings for the answer.
    #[test]
    fn test_login_requests() {
        let begin = crate::json::to_string(&ClientRequest::LoginBegin).unwrap();
        assert_eq!(begin, r#""loginBegin""#);

        let answer = ClientRequest::LoginAnswer {
            macs: alloc::vec![lpc_access::LoginMac([0xab; 32])],
        };
        let json = crate::json::to_string(&answer).unwrap();
        assert_eq!(
            json,
            r#"{"loginAnswer":{"macs":["q6urq6urq6urq6urq6urq6urq6urq6urq6urq6urq6s="]}}"#
        );
        match crate::json::from_str::<ClientRequest>(&json).unwrap() {
            ClientRequest::LoginAnswer { macs } => {
                assert_eq!(macs, alloc::vec![lpc_access::LoginMac([0xab; 32])]);
            }
            other => panic!("wrong request type: {other:?}"),
        }
    }

    /// Access requests: the bare unit spelling for the list, the entry in
    /// its file spelling for add, the salt as base64 for remove, and
    /// camelCase optional switches (an absent one is omitted).
    #[test]
    fn test_access_requests() {
        let list = crate::json::to_string(&ClientRequest::AccessList).unwrap();
        assert_eq!(list, r#""accessList""#);

        let entry = lpc_access::SecretEntry::from_password(
            "friends",
            lpc_access::Tier::Play,
            b"pw",
            [1; 16],
            1,
        );
        let json = crate::json::to_string(&ClientRequest::AccessAdd {
            entry: entry.clone(),
        })
        .unwrap();
        assert!(
            json.starts_with(
                r#"{"accessAdd":{"entry":{"label":"friends","kind":"password","tier":"play","salt":"AQEBAQEBAQEBAQEBAQEBAQ==","iterations":1,"k":""#
            ),
            "{json}"
        );
        match crate::json::from_str::<ClientRequest>(&json).unwrap() {
            ClientRequest::AccessAdd { entry: back } => assert_eq!(back, entry),
            other => panic!("wrong request type: {other:?}"),
        }

        let remove = ClientRequest::AccessRemove { salt: [1; 16] };
        let json = crate::json::to_string(&remove).unwrap();
        assert_eq!(
            json,
            r#"{"accessRemove":{"salt":"AQEBAQEBAQEBAQEBAQEBAQ=="}}"#
        );
        assert!(matches!(
            crate::json::from_str::<ClientRequest>(&json).unwrap(),
            ClientRequest::AccessRemove { salt } if salt == [1; 16]
        ));

        let switches = ClientRequest::AccessSetSwitches {
            ble_enabled: Some(false),
            open: None,
        };
        let json = crate::json::to_string(&switches).unwrap();
        assert_eq!(json, r#"{"accessSetSwitches":{"bleEnabled":false}}"#);
        assert!(matches!(
            crate::json::from_str::<ClientRequest>(r#"{"accessSetSwitches":{"open":true}}"#)
                .unwrap(),
            ClientRequest::AccessSetSwitches {
                ble_enabled: None,
                open: Some(true)
            }
        ));
    }

    #[test]
    fn test_stop_all_projects_request() {
        let req = ClientRequest::StopAllProjects;
        let json = crate::json::to_string(&req).unwrap();
        let deserialized: ClientRequest = crate::json::from_str(&json).unwrap();
        match deserialized {
            ClientRequest::StopAllProjects => {}
            _ => panic!("Wrong request type"),
        }
    }
}
