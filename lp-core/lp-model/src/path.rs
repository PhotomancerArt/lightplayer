use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::convert::AsRef;
use core::fmt;
use core::ops::Deref;
use serde::{Deserialize, Serialize};

/// Light Player path slice - borrowed view of a path (like `Path`).
///
/// This is an unsized type that provides a view into path data stored elsewhere.
/// Use `LpPathBuf` for owned paths.
#[repr(transparent)]
pub struct LpPath(str);

impl LpPath {
    /// Directly wraps a string slice as a `LpPath` slice.
    ///
    /// This is a cost-free conversion. No normalization is performed.
    ///
    /// # Examples
    ///
    /// ```
    /// use lp_model::path::LpPath;
    ///
    /// let path = LpPath::new("foo.txt");
    /// ```
    pub fn new(s: &str) -> &LpPath {
        unsafe { &*(s as *const str as *const LpPath) }
    }

    /// Get the path as a string slice
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Check if the path is absolute (starts with `/`)
    pub fn is_absolute(&self) -> bool {
        self.0.starts_with('/')
    }

    /// Check if the path is relative (!starts with `/`)
    pub fn is_relative(&self) -> bool {
        !self.0.starts_with('/')
    }

    /// Convert to an owned `LpPathBuf`
    ///
    /// This is equivalent to `LpPathBuf::from(self)` but may be more readable.
    /// The path will be normalized during conversion.
    ///
    /// # Examples
    ///
    /// ```
    /// use lp_model::path::{LpPath, LpPathBuf};
    ///
    /// let path = LpPath::new("/foo/bar");
    /// let owned = path.to_path_buf();
    /// ```
    pub fn to_path_buf(&self) -> LpPathBuf {
        LpPathBuf::from(self.as_str())
    }

    /// Get the parent directory path
    ///
    /// Returns `None` if the path is root (`/`) or empty, or if there's no parent component.
    /// Returns a borrowed view into the original path.
    pub fn parent(&self) -> Option<&LpPath> {
        if self.as_str() == "/" || self.as_str().is_empty() {
            return None;
        }

        // Find the last `/`
        if let Some(last_slash) = self.0.rfind('/') {
            if last_slash == 0 {
                // Only root `/` remains
                Some(LpPath::new("/"))
            } else {
                // Return path up to (but not including) last `/`
                Some(LpPath::new(&self.0[..last_slash]))
            }
        } else {
            // No `/` found, check if there's a parent component
            // For relative paths like "src/test", parent is "src"
            // For relative paths like "test", there's no parent
            if self.0.contains('/') {
                // This shouldn't happen after normalization, but handle it
                if let Some(last_slash) = self.0.rfind('/') {
                    if last_slash == 0 {
                        Some(LpPath::new("/"))
                    } else {
                        Some(LpPath::new(&self.0[..last_slash]))
                    }
                } else {
                    None
                }
            } else {
                // Single component relative path, no parent
                None
            }
        }
    }

    /// Get the last component (file name)
    ///
    /// Returns `None` if the path is root (`/`) or empty.
    pub fn file_name(&self) -> Option<&str> {
        let s = self.as_str();
        if s == "/" || s.is_empty() {
            return None;
        }

        // Find the last `/`
        if let Some(last_slash) = s.rfind('/') {
            if last_slash == s.len() - 1 {
                // Trailing `/`, this shouldn't happen after normalization, but handle it
                // Find the component before the trailing slash
                let trimmed = &s[..last_slash];
                if trimmed.is_empty() || trimmed == "/" {
                    None
                } else if let Some(prev_slash) = trimmed.rfind('/') {
                    Some(&trimmed[prev_slash + 1..])
                } else {
                    Some(trimmed)
                }
            } else {
                Some(&s[last_slash + 1..])
            }
        } else {
            // No `/` found, entire path is the file name
            Some(s)
        }
    }

