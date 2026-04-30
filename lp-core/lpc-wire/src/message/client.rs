//! Client → server payloads.

use crate::project::{WireProjectHandle, WireProjectRequest};
use crate::server::FsRequest;
use alloc::string::String;
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
    Filesystem(FsRequest),
    LoadProject {
        path: String,
    },
    UnloadProject {
        handle: WireProjectHandle,
    },
    ProjectRequest {
        handle: WireProjectHandle,
        request: WireProjectRequest,
    },
    ListAvailableProjects,
    ListLoadedProjects,
    StopAllProjects,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::WireNodeSpecifier;
    use lpc_model::lp_path::AsLpPathBuf;
    use lpc_model::project::FrameId;

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
    fn test_project_request() {
        let req = ClientRequest::ProjectRequest {
            handle: WireProjectHandle::new(1),
            request: WireProjectRequest::GetChanges {
                since_frame: FrameId::default(),
                detail_specifier: WireNodeSpecifier::All,
            },
        };
        let json = crate::json::to_string(&req).unwrap();
        let deserialized: ClientRequest = crate::json::from_str(&json).unwrap();
        match deserialized {
            ClientRequest::ProjectRequest { handle, request } => {
                assert_eq!(handle.id(), 1);
                match request {
                    WireProjectRequest::GetChanges {
                        since_frame,
                        detail_specifier,
                    } => {
                        assert_eq!(since_frame, FrameId::default());
                        assert_eq!(detail_specifier, WireNodeSpecifier::All);
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
