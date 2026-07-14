//! Shell banner for local-store trouble states.
//!
//! Renders nothing while the store is initializing or ready — persistence is
//! invisible when it works.

use dioxus::prelude::*;

use crate::local_store::LocalStoreStatus;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn LocalStoreBanner(status: LocalStoreStatus) -> Element {
    match status {
        LocalStoreStatus::Initializing | LocalStoreStatus::Ready => rsx! {},
        LocalStoreStatus::Unavailable(reason) => rsx! {
            div {
                class: "tw:mb-3.5 tw:rounded-md tw:border tw:border-red-600/40 tw:bg-red-500/10 tw:px-4 tw:py-2.5 tw:text-sm tw:text-red-200",
                span { "This browser can't store projects locally. Changes won't survive a reload." }
                span { class: "tw:ml-2 tw:text-red-300/70", "({reason})" }
            }
        },
    }
}
