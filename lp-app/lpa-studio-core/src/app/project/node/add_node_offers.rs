//! The add-node picker's verbs, as offers: `add-node` (a `kind` choice),
//! `import-pattern` (a `pattern` choice) and `paste-node` (the clipboard's
//! text), published once per picker at the menu's
//! [`UiAddNodeMenu::offers_at`] — `project/…` for the project root's
//! picker, `project/<playlist>/…` for a playlist's.
//!
//! One offer per verb, its choices the picker's rows (the board-ids ADR's
//! "one offer with a choice list"): the web's picker draws the rows from the
//! menu and presses these offers with a row's value, and the app agent
//! presses them by path with the same value. The offers are built from the
//! menu **after** the device gate, so a kind the board cannot run is a
//! disabled option here exactly as it is a disabled row there.

use lpc_model::NodeKind;

use super::node_create_op::{NodeCreateOp, UiAttachTarget};
use super::node_import_op::{ImportSource, NodeImportOp};
use super::node_share_op::NodePasteOp;
use super::ui_add_node_menu::{IMPORT_BUILTIN_SECTION, IMPORT_LIBRARY_SECTION, UiAddNodeMenu};
use crate::{
    ControllerId, NODE_KIND, OfferArgError, OfferArgs, OfferBinder, OfferChoice, OfferParam,
    ProjectController, UiAction, UiOffer, UiOfferTree, peek_header,
};

/// The verb that creates a blank node of a picked kind.
pub const ADD_NODE_VERB: &str = "add-node";
/// [`ADD_NODE_VERB`]'s parameter: the kind, by its slug (`shader`).
pub const ADD_NODE_KIND_PARAM: &str = "kind";
/// The verb that vendors a pattern from the library or the catalog.
pub const IMPORT_PATTERN_VERB: &str = "import-pattern";
/// [`IMPORT_PATTERN_VERB`]'s parameter: the pattern (`catalog/comet`).
pub const IMPORT_PATTERN_PARAM: &str = "pattern";
/// The verb that creates a node from a copied one.
pub const PASTE_NODE_VERB: &str = "paste-node";
/// [`PASTE_NODE_VERB`]'s parameter: the clipboard's text, which only the
/// browser can read — the web reads it and presses with it.
pub const PASTE_NODE_CLIPBOARD_PARAM: &str = "clipboard";

/// Publish `menu`'s verbs: `add-node` always, `import-pattern` when the
/// menu has import rows (only the project root's does today), and
/// `paste-node` always. All three are Routine: each adds something, and
/// Remove takes it away again.
pub fn publish_add_node_offers(offers: &mut UiOfferTree, menu: &UiAddNodeMenu) {
    offers.publish(add_node_offer(menu));
    if let Some(import) = import_pattern_offer(menu) {
        offers.publish(import);
    }
    offers.publish(paste_node_offer(menu));
}

/// `…/add-node`: one option per kind row, in the picker's order, a kind the
/// connected board cannot run disabled with the row's reason.
fn add_node_offer(menu: &UiAddNodeMenu) -> UiOffer {
    let options = menu
        .entries
        .iter()
        .map(|entry| {
            with_reason(
                OfferChoice::new(&entry.value, &entry.label),
                &entry.unavailable,
            )
        })
        .collect();
    let kinds: Vec<(String, NodeKind, String)> = menu
        .entries
        .iter()
        .map(|entry| (entry.value.clone(), entry.kind, entry.label.clone()))
        .collect();
    let attach = menu.attach.clone();
    let action = move |kind: NodeKind| {
        UiAction::from_op(
            ControllerId::new(ProjectController::NODE_ID),
            NodeCreateOp {
                kind,
                attach: attach.clone(),
            },
        )
    };
    let unbound = action(
        menu.entries
            .first()
            .map_or(NodeKind::Shader, |entry| entry.kind),
    )
    .with_summary(add_summary(&menu.attach));
    let binder = OfferBinder::new(move |args: &OfferArgs| {
        let picked = args.choice(ADD_NODE_KIND_PARAM).unwrap_or_default();
        let (_, kind, label) = kinds
            .iter()
            .find(|(value, ..)| value == picked)
            .ok_or_else(|| missing(ADD_NODE_KIND_PARAM, "kind"))?;
        Ok(action(*kind)
            .with_label(format!("Add {label}"))
            .with_summary(format!("Create a new {} node.", label.to_lowercase())))
    });
    UiOffer::with_params(
        menu.offers_at().child(ADD_NODE_VERB),
        "add",
        vec![OfferParam::choice(
            ADD_NODE_KIND_PARAM,
            "kind",
            options,
            None,
        )],
        binder,
        unbound,
    )
}

/// One import option's value, what it vendors, and how its press reads.
struct ImportPick {
    value: String,
    source: ImportSource,
    export: String,
    label: String,
    summary: String,
}

