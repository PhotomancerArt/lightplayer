//! `ProjectRegistry::entry_issues`: the all-entries check (D19). Studio and
//! `lp-cli upload` both walk every entry, not only the resident one, since a
//! device loads just the playing entry and would only find a broken dormant
//! pattern when it is picked.

mod support;

use support::RegistryScenario;

#[test]
fn broken_entry_among_dormant_siblings_is_named() {
    let (mut scenario, _) = three_entry_project_with_broken_entry_two();
    scenario.make_every_entry_resident();

    let issues = scenario.registry().entry_issues();
    assert_eq!(issues.len(), 1, "issues: {issues:?}");
    let issue = &issues[0];
    assert_eq!(issue.entry, 2);
    assert_eq!(issue.name.as_deref(), Some("two"));
    assert!(
        issue.display().contains("entry 2"),
        "display: {}",
        issue.display()
    );
    assert!(
        issue.display().contains("playlist.json"),
        "display: {}",
        issue.display()
    );
}

#[test]
fn a_dormant_entry_left_dormant_is_not_reported() {
    let (scenario, _) = three_entry_project_with_broken_entry_two();
    // At load, only the idle entry (1) is resident; entries 2 (broken) and 3
    // stay dormant. `entry_issues` speaks only about the current inventory
    // (AC1): a dormant entry is silently absent, not reported broken.
    assert!(scenario.registry().entry_issues().is_empty());
}

#[test]
fn a_project_with_every_entry_fine_reports_nothing() {
    let mut scenario = RegistryScenario::empty();
    scenario.write_container_manifest();
    scenario.write_file(
        "/module.json",
        r#"{
  "kind": "Module",
  "nodes": {
    "playlist": { "ref": "./playlist.json" }
  }
}"#,
    );
    scenario.write_file(
        "/playlist.json",
        r#"{
  "kind": "Playlist",
  "idle_entry": 1,
  "entries": {
    "1": { "name": "one", "node": { "ref": "./one/shader.json" } },
    "2": { "name": "two", "node": { "ref": "./two/shader.json" } }
  }
}"#,
    );
    for name in ["one", "two"] {
        scenario.write_file(
            &format!("/{name}/shader.json"),
            r#"{
  "kind": "Shader",
  "source": { "path": "./shader.glsl" }
}"#,
        );
        scenario.write_file(
            &format!("/{name}/shader.glsl"),
            "vec4 render_2d(vec2 pos) { return vec4(0.0); }",
        );
    }
    scenario.load_root("/module.json");
    scenario.make_every_entry_resident();

    assert!(scenario.registry().entry_issues().is_empty());
}

/// A module with a three-entry playlist (idle = 1). Entry 2's def file is
/// not valid JSON; entries 1 and 3 are ordinary shaders.
fn three_entry_project_with_broken_entry_two() -> (RegistryScenario, lpc_registry::LoadResult) {
    let mut scenario = RegistryScenario::empty();
    scenario.write_container_manifest();
    scenario.write_file(
        "/module.json",
        r#"{
  "kind": "Module",
  "nodes": {
    "playlist": { "ref": "./playlist.json" }
  }
}"#,
    );
    scenario.write_file(
        "/playlist.json",
        r#"{
  "kind": "Playlist",
  "idle_entry": 1,
  "entries": {
    "1": { "name": "one", "node": { "ref": "./one/shader.json" } },
    "2": { "name": "two", "node": { "ref": "./two/shader.json" } },
    "3": { "name": "three", "node": { "ref": "./three/shader.json" } }
  }
}"#,
    );
    for name in ["one", "three"] {
        scenario.write_file(
            &format!("/{name}/shader.json"),
            r#"{
  "kind": "Shader",
  "source": { "path": "./shader.glsl" }
}"#,
        );
        scenario.write_file(
            &format!("/{name}/shader.glsl"),
            "vec4 render_2d(vec2 pos) { return vec4(0.0); }",
        );
    }
    // Entry two: not valid JSON at all, so this is a `ParseError` state.
    scenario.write_file("/two/shader.json", "{ this is not valid json");

    let load = scenario.load_root("/module.json");
    (scenario, load)
}
