//! The device/sim cards' rich-object detail popovers — **retired from
//! cards** (M7′ P1: the card's icon tabs render the same sections; see
//! `device_card.rs`). No card mounts these anymore; the components are
//! kept compiling only until M7′ P3 deletes the file (the ratified
//! sequencing: P1 unmounts, P3 deletes alongside `StatusCircle` and the
//! ADR amendments). `DetailPopover`/`RichObjectPane` stay for nodes.

#![allow(
    dead_code,
    reason = "unmounted from cards in M7' P1; the file is deleted in P3"
)]

use dioxus::prelude::*;
use lpa_studio_core::{
    BundledFirmware, DeviceRichInput, RichSection, SimRichInput, UiAction, UiDeviceCard,
    device_rich_object, sim_rich_object,
};

use crate::app::affordance::{status_trigger_active, status_trigger_style};
use crate::app::home::device_card::{wire_section, wire_sim_section};
use crate::base::{DetailPopover, DetailSection};
use crate::core::RichDetailSection;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn DeviceDetailPopover(
    card: UiDeviceCard,
    /// Fixed clock (stories) or the platform clock, resolved by the card.
    now_secs: f64,
    /// Studio's bundled firmware image, when the packaged manifest is on
    /// hand — evidence for the advisory update chip.
    #[props(default)]
    bundled_fw: Option<BundledFirmware>,
    /// Open on mount (story captures only).
    #[props(default = false)]
    initially_open: bool,
    on_action: EventHandler<UiAction>,
) -> Element {
    let view = device_rich_object(&DeviceRichInput {
        state: &card.state,
        uid: card.uid.as_deref(),
        transport: &card.transport,
        project_name: card.project.as_ref().map(|chip| chip.name.as_str()),
        fw: card.fw.as_ref(),
        bundled_fw: bundled_fw.as_ref(),
        now_secs,
    });
    let rollup = view.rollup();
    let style = status_trigger_style(rollup.tone);
    let active = status_trigger_active(rollup.tone);
    let label = format!("{} details", card.name);
    let sections: Vec<RichSection<UiAction>> = view
        .sections
        .into_iter()
        .map(|section| wire_section(&card, section))
        .collect();

    rsx! {
        DetailPopover {
            icon: style.icon,
            label,
            tone: style.tone,
            active,
            initially_open,
            DetailSection {
                div { class: "tw:flex tw:min-w-0 tw:items-start tw:justify-between tw:gap-4 tw:py-1",
                    div { class: "tw:grid tw:min-w-0 tw:gap-0.5",
                        strong { class: "tw:min-w-0 tw:text-sm tw:text-strong-foreground tw:break-words",
                            "{card.name}"
                        }
                        span { class: "tw:text-xs tw:font-bold tw:text-subtle-foreground", "Device" }
                    }
                }
            }
            for section in sections {
                RichDetailSection { section, on_action }
            }
        }
    }
}

/// The sim card's rich-object detail popover — retired with the device
/// popover above (the sim card's tabs carry the sections).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn SimDetailPopover(
    card: UiDeviceCard,
    /// Fixed clock (stories) or the platform clock, resolved by the card.
    now_secs: f64,
    /// Open on mount (story captures only).
    #[props(default = false)]
    initially_open: bool,
    on_action: EventHandler<UiAction>,
) -> Element {
    let view = sim_rich_object(&SimRichInput {
        state: &card.state,
        project_name: card.project.as_ref().map(|chip| chip.name.as_str()),
        now_secs,
    });
    let rollup = view.rollup();
    let style = status_trigger_style(rollup.tone);
    let active = status_trigger_active(rollup.tone);
    let label = format!("{} details", card.name);
    let sections: Vec<RichSection<UiAction>> =
        view.sections.into_iter().map(wire_sim_section).collect();

    rsx! {
        DetailPopover {
            icon: style.icon,
            label,
            tone: style.tone,
            active,
            initially_open,
            DetailSection {
                div { class: "tw:flex tw:min-w-0 tw:items-start tw:justify-between tw:gap-4 tw:py-1",
                    div { class: "tw:grid tw:min-w-0 tw:gap-0.5",
                        strong { class: "tw:min-w-0 tw:text-sm tw:text-strong-foreground tw:break-words",
                            "{card.name}"
                        }
                        span { class: "tw:text-xs tw:font-bold tw:text-subtle-foreground", "Simulator" }
                    }
                }
            }
            for section in sections {
                RichDetailSection { section, on_action }
            }
        }
    }
}
