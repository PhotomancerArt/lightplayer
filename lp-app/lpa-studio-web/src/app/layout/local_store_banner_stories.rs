//! Local-store banner stories: the trouble state. (The whole-library
//! "locked by another tab" state died with M4b's per-project locking.)

use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;

use crate::app::layout::LocalStoreBanner;
use crate::local_store::LocalStoreStatus;

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
