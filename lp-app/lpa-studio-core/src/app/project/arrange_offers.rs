//! The Arrange canvas's commits, as offers (M6e): where a fixture sits on
//! the project's arrange canvas (`editor.json`), published on the fixture
//! it moves, under that node's `arrange` group.
//!
//! - **`project/<fixture>/arrange/set`** places one fixture: `x`, `y`,
//!   `rotation` (degrees) and `scale`, each optional — a value left out
//!   keeps the fixture's current one (identity while it has never been
//!   arranged). Numbers are text until offers get a number kind (M6c).
//! - **`project/arrange/undo`** and **`project/arrange/redo`** walk the
//!   arrange history, published only while it has a step, as the patch
//!   history's are.
//!
//! A drag presses ONCE, on release: the live preview while the pointer
//! moves stays the web's own (`drag_override`), and the commit is the
//! press. A multi-fixture drag presses each moved fixture's `set` and
//! hands the presses to [`arrange_batch`], which folds them into one write
//! and one undo step — every transform still checked by its own offer.
//!
//! **Levels.** All Routine: a placement is presentation only (never a
//! sampling input), and the arrange history's Undo puts any of it back.
//!
//! The `arrange` segment keeps these out of the node card's header
//! ([`crate::UiOfferTree::verbs_of`] draws exactly one segment under the
//! node), while the agent's readout and `read` count them as the node's own
//! ([`crate::UiOfferTree::own_verbs_of`]).

use lpc_model::NodeId;

use crate::{
    ControllerId, EditorMetaFixture, EditorMetaOp, EditorMetaSet, EditorMetaVerb, OfferArgError,
    OfferArgs, OfferBinder, OfferParam, OfferPath, ProjectController, ProjectNodeAddress, UiAction,
    UiArrangeTransform, UiOffer, UiOfferTree, UiPatchSurface,
};

/// The group a fixture's arrange verbs live in (`project/<fixture>/arrange/…`),
/// and the arrange history's (`project/arrange/…`).
pub const ARRANGE_GROUP: &str = "arrange";
/// Place a fixture on the arrange canvas.
pub const ARRANGE_SET_VERB: &str = "set";
/// Undo the last arrange edit.
pub const ARRANGE_UNDO_VERB: &str = "undo";
/// Redo the last undone arrange edit.
pub const ARRANGE_REDO_VERB: &str = "redo";

/// Where the fixture's origin sits, across.
pub const ARRANGE_X_PARAM: &str = "x";
/// Where the fixture's origin sits, up.
pub const ARRANGE_Y_PARAM: &str = "y";
/// The fixture's rotation, in degrees.
pub const ARRANGE_ROTATION_PARAM: &str = "rotation";
/// The fixture's uniform scale (1 is its own size).
pub const ARRANGE_SCALE_PARAM: &str = "scale";

/// The smallest and largest scale a placement takes — the canvas's own
/// limits on a scale gesture.
const SCALE_RANGE: (f64, f64) = (0.05, 20.0);

/// Publish every arrange verb the surface has a target for: each fixture's
/// `set` (one with a tree address, once the surface knows where
/// `editor.json` lives), then the history's while it has something to walk.
pub fn publish_arrange_offers(
    offers: &mut UiOfferTree,
    surface: &UiPatchSurface,
    can_undo: bool,
    can_redo: bool,
) {
    let Some(artifact) = surface.editor_meta_artifact.clone() else {
        return;
    };
    let site = ArrangeSite {
        artifact,
        fixtures: arrange_fixtures(surface),
        refused: surface.editor_meta_error.clone(),
    };
    for fixture in &surface.fixtures {
        let Some(key) = fixture.address.clone() else {
            continue;
        };
        let Some(at) = arrange_verbs_at(Some(&key)) else {
            continue;
        };
        let current = fixture
            .arrange
            .as_ref()
            .map(|arrange| arrange.transform)
            .unwrap_or_default();
        offers.publish(set_offer(
            &site,
            at,
            key,
            fixture.node,
            &fixture.label,
            current,
        ));
    }
    if can_undo {
        offers.publish(history_offer(&site, ARRANGE_UNDO_VERB));
    }
    if can_redo {
        offers.publish(history_offer(&site, ARRANGE_REDO_VERB));
    }
}

/// Where the arrange history's verb `verb` lives: `project/arrange/<verb>`.
pub fn arrange_history_path(verb: &str) -> OfferPath {
    OfferPath::project().child(ARRANGE_GROUP).child(verb)
}

