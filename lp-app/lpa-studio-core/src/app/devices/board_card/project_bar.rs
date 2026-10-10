//! The project bar: what the board plays, and how many boards share it.
//!
//! First match wins:
//!
//! | Board | Summary | Aside | Tone | Action |
//! |---|---|---|---|---|
//! | A push or a removal running | (the work shows) | — | Neutral | the work's Cancel |
//! | Locked (its link unlocked nothing) | "Not known yet" | — | Neutral | — |
//! | Degraded | the fault, without its "Degraded: " lead | — | Attention | — |
//! | Given an older version than the project's newest | "Out of date" | the project | Attention | "Send latest" (`push`, the project preset) |
//! | Says it runs nothing | "Nothing on it yet", or what starts at its next power-up | — | Neutral | "Add a project" (`push`, the picker) |
//! | Open in the editor, or given the newest | the project | "N boards" when shared | Neutral | Edit |
//! | Runs something this library cannot name | its label | — | Neutral | Edit |
//! | Has not said | "Not known yet" | — | Neutral | — |
//!
//! The order is the order of what a person needs to know first: what is
//! happening to it now, then what is wrong with it, then what is stale, then
//! what it is. `push` is offered only when the board can take a project and
//! has said what it runs; without it there is no action, and the words
//! stay.
//!
//! **Edit** (D32) takes the slot whenever no project notice does: Edit
//! (`edit`, pencil) on a board you can edit, Edit with a lock (`unlock`, the
//! password sheet) on a play-only one ([`edit_action`]). When "Send latest"
//! or "Add a project" holds the slot, Edit is in the project's details. It
//! is where `edit` is offered, so never on the editor's docked card (the
//! editor already shows the board). While the board is connected the bars
//! give way to its panel, and the same action rides the panel's All
//! controls row ([`super::UiBoardPanel::edit`]): one function builds both,
//! so they cannot disagree.

use lpa_devices::view::LoadedProject;

use super::bar_work::bar_work;
use super::board_card_input::BoardCardInput;
use super::detail_sections::{danger, facts, notice, verbs, without_empty};
use super::ui_bar_work::BarWorkState;
use super::ui_card_action::{UiActionDraw, UiCardAction};
use super::ui_stack_bar::{BarLayer, UiBarDetails, UiStackBar};
use crate::app::devices::age_words::age_words;
use crate::app::devices::board_plays::BoardPlays;
use crate::app::devices::device_push_offer::PUSH_SOURCE_PARAM;
use crate::{OfferArgs, RichLine, UiStatusKind};

/// The words a degraded board's fault leads with, which the bar drops.
const DEGRADED_LEAD: &str = "Degraded: ";
/// The sentence an out-of-date board's notice opens with.
const NEWER_ELSEWHERE: &str =
    "Studio knows what this browser sent; another browser may have sent a newer one.";

