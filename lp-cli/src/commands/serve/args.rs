use std::path::PathBuf;

pub struct ServeArgs {
    pub dir: Option<PathBuf>,
    pub init: bool,
    pub memory: bool,
    /// Put the host board on the cloud relay at this origin
    /// (`https://lightplayer.app`, or `http://127.0.0.1:<port>` for a local
    /// `lp-cloud-server`).
    pub relay: Option<String>,
}
