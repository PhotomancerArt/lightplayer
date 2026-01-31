use std::path::PathBuf;

pub struct CreateArgs {
    pub dir: PathBuf,
    pub name: Option<String>,
    pub uid: Option<String>,
}
