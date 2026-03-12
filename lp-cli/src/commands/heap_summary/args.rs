use std::path::PathBuf;

pub struct HeapSummaryArgs {
    pub trace_dir: PathBuf,
    pub top: usize,
}