    /// Get the file stem (file name without extension)
    ///
    /// Returns `None` if there is no file name or no extension.
    pub fn file_stem(&self) -> Option<&str> {
        self.file_name().and_then(|name| {
            if let Some(dot_pos) = name.rfind('.') {
                if dot_pos > 0 {
                    Some(&name[..dot_pos])
                } else {
                    // File name starts with `.`, no stem
                    None
                }
            } else {
                // No extension, entire name is the stem
                Some(name)
            }
        })
    }

    /// Get the file extension (without leading dot)
    ///
    /// Returns `None` if there is no file name or no extension.
    pub fn extension(&self) -> Option<&str> {
        self.file_name().and_then(|name| {
            if let Some(dot_pos) = name.rfind('.') {
                if dot_pos < name.len() - 1 {
                    Some(&name[dot_pos + 1..])
                } else {
                    // Trailing `.`, no extension
                    None
                }
            } else {
                None
            }
        })
    }

    /// Strip a prefix from this path
    ///
    /// Returns `None` if the prefix doesn't match.
    /// Returns a borrowed view into the original path.
    pub fn strip_prefix<P: AsRef<str>>(&self, prefix: P) -> Option<&LpPath> {
        let prefix_str = prefix.as_ref();
        let prefix_path = LpPath::new(prefix_str);

        if !self.starts_with(prefix_str) {
            return None;
        }

        // Get components after prefix
        let self_components: Vec<&str> = self.components().collect();
        let prefix_components: Vec<&str> = prefix_path.components().collect();

        if prefix_components.len() > self_components.len() {
            return None;
        }

        if prefix_components.len() == self_components.len() {
            // Prefix matches entire path
            return Some(LpPath::new(if self.is_absolute() { "/" } else { "." }));
        }

        // Find where the prefix components end in the original string
        // We need to find the position after the last component of the prefix
        let s = self.as_str();
        let mut pos = if self.is_absolute() { 1 } else { 0 };
        for (i, component) in self_components.iter().enumerate() {
            if i < prefix_components.len() {
                // Skip this component, it's part of the prefix
                pos += component.len();
                if i < prefix_components.len() - 1 {
                    pos += 1; // Skip the `/`
                }
            } else {
                // This is the first component after the prefix
                break;
            }
        }

        // Create view from the remaining position
        // For absolute paths, ensure we have a leading slash
        if self.is_absolute() {
            if pos == 1 {
                // Only root was stripped
                Some(LpPath::new(&s[pos - 1..]))
            } else {
                Some(LpPath::new(&s[pos..]))
            }
        } else {
            Some(LpPath::new(&s[pos..]))
        }
    }

    /// Check if this path starts with the given base path
    ///
    /// Only considers whole path components to match.
    pub fn starts_with<P: AsRef<str>>(&self, base: P) -> bool {
        let base_str = base.as_ref();
        let base_path = LpPath::new(base_str);

        let self_components: Vec<&str> = self.components().collect();
        let base_components: Vec<&str> = base_path.components().collect();

        if base_components.len() > self_components.len() {
            return false;
        }

        self_components[..base_components.len()] == base_components[..]
    }

    /// Check if this path ends with the given child path
    ///
    /// Only considers whole path components to match.
    pub fn ends_with<P: AsRef<str>>(&self, child: P) -> bool {
        let child_str = child.as_ref();
        let child_path = LpPath::new(child_str);

        let self_components: Vec<&str> = self.components().collect();
        let child_components: Vec<&str> = child_path.components().collect();

        if child_components.len() > self_components.len() {
            return false;
        }

        let start_idx = self_components.len() - child_components.len();
        self_components[start_idx..] == child_components[..]
    }

    /// Check if this path ends with the given string suffix
    ///
    /// This is a string-based check (unlike `ends_with` which checks path components).
    /// Useful for checking file extensions or specific file names.
    ///
    /// # Examples
    ///
    /// ```
    /// use lp_model::path::LpPath;
    ///
    /// let path = LpPath::new("/src/my-shader.shader");
    /// assert!(path.has_suffix(".shader"));
    /// assert!(path.has_suffix("shader"));
    ///
    /// let file_path = LpPath::new("/src/my-shader.shader/node.json");
    /// assert!(file_path.has_suffix("/node.json"));
    /// assert!(file_path.has_suffix("node.json"));
    /// ```
    pub fn has_suffix<S: AsRef<str>>(&self, suffix: S) -> bool {
        self.as_str().ends_with(suffix.as_ref())
    }

