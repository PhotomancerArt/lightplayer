//! One catalog entry as the gallery surfaces show it.

use lpc_model::ProjectKind;

use super::embedded_example::EmbeddedExample;

/// A catalog card. Clicking one opens it in a transient session (viewing
/// is stateless, examples vision D2); an explicit save forks the copy
/// into the library with `SeededFrom { source: id }` provenance.
#[derive(Clone, Debug, PartialEq)]
pub struct UiExampleCard {
    /// Stable, bucket-free id, e.g. `catalog/plasma` — doubles as the
    /// seed-once provenance source.
    pub id: String,
    pub name: String,
    /// The manifest's authored kind: what section the card lands in
    /// ([`super::ui_example_groups::example_groups`]) and the word a
    /// picker's provenance tag reads.
    pub kind: ProjectKind,
    /// The manifest's one-line blurb (`description`); empty when the entry
    /// authors none, and then no element renders for it.
    pub description: String,
}

impl UiExampleCard {
    /// The card for one registry entry — the one projection every surface
    /// shares (home landing, Explore, the device picker, stories).
    pub fn from_embedded(example: &EmbeddedExample) -> Self {
        Self {
            id: example.id.to_string(),
            name: example.name.to_string(),
            kind: example.kind.clone(),
            description: example.description.to_string(),
        }
    }

    /// The kind as a display word (`General` | `Pattern` | `Show` | `Rig`).
    pub fn kind_label(&self) -> &'static str {
        crate::app::library::package_manifest::kind_label(&self.kind)
    }
}
