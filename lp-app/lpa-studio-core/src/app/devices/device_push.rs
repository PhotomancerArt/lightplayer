//! The empty face's decisions: which projects a board can be given, and what
//! picking one means.
//!
//! In core rather than in the renderer for the usual reason — these are
//! decisions, and decisions get tests. The renderer lays out [`PushOffer`]'s
//! cards and dispatches the op it hands back.
//!
//! The rules (plan.md's card ruling, M3's §2):
//!
//! - **ONE inline picker, three sources, no dialog flow.** An example (the
//!   gallery's, installed as a library project), a project the library
//!   already has, or a new project generated for this board.
//! - **No naming, anywhere.** Every created or installed project takes the
//!   library's dated-slug name; renaming is a later, separate gesture.
//! - **The new-project entry is disabled honestly when the board has not
//!   said which board it is.** The starter generator refuses to guess a pin
//!   map (`UnknownBoard`/`NoDefaultWire`), and offering a verb that would be
//!   refused is worse than saying why it is not offered.

use lpa_devices::identity::DeviceId;
use lpa_devices::view::DeviceView;

use crate::app::home::embedded_example::embedded_examples;
use crate::app::home::{UiExampleCard, UiPackageCard};

/// Where a pushed project comes from.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PushSource {
    /// A gallery example. Installed into the library first, with a fresh uid
    /// minted at install and the incoming manifest left alone (the examples
    /// vision's fork shape — never patch a manifest at install), then
    /// pushed.
    Example { example_id: String },
    /// A project the library already holds.
    Library { project_uid: String },
    /// A starter generated for this board: a complete package with the
    /// board's own default LED wire.
    NewForBoard { board_id: String },
}

/// Which part of the picker an entry belongs to.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum PushSourceGroup {
    /// "A new project" — the board starter.
    New,
    /// "An example" — the gallery.
    Example,
    /// "A project I have" — the library.
    Library,
}

impl PushSourceGroup {
    pub fn label(self) -> &'static str {
        match self {
            Self::New => "A new project",
            Self::Example => "An example",
            Self::Library => "A project I have",
        }
    }
}

/// One thing the user can put on the board.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PushSourceChoice {
    /// Stable key for the picker's selection state.
    pub key: String,
    pub title: String,
    /// One terse consequence line for the option card.
    pub blurb: String,
    pub group: PushSourceGroup,
    pub source: PushSource,
}

/// Everything the empty face needs to offer its one primary verb.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PushOffer {
    /// Every source, new-project first, then examples, then the library —
    /// the order a first-plug walk reads them in.
    pub choices: Vec<PushSourceChoice>,
    /// Picked for the user when there is exactly one thing to pick.
    pub preselect: Option<String>,
    /// Why "a new project" is not among the choices, when it is not. Honest
    /// copy, not a hidden entry.
    pub new_project_unavailable: Option<String>,
    /// Why there is nothing at all to offer, when there is nothing.
    pub unavailable: Option<String>,
}

/// The picker for one board.
pub fn push_offer(
    card: &DeviceView,
    projects: &[UiPackageCard],
    examples: &[UiExampleCard],
) -> PushOffer {
    let mut choices = Vec::new();
    let mut new_project_unavailable = None;
    match starter_board(card.board_id.as_deref()) {
        Ok(board_id) => choices.push(PushSourceChoice {
            key: format!("new:{board_id}"),
            title: "Start something new".to_string(),
            blurb: "A starter project wired for this board.".to_string(),
            group: PushSourceGroup::New,
            source: PushSource::NewForBoard { board_id },
        }),
        Err(reason) => new_project_unavailable = Some(reason),
    }
    for example in examples {
        choices.push(PushSourceChoice {
            key: format!("example:{}", example.id),
            title: example.name.clone(),
            // Examples lost their blurbs in the card-overlay slim (#470);
            // the kind chip label is what identifies one now.
            blurb: example.kind.clone(),
            group: PushSourceGroup::Example,
            source: PushSource::Example {
                example_id: example.id.clone(),
            },
        });
    }
    for project in projects {
        choices.push(PushSourceChoice {
            key: format!("library:{}", project.uid),
            title: project.slug.clone(),
            blurb: project.project_kind.clone(),
            group: PushSourceGroup::Library,
            source: PushSource::Library {
                project_uid: project.uid.clone(),
            },
        });
    }
    let preselect = match choices.as_slice() {
        [only] => Some(only.key.clone()),
        _ => None,
    };
    let unavailable = choices.is_empty().then(|| {
        "Studio has nothing to put on this board yet — no examples are bundled in \
         this build, the library is empty, and the board has not said which board \
         it is."
            .to_string()
    });
    PushOffer {
        choices,
        preselect,
        new_project_unavailable,
        unavailable,
    }
}

