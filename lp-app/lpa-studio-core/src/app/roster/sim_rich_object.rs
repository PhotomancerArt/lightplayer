//! The live simulator session as a rich object (D36, runtime-pool P4).
//!
//! [`sim_rich_object`] builds the sim card's detail sections from the same
//! derived card state the card renders — the schema is the device's
//! (Health, Project, …, Danger zone) with only the sections the sim
//! honestly has data for:
//!
//! | Section | tone source | weight | present when |
//! |---|---|---|---|
//! | Health | the card state's circle tone | Actionable | always |
//! | Project | Neutral (the sim runs the pushed head — no drift) | Actionable | a project is loaded |
//! | Danger zone | Neutral (never colors rollup) | Danger | always — Stop simulator |
//!
//! No Technical section (no identity, transport, or firmware provenance
//! exists for the sim — worker/tier facts don't flow to cards), no Backup
//! (nothing is banked from the sim), no Performance. Omission is honest
//! evidence of absence, exactly like the device builder.
//!
//! The one fact the sim gained is its BOARD identity (gallery-rework
//! vision D4): a sim running a targeted project says "as \<board\>" under
//! its status line — a Health fact, not a Technical claim, because it is
//! what this session is pretending to be rather than anything measured.

use crate::app::rich_object::{RichLine, RichObjectView, RichSection, RichWeight};
use crate::core::status::UiStatusKind;

use super::roster_card_state::RosterCardState;

/// A sim section's affordance identity. Wiring to the concrete action is
/// the renderer's job (matching [`super::DeviceDetailAffordance`]).
#[derive(Clone, Debug, PartialEq)]
pub enum SimDetailAffordance {
    /// Health, project loaded: the visible editor CTA (2026-07-26 walk —
    /// the grow ⤢ stays; the card face says it out loud too).
    OpenEditor,
    /// Danger zone: destroy the simulator session (worker + wire client).
    StopSimulator,
}

/// Everything the sim builder may know. The state is the card's (Running /
/// nothing loaded); the project name is the loaded project's display name.
#[derive(Clone, Debug, PartialEq)]
pub struct SimRichInput<'a> {
    /// The derived sim card state.
    pub state: &'a RosterCardState,
    /// The loaded project's display name, when one is loaded.
    pub project_name: Option<&'a str>,
    /// The board the sim claims to be (`vendor/product`), when it has one
    /// — vision D4. `None` (no board known) is the ordinary default and
    /// simply omits the line.
    pub board_id: Option<&'a str>,
    /// f64 epoch seconds for status-line copy.
    pub now_secs: f64,
}

/// Build the sim's rich-object view. Pure; the section table on the module
/// doc is normative.
pub fn sim_rich_object(input: &SimRichInput<'_>) -> RichObjectView<SimDetailAffordance> {
    let mut sections = vec![health_section(input)];
    sections.extend(project_section(input));
    sections.push(danger_section());
    RichObjectView::new(sections)
}

/// Health: the card state itself, as a section — one derivation, consumed
/// everywhere (the popover can never disagree with the circle). With a
/// project loaded it carries the visible editor CTA (the grow ⇲ stays).
fn health_section(input: &SimRichInput<'_>) -> RichSection<SimDetailAffordance> {
    let mut lines = vec![RichLine::new(
        "status",
        input.state.status_line(input.now_secs),
    )];
    // D4: "as ESP32-S3 DevKitC-1" — the board this session pretends to be.
    // Omitted entirely when no board is known (the default), so an
    // untargeted sim card reads exactly as it did before.
    if let Some(board_id) = input.board_id {
        // the VALUE carries the "as", because the card face renders a
        // Health line's value alone (the label is a kv-row affordance)
        lines.push(RichLine::new(
            "board",
            format!("as {}", board_display_name(board_id)),
        ));
    }
    RichSection {
        title: "Health".to_string(),
        tone: input.state.spec().tone,
        lines,
        chip: None,
        affordances: input
            .project_name
            .is_some()
            .then_some(SimDetailAffordance::OpenEditor)
            .into_iter()
            .collect(),
        weight: RichWeight::Actionable,
    }
}

/// Project: what the sim runs. Load-as-push always runs the pushed head,
/// so there is no drift story — the section is a plain fact row.
fn project_section(input: &SimRichInput<'_>) -> Option<RichSection<SimDetailAffordance>> {
    let name = input.project_name?;
    Some(RichSection {
        title: "Project".to_string(),
        tone: UiStatusKind::Neutral,
        lines: vec![RichLine::new("running", name)],
        chip: None,
        affordances: Vec::new(),
        weight: RichWeight::Actionable,
    })
}

