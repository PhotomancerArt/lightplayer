//! `devices/<board>/push`: put a project on a board, as an offer that takes
//! what to put there.
//!
//! The choice list has always been core's ([`push_offer`]: a starter for
//! this board, the gallery's examples, the library's projects, with their
//! stable keys and the preselect); only the PICKED key was the web's. Here
//! it becomes the offer's `source` parameter, and the binder builds the
//! same [`DevicePushOp`] the picker's button dispatches.
//!
//! The optional `name` belongs to the starter alone: it names the new
//! project and, when it differs from the board's own name, renames the
//! board to match — unless `name_board` (the picker's "Name the board the
//! same" tick, on by default) is turned off. A press that names an example
//! or a library project is refused, because neither is renamed by a push.
//!
//! **The level depends on what the push lands on (Q4).** Onto an empty
//! board nothing is lost, and over a project the library holds the library
//! copy stands behind it: Routine. Over something the library does not
//! hold, what the board runs now is gone for good: Lasting, and the copy
//! says so ([`PushOver`]).

use std::rc::Rc;

use lpa_devices::identity::DeviceId;
use lpa_devices::view::{DeviceView, LoadedProject};

use super::device_push::{DevicePushOp, PushSource, PushSourceChoice, push_offer};
use crate::app::home::{UiExampleCard, UiPackageCard};
use crate::{
    ActionConfirmation, ActionConsequence, OfferArgError, OfferArgs, OfferBinder, OfferChoice,
    OfferParam, OfferPath, UiAction, UiOffer,
};

/// The Push offer's source parameter.
pub const PUSH_SOURCE_PARAM: &str = "source";
/// The Push offer's optional name parameter (a new project only).
pub const PUSH_NAME_PARAM: &str = "name";
/// The Push offer's toggle: whether a new project's name renames the board
/// to match (on by default; it has an effect only when the two differ).
pub const PUSH_NAME_BOARD_PARAM: &str = "name_board";

/// What an empty name field means.
const NAME_PLACEHOLDER: &str = "A new project only. Leave blank to name it after the board";

/// What a push would land on, as far as Studio can tell.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PushOver {
    /// The board reported nothing loaded.
    Empty,
    /// The board runs a project this library holds: its registry row's
    /// association names a library project.
    Banked,
    /// The board runs something the library does not hold — no
    /// association, or one naming a project this library does not have.
    /// `label` is the storage dir it runs from, the only name the wire
    /// carries.
    Unbanked { label: String },
}

impl PushOver {
    /// What a board's report and the library's answer make of it, or `None`
    /// while the board has not said what it runs: a push aimed at a guess
    /// is not offered (the card's own rule).
    pub fn for_view(view: &DeviceView, banked: bool) -> Option<Self> {
        match &view.loaded_project {
            LoadedProject::Unknown => None,
            LoadedProject::Empty => Some(Self::Empty),
            LoadedProject::Running { .. } if banked => Some(Self::Banked),
            LoadedProject::Running { label } => Some(Self::Unbanked {
                label: label.clone(),
            }),
        }
    }

    /// What pressing the push costs.
    pub fn consequence(&self) -> ActionConsequence {
        match self {
            Self::Empty | Self::Banked => ActionConsequence::Routine,
            Self::Unbanked { label } => ActionConsequence::Lasting(ActionConfirmation::new(
                "Replace what this board is running?",
                format!(
                    "This board is running \u{201c}{label}\u{201d}, and your library has no \
                     copy of it that Studio knows of. Putting a project on it means \
                     \u{201c}{label}\u{201d} will be gone."
                ),
                "replace",
            )),
        }
    }
}

