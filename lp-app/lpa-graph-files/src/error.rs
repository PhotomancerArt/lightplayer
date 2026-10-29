//! What went wrong opening or saving, in words: which file, and why.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilesError {
    /// The file it is about (project-relative), or empty for the project.
    pub path: String,
    pub what: String,
}

impl FilesError {
    pub fn new(path: &str, what: impl Into<String>) -> Self {
        Self {
            path: path.to_string(),
            what: what.into(),
        }
    }
}

impl std::fmt::Display for FilesError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.path.is_empty() {
            f.write_str(&self.what)
        } else {
            write!(f, "{}: {}", self.path, self.what)
        }
    }
}

impl std::error::Error for FilesError {}