/// The project bar.
pub(crate) fn project_bar(input: &BoardCardInput<'_>) -> UiStackBar {
    let view = input.view;
    let slug = || {
        input
            .project
            .map(|project| project.slug.clone())
            .or_else(|| input.plays.project_uid().map(str::to_string))
            .unwrap_or_default()
    };
    let note = input
        .layout
        .and_then(|layout| layout.project_note.as_ref())
        .filter(|_| input.idle() && view.loaded_project == LoadedProject::Empty);
    let push = input.offer("push");
    // A link nothing has unlocked: the board answers only its hello and
    // the unlock, so its "nothing on it" is a refused read, not its word.
    let locked = input
        .access
        .is_some_and(|access| access.unlock == Some(crate::app::access::UiUnlockOffer::Locked));
    let (summary, aside, tone, action) = if locked {
        (
            "Not known yet".to_string(),
            None,
            UiStatusKind::Neutral,
            None,
        )
    } else if let Some(fault) = &view.degraded {
        (
            fault
                .strip_prefix(DEGRADED_LEAD)
                .unwrap_or(fault)
                .to_string(),
            None,
            UiStatusKind::Attention,
            None,
        )
    } else {
        match input.plays {
            BoardPlays::Given {
                at_head: false,
                project_uid,
            } => (
                "Out of date".to_string(),
                Some(slug()),
                UiStatusKind::Attention,
                push.map(|push| {
                    UiCardAction::press(push, "Send latest")
                        .with_icon("upload")
                        .with_args(
                            OfferArgs::new()
                                .with(PUSH_SOURCE_PARAM, format!("library:{project_uid}")),
                        )
                }),
            ),
            BoardPlays::Nothing => (
                note.map_or_else(|| "Nothing on it yet".to_string(), |note| note.line.clone()),
                None,
                UiStatusKind::Neutral,
                push.map(|push| {
                    UiCardAction::press(push, "Add a project")
                        .with_icon("add")
                        .drawn(UiActionDraw::ProjectPick {
                            board_id: view.board_id.clone(),
                        })
                }),
            ),
            BoardPlays::Open { .. } | BoardPlays::Given { at_head: true, .. } => (
                slug(),
                (input.sharing >= 2).then(|| format!("{} boards", input.sharing)),
                UiStatusKind::Neutral,
                None,
            ),
            BoardPlays::Running { label } => (label.clone(), None, UiStatusKind::Neutral, None),
            BoardPlays::Unknown => (
                "Not known yet".to_string(),
                None,
                UiStatusKind::Neutral,
                None,
            ),
        }
    };
    let work = bar_work(input, BarLayer::Project);
    let running = work
        .as_ref()
        .is_some_and(|work| work.state == BarWorkState::Running);
    // A notice's action keeps the slot; Edit takes it otherwise, and goes
    // to the details when a notice holds it.
    let edit = edit_action(input);
    let edit_in_details = action.is_some();
    let action = action.or_else(|| edit.clone());
    UiStackBar {
        layer: BarLayer::Project,
        icon: "project".to_string(),
        details: details(
            input,
            &summary,
            tone,
            note.map(|note| note.detail.as_str()),
            edit.filter(|_| edit_in_details),
        ),
        summary,
        aside,
        aside_icon: None,
        // Work in progress reads neutral; its words are the bar's.
        tone: if running { UiStatusKind::Neutral } else { tone },
        action: if running { None } else { action },
        work,
    }
}

/// Edit, as the project bar and the connected panel's All controls row
/// draw it (D32, CD15): `edit` with the pencil on a board you can edit, or
/// Edit with a lock — the `unlock` sheet — on a board whose link holds play
/// only (an editor there could not edit). `None` where `edit` is not
/// offered: a board running nothing, one not idle, and the board the editor
/// already shows.
pub(crate) fn edit_action(input: &BoardCardInput<'_>) -> Option<UiCardAction> {
    let edit = input.offer("edit")?;
    if input.play_only()
        && let Some(unlock) = input.offer("unlock")
    {
        return Some(
            UiCardAction::press(unlock, "Edit")
                .with_icon("lock")
                .drawn(UiActionDraw::Sheet),
        );
    }
    Some(UiCardAction::press(edit, "Edit").with_icon("edit"))
}

/// A new board's project bar: it has not said what it runs.
pub(crate) fn pending_project_bar() -> UiStackBar {
    let mut known = facts("Project", Vec::new());
    known.sentence = Some("It says what it runs once it identifies.".to_string());
    UiStackBar {
        layer: BarLayer::Project,
        icon: "project".to_string(),
        summary: "Not known yet".to_string(),
        aside: None,
        aside_icon: None,
        tone: UiStatusKind::Neutral,
        action: None,
        work: None,
        details: UiBarDetails {
            sections: vec![known],
            panels: Vec::new(),
            raised: false,
        },
    }
}

