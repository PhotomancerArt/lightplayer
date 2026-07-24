//! One library package as the gallery shows it.

/// A "Your projects" card. The thumbnail is deliberately absent from the
/// model: the source is swappable by design (placeholder now, cached rendered
/// frame later) and lives entirely in the renderer.
#[derive(Clone, Debug, PartialEq)]
pub struct UiPackageCard {
    /// `prj_…` uid string — the identity every card action carries.
    pub uid: String,
    /// Manifest kind ("Project" today, "Module" later).
    pub kind: String,
    /// THE user-facing identifier (dated: `2026-07-09-1421-basic`): card
    /// title, URL, export name. Rename edits it.
    pub slug: String,
    /// The last `Saved` event's timestamp (f64 epoch seconds), or the
    /// package's creation time before any save.
    pub last_saved_at: Option<f64>,
    /// Human provenance line for remixes/forks/imports; `None` for
    /// created-from-scratch packages.
    pub provenance: Option<String>,
    /// Parity line: the name of a registered device currently holding this
    /// package's head, when one does ("On <name> ✓").
    pub on_device: Option<String>,
    /// Another tab holds this project open (its `lp-project` Web Lock).
    /// Structural actions refuse while set; the card gets the badge
    /// treatment (M4b P4).
    pub open_elsewhere: bool,
    /// A LIVE connected device currently holds this project — D24: one
    /// card, connected indication (no separate device card).
    pub connected_device: Option<UiCardConnection>,
    /// The live SIM session currently runs this project (the D28 grammar's
    /// sim arm — one fact, two views: the sim card wears the project chip,
    /// this card wears the "Running in simulator" indication). Independent
    /// of `connected_device`: a device and the sim can honestly run the
    /// same project at once.
    pub running_in_sim: bool,
}

/// The live-device indication a unified project card carries (D24).
#[derive(Clone, Debug, PartialEq)]
pub struct UiCardConnection {
    pub device_name: String,
    /// How the device's copy relates to the library line.
    pub relation: lpc_history::SyncRelation,
}
