//! Why a lookup path does not name anything the store could hold.

use core::fmt;

/// Why `/firmware/<target>/<release>/<file>` was refused by the grammar.
///
/// Every variant is a 404 at the route, answered without asking upstream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LookupError {
    /// Not `/firmware/` followed by exactly three non-empty segments.
    NotALookupPath,
    /// The target is outside `[a-z0-9][a-z0-9-]{0,63}`.
    BadTarget,
    /// The release is a dev version or a dev build id: never in the store.
    DevBuild,
    /// The release starts with a digit but is neither a release version nor
    /// a build id with exactly 12 hex digits.
    BadRelease,
    /// The file name is outside `[A-Za-z0-9][A-Za-z0-9._-]{0,127}` or holds `..`.
    BadFile,
}

impl LookupError {
    /// A one-line reason, fit for a plain-text 404 body.
    pub fn reason(self) -> &'static str {
        match self {
            Self::NotALookupPath => "not a firmware lookup path",
            Self::BadTarget => "no such target",
            Self::DevBuild => "dev builds are not in the store",
            Self::BadRelease => "no such release",
            Self::BadFile => "no such file",
        }
    }
}

impl fmt::Display for LookupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.reason())
    }
}

impl core::error::Error for LookupError {}
