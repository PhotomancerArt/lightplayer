//! The Devices page (`#/`, vision D9): the runtime roster.
//!
//! ⚠️ **Device support is being rebuilt** (M2 of the device-model
//! rebuild). The old page — the device roster, the two creation cards, the
//! setup wizard, the recovery-flash chip and the granted-ports probe —
//! was deleted with the device system it drove. What remains is honest:
//! the live simulator's card when a sim session is running, and a note
//! saying what happened to the rest, plus the names of the boards Studio
//! still remembers (the registry survived — it is the store the rebuilt
//! model reads).
//!
//! No dead buttons: nothing here dispatches a device verb, because there
//! are no device verbs to dispatch.

use dioxus::prelude::*;
use lpa_studio_core::{UiAction, UiHomeView};

use crate::app::home::sim_card::SimCard;
use crate::app::home::{device_grid_class, section_title_class};

/// The runtime roster page (roadmap M4's gallery top, re-homed).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn DevicesPage(home: UiHomeView, on_action: EventHandler<UiAction>) -> Element {
    let remembered = home.remembered.clone();
    rsx! {
        div { class: "tw:grid tw:content-start tw:gap-7",
            if let Some(issue) = home.issue.clone() {
                div { class: "tw:flex tw:items-center tw:gap-3 tw:rounded-md tw:border tw:border-status-error-border tw:bg-status-error-bg tw:px-4 tw:py-2.5 tw:text-sm tw:text-status-error-foreground",
                    span { "{issue.message}" }
                }
            }

            // The live simulator, while a session is running (D36: its
            // card exists exactly as long as the session does).
            if let Some(card) = home.sim.clone() {
                section { class: "tw:grid tw:gap-3",
                    header { class: "tw:flex tw:items-baseline tw:justify-between tw:gap-3",
                        h2 { class: section_title_class(), "Runtimes" }
                    }
                    div { class: device_grid_class(),
                        SimCard { key: "{card.render_key()}", card, on_action }
                    }
                }
            }

            section { class: "tw:grid tw:gap-3",
                header { class: "tw:flex tw:items-baseline tw:justify-between tw:gap-3",
                    h2 { class: section_title_class(), "Devices" }
                }
                div { class: "tw:grid tw:gap-2 tw:rounded-md tw:border tw:border-dashed tw:border-border tw:px-4 tw:py-5",
                    p { class: "tw:m-0 tw:text-sm tw:font-semibold tw:text-strong-foreground",
                        "Device support is being rebuilt"
                    }
                    p { class: "tw:m-0 tw:max-w-prose tw:text-xs tw:leading-relaxed tw:text-subtle-foreground",
                        "Connecting, flashing and pushing to hardware are unavailable in this build. \
                         The simulator is unaffected, and your projects and the devices Studio \
                         remembers are untouched."
                    }
                    if !remembered.is_empty() {
                        div { class: "tw:grid tw:gap-1",
                            p { class: "tw:m-0 tw:text-[0.68rem] tw:font-bold tw:uppercase tw:tracking-wide tw:text-subtle-foreground",
                                "Remembered"
                            }
                            ul { class: "tw:m-0 tw:grid tw:list-none tw:gap-0.5 tw:p-0",
                                for name in remembered.iter() {
                                    li { key: "{name}",
                                        class: "tw:m-0 tw:font-mono tw:text-xs tw:text-muted-foreground",
                                        "{name}"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
