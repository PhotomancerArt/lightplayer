/// Client API trait shell retained until canonical project sync is rebuilt.
pub trait ClientApi {
    /// Project sync is intentionally unavailable until M3/M4 canonical sync/view rebuild.
    fn project_sync_disabled(&self) -> Result<(), alloc::string::String>;
}
