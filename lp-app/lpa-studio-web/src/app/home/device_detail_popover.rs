//! The device card's rich-object detail popover.
//!
//! Q1 of the rich-object spike: the trigger is the NODE-STYLE
//! affordance-following icon at the header's right edge — a quiet "i"
//! that escalates with the rolled-up status (the same `DetailPopover`
//! trigger the node header wears) — while the status circle stays a pure
//! indicator on the left. The popover carries the device's rich-object
//! sections ([`device_rich_object`]) in fixed schema order, danger zone
//! pinned last as the inline red-tinted section (Q5). The interim
//! More-menu this replaces lived here in `device_card.rs`; its
//! Flash/Erase/Forget rows migrated into the danger zone.

use dioxus::prelude::*;
use lpa_studio_core::{
    BundledFirmware, DeviceDetailAffordance, DeviceRichInput, RichSection, UiAction, UiDeviceCard,
    device_rich_object,
};

use crate::app::affordance::{status_trigger_active, status_trigger_style};
use crate::app::home::device_card::{
    device_affordance_action, erase_device_action, flash_device_action_destructive,
    forget_device_action,
};
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
    // Identity→action wiring happens here, once, so the generic renderer
    // below never learns device ops. Identities without a live flow (open
    // editor before M5, name-device — the card's inline form owns it)
    // render no row.
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
            // Identity section, mirroring the node popover's anatomy. The
            // status word itself lives in the Health section below — never
            // duplicated here.
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

/// Map one section's affordance identities onto concrete `UiAction`s for
/// this card.
fn wire_section(
    card: &UiDeviceCard,
    section: RichSection<DeviceDetailAffordance>,
) -> RichSection<UiAction> {
    RichSection {
        title: section.title,
        tone: section.tone,
        lines: section.lines,
        chip: section.chip,
        affordances: section
            .affordances
            .iter()
            .filter_map(|affordance| wire_affordance(card, affordance))
            .collect(),
        weight: section.weight,
    }
}

fn wire_affordance(card: &UiDeviceCard, affordance: &DeviceDetailAffordance) -> Option<UiAction> {
    match affordance {
        DeviceDetailAffordance::Roster(affordance) => device_affordance_action(card, affordance),
        DeviceDetailAffordance::FlashFirmware => Some(flash_device_action_destructive()),
        DeviceDetailAffordance::EraseDevice => Some(erase_device_action(card.name.clone())),
        DeviceDetailAffordance::ForgetDevice => card
            .uid
            .clone()
            .map(|uid| forget_device_action(uid, card.name.clone())),
    }
}
