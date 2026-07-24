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

use crate::app::rich_object::{RichLine, RichObjectView, RichSection, RichWeight};
use crate::core::status::UiStatusKind;

use super::roster_card_state::RosterCardState;

/// A sim section's affordance identity. Wiring to the concrete action is
/// the renderer's job (matching [`super::DeviceDetailAffordance`]).
#[derive(Clone, Debug, PartialEq)]
pub enum SimDetailAffordance {
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
/// everywhere (the popover can never disagree with the circle). No
/// affordance: the card body IS the open-editor click.
fn health_section(input: &SimRichInput<'_>) -> RichSection<SimDetailAffordance> {
    RichSection {
        title: "Health".to_string(),
        tone: input.state.circle().tone,
        lines: vec![RichLine::new(
            "status",
            input.state.status_line(input.now_secs),
        )],
        chip: None,
        affordances: Vec::new(),
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
            now_secs: NOW,
        });
        assert_eq!(titles(&view), vec!["Health", "Project", "Danger zone"]);

        let rollup = view.rollup();
        assert_eq!(rollup.tone, UiStatusKind::Good);
        assert_eq!(rollup.affordance, None, "the card click is the action");

        let danger = view.sections.last().unwrap();
        assert_eq!(danger.weight, RichWeight::Danger);
        assert_eq!(danger.affordances, vec![SimDetailAffordance::StopSimulator]);
    }

    #[test]
    fn empty_sim_omits_the_project_section_but_keeps_the_stop() {
        let view = sim_rich_object(&SimRichInput {
            state: &RosterCardState::ConnectedEmpty,
            project_name: None,
            now_secs: NOW,
        });
        assert_eq!(titles(&view), vec!["Health", "Danger zone"]);
        assert_eq!(view.rollup().tone, UiStatusKind::Good);
        assert_eq!(
            view.sections[0].lines[0].value, "Connected — nothing loaded",
            "the health fact speaks the card copy"
        );
    }

    fn titles(view: &RichObjectView<SimDetailAffordance>) -> Vec<&str> {
        view.sections
            .iter()
            .map(|section| section.title.as_str())
            .collect()
    }
}
