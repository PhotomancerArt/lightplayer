//! Shell banner for local-store trouble states.
//!
//! Renders nothing while the store is initializing or ready — persistence is
//! invisible when it works.

use dioxus::prelude::*;

use crate::local_store::LocalStoreStatus;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn LocalStoreBanner(status: LocalStoreStatus, on_retry: Option<EventHandler<()>>) -> Element {
    match status {
        LocalStoreStatus::Initializing | LocalStoreStatus::Ready => rsx! {},
        LocalStoreStatus::LockedByAnotherTab => rsx! {
            div {
                class: "tw:mb-3.5 tw:flex tw:items-center tw:gap-3 tw:rounded-md tw:border tw:border-amber-600/40 tw:bg-amber-500/10 tw:px-4 tw:py-2.5 tw:text-sm tw:text-amber-200",
                span { "LightPlayer is open in another tab. Projects can't be saved here until the other tab closes." }
                if let Some(on_retry) = on_retry {
                    button {
                        class: "tw:ml-auto tw:shrink-0 tw:rounded tw:border tw:border-amber-500/50 tw:px-3 tw:py-1 tw:text-amber-100 tw:hover:bg-amber-500/20",
                        onclick: move |_| on_retry.call(()),
                        "Retry"
                    }
                }
            }
        },
        LocalStoreStatus::Unavailable(reason) => rsx! {
            div {
                class: "tw:mb-3.5 tw:rounded-md tw:border tw:border-red-600/40 tw:bg-red-500/10 tw:px-4 tw:py-2.5 tw:text-sm tw:text-red-200",
                span { "This browser can't store projects locally. Changes won't survive a reload." }
                span { class: "tw:ml-2 tw:text-red-300/70", "({reason})" }
            }
        },
    }
}
