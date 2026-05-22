//! Compare committed filesystem state to a target snapshot.

use alloc::string::String;

use lpc_model::NodeDef;
use lpfs::LpFs;

use super::snapshot::ProjectSnapshot;
use crate::ParseCtx;

/// Failure while diffing or checking equivalence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiffError {
    Fs { message: String },
    Parse { message: String },
    Diff { message: String },
    Equivalent { message: String },
}

impl core::fmt::Display for DiffError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Fs { message } => write!(f, "filesystem error: {message}"),
            Self::Parse { message } => write!(f, "parse error: {message}"),
            Self::Diff { message } => write!(f, "diff error: {message}"),
            Self::Equivalent { message } => write!(f, "not equivalent: {message}"),
        }
    }
}

/// Assert `fs` matches `target` (path set, asset bytes, parsed `.toml` defs).
pub fn assert_equivalent(
    fs: &dyn LpFs,
    target: &ProjectSnapshot,
    ctx: &ParseCtx<'_>,
) -> Result<(), DiffError> {
    let actual = ProjectSnapshot::from_fs(fs)?;
    if actual.len() != target.len() {
        return Err(DiffError::Equivalent {
            message: alloc::format!(
                "path count mismatch: actual {} target {}",
                actual.len(),
                target.len()
            ),
        });
    }
    for (path, expected_bytes) in target.iter() {
        let Some(actual_bytes) = actual.get(path) else {
            return Err(DiffError::Equivalent {
                message: alloc::format!("missing path `{path}`"),
            });
        };
        if path.ends_with(".toml") {
            equivalent_toml(actual_bytes, expected_bytes, ctx, path)?;
        } else if actual_bytes != expected_bytes {
            return Err(DiffError::Equivalent {
                message: alloc::format!("byte mismatch at `{path}`"),
            });
        }
    }
    Ok(())
}

fn equivalent_toml(
    actual: &[u8],
    expected: &[u8],
    ctx: &ParseCtx<'_>,
    path: &str,
) -> Result<(), DiffError> {
    let actual_text = core::str::from_utf8(actual).map_err(|err| DiffError::Parse {
        message: alloc::format!("`{path}` utf-8: {err}"),
    })?;
    let expected_text = core::str::from_utf8(expected).map_err(|err| DiffError::Parse {
        message: alloc::format!("`{path}` utf-8: {err}"),
    })?;
    let actual_def =
        NodeDef::read_toml(ctx.shapes, actual_text).map_err(|err| DiffError::Parse {
            message: alloc::format!("`{path}`: {err}"),
        })?;
    let expected_def =
        NodeDef::read_toml(ctx.shapes, expected_text).map_err(|err| DiffError::Parse {
            message: alloc::format!("`{path}`: {err}"),
        })?;
    if actual_def != expected_def {
        return Err(DiffError::Equivalent {
            message: alloc::format!("parsed def mismatch at `{path}`"),
        });
    }
    Ok(())
}
