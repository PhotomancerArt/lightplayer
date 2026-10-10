//! Every board's card, from the roster view and the published offer tree:
//! a card for each pending link (a new board), then one for each roster
//! board, in the roster's order.
//!
//! The controller calls this once its view's offers are published
//! (`DeviceRosterView.cards`, and the editor's docked card); a story calls
//! it on its fixtures, so a story's page shows exactly the cards core would
//! build for that roster and never hand-builds one.

use lpa_devices::DeviceId;

use super::{BoardCardInput, UiBoardCard, board_card, pending_board_card};
use crate::{DeviceRosterView, DeviceView, UiOffer, UiOfferTree, UiPackageCard};

/// What every card on a roster is built from.
#[derive(Clone, Copy, Debug)]
pub struct RosterCardsInput<'a> {
    /// The roster and its side maps: feeds, bands, access, Wi‑Fi, updates,
    /// layout, which board plays which project, link kinds, last seen,
    /// activity ends.
    pub roster: &'a DeviceRosterView,
    /// The view's published tree: each card reads only its own board's
    /// verbs, so it can point at nothing core did not offer.
    pub offers: &'a UiOfferTree,
    /// The library: the project bar names the project a board plays.
    pub projects: &'a [UiPackageCard],
    /// The board the editor is open on: its card has no primary until
    /// Done lands.
    pub lens: Option<DeviceId>,
    /// Now, in epoch seconds (ages and a Done bar's few seconds).
    pub now: f64,
}

/// Every card on the roster: new boards first, then the roster's boards. A
/// board the tree places nowhere has no card.
pub fn roster_board_cards(input: &RosterCardsInput<'_>) -> Vec<UiBoardCard> {
    let RosterCardsInput { roster, offers, .. } = *input;
    let mut cards = Vec::new();
    for pending in &roster.roster.pending {
        let Some(board) = offers.device_prefix(pending.device) else {
            continue;
        };
        let verbs: Vec<UiOffer> = offers.own_verbs_of(board).cloned().collect();
        // How it arrived: the pending link's endpoint, read as the roster's
        // boards are.
        let link = roster
            .link_kinds
            .get(&pending.device)
            .copied()
            .unwrap_or_default();
        cards.push(pending_board_card(pending, board, &verbs, link));
    }
    for view in &roster.roster.devices {
        if let Some(card) = roster_board_card(input, view) {
            cards.push(card);
        }
    }
    cards
}

/// One roster board's card, built from the roster view's joins and the
/// board's own verbs; `None` for a board the tree places nowhere.
pub fn roster_board_card(input: &RosterCardsInput<'_>, view: &DeviceView) -> Option<UiBoardCard> {
    let RosterCardsInput {
        roster,
        offers,
        projects,
        lens,
        now,
    } = *input;
    let board = offers.device_prefix(view.id)?;
    let verbs: Vec<UiOffer> = offers.own_verbs_of(board).cloned().collect();
    let plays = roster.board_projects.plays(view.id);
    let project = plays
        .project_uid()
        .and_then(|uid| projects.iter().find(|project| project.uid == uid));
    // The other boards playing the same project, by title, in roster
    // order.
    let shared_with: Vec<String> = plays
        .project_uid()
        .map(|uid| roster.board_projects.boards_playing(uid))
        .unwrap_or_default()
        .into_iter()
        .filter(|board| *board != view.id)
        .filter_map(|board| {
            roster
                .roster
                .devices
                .iter()
                .find(|device| device.id == board)
                .map(|device| device.title.clone())
        })
        .collect();
    Some(board_card(&BoardCardInput {
        view,
        board,
        offers: &verbs,
        link: roster.link_kinds.get(&view.id).copied(),
        feed: roster.feeds.get(&view.id),
        runtime: roster.runtime_bands.get(&view.id),
        access: roster.access.get(&view.id),
        wifi: roster.wifi.get(&view.id),
        lan: roster.lan_links.get(&view.id),
        wifi_connect: roster.wifi_connects.get(&view.id),
        take_over: roster.take_overs.get(&view.id),
        update: roster.updates.get(&view.id),
        layout: roster.layout.get(&view.id),
        plays,
        sharing: roster.board_projects.sharing(view.id),
        project,
        shared_with: &shared_with,
        last_seen_at: roster.last_seen.get(&view.id).copied(),
        ended: roster.ends.get(&view.id),
        editor_holds_it: lens == Some(view.id),
        now,
    }))
}
