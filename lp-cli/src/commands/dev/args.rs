use std::path::PathBuf;

pub struct DevArgs {
    pub dir: PathBuf,
    /// If Some(Some(host)), push to the specified host
    /// If Some(None), push to local in-memory server
    /// If None, don't push
    pub push_host: Option<Option<String>>,
    /// If true, run without UI (headless mode)
    pub headless: bool,
}