/// The project bar's details: the notice, what it plays, its verbs (Edit
/// among them when a notice holds the bar's slot), and Remove project
/// apart.
fn details(
    input: &BoardCardInput<'_>,
    summary: &str,
    tone: UiStatusKind,
    note: Option<&str>,
    edit: Option<UiCardAction>,
) -> UiBarDetails {
    let view = input.view;
    let mut sections = Vec::new();
    if tone != UiStatusKind::Neutral {
        let sentence = match &view.degraded {
            Some(fault) => fault.clone(),
            None => format!(
                "This board has an older save of {}. {NEWER_ELSEWHERE}",
                input
                    .project
                    .map(|project| project.slug.as_str())
                    .unwrap_or("this project")
            ),
        };
        sections.push(notice("Project", tone, sentence, None));
    }
    let mut lines = Vec::new();
    if input.plays != &BoardPlays::Unknown || view.loaded_project != LoadedProject::Unknown {
        let playing = match input.plays {
            BoardPlays::Nothing => "nothing".to_string(),
            BoardPlays::Running { label } => label.clone(),
            _ => input
                .project
                .map(|project| project.slug.clone())
                .unwrap_or_else(|| summary.to_string()),
        };
        lines.push(RichLine::new("Playing", playing));
    }
    if let Some(saved) = input.project.and_then(|project| project.last_saved_at) {
        lines.push(RichLine::new("Saved", age_words(input.now - saved)));
    }
    if input.plays.project_uid().is_some() {
        lines.push(RichLine::new(
            "In sync with",
            match input.shared_with.is_empty() {
                true => "only this board".to_string(),
                false => input.shared_with.join(" · "),
            },
        ));
    }
    let mut project = facts("Project", lines);
    project.sentence = note.map(str::to_string);
    sections.push(project);

    let running_something = matches!(view.loaded_project, LoadedProject::Running { .. });
    let mut actions: Vec<UiCardAction> = edit.into_iter().collect();
    if let Some(push) = input
        .offer("push")
        .filter(|push| running_something && input.idle() && !push.params().is_empty())
    {
        actions.push(
            UiCardAction::press(push, "Put another project on it")
                .with_icon("upload")
                .drawn(UiActionDraw::ProjectPick {
                    board_id: view.board_id.clone(),
                }),
        );
    }
    if let Some(clear) = input.offer("clear-faults") {
        actions.push(UiCardAction::own_words(clear).with_icon(clear.icon.clone()));
    }
    sections.push(verbs(actions));
    sections.push(danger(
        input
            .offer("remove-project")
            .map(|remove| UiCardAction::own_words(remove).with_icon(remove.icon.clone()))
            .into_iter()
            .collect(),
    ));
    UiBarDetails {
        sections: without_empty(sections),
        panels: Vec::new(),
        raised: false,
    }
}

#[cfg(test)]
mod tests {
    use lpa_devices::ActivityKind;
    use lpa_devices::device::DeviceStatus;

    use super::super::card_fixtures::{CardFixture, activity, library_project};
    use super::*;

    #[test]
    fn a_running_push_is_the_bars_work_with_its_cancel() {
        let mut fixture = CardFixture::ready().with_activity(activity(
            ActivityKind::Push,
            "Sending the project",
            Some(40),
        ));
        let bar = project_bar(&fixture.input());
        let work = bar.work.expect("the push");
        assert_eq!(work.words, "Sending the project · 40%");
        assert!(
            work.cancel
                .is_some_and(|cancel| cancel.to_string().ends_with("/cancel"))
        );
        assert_eq!(bar.tone, UiStatusKind::Neutral);
        assert_eq!(bar.action, None);
    }

    /// Ported: the fault wears the tone a degraded board's status reads in —
    /// Attention, never the error voice (the board is still running).
    #[test]
    fn the_fault_line_wears_the_same_tone_as_the_degraded_chip() {
        assert_eq!(
            crate::device_status_kind(DeviceStatus::Degraded),
            UiStatusKind::Attention
        );
        let mut fixture = CardFixture::ready();
        fixture.view.status = DeviceStatus::Degraded;
        fixture.view.degraded = Some("Degraded: node /studio.show/s faulted".to_string());
        let bar = project_bar(&fixture.input());
        assert_eq!(bar.summary, "node /studio.show/s faulted");
        assert_eq!(bar.tone, UiStatusKind::Attention);
        let notice = bar.details.notice().expect("a notice");
        assert_eq!(
            notice.sentence.as_deref(),
            Some("Degraded: node /studio.show/s faulted"),
            "the details keep the whole fault"
        );
        assert_eq!(notice.tone, UiStatusKind::Attention);
        assert!(
            verb_words(&bar).contains(&"Clear faults".to_string()),
            "{:?}",
            verb_words(&bar)
        );
    }

