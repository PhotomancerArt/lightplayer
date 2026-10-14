//! [`UiBoardCard`]: one board, as the card draws it — the picture, the
//! status corner, the name bar and its primary, then the five bars. Built in
//! core ([`super::board_card`], [`super::pending_board_card`]) from the
//! roster's facts and the board's own offers, so the web decides how a bar
//! looks and never what it says or which offer it carries
//! (`docs/adr/2026-10-08-the-board-card-and-one-home-page.md`).

use lpa_devices::DeviceId;

use crate::OfferPath;

use super::ui_bar_work::{BarWorkState, UiBarWork};
use super::ui_board_picture::UiBoardPicture;
use super::ui_card_action::UiCardAction;
use super::ui_name_bar::{UiNameBar, UiPrimary};
use super::ui_stack_bar::{BarLayer, UiStackBar};
use super::ui_status_corner::UiStatusCorner;

/// One board's card.
#[derive(Clone, Debug, PartialEq)]
pub struct UiBoardCard {
    /// `devices/<board ref>`: every action on the card lives under it.
    pub board: OfferPath,
    /// The roster's handle: the web's key, and the picture's mount lease.
    pub device: DeviceId,
    /// New, online or offline: the home page's sections (M3).
    pub presence: UiBoardPresence,
    pub picture: UiBoardPicture,
    pub status: UiStatusCorner,
    pub name_bar: UiNameBar,
    /// Always five, in [`BarLayer::ALL`]'s order.
    pub bars: Vec<UiStackBar>,
}

/// Which section of the home page a board belongs in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiBoardPresence {
    /// A link still saying who it is.
    New,
    /// A board Studio reaches now.
    Online,
    /// A board Studio remembers and cannot reach.
    Offline,
}

impl UiBoardCard {
    /// The bar of `layer`.
    pub fn bar(&self, layer: BarLayer) -> &UiStackBar {
        self.bars
            .iter()
            .find(|bar| bar.layer == layer)
            .expect("a card always has its five bars")
    }

    /// Every action anywhere on the card: the primary, each bar's action,
    /// its work's Retry, and every verb in every details section.
    pub fn actions(&self) -> Vec<&UiCardAction> {
        let mut actions = Vec::new();
        if let Some(UiPrimary::Offer(action)) = &self.name_bar.primary {
            actions.push(action);
        }
        for section in &self.status.details.sections {
            actions.extend(section.affordances.iter());
        }
        for bar in &self.bars {
            actions.extend(bar.action.iter());
            if let Some(UiBarWork {
                state: BarWorkState::Failed { retry: Some(retry) },
                ..
            }) = &bar.work
            {
                actions.push(retry);
            }
            for section in &bar.details.sections {
                actions.extend(section.affordances.iter());
            }
        }
        actions
    }

    /// Every offer the card points at: its actions', the running work's
    /// Cancel, and the paths its panels name.
    pub fn offer_paths(&self) -> Vec<&OfferPath> {
        let mut paths: Vec<&OfferPath> = self
            .actions()
            .into_iter()
            .map(|action| &action.offer)
            .collect();
        for bar in &self.bars {
            if let Some(cancel) = bar.work.as_ref().and_then(|work| work.cancel.as_ref()) {
                paths.push(cancel);
            }
            for panel in &bar.details.panels {
                paths.extend(panel.offer_paths());
            }
        }
        for panel in &self.status.details.panels {
            paths.extend(panel.offer_paths());
        }
        paths
    }
}
