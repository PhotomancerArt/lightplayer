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
                class: "tw:mb-3.5 tw:rounded-md tw:border tw:border-status-error-border tw:bg-status-error-bg tw:px-4 tw:py-2.5 tw:text-sm tw:text-status-error-foreground",
                span { "This browser can't store projects locally. Changes won't survive a reload." }
                span { class: "tw:ml-2 tw:text-status-error-foreground/70", "({reason})" }
            }
        },
    }
}
