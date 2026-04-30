//! lpl-model: legacy node configs and legacy-aware payload
//! types for LightPlayer 2025 (Texture / Shader / Output /
//! Fixture). The protocol envelope lives in `lpc-wire`.

#![no_std]

extern crate alloc;

pub mod glsl_opts;
pub mod nodes;
pub mod project;

pub use nodes::{NodeConfig, NodeKind};
pub use project::{
    NodeChange, NodeDetail, NodeState, ProjectResponse, SerializableNodeDetail,
    SerializableProjectResponse,
};

pub type LegacyMessage = lpc_wire::Message<SerializableProjectResponse>;
pub type LegacyServerMessage = lpc_wire::ServerMessage<SerializableProjectResponse>;
pub type LegacyServerMsgBody = lpc_wire::server::ServerMsgBody<SerializableProjectResponse>;

#[cfg(test)]
mod legacy_message_tests {
    use super::{LegacyMessage, LegacyServerMessage};
    use lpc_model::AsLpPathBuf;
    use lpc_wire::message::{ClientMessage, ClientRequest};
    use lpc_wire::server::ServerMsgBody as ServerMessagePayload;
    use lpc_wire::server::{FsRequest, FsResponse};

    #[test]
    fn test_message_serialization() {
        let client_msg = ClientMessage {
            id: 1,
            msg: ClientRequest::Filesystem(FsRequest::Read {
                path: "/project.json".as_path_buf(),
            }),
        };
        let message = LegacyMessage::Client(client_msg);
        let json = lpc_wire::json::to_string(&message).unwrap();
        let deserialized: LegacyMessage = lpc_wire::json::from_str(&json).unwrap();
        match deserialized {
            LegacyMessage::Client(ClientMessage { id, msg }) => {
                assert_eq!(id, 1);
                match msg {
                    ClientRequest::Filesystem(FsRequest::Read { path }) => {
                        assert_eq!(path.as_str(), "/project.json");
                    }
                    _ => panic!("Wrong request type"),
                }
            }
            _ => panic!("Wrong message direction"),
        }
    }

    #[test]
    fn test_server_message_serialization() {
        let server_msg = LegacyServerMessage {
            id: 1,
            msg: ServerMessagePayload::Filesystem(FsResponse::Read {
                path: "/project.json".as_path_buf(),
                data: Some(b"{}".to_vec()),
                error: None,
            }),
        };
        let message = LegacyMessage::Server(server_msg);
        let json = lpc_wire::json::to_string(&message).unwrap();
        let deserialized: LegacyMessage = lpc_wire::json::from_str(&json).unwrap();
        match deserialized {
            LegacyMessage::Server(LegacyServerMessage { id, msg }) => {
                assert_eq!(id, 1);
                match msg {
                    ServerMessagePayload::Filesystem(FsResponse::Read { path, data, error }) => {
                        assert_eq!(path.as_str(), "/project.json");
                        assert_eq!(data, Some(b"{}".to_vec()));
                        assert_eq!(error, None);
                    }
                    _ => panic!("Wrong response type"),
                }
            }
            _ => panic!("Wrong message direction"),
        }
    }
}
