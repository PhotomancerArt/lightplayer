//! Story fixtures for Home's own offers (`project/new`, `project/open`).
//!
//! The Projects page's New menu draws `project/new` from the view's offer
//! tree, which the shell provides. A story mounts the page (or the menu) on
//! its own, so it builds the tree the way core publishes it —
//! [`home_offers`] over the story's own home view — and hands it down with
//! [`OffersProvider`]. Nothing here invents a verb.

use dioxus::prelude::*;
use lpa_studio_core::{UiAction, UiHomeView, UiOfferTree, home_offers};

use crate::app::home::ProjectsPage;
use crate::core::OffersProvider;

/// The tree core publishes on Home for `home`.
pub(crate) fn home_offer_tree(home: &UiHomeView) -> UiOfferTree {
    let mut tree = UiOfferTree::new();
    for offer in home_offers(home) {
        tree.publish(offer);
    }
    tree
}

/// `children` under the tree core publishes on Home for `home`.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn HomeOffers(home: UiHomeView, children: Element) -> Element {
    let offers = home_offer_tree(&home);
    rsx! {
        OffersProvider { offers, {children} }
    }
}

/// [`ProjectsPage`] under the tree core publishes on Home for its view.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn StoryProjectsPage(
    home: UiHomeView,
    #[props(default)] now_secs: Option<f64>,
    on_action: EventHandler<UiAction>,
) -> Element {
    rsx! {
        HomeOffers { home: home.clone(),
            ProjectsPage { home, now_secs, on_action }
        }
    }
}
