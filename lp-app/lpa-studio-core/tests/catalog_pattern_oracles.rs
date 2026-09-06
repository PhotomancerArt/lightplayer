//! Every `kind: pattern` catalog entry is held to the four oracles the
//! pattern templates are (`support/`): the checked-in schemas, the real
//! `ProjectLoader`, a library round trip that keeps `kind`/`exports`, and
//! an export lint that is clean from the installed copy. Adopting the
//! pattern shape is what makes an entry importable (modules.md §6); this
//! is the gate that says the adoption is real.

mod support;

use lpa_studio_core::app::home::embedded_examples;
use lpc_model::ProjectKind;
use support::{
    Files, Schemas, assert_installed_copy_is_a_lint_clean_pattern,
    assert_loads_through_the_real_loader, install_and_read_back, schema_failures,
};

/// `(id, name, files)` for every catalog entry whose manifest says
/// `pattern`; at least the eight that adopted the shape, so an empty walk
/// cannot pass.
fn catalog_patterns() -> Vec<(String, String, Files)> {
    let patterns: Vec<_> = embedded_examples()
        .iter()
        .filter(|example| matches!(example.kind, ProjectKind::Pattern { .. }))
        .map(|example| {
            (
                example.id.to_string(),
                example.name.to_string(),
                example.files(),
            )
        })
        .collect();
    assert!(
        patterns.len() >= 8,
        "the catalog holds at least eight patterns: {:?}",
        patterns.iter().map(|(id, _, _)| id).collect::<Vec<_>>()
    );
    patterns
}

#[test]
fn every_catalog_pattern_validates_against_the_checked_in_schemas() {
    let schemas = Schemas::checked_in();
    let mut failures = Vec::new();
    for (id, _, files) in catalog_patterns() {
        failures.extend(schema_failures(&schemas, &id, &files));
    }
    assert!(
        failures.is_empty(),
        "{} catalog pattern file(s) failed schema conformance:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn every_catalog_pattern_loads_through_the_real_loader() {
    for (id, _, files) in catalog_patterns() {
        assert_loads_through_the_real_loader(&id, &files);
    }
}

/// The consumption half: installing a catalog pattern into a library
/// keeps its designation, and the installed export lints clean — which is
/// exactly what the import picker vendors from.
#[test]
fn every_catalog_pattern_installs_as_a_lint_clean_pattern_project() {
    for (id, name, files) in catalog_patterns() {
        let installed = install_and_read_back(&id, &name, files);
        assert_installed_copy_is_a_lint_clean_pattern(&id, &installed);
    }
}
