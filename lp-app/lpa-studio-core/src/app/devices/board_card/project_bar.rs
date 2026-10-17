//! The Project row: what the board plays (`docs/ux/anatomy/board-card.md`,
//! "Project ◆"). **Project · name · state · action**, the layers icon.
//!
//! First match wins:
//!
//! | Board | Name | State | Tone | Action |
//! |---|---|---|---|---|
//! | A push or a removal running | the project, else none | the work: "sending · 40%" | Neutral | the work's Cancel |
//! | Locked (its link unlocked nothing) | "hidden" | — | Neutral | — (Unlock is the primary) |
//! | Its data was lost (only on evidence it had some: never a fresh install) | "none" | "data lost" | Attention | Restore (`restore-files`), Reset (`start-fresh`) beside it (O3) |
//! | Offline | the last project known, dimmed, else "not known" | — | Neutral | Edit, when offered |
//! | A LightPlayer Studio can't read (older, or from before the hello), or a board that says nothing | "not known" | — | Neutral | — (the primary is its firmware) |
//! | No LightPlayer on it (blank, somebody else's firmware) | "none" | — | Neutral | — (Install firmware is the primary) |
//! | A shader error (degraded) | the project | "error" | Error | — (the error is in the details) |
//! | Given an older version than the project's newest | the project | "out of date" | Neutral | "Send latest" (`push`, the project preset) |
//! | Says it runs nothing | "none", or what starts at its next power-up | — | Neutral | "Add a project" (`push`, the picker), unless the primary carries it |
//! | Open in the editor, or given the newest | the project | "N boards" when shared | Neutral | Edit |
//! | Runs something this library cannot name | its label | — | Neutral | Edit |
//! | Has not said yet, a LightPlayer on a link | — (the spinner, a moment) | — | Neutral | — |
//! | Has not said, and will not (nothing reaches it, or only its core runs) | "not known" | — | Neutral | — |
//!
//! "none" only when the board said it runs nothing, or has no LightPlayer
//! to run anything: a board Studio cannot read is "not known", never a
//! false "none" (the desk C6, 2026-10-17, played `studio` while its card
//! said "Project none"). The name stays in place in every state (P3): "out
//! of date" follows it, never moves in front of it. An older copy is no
//! alarm: the row is untinted and its button says what to do (P16). A
//! shader error is the one broken state, red, and its whole fault is in
//! the details (P5) — and while the surface says it, the row goes quiet
//! ([`super::board_surface::carried_layer`]). `push` is
//! offered only when the board can take a project and has said what it
//! runs; without it there is no action, and the words stay. On a ready
//! board running no project, Add a project is the card's primary
//! ([`super::primary_action::add_a_project`]), and the row does not repeat
//! it.
//!
//! **Edit** (D32) takes the slot whenever no project state does: Edit
//! (`edit`, pencil) on a board you can edit, Edit with a lock (`unlock`, the
//! password sheet) on a play-only one ([`edit_action`]). When "Send latest"
//! or "Add a project" holds the slot, Edit is in the project's details. It
//! is where `edit` is offered, so never on the editor's docked card (the
//! editor already shows the board). While the board is connected the rows
//! give way to its panel, and the same action rides the panel's All
//! controls row ([`super::UiBoardPanel::edit`]): one function builds both,
//! so they cannot disagree.

use lpa_devices::view::{FirmwareFace, LoadedProject};

use super::bar_work::bar_work;
use super::board_card_input::BoardCardInput;
use super::detail_sections::{danger, facts, notice, verbs, without_empty};
use super::primary_action::{ADD_A_PROJECT, primary_action};
use super::ui_bar_work::{BarWorkState, UiBarWork};
use super::ui_card_action::{UiActionDraw, UiCardAction};
use super::ui_detail_panel::UiDetailPanel;
use super::ui_name_bar::UiPrimary;
use super::ui_stack_bar::{BarLayer, UiBarDetails, UiStackBar};
use crate::app::devices::board_plays::BoardPlays;
use crate::app::devices::device_push_offer::PUSH_SOURCE_PARAM;
use crate::core::data_kinds::age_words;
use crate::{OfferArgs, RichLine, UiStatusKind};

