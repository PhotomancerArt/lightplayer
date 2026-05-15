use std::error::Error as StdError;
use std::path::PathBuf;
use std::{fmt, io};

#[derive(Debug)]
pub enum SlotShapeCodegenError {
    Io(io::Error),
    Parse {
        path: PathBuf,
        source: syn::Error,
    },
    MissingSrcDir(PathBuf),
    NonUtf8Path(PathBuf),
    DuplicateShapeIdName {
        name: String,
        first: PathBuf,
        second: PathBuf,
    },
}

impl fmt::Display for SlotShapeCodegenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "{err}"),
            Self::Parse { path, source } => {
                write!(f, "failed to parse {}: {source}", path.display())
            }
            Self::MissingSrcDir(path) => write!(
                f,
                "crate source directory does not exist: {}",
                path.display()
            ),
            Self::NonUtf8Path(path) => write!(f, "source path is not UTF-8: {}", path.display()),
            Self::DuplicateShapeIdName {
                name,
                first,
                second,
            } => write!(
                f,
                "duplicate slot shape id name {name:?}: {} and {}",
                first.display(),
                second.display()
            ),
        }
    }
}

impl StdError for SlotShapeCodegenError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            Self::Parse { source, .. } => Some(source),
            Self::MissingSrcDir(_) | Self::NonUtf8Path(_) | Self::DuplicateShapeIdName { .. } => {
                None
            }
        }
    }
}
