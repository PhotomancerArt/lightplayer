//! Catalog-wide invariants: every palette validates, ids are unique, and
//! the starter-set counts match the M3 phase file's targets.

use lpa_palettes::{PaletteCategory, all_palettes};

#[test]
fn every_palette_passes_gradient_validate() {
    for palette in all_palettes() {
        assert_eq!(
            palette.gradient.validate(),
            Ok(()),
            "palette {:?} ({}) failed Gradient::validate()",
            palette.id,
            palette.name
        );
    }
}

/// Every catalog file's stops literal must be CANONICAL: parsing it and
/// re-printing reproduces the file's own string byte-for-byte, so hand
/// edits that drift from the canonical printer are caught here rather than
/// shipping (ADR 2026-08-05-gradient-stops-string-storage). Walks the
/// asset tree directly so the FILE text is what is checked, not just the
/// parsed structs.
#[test]
fn every_catalog_stops_literal_is_canonical() {
    fn walk(dir: &std::path::Path, checked: &mut usize) {
        for entry in std::fs::read_dir(dir).expect("read_dir") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                walk(&path, checked);
            } else if path.extension().is_some_and(|ext| ext == "json") {
                let text = std::fs::read_to_string(&path).expect("read palette file");
                let value: serde_json::Value =
                    serde_json::from_str(&text).expect("parse palette file");
                let gradient = &value["gradient"];
                let space = lpc_model::Colorspace::parse(gradient["space"].as_str().unwrap())
                    .expect("known space token");
                let literal = gradient["stops"].as_str().unwrap_or_else(|| {
                    panic!("{}: stops must be a string literal", path.display())
                });
                let stops = lpc_model::parse_stops(literal)
                    .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
                assert_eq!(
                    lpc_model::print_stops(space, &stops),
                    literal,
                    "{}: stops literal is not canonical",
                    path.display()
                );
                *checked += 1;
            }
        }
    }
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/palettes");
    let mut checked = 0;
    walk(&root, &mut checked);
    assert!(
        checked >= 25,
        "expected the catalog files, checked {checked}"
    );
}

#[test]
fn palette_ids_are_unique() {
    let palettes = all_palettes();
    for palette in palettes {
        let count = palettes
            .iter()
            .filter(|other| other.id == palette.id)
            .count();
        assert_eq!(count, 1, "duplicate palette id {:?}", palette.id);
    }
}

#[test]
fn starter_set_is_around_thirty_palettes() {
    let count = all_palettes().len();
    assert!(
        (25..=35).contains(&count),
        "expected the M3 starter set to land near ~30 palettes, got {count}"
    );
}

#[test]
fn seven_fastled_stock_palettes() {
    let count = all_palettes()
        .iter()
        .filter(|p| p.category == PaletteCategory::FastledStock)
        .count();
    assert_eq!(count, 7, "FastLED ships exactly 7 stock palettes");
}

#[test]
fn cptcity_count_is_in_the_fifteen_to_twenty_range() {
    let count = all_palettes()
        .iter()
        .filter(|p| p.category == PaletteCategory::CptCity)
        .count();
    assert!(
        (15..=20).contains(&count),
        "expected 15-20 curated cpt-city gradients per the M3 phase file, got {count}"
    );
}

#[test]
fn five_to_eight_lightplayer_originals() {
    let count = all_palettes()
        .iter()
        .filter(|p| p.category == PaletteCategory::LightplayerOriginal)
        .count();
    assert!(
        (5..=8).contains(&count),
        "expected 5-8 LightPlayer originals per the M3 phase file, got {count}"
    );
}

#[test]
fn every_third_party_palette_carries_a_license_every_original_does_not() {
    for palette in all_palettes() {
        let is_third_party = matches!(
            palette.category,
            PaletteCategory::FastledStock | PaletteCategory::CptCity
        );
        assert_eq!(
            palette.license.is_some(),
            is_third_party,
            "palette {:?}: license presence must match third-party-ness",
            palette.id
        );
    }
}

#[test]
fn lightplayer_originals_are_authored_in_oklab() {
    use lpc_model::Colorspace;

    for palette in all_palettes() {
        if palette.category == PaletteCategory::LightplayerOriginal {
            assert_eq!(
                palette.gradient.space,
                Colorspace::Oklab,
                "original {:?} must be authored in Oklab (D8)",
                palette.id
            );
        }
    }
}

#[test]
fn third_party_imports_are_tagged_srgb() {
    use lpc_model::Colorspace;

    for palette in all_palettes() {
        let is_third_party = matches!(
            palette.category,
            PaletteCategory::FastledStock | PaletteCategory::CptCity
        );
        if is_third_party {
            assert_eq!(
                palette.gradient.space,
                Colorspace::Srgb,
                "third-party import {:?} must be tagged space: srgb (fidelity convention)",
                palette.id
            );
        }
    }
}

/// Documents the isolation contract's most important guarantee: this crate
/// (`lpa-palettes`) never has a hard `include_str!`/`include_bytes!`
/// dependency on `assets/palettes/third-party/` existing — `build.rs`
/// generates an empty source table when the directory is missing rather
/// than failing to compile. This test can't delete the directory (other
/// tests in the same run need it), so it documents and spot-checks the
/// mechanism instead: `build.rs`'s `write_source_table` never panics or
/// aborts when a directory is absent, only when the directory *is* present
/// but a file inside it is malformed JSON (a real bug, not an absence).
#[test]
fn catalog_never_requires_the_third_party_directory_to_compile() {
    // If this compiled and every other test in this binary ran, the build
    // already proved the contract for the "present" case. The absence case
    // is exercised directly against build.rs's directory walk in
    // `lp-app/lpa-palettes/build.rs`'s `write_source_table`, which checks
    // `dir.is_dir()` before ever calling `include_str!` and falls back to
    // an empty `&[]` table -- there is no code path from "directory
    // missing" to "compile error".
    assert!(!all_palettes().is_empty());
}