/// `<prefix>/push` for a board that can take a project right now, or `None`
/// when it cannot: not a LightPlayer on an open port, busy, or not yet
/// saying what it runs. `banked` is whether the library holds what it runs
/// ([`PushOver::Banked`]).
///
/// With nothing to offer at all, the offer is published disabled with
/// core's reason and no parameters.
pub fn push_device_offer(
    view: &DeviceView,
    prefix: OfferPath,
    projects: &[UiPackageCard],
    examples: &[UiExampleCard],
    banked: bool,
) -> Option<UiOffer> {
    if !view.can_receive_project {
        return None;
    }
    let over = PushOver::for_view(view, banked)?;
    let path = prefix.child("push");
    let consequence = over.consequence();
    let device = view.id;
    let template = push_action(
        device,
        PushSource::Library {
            project_uid: String::new(),
        },
        None,
        &consequence,
    );
    let offer = push_offer(view, projects, examples);
    if let Some(reason) = offer.unavailable {
        return Some(UiOffer::new(path, "upload", template.disabled(reason)));
    }
    let options = offer.choices.iter().map(source_option).collect();
    let mut params = vec![OfferParam::choice(
        PUSH_SOURCE_PARAM,
        "project",
        options,
        offer.preselect,
    )];
    if offer.new_project_unavailable.is_none() {
        params.push(OfferParam::text(PUSH_NAME_PARAM, "name", NAME_PLACEHOLDER).optional());
        params.push(OfferParam::toggle(
            PUSH_NAME_BOARD_PARAM,
            "name the board the same",
            true,
        ));
    }
    let choices = Rc::new(offer.choices);
    let board_title = view.title.trim().to_string();
    let binder = OfferBinder::new(move |args: &OfferArgs| {
        let key = args.choice(PUSH_SOURCE_PARAM).unwrap_or_default();
        let choice = choices
            .iter()
            .find(|choice| choice.key == key)
            .ok_or_else(|| OfferArgError::NotAnOption {
                name: PUSH_SOURCE_PARAM.to_string(),
                value: key.to_string(),
                options: choices.iter().map(|choice| choice.key.clone()).collect(),
            })?;
        let typed = args.text(PUSH_NAME_PARAM);
        let (source, device_name) = match &choice.source {
            PushSource::NewForBoard { board_id, .. } => {
                let name = typed.unwrap_or(board_title.as_str()).to_string();
                let name_board = args.toggle(PUSH_NAME_BOARD_PARAM).unwrap_or(true);
                let rename = (name_board && name != board_title).then(|| name.clone());
                (
                    PushSource::NewForBoard {
                        board_id: board_id.clone(),
                        name: Some(name),
                    },
                    rename,
                )
            }
            other if typed.is_some() => {
                return Err(OfferArgError::Inapplicable {
                    name: PUSH_NAME_PARAM.to_string(),
                    reason: format!(
                        "it names a new project, and `{}` is {}",
                        choice.key,
                        match other {
                            PushSource::Example { .. } => "an example",
                            _ => "a project the library already has",
                        }
                    ),
                });
            }
            other => (other.clone(), None),
        };
        Ok(push_action(device, source, device_name, &consequence))
    });
    Some(UiOffer::with_params(
        path, "upload", params, binder, template,
    ))
}

/// One source as a choice: its key, its title, and which part of the picker
/// it sits in with its one-line blurb.
fn source_option(choice: &PushSourceChoice) -> OfferChoice {
    let detail = match choice.blurb.is_empty() {
        true => choice.group.label().to_string(),
        false => format!("{} \u{b7} {}", choice.group.label(), choice.blurb),
    };
    OfferChoice::new(&choice.key, &choice.title).with_detail(detail)
}

/// The push op for `device`, wearing `consequence`.
fn push_action(
    device: DeviceId,
    source: PushSource,
    device_name: Option<String>,
    consequence: &ActionConsequence,
) -> UiAction {
    DevicePushOp {
        device,
        source,
        device_name,
    }
    .into_action()
    .with_consequence(consequence.clone())
}

#[cfg(test)]
mod tests {
    use lpa_devices::device::DeviceStatus;
    use lpa_devices::view::{Escape, FirmwareFace};

    use super::*;

    #[test]
    fn an_empty_board_lists_its_sources_and_binds_an_example_routinely() {
        let view = board(LoadedProject::Empty, None);
        let offer = push_device_offer(&view, prefix(), &[project()], &[example()], false)
            .expect("an empty LightPlayer takes a project");

        assert_eq!(offer.path.to_string(), "devices/mac-a0f26287b48c/push");
        assert!(offer.consequence().is_routine(), "nothing on it to lose");
        let [source] = offer.params() else {
            panic!(
                "no starter for an unnamed board, so no name: {:?}",
                offer.params()
            );
        };
        let crate::OfferParamKind::Choice { options, preselect } = &source.kind else {
            panic!("source is a choice: {source:?}");
        };
        assert_eq!(
            options.iter().map(|o| o.value.as_str()).collect::<Vec<_>>(),
            ["library:prj_1", "example:catalog/plasma"],
            "the library's own projects come before the catalog"
        );
        assert_eq!(preselect, &None, "two to choose from");
        assert!(!offer.is_enabled(), "choose a project first");

        let bound = offer
            .press(&OfferArgs::new().with("source", "example:catalog/plasma"))
            .expect("an example binds");
        assert_eq!(
            bound.op_as::<DevicePushOp>(),
            Some(&DevicePushOp {
                device: DeviceId(7),
                source: PushSource::Example {
                    example_id: "catalog/plasma".to_string()
                },
                device_name: None,
            })
        );
        assert!(bound.meta().consequence.is_routine());
    }

    #[test]
    fn pushing_over_a_project_the_library_does_not_hold_is_lasting() {
        let running = LoadedProject::Running {
            label: "porch".to_string(),
        };
        let view = board(running.clone(), None);
        let offer = push_device_offer(&view, prefix(), &[project()], &[], false).unwrap();
        let copy = offer
            .consequence()
            .copy()
            .expect("an un-banked project is Lasting");
        assert_eq!(copy.title, "Replace what this board is running?");
        assert!(
            copy.message.contains("\u{201c}porch\u{201d} will be gone"),
            "{copy:?}"
        );
        let bound = offer
            .press(&OfferArgs::new().with("source", "library:prj_1"))
            .unwrap();
        assert!(bound.meta().consequence.arms(), "bound Lasting too");

        let banked =
            push_device_offer(&board(running, None), prefix(), &[project()], &[], true).unwrap();
        assert!(
            banked.consequence().is_routine(),
            "the library copy stands behind it"
        );
    }

