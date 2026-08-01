use std::path::PathBuf;

pub struct UploadArgs {
    pub dir: PathBuf,
    pub host: String,
    /// Skip waiting for evidence the deployed project is running; restores
    /// the pre-P5 fire-and-forget behaviour (disconnect the instant the
    /// deploy is acked).
    pub no_wait: bool,
    /// How long to wait for running evidence before giving up and exiting
    /// nonzero, in seconds. Ignored when `no_wait` is set.
    pub wait_timeout_secs: u64,
}
