//! The save-time all-entries check (D19): after a save pulls the library
//! copy up to date, walk every playlist entry — not only the resident one
//! the running project keeps loaded — and warn about any that fail to
//! load. Unlike `lp-cli upload`, this never refuses: a work-in-progress
//! must still save.

use lpc_engine::{EngineServices, ProjectLoader};
use lpc_model::TreePath;
use lpfs::LpFs;

/// Every entry, of every playlist, that fails to load in `fs` — by display
/// name, `EntryIssue::display()`'s shape ("entry 2 (\"blast\") of playlist
/// /playlist.json: ..."). Empty when the project root itself fails to load
/// too: that failure is not this check's business (the running project
/// already surfaced it, since the resident subtree loaded from the very
/// files just saved).
pub(crate) fn entries_failing_to_load(fs: &dyn LpFs) -> Vec<String> {
    // An arbitrary, fixed anchor: this loader instance never ticks or
    // resolves against a real show tree, so the path's content doesn't
    // matter, only that it parses.
    let root_path = match TreePath::parse("/studio_save_check.show") {
        Ok(path) => path,
        Err(_) => return Vec::new(),
    };
    let services = EngineServices::new(root_path);
    let Ok(runtime) = ProjectLoader::load_from_root_with_every_entry_resident(fs, services) else {
        return Vec::new();
    };
    runtime
        .registry()
        .entry_issues()
        .iter()
        .map(|issue| issue.display())
        .collect()
}

/// The notice `save_overlay` pushes when [`entries_failing_to_load`] found
/// something — a warning, never a refusal (a work-in-progress must still
/// save).
pub(crate) fn issues_notice(issues: &[String]) -> Option<crate::UiNotice> {
    if issues.is_empty() {
        return None;
    }
    Some(crate::UiNotice::warning(format!(
        "{} pattern{} failed to load:\n{}",
        issues.len(),
        if issues.len() == 1 { "" } else { "s" },
        issues.join("\n")
    )))
}

#[cfg(test)]
mod tests {
    use lpfs::{LpFs, LpFsMemory, LpPath};

    use super::{entries_failing_to_load, issues_notice};

    /// A playlist with an idle entry and a dormant entry whose def is not
    /// valid JSON: `entries_failing_to_load` names the broken entry, and
    /// `issues_notice` turns that into a warning (never `None`, never a
    /// refusal — this is the save-time check, which must not block a
    /// work-in-progress).
    #[test]
    fn a_broken_dormant_entry_is_named_and_warned_about() {
        let fs = LpFsMemory::new();
        write(&fs, "/project.json", "{\n  \"format\": 11\n}\n");
        write(
            &fs,
            "/module.json",
            r#"{
  "kind": "Module",
  "nodes": { "playlist": { "ref": "./playlist.json" } }
}"#,
        );
        write(
            &fs,
            "/playlist.json",
            r#"{
  "kind": "Playlist",
  "idle_entry": 1,
  "entries": {
    "1": { "name": "idle", "node": { "ref": "./idle.json" } },
    "2": { "name": "broken", "node": { "ref": "./broken.json" } }
  }
}"#,
        );
        write(
            &fs,
            "/idle.json",
            r#"{
  "kind": "Shader",
  "source": { "path": "./idle.glsl" }
}"#,
        );
        write(
            &fs,
            "/idle.glsl",
            "vec4 render_2d(vec2 pos) { return vec4(0.0); }",
        );
        write(&fs, "/broken.json", "{ this is not valid json");

        let issues = entries_failing_to_load(&fs);
        assert_eq!(issues.len(), 1, "issues: {issues:?}");
        assert!(issues[0].contains("entry 2"), "issue: {}", issues[0]);
        assert!(
            issues[0].contains("\"broken\""),
            "issue: {}",
            issues[0]
        );

        let notice = issues_notice(&issues).expect("issues produce a notice");
        assert!(
            notice.message.contains("entry 2"),
            "notice: {}",
            notice.message
        );
    }

    /// Every entry loading fine produces no issues and no notice — the
    /// common case must stay silent.
    #[test]
    fn a_project_with_every_entry_fine_has_no_notice() {
        let fs = LpFsMemory::new();
        write(&fs, "/project.json", "{\n  \"format\": 11\n}\n");
        write(
            &fs,
            "/module.json",
            r#"{
  "kind": "Module",
  "nodes": { "playlist": { "ref": "./playlist.json" } }
}"#,
        );
        write(
            &fs,
            "/playlist.json",
            r#"{
  "kind": "Playlist",
  "idle_entry": 1,
  "entries": {
    "1": { "name": "idle", "node": { "ref": "./idle.json" } }
  }
}"#,
        );
        write(
            &fs,
            "/idle.json",
            r#"{
  "kind": "Shader",
  "source": { "path": "./idle.glsl" }
}"#,
        );
        write(
            &fs,
            "/idle.glsl",
            "vec4 render_2d(vec2 pos) { return vec4(0.0); }",
        );

        assert!(entries_failing_to_load(&fs).is_empty());
        assert!(issues_notice(&entries_failing_to_load(&fs)).is_none());
    }

    fn write(fs: &LpFsMemory, path: &str, body: &str) {
        fs.write_file(LpPath::new(path), body.as_bytes())
            .expect("write fixture file");
    }
}
