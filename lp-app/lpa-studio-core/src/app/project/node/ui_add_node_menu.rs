//! Add-node picker data (controller-produced, pane-grammar style).
//!
//! The picker's rows are presentation: a kind's glyph, its label, the
//! section it sits in, and why it is disabled. What a row *does* is an offer
//! in the view's offer tree ([`super::add_node_offers`]): the kind rows press
//! `…/add-node` with their [`UiAddNodeMenuEntry::value`] as `kind`, the
//! import rows press `…/import-pattern` with theirs as `pattern`. Both offers
//! are built from this same menu after the device gate, so a row and the
//! option it presses can never disagree.

use lpc_model::{LpFeature, NodeKind};

use crate::OfferPath;

use super::node_create_op::UiAttachTarget;
use super::node_import_op::ImportSource;
use super::node_naming::{node_kind_label, node_kind_slug};

/// Picker order: the common authoring targets first, hardware-/niche kinds
/// last. Stable — the picker never reorders. `Module` sits with the other
/// container (settled D-C: an empty module is creatable, and everything
/// composable should be authorable).
///
/// Must stay a permutation of [`NodeKind::ALL`] —
/// [`tests::picker_kinds_is_a_permutation_of_all_kinds`] fails on a kind
/// added without a picker placement, so a new kind cannot silently skip
/// every picker.
const PICKER_KINDS: &[NodeKind] = &[
    NodeKind::Shader,
    NodeKind::Texture,
    NodeKind::Playlist,
    NodeKind::Module,
    NodeKind::Clock,
    NodeKind::Fixture,
    NodeKind::Output,
    NodeKind::Fluid,
    NodeKind::ComputeShader,
    NodeKind::Button,
    NodeKind::PowerButton,
    NodeKind::ControlRadio,
];

/// The add-node picker's data: one entry per instantiable kind, in stable
/// order. Exposed on [`crate::ProjectEditorView`] (project pane "+", attach
/// = project root) and on a playlist card's [`crate::UiNodeView`] (strip
/// "+", attach = that playlist).
#[derive(Clone, Debug, PartialEq)]
pub struct UiAddNodeMenu {
    pub entries: Vec<UiAddNodeMenuEntry>,
    /// Where this menu's creates attach — which also says where its offers
    /// live ([`Self::offers_at`]): `add-node`, `import-pattern` and
    /// `paste-node`, the last of which takes the clipboard's contents that
    /// only the browser edge can read
    /// (`docs/adr/2026-07-28-share-envelopes.md`).
    pub attach: UiAttachTarget,
    /// The **import** source (module authoring unit, P5): one row per
    /// pattern export the local library offers, each pressing the menu's
    /// `import-pattern` offer, which vendors the folder into this project
    /// ([`super::NodeImportOp`]).
    ///
    /// The picker's third source after kinds and the clipboard. Empty on
    /// every non-root menu — this round vendors into the project `nodes`
    /// map only — in which case [`Self::imports_builtin`] is empty and
    /// [`Self::imports_empty`] is `None` too, and the renderer draws no
    /// section at all.
    pub imports: Vec<UiAddNodeMenuEntry>,
    /// The same source's second half (catalog content tree, P6): one row
    /// per built-in catalog pattern export, vendored from the compiled-in
    /// bytes. Rendered under [`IMPORT_BUILTIN_SECTION`] after the library's
    /// rows when both exist, heading-less when the library offers none.
    pub imports_builtin: Vec<UiAddNodeMenuEntry>,
    /// Why the import section has nothing to offer, when the section is
    /// still worth drawing: an empty library should say so (one disabled
    /// row) rather than leave a hole where a source used to be. `None`
    /// means "draw nothing" — either the rows are there, or this menu is
    /// not an import site.
    pub imports_empty: Option<String>,
    /// What each import row's `value` vendors: the source and the export
    /// folder. Core's own lookup for the `import-pattern` offer's binder —
    /// a renderer only ever hands the value back.
    import_sources: Vec<ImportChoice>,
}

