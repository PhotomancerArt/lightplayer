//! Legacy runtime state and client/server protocol payloads.

pub mod nodes;
pub mod project;

pub use project::{
    NodeChange, NodeDetail, NodeState, ProjectResponse, SerializableNodeDetail,
    SerializableProjectResponse,
};

pub type LegacyMessage = crate::Message<SerializableProjectResponse>;
pub type LegacyServerMessage = crate::ServerMessage<SerializableProjectResponse>;
pub type LegacyServerMsgBody = crate::server::ServerMsgBody<SerializableProjectResponse>;

#[cfg(test)]
mod legacy_message_tests {
    use super::{LegacyMessage, LegacyServerMessage};
    use crate::message::{ClientMessage, ClientRequest};
    use crate::server::ServerMsgBody as ServerMessagePayload;
    use crate::server::{FsRequest, FsResponse};
    use lpc_model::AsLpPathBuf;

    #[test]
    fn test_message_serialization() {
        let client_msg = ClientMessage {
            id: 1,
            msg: ClientRequest::Filesystem(FsRequest::Read {
                path: "/project.json".as_path_buf(),
            }),
        };
        let message = LegacyMessage::Client(client_msg);
        let json_s = crate::json::to_string(&message).unwrap();
        let deserialized: LegacyMessage = crate::json::from_str(&json_s).unwrap();
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
        let json_s = crate::json::to_string(&message).unwrap();
        let deserialized: LegacyMessage = crate::json::from_str(&json_s).unwrap();
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