/// The board a starter can be generated for, or why it cannot be.
///
/// The generator needs a board with a checked-in default LED wire; a board id
/// the catalog does not carry, or one with no wire, would make it refuse
/// mid-flow. Both are answered here, before a verb is drawn.
fn starter_board(board_id: Option<&str>) -> Result<String, String> {
    let Some(board_id) = board_id else {
        return Err(
            "Studio can't tell which board this is yet, so it can't wire a new \
                    project's LED output. A board that was just flashed says so after \
                    its next restart; until then, pick an example or a project you have."
                .to_string(),
        );
    };
    let Some(board) = lpa_boards::board_by_id(board_id) else {
        return Err(format!(
            "This board reports itself as {board_id}, which this build of Studio has \
             no catalog entry for — so it cannot guess the pin map for a new project."
        ));
    };
    if board.default_led_wire().is_none() {
        return Err(format!(
            "{} has no default LED wiring in the catalog, so a new project would have \
             nowhere to send light.",
            board.display_name
        ));
    }
    Ok(board.board_id.clone())
}

/// The example the walk reaches for when nothing else is picked: the first
/// bundled one. Exists so the fake-device bench and the page agree on what
/// "the starter example" means.
pub fn first_bundled_example_id() -> Option<&'static str> {
    embedded_examples().first().map(|example| example.id)
}

/// The app-level "prepare a project and put it on this board" gesture.
///
/// It is NOT an `lpa-devices` action: resolving a source means installing an
/// example, generating a starter, or reading a library package — library work
/// the model must never learn about. The controller does that work, stages
/// the result with the effects layer, and only then folds the model's own
/// `Action::Push`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DevicePushOp {
    pub device: DeviceId,
    pub source: PushSource,
}

impl DevicePushOp {
    /// The node id push gestures target.
    pub const NODE_ID: &'static str = "studio|device-push";

    /// This op as a dispatchable [`UiAction`](crate::UiAction).
    pub fn action_for(device: DeviceId, source: PushSource) -> crate::UiAction {
        crate::UiAction::from_op(
            crate::ControllerId::new(Self::NODE_ID),
            Self { device, source },
        )
    }
}

impl crate::ControllerOp for DevicePushOp {
    fn default_action_meta(&self) -> crate::ActionMeta {
        crate::ActionMeta::new(
            "Put it on the board",
            "Send the picked project to this board and start it running.",
            crate::ActionPriority::Primary,
        )
    }

    /// Recovery, like every other device gesture: it owns a port for its
    /// duration, and its whole point is to be reachable while the card is
    /// stuck on an empty or failed face.
    fn action_class(&self) -> crate::ActionClass {
        crate::ActionClass::Recovery
    }

    fn clone_box(&self) -> Box<dyn crate::ControllerOp> {
        Box::new(self.clone())
    }

    fn eq_op(&self, other: &dyn crate::ControllerOp) -> bool {
        other.as_any().downcast_ref::<Self>() == Some(self)
    }

    fn as_any(&self) -> &dyn core::any::Any {
        self
    }

