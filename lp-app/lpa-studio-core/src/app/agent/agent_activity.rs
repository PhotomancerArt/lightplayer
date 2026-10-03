//! [`AgentActivity`]: what the app agent just did, said by where it lives —
//! the offer it pressed, the card it handed over, the node it edited — so
//! the page can light that control for a moment and the chat can say where
//! it is (agentic-UI roadmap M8, D2: "show in the ui what the agent is
//! doing … the agent should be able to help the user learn").
//!
//! Sans-IO. Every entry is stamped with the caller's clock (the
//! controller's injected `now_secs`) and goes dark at its own expiry;
//! nothing here reads time or schedules a wake. The controller asks
//! [`AgentActivity::went_dark`] on each batch, so a light that expired
//! republishes the view without it.
//!
//! A light is a reading, never a move: it changes no focus and scrolls
//! nothing. Only the user's own Show ([`crate::AgentOp::Show`]) asks the
//! page to bring a control into view ([`UiAgentReveal`]).

use std::collections::VecDeque;

use crate::OfferPath;
use crate::app::agent::ui_agent_activity::{UiAgentActivity, UiAgentLit, UiAgentReveal};

/// How long one light stays on, in seconds of the injected clock: long
/// enough to find with the eye after a glance from the chat, short enough
/// that a run of presses reads as a trail rather than a lit-up page.
pub const AGENT_ACTIVITY_LIT_SECS: f64 = 4.0;

/// How many entries the history keeps (oldest drop first). The chat's rows
/// find their place phrase and their Show through it, so a row older than
/// this still names its press but has no Show any more.
pub const AGENT_ACTIVITY_KEPT: usize = 64;

/// What the agent did at an entry's target.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AgentActivityKind {
    /// It pressed the offer.
    Pressed,
    /// It handed the offer to the user on a card: the real control the
    /// card stands for is lit too, so the user learns where it lives.
    Handed,
    /// It edited the node (the target is the node's prefix,
    /// `project/demo.module/fixture.fixture`).
    Edited,
}

/// One thing the agent did.
#[derive(Clone, Debug, PartialEq)]
pub struct AgentActivityEntry {
    /// Monotonic for the page's life; a re-light is a new entry.
    pub seq: u64,
    /// Where it lives: an offer path, or a node's prefix for an edit.
    pub target: OfferPath,
    pub kind: AgentActivityKind,
    /// The control's name as the user reads it (`Save`, `Remove node`, the
    /// node's name for an edit).
    pub label: String,
    /// Where on the page it is, as a phrase that follows the label
    /// (`in the project header`, `on the fixture card`).
    pub place: String,
    /// When it happened (the injected clock, seconds).
    pub at_secs: f64,
    /// When its light goes out.
    pub until_secs: f64,
}

impl AgentActivityEntry {
    /// Whether its light is still on at `now`.
    pub fn is_lit(&self, now_secs: f64) -> bool {
        now_secs < self.until_secs
    }
}

/// The app agent's recent activity: a bounded history, the lights still
/// on, and the last reveal the user asked for.
#[derive(Clone, Debug, Default)]
pub struct AgentActivity {
    entries: VecDeque<AgentActivityEntry>,
    next_seq: u64,
    /// The entries the last view showed lit, by seq: what
    /// [`Self::went_dark`] watches.
    lit: Vec<u64>,
    reveal: Option<UiAgentReveal>,
}

impl AgentActivity {
    /// Record that the agent did `kind` at `target` at `now`; its light is
    /// on for [`AGENT_ACTIVITY_LIT_SECS`]. Returns the entry's seq.
    pub fn record(
        &mut self,
        target: OfferPath,
        kind: AgentActivityKind,
        label: impl Into<String>,
        place: impl Into<String>,
        now_secs: f64,
    ) -> u64 {
        self.next_seq += 1;
        let seq = self.next_seq;
        self.entries.push_back(AgentActivityEntry {
            seq,
            target,
            kind,
            label: label.into(),
            place: place.into(),
            at_secs: now_secs,
            until_secs: now_secs + AGENT_ACTIVITY_LIT_SECS,
        });
        while self.entries.len() > AGENT_ACTIVITY_KEPT {
            self.entries.pop_front();
        }
        self.lit.push(seq);
        seq
    }

    /// The user's Show for `target`: light it again (a new entry like the
    /// newest one there) and ask the page to bring it into view. `false`
    /// when the history knows nothing at `target`.
    pub fn show(&mut self, target: &OfferPath, now_secs: f64) -> bool {
        let Some(latest) = self.latest(target).cloned() else {
            return false;
        };
        let seq = self.record(
            latest.target,
            latest.kind,
            latest.label,
            latest.place,
            now_secs,
        );
        self.reveal = Some(UiAgentReveal {
            path: target.clone(),
            generation: seq,
        });
        true
    }