/// The name of a board that plays nothing.
pub const PROJECT_NONE: &str = "none";
/// The name on a locked board: it plays something, and the link may not
/// say what.
pub const PROJECT_HIDDEN: &str = "hidden";
/// The name when Studio cannot read what the board plays: a LightPlayer it
/// cannot speak to, or a board that has not said and will not.
pub const PROJECT_UNKNOWN: &str = "not known";
/// The state of a board whose data was lost (O3).
pub const DATA_LOST: &str = "data lost";
/// The state of a board given an older version than the project's newest.
const OUT_OF_DATE: &str = "out of date";
/// The state of a board whose project faulted (a shader error).
const PROJECT_ERROR: &str = "error";
/// The sentence an out-of-date board's details open with.
const NEWER_ELSEWHERE: &str =
    "Studio knows what this browser sent; another browser may have sent a newer one.";

/// The Project row.
pub(crate) fn project_bar(input: &BoardCardInput<'_>) -> UiStackBar {
    let view = input.view;
    let name = project_name(input);
    let note = input
        .layout
        .and_then(|layout| layout.project_note.as_ref())
        .filter(|_| input.idle() && view.loaded_project == LoadedProject::Empty);
    let push = input.offer("push");
    let lost = data_lost(input);
    let mut way_out = None;
    let mut stale = false;
    let mut waiting = false;
    let (summary, aside, tone, action) = if input.locked() {
        (
            PROJECT_HIDDEN.to_string(),
            None,
            UiStatusKind::Neutral,
            None,
        )
    } else if lost {
        // The fix, and a way out (P7): Restore puts the backup back; Reset
        // is today's Start fresh — the board waits for a new project.
        let restore = input
            .offer("restore-files")
            .filter(|_| view.firmware_blocked.is_none())
            .map(|restore| UiCardAction::press(restore, "Restore").with_icon("upload"));
        let reset = input
            .offer("start-fresh")
            .map(|fresh| UiCardAction::press(fresh, "Reset").with_icon("add"));
        let (action, beside) = match restore {
            Some(restore) => (Some(restore), reset),
            None => (reset, None),
        };
        way_out = beside;
        (
            PROJECT_NONE.to_string(),
            Some(DATA_LOST.to_string()),
            UiStatusKind::Attention,
            action,
        )
    } else if input.offline() && !input.stand_in() {
        stale = true;
        (
            name.unwrap_or_else(|| PROJECT_UNKNOWN.to_string()),
            None,
            UiStatusKind::Neutral,
            None,
        )
    } else if view.needs_firmware() {
        // No LightPlayer on it plays nothing (Install firmware is the
        // primary); a LightPlayer this Studio cannot read plays something it
        // cannot name.
        let name = match view.firmware_face {
            FirmwareFace::Blank | FirmwareFace::Foreign { .. } => PROJECT_NONE,
            _ => PROJECT_UNKNOWN,
        };
        (name.to_string(), None, UiStatusKind::Neutral, None)
    } else if view.degraded.is_some() {
        (
            name.unwrap_or_else(|| PROJECT_UNKNOWN.to_string()),
            Some(PROJECT_ERROR.to_string()),
            UiStatusKind::Error,
            None,
        )
    } else {
        match input.plays {
            BoardPlays::Given {
                at_head: false,
                project_uid,
            } => (
                name.unwrap_or_default(),
                Some(OUT_OF_DATE.to_string()),
                UiStatusKind::Neutral,
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
                note.map_or_else(|| PROJECT_NONE.to_string(), |note| note.line.clone()),
                None,
                UiStatusKind::Neutral,
                push.filter(|_| !primary_adds_a_project(input)).map(|push| {
                    UiCardAction::press(push, ADD_A_PROJECT)
                        .with_icon("add")
                        .drawn(UiActionDraw::ProjectPick {
                            board_id: view.board_id.clone(),
                        })
                }),
            ),
            BoardPlays::Open { .. } | BoardPlays::Given { at_head: true, .. } => (
                name.unwrap_or_default(),
                (input.sharing >= 2).then(|| format!("{} boards", input.sharing)),
                UiStatusKind::Neutral,
                None,
            ),
            BoardPlays::Running { label } => (label.clone(), None, UiStatusKind::Neutral, None),
            // A moment: a LightPlayer on a link has not said yet. A
            // spinner, no words. A core alone never will.
            BoardPlays::Unknown
                if input.linked()
                    && matches!(
                        view.firmware_face,
                        FirmwareFace::LightPlayer { .. } | FirmwareFace::Unknown
                    ) =>
            {
                waiting = true;
                (String::new(), None, UiStatusKind::Neutral, None)
            }
            BoardPlays::Unknown => (
                PROJECT_UNKNOWN.to_string(),
                None,
                UiStatusKind::Neutral,
                None,
            ),
        }
    };
    let work = bar_work(input, BarLayer::Project).or_else(|| {
        // The board has not said what it runs: the row waits, quietly.
        waiting.then(|| UiBarWork::running("", None))
    });
    let running = work
        .as_ref()
        .is_some_and(|work| work.state == BarWorkState::Running);
    // While its own work runs the row keeps its name — what it plays now;
    // on a board that plays nothing yet the work's words take the name's
    // place ("◌ Project  sending · 40%").
    let summary = match running && !waiting && summary == PROJECT_NONE {
        true => String::new(),
        false => summary,
    };
    // A state's action keeps the slot; Edit takes it otherwise, and goes to
    // the details when a state holds it.
    let edit = edit_action(input).filter(|_| !lost);
    let edit_in_details = action.is_some();
    let action = action.or_else(|| edit.clone());
    UiStackBar {
        aside: if running { None } else { aside },
        // Work in progress reads neutral; its words are the row's state.
        tone: if running { UiStatusKind::Neutral } else { tone },
        action: if running { None } else { action },
        way_out: if running { None } else { way_out },
        stale,
        details: details(
            input,
            tone,
            lost,
            note.map(|note| note.detail.as_str()),
            edit.filter(|_| edit_in_details),
        ),
        work,
        ..UiStackBar::new(BarLayer::Project, "project", summary)
    }
}