    /// Get an iterator over path components
    ///
    /// Skips root `/` for absolute paths and empty components.
    pub fn components(&self) -> Components<'_> {
        Components {
            path: &self.0,
            start: if self.0.starts_with('/') { 1 } else { 0 },
        }
    }
}

impl fmt::Debug for LpPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_str(), f)
    }
}

impl PartialEq for LpPath {
    fn eq(&self, other: &LpPath) -> bool {
        self.as_str() == other.as_str()
    }
}

impl Eq for LpPath {}

/// Light Player path buffer - owned, mutable path (like `PathBuf`).
///
/// Supports both absolute (starting with `/`) and relative paths.
/// Paths are automatically normalized on construction.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LpPathBuf(String);

impl LpPathBuf {
    /// Create a new LpPath, normalizing the path
    pub fn new(path: String) -> Self {
        Self(normalize(&path))
    }

    /// Get the path as a string slice
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Get as &LpPath
    pub fn as_path(&self) -> &LpPath {
        LpPath::new(&self.0)
    }

    /// Join a path to this path
    ///
    /// Matches PathBuf::join behavior:
    /// - If `path` is absolute, replace base path
    /// - If `path` is relative, append to base (does NOT resolve `..` components)
    /// - Normalizes result
    pub fn join<P: AsRef<str>>(&self, path: P) -> LpPathBuf {
        let path_str = path.as_ref();
        if path_str.starts_with('/') {
            // Absolute path, replace base
            LpPathBuf::from(path_str)
        } else {
            // Relative path, append to base
            if self.0 == "/" {
                LpPathBuf::from(format!("/{path_str}"))
            } else {
                LpPathBuf::from(format!("{}/{}", self.0, path_str))
            }
        }
    }

    /// Join and resolve a relative path
    ///
    /// Convenience method beyond PathBuf API:
    /// - Similar to `join()` but resolves `.` and `..` components
    /// - Returns `None` if result would be invalid (e.g., goes above root for absolute paths)
    pub fn join_relative<P: AsRef<str>>(&self, path: P) -> Option<LpPathBuf> {
        let path_str = path.as_ref();
        if path_str.starts_with('/') {
            // Absolute path, just normalize
            return Some(LpPathBuf::from(path_str));
        }

        // Split into components
        let mut components: Vec<&str> = self
            .components()
            .collect::<Vec<_>>()
            .iter()
            .copied()
            .collect();

        // Add relative path components
        let relative_components: Vec<&str> = path_str.split('/').collect();
        for component in relative_components {
            match component {
                "." => {
                    // Current directory - no change
                }
                ".." => {
                    // Parent directory - remove last component
                    if components.is_empty() {
                        // Going above root for absolute path
                        if self.is_absolute() {
                            return None;
                        }
                        // For relative paths, allow going "up"
                    } else {
                        components.pop();
                    }
                }
                "" => {
                    // Empty component (e.g., leading/trailing slash) - ignore
                }
                name => {
                    // Regular component - add it
                    components.push(name);
                }
            }
        }

        // Reconstruct path
        let resolved_path = if components.is_empty() {
            if self.is_absolute() {
                "/".to_string()
            } else {
                ".".to_string()
            }
        } else if self.is_absolute() {
            format!("/{}", components.join("/"))
        } else {
            components.join("/")
        };

        Some(LpPathBuf::from(resolved_path))
    }
}

impl From<String> for LpPathBuf {
    fn from(s: String) -> Self {
        Self(normalize(&s))
    }
}

impl From<&str> for LpPathBuf {
    fn from(s: &str) -> Self {
        Self(normalize(s))
    }
}

impl From<&LpPath> for LpPathBuf {
    fn from(path: &LpPath) -> Self {
        Self(normalize(path.as_str()))
    }
}

// AsRef implementations for LpPath

