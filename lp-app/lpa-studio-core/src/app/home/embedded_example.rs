//! The compiled-in example packages (offline/first-run fallback).
//!
//! Until the examples place lands (M6, D17), the gallery's *Examples*
//! section lists these. The id doubles as the seed-once provenance source
//! (`SeededFrom { source }`), so a package seeded by the pre-M4 demo flow
//! and one opened from the gallery are the same package.
//!
//! Each package's files are `include_bytes!`d from `examples/<name>/`, so
//! the wasm bundle carries them and the checked-in example IS what the
//! gallery opens. Adding an example means adding its file table here —
//! and remembering that an existing library store keeps the package it
//! already seeded (delete the gallery package to re-seed).

/// One file in an embedded package: its package-relative path and bytes.
pub type ExampleFile = (&'static str, &'static [u8]);

/// One compiled-in example.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EmbeddedExample {
    pub id: &'static str,
    pub name: &'static str,
    pub kind: &'static str,
    /// The package's files, in deploy order (`project.json` first).
    pub files: &'static [ExampleFile],
}

impl EmbeddedExample {
    /// The example's package files as owned (relative path, bytes) pairs.
    pub fn files(&self) -> Vec<(String, Vec<u8>)> {
        self.files
            .iter()
            .map(|(path, bytes)| ((*path).to_string(), bytes.to_vec()))
            .collect()
    }
}

/// `examples/fyeah-sign` — the Studio demo project (see
/// [`crate::app::project::demo_project`] for why this one).
pub static FYEAH_SIGN_FILES: &[ExampleFile] = &[
    (
        "project.json",
        include_bytes!("../../../../../examples/fyeah-sign/project.json"),
    ),
    (
        "module.json",
        include_bytes!("../../../../../examples/fyeah-sign/module.json"),
    ),
    (
        "button.json",
        include_bytes!("../../../../../examples/fyeah-sign/button.json"),
    ),
    (
        "clock.json",
        include_bytes!("../../../../../examples/fyeah-sign/clock.json"),
    ),
    (
        "fixture.json",
        include_bytes!("../../../../../examples/fyeah-sign/fixture.json"),
    ),
    (
        "output.json",
        include_bytes!("../../../../../examples/fyeah-sign/output.json"),
    ),
    (
        "playlist.json",
        include_bytes!("../../../../../examples/fyeah-sign/playlist.json"),
    ),
    (
        "radio.json",
        include_bytes!("../../../../../examples/fyeah-sign/radio.json"),
    ),
    (
        "idle.json",
        include_bytes!("../../../../../examples/fyeah-sign/idle.json"),
    ),
    (
        "idle.glsl",
        include_bytes!("../../../../../examples/fyeah-sign/idle.glsl"),
    ),
    (
        "blast.json",
        include_bytes!("../../../../../examples/fyeah-sign/blast.json"),
    ),
    (
        "blast.glsl",
        include_bytes!("../../../../../examples/fyeah-sign/blast.glsl"),
    ),
    (
        "fyeah.map2d.json",
        include_bytes!("../../../../../examples/fyeah-sign/fyeah.map2d.json"),
    ),
];

/// `examples/plasma` — one shader, two public knobs. The smallest module
/// whose root panel is not empty: `scale` and the phasor slot's period
/// (bound to the `speed` channel, which carries the whole `PhasorConfig`)
/// are bound to root scope channels, so binding-is-publicity (Q13) puts
/// them on the module card's panel with nothing else authored.
pub static PLASMA_FILES: &[ExampleFile] = &[
    (
        "project.json",
        include_bytes!("../../../../../examples/plasma/project.json"),
    ),
    (
        "module.json",
        include_bytes!("../../../../../examples/plasma/module.json"),
    ),
    (
        "clock.json",
        include_bytes!("../../../../../examples/plasma/clock.json"),
    ),
    (
        "fixture.json",
        include_bytes!("../../../../../examples/plasma/fixture.json"),
    ),
    (
        "output.json",
        include_bytes!("../../../../../examples/plasma/output.json"),
    ),
    (
        "shader.json",
        include_bytes!("../../../../../examples/plasma/shader.json"),
    ),
    (
        "shader.glsl",
        include_bytes!("../../../../../examples/plasma/shader.glsl"),
    ),
    (
        "fixture.map2d.json",
        include_bytes!("../../../../../examples/plasma/fixture.map2d.json"),
    ),
];