    fn into_any(self: Box<Self>) -> Box<dyn core::any::Any> {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpa_devices::view::LoadedProject;

    fn card(board_id: Option<&str>) -> DeviceView {
        DeviceView {
            id: DeviceId(1),
            title: "Bench board".to_string(),
            status: lpa_devices::device::DeviceStatus::Ready,
            state_label: "Ready".to_string(),
            detail: None,
            freshness_label: None,
            identity_label: None,
            detected_chip: None,
            board_id: board_id.map(str::to_string),
            firmware: None,
            needs_firmware: false,
            degraded: None,
            loaded_project: LoadedProject::Empty,
            can_receive_project: true,
            can_remove_project: false,
            activity: None,
            last_outcome: None,
            terminal: Vec::new(),
            terminal_dropped: 0,
            escapes: vec![lpa_devices::view::Escape::Forget],
        }
    }

    fn example(id: &str, name: &str) -> UiExampleCard {
        UiExampleCard {
            id: id.to_string(),
            name: name.to_string(),
            kind: "Module".to_string(),
        }
    }

    fn project(uid: &str, slug: &str) -> UiPackageCard {
        UiPackageCard {
            uid: uid.to_string(),
            kind: "Module".to_string(),
            project_kind: "General".to_string(),
            exports: Vec::new(),
            slug: slug.to_string(),
            last_saved_at: None,
            provenance: None,
            on_device: None,
            open_elsewhere: false,
            running_in_sim: false,
            target: None,
            health: crate::app::library::PackageHealth::Ready,
        }
    }

    /// A board that has not said what it is still gets the two sources that
    /// need no pin map — and is told why the third is missing.
    #[test]
    fn an_unknown_board_offers_examples_and_the_library_but_says_why_not_new() {
        let offer = push_offer(
            &card(None),
            &[project("prj_1", "2026-08-30-porch")],
            &[example("examples/plasma", "Plasma")],
        );

        assert_eq!(offer.choices.len(), 2, "{offer:?}");
        assert!(
            offer
                .choices
                .iter()
                .all(|choice| choice.group != PushSourceGroup::New)
        );
        assert!(
            offer
                .new_project_unavailable
                .as_deref()
                .is_some_and(|copy| copy.contains("can't tell which board")),
            "{offer:?}"
        );
        assert!(offer.unavailable.is_none());
    }

    /// A known board leads with the starter, because that is the shortest
    /// road from "empty board" to "lit board".
    #[test]
    fn a_known_board_leads_with_the_starter() {
        let board = lpa_boards::all_boards()
            .iter()
            .find(|board| board.default_led_wire().is_some())
            .expect("the catalog ships a board with a default wire");

        let offer = push_offer(&card(Some(&board.board_id)), &[], &[]);

        assert_eq!(offer.choices.len(), 1, "{offer:?}");
        assert_eq!(offer.choices[0].group, PushSourceGroup::New);
        assert_eq!(
            offer.choices[0].source,
            PushSource::NewForBoard {
                board_id: board.board_id.clone()
            }
        );
        assert_eq!(
            offer.preselect.as_deref(),
            Some(offer.choices[0].key.as_str()),
            "one candidate: the verb is one click"
        );
        assert!(offer.new_project_unavailable.is_none());
    }

    /// A board reporting an id this build has never heard of is said out
    /// loud, not silently dropped.
    #[test]
    fn an_uncatalogued_board_id_reads_honestly() {
        let offer = push_offer(&card(Some("some-board-from-the-future")), &[], &[]);

        assert!(offer.choices.is_empty());
        assert!(
            offer
                .new_project_unavailable
                .as_deref()
                .is_some_and(|copy| copy.contains("some-board-from-the-future")),
            "{offer:?}"
        );
        assert!(
            offer
                .unavailable
                .as_deref()
                .is_some_and(|copy| copy.contains("nothing to put on this board")),
            "{offer:?}"
        );
    }

    #[test]
    fn every_bundled_example_is_offered_and_keys_are_unique() {
        let examples: Vec<UiExampleCard> = embedded_examples()
            .iter()
            .map(|example| self::example(example.id, example.name))
            .collect();

        let offer = push_offer(&card(None), &[], &examples);

        assert_eq!(offer.choices.len(), examples.len());
        let mut keys: Vec<&str> = offer.choices.iter().map(|c| c.key.as_str()).collect();
        keys.sort_unstable();
        let before = keys.len();
        keys.dedup();
        assert_eq!(keys.len(), before, "picker keys must be unique");
    }
}
