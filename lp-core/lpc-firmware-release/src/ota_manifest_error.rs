//! Why an `ota-manifest.json` was refused.

use alloc::string::String;
use core::fmt;

/// Why `ota-manifest.json` could not be read, or failed [`validate`].
///
/// [`validate`]: crate::OtaManifest::validate
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OtaManifestError {
    /// Not JSON, or not the format-1 shape.
    Json(String),
    /// No integer `format` key.
    MissingFormat,
    /// A `format` this reader does not read.
    UnsupportedFormat(u64),
    /// `target` is outside `[a-z0-9][a-z0-9-]{0,63}`.
    BadTarget(String),
    /// `chip` is empty or not lowercase ASCII.
    BadChip(String),
    /// `version` is neither a release nor a dev version.
    BadVersion(String),
    /// `commit` is not 40 lowercase hex.
    BadCommit(String),
    /// A `sha256` is not 64 lowercase hex.
    BadSha256 {
        /// Which field.
        field: &'static str,
    },
    /// A file name is outside the lookup grammar, or is `ota-manifest.json`.
    BadFileName(String),
    /// Two files share a name.
    DuplicateFile(String),
    /// A known encoding's entry does not have its shape.
    MalformedEncoding {
        /// The encoding id.
        id: u32,
    },
    /// A known encoding says `chunkBytes: 0`.
    ZeroChunkBytes {
        /// The encoding id.
        id: u32,
    },
    /// `chunks` has the wrong number of entries for the piece.
    ChunkCount {
        /// Which piece's `.z` file.
        file: String,
        /// `ceil(piece length / chunkBytes)`.
        expected: u64,
        /// Entries in `chunks`.
        actual: u64,
    },
    /// The sum of `chunks` is not the `.z` file's `length`.
    ChunkSum {
        /// Which `.z` file.
        file: String,
        /// The file's `length`.
        expected: u64,
        /// The sum of `chunks`.
        actual: u64,
    },
}

impl fmt::Display for OtaManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(e) => write!(f, "ota-manifest.json does not parse: {e}"),
            Self::MissingFormat => f.write_str("ota-manifest.json has no integer `format`"),
            Self::UnsupportedFormat(n) => {
                write!(
                    f,
                    "ota-manifest.json format {n} is not read here (this reader reads format 1)"
                )
            }
            Self::BadTarget(t) => write!(f, "bad target {t:?}"),
            Self::BadChip(c) => write!(f, "bad chip {c:?}"),
            Self::BadVersion(v) => {
                write!(f, "bad version {v:?} (neither a release nor a dev version)")
            }
            Self::BadCommit(c) => write!(f, "bad commit {c:?} (want 40 lowercase hex)"),
            Self::BadSha256 { field } => write!(f, "{field} is not 64 lowercase hex"),
            Self::BadFileName(n) => write!(f, "bad file name {n:?}"),
            Self::DuplicateFile(n) => write!(f, "file {n:?} is named twice"),
            Self::MalformedEncoding { id } => write!(f, "encoding {id} does not have its shape"),
            Self::ZeroChunkBytes { id } => write!(f, "encoding {id} has chunkBytes 0"),
            Self::ChunkCount {
                file,
                expected,
                actual,
            } => write!(
                f,
                "{file}: {actual} chunks listed, the piece has {expected}"
            ),
            Self::ChunkSum {
                file,
                expected,
                actual,
            } => write!(
                f,
                "{file}: chunks sum to {actual}, the file is {expected} bytes"
            ),
        }
    }
}

impl core::error::Error for OtaManifestError {}