    #[test]
    fn the_name_applies_to_a_new_project_only() {
        let starter = lpa_boards::all_boards()
            .iter()
            .find(|board| board.default_led_wire().is_some())
            .expect("the catalog ships a board with a default wire");
        let view = board(LoadedProject::Empty, Some(&starter.board_id));
        let offer = push_device_offer(&view, prefix(), &[], &[example()], false).unwrap();
        assert_eq!(
            offer
                .params()
                .iter()
                .map(|param| param.name.as_str())
                .collect::<Vec<_>>(),
            ["source", "name", "name_board"]
        );
        let new_key = format!("new:{}", starter.board_id);

        let named = offer
            .press(
                &OfferArgs::new()
                    .with("source", &new_key)
                    .with("name", "Porch"),
            )
            .unwrap();
        assert_eq!(
            named.op_as::<DevicePushOp>(),
            Some(&DevicePushOp {
                device: DeviceId(7),
                source: PushSource::NewForBoard {
                    board_id: starter.board_id.clone(),
                    name: Some("Porch".to_string()),
                },
                device_name: Some("Porch".to_string()),
            }),
            "the board is named to match, as the picker's ticked offer does"
        );
        let kept = offer
            .press(
                &OfferArgs::new()
                    .with("source", &new_key)
                    .with("name", "Porch")
                    .with("name_board", "false"),
            )
            .unwrap();
        assert_eq!(
            kept.op_as::<DevicePushOp>().unwrap().device_name,
            None,
            "the tick off: the project is named, the board keeps its name"
        );
        let unnamed = offer
            .press(&OfferArgs::new().with("source", &new_key))
            .unwrap();
        assert_eq!(
            unnamed.op_as::<DevicePushOp>().unwrap().device_name,
            None,
            "blank keeps the board's name, and the project wears it"
        );

        let refused = offer
            .press(
                &OfferArgs::new()
                    .with("source", "example:catalog/plasma")
                    .with("name", "Porch"),
            )
            .unwrap_err();
        assert!(
            matches!(&refused, OfferArgError::Inapplicable { name, .. } if name == "name"),
            "{refused:?}"
        );
    }

    #[test]
    fn a_board_that_has_not_said_what_it_runs_or_cannot_take_one_gets_no_push() {
        assert_eq!(
            push_device_offer(
                &board(LoadedProject::Unknown, None),
                prefix(),
                &[project()],
                &[],
                false
            ),
            None,
            "no push aimed at a guess"
        );
        let mut busy = board(LoadedProject::Empty, None);
        busy.can_receive_project = false;
        assert_eq!(
            push_device_offer(&busy, prefix(), &[project()], &[], false),
            None
        );
    }

    #[test]
    fn with_nothing_to_offer_the_push_is_disabled_with_cores_reason() {
        let offer = push_device_offer(
            &board(LoadedProject::Empty, None),
            prefix(),
            &[],
            &[],
            false,
        )
        .unwrap();
        assert!(offer.params().is_empty());
        assert!(matches!(
            offer.press(&OfferArgs::new()),
            Err(OfferArgError::Unavailable { reason }) if reason.contains("nothing to put on this board")
        ));
    }

    fn prefix() -> OfferPath {
        OfferPath::board(&crate::BoardRef::Mac(
            lpa_devices::BoardKey::parse("a0:f2:62:87:b4:8c").unwrap(),
        ))
    }

    fn example() -> UiExampleCard {
        UiExampleCard {
            id: "catalog/plasma".to_string(),
            name: "Plasma".to_string(),
            kind: lpc_model::ProjectKind::General,
            description: String::new(),
        }
    }

    fn project() -> UiPackageCard {
        UiPackageCard {
            uid: "prj_1".to_string(),
            kind: "Module".to_string(),
            project_kind: "General".to_string(),
            exports: Vec::new(),
            slug: "2026-08-30-porch".to_string(),
            last_saved_at: None,
            provenance: None,
            on_device: None,
            open_elsewhere: false,
            target: None,
            health: crate::app::library::PackageHealth::Ready,
        }
    }

    /// A Ready LightPlayer on an open port, idle, reporting `loaded`.
    fn board(loaded: LoadedProject, board_id: Option<&str>) -> DeviceView {
        DeviceView {
            id: DeviceId(7),
            title: "Bench board".to_string(),
            status: DeviceStatus::Ready,
            state_label: "Ready".to_string(),
            detail: None,
            freshness_label: None,
            identity_label: None,
            detected_chip: None,
            board_id: board_id.map(str::to_string),
            firmware_face: FirmwareFace::LightPlayer {
                firmware: None,
                wire: lpa_devices::WireVersion::Match,
            },
            remembered_firmware: None,
            degraded: None,
            loaded_project: loaded,
            engine_fps: None,
            link_counters: None,
            can_receive_project: true,
            can_remove_project: false,
            activity: None,
            last_outcome: None,
            terminal: Vec::new(),
            terminal_dropped: 0,
            firmware_blocked: None,
            escapes: vec![Escape::Disconnect, Escape::Forget],
        }
    }
}
