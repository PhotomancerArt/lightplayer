/// Client API trait shell for stateless project reads.
pub trait ClientApi {
    /// Request a stateless project read.
    fn project_read(
        &self,
        request: lpc_wire::ProjectReadRequest,
    ) -> Result<lpc_wire::ProjectReadResponse, alloc::string::String>;
}