/// `examples/plasma-grid` — the SAME plasma shader and knobs on a 16×16
/// grid mapping. Exists for the "one effect, any shape" beat of the
/// interactive docs ("What's a shader?"): a docs page runs `plasma` and
/// `plasma-grid` side by side off shared knobs, so the two must stay
/// byte-identical except `project.json` (the name) and
/// `fixture.map2d.json` (the shape).
pub static PLASMA_GRID_FILES: &[ExampleFile] = &[
    (
        "project.json",
        include_bytes!("../../../../../examples/plasma-grid/project.json"),
    ),
    (
        "module.json",
        include_bytes!("../../../../../examples/plasma-grid/module.json"),
    ),
    (
        "clock.json",
        include_bytes!("../../../../../examples/plasma-grid/clock.json"),
    ),
    (
        "fixture.json",
        include_bytes!("../../../../../examples/plasma-grid/fixture.json"),
    ),
    (
        "output.json",
        include_bytes!("../../../../../examples/plasma-grid/output.json"),
    ),
    (
        "shader.json",
        include_bytes!("../../../../../examples/plasma-grid/shader.json"),
    ),
    (
        "shader.glsl",
        include_bytes!("../../../../../examples/plasma-grid/shader.glsl"),
    ),
    (
        "fixture.map2d.json",
        include_bytes!("../../../../../examples/plasma-grid/fixture.map2d.json"),
    ),
];

/// `examples/meteor` — a compute/render pair: `sim` integrates meteor heads
/// into a persistent map, `render` draws their tails from it over a
/// node-to-node binding. Publishes `speed`, `count` (a stepped knob) and
/// `decay` on the root panel.
pub static METEOR_FILES: &[ExampleFile] = &[
    (
        "project.json",
        include_bytes!("../../../../../examples/meteor/project.json"),
    ),
    (
        "module.json",
        include_bytes!("../../../../../examples/meteor/module.json"),
    ),
    (
        "clock.json",
        include_bytes!("../../../../../examples/meteor/clock.json"),
    ),
    (
        "fixture.json",
        include_bytes!("../../../../../examples/meteor/fixture.json"),
    ),
    (
        "output.json",
        include_bytes!("../../../../../examples/meteor/output.json"),
    ),
    (
        "sim.json",
        include_bytes!("../../../../../examples/meteor/sim.json"),
    ),
    (
        "sim.glsl",
        include_bytes!("../../../../../examples/meteor/sim.glsl"),
    ),
    (
        "render.json",
        include_bytes!("../../../../../examples/meteor/render.json"),
    ),
    (
        "render.glsl",
        include_bytes!("../../../../../examples/meteor/render.glsl"),
    ),
    (
        "fixture.map2d.json",
        include_bytes!("../../../../../examples/meteor/fixture.map2d.json"),
    ),
];

/// `examples/zook-dome` — a real 16' geodesic dome: 1500 LEDs as five
/// 300-lamp channels, mapped top-down from the builder's wiring sketch
/// (`scripts/zook-dome/`). The mapping-scale example: rings from the apex
/// cross all five channels with no per-channel configuration.
pub static ZOOK_DOME_FILES: &[ExampleFile] = &[
    (
        "project.json",
        include_bytes!("../../../../../examples/zook-dome/project.json"),
    ),
    (
        "module.json",
        include_bytes!("../../../../../examples/zook-dome/module.json"),
    ),
    (
        "clock.json",
        include_bytes!("../../../../../examples/zook-dome/clock.json"),
    ),
    (
        "fixture.json",
        include_bytes!("../../../../../examples/zook-dome/fixture.json"),
    ),
    (
        "output.json",
        include_bytes!("../../../../../examples/zook-dome/output.json"),
    ),
    (
        "shader.json",
        include_bytes!("../../../../../examples/zook-dome/shader.json"),
    ),
    (
        "shader.glsl",
        include_bytes!("../../../../../examples/zook-dome/shader.glsl"),
    ),
    (
        "fixture.map2d.json",
        include_bytes!("../../../../../examples/zook-dome/fixture.map2d.json"),
    ),
];

