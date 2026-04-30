use lpc_wire::WireProjectRequest;
use lpl_model::ProjectResponse;

/// Client API trait - implemented by server connection
pub trait ClientApi {
    /// Get changes from server
    fn get_changes(
        &self,
        request: WireProjectRequest,
    ) -> Result<ProjectResponse, alloc::string::String>;
}