    /// Whether a light the last view showed has gone out by `now` — the
    /// view must be published again without it. Each light is reported
    /// once.
    pub fn went_dark(&mut self, now_secs: f64) -> bool {
        let before = self.lit.len();
        let entries = &self.entries;
        self.lit.retain(|seq| {
            entries
                .iter()
                .any(|entry| entry.seq == *seq && entry.is_lit(now_secs))
        });
        self.lit.len() != before
    }

    /// The newest entry at `target`, if the history holds one.
    pub fn latest(&self, target: &OfferPath) -> Option<&AgentActivityEntry> {
        self.entries
            .iter()
            .rev()
            .find(|entry| &entry.target == target)
    }

    /// Every target the history holds, newest first, each once.
    pub fn targets(&self) -> Vec<&OfferPath> {
        let mut targets: Vec<&OfferPath> = Vec::new();
        for entry in self.entries.iter().rev() {
            if !targets.contains(&&entry.target) {
                targets.push(&entry.target);
            }
        }
        targets
    }

    /// The history, oldest first.
    pub fn entries(&self) -> impl Iterator<Item = &AgentActivityEntry> {
        self.entries.iter()
    }

    /// What the page draws at `now`: the lights still on (newest last) and
    /// the last reveal.
    pub fn view(&self, now_secs: f64) -> UiAgentActivity {
        UiAgentActivity {
            lit: self
                .entries
                .iter()
                .filter(|entry| entry.is_lit(now_secs))
                .map(|entry| UiAgentLit {
                    path: entry.target.clone(),
                    seq: entry.seq,
                    kind: entry.kind,
                })
                .collect(),
            reveal: self.reveal.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn save() -> OfferPath {
        OfferPath::project().child("save")
    }

    #[test]
    fn a_press_is_lit_until_its_expiry_and_then_goes_dark_once() {
        let mut activity = AgentActivity::default();
        activity.record(
            save(),
            AgentActivityKind::Pressed,
            "Save",
            "in the project header",
            10.0,
        );

        let lit = activity.view(10.5).lit;
        assert_eq!(lit.len(), 1);
        assert_eq!(lit[0].path, save());
        assert!(!activity.went_dark(10.0 + AGENT_ACTIVITY_LIT_SECS - 0.1));

        let later = 10.0 + AGENT_ACTIVITY_LIT_SECS;
        assert!(activity.view(later).lit.is_empty());
        assert!(activity.went_dark(later), "the light went out");
        assert!(!activity.went_dark(later + 1.0), "reported once");
        assert_eq!(
            activity.latest(&save()).map(|entry| entry.place.as_str()),
            Some("in the project header"),
            "the history keeps it after the light is out"
        );
    }

    #[test]
    fn show_relights_the_newest_entry_and_asks_for_a_reveal() {
        let mut activity = AgentActivity::default();
        assert!(!activity.show(&save(), 0.0), "nothing to show yet");
        activity.record(save(), AgentActivityKind::Pressed, "Save", "here", 0.0);

        assert!(activity.show(&save(), 100.0));
        let view = activity.view(100.0);
        assert_eq!(view.lit.len(), 1);
        let reveal = view.reveal.expect("a reveal");
        assert_eq!(reveal.path, save());
        assert_eq!(reveal.generation, view.lit[0].seq);
        assert!(activity.show(&save(), 101.0));
        assert!(
            activity.view(101.0).reveal.unwrap().generation > reveal.generation,
            "each Show is a new reveal"
        );
    }

    #[test]
    fn the_history_is_bounded_and_targets_come_newest_first_once() {
        let mut activity = AgentActivity::default();
        let revert = OfferPath::project().child("revert");
        activity.record(save(), AgentActivityKind::Pressed, "Save", "", 0.0);
        activity.record(
            revert.clone(),
            AgentActivityKind::Pressed,
            "Revert",
            "",
            1.0,
        );
        activity.record(save(), AgentActivityKind::Pressed, "Save", "", 2.0);
        assert_eq!(activity.targets(), vec![&save(), &revert]);

        for at in 0..AGENT_ACTIVITY_KEPT {
            activity.record(save(), AgentActivityKind::Pressed, "Save", "", at as f64);
        }
        assert_eq!(activity.entries().count(), AGENT_ACTIVITY_KEPT);
        assert!(activity.latest(&revert).is_none(), "the oldest dropped");
    }
}