impl AsRef<str> for LpPath {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl AsRef<LpPath> for &str {
    fn as_ref(&self) -> &LpPath {
        LpPath::new(self)
    }
}

impl AsRef<LpPath> for String {
    fn as_ref(&self) -> &LpPath {
        LpPath::new(self.as_str())
    }
}

impl AsRef<LpPath> for &LpPath {
    fn as_ref(&self) -> &LpPath {
        self
    }
}

impl AsRef<LpPath> for LpPathBuf {
    fn as_ref(&self) -> &LpPath {
        self.deref()
    }
}

impl Deref for LpPathBuf {
    type Target = LpPath;

    fn deref(&self) -> &LpPath {
        LpPath::new(&self.0)
    }
}

impl fmt::Display for LpPathBuf {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Trait for ergonomic conversion to `&LpPath`
///
/// Provides `.as_path()` method for common path-like types, making it easier
/// to convert strings to `&LpPath` for use with `LpFs` trait methods.
///
/// # Examples
///
/// ```
/// use lp_model::path::AsLpPath;
///
/// let path_str = "/test.txt";
/// let path = path_str.as_path();
/// ```
pub trait AsLpPath {
    /// Convert this value to `&LpPath`
    fn as_path(&self) -> &LpPath;
}

impl AsLpPath for &str {
    fn as_path(&self) -> &LpPath {
        LpPath::new(self)
    }
}

impl AsLpPath for String {
    fn as_path(&self) -> &LpPath {
        LpPath::new(self.as_str())
    }
}

impl AsLpPath for LpPathBuf {
    fn as_path(&self) -> &LpPath {
        self.deref()
    }
}

impl AsLpPath for &LpPath {
    fn as_path(&self) -> &LpPath {
        self
    }
}

/// Trait for ergonomic conversion to `LpPathBuf`
///
/// Provides `.as_path_buf()` method for common path-like types, making it easier
/// to convert strings to `LpPathBuf` for owned path operations.
///
/// # Examples
///
/// ```
/// use lp_model::path::AsLpPathBuf;
///
/// let path_str = "/test.txt";
/// let path_buf = path_str.as_path_buf();
/// ```
pub trait AsLpPathBuf {
    /// Convert this value to `LpPathBuf`
    fn as_path_buf(&self) -> LpPathBuf;
}

impl AsLpPathBuf for &str {
    fn as_path_buf(&self) -> LpPathBuf {
        LpPathBuf::from(*self)
    }
}

impl AsLpPathBuf for String {
    fn as_path_buf(&self) -> LpPathBuf {
        LpPathBuf::from(self.as_str())
    }
}

impl AsLpPathBuf for LpPathBuf {
    fn as_path_buf(&self) -> LpPathBuf {
        self.clone()
    }
}

impl AsLpPathBuf for &LpPath {
    fn as_path_buf(&self) -> LpPathBuf {
        self.to_path_buf()
    }
}

/// Iterator over path components
pub struct Components<'a> {
    path: &'a str,
    start: usize,
}

impl<'a> Iterator for Components<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        if self.start >= self.path.len() {
            return None;
        }

        let remaining = &self.path[self.start..];
        if let Some(slash_pos) = remaining.find('/') {
            if slash_pos == 0 {
                // Skip empty component
                self.start += 1;
                return self.next();
            }
            let component = &remaining[..slash_pos];
            self.start += slash_pos + 1;
            Some(component)
        } else {
            // Last component
            if remaining.is_empty() {
                None
            } else {
                let component = remaining;
                self.start = self.path.len();
                Some(component)
            }
        }
    }
}

