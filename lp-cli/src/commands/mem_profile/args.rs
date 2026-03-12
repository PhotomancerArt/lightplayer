use std::path::PathBuf;

pub struct MemProfileArgs {
    pub dir: PathBuf,
    pub frames: u32,
    pub note: Option<String>,
}
