//! The save-time all-entries check (D19): after a save pulls the library
//! copy up to date, walk every playlist entry — not only the resident one
//! the running project keeps loaded — and warn about any that fail to
//! load, and about a module holding two playlists (playlists-on-the-bus
//! D10). Unlike `lp-cli upload`, this never refuses: a work-in-progress
//! must still save.

use lpc_engine::{EngineServices, ProjectLoader};
use lpc_model::TreePath;
use lpfs::LpFs;

/// What the save-time check found.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct SaveCheck {
    /// Every entry, of every playlist, that fails to load — by display
    /// name, `EntryIssue::display()`'s shape ("entry 2 (\"blast\") of
    /// playlist /playlist.json: ...").
    pub failing: Vec<String>,
    /// Every playlist past the first that reads the same module scope as
    /// another, by tree path: the two share one pick, cycle and skip list
    /// (D10, one playlist per module).
    pub shared_playlists: Vec<String>,
}

/// Load `fs` with every entry resident and check it. Empty when the
/// project root itself fails to load too: that failure is not this check's
/// business (the running project already surfaced it, since the resident
/// subtree loaded from the very files just saved).
pub(crate) fn save_check(fs: &dyn LpFs) -> SaveCheck {
    // An arbitrary, fixed anchor: this loader instance never ticks or
    // resolves against a real show tree, so the path's content doesn't
    // matter, only that it parses.
    let root_path = match TreePath::parse("/studio_save_check.show") {
        Ok(path) => path,
        Err(_) => return SaveCheck::default(),
    };
    let services = EngineServices::new(root_path);
    let Ok(runtime) = ProjectLoader::load_from_root_with_every_entry_resident(fs, services) else {
        return SaveCheck::default();
    };
    let failing = runtime
        .registry()
        .entry_issues()
        .iter()
        .map(|issue| issue.display())
        .collect();
    let tree = runtime.engine().tree();
    let registry = runtime.registry();
    let mut scopes = Vec::new();
    let mut shared_playlists = Vec::new();
    for entry in tree.entries() {
        let is_playlist = entry
            .def_location
            .as_ref()
            .and_then(|location| registry.def(location))
            .and_then(|def| def.state.loaded_def())
            .is_some_and(|def| def.as_playlist().is_some());
        if !is_playlist {
            continue;
        }
        let Some(scope) = tree.bus_read_scope(entry.id) else {
            continue;
        };
        if scopes.contains(&scope) {
            shared_playlists.push(entry.path.to_string());
        } else {
            scopes.push(scope);
        }
    }
    SaveCheck {
        failing,
        shared_playlists,
    }
}

/// The notice for playlists sharing a module, if any — a warning, never a
/// refusal.
pub(crate) fn shared_playlists_notice(paths: &[String]) -> Option<crate::UiNotice> {
    if paths.is_empty() {
        return None;
    }
    Some(crate::UiNotice::warning(format!(
        "A module holds more than one playlist, so they share one picked pattern, cycle and \
         switch list: {}. Put each playlist in its own module.",
        paths.join(", ")
    )))
}

/// The notice `save_overlay` pushes when [`save_check`] found
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

    use super::{issues_notice, save_check, shared_playlists_notice};

    /// A playlist with an idle entry and a dormant entry whose def is not
    /// valid JSON: `entries_failing_to_load` names the broken entry, and
    /// `issues_notice` turns that into a warning (never `None`, never a
    /// refusal — this is the save-time check, which must not block a
    /// work-in-progress).
    #[test]
    fn a_broken_dormant_entry_is_named_and_warned_about() {
        let fs = LpFsMemory::new();
        write(&fs, "/project.json", "{\n  \"format\": 12\n}\n");
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

        let issues = save_check(&fs).failing;
        assert_eq!(issues.len(), 1, "issues: {issues:?}");
        assert!(issues[0].contains("entry 2"), "issue: {}", issues[0]);
        assert!(issues[0].contains("\"broken\""), "issue: {}", issues[0]);

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
        write(&fs, "/project.json", "{\n  \"format\": 12\n}\n");
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

        assert!(save_check(&fs).failing.is_empty());
        assert!(issues_notice(&save_check(&fs).failing).is_none());
    }

    /// Two playlists in one module share one pick, cycle and skip list
    /// (D10): the save warns, naming the second.
    #[test]
    fn two_playlists_in_one_module_are_warned_about() {
        let fs = LpFsMemory::new();
        write(&fs, "/project.json", "{\n  \"format\": 12\n}\n");
        write(
            &fs,
            "/module.json",
            r#"{
  "kind": "Module",
  "nodes": {
    "a": { "ref": "./a.json" },
    "b": { "ref": "./b.json" }
  }
}"#,
        );
        for name in ["a", "b"] {
            write(
                &fs,
                &format!("/{name}.json"),
                r#"{
  "kind": "Playlist",
  "idle_entry": 1,
  "entries": { "1": { "node": { "ref": "./idle.json" } } }
}"#,
            );
        }
        write(
            &fs,
            "/idle.json",
            r#"{ "kind": "Shader", "source": "idle.glsl" }"#,
        );
        write(
            &fs,
            "/idle.glsl",
            "vec4 render_2d(vec2 pos) { return vec4(0.0); }",
        );

        let check = save_check(&fs);
        assert_eq!(check.shared_playlists.len(), 1, "{check:?}");
        assert!(shared_playlists_notice(&check.shared_playlists).is_some());
        assert!(shared_playlists_notice(&[]).is_none());
    }

    fn write(fs: &LpFsMemory, path: &str, body: &str) {
        fs.write_file(LpPath::new(path), body.as_bytes())
            .expect("write fixture file");
    }
}