/// `…/import-pattern`: one option per import row, the library's first
/// (detail "Your library"), then the catalog's ("Built-in"). `None` when
/// the menu is not an import site.
fn import_pattern_offer(menu: &UiAddNodeMenu) -> Option<UiOffer> {
    let mut options = Vec::new();
    let mut picks = Vec::new();
    for entry in menu.import_rows() {
        let Some((source, export)) = menu.import_source(&entry.value) else {
            continue;
        };
        let section = match source {
            ImportSource::Library { .. } => IMPORT_LIBRARY_SECTION,
            ImportSource::BuiltIn { .. } => IMPORT_BUILTIN_SECTION,
        };
        options.push(with_reason(
            OfferChoice::new(&entry.value, &entry.label).with_detail(section),
            &entry.unavailable,
        ));
        picks.push(ImportPick {
            value: entry.value.clone(),
            source: source.clone(),
            export: export.to_string(),
            label: entry.label.clone(),
            summary: entry.summary.clone(),
        });
    }
    let first = picks.first()?;
    let attach = menu.attach.clone();
    let action = move |source: &ImportSource, export: &str| {
        UiAction::from_op(
            ControllerId::new(ProjectController::NODE_ID),
            NodeImportOp {
                source: source.clone(),
                export: export.to_string(),
                attach: attach.clone(),
            },
        )
    };
    let unbound = action(&first.source, &first.export);
    let binder = OfferBinder::new(move |args: &OfferArgs| {
        let picked = args.choice(IMPORT_PATTERN_PARAM).unwrap_or_default();
        let pick = picks
            .iter()
            .find(|pick| pick.value == picked)
            .ok_or_else(|| missing(IMPORT_PATTERN_PARAM, "pattern"))?;
        Ok(action(&pick.source, &pick.export)
            .with_label(format!("Import {}", pick.label))
            .with_summary(pick.summary.clone()))
    });
    Some(UiOffer::with_params(
        menu.offers_at().child(IMPORT_PATTERN_VERB),
        "add",
        vec![OfferParam::choice(
            IMPORT_PATTERN_PARAM,
            "pattern",
            options,
            None,
        )],
        binder,
        unbound,
    ))
}

/// `…/paste-node`: create a node from a copied `lp.node` envelope. The
/// clipboard is the browser's to read, so the press carries its text; the
/// binder refuses text that is not a copied node (another envelope kind, or
/// not an envelope at all) before anything is sent. A copied node that is
/// a node but cannot be pasted here (an older format) is the controller's
/// to refuse, as before, with the envelope's own words.
fn paste_node_offer(menu: &UiAddNodeMenu) -> UiOffer {
    let attach = menu.attach.clone();
    let action = move |envelope: String| {
        UiAction::from_op(
            ControllerId::new(ProjectController::NODE_ID),
            NodePasteOp {
                envelope,
                attach: attach.clone(),
            },
        )
    };
    let unbound = action(String::new());
    let binder = OfferBinder::new(move |args: &OfferArgs| {
        let text = args.text(PASTE_NODE_CLIPBOARD_PARAM).unwrap_or_default();
        match peek_header(text) {
            Ok(header) if header.kind == NODE_KIND => Ok(action(text.to_string())),
            Ok(header) => Err(OfferArgError::Invalid {
                name: PASTE_NODE_CLIPBOARD_PARAM.to_string(),
                reason: format!("it holds an {} envelope, not a copied node", header.kind),
            }),
            Err(error) => Err(OfferArgError::Invalid {
                name: PASTE_NODE_CLIPBOARD_PARAM.to_string(),
                reason: error.to_string(),
            }),
        }
    });
    UiOffer::with_params(
        menu.offers_at().child(PASTE_NODE_VERB),
        "copy",
        vec![OfferParam::text(
            PASTE_NODE_CLIPBOARD_PARAM,
            "copied node",
            "the copied node's JSON, as the clipboard holds it",
        )],
        binder,
        unbound,
    )
}

/// What the unbound `add-node` says it does, by site.
fn add_summary(attach: &UiAttachTarget) -> String {
    match attach {
        UiAttachTarget::ProjectRoot => "Create a new node in this project.".to_string(),
        UiAttachTarget::Playlist { .. } => {
            "Create a new node as this playlist's next entry.".to_string()
        }
    }
}

/// `choice`, disabled with `reason` when there is one.
fn with_reason(choice: OfferChoice, reason: &Option<String>) -> OfferChoice {
    match reason {
        Some(reason) => choice.disabled(reason.clone()),
        None => choice,
    }
}

