//! The checked-in Studio palette catalog (M3 of the palette plan): starter
//! set, WLED custom-palette import, no picker UI (M4).
//!
//! Boundary, mirroring `lpa-boards`: catalog data in, nothing out. This
//! crate owns two source trees under `assets/palettes/` —
//! `third-party/` (FastLED + cpt-city conversions, isolated behind a
//! license/attribution contract, see that directory's README) and
//! `originals/` (LightPlayer's own Oklab-authored palettes) — and merges
//! them into one [`PaletteEntry`] catalog via [`all_palettes`].
//!
//! `third_party` and `originals` are the only modules that reference their
//! respective asset directories (enforced by `tests/license_manifest.rs`);
//! everything else in this crate, and every other crate in the workspace,
//! goes through [`all_palettes`] / [`palette_by_id`].

mod catalog;
mod entry;
mod originals;
pub mod sample;
mod third_party;
pub mod wled_import;

pub use catalog::{all_palettes, palette_by_id};
pub use entry::{PaletteCategory, PaletteEntry, PaletteLicense, PaletteLoadError};
pub use sample::{
    from_display_srgb, sample_gradient_as_srgb, sample_linear, sample_step, to_display_srgb,
};
pub use wled_import::{WledImportError, import_wled_custom_palette};