    #[test]
    fn an_out_of_date_board_offers_send_latest_with_the_project_preset() {
        let mut fixture = CardFixture::ready();
        let project = library_project("prjaaaa", "holiday-eaves");
        fixture.plays = BoardPlays::Given {
            project_uid: project.uid.clone(),
            at_head: false,
        };
        fixture.project = Some(project);
        let bar = project_bar(&fixture.input());
        assert_eq!(bar.summary, "Out of date");
        assert_eq!(bar.aside.as_deref(), Some("holiday-eaves"));
        assert_eq!(bar.tone, UiStatusKind::Attention);
        let action = bar.action.expect("Send latest");
        assert_eq!(action.word, "Send latest");
        assert!(action.offer.to_string().ends_with("/push"));
        assert_eq!(action.draw, UiActionDraw::Press);
        assert_eq!(action.args.get(PUSH_SOURCE_PARAM), Some("library:prjaaaa"));
        let notice = bar.details.notice().expect("a notice");
        assert_eq!(
            notice.sentence.as_deref(),
            Some(
                "This board has an older save of holiday-eaves. Studio knows what this browser \
                 sent; another browser may have sent a newer one."
            )
        );
    }

    #[test]
    fn an_empty_board_offers_add_a_project_with_the_picker() {
        let mut fixture = CardFixture::ready();
        fixture.view.loaded_project = LoadedProject::Empty;
        fixture.view.can_remove_project = false;
        fixture.plays = BoardPlays::Nothing;
        let bar = project_bar(&fixture.input());
        assert_eq!(bar.summary, "Nothing on it yet");
        assert_eq!(bar.tone, UiStatusKind::Neutral);
        let action = bar.action.expect("Add a project");
        assert_eq!(action.word, "Add a project");
        assert!(action.offer.to_string().ends_with("/push"));
        assert!(matches!(
            action.draw,
            UiActionDraw::ProjectPick { board_id: Some(_) }
        ));
    }

    /// #1045: after a Remove left a folder, an empty board says what
    /// starts at its next power-up, and the whole sentence is in details.
    #[test]
    fn an_empty_board_says_what_starts_at_its_next_power_up() {
        let mut fixture = CardFixture::ready();
        fixture.view.loaded_project = LoadedProject::Empty;
        fixture.view.can_remove_project = false;
        fixture.plays = BoardPlays::Nothing;
        fixture.layout = Some(crate::UiDeviceLayout::note_only(
            fixture.board.clone(),
            "studio-b",
        ));
        let bar = project_bar(&fixture.input());
        assert_eq!(bar.summary, "studio-b starts at next power-up");
        assert!(bar.details.sections.iter().any(|section| {
            section.sentence.as_deref()
                == Some("studio-b is still on the board and will start when it's next powered on.")
        }));
    }

    #[test]
    fn a_shared_project_names_itself_and_how_many_boards_play_it() {
        let mut fixture = CardFixture::ready();
        let project = library_project("prjbbbb", "porch-glow");
        fixture.plays = BoardPlays::Given {
            project_uid: project.uid.clone(),
            at_head: true,
        };
        fixture.project = Some(project);
        fixture.sharing = 3;
        fixture.shared_with = vec!["Garage".to_string(), "Back fence".to_string()];
        let bar = project_bar(&fixture.input());
        assert_eq!(bar.summary, "porch-glow");
        assert_eq!(bar.aside.as_deref(), Some("3 boards"));
        assert_eq!(
            bar.action.as_ref().map(|action| action.word.as_str()),
            Some("Edit"),
            "no notice holds the slot"
        );
        assert_eq!(
            line(&bar, "In sync with").as_deref(),
            Some("Garage · Back fence")
        );
        assert_eq!(line(&bar, "Saved").as_deref(), Some("2 h ago"));

        fixture.sharing = 1;
        fixture.shared_with = Vec::new();
        let bar = project_bar(&fixture.input());
        assert_eq!(bar.aside, None);
        assert_eq!(
            line(&bar, "In sync with").as_deref(),
            Some("only this board")
        );
    }

