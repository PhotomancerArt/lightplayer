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
//!   link in use — how that link reaches the board, then the row's own
//!   facts and panels, then its verbs: on the row of the link in use,
//!   Identify again and Connect (the same link again) first, Disconnect
//!   last, and that link's counters.

use super::board_card_input::{BoardCardInput, link_icon};
use super::detail_sections::{verbs, without_empty};
use super::link_session::{reach_facts, session_line, session_notice, session_work};
use super::primary_action::primary_action;
use super::ui_bar_work::UiBarWork;
use super::ui_card_action::UiCardAction;
use super::ui_detail_panel::UiDetailPanel;
use super::ui_name_bar::UiPrimary;
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
    /// The word on a flush section that opens the row's own details
    /// instead of pressing an offer: "Set up", "Change".
    pub opens_details: Option<&'static str>,
    /// The board's own work on this link (Wi‑Fi joining a network), when
    /// the session has none.
    pub work: Option<UiBarWork>,
    /// Last known, dimmed (P13).
    pub stale: bool,
    /// Something is left to do here, and the action does it.
    pub todo: bool,
    /// The row's own details, after the session's notice and the reach.
    pub details: UiBarDetails,
    /// The row's own verbs (its link's Connect), between Identify again
    /// and Disconnect on the row of the link in use.
    pub verbs: Vec<UiCardAction>,
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
            opens_details: None,
            work: None,
            stale: false,
            todo: false,
            details: UiBarDetails::default(),
            verbs: Vec::new(),
        }
    }

    /// The same facts, saying a name rather than a state: the session's
    /// work keeps it.
    pub fn named(mut self) -> Self {
        self.named = true;
        self
    }
}

/// `layer`'s row from its `facts` and this tab's session.
pub(crate) fn link_bar(
    input: &BoardCardInput<'_>,
    layer: BarLayer,
    facts: LinkFacts,
) -> UiStackBar {
    let session = session_line(input, layer);
    let session_work = session_work(input, layer);
    let news = session.is_some() || session_work.is_some();
    let work = session_work.or(facts.work);
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
    let in_use = BarLayer::of_link(input.link_kind()) == layer;
    let mut sections = Vec::new();
    sections.extend(session_notice(input, layer));
    if in_use {
        sections.push(reach_facts(input, &now));
    }
    sections.extend(facts.details.sections);
    let mut actions = Vec::new();
    let mut panels = facts.details.panels;
    if in_use {
        actions.extend(
            input
                .offer("identify")
                .map(|identify| UiCardAction::press(identify, "Identify again").with_icon("info")),
        );
        actions.extend(connect_verb(
            input,
            "reconnect",
            "Connect",
            link_icon(input.link_kind()),
        ));
    }
    actions.extend(facts.verbs);
    if in_use {
        actions.extend(input.offer("disconnect").map(|disconnect| {
            UiCardAction::own_words(disconnect).with_icon(disconnect.icon.clone())
        }));
        panels.extend(input.view.link_counters.map(UiDetailPanel::LinkCounters));
    }
    sections.push(verbs(actions));
    let quiet = work.is_some();
    UiStackBar {
        kind: facts.kind.or(layer.kind_word()),
        aside,
        tone: if quiet { UiStatusKind::Neutral } else { tone },
        action: if quiet { None } else { action },
        opens_details: facts.opens_details.filter(|_| !news && !quiet),
        stale: facts.stale && !news,
        todo: facts.todo && !news && !quiet,
        work,
        details: UiBarDetails {
            sections: without_empty(sections),
            panels,
            raised: facts.details.raised,
            foot: without_empty(facts.details.foot),
        },
        ..UiStackBar::new(layer, facts.icon, summary)
    }
}

/// A link's Connect in its row's details (`reconnect`, `connect-wifi`,
/// `connect-relay`), worded `word`: not on a board another tab holds,
/// which is reached by the take-over (the primary) — each of these would
/// fight that tab for the board — and not where the primary already offers
/// that road.
pub(crate) fn connect_verb(
    input: &BoardCardInput<'_>,
    verb: &str,
    word: &str,
    icon: &str,
) -> Option<UiCardAction> {
    if input.held().is_some() {
        return None;
    }
    let primary = match primary_action(input, None) {
        Some(UiPrimary::Offer(action)) => Some(action.offer),
        _ => None,
    };
    // The road the primary's Connect takes to an offline board: its own
    // offer is the same reach, and is not offered twice.
    let primary_road = input
        .offer("connect")
        .filter(|connect| Some(&connect.path) == primary.as_ref())
        .and_then(|connect| match connect.action.op_as::<crate::RuntimeOp>() {
            Some(crate::RuntimeOp::ConnectDevice {
                reach: Some(reach), ..
            }) => Some(road_verb(*reach)),
            _ => None,
        });
    if Some(verb) == primary_road {
        return None;
    }
    input
        .offer(verb)
        .filter(|offer| Some(&offer.path) != primary.as_ref())
        .map(|offer| UiCardAction::press(offer, word).with_icon(icon))
}

/// The verb of the offer that reaches an offline board by `reach`, the
/// road a Connect takes there.
fn road_verb(reach: crate::ConnectReach) -> &'static str {
    match reach {
        crate::ConnectReach::Wifi => "connect-wifi",
        crate::ConnectReach::Relay => "connect-relay",
        crate::ConnectReach::Usb => "reconnect",
    }
}
