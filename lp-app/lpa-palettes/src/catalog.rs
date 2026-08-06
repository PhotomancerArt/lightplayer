//! The merged starter-set catalog: originals plus (if present) third-party.

use std::sync::OnceLock;

use crate::entry::PaletteEntry;
use crate::{originals, third_party};

/// Every checked-in palette, parsed once. Originals load first so a build
/// with `third-party/` deleted still gets a stable, non-empty catalog.
///
/// Panics on malformed embedded data — same contract as `lpa-boards::
/// all_boards`: the tests and this crate's own build keep that impossible
/// at HEAD.
pub fn all_palettes() -> &'static [PaletteEntry] {
    static PALETTES: OnceLock<Vec<PaletteEntry>> = OnceLock::new();
    PALETTES.get_or_init(|| {
        let mut palettes = originals::load_all()
            .unwrap_or_else(|error| panic!("embedded LightPlayer original palette: {error}"));
        palettes.extend(
            third_party::load_all()
                .unwrap_or_else(|error| panic!("embedded third-party palette: {error}")),
        );
        palettes
    })
}

pub fn palette_by_id(id: &str) -> Option<&'static PaletteEntry> {
    all_palettes().iter().find(|palette| palette.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_loads_and_is_non_empty() {
        assert!(!all_palettes().is_empty());
    }

    #[test]
    fn palette_by_id_finds_a_known_entry() {
        assert!(palette_by_id("fastled_ocean").is_some());
        assert!(palette_by_id("does_not_exist").is_none());
    }
}
