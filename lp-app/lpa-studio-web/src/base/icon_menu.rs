use dioxus::prelude::*;

use crate::base::{IconPopoverButton, PopoverPlacement, StudioIconName};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn IconMenuButton(
    icon: StudioIconName,
    label: String,
    #[props(default = label.clone())] title: String,
    #[props(default = 14)] icon_size: u32,
    #[props(default = IconMenuTone::Neutral)] tone: IconMenuTone,
    #[props(default = PopoverPlacement::BottomEnd)] placement: PopoverPlacement,
    #[props(default = false)] active: bool,
    #[props(default = false)] initially_open: bool,
    children: Element,
) -> Element {
    rsx! {
        IconPopoverButton {
            class: icon_menu_class(tone, active).to_string(),
            open_class: icon_menu_open_class(tone).to_string(),
            icon,
            icon_size,
            label,
            title,
            popup_class: "tw:grid tw:w-[min(320px,calc(100vw-24px))] tw:gap-3 tw:rounded-md tw:border tw:border-border tw:bg-card tw:p-3 tw:text-sm tw:text-muted-foreground tw:shadow-lg".to_string(),
            placement,
            initially_open,
            {children}
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IconMenuTone {
    Neutral,
    Accent,
    Good,
    Working,
    Warning,
    Error,
}

fn icon_menu_class(tone: IconMenuTone, active: bool) -> &'static str {
    match (tone, active) {
        (IconMenuTone::Neutral, false) => {
            "tw:inline-flex tw:h-6 tw:w-6 tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-border-subtle tw:bg-page tw:p-0 tw:text-subtle-foreground tw:hover:border-border-strong tw:hover:text-muted-foreground"
        }
        (IconMenuTone::Neutral, true) => {
            "tw:inline-flex tw:h-6 tw:w-6 tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-border-strong tw:bg-card-muted tw:p-0 tw:text-muted-foreground"
        }
        (IconMenuTone::Accent, false) => {
            "tw:inline-flex tw:h-6 tw:w-6 tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-border-subtle tw:bg-page tw:p-0 tw:text-subtle-foreground tw:hover:border-accent-border tw:hover:text-accent"
        }
        (IconMenuTone::Accent, true) => {
            "tw:inline-flex tw:h-6 tw:w-6 tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-accent-border tw:bg-accent-bg tw:p-0 tw:text-accent"
        }
        (IconMenuTone::Good, _) => {
            "tw:inline-flex tw:h-6 tw:w-6 tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-status-good-border tw:bg-status-good-bg tw:p-0 tw:text-status-good-foreground"
        }
        (IconMenuTone::Working, _) => {
            "tw:inline-flex tw:h-6 tw:w-6 tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-status-working-border tw:bg-status-working-bg tw:p-0 tw:text-status-working-foreground"
        }
        (IconMenuTone::Warning, _) => {
            "tw:inline-flex tw:h-6 tw:w-6 tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-status-warning-border tw:bg-status-warning-bg tw:p-0 tw:text-status-warning-foreground"
        }
        (IconMenuTone::Error, _) => {
            "tw:inline-flex tw:h-6 tw:w-6 tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-status-error-border tw:bg-status-error-bg tw:p-0 tw:text-status-error-foreground"
        }
    }
}

fn icon_menu_open_class(tone: IconMenuTone) -> &'static str {
    match tone {
        IconMenuTone::Neutral => {
            "tw:inline-flex tw:h-6 tw:w-6 tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-border-strong tw:bg-card-subtle tw:p-0 tw:text-strong-foreground"
        }
        IconMenuTone::Accent => {
            "tw:inline-flex tw:h-6 tw:w-6 tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-accent-border tw:bg-accent-bg tw:p-0 tw:text-accent"
        }
        IconMenuTone::Good => {
            "tw:inline-flex tw:h-6 tw:w-6 tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-status-good-border tw:bg-status-good-bg tw:p-0 tw:text-status-good-foreground"
        }
        IconMenuTone::Working => {
            "tw:inline-flex tw:h-6 tw:w-6 tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-status-working-border tw:bg-status-working-bg tw:p-0 tw:text-status-working-foreground"
        }
        IconMenuTone::Warning => {
            "tw:inline-flex tw:h-6 tw:w-6 tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-status-warning-border tw:bg-status-warning-bg tw:p-0 tw:text-status-warning-foreground"
        }
        IconMenuTone::Error => {
            "tw:inline-flex tw:h-6 tw:w-6 tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-status-error-border tw:bg-status-error-bg tw:p-0 tw:text-status-error-foreground"
        }
    }
}
