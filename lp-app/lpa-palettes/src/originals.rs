//! LightPlayer original palette sources — `assets/palettes/originals/`, the
//! sibling of the third-party isolation directory. Always present; nothing
//! degrades when it's the only thing left.

use crate::PaletteCategory;
use crate::entry::{PaletteEntry, PaletteLoadError, parse_entry};

include!(concat!(env!("OUT_DIR"), "/originals_catalog.generated.rs"));

pub(crate) fn load_all() -> Result<Vec<PaletteEntry>, PaletteLoadError> {
    ORIGINAL_SOURCES
        .iter()
        .map(|(table_key, json_source)| {
            parse_entry(
                table_key,
                json_source,
                PaletteCategory::LightplayerOriginal,
                false,
            )
        })
        .collect()
}
