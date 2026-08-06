//! [`PaletteEntry`]: one catalog palette plus the provenance the picker (M4)
//! will surface.

use lpc_model::Gradient;
use serde::Deserialize;

/// Where a palette came from, in the order the starter set groups them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PaletteCategory {
    /// One of FastLED's 7 stock `CRGBPalette16` tables.
    FastledStock,
    /// A cpt-city gradient from a collection whose `COPYING.yaml` grants
    /// redistribution (PD / CC0 / CC-BY / MIT only — see
    /// `assets/palettes/third-party/COPYING.md`).
    CptCity,
    /// Authored in-house, directly in Oklab.
    LightplayerOriginal,
}

/// Per-asset license record for a third-party palette (`assets/palettes/
/// third-party/COPYING.md` is the human-readable mirror of this data).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct PaletteLicense {
    /// License identifier: `"MIT"`, `"CC-BY-3.0"`, or `"Public Domain"` for
    /// the palettes shipped so far. Not strict SPDX (Public Domain isn't an
    /// SPDX identifier) but stable, machine-comparable strings.
    pub spdx: String,
    pub author: String,
    pub source_url: String,
}

/// One catalog palette: a [`Gradient`] plus the metadata the M4 picker shows
/// (name, category, provenance) per the M3 phase file's data-shape note.
#[derive(Clone, Debug, PartialEq)]
pub struct PaletteEntry {
    /// Stable identifier, `snake_case`, matches the source JSON file's `id`.
    pub id: String,
    pub name: String,
    pub category: PaletteCategory,
    /// `Some` for every third-party palette, `None` for LightPlayer
    /// originals (nothing to attribute).
    pub license: Option<PaletteLicense>,
    pub gradient: Gradient,
}

/// The on-disk JSON shape for one palette file. `gradient` deserializes
/// directly as [`Gradient`] — token metadata plus one stops literal, the
/// same shape every other surface carries (see `gradient.rs`'s
/// `gradient_serde_is_the_stops_literal_form` test upstream).
#[derive(Deserialize)]
pub(crate) struct PaletteFile {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub license: Option<PaletteLicense>,
    pub gradient: Gradient,
}

/// Parse-error context: which embedded source failed and why.
#[derive(Debug)]
pub struct PaletteLoadError {
    pub source_id: &'static str,
    pub message: String,
}

impl core::fmt::Display for PaletteLoadError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "palette source {:?}: {}", self.source_id, self.message)
    }
}

impl std::error::Error for PaletteLoadError {}

/// Parse one embedded `(table_key, json_source)` entry into a
/// [`PaletteEntry`], tagging it with `category`. `table_key` is the
/// generated table's file-path-derived key (e.g. `"cptcity/jjg-misc/
/// rainfall"`) — used only for error context, since the file's own `id`
/// field (e.g. `"jjg_misc_rainfall"`) is what the catalog actually keys on.
/// `expect_license` enforces the isolation convention at parse time:
/// third-party sources must carry a license block, originals must not.
pub(crate) fn parse_entry(
    table_key: &'static str,
    json_source: &'static str,
    category: PaletteCategory,
    expect_license: bool,
) -> Result<PaletteEntry, PaletteLoadError> {
    let file: PaletteFile =
        serde_json::from_str(json_source).map_err(|error| PaletteLoadError {
            source_id: table_key,
            message: format!("invalid JSON: {error}"),
        })?;

    if expect_license && file.license.is_none() {
        return Err(PaletteLoadError {
            source_id: table_key,
            message: "third-party palette is missing its license block".to_string(),
        });
    }
    if !expect_license && file.license.is_some() {
        return Err(PaletteLoadError {
            source_id: table_key,
            message: "LightPlayer original palette must not carry a license block".to_string(),
        });
    }

    Ok(PaletteEntry {
        id: file.id,
        name: file.name,
        category,
        license: file.license,
        gradient: file.gradient,
    })
}
