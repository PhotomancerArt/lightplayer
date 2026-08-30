//! Add-node picker data (controller-produced, pane-grammar style).

use lpc_model::{LpFeature, NodeKind};

use crate::{ControllerId, UiAction};

use super::node_create_op::{NodeCreateOp, UiAttachTarget};
use super::node_import_op::NodeImportOp;
use super::node_naming::{node_kind_label, node_kind_slug};

/// Picker order: the common authoring targets first, hardware-/niche kinds
/// last. Stable — the picker never reorders. `Module` sits with the other
/// container (settled D-C: an empty module is creatable, and everything
/// composable should be authorable).
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
    NodeKind::ControlRadio,
];

/// The add-node picker's data: one entry per instantiable kind, in stable
/// order. Exposed on [`crate::ProjectEditorView`] (project pane "+", attach
/// = project root) and on a playlist card's [`crate::UiNodeView`] (strip
/// "+", attach = that playlist).
#[derive(Clone, Debug, PartialEq)]
pub struct UiAddNodeMenu {
    pub entries: Vec<UiAddNodeMenuEntry>,
    /// Where this menu's creates attach. Carried alongside the entries so
    /// the picker can offer sources the controller cannot pre-build an
    /// action for — paste needs the clipboard's contents, which only the
    /// browser edge can read (`docs/adr/2026-07-28-share-envelopes.md`).
    pub attach: UiAttachTarget,
    /// The **import** source (module authoring unit, P5): one row per
    /// pattern export the local library offers, each dispatching a
    /// [`NodeImportOp`] that vendors the folder into this project.
    ///
    /// The picker's third source after kinds and the clipboard. Empty on
    /// every non-root menu — this round vendors into the project `nodes`
    /// map only — in which case [`Self::imports_empty`] is `None` too and
    /// the renderer draws no section at all.
    pub imports: Vec<UiAddNodeMenuEntry>,
    /// Why the import section has nothing to offer, when the section is
    /// still worth drawing: an empty library should say so (one disabled
    /// row) rather than leave a hole where a source used to be. `None`
    /// means "draw nothing" — either the rows are there, or this menu is
    /// not an import site.
    pub imports_empty: Option<String>,
}

/// One pattern export the local library can vendor into the open project.
///
/// Built from the same gallery snapshot the home cards come from (the
/// studio controller pushes it in at each library settle) — the picker is
/// a view, and a view never reaches for a store.
#[derive(Clone, Debug, PartialEq)]
pub struct UiImportablePattern {
    /// Source package `prj_…` uid — what the import op resolves.
    pub package_uid: String,
    /// The package's slug: the row's package half.
    pub package_label: String,
    /// The export folder's name inside that package (`effect`, `fire`).
    pub export: String,
    /// The package designates more than one export, so the row has to say
    /// WHICH one (`sparkle-pack · fire`). A single-export package reads as
    /// its own name — the common case, and the quieter row.
    pub family: bool,
}

/// Copy for the empty import section.
const NO_PATTERNS_COPY: &str = "No patterns in your library";

/// Attach the import source to a menu: one row per `patterns` entry,
/// skipping `exclude_uid` (the open project cannot import from itself —
/// its export folder is already right there).
///
/// Only the project-root site gets the source this round; a playlist's
/// picker keeps the two it had, with no empty-state row to explain a
/// section it never offered.
pub fn set_import_source(
    menu: &mut UiAddNodeMenu,
    patterns: &[UiImportablePattern],
    exclude_uid: Option<&str>,
) {
    if !matches!(menu.attach, UiAttachTarget::ProjectRoot) {
        menu.imports = Vec::new();
        menu.imports_empty = None;
        return;
    }
    menu.imports = patterns
        .iter()
        .filter(|pattern| exclude_uid != Some(pattern.package_uid.as_str()))
        .map(|pattern| import_entry(pattern, &menu.attach))
        .collect();
    menu.imports_empty = menu
        .imports
        .is_empty()
        .then(|| NO_PATTERNS_COPY.to_string());
}