/// A binder handed a value it does not know. [`UiOffer::press`] checks the
/// value against the options first, so only a binding with no value at all
/// gets here.
fn missing(name: &str, label: &str) -> OfferArgError {
    OfferArgError::Missing {
        name: name.to_string(),
        label: label.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use lpc_model::LpFeature;

    use super::*;
    use crate::app::project::node::{add_node_menu, gate_add_node_menu, set_import_source};
    use crate::{OfferParamKind, OfferPath, ProjectNodeAddress, UiImportablePattern};

    #[test]
    fn add_node_takes_a_kind_and_creates_it_at_the_menu_site() {
        let playlist = ProjectNodeAddress::parse("/demo.module/loop.playlist").unwrap();
        let attach = UiAttachTarget::Playlist {
            node: playlist.clone(),
        };
        let offers = published(&add_node_menu(&attach));
        let add = offers
            .get(&OfferPath::project_node(&playlist).child(ADD_NODE_VERB))
            .expect("the playlist's add-node");
        assert!(!add.is_enabled(), "nothing is preselected");
        assert!(add.consequence().is_routine());

        let pressed = add
            .press(&OfferArgs::new().with(ADD_NODE_KIND_PARAM, "fluid"))
            .expect("fluid fits a playlist");
        let op = pressed.op_as::<NodeCreateOp>().expect("a create");
        assert_eq!(op.kind, NodeKind::Fluid);
        assert_eq!(op.attach, attach);
        assert_eq!(pressed.meta().label, "Add Fluid");
        assert!(
            add.press(&OfferArgs::new().with(ADD_NODE_KIND_PARAM, "clock"))
                .is_err(),
            "a clock is no playlist entry"
        );
    }

    #[test]
    fn a_kind_the_board_cannot_run_is_a_disabled_option() {
        let mut menu = add_node_menu(&UiAttachTarget::ProjectRoot);
        gate_add_node_menu(&mut menu, Some(&[LpFeature::NodeShader]));
        let offers = published(&menu);
        let add = offers
            .get(&OfferPath::project().child(ADD_NODE_VERB))
            .unwrap();
        assert_eq!(
            add.press(&OfferArgs::new().with(ADD_NODE_KIND_PARAM, "fluid"))
                .map_err(|error| error.to_string()),
            Err("`kind` cannot be `fluid` right now: Not on this device".to_string())
        );
        assert!(
            add.press(&OfferArgs::new().with(ADD_NODE_KIND_PARAM, "shader"))
                .is_ok()
        );
    }

    #[test]
    fn import_pattern_lists_the_library_then_the_catalog() {
        let mut menu = add_node_menu(&UiAttachTarget::ProjectRoot);
        set_import_source(
            &mut menu,
            &[UiImportablePattern {
                source: ImportSource::Library {
                    package_uid: "prj_a".to_string(),
                },
                package_label: "aurora".to_string(),
                export: "effect".to_string(),
                family: false,
            }],
            None,
        );
        let offers = published(&menu);
        let import = offers
            .get(&OfferPath::project().child(IMPORT_PATTERN_VERB))
            .expect("the root imports");
        let OfferParamKind::Choice { options, .. } = &import.params()[0].kind else {
            panic!("a choice");
        };
        assert_eq!(options[0].value, "library/prj_a");
        assert_eq!(options[0].detail.as_deref(), Some(IMPORT_LIBRARY_SECTION));
        assert!(options[1].value.starts_with("catalog/"));
        assert_eq!(options[1].detail.as_deref(), Some(IMPORT_BUILTIN_SECTION));

        let pressed = import
            .press(&OfferArgs::new().with(IMPORT_PATTERN_PARAM, "catalog/comet"))
            .expect("comet is in the catalog");
        let op = pressed.op_as::<NodeImportOp>().expect("an import");
        assert_eq!(
            op.source,
            ImportSource::BuiltIn {
                example_id: "catalog/comet".to_string()
            }
        );
        assert_eq!(op.export, "effect");
        assert_eq!(op.attach, UiAttachTarget::ProjectRoot);
        assert_eq!(pressed.meta().label, "Import Comet");
    }

    #[test]
    fn a_menu_with_no_import_rows_publishes_no_import() {
        let offers = published(&add_node_menu(&UiAttachTarget::ProjectRoot));
        assert!(
            offers
                .get(&OfferPath::project().child(IMPORT_PATTERN_VERB))
                .is_none()
        );
        assert!(
            offers
                .get(&OfferPath::project().child(PASTE_NODE_VERB))
                .is_some()
        );
    }

    #[test]
    fn paste_node_takes_a_copied_node_and_refuses_anything_else() {
        let offers = published(&add_node_menu(&UiAttachTarget::ProjectRoot));
        let paste = offers
            .get(&OfferPath::project().child(PASTE_NODE_VERB))
            .unwrap();
        assert!(!paste.is_enabled(), "it waits for the clipboard's text");

        let node = crate::app::share::NodeEnvelope::encode(
            "Orbit",
            "./orbit.json",
            br#"{"kind":"Shader"}"#,
            &[],
        )
        .to_json()
        .unwrap();
        let pressed = paste
            .press(&OfferArgs::new().with(PASTE_NODE_CLIPBOARD_PARAM, &node))
            .expect("a copied node pastes");
        let op = pressed.op_as::<NodePasteOp>().expect("a paste");
        assert_eq!(op.envelope, node.trim());
        assert_eq!(op.attach, UiAttachTarget::ProjectRoot);

        let refused = paste
            .press(&OfferArgs::new().with(PASTE_NODE_CLIPBOARD_PARAM, "hello"))
            .unwrap_err();
        assert!(
            matches!(&refused, OfferArgError::Invalid { name, .. } if name == PASTE_NODE_CLIPBOARD_PARAM),
            "{refused}"
        );
    }

    fn published(menu: &UiAddNodeMenu) -> UiOfferTree {
        let mut offers = UiOfferTree::new();
        publish_add_node_offers(&mut offers, menu);
        offers
    }
}