/// Fold several presses of fixtures' `arrange/set` into ONE action: one
/// `editor.json` write and one arrange undo step, for a gesture that moved
/// a set of fixtures together. A single press comes back as it is. `None`
/// for no presses, or for an action that is not an arrange `set`.
pub fn arrange_batch(presses: Vec<UiAction>) -> Option<UiAction> {
    if presses.len() <= 1 {
        return presses.into_iter().next();
    }
    let mut base: Option<EditorMetaOp> = None;
    let mut entries = Vec::with_capacity(presses.len());
    for press in &presses {
        let op = press.op_as::<EditorMetaOp>()?;
        let EditorMetaVerb::Set {
            node_key,
            node,
            transform,
        } = &op.verb
        else {
            return None;
        };
        entries.push(EditorMetaSet {
            node_key: node_key.clone(),
            node: *node,
            transform: *transform,
        });
        base.get_or_insert_with(|| op.clone());
    }
    let base = base?;
    Some(
        arrange_action(EditorMetaOp {
            artifact: base.artifact,
            fixtures: base.fixtures,
            verb: EditorMetaVerb::SetMany { entries },
        })
        .with_label(format!("Arrange {} fixtures", presses.len())),
    )
}

impl UiPatchSurface {
    /// Where the arrange verbs of the fixture `node` live:
    /// `project/<fixture>/arrange`. `None` for a node the surface does not
    /// list as a fixture, or one it has no tree address for.
    pub fn arrange_verbs_of(&self, node: NodeId) -> Option<OfferPath> {
        let fixture = self.fixtures.iter().find(|fixture| fixture.node == node)?;
        arrange_verbs_at(fixture.address.as_deref())
    }
}

/// What every arrange verb shares.
struct ArrangeSite {
    artifact: lpc_model::ArtifactLocation,
    /// Every fixture whose cached map2d can refresh its footprint as part
    /// of a write.
    fixtures: Vec<EditorMetaFixture>,
    /// Why `editor.json` cannot be written (newer format, parse error).
    refused: Option<String>,
}

impl ArrangeSite {
    fn action(&self, verb: EditorMetaVerb) -> UiAction {
        let action = arrange_action(EditorMetaOp {
            artifact: self.artifact.clone(),
            fixtures: self.fixtures.clone(),
            verb,
        });
        match &self.refused {
            Some(reason) => action.disabled(format!("editor.json cannot be written: {reason}")),
            None => action,
        }
    }
}

/// `project/<node>/arrange` for a surface row's address text.
fn arrange_verbs_at(address: Option<&str>) -> Option<OfferPath> {
    let address = ProjectNodeAddress::parse(address?).ok()?;
    Some(OfferPath::project_node(&address).child(ARRANGE_GROUP))
}

/// Every fixture's footprint-refresh facts (the op carries them all).
fn arrange_fixtures(surface: &UiPatchSurface) -> Vec<EditorMetaFixture> {
    surface
        .fixtures
        .iter()
        .filter_map(|fixture| {
            Some(EditorMetaFixture {
                node_key: fixture.address.clone()?,
                mapping_artifact: fixture.mapping_artifact.clone(),
            })
        })
        .collect()
}

/// `…/arrange/set`: place the fixture. A value left out keeps `current`.
fn set_offer(
    site: &ArrangeSite,
    at: OfferPath,
    key: String,
    node: NodeId,
    label: &str,
    current: UiArrangeTransform,
) -> UiOffer {
    let set = {
        let key = key.clone();
        move |transform: UiArrangeTransform| EditorMetaVerb::Set {
            node_key: key.clone(),
            node: Some(node),
            transform,
        }
    };
    let unbound = site
        .action(set(current))
        .with_label(format!("Arrange {label}"))
        .with_summary("Place the fixture on the arrange canvas: where, turned how far, how big.");
    let refused = site.refused.clone();
    let artifact = site.artifact.clone();
    let fixtures = site.fixtures.clone();
    let label = label.to_string();
    let binder = OfferBinder::new(move |args: &OfferArgs| {
        let transform = UiArrangeTransform {
            t: [
                number(args, ARRANGE_X_PARAM, current.t[0])?,
                number(args, ARRANGE_Y_PARAM, current.t[1])?,
            ],
            r: number(args, ARRANGE_ROTATION_PARAM, current.r)?,
            s: scale(args, current.s)?,
        };
        let action = arrange_action(EditorMetaOp {
            artifact: artifact.clone(),
            fixtures: fixtures.clone(),
            verb: set(transform),
        })
        .with_label(format!("Arrange {label}"))
        .with_summary(format!(
            "Place {label} at ({}, {}), turned {}°, at {}×.",
            transform.t[0], transform.t[1], transform.r, transform.s
        ));
        Ok(match &refused {
            Some(reason) => action.disabled(format!("editor.json cannot be written: {reason}")),
            None => action,
        })
    });
    let param = |name: &str, label: &str, now: f64| {
        OfferParam::text(name, label, format!("{now} now; left blank, it stays")).optional()
    };
    UiOffer::with_params(
        at.child(ARRANGE_SET_VERB),
        "edit",
        vec![
            param(ARRANGE_X_PARAM, "x", current.t[0]),
            param(ARRANGE_Y_PARAM, "y", current.t[1]),
            param(ARRANGE_ROTATION_PARAM, "rotation in degrees", current.r),
            param(ARRANGE_SCALE_PARAM, "scale", current.s),
        ],
        binder,
        unbound,
    )
}

