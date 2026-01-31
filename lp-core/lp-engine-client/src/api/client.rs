use lp_model::project::api::{ProjectRequest, ProjectResponse};

/// Client API trait - implemented by server connection
pub trait ClientApi {
    /// Get changes from server
    fn get_changes(
        &self,
        request: ProjectRequest,
    ) -> Result<ProjectResponse, alloc::string::String>;
}