/// One import row. Same entry shape as a kind row — glyph, label, ready
/// action — so the picker renders both through one component.
fn import_entry(pattern: &UiImportablePattern, attach: &UiAttachTarget) -> UiAddNodeMenuEntry {
    let label = if pattern.family {
        format!("{} · {}", pattern.package_label, pattern.export)
    } else {
        pattern.package_label.clone()
    };
    UiAddNodeMenuEntry {
        kind: NodeKind::Module,
        label,
        icon: node_kind_slug(NodeKind::Module).to_string(),
        action: UiAction::from_op(
            ControllerId::new(crate::ProjectController::NODE_ID),
            NodeImportOp {
                package_uid: pattern.package_uid.clone(),
                export: pattern.export.clone(),
                attach: attach.clone(),
            },
        )
        .with_label(format!("Import {}", pattern.export))
        .with_summary(format!(
            "Copy {}'s {} module into this project.",
            pattern.package_label, pattern.export
        )),
        unavailable: None,
    }
}

/// One picker entry. `action` is the ready-to-dispatch create (pane grammar:
/// actions are controller-produced data; the renderer never assembles ops).
#[derive(Clone, Debug, PartialEq)]
pub struct UiAddNodeMenuEntry {
    pub kind: NodeKind,
    /// Human-readable kind label ("Shader", "Compute shader", …).
    pub label: String,
    /// Icon token for the renderer (the kind's name slug).
    pub icon: String,
    /// Dispatches [`NodeCreateOp`] for this kind at the menu's attach site.
    pub action: UiAction,
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
        imports_empty: None,
        entries: PICKER_KINDS
            .iter()
            .filter(|kind| kind_fits_attach(**kind, attach))
            .map(|kind| {
                let label = node_kind_label(*kind);
                UiAddNodeMenuEntry {
                    kind: *kind,
                    label: label.to_string(),
                    icon: node_kind_slug(*kind).to_string(),
                    action: UiAction::from_op(
                        ControllerId::new(crate::ProjectController::NODE_ID),
                        NodeCreateOp {
                            kind: *kind,
                            attach: attach.clone(),
                        },
                    )
                    .with_label(format!("Add {label}"))
                    .with_summary(format!("Create a new {} node.", label.to_lowercase())),
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

    #[test]
    fn menu_offers_every_kind_in_stable_order() {
        let menu = add_node_menu(&UiAttachTarget::ProjectRoot);

        assert_eq!(menu.entries.len(), 11, "every instantiable kind");
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

    #[test]
    fn entry_actions_dispatch_create_at_the_menu_site() {
        let playlist = UiAttachTarget::Playlist {
            node: crate::ProjectNodeAddress::parse("/demo.module/loop.playlist").unwrap(),
        };
        let menu = add_node_menu(&playlist);
        let entry = &menu.entries[0];

        assert!(entry.action.is_for_node(crate::ProjectController::NODE_ID));
        let op = entry.action.op_as::<NodeCreateOp>().expect("create op");
        assert_eq!(op.kind, NodeKind::Shader);
        assert_eq!(op.attach, playlist);
        assert_eq!(entry.action.meta().label, "Add Shader");
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
            package_uid: uid.to_string(),
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
        let op = menu.imports[1]
            .action
            .op_as::<NodeImportOp>()
            .expect("import op");
        assert_eq!(op.package_uid, "prj_b");
        assert_eq!(op.export, "fire");
        assert_eq!(op.attach, UiAttachTarget::ProjectRoot);
        assert_eq!(menu.imports[1].kind, NodeKind::Module);
    }

    /// An empty library says so on a row rather than dropping the source.
    #[test]
    fn an_empty_library_keeps_the_section_and_explains_itself() {
        let mut menu = add_node_menu(&UiAttachTarget::ProjectRoot);
        set_import_source(&mut menu, &[], None);
        assert!(menu.imports.is_empty());
        assert_eq!(menu.imports_empty.as_deref(), Some(NO_PATTERNS_COPY));
    }

    /// This round vendors into the project `nodes` map only, so a
    /// playlist's picker gets no import section at all — not an empty one.
    #[test]
    fn a_playlist_menu_offers_no_import_section() {
        let mut menu = add_node_menu(&UiAttachTarget::Playlist {
            node: crate::ProjectNodeAddress::parse("/demo.module/loop.playlist").unwrap(),
        });
        set_import_source(
            &mut menu,
            &[pattern("prj_a", "aurora", "effect", false)],
            None,
        );
        assert!(menu.imports.is_empty());
        assert_eq!(menu.imports_empty, None);
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