/// One import row's value and what it vendors.
#[derive(Clone, Debug, PartialEq)]
struct ImportChoice {
    value: String,
    source: ImportSource,
    export: String,
}

impl UiAddNodeMenu {
    /// Where this menu's offers live: `project` for the project root's
    /// picker (`project/add-node`), the playlist's node path for a
    /// playlist's (`project/<playlist>/add-node`).
    pub fn offers_at(&self) -> OfferPath {
        match &self.attach {
            UiAttachTarget::ProjectRoot => OfferPath::project(),
            UiAttachTarget::Playlist { node } => OfferPath::project_node(node),
        }
    }

    /// What the import row whose value is `value` vendors: its source and
    /// export folder.
    pub(crate) fn import_source(&self, value: &str) -> Option<(&ImportSource, &str)> {
        self.import_sources
            .iter()
            .find(|choice| choice.value == value)
            .map(|choice| (&choice.source, choice.export.as_str()))
    }

    /// Every import row, library first, then built-in: the
    /// `import-pattern` offer's options, in the picker's order.
    pub(crate) fn import_rows(&self) -> impl Iterator<Item = &UiAddNodeMenuEntry> {
        self.imports.iter().chain(self.imports_builtin.iter())
    }
}

/// One pattern export the picker can vendor into the open project: a
/// library package's, or a built-in catalog pattern's.
///
/// Library rows are built from the same gallery snapshot the home cards
/// come from (the studio controller pushes them in at each library settle)
/// — the picker is a view, and a view never reaches for a store. Built-in
/// rows come from the catalog registry, which is compiled in.
#[derive(Clone, Debug, PartialEq)]
pub struct UiImportablePattern {
    /// Where the export is read from — what the import op resolves.
    pub source: ImportSource,
    /// The package's slug (library) or the entry's name (built-in): the
    /// row's package half.
    pub package_label: String,
    /// The export folder's name inside that package (`effect`, `fire`).
    pub export: String,
    /// The package designates more than one export, so the row has to say
    /// WHICH one (`sparkle-pack · fire`). A single-export package reads as
    /// its own name — the common case, and the quieter row.
    pub family: bool,
}

/// Copy for the empty import section — reachable only when the catalog
/// ships no patterns either, which a green tree never does.
const NO_PATTERNS_COPY: &str = "No patterns to import";

/// The heading over the library's rows when the built-in rows follow.
pub const IMPORT_LIBRARY_SECTION: &str = "Your library";
/// The heading over the catalog's rows when the library's rows precede.
pub const IMPORT_BUILTIN_SECTION: &str = "Built-in";

/// Attach the import source to a menu: one row per `patterns` entry,
/// skipping `exclude_uid` (the open project cannot import from itself —
/// its export folder is already right there), then one row per built-in
/// catalog pattern export (self-exclusion is a library matter; a catalog
/// entry is never the open project).
///
/// Both sites get the source: an import at the project root mounts the
/// vendored module in the root's `nodes`, one in a playlist becomes its
/// next entry (the app agent builds playlists of catalog patterns through
/// this same op, so the person can too).
pub fn set_import_source(
    menu: &mut UiAddNodeMenu,
    patterns: &[UiImportablePattern],
    exclude_uid: Option<&str>,
) {
    let excluded = |pattern: &&UiImportablePattern| match &pattern.source {
        ImportSource::Library { package_uid } => exclude_uid == Some(package_uid.as_str()),
        ImportSource::BuiltIn { .. } => false,
    };
    let library: Vec<&UiImportablePattern> = patterns
        .iter()
        .filter(|pattern| matches!(pattern.source, ImportSource::Library { .. }))
        .filter(|pattern| !excluded(pattern))
        .collect();
    let builtin = crate::app::home::home_view_builder::builtin_importable_patterns();
    menu.imports = library
        .iter()
        .map(|pattern| import_entry(pattern))
        .collect();
    menu.imports_builtin = builtin.iter().map(import_entry).collect();
    menu.import_sources = library
        .into_iter()
        .chain(builtin.iter())
        .map(|pattern| ImportChoice {
            value: import_value(pattern),
            source: pattern.source.clone(),
            export: pattern.export.clone(),
        })
        .collect();
    menu.imports_empty = (menu.imports.is_empty() && menu.imports_builtin.is_empty())
        .then(|| NO_PATTERNS_COPY.to_string());
}