/// The project's name, when Studio knows it: the library project's slug,
/// else what the board says it runs.
fn project_name(input: &BoardCardInput<'_>) -> Option<String> {
    input
        .project
        .map(|project| project.slug.clone())
        .or_else(|| input.plays.project_uid().map(str::to_string))
        .or_else(|| match input.plays {
            BoardPlays::Running { label } => Some(label.clone()),
            BoardPlays::Nothing => Some(PROJECT_NONE.to_string()),
            _ => match &input.view.loaded_project {
                LoadedProject::Running { label } => Some(label.clone()),
                LoadedProject::Empty => Some(PROJECT_NONE.to_string()),
                LoadedProject::Unknown => None,
            },
        })
}

/// The board came back without the data it had: this browser kept a
/// backup (`restore-files`), or a backup file can bring it back
/// (`restore-from-file`). Only ever on evidence it had some — never a fresh
/// install (`device_layout_view`). The Project row says it (O3).
pub(crate) fn data_lost(input: &BoardCardInput<'_>) -> bool {
    input.offer("restore-files").is_some() || input.offer("restore-from-file").is_some()
}

/// Edit, as the Project row and the connected panel's All controls row
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

/// Whether the card's primary is Add a project (a ready board running no
/// project): the row then does not draw the same button again.
fn primary_adds_a_project(input: &BoardCardInput<'_>) -> bool {
    matches!(
        primary_action(input, None),
        Some(UiPrimary::Offer(action)) if action.word == ADD_A_PROJECT
    )
}

/// A new board's Project row: while it is still saying who it is, nothing
/// is known ("not known"); once it settles on needing firmware, it plays
/// nothing ("none": there is no LightPlayer on it to play anything) — or,
/// a LightPlayer this Studio cannot read, still "not known".
pub(crate) fn pending_project_bar(pending: &lpa_devices::view::PendingLinkView) -> UiStackBar {
    let settled = pending.needs_firmware()
        && matches!(
            pending.firmware_face,
            FirmwareFace::Blank | FirmwareFace::Foreign { .. }
        );
    let mut known = facts("Project", Vec::new());
    known.sentence = Some(
        match settled {
            true => "There is no LightPlayer on this board yet, so it plays nothing.",
            false => "It says what it plays once it identifies.",
        }
        .to_string(),
    );
    UiStackBar {
        details: UiBarDetails {
            sections: vec![known],
            panels: Vec::new(),
            raised: false,
        },
        ..UiStackBar::new(
            BarLayer::Project,
            "project",
            match settled {
                true => PROJECT_NONE,
                false => PROJECT_UNKNOWN,
            },
        )
    }
}

