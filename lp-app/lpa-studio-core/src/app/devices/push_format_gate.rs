//! **A push to a board whose engine reads an older project format** (M3
//! P11, Q111, C44): refused before the board is touched.
//!
//! The library's projects are at this Studio's project format; a board that
//! has not been updated reads an older one, and would refuse the project only
//! after the push had stopped and cleared it. So the push asks first: the
//! project's `project.json` format against what the board's hello says its
//! engine reads. A hello that says nothing of it (a board on an older wire,
//! from before the field) reads at most the format before the field existed.

use lpa_devices::wire::HelloFacts;
use lpa_upgrade::{PROJECT_MANIFEST, ProjectFiles, stored_format};

use super::engine_project_format::FORMAT_BEFORE_THE_HELLO_SAID;

/// Why a push of `files` to the board that said `hello` is refused, or
/// `None` when it may go: the project's format is one the board reads, the
/// board has said no hello this window (nothing to read against), or the
/// project's format cannot be read (the board says why itself).
pub fn push_refusal(files: &[(String, Vec<u8>)], hello: Option<&HelloFacts>) -> Option<String> {
    let hello = hello?;
    let manifest = files.iter().find(|(path, _)| {
        path.trim_start_matches("./").trim_start_matches('/') == PROJECT_MANIFEST
    })?;
    let mut project = ProjectFiles::new();
    project.insert(PROJECT_MANIFEST, manifest.1.clone());
    let format = stored_format(&project).ok()?;
    let reads = hello.project_format.unwrap_or(FORMAT_BEFORE_THE_HELLO_SAID);
    (format > reads).then(|| {
        format!(
            "This board's firmware reads projects up to format {reads}. Update it to push this \
             project."
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_project_newer_than_the_board_reads_is_refused_before_the_push() {
        let files = project(12);
        let older = hello(Some(11));
        assert_eq!(
            push_refusal(&files, Some(&older)).as_deref(),
            Some(
                "This board's firmware reads projects up to format 11. Update it to push this \
                 project."
            )
        );
        // A board whose hello predates the field reads at most 11.
        assert!(push_refusal(&files, Some(&hello(None))).is_some());
    }

    #[test]
    fn a_project_the_board_reads_goes_and_an_unknown_board_is_not_held() {
        assert_eq!(push_refusal(&project(12), Some(&hello(Some(12)))), None);
        assert_eq!(push_refusal(&project(11), Some(&hello(Some(12)))), None);
        assert_eq!(push_refusal(&project(12), None), None);
        assert_eq!(push_refusal(&[], Some(&hello(Some(11)))), None);
    }

    fn project(format: u32) -> Vec<(String, Vec<u8>)> {
        vec![
            (
                "project.json".to_string(),
                format!("{{\"format\": {format}, \"name\": \"eaves\"}}").into_bytes(),
            ),
            ("fixture.json".to_string(), b"{}".to_vec()),
        ]
    }

    fn hello(project_format: Option<u32>) -> HelloFacts {
        HelloFacts {
            proto: lpc_wire::WIRE_PROTO_VERSION,
            project_format,
            ..HelloFacts::default()
        }
    }
}
