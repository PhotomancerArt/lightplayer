//! Local-store banner stories: the two trouble states.

use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;

use crate::app::layout::LocalStoreBanner;
use crate::local_store::LocalStoreStatus;

#[story]
fn locked_by_another_tab() -> Element {
    rsx! {
        section { class: "tw:p-4",
            LocalStoreBanner {
                status: LocalStoreStatus::LockedByAnotherTab,
                on_retry: |_| {},
            }
        }
    }
}

#[story]
fn storage_unavailable() -> Element {
    rsx! {
        section { class: "tw:p-4",
            LocalStoreBanner {
                status: LocalStoreStatus::Unavailable("opfs get_directory failed".to_string()),
            }
        }
    }
}
