//! One example package as the gallery's *Examples* section shows it.

/// An example card: embedded packages until the examples place lands (M6).
/// Clicking one seeds it into the library (once) and opens the copy — the
/// window-shopper path.
#[derive(Clone, Debug, PartialEq)]
pub struct UiExampleCard {
    /// Stable example id, e.g. `catalog/plasma` — doubles as the seed-once
    /// provenance source.
    pub id: String,
    pub name: String,
    /// Package kind, for the section's kind filter chips.
    pub kind: String,
}

/// The `kind` every catalog card carries today: the root module node's
/// kind, which is what the card icon keys on. Every catalog entry's root
/// `module.json` is a `Module`; the grouped surfaces (P5) replace this
/// string with the manifest's project kind.
pub const EXAMPLE_CARD_KIND: &str = "Module";
