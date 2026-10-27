//! Reading a format-11 project's files by the paths its documents name.
//!
//! Node `ref`s and asset `source`s are relative to the file that names them
//! (`./dome/module.json` from `/module.json`); a leading `/` is the project
//! root. [`ProjectFiles`] keys may or may not carry the leading slash, so a
//! lookup tries both.

use crate::json::JsonNode;
use crate::project_files::ProjectFiles;
use crate::upgrade_error::UpgradeError;

/// `relative` resolved against the directory of `from` (both project paths
/// with a leading `/`), `.` and `..` folded away.
pub(crate) fn resolve_path(from: &str, relative: &str) -> Result<String, UpgradeError> {
    let mut segments: Vec<&str> = Vec::new();
    if !relative.starts_with('/') {
        segments.extend(from.split('/').filter(|segment| !segment.is_empty()));
        segments.pop();
    }
    for segment in relative.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                if segments.pop().is_none() {
                    return Err(refused(
                        from,
                        &format!("names {relative:?}, which leaves the project"),
                    ));
                }
            }
            segment => segments.push(segment),
        }
    }
    Ok(format!("/{}", segments.join("/")))
}

/// The text of the file at project path `path` (leading `/`).
pub(crate) fn read_text<'a>(
    files: &'a ProjectFiles,
    path: &str,
    named_by: &str,
) -> Result<&'a str, UpgradeError> {
    let bare = path.trim_start_matches('/');
    let bytes = files.get(bare).or_else(|| files.get(path)).ok_or_else(|| {
        refused(
            named_by,
            &format!("names {path}, which is not in the project"),
        )
    })?;
    core::str::from_utf8(bytes).map_err(|_| UpgradeError::Malformed {
        file: path.to_string(),
        detail: String::from("not UTF-8"),
    })
}

/// The JSON document at project path `path`.
pub(crate) fn read_json(
    files: &ProjectFiles,
    path: &str,
    named_by: &str,
) -> Result<JsonNode, UpgradeError> {
    let text = read_text(files, path, named_by)?;
    JsonNode::parse(text).map_err(|error| UpgradeError::Malformed {
        file: path.to_string(),
        detail: error.detail,
    })
}

/// A refusal naming `file`, for a shape the frozen landing does not read.
pub(crate) fn refused(file: &str, reason: &str) -> UpgradeError {
    UpgradeError::Refused {
        file: file.trim_start_matches('/').to_string(),
        reason: reason.to_string(),
    }
}

/// A scalar read through the same serde path the typed model's derive takes
/// (`serde_json`'s own number parsing), so a value means here exactly what
/// it means to the engine.
pub(crate) fn scalar<T: serde::de::DeserializeOwned>(node: &JsonNode) -> Option<T> {
    let JsonNode::Scalar(text) = node else {
        return None;
    };
    serde_json::from_str(text).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ref_resolves_against_the_naming_files_directory() {
        assert_eq!(
            resolve_path("/module.json", "./dome/module.json").unwrap(),
            "/dome/module.json"
        );
        assert_eq!(
            resolve_path("/dome/dome.json", "dome.map2d.json").unwrap(),
            "/dome/dome.map2d.json"
        );
        assert_eq!(
            resolve_path("/a/b/module.json", "../c.json").unwrap(),
            "/a/c.json"
        );
        assert_eq!(resolve_path("/a/x.json", "/y.json").unwrap(), "/y.json");
    }

    #[test]
    fn a_ref_that_leaves_the_project_is_refused() {
        assert!(resolve_path("/module.json", "../outside.json").is_err());
    }

    #[test]
    fn files_are_found_with_or_without_a_leading_slash() {
        let files: ProjectFiles = [("module.json", b"{}".to_vec())].into_iter().collect();
        assert_eq!(read_text(&files, "/module.json", "x").unwrap(), "{}");
        let rooted: ProjectFiles = [("/module.json", b"{}".to_vec())].into_iter().collect();
        assert_eq!(read_text(&rooted, "/module.json", "x").unwrap(), "{}");
    }
}