    #[test]
    fn a_board_running_something_unnamed_says_its_label() {
        let bar = project_bar(&CardFixture::ready().input());
        assert_eq!(bar.summary, "porch");
        assert_eq!(bar.tone, UiStatusKind::Neutral);
    }

    /// A Locked board answers only its hello and the unlock: what it runs
    /// is not known, whatever its refused read said (Q38).
    #[test]
    fn a_locked_board_is_not_known_yet_even_when_its_read_said_nothing() {
        let mut fixture = CardFixture::ready().locked();
        fixture.view.loaded_project = LoadedProject::Empty;
        fixture.plays = BoardPlays::Nothing;
        let bar = project_bar(&fixture.input());
        assert_eq!(bar.summary, "Not known yet");
        assert_eq!(bar.action, None, "no Add a project on a locked board");
    }

    #[test]
    fn a_board_that_has_not_said_is_not_known_yet() {
        let mut fixture = CardFixture::ready();
        fixture.view.loaded_project = LoadedProject::Unknown;
        fixture.plays = BoardPlays::Unknown;
        assert_eq!(project_bar(&fixture.input()).summary, "Not known yet");
    }

    /// Ported from the old card: the work's narration, then the fault, then
    /// what the board plays — and "Not known yet" when it has not said.
    #[test]
    fn the_project_line_follows_the_ruled_priority() {
        let mut fixture = CardFixture::ready();
        fixture.view.loaded_project = LoadedProject::Unknown;
        fixture.plays = BoardPlays::Unknown;
        assert_eq!(project_bar(&fixture.input()).summary, "Not known yet");
        fixture.view.loaded_project = LoadedProject::Empty;
        fixture.plays = BoardPlays::Nothing;
        assert_eq!(project_bar(&fixture.input()).summary, "Nothing on it yet");
        fixture.view.loaded_project = LoadedProject::Running {
            label: "porch-sign".to_string(),
        };
        fixture.plays = BoardPlays::Running {
            label: "porch-sign".to_string(),
        };
        assert_eq!(project_bar(&fixture.input()).summary, "porch-sign");
        fixture.view.degraded = Some("node /studio.show/s faulted".to_string());
        assert_eq!(
            project_bar(&fixture.input()).summary,
            "node /studio.show/s faulted",
            "a fault outranks the project name"
        );
        let mut busy = fixture.clone().with_activity(activity(
            ActivityKind::Push,
            "Sending the project",
            Some(40),
        ));
        let bar = project_bar(&busy.input());
        assert_eq!(
            bar.work.map(|work| work.words).as_deref(),
            Some("Sending the project · 40%")
        );
        // A flash is firmware work: the project bar goes on saying the fault.
        let mut flashing =
            fixture.with_activity(activity(ActivityKind::Flash, "Flashing firmware", Some(62)));
        let bar = project_bar(&flashing.input());
        assert_eq!(bar.work, None);
        assert_eq!(bar.summary, "node /studio.show/s faulted");
    }

    /// D32: Edit is the bar's action on a board you can edit — the `edit`
    /// offer, with the pencil.
    #[test]
    fn edit_is_the_bars_action_at_the_edit_tier() {
        let mut fixture = CardFixture::ready();
        let edit = project_bar(&fixture.input()).action.expect("Edit");
        assert_eq!(edit.word, "Edit");
        assert_eq!(edit.icon.as_deref(), Some("edit"));
        assert!(edit.offer.to_string().ends_with("/edit"));
        assert_eq!(edit.draw, UiActionDraw::Press);
        // Unlocked at edit over Bluetooth: the same.
        let mut granted = CardFixture::ready().over(crate::UiLinkKind::Bluetooth);
        granted.access = Some(crate::UiDeviceAccess {
            grant: Some(crate::UiAccessGrant {
                tier: lpc_access::Tier::Edit,
                key: Some("Yona's MacBook".to_string()),
            }),
            ..crate::UiDeviceAccess::default()
        });
        let edit = project_bar(&granted.input()).action.expect("Edit");
        assert!(edit.offer.to_string().ends_with("/edit"));
        assert_eq!(edit.icon.as_deref(), Some("edit"));
    }

