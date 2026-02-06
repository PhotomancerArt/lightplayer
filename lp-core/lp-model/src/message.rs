//! Message protocol types
//!
//! Defines the message envelope and request/response types for client-server communication.

use crate::project::{api::ProjectRequest, handle::ProjectHandle};
use crate::server::{FsRequest, ServerMsgBody as ServerMessagePayload};
use alloc::string::String;
use serde::{Deserialize, Serialize};

/// Top-level message envelope
///
/// Messages are wrapped in this enum to distinguish between client and server messages.
/// Note: Cannot derive Clone because ServerMessage contains non-cloneable types.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Message {
    /// Message from client to server
    Client(ClientMessage),
    /// Message from server to client
    Server(ServerMessage),
}

/// Client message with request ID
///
/// Wraps a client request with an ID for request/response correlation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientMessage {
    /// Request ID for correlating requests and responses
    pub id: u64,
    /// The request payload
    pub msg: ClientRequest,
}

/// Server message with request ID
///
/// Wraps a server response with an ID matching the original request.
///
/// Note: Cannot derive Clone because ServerResponse contains non-cloneable types
/// (specifically ProjectResponse which contains NodeDetail with trait objects).
#[derive(Debug, Serialize, Deserialize)]
pub struct ServerMessage {
    /// Request ID matching the original client request
    pub id: u64,
    /// The response payload
    pub msg: ServerMessagePayload,
}

/// Client request types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ClientRequest {
    /// Filesystem operation request
    Filesystem(FsRequest),
    /// Load a project
    LoadProject { path: String },
    /// Unload a project
    UnloadProject { handle: ProjectHandle },
    /// Project-specific request
    ProjectRequest {
        handle: ProjectHandle,
        request: ProjectRequest,
    },
    /// List available projects
    ListAvailableProjects,
    /// List loaded projects
    ListLoadedProjects,
    /// Stop all loaded projects
    StopAllProjects,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AsLpPathBuf;
    use crate::server::FsResponse;
    use alloc::string::ToString;

    #[test]
    fn test_message_serialization() {
        let client_msg = ClientMessage {
            id: 1,
            msg: ClientRequest::Filesystem(FsRequest::Read {
                path: "/project.json".as_path_buf(),
            }),
        };
        let message = Message::Client(client_msg);
        let json = crate::json::to_string(&message).unwrap();
        // Verify round-trip serialization
        let deserialized: Message = crate::json::from_str(&json).unwrap();
        match deserialized {
            Message::Client(ClientMessage { id, msg }) => {
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
        use crate::server::ServerMsgBody as ServerMessagePayload;
        let server_msg = ServerMessage {
            id: 1,
            msg: ServerMessagePayload::Filesystem(FsResponse::Read {
                path: "/project.json".as_path_buf(),
                data: Some(b"{}".to_vec()),
                error: None,
            }),
        };
        let message = Message::Server(server_msg);
        let json = crate::json::to_string(&message).unwrap();
        // Verify round-trip serialization
        let deserialized: Message = crate::json::from_str(&json).unwrap();
        match deserialized {
            Message::Server(ServerMessage { id, msg }) => {
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

    #[test]
    fn test_nested_filesystem_request() {
        let req = ClientRequest::Filesystem(FsRequest::Write {
            path: "/test.txt".as_path_buf(),
            data: b"hello".to_vec(),
        });
        let json = crate::json::to_string(&req).unwrap();
        // Verify round-trip serialization
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
            handle: ProjectHandle::new(1),
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
    fn test_project_request() {
        use crate::project::FrameId;
        use crate::project::api::ApiNodeSpecifier;
        let req = ClientRequest::ProjectRequest {
            handle: ProjectHandle::new(1),
            request: ProjectRequest::GetChanges {
                since_frame: FrameId::default(),
                detail_specifier: ApiNodeSpecifier::All,
            },
        };
        let json = crate::json::to_string(&req).unwrap();
        let deserialized: ClientRequest = crate::json::from_str(&json).unwrap();
        match deserialized {
            ClientRequest::ProjectRequest { handle, request } => {
                assert_eq!(handle.id(), 1);
                match request {
                    ProjectRequest::GetChanges {
                        since_frame,
                        detail_specifier,
                    } => {
                        assert_eq!(since_frame, FrameId::default());
                        assert_eq!(detail_specifier, ApiNodeSpecifier::All);
                    }
                }
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
