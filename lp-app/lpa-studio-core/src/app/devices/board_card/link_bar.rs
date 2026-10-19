//! One link row, assembled: the row's own facts ([`LinkFacts`]: what USB,
//! Wi‑Fi, Bluetooth or lightplayer.app says about the board), with this
//! tab's session laid over them where it is about that link
//! ([`super::link_session`]). Each link row's file says its facts; this one
//! says how the session's news wins over them, the same way on all four.
//!
//! - Someone holding the board, or a board that stopped answering on this
//!   link, takes the row's words, tone and action.
//! - Work (connecting, reconnecting, a Connect that failed) is the row's
//!   state: the row keeps a name it has (a network, the board's Bluetooth
//!   name, the account) and gives up a bare state word ("not connected"),
//!   and reads neutral while the work runs.
//! - The details lead with the session's notice, then — on the row of the
//!   link in use — how that link reaches the board, then the row's own.

use super::board_card_input::BoardCardInput;
use super::detail_sections::without_empty;
use super::link_session::{reach_facts, session_line, session_notice, session_work};
use super::ui_card_action::UiCardAction;
use super::ui_stack_bar::{BarLayer, UiBarDetails, UiStackBar};
use crate::UiStatusKind;

/// A link row's own facts, before the session's news.
#[derive(Clone, Debug)]
pub(crate) struct LinkFacts {
    /// The row's icon (`usb`, `wifi`, `bluetooth`, `cloud`, or a stand-in's
    /// `play`).
    pub icon: &'static str,
    /// The kind in the row's own words when it is not the link's name: a
    /// stand-in's "Simulated" or "Emulated".
    pub kind: Option<&'static str>,
    /// What the link reaches or how it stands: "connected", "Cedar Lane",
    /// "not connected", "disabled", "—".
    pub summary: String,
    /// The summary names a thing (a network, the board's Bluetooth name,
    /// the account) rather than a state, so it stays while the session's
    /// work says the state.
    pub named: bool,
    pub aside: Option<String>,
    pub tone: UiStatusKind,
    pub action: Option<UiCardAction>,
    /// Last known, dimmed (P13).
    pub stale: bool,
    /// Something is left to do here, and the action does it.
    pub todo: bool,
    /// The row's own details, after the session's notice and the reach.
    pub details: UiBarDetails,
}

impl LinkFacts {
    /// A quiet row with `icon` saying `summary`, and nothing else yet.
    pub fn new(icon: &'static str, summary: impl Into<String>) -> Self {
        Self {
            icon,
            kind: None,
            summary: summary.into(),
            named: false,
            aside: None,
            tone: UiStatusKind::Neutral,
            action: None,
            stale: false,
            todo: false,
            details: UiBarDetails::default(),
        }
    }
}

/// `layer`'s row from its `facts` and this tab's session.
pub(crate) fn link_bar(
    input: &BoardCardInput<'_>,
    layer: BarLayer,
    facts: LinkFacts,
) -> UiStackBar {
    let session = session_line(input, layer);
    let work = session_work(input, layer);
    let news = session.is_some() || work.is_some();
    let (summary, aside, tone, action) = match session {
        Some(line) => (line.summary, line.aside, line.tone, line.action),
        None => (
            match work.is_some() && !facts.named {
                true => String::new(),
                false => facts.summary,
            },
            facts.aside.filter(|_| work.is_none()),
            facts.tone,
            facts.action,
        ),
    };
    let now = match &work {
        Some(work) if summary.is_empty() => work.words.clone(),
        Some(work) => format!("{summary} \u{b7} {}", work.words),
        None => match &aside {
            Some(aside) => format!("{summary} \u{b7} {aside}"),
            None => summary.clone(),
        },
    };
    let mut sections = Vec::new();
    sections.extend(session_notice(input, layer));
    if BarLayer::of_link(input.link_kind()) == layer {
        sections.push(reach_facts(input, &now));
    }
    sections.extend(facts.details.sections);
    let quiet = work.is_some();
    UiStackBar {
        kind: facts.kind.or(layer.kind_word()),
        aside,
        tone: if quiet { UiStatusKind::Neutral } else { tone },
        action: if quiet { None } else { action },
        stale: facts.stale && !news,
        todo: facts.todo && !news,
        work,
        details: UiBarDetails {
            sections: without_empty(sections),
            panels: facts.details.panels,
            raised: facts.details.raised,
        },
        ..UiStackBar::new(layer, facts.icon, summary)
    }
}

/// A radio row Studio says nothing about yet: its kind, and "—".
pub(crate) fn radio_placeholder(input: &BoardCardInput<'_>, layer: BarLayer) -> UiStackBar {
    let icon = layer
        .link()
        .map_or("usb", super::board_card_input::link_icon);
    link_bar(
        input,
        layer,
        LinkFacts::new(icon, super::link_session::NOT_KNOWN),
    )
}