/// `project/arrange/undo` or `…/redo`.
fn history_offer(site: &ArrangeSite, verb: &str) -> UiOffer {
    let (op, label) = match verb {
        ARRANGE_UNDO_VERB => (EditorMetaVerb::Undo, "Undo arrange edit"),
        _ => (EditorMetaVerb::Redo, "Redo arrange edit"),
    };
    UiOffer::new(
        arrange_history_path(verb),
        "revert",
        site.action(op).with_label(label),
    )
}

/// An arrange op made into an action for the project controller.
fn arrange_action(op: EditorMetaOp) -> UiAction {
    UiAction::from_op(ControllerId::new(ProjectController::NODE_ID), op)
}

/// An optional text parameter read as a finite number, `current` when the
/// press left it out.
fn number(args: &OfferArgs, name: &str, current: f64) -> Result<f64, OfferArgError> {
    let Some(text) = args.text(name) else {
        return Ok(current);
    };
    match text.parse::<f64>() {
        Ok(value) if value.is_finite() => Ok(value),
        _ => Err(OfferArgError::Invalid {
            name: name.to_string(),
            reason: format!("`{text}` is not a number"),
        }),
    }
}

/// The `scale` parameter: a number inside [`SCALE_RANGE`].
fn scale(args: &OfferArgs, current: f64) -> Result<f64, OfferArgError> {
    if args.text(ARRANGE_SCALE_PARAM).is_none() {
        return Ok(current);
    }
    let value = number(args, ARRANGE_SCALE_PARAM, current)?;
    let (low, high) = SCALE_RANGE;
    if (low..=high).contains(&value) {
        Ok(value)
    } else {
        Err(OfferArgError::Invalid {
            name: ARRANGE_SCALE_PARAM.to_string(),
            reason: format!("a scale is between {low} and {high}, not {value}"),
        })
    }
}

#[cfg(test)]
mod tests {
    use lpc_model::ArtifactLocation;

    use super::*;
    use crate::{UiArrangeMeta, UiPatchSurfaceFixture};

    #[test]
    fn every_addressed_fixture_publishes_set_and_the_history_its_steps() {
        let offers = published(&surface(None), true, false);
        let paths: Vec<String> = offers.iter().map(|offer| offer.path.to_string()).collect();
        assert_eq!(
            paths,
            [
                "project/demo.module/dome.fixture/arrange/set",
                "project/demo.module/doors.fixture/arrange/set",
                "project/arrange/undo",
            ],
            "an unaddressed fixture has nowhere to live; redo has no step"
        );
        assert!(
            offers.iter().all(|offer| offer.consequence().is_routine()),
            "placement is presentation only"
        );
        assert_eq!(
            surface(None).arrange_verbs_of(NodeId::new(2)),
            Some(OfferPath::parse("project/demo.module/dome.fixture/arrange").unwrap())
        );
        let mut unsettled = surface(None);
        unsettled.editor_meta_artifact = None;
        assert!(
            published(&unsettled, true, true).is_empty(),
            "nowhere to write until the surface knows where editor.json lives"
        );
    }

