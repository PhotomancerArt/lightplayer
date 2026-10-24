//! Which layer a control's value came from — the panel's answer to "why".

use crate::preset::PresetId;

/// The four layers, bottom to top. Within the activated presets, the
/// presets' own order decides (later in the list is higher).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layer {
    /// What the project's author set.
    Project,
    /// The baseline the user set: a preset that never ends.
    PanelDefaults,
    /// An active preset.
    Preset(PresetId),
    /// The user's temporary changes, on top of everything.
    Overrides,
}
