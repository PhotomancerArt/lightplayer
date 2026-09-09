//! P4 gate: both pattern templates must produce projects that are real,
//! not merely well-formed.
//!
//! The four oracles live in `support/` and are shared with the catalog's
//! own patterns (`catalog_pattern_oracles.rs`): the same three checks the
//! generated board projects are held to (`generated_board_projects.rs`),
//! plus the one that is specific to a library project — the export lints
//! clean *from the installed copy*, the export lint's real input.

mod support;

use lpa_studio_core::app::home::{ProjectTemplate, template_project_files};
use support::{
    Files, Schemas, assert_installed_copy_is_a_lint_clean_pattern,
    assert_loads_through_the_real_loader, install_and_read_back, schema_failures,
};

fn templates() -> [ProjectTemplate; 2] {
    [ProjectTemplate::Pattern1d, ProjectTemplate::Pattern2d]
}

fn files(template: ProjectTemplate) -> Files {
    template_project_files(template)
        .unwrap_or_else(|error| panic!("{template:?}: {error:?}"))
        .unwrap_or_else(|| panic!("{template:?} generates files"))
}

#[test]
fn every_pattern_template_validates_against_the_checked_in_schemas() {
    let schemas = Schemas::checked_in();
    let mut failures = Vec::new();
    for template in templates() {
        failures.extend(schema_failures(
            &schemas,
            &format!("{template:?}"),
            &files(template),
        ));
    }
    assert!(
        failures.is_empty(),
        "{} template file(s) failed schema conformance:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// The oracle that matters most: the template loads through the SAME
/// loader the sim and the device use. A `render(vec2)` shader referencing
/// a missing `shader.glsl`, an `effect/` module whose mirror publishes
/// nothing, or a fixture bound to a control channel with no output would
/// all surface here.
#[test]
fn every_pattern_template_loads_through_the_real_loader() {
    for template in templates() {
        assert_loads_through_the_real_loader(&format!("{template:?}"), &files(template));
    }
}

/// The library round trip: creating from a template installs the authored
/// files verbatim, and the `kind`/`exports` the manifest carries survive
/// the uid/name rewrite `install_package` does on the way in. Without this
/// the New menu would quietly hand back a general project.
#[test]
fn creating_from_a_template_installs_a_pre_designated_pattern_project() {
    for template in templates() {
        let label = format!("{template:?}");
        let installed =
            install_and_read_back(&label, template.default_project_name(), files(template));
        assert_installed_copy_is_a_lint_clean_pattern(&label, &installed);
    }
}

/// A blank create still sends no files, so the store's own scaffold — the
/// minimal manifest plus the one-line root module — is what a blank
/// project has always been.
#[test]
fn the_blank_template_still_takes_the_stores_own_scaffold() {
    assert_eq!(
        template_project_files(ProjectTemplate::Blank).unwrap(),
        None
    );
}