/// The Project row's details: the state's notice, what it plays, its verbs
/// (Edit among them when a state holds the row's slot), the lost data's
/// ways back, and Remove project apart.
fn details(
    input: &BoardCardInput<'_>,
    tone: UiStatusKind,
    lost: bool,
    note: Option<&str>,
    edit: Option<UiCardAction>,
) -> UiBarDetails {
    let view = input.view;
    let mut sections = Vec::new();
    let mut panels = Vec::new();
    if lost {
        let line = input
            .layout
            .and_then(|layout| layout.line.clone())
            .unwrap_or_else(|| "This board's files are gone.".to_string());
        sections.push(notice("Project", tone, line, None));
    } else if let Some(fault) = &view.degraded {
        sections.push(notice("Project", UiStatusKind::Error, fault.clone(), None));
    } else if let BoardPlays::Given { at_head: false, .. } = input.plays {
        sections.push(notice(
            "Project",
            UiStatusKind::Neutral,
            format!(
                "This board has an older save of {}. {NEWER_ELSEWHERE}",
                input
                    .project
                    .map(|project| project.slug.as_str())
                    .unwrap_or("this project")
            ),
            None,
        ));
    }
    let mut lines = Vec::new();
    if input.plays != &BoardPlays::Unknown || view.loaded_project != LoadedProject::Unknown {
        let playing = match input.plays {
            BoardPlays::Nothing => "nothing".to_string(),
            BoardPlays::Running { label } => label.clone(),
            _ => project_name(input).unwrap_or_else(|| PROJECT_UNKNOWN.to_string()),
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
    if lost {
        // The ways back the row does not draw: a backup file this browser
        // never kept, and the backup it did keep, to download.
        if let Some(restore) = input.offer("restore-from-file") {
            panels.push(UiDetailPanel::RestoreFromFile {
                offer: restore.path.clone(),
                current_base_mac: input
                    .layout
                    .and_then(|layout| layout.current_base_mac.clone()),
            });
        }
        if let Some(download) = input.offer("download-backup") {
            actions.push(UiCardAction::own_words(download).with_icon("download"));
        }
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
        panels,
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
    fn a_row_names_its_kind_with_the_layers_icon() {
        let bar = project_bar(&CardFixture::ready().input());
        assert_eq!(bar.kind, Some("Project"));
        assert_eq!(bar.icon, "project");
    }

    /// O1: a push stays in place — the name, then "sending · 40%".
    #[test]
    fn a_running_push_keeps_the_name_and_says_sending() {
        let mut fixture = CardFixture::ready().with_activity(activity(
            ActivityKind::Push,
            "Sending the project",
            Some(40),
        ));
        let bar = project_bar(&fixture.input());
        assert_eq!(bar.summary, "porch", "the name stays");
        let work = bar.work.expect("the push");
        assert_eq!(work.words, "sending · 40%");
        assert!(
            work.cancel
                .is_some_and(|cancel| cancel.to_string().ends_with("/cancel"))
        );
        assert_eq!(bar.tone, UiStatusKind::Neutral);
        assert_eq!(bar.action, None);
    }

    /// A shader error is the one broken state: red, "error", the whole
    /// fault in the details.
    #[test]
    fn a_shader_error_is_red_with_the_fault_in_the_details() {
        let mut fixture = CardFixture::ready();
        fixture.view.status = DeviceStatus::Degraded;
        fixture.view.degraded = Some("Degraded: node /studio.show/s faulted".to_string());
        let bar = project_bar(&fixture.input());
        assert_eq!(bar.summary, "porch", "the name stays");
        assert_eq!(bar.aside.as_deref(), Some("error"));
        assert_eq!(bar.tone, UiStatusKind::Error);
        let notice = bar.details.notice().expect("a notice");
        assert_eq!(
            notice.sentence.as_deref(),
            Some("Degraded: node /studio.show/s faulted"),
            "the details keep the whole fault"
        );
        assert_eq!(notice.tone, UiStatusKind::Error);
        assert!(
            verb_words(&bar).contains(&"Clear faults".to_string()),
            "{:?}",
            verb_words(&bar)
        );
    }

    /// An older copy stays in place and is no alarm: the name, "out of
    /// date", untinted, with Send latest (P3, P16).
    #[test]
    fn an_out_of_date_board_offers_send_latest_with_no_tone() {
        let mut fixture = CardFixture::ready();
        let project = library_project("prjaaaa", "holiday-eaves");
        fixture.plays = BoardPlays::Given {
            project_uid: project.uid.clone(),
            at_head: false,
        };
        fixture.project = Some(project);
        let bar = project_bar(&fixture.input());
        assert_eq!(bar.summary, "holiday-eaves");
        assert_eq!(bar.aside.as_deref(), Some("out of date"));
        assert_eq!(bar.tone, UiStatusKind::Neutral);
        let action = bar.action.expect("Send latest");
        assert_eq!(action.word, "Send latest");
        assert!(action.offer.to_string().ends_with("/push"));
        assert_eq!(action.draw, UiActionDraw::Press);
        assert_eq!(action.args.get(PUSH_SOURCE_PARAM), Some("library:prjaaaa"));
        let notice = bar.details.sections.first().expect("the sentence");
        assert_eq!(
            notice.sentence.as_deref(),
            Some(
                "This board has an older save of holiday-eaves. Studio knows what this browser \
                 sent; another browser may have sent a newer one."
            )
        );
    }

    /// A ready board running no project says "none"; Add a project is the
    /// card's primary, so the row does not repeat it.
    #[test]
    fn an_empty_board_says_none_and_leaves_add_a_project_to_the_primary() {
        let mut fixture = CardFixture::ready();
        fixture.view.loaded_project = LoadedProject::Empty;
        fixture.view.can_remove_project = false;
        fixture.plays = BoardPlays::Nothing;
        let bar = project_bar(&fixture.input());
        assert_eq!(bar.summary, "none");
        assert_eq!(bar.tone, UiStatusKind::Neutral);
        assert_eq!(bar.action, None, "the primary carries Add a project");
        let Some(UiPrimary::Offer(primary)) = primary_action(&fixture.input(), None) else {
            panic!("Add a project is the primary");
        };
        assert_eq!(primary.word, "Add a project");
        assert!(primary.offer.to_string().ends_with("/push"));
        assert!(matches!(
            primary.draw,
            UiActionDraw::ProjectPick { board_id: Some(_) }
        ));
    }

    /// Where the primary is something else (the editor's docked card shows
    /// Done), the row keeps Add a project as its action.
    #[test]
    fn the_row_keeps_add_a_project_when_the_primary_is_another_verb() {
        let mut fixture = CardFixture::ready();
        fixture.view.loaded_project = LoadedProject::Empty;
        fixture.view.can_remove_project = false;
        fixture.plays = BoardPlays::Nothing;
        fixture.editor_holds_it = true;
        fixture.docked = true;
        let action = project_bar(&fixture.input()).action.expect("Add a project");
        assert_eq!(action.word, "Add a project");
        assert!(action.offer.to_string().ends_with("/push"));
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
            "no state holds the slot"
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

    /// A Locked board answers only its hello and the unlock: its project is
    /// hidden, whatever its refused read said (Q38), and Unlock is the
    /// primary.
    #[test]
    fn a_locked_boards_project_is_hidden_even_when_its_read_said_nothing() {
        let mut fixture = CardFixture::ready().locked();
        fixture.view.loaded_project = LoadedProject::Empty;
        fixture.plays = BoardPlays::Nothing;
        let bar = project_bar(&fixture.input());
        assert_eq!(bar.summary, "hidden");
        assert_eq!(bar.action, None, "no Add a project on a locked board");
    }

    /// A board that has not said yet waits a moment: a spinner, no words.
    #[test]
    fn a_board_that_has_not_said_waits_with_a_spinner() {
        let mut fixture = CardFixture::ready();
        fixture.view.loaded_project = LoadedProject::Unknown;
        fixture.plays = BoardPlays::Unknown;
        let bar = project_bar(&fixture.input());
        assert_eq!(bar.summary, "");
        let work = bar.work.expect("the spinner");
        assert_eq!(work.state, BarWorkState::Running);
        assert_eq!(work.words, "");
        assert_eq!(work.percent, None);
    }

    /// P13: offline, the last project known stays, dimmed.
    #[test]
    fn an_offline_board_keeps_its_last_project_dimmed() {
        let mut fixture = CardFixture::offline();
        let project = library_project("prjbbbb", "studio");
        fixture.plays = BoardPlays::Given {
            project_uid: project.uid.clone(),
            at_head: true,
        };
        fixture.project = Some(project);
        let bar = project_bar(&fixture.input());
        assert_eq!(bar.summary, "studio");
        assert!(bar.stale, "drawn stale");
        assert_eq!(bar.work, None, "no spinner for a board that is away");

        let mut unknown = CardFixture::offline();
        unknown.plays = BoardPlays::Unknown;
        unknown.view.loaded_project = LoadedProject::Unknown;
        let bar = project_bar(&unknown.input());
        assert_eq!(bar.summary, "not known");
        assert!(bar.stale);
    }

    /// A new board says "not known" while it identifies, and "none" once it
    /// settles on having no LightPlayer.
    #[test]
    fn a_new_board_is_not_known_then_none() {
        let mut pending = super::super::primary_action::tests::pending_view();
        assert_eq!(pending_project_bar(&pending).summary, "not known");
        pending.firmware_face = FirmwareFace::Blank;
        assert_eq!(pending_project_bar(&pending).summary, "none");
        assert_eq!(pending_project_bar(&pending).kind, Some("Project"));
        pending.firmware_face = FirmwareFace::OlderLightPlayer { proto: Some(33) };
        assert_eq!(pending_project_bar(&pending).summary, "not known");
    }

    /// The desk C6 of 2026-10-17: an older LightPlayer plays a project this
    /// Studio cannot read. "not known", never a false "none"; a chip with no
    /// LightPlayer plays nothing; a core alone never says, so no spinner.
    #[test]
    fn a_board_studio_cannot_read_is_not_known_never_none() {
        for (face, name) in [
            (
                FirmwareFace::OlderLightPlayer { proto: Some(33) },
                "not known",
            ),
            (FirmwareFace::NoHello, "not known"),
            (FirmwareFace::Silent, "not known"),
            (FirmwareFace::Bootloader, "not known"),
            (FirmwareFace::Blank, "none"),
            (FirmwareFace::Foreign { label: None }, "none"),
            (
                FirmwareFace::CoreOnly {
                    version: None,
                    state: lpa_devices::UpdateBoardState::NeedsEngine,
                },
                "not known",
            ),
        ] {
            let mut fixture = CardFixture::ready();
            fixture.view.firmware_face = face.clone();
            fixture.view.status = lpa_devices::device::DeviceStatus::NeedsAttention;
            fixture.view.loaded_project = LoadedProject::Unknown;
            fixture.view.can_receive_project = false;
            fixture.view.can_remove_project = false;
            fixture.plays = BoardPlays::Unknown;
            let bar = project_bar(&fixture.input());
            assert_eq!(bar.summary, name, "{face:?}");
            assert_eq!(bar.work, None, "{face:?}: no spinner");
        }
    }

    /// Ported from the old card: the work's narration, then the fault, then
    /// what the board plays.
    #[test]
    fn the_project_line_follows_the_ruled_priority() {
        let mut fixture = CardFixture::ready();
        fixture.view.loaded_project = LoadedProject::Empty;
        fixture.plays = BoardPlays::Nothing;
        assert_eq!(project_bar(&fixture.input()).summary, "none");
        fixture.view.loaded_project = LoadedProject::Running {
            label: "porch-sign".to_string(),
        };
        fixture.plays = BoardPlays::Running {
            label: "porch-sign".to_string(),
        };
        assert_eq!(project_bar(&fixture.input()).summary, "porch-sign");
        fixture.view.degraded = Some("node /studio.show/s faulted".to_string());
        let bar = project_bar(&fixture.input());
        assert_eq!(bar.summary, "porch-sign");
        assert_eq!(bar.aside.as_deref(), Some("error"), "a fault is the state");
        let mut busy = fixture.clone().with_activity(activity(
            ActivityKind::Push,
            "Sending the project",
            Some(40),
        ));
        let bar = project_bar(&busy.input());
        assert_eq!(
            bar.work.map(|work| work.words).as_deref(),
            Some("sending · 40%")
        );
        // A flash is firmware work: the Project row goes on saying the fault.
        let mut flashing =
            fixture.with_activity(activity(ActivityKind::Flash, "Flashing firmware", Some(62)));
        let bar = project_bar(&flashing.input());
        assert_eq!(bar.work, None);
        assert_eq!(bar.aside.as_deref(), Some("error"));
    }

    /// D32: Edit is the row's action on a board you can edit — the `edit`
    /// offer, with the pencil.
    #[test]
    fn edit_is_the_rows_action_at_the_edit_tier() {
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
            "Edit is the row's own action"
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