/// Normalize a path string
///
/// Normalization rules:
/// - Trim whitespace
/// - Remove leading `./` or `.` (if present)
/// - For absolute paths: ensure leading `/`
/// - For relative paths: keep as-is (no leading `/`)
/// - Collapse multiple consecutive slashes (`//` → `/`)
/// - Remove trailing `/` unless it's the root path (`/`)
/// - Handle empty paths: `""` → `"/"` (absolute root)
fn normalize(path: &str) -> String {
    let mut normalized = path.trim();

    // Remove leading "./" or "."
    if normalized.starts_with("./") {
        normalized = &normalized[2..];
    } else if normalized == "." {
        normalized = "";
    }

    // Handle empty path
    let normalized = if normalized.is_empty() {
        "/".to_string()
    } else {
        normalized.to_string()
    };

    // Collapse multiple slashes
    let normalized = normalized.replace("//", "/");

    // Remove trailing "/" unless it's the root
    if normalized.len() > 1 && normalized.ends_with('/') {
        normalized[..normalized.len() - 1].to_string()
    } else {
        normalized
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lp_path_creation() {
        let path = LpPathBuf::new("/src/test.texture".to_string());
        assert_eq!(path.as_str(), "/src/test.texture");
    }

    #[test]
    fn test_lp_path_from_string() {
        let path = LpPathBuf::from("/src/test.shader".to_string());
        assert_eq!(path.as_str(), "/src/test.shader");
    }

    #[test]
    fn test_lp_path_from_str() {
        let path = LpPathBuf::from("/src/test.output");
        assert_eq!(path.as_str(), "/src/test.output");
    }

    #[test]
    fn test_normalization_absolute() {
        assert_eq!(LpPathBuf::from("//src//test//").as_str(), "/src/test");
        assert_eq!(LpPathBuf::from("/src/test/").as_str(), "/src/test");
        assert_eq!(LpPathBuf::from("  /src/test  ").as_str(), "/src/test");
    }

    #[test]
    fn test_normalization_relative() {
        assert_eq!(LpPathBuf::from("src/test").as_str(), "src/test");
        assert_eq!(LpPathBuf::from("./src/test").as_str(), "src/test");
        assert_eq!(LpPathBuf::from(".").as_str(), "/");
        assert_eq!(LpPathBuf::from("").as_str(), "/");
    }

    #[test]
    fn test_is_absolute() {
        // Test via Deref - LpPathBuf can use LpPath methods directly
        assert!(LpPathBuf::from("/src/test").is_absolute());
        assert!(!LpPathBuf::from("src/test").is_absolute());
        assert!(LpPathBuf::from("/").is_absolute());
    }

    #[test]
    fn test_is_relative() {
        // Test via Deref - LpPathBuf can use LpPath methods directly
        assert!(!LpPathBuf::from("/src/test").is_relative());
        assert!(LpPathBuf::from("src/test").is_relative());
        assert!(!LpPathBuf::from("/").is_relative());
    }

    #[test]
    fn test_parent() {
        assert_eq!(
            LpPathBuf::from("/src/test")
                .as_path()
                .parent()
                .map(|p| p.as_str()),
            Some("/src")
        );
        assert_eq!(
            LpPathBuf::from("/src")
                .as_path()
                .parent()
                .map(|p| p.as_str()),
            Some("/")
        );
        assert_eq!(LpPathBuf::from("/").as_path().parent(), None);
        assert_eq!(
            LpPathBuf::from("src/test")
                .as_path()
                .parent()
                .map(|p| p.as_str()),
            Some("src")
        );
        assert_eq!(LpPathBuf::from("test").as_path().parent(), None);
    }

    #[test]
    fn test_file_name() {
        // Test via Deref
        assert_eq!(
            LpPathBuf::from("/src/test.txt").file_name(),
            Some("test.txt")
        );
        assert_eq!(LpPathBuf::from("/src/test").file_name(), Some("test"));
        assert_eq!(LpPathBuf::from("/").file_name(), None);
        assert_eq!(LpPathBuf::from("test.txt").file_name(), Some("test.txt"));
    }

    #[test]
    fn test_file_stem() {
        // Test via Deref
        assert_eq!(LpPathBuf::from("/src/test.txt").file_stem(), Some("test"));
        assert_eq!(
            LpPathBuf::from("/src/test.tar.gz").file_stem(),
            Some("test.tar")
        );
        assert_eq!(LpPathBuf::from("/src/test").file_stem(), Some("test"));
        assert_eq!(LpPathBuf::from("/src/.hidden").file_stem(), None);
        assert_eq!(LpPathBuf::from("/").file_stem(), None);
    }

    #[test]
    fn test_extension() {
        // Test via Deref
        assert_eq!(LpPathBuf::from("/src/test.txt").extension(), Some("txt"));
        assert_eq!(LpPathBuf::from("/src/test.tar.gz").extension(), Some("gz"));
        assert_eq!(LpPathBuf::from("/src/test").extension(), None);
        assert_eq!(LpPathBuf::from("/src/test.").extension(), None);
        assert_eq!(LpPathBuf::from("/").extension(), None);
    }

    #[test]
    fn test_join() {
        assert_eq!(LpPathBuf::from("/src").join("test").as_str(), "/src/test");
        assert_eq!(LpPathBuf::from("/src").join("/test").as_str(), "/test");
        assert_eq!(
            LpPathBuf::from("/src/a").join("../b").as_str(),
            "/src/a/../b"
        );
        assert_eq!(LpPathBuf::from("/").join("test").as_str(), "/test");
        assert_eq!(LpPathBuf::from("src").join("test").as_str(), "src/test");
    }

    #[test]
    fn test_join_relative() {
        assert_eq!(
            LpPathBuf::from("/src/a").join_relative("../b"),
            Some(LpPathBuf::from("/src/b"))
        );
        assert_eq!(LpPathBuf::from("/src").join_relative("../../root"), None);
        assert_eq!(
            LpPathBuf::from("/src").join_relative("./test"),
            Some(LpPathBuf::from("/src/test"))
        );
        assert_eq!(
            LpPathBuf::from("src/a").join_relative("../b"),
            Some(LpPathBuf::from("src/b"))
        );
    }

    #[test]
    fn test_strip_prefix() {
        assert_eq!(
            LpPathBuf::from("/projects/my-project/src")
                .as_path()
                .strip_prefix("/projects/my-project")
                .map(|p| p.as_str()),
            Some("/src")
        );
        assert!(
            LpPathBuf::from("/src")
                .as_path()
                .strip_prefix("/projects")
                .is_none()
        );
        assert_eq!(
            LpPathBuf::from("/projects/my-project")
                .as_path()
                .strip_prefix("/projects/my-project")
                .map(|p| p.as_str()),
            Some("/")
        );
    }

    #[test]
    fn test_starts_with() {
        assert!(LpPathBuf::from("/etc/passwd").as_path().starts_with("/etc"));
        assert!(
            LpPathBuf::from("/etc/passwd")
                .as_path()
                .starts_with("/etc/")
        );
        assert!(!LpPathBuf::from("/etc/passwd").as_path().starts_with("/usr"));
        assert!(
            !LpPathBuf::from("/etc/foo.rs")
                .as_path()
                .starts_with("/etc/foo")
        );
    }

    #[test]
    fn test_ends_with() {
        assert!(
            LpPathBuf::from("/etc/resolv.conf")
                .as_path()
                .ends_with("resolv.conf")
        );
        assert!(
            LpPathBuf::from("/etc/resolv.conf")
                .as_path()
                .ends_with("etc/resolv.conf")
        );
        assert!(
            LpPathBuf::from("/etc/resolv.conf")
                .as_path()
                .ends_with("/etc/resolv.conf")
        );
        // /resolv.conf normalized is absolute, but we compare components
        // So /etc/resolv.conf ends with resolv.conf component
        assert!(
            LpPathBuf::from("/etc/resolv.conf")
                .as_path()
                .ends_with("/resolv.conf")
        );
        assert!(
            !LpPathBuf::from("/etc/resolv.conf")
                .as_path()
                .ends_with("conf")
        );
    }

    #[test]
    fn test_components() {
        let path1 = LpPathBuf::from("/src/test");
        let components: Vec<&str> = path1.as_path().components().collect();
        assert_eq!(components, Vec::from(["src", "test"]));

        let path2 = LpPathBuf::from("src/test");
        let components: Vec<&str> = path2.as_path().components().collect();
        assert_eq!(components, Vec::from(["src", "test"]));

        let path3 = LpPathBuf::from("/");
        let components: Vec<&str> = path3.as_path().components().collect();
        assert_eq!(components, Vec::<&str>::new());
    }
}