/// A board id's human name for the card line: the catalog's `display_name`
/// when the id is a known board, else the raw id verbatim — advisory
/// metadata may name a board this build's catalog doesn't carry (a future
/// board, a typo'd id), and the line should still say something rather
/// than disappear. Same rule as the project card's "for \<board\>" badge.
fn board_display_name(board_id: &str) -> String {
    lpa_boards::board_by_id(board_id)
        .map(|board| board.display_name.clone())
        .unwrap_or_else(|| board_id.to_string())
}

/// Danger zone, pinned last: Stop simulator (runtime-pool P3's explicit
/// destroy — the worker terminates; unsaved changes on it are gone).
fn danger_section() -> RichSection<SimDetailAffordance> {
    RichSection {
        title: "Danger zone".to_string(),
        // Neutral by construction: Danger weight never colors the rollup;
        // the renderer's inline-tinted treatment carries the red.
        tone: UiStatusKind::Neutral,
        lines: Vec::new(),
        chip: None,
        affordances: vec![SimDetailAffordance::StopSimulator],
        weight: RichWeight::Danger,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: f64 = 1_800_000_000.0;

    #[test]
    fn running_sim_carries_health_project_and_the_stop_danger_zone() {
        let view = sim_rich_object(&SimRichInput {
            state: &RosterCardState::RunningUpToDate,
            project_name: Some("2026-07-02-0930-porch-sign"),
            board_id: None,
            now_secs: NOW,
        });
        assert_eq!(titles(&view), vec!["Health", "Project", "Danger zone"]);

        let rollup = view.rollup();
        assert_eq!(rollup.tone, UiStatusKind::Good);
        assert_eq!(
            rollup.affordance,
            Some(&SimDetailAffordance::OpenEditor),
            "the loaded sim's face carries the visible editor CTA"
        );

        let danger = view.sections.last().unwrap();
        assert_eq!(danger.weight, RichWeight::Danger);
        assert_eq!(danger.affordances, vec![SimDetailAffordance::StopSimulator]);
    }

    #[test]
    fn empty_sim_omits_the_project_section_but_keeps_the_stop() {
        let view = sim_rich_object(&SimRichInput {
            state: &RosterCardState::ConnectedEmpty,
            project_name: None,
            board_id: None,
            now_secs: NOW,
        });
        assert_eq!(titles(&view), vec!["Health", "Danger zone"]);
        assert_eq!(view.rollup().tone, UiStatusKind::Good);
        assert_eq!(
            view.sections[0].lines[0].value, "Connected — nothing loaded",
            "the health fact speaks the card copy"
        );
        assert_eq!(
            view.sections[0].lines.len(),
            1,
            "no board known: no 'as <board>' line at all (today's card)"
        );
    }

    #[test]
    fn a_boarded_sim_says_what_it_is_pretending_to_be() {
        // D4: the sim inherits its board from the project it runs, and the
        // card's fact line names it with the CATALOG's display name.
        let view = sim_rich_object(&SimRichInput {
            state: &RosterCardState::RunningUpToDate,
            project_name: Some("2026-07-02-0930-porch-sign"),
            board_id: Some("seeed/xiao-esp32-c6"),
            now_secs: NOW,
        });
        let health = &view.sections[0];
        assert_eq!(health.lines[1].label, "board");
        assert_eq!(
            health.lines[1].value,
            format!(
                "as {}",
                lpa_boards::board_by_id("seeed/xiao-esp32-c6")
                    .expect("a catalog board")
                    .display_name
            )
        );
    }

    #[test]
    fn an_unknown_board_id_still_says_something() {
        // Advisory metadata may name a board this build doesn't carry —
        // the line degrades to the raw id rather than vanishing.
        let view = sim_rich_object(&SimRichInput {
            state: &RosterCardState::ConnectedEmpty,
            project_name: None,
            board_id: Some("acme/not-a-real-board"),
            now_secs: NOW,
        });
        assert_eq!(view.sections[0].lines[1].value, "as acme/not-a-real-board");
    }

    fn titles(view: &RichObjectView<SimDetailAffordance>) -> Vec<&str> {
        view.sections
            .iter()
            .map(|section| section.title.as_str())
            .collect()
    }
}