/// An import row's choice value: `catalog/<slug>` for a built-in pattern
/// (its registry id, the name the app agent's catalog reference uses),
/// `library/<package uid>` for a library package's, with `/<export>` after
/// either when the package designates more than one export.
fn import_value(pattern: &UiImportablePattern) -> String {
    let package = match &pattern.source {
        ImportSource::Library { package_uid } => format!("library/{package_uid}"),
        ImportSource::BuiltIn { example_id } => example_id.clone(),
    };
    if pattern.family {
        format!("{package}/{}", pattern.export)
    } else {
        package
    }
}

/// One import row. Same entry shape as a kind row — glyph, label, the value
/// its offer takes — so the picker renders both through one component.
fn import_entry(pattern: &UiImportablePattern) -> UiAddNodeMenuEntry {
    let label = if pattern.family {
        format!("{} · {}", pattern.package_label, pattern.export)
    } else {
        pattern.package_label.clone()
    };
    let summary = match &pattern.source {
        ImportSource::Library { .. } => format!(
            "Copy {}'s {} module into this project.",
            pattern.package_label, pattern.export
        ),
        ImportSource::BuiltIn { .. } => format!(
            "Copy the built-in {} pattern's {} module into this project.",
            pattern.package_label, pattern.export
        ),
    };
    UiAddNodeMenuEntry {
        kind: NodeKind::Module,
        label,
        icon: node_kind_slug(NodeKind::Module).to_string(),
        value: import_value(pattern),
        summary,
        unavailable: None,
    }
}

/// One picker entry: what the row shows, and the value its offer takes.
/// The row presses an offer from the view's offer tree — never an action
/// of its own (pane grammar: the renderer never assembles ops).
#[derive(Clone, Debug, PartialEq)]
pub struct UiAddNodeMenuEntry {
    pub kind: NodeKind,
    /// Human-readable kind label ("Shader", "Compute shader", …).
    pub label: String,
    /// Icon token for the renderer (the kind's name slug).
    pub icon: String,
    /// The value the row's offer takes: the `kind` of the menu's
    /// `add-node` offer (the kind's slug, `shader`) for a kind row, the
    /// `pattern` of its `import-pattern` offer for an import row
    /// (`catalog/comet`).
    pub value: String,
    /// What the row does, in a sentence (the row's tooltip).
    pub summary: String,
    /// Why this entry is unavailable, when it is — the connected device's
    /// firmware carries no runtime for the kind. `None` = offer it.
    ///
    /// Unavailable kinds are DISABLED, never hidden: a picker that silently
    /// drops entries teaches the wrong catalog, and "why can't I add a
    /// Fluid?" has no answer if the row is not there to carry one.
    pub unavailable: Option<String>,
}

/// Whether a kind belongs in `attach`'s picker at all. The project root
/// hosts anything; a playlist's entries hold visual children — the playlist
/// blends its entries' outputs into its own (`PlaylistState.output`) — so
/// only kinds whose runtime publishes a visual product fit.
///
/// Site fit FILTERS where the device gate disables: an unavailable kind is
/// part of the catalog and the row carries the "why not", but a kind that
/// can never be a playlist entry is not part of the entry picker's catalog,
/// and a permanent row of never-enabled kinds teaches nothing.
fn kind_fits_attach(kind: NodeKind, attach: &UiAttachTarget) -> bool {
    match attach {
        UiAttachTarget::ProjectRoot => true,
        UiAttachTarget::Playlist { .. } => kind.produces_visual(),
    }
}

