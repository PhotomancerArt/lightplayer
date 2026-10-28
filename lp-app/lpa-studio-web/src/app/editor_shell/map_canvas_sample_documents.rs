//! Real map2d documents the Map canvas's stories and tests draw: the
//! small-dome's and the peach's, as they were before project format 12.
//!
//! The catalog's copies are map2d format 6 now, which the Map canvas (and
//! its `Map2dDoc` reader) does not read; the Map tab is hidden until M4's
//! Graph replaces it. These are the frozen format-11 inputs of the upgrade
//! corpus (`lpa-upgrade/tests/corpus/v11/`), byte-for-byte the catalog's
//! documents before the migration, and never edited.

/// The small-dome's dome document (map2d format 4): fifty polygon panels
/// in repeats, with sticky ids.
pub(crate) const SMALL_DOME_DOME: &str =
    include_str!("../../../../lpa-upgrade/tests/corpus/v11/small-dome/dome/dome.map2d.json");

/// The small-dome's doors document (map2d format 4).
pub(crate) const SMALL_DOME_DOORS: &str =
    include_str!("../../../../lpa-upgrade/tests/corpus/v11/small-dome/doors/doors.map2d.json");

/// The peach's body document (map2d format 1, id-less: two paths at the
/// scale-extreme pitch).
pub(crate) const PEACH_BODY: &str =
    include_str!("../../../../lpa-upgrade/tests/corpus/v11/peach-1d/body/peach_body.map2d.json");

/// The peach's leaf document (map2d format 1).
pub(crate) const PEACH_LEAF: &str =
    include_str!("../../../../lpa-upgrade/tests/corpus/v11/peach-1d/leaf/peach_leaf.map2d.json");

#[cfg(test)]
mod tests {
    use super::*;

    /// Every sample still reads as the Map canvas's document: the stories
    /// and tests that draw them depend on it.
    #[test]
    fn the_samples_read_as_map_canvas_documents() {
        for (name, text) in [
            ("dome", SMALL_DOME_DOME),
            ("doors", SMALL_DOME_DOORS),
            ("peach body", PEACH_BODY),
            ("peach leaf", PEACH_LEAF),
        ] {
            let doc = lpc_mapping::Map2dDoc::from_json(text)
                .unwrap_or_else(|error| panic!("the {name} sample reads: {error}"));
            assert!(!doc.objects.is_empty(), "the {name} sample draws something");
        }
    }
}
