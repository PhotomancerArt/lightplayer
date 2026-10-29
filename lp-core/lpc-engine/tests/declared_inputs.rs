//! A module's declared inputs list in its host scope — through a playlist
//! entry's sink — with no demand on dormant entries (playlists-on-the-bus
//! D5, modules.md R2's 2026-10-20 amendment).
//!
//! ```bash
//! cargo test -p lpc-engine --test declared_inputs
//! ```

use lpc_engine::engine::LoadedProjectRuntime;
use lpc_engine::node::ScopeRef;
use lpc_engine::{EngineServices, ProjectLoader};
use lpc_model::TreePath;
use lpfs::{AsLpPath, LpFs, LpFsMemory};

/// Entry 1 plays a pattern that declares `palette` and `detail`; entry 2
/// one that declares nothing. `idle` picks which is resident at load.
fn project(idle: u32) -> LoadedProjectRuntime {
    let fs = LpFsMemory::new();
    let write = |path: &str, body: &str| {
        fs.write_file(path.as_path(), body.as_bytes())
            .expect("write fixture");
    };
    write("/project.json", "{\n  \"format\": 12\n}\n");
    write(
        "/module.json",
        r#"{ "kind": "Module", "nodes": {
            "clock": { "ref": "./clock.json" },
            "list": { "ref": "./playlist.json" } } }"#,
    );
    write("/clock.json", r#"{ "kind": "Clock" }"#);
    write(
        "/playlist.json",
        &format!(
            r#"{{ "kind": "Playlist",
            "bindings": {{ "time": {{ "source": "bus:time" }} }},
            "idle_entry": {idle},
            "entries": {{
              "1": {{ "name": "shared", "node": {{ "ref": "./shared/module.json" }} }},
              "2": {{ "name": "own", "node": {{ "ref": "./own/module.json" }} }}
            }} }}"#
        ),
    );
    for (dir, inputs) in [
        ("shared", r#""inputs": ["palette", "detail"],"#),
        ("own", ""),
    ] {
        write(
            &format!("/{dir}/module.json"),
            &format!(
                r#"{{ "kind": "Module", "nodes": {{ "shader": {{ "ref": "./shader.json" }} }}, {inputs} "provenance": {{ "author": "test" }} }}"#
            ),
        );
        write(
            &format!("/{dir}/shader.json"),
            r#"{ "kind": "Shader", "source": "shader.glsl" }"#,
        );
        write(
            &format!("/{dir}/shader.glsl"),
            "vec4 render_2d(vec2 pos) { return vec4(0.0); }\n",
        );
    }
    ProjectLoader::load_from_root(
        &fs,
        EngineServices::new(TreePath::parse("/inputs.show").expect("path")),
    )
    .expect("load")
}

fn root_channels(rt: &LoadedProjectRuntime) -> Vec<String> {
    let tree = rt.engine().tree();
    tree.scope_channels(ScopeRef::Module { owner: tree.root() })
        .into_iter()
        .map(|(name, _)| name.0)
        .collect()
}

#[test]
fn a_playing_patterns_declared_inputs_list_at_the_playlists_level() {
    let channels = root_channels(&project(1));
    assert!(channels.contains(&"palette".to_string()), "{channels:?}");
    assert!(channels.contains(&"detail".to_string()), "{channels:?}");
}

/// No demand (R2.2): the declaring entry is dormant, so it has no node and
/// declares nothing; the playing one declares nothing either.
#[test]
fn a_dormant_entrys_inputs_never_list() {
    let channels = root_channels(&project(2));
    assert!(!channels.contains(&"palette".to_string()), "{channels:?}");
    assert!(!channels.contains(&"detail".to_string()), "{channels:?}");
}

#[test]
fn a_declared_inputs_kind_comes_from_the_well_known_registry() {
    let rt = project(1);
    let tree = rt.engine().tree();
    let detail = tree
        .scope_channels(ScopeRef::Module { owner: tree.root() })
        .into_iter()
        .find(|(name, _)| name.0 == "detail")
        .expect("detail lists");
    assert_eq!(detail.1, lpc_model::Kind::Ratio);
}