    /// D32: on a board whose link holds play only, Edit wears a lock and
    /// presses `unlock` (the password sheet): an editor there could not
    /// edit.
    #[test]
    fn edit_wears_a_lock_at_the_play_tier() {
        let mut fixture = CardFixture::ready().over(crate::UiLinkKind::Bluetooth);
        fixture.access = Some(crate::UiDeviceAccess {
            unlock: Some(crate::UiUnlockOffer::PlayOnly),
            ..crate::UiDeviceAccess::default()
        });
        let edit = project_bar(&fixture.input()).action.expect("Edit, locked");
        assert_eq!(edit.word, "Edit");
        assert_eq!(edit.icon.as_deref(), Some("lock"));
        assert!(edit.offer.to_string().ends_with("/unlock"));
        assert_eq!(edit.draw, UiActionDraw::Sheet);
    }

    /// When "Send latest" holds the slot, Edit is in the project's details.
    #[test]
    fn edit_is_in_the_details_when_send_latest_takes_the_slot() {
        let mut fixture = CardFixture::ready();
        let project = library_project("prjaaaa", "holiday-eaves");
        fixture.plays = BoardPlays::Given {
            project_uid: project.uid.clone(),
            at_head: false,
        };
        fixture.project = Some(project);
        let bar = project_bar(&fixture.input());
        assert_eq!(
            bar.action.as_ref().map(|action| action.word.as_str()),
            Some("Send latest")
        );
        assert_eq!(verb_words(&bar), ["Edit", "Put another project on it"]);
    }

    /// The editor's docked card: the editor already shows the board, so
    /// `edit` is not offered there and no Edit is drawn, locked or not.
    #[test]
    fn no_edit_on_the_editors_docked_card() {
        let mut fixture = CardFixture::ready();
        fixture.editor_holds_it = true;
        fixture.docked = true;
        let bar = project_bar(&fixture.input());
        assert_eq!(bar.action, None);
        assert!(!verb_words(&bar).contains(&"Edit".to_string()));

        let mut play_only = CardFixture::ready().over(crate::UiLinkKind::Bluetooth);
        play_only.access = Some(crate::UiDeviceAccess {
            unlock: Some(crate::UiUnlockOffer::PlayOnly),
            ..crate::UiDeviceAccess::default()
        });
        play_only.editor_holds_it = true;
        play_only.docked = true;
        assert_eq!(project_bar(&play_only.input()).action, None);
    }

    #[test]
    fn its_details_hold_replace_and_remove_apart() {
        let bar = project_bar(&CardFixture::ready().input());
        assert_eq!(
            verb_words(&bar),
            ["Put another project on it"],
            "Edit is the bar's own action"
        );
        let danger = bar.details.sections.last().expect("the danger section");
        assert_eq!(danger.weight, crate::RichWeight::Danger);
        assert_eq!(danger.affordances[0].word, "Remove project");
        assert!(
            danger.affordances[0]
                .offer
                .to_string()
                .ends_with("/remove-project")
        );
    }

    fn verb_words(bar: &UiStackBar) -> Vec<String> {
        bar.details
            .sections
            .iter()
            .filter(|section| section.title == "Actions")
            .flat_map(|section| section.affordances.iter().map(|action| action.word.clone()))
            .collect()
    }

    fn line(bar: &UiStackBar, label: &str) -> Option<String> {
        bar.details
            .sections
            .iter()
            .flat_map(|section| section.lines.iter())
            .find(|line| line.label == label)
            .map(|line| line.value.clone())
    }
}