/// Build the picker for one attach site: every instantiable kind that fits
/// the site ([`kind_fits_attach`]), in [`PICKER_KINDS`] order, with every
/// entry enabled.
///
/// The device gate is applied afterwards by [`gate_add_node_menu`], once,
/// where the lens session is known — menus are built in several places and
/// only one of them can see the device.
pub fn add_node_menu(attach: &UiAttachTarget) -> UiAddNodeMenu {
    UiAddNodeMenu {
        attach: attach.clone(),
        // The import source is attached afterwards by [`set_import_source`],
        // where the library snapshot is known — same "build then narrow"
        // shape as the device gate below.
        imports: Vec::new(),
        imports_builtin: Vec::new(),
        imports_empty: None,
        import_sources: Vec::new(),
        entries: PICKER_KINDS
            .iter()
            .filter(|kind| kind_fits_attach(**kind, attach))
            .map(|kind| {
                let label = node_kind_label(*kind);
                UiAddNodeMenuEntry {
                    kind: *kind,
                    label: label.to_string(),
                    icon: node_kind_slug(*kind).to_string(),
                    value: node_kind_slug(*kind).to_string(),
                    summary: format!("Create a new {} node.", label.to_lowercase()),
                    unavailable: None,
                }
            })
            .collect(),
    }
}

/// Disable the entries the connected device cannot run.
///
/// `device_features` is what that device's hello reported. **`None` means
/// "no device has said otherwise" (a sim/host lens, or a link that is not
/// Ready yet) and everything stays enabled** — gating only ever narrows
/// when a real device affirmatively reports its build. Idempotent, and it
/// never re-enables an entry.
pub fn gate_add_node_menu(menu: &mut UiAddNodeMenu, device_features: Option<&[LpFeature]>) {
    let Some(features) = device_features else {
        return;
    };
    // Imports land as module nodes, so they take the same gate — a board
    // with no module runtime cannot run what the vendoring would write.
    for entry in menu.entries.iter_mut().chain(menu.imports.iter_mut()) {
        if entry.unavailable.is_none() && kind_is_missing(entry.kind, features) {
            entry.unavailable = Some(UNAVAILABLE_COPY.to_string());
        }
    }
}

/// Picker annotation for a kind the device's firmware does not carry — the
/// same "Not on this device" family the tree status uses.
const UNAVAILABLE_COPY: &str = "Not on this device";