/// The gallery's *Examples* section, in order — the demo first, then the
/// single-effect modules.
static EMBEDDED_EXAMPLES: &[EmbeddedExample] = &[
    EmbeddedExample {
        id: crate::STUDIO_DEMO_PROJECT_ID,
        name: "Fyeah Sign",
        kind: "Module",
        files: FYEAH_SIGN_FILES,
    },
    EmbeddedExample {
        id: "examples/plasma",
        name: "Plasma",
        kind: "Module",
        files: PLASMA_FILES,
    },
    EmbeddedExample {
        id: "examples/meteor",
        name: "Meteor",
        kind: "Module",
        files: METEOR_FILES,
    },
    EmbeddedExample {
        id: "examples/plasma-grid",
        name: "Plasma Grid",
        kind: "Module",
        files: PLASMA_GRID_FILES,
    },
    EmbeddedExample {
        id: "examples/zook-dome",
        name: "Zook dome",
        kind: "Module",
        files: ZOOK_DOME_FILES,
    },
];

/// All embedded examples, gallery order.
pub fn embedded_examples() -> &'static [EmbeddedExample] {
    EMBEDDED_EXAMPLES
}

/// Look up an embedded example by id.
pub fn embedded_example(id: &str) -> Option<EmbeddedExample> {
    embedded_examples()
        .iter()
        .copied()
        .find(|example| example.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::project::demo_project::DEMO_PROJECT_ID;

    #[test]
    fn demo_example_is_embedded_with_files() {
        let example = embedded_example(DEMO_PROJECT_ID).expect("demo example is embedded");
        assert_eq!(example.name, "Fyeah Sign");
        assert_eq!(example.kind, "Module");
        let files = example.files();
        assert!(
            files
                .iter()
                .any(|(path, _)| path == "project.json" && !files.is_empty())
        );
    }

    #[test]
    fn unknown_example_is_none() {
        assert!(embedded_example("examples/unknown").is_none());
    }

    /// The "one effect, any shape" contract (interactive docs): the grid
    /// variant is byte-identical to plasma except its name and mapping,
    /// so the docs page's shared knobs honestly drive one shader on two
    /// shapes. A drift here (e.g. a plasma shader tweak not copied over)
    /// silently breaks that story.
    #[test]
    fn plasma_grid_differs_from_plasma_only_in_name_and_mapping() {
        let plasma = embedded_example("examples/plasma").expect("plasma is embedded");
        let grid = embedded_example("examples/plasma-grid").expect("plasma-grid is embedded");
        let plasma_files: std::collections::BTreeMap<_, _> = plasma.files().into_iter().collect();
        let grid_files: std::collections::BTreeMap<_, _> = grid.files().into_iter().collect();
        assert_eq!(
            plasma_files.keys().collect::<Vec<_>>(),
            grid_files.keys().collect::<Vec<_>>(),
            "the two variants ship the same file set"
        );
        for (path, bytes) in &plasma_files {
            let grid_bytes = &grid_files[path];
            if path == "project.json" || path == "fixture.map2d.json" {
                assert_ne!(bytes, grid_bytes, "{path} is the deliberate difference");
            } else {
                assert_eq!(bytes, grid_bytes, "{path} must stay byte-identical");
            }
        }
    }

    #[test]
    fn every_example_ships_the_two_container_files() {
        // Mitosis (modules.md §1/§6): a package is unopenable without BOTH
        // the container manifest and the root module. Found the hard way
        // when a fixture's mapping document was left out of the demo list.
        for example in embedded_examples() {
            let files = example.files();
            for required in ["project.json", "module.json"] {
                assert!(
                    files.iter().any(|(path, _)| path == required),
                    "{} must ship {required}",
                    example.id
                );
            }
            assert_eq!(
                files.first().map(|(path, _)| path.as_str()),
                Some("project.json"),
                "{} deploys the container manifest first",
                example.id
            );
        }
    }

    #[test]
    fn example_ids_and_names_are_unique() {
        let mut ids: Vec<&str> = embedded_examples().iter().map(|it| it.id).collect();
        let count = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), count, "example ids collide");
    }
}
