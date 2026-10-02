//! The view's offer tree, in a context every component under the shell can
//! read.
//!
//! Core publishes every migrated surface's verbs into
//! `UiStudioView::offers`; the shell provides that tree here once
//! ([`use_provide_offers`]), and a surface asks for its own verbs
//! ([`use_verbs_of`]) instead of reading an action field off its DTO.
//!
//! Re-render cost: the shell rebuilds the tree on every view, but the
//! context signal is only written when the tree actually differs, and each
//! surface reads its verbs through a memo, so a card re-renders when ITS
//! verbs change, never because some other card's did.

use dioxus::prelude::*;
use lpa_studio_core::{OfferPath, UiOffer, UiOfferTree};

/// The view's offer tree, shared through Dioxus context.
#[derive(Clone, Copy)]
struct OffersContext(Signal<UiOfferTree>);

/// Provide `offers` to every component below the caller. Call it on every
/// render with the view's current tree; readers are only notified when the
/// tree changes.
pub fn use_provide_offers(offers: &UiOfferTree) {
    let mut tree = use_context_provider(|| OffersContext(Signal::new(offers.clone()))).0;
    if *tree.peek() != *offers {
        tree.set(offers.clone());
    }
}

/// The view's offer tree. Outside any provider (a story or docs embed that
/// renders a surface on its own) it is an empty tree, so the surface
/// renders with no verbs rather than failing.
pub fn use_offers() -> Signal<UiOfferTree> {
    let fallback = use_signal(UiOfferTree::default);
    use_hook(try_consume_context::<OffersContext>).map_or(fallback, |context| context.0)
}

/// The verbs directly under `prefix`, in publish order (see
/// [`UiOfferTree::verbs_of`]); `None` (a surface with no address) has
/// none. Memoized: the caller re-renders only when this list changes.
pub fn use_verbs_of(prefix: Option<OfferPath>) -> Memo<Vec<UiOffer>> {
    let tree = use_offers();
    let mut at = use_signal(|| prefix.clone());
    if *at.peek() != prefix {
        at.set(prefix);
    }
    use_memo(move || match &*at.read() {
        Some(prefix) => tree.read().verbs_of(prefix).cloned().collect(),
        None => Vec::new(),
    })
}

/// Provide `offers` to `children` — the stories' way to hand a surface the
/// tree a shell would have provided.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn OffersProvider(offers: UiOfferTree, children: Element) -> Element {
    use_provide_offers(&offers);
    children
}