/// Whether the device's reported features lack this kind's runtime. Ungated
/// kinds ([`LpFeature::for_node_kind`] → `None`) are always available.
fn kind_is_missing(kind: NodeKind, features: &[LpFeature]) -> bool {
    LpFeature::for_node_kind(kind).is_some_and(|required| !features.contains(&required))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The one non-compile-checked seam in the "new kind must be placed"
    /// chain: `NodeKind::ALL` and `produces_visual` are wildcard-free (a
    /// new variant fails to compile until placed), but `PICKER_KINDS` is a
    /// hand-ordered list. Enforce that it names every kind exactly once so
    /// a new kind cannot ship without a deliberate picker placement.
    #[test]
    fn picker_kinds_is_a_permutation_of_all_kinds() {
        assert_eq!(PICKER_KINDS.len(), NodeKind::ALL.len());
        for kind in NodeKind::ALL {
            assert_eq!(
                PICKER_KINDS.iter().filter(|k| **k == kind).count(),
                1,
                "{kind:?} must appear exactly once in PICKER_KINDS"
            );
        }
    }

    #[test]
    fn menu_offers_every_kind_in_stable_order() {
        let menu = add_node_menu(&UiAttachTarget::ProjectRoot);

        assert_eq!(
            menu.entries.len(),
            NodeKind::ALL.len(),
            "every instantiable kind"
        );
        assert!(menu.entries.iter().any(|e| e.kind == NodeKind::Module));
        assert_eq!(menu.entries[0].kind, NodeKind::Shader);
        assert_eq!(menu.entries[0].label, "Shader");
        assert_eq!(menu.entries[0].icon, "shader");
        // Rebuilding yields the identical menu (stable order, stable data).
        assert_eq!(menu, add_node_menu(&UiAttachTarget::ProjectRoot));
    }

    /// A playlist's picker offers only the kinds that can BE an entry —
    /// visual producers — in the same stable order. Everything else never
    /// enters this site's catalog (site fit filters; only the device gate
    /// disables).
    #[test]
    fn a_playlist_menu_offers_only_visual_kinds() {
        let menu = add_node_menu(&UiAttachTarget::Playlist {
            node: crate::ProjectNodeAddress::parse("/demo.module/loop.playlist").unwrap(),
        });

        let kinds: Vec<NodeKind> = menu.entries.iter().map(|entry| entry.kind).collect();
        assert_eq!(
            kinds,
            vec![
                NodeKind::Shader,
                NodeKind::Playlist,
                NodeKind::Module,
                NodeKind::Fluid,
            ]
        );
        assert!(menu.entries.iter().all(|e| e.unavailable.is_none()));
    }

    /// A kind row carries the kind's slug: the `kind` its menu's
    /// `add-node` offer takes, and where that offer lives follows the
    /// menu's attach site.
    #[test]
    fn kind_rows_carry_the_value_their_offer_takes() {
        let playlist = crate::ProjectNodeAddress::parse("/demo.module/loop.playlist").unwrap();
        let menu = add_node_menu(&UiAttachTarget::Playlist {
            node: playlist.clone(),
        });
        let entry = &menu.entries[0];
        assert_eq!(entry.value, "shader");
        assert_eq!(entry.summary, "Create a new shader node.");
        assert_eq!(menu.offers_at(), OfferPath::project_node(&playlist));
        assert_eq!(
            add_node_menu(&UiAttachTarget::ProjectRoot).offers_at(),
            OfferPath::project()
        );
    }

    /// A device that reports its build disables exactly the kinds it lacks
    /// — and never removes an entry.
    #[test]
    fn a_reporting_device_disables_only_the_kinds_it_lacks() {
        let mut menu = add_node_menu(&UiAttachTarget::ProjectRoot);
        let before = menu.entries.len();
        // A build with everything except the fluid and radio runtimes.
        let features = [
            LpFeature::NodeButton,
            LpFeature::NodeClock,
            LpFeature::NodeFixture,
            LpFeature::NodePlaylist,
            LpFeature::NodePowerButton,
            LpFeature::NodeShader,
            LpFeature::NodeTexture,
            LpFeature::GfxLpvm,
        ];
        gate_add_node_menu(&mut menu, Some(&features));

        assert_eq!(
            menu.entries.len(),
            before,
            "entries are disabled, never hidden"
        );
        let disabled: Vec<NodeKind> = menu
            .entries
            .iter()
            .filter(|entry| entry.unavailable.is_some())
            .map(|entry| entry.kind)
            .collect();
        assert_eq!(disabled, vec![NodeKind::Fluid, NodeKind::ControlRadio]);
        // Output is ungated in the engine, so it survives any feature set.
        let output = menu
            .entries
            .iter()
            .find(|entry| entry.kind == NodeKind::Output)
            .expect("output entry");
        assert_eq!(output.unavailable, None);
        // Shader and ComputeShader share one gate; both stay offered.
        assert!(
            menu.entries
                .iter()
                .filter(|entry| matches!(entry.kind, NodeKind::Shader | NodeKind::ComputeShader))
                .all(|entry| entry.unavailable.is_none())
        );
    }

    fn pattern(uid: &str, label: &str, export: &str, family: bool) -> UiImportablePattern {
        UiImportablePattern {
            source: ImportSource::Library {
                package_uid: uid.to_string(),
            },
            package_label: label.to_string(),
            export: export.to_string(),
            family,
        }
    }

    /// P5: the import source lists one row per export, names the export
    /// only when the package has more than one, and never offers the
    /// project you are standing in.
    #[test]
    fn the_import_source_lists_library_patterns_minus_the_open_one() {
        let patterns = [
            pattern("prj_a", "aurora", "effect", false),
            pattern("prj_b", "sparkle-pack", "fire", true),
            pattern("prj_b", "sparkle-pack", "ice", true),
            pattern("prj_self", "this-one", "effect", false),
        ];
        let mut menu = add_node_menu(&UiAttachTarget::ProjectRoot);
        set_import_source(&mut menu, &patterns, Some("prj_self"));

        let labels: Vec<&str> = menu.imports.iter().map(|e| e.label.as_str()).collect();
        assert_eq!(
            labels,
            vec!["aurora", "sparkle-pack · fire", "sparkle-pack · ice"],
        );
        assert_eq!(menu.imports_empty, None);
        let values: Vec<&str> = menu.imports.iter().map(|e| e.value.as_str()).collect();
        assert_eq!(
            values,
            vec!["library/prj_a", "library/prj_b/fire", "library/prj_b/ice"],
            "a family names its export; a single export reads as its package"
        );
        assert_eq!(
            menu.import_source("library/prj_b/fire"),
            Some((
                &ImportSource::Library {
                    package_uid: "prj_b".to_string()
                },
                "fire"
            ))
        );
        assert_eq!(menu.imports[1].kind, NodeKind::Module);
    }

    /// An empty library still fills the section with the catalog's own
    /// patterns (P6), heading-less — never a hole where the source was.
    #[test]
    fn an_empty_library_still_offers_the_built_in_patterns() {
        let mut menu = add_node_menu(&UiAttachTarget::ProjectRoot);
        set_import_source(&mut menu, &[], None);
        assert!(menu.imports.is_empty());
        assert_eq!(menu.imports_empty, None);
        assert!(
            menu.imports_builtin.len() >= 7,
            "every catalog pattern is offered: {:?}",
            menu.imports_builtin
                .iter()
                .map(|e| e.label.as_str())
                .collect::<Vec<_>>()
        );
        let value = &menu.imports_builtin[0].value;
        assert!(value.starts_with("catalog/"), "{value}");
        let (source, export) = menu.import_source(value).expect("its source");
        assert!(matches!(source, ImportSource::BuiltIn { .. }));
        assert_eq!(export, "effect");
    }

    /// A playlist's picker imports too, and each import row attaches to
    /// THIS playlist — the vendored module becomes its next entry (the app
    /// agent builds playlists of catalog patterns through the same op).
    #[test]
    fn a_playlist_menu_imports_into_the_playlist() {
        let playlist = crate::ProjectNodeAddress::parse("/demo.module/loop.playlist").unwrap();
        let mut menu = add_node_menu(&UiAttachTarget::Playlist {
            node: playlist.clone(),
        });
        set_import_source(
            &mut menu,
            &[pattern("prj_a", "aurora", "effect", false)],
            None,
        );
        assert_eq!(menu.imports.len(), 1);
        assert_eq!(menu.offers_at(), OfferPath::project_node(&playlist));
        assert!(!menu.imports_builtin.is_empty(), "catalog rows too");
    }

    /// No device has reported: nothing is gated. A sim lens must never be
    /// narrowed by a device that is not there.
    #[test]
    fn an_unknown_device_gates_nothing() {
        let mut menu = add_node_menu(&UiAttachTarget::ProjectRoot);
        gate_add_node_menu(&mut menu, None);
        assert!(menu.entries.iter().all(|entry| entry.unavailable.is_none()));
    }
}