    #[test]
    fn set_keeps_what_the_press_leaves_out() {
        let offers = published(&surface(None), false, false);
        let set = &offers[0];
        let action = set
            .press(&OfferArgs::new().with(ARRANGE_X_PARAM, "40"))
            .expect("x alone binds");
        assert_eq!(
            set_transform(&action),
            UiArrangeTransform {
                t: [40.0, 5.0],
                r: 45.0,
                s: 2.0
            },
            "y, rotation and scale stay where the dome sits now"
        );
        assert_eq!(
            set_transform(&set.action),
            UiArrangeTransform {
                t: [10.0, 5.0],
                r: 45.0,
                s: 2.0
            },
            "a press with nothing places it where it is"
        );
        for (name, value, reason) in [
            (ARRANGE_Y_PARAM, "up", "`up` is not a number"),
            (
                ARRANGE_SCALE_PARAM,
                "0",
                "a scale is between 0.05 and 20, not 0",
            ),
            (ARRANGE_ROTATION_PARAM, "inf", "`inf` is not a number"),
        ] {
            assert_eq!(
                set.press(&OfferArgs::new().with(name, value)),
                Err(OfferArgError::Invalid {
                    name: name.to_string(),
                    reason: reason.to_string()
                })
            );
        }
    }

    #[test]
    fn an_unreadable_editor_json_disables_every_write() {
        let offers = published(&surface(Some("newer format")), true, true);
        assert!(offers.iter().all(|offer| !offer.is_enabled()));
        assert!(matches!(
            offers[0].press(&OfferArgs::new().with(ARRANGE_X_PARAM, "1")),
            Err(OfferArgError::Unavailable { reason }) if reason.contains("newer format")
        ));
    }

    #[test]
    fn a_batch_of_set_presses_is_one_set_many_write() {
        let offers = published(&surface(None), false, false);
        let press = |offer: &UiOffer, x: &str| {
            offer
                .press(&OfferArgs::new().with(ARRANGE_X_PARAM, x))
                .expect("binds")
        };
        let one = press(&offers[0], "1");
        assert_eq!(
            arrange_batch(vec![one.clone()]),
            Some(one.clone()),
            "one press is itself"
        );
        assert_eq!(arrange_batch(Vec::new()), None);
        let both = arrange_batch(vec![one, press(&offers[1], "2")]).expect("two fold");
        let op = both.op_as::<EditorMetaOp>().expect("an arrange op");
        let EditorMetaVerb::SetMany { entries } = &op.verb else {
            panic!("one SetMany: {:?}", op.verb);
        };
        assert_eq!(
            entries
                .iter()
                .map(|entry| (entry.node_key.as_str(), entry.transform.t[0]))
                .collect::<Vec<_>>(),
            [
                ("/demo.module/dome.fixture", 1.0),
                ("/demo.module/doors.fixture", 2.0)
            ]
        );
        assert_eq!(op.fixtures.len(), 2, "every footprint can refresh");
        let undo = published(&surface(None), true, false)
            .into_iter()
            .last()
            .expect("undo");
        assert_eq!(
            arrange_batch(vec![press(&offers[0], "1"), undo.action]),
            None,
            "only placements fold"
        );
    }

    fn published(surface: &UiPatchSurface, undo: bool, redo: bool) -> Vec<UiOffer> {
        let mut tree = UiOfferTree::new();
        publish_arrange_offers(&mut tree, surface, undo, redo);
        tree.iter().cloned().collect()
    }

    fn set_transform(action: &UiAction) -> UiArrangeTransform {
        match &action.op_as::<EditorMetaOp>().expect("an arrange op").verb {
            EditorMetaVerb::Set { transform, .. } => *transform,
            other => panic!("a Set: {other:?}"),
        }
    }

    fn surface(error: Option<&str>) -> UiPatchSurface {
        let fixture = |node: u32, name: &str, address: bool| UiPatchSurfaceFixture {
            node: NodeId::new(node),
            label: name.to_string(),
            address: address.then(|| format!("/demo.module/{name}.fixture")),
            mapping_artifact: Some(ArtifactLocation::file(format!("/{name}.map2d.json"))),
            ..Default::default()
        };
        let mut dome = fixture(2, "dome", true);
        dome.arrange = Some(UiArrangeMeta {
            arranged: true,
            transform: UiArrangeTransform {
                t: [10.0, 5.0],
                r: 45.0,
                s: 2.0,
            },
            footprint: None,
        });
        UiPatchSurface {
            fixtures: vec![dome, fixture(3, "doors", true), fixture(4, "loose", false)],
            editor_meta_loaded: true,
            editor_meta_error: error.map(str::to_string),
            editor_meta_artifact: Some(ArtifactLocation::file("/editor.json")),
            ..Default::default()
        }
    }
}
