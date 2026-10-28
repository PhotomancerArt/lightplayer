//! Every project `lpc-model`'s templates create loads at the current
//! project format and lights every lamp, in order, from port 0: lamp k of
//! each fixture lands on channel 3k of port 0 of its output, and none is
//! dark.
//!
//! The templates are `lp-cli create project`'s starter and Studio's two
//! pattern projects. Map2d format 6 has no default output, so each
//! fixture's mapping starts on its output by name; a name that no output
//! carries would leave every lamp dark, which this catches.
//!
//! ```bash
//! cargo test -p lpc-engine --test creatable_projects_land
//! ```

mod landing_support;

use std::path::Path;

use landing_support::{ProjectLanding, fs_landing};
use lpc_model::{
    PROJECT_FORMAT_VERSION, SlotShapeRegistry, pattern_project_files_1d, pattern_project_files_2d,
    starter_project_files,
};
use lpfs::{LpFs, LpFsMemory, LpPath};

/// `lp-cli create project`'s starter lights its 8 × 8 grid from port 0.
#[test]
fn the_starter_project_lights_every_lamp_in_order_from_port_0() {
    let files = starter_project_files("starter", &SlotShapeRegistry::default()).expect("starter");
    let landing = land("starter", &files);
    assert_every_lamp_in_order_from_port_0("starter", &landing, &[("/fixture.json", 64)]);
}

/// The 1D pattern project lights its strand and its panel, each from port 0
/// of its own output.
#[test]
fn the_1d_pattern_project_lights_every_lamp_in_order_from_port_0() {
    let files = pattern_project_files_1d("pattern", &SlotShapeRegistry::default()).expect("1d");
    let landing = land("pattern-1d", &files);
    assert_every_lamp_in_order_from_port_0(
        "pattern-1d",
        &landing,
        &[("/strip_300.json", 300), ("/matrix_32x16.json", 512)],
    );
}

/// The 2D pattern project lights its panel from port 0.
#[test]
fn the_2d_pattern_project_lights_every_lamp_in_order_from_port_0() {
    let files = pattern_project_files_2d("pattern", &SlotShapeRegistry::default()).expect("2d");
    let landing = land("pattern-2d", &files);
    assert_every_lamp_in_order_from_port_0("pattern-2d", &landing, &[("/matrix_32x16.json", 512)]);
}

// --- helpers ----------------------------------------------------------------

/// Stage `files` in memory and read where every lamp lands.
fn land(label: &str, files: &[(String, Vec<u8>)]) -> ProjectLanding {
    let fs = LpFsMemory::new();
    for (path, bytes) in files {
        fs.write_file(LpPath::new(&format!("/{path}")), bytes)
            .unwrap_or_else(|error| panic!("{label}: staging {path}: {error:?}"));
    }
    fs_landing(&fs, u64::from(PROJECT_FORMAT_VERSION), Path::new(label))
}

/// Each named fixture has `lamps` lamps; lamp k lands once, on channel 3k of
/// port 0 of one output; nothing is dark; no other fixture exists.
fn assert_every_lamp_in_order_from_port_0(
    label: &str,
    landing: &ProjectLanding,
    fixtures: &[(&str, u32)],
) {
    assert_eq!(
        landing.refused, None,
        "{label}: the project loads and renders"
    );
    let names: Vec<&str> = landing.fixtures.keys().map(String::as_str).collect();
    let expected: Vec<&str> = fixtures.iter().map(|(path, _)| *path).collect();
    let mut sorted = expected.clone();
    sorted.sort_unstable();
    assert_eq!(names, sorted, "{label}: its fixtures");
    for (path, lamps) in fixtures {
        let fixture = landing.fixture(path);
        assert_eq!(fixture.lamps, *lamps, "{label} {path}: lamps");
        assert!(
            fixture.dark.is_empty(),
            "{label} {path}: dark {:?}",
            fixture.dark
        );
        let output = &fixture.lands(0)[0].output;
        for lamp in 0..*lamps {
            let lands = fixture.lands(lamp);
            assert_eq!(lands.len(), 1, "{label} {path} lamp {lamp}: {lands:?}");
            assert_eq!(
                (&lands[0].output, lands[0].port, lands[0].channel),
                (output, 0, lamp * 3),
                "{label} {path} lamp {lamp}"
            );
        }
    }
}
