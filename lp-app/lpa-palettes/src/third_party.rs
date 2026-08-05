//! Third-party palette sources — see `assets/palettes/third-party/README.md`
//! for the isolation contract this module is the one exception to.
//!
//! This is the *only* module allowed to reference
//! `assets/palettes/third-party/`, and it does so only indirectly, through
//! `build.rs`'s generated `THIRD_PARTY_SOURCES` table (empty if the
//! directory is missing — see `build.rs` doc comment).

use crate::PaletteCategory;
use crate::entry::{PaletteEntry, PaletteLoadError, parse_entry};

include!(concat!(
    env!("OUT_DIR"),
    "/third_party_catalog.generated.rs"
));

/// Parse every embedded third-party source. Table keys under `fastled/` are
/// [`PaletteCategory::FastledStock`]; everything else (`cptcity/...`) is
/// [`PaletteCategory::CptCity`].
pub(crate) fn load_all() -> Result<Vec<PaletteEntry>, PaletteLoadError> {
    THIRD_PARTY_SOURCES
        .iter()
        .map(|(table_key, json_source)| {
            let category = if table_key.starts_with("fastled/") {
                PaletteCategory::FastledStock
            } else {
                PaletteCategory::CptCity
            };
            parse_entry(table_key, json_source, category, true)
        })
        .collect()
}
