use std::path::PathBuf;

pub struct ServeArgs {
    pub dir: Option<PathBuf>,
    pub init: bool,
    pub memory: bool,
}
