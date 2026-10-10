//! The play page of the session the lens holds — where a connected card's
//! **All controls** goes — in a context every card can read.
//!
//! The router builds the address once ([`crate::router::play_address`]) and
//! the web app provides it here ([`use_provide_card_play_address`]); the
//! connected card reads it ([`use_card_play_address`]). Only one board is
//! connected at a time, so one address serves every card: a card that
//! draws no panel never reads it into anything.
//!
//! Re-render cost: the signal is written only when the address changes.

use dioxus::prelude::*;

/// The lens session's play address, shared through Dioxus context.
#[derive(Clone, Copy)]
struct CardPlayAddress(Signal<Option<String>>);

/// Provide `address` to every card below the caller. Call it on every
/// render with the view's current address; readers are notified only when
/// it changes.
pub(crate) fn use_provide_card_play_address(address: Option<String>) {
    let mut provided = use_context_provider(|| CardPlayAddress(Signal::new(address.clone()))).0;
    if *provided.peek() != address {
        provided.set(address);
    }
}

/// The lens session's play address, or `None` outside a provider (a story
/// that mounts a card on its own provides its own, or draws All controls
/// without a link).
pub(crate) fn use_card_play_address() -> Option<String> {
    let fallback = use_signal(|| None::<String>);
    let address = use_hook(try_consume_context::<CardPlayAddress>).map_or(fallback, |it| it.0);
    address.read().clone()
}
