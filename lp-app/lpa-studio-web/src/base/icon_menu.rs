use dioxus::prelude::*;

use crate::base::{IconPopoverButton, PopoverPlacement, StudioIconName};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn IconMenuButton(
    icon: StudioIconName,
    label: String,
    #[props(default = label.clone())] title: String,
    #[props(default = 16)] icon_size: u32,
    #[props(default = IconMenuTone::Neutral)] tone: IconMenuTone,
    #[props(default = PopoverPlacement::BottomEnd)] placement: PopoverPlacement,
    #[props(default = false)] active: bool,
    #[props(default = IconMenuVisualState::Rest)] visual_state: IconMenuVisualState,
    #[props(default = false)] initially_open: bool,
    #[props(default = default_icon_menu_popup_class().to_string())] popup_class: String,
    children: Element,
) -> Element {
    let class = icon_menu_visual_class(tone, active, visual_state);
    let chrome_class = icon_menu_chrome_class(tone);

    rsx! {
        IconPopoverButton {
            class: class.to_string(),
            open_class: icon_menu_open_class(tone).to_string(),
            icon,
            icon_size,
            label,
            title,
            popup_class,
            chrome_class: chrome_class.to_string(),
            placement,
            initially_open,
            {children}
        }
    }
}

fn default_icon_menu_popup_class() -> &'static str {
    "tw:grid tw:w-[min(320px,calc(100vw-24px))] tw:gap-3 tw:rounded-md tw:border tw:border-border tw:bg-card tw:p-3 tw:text-sm tw:text-muted-foreground tw:shadow-lg"
}

fn icon_menu_chrome_class(tone: IconMenuTone) -> &'static str {
    match tone {
        IconMenuTone::Quiet => "ux-popover-chrome-quiet",
        IconMenuTone::Neutral => "ux-popover-chrome-neutral",
        IconMenuTone::Accent => "ux-popover-chrome-accent",
        IconMenuTone::Good => "ux-popover-chrome-good",
        IconMenuTone::Working => "ux-popover-chrome-working",
        IconMenuTone::Live => "ux-popover-chrome-live",
        IconMenuTone::Warning => "ux-popover-chrome-warning",
        IconMenuTone::Attention => "ux-popover-chrome-attention",
        IconMenuTone::Error => "ux-popover-chrome-error",
        IconMenuTone::Bound => "ux-popover-chrome-bound",
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IconMenuTone {
    Quiet,
    Neutral,
    Accent,
    Good,
    Working,
    /// Live-only (transient) edit state, blue.
    Live,
    /// Unsaved/edit state, yellow (node vocabulary).
    Warning,
    /// Health-attention state, orange (device/roster vocabulary).
    Attention,
    Error,
    /// Bound/bus-linked state, violet.
    Bound,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IconMenuVisualState {
    Rest,
    Hover,
    Open,
}

fn icon_menu_visual_class(
    tone: IconMenuTone,
    active: bool,
    state: IconMenuVisualState,
) -> &'static str {
    match state {
        IconMenuVisualState::Rest => icon_menu_class(tone, active),
        IconMenuVisualState::Hover => icon_menu_hover_class(tone, active),
        IconMenuVisualState::Open => icon_menu_open_class(tone),
    }
}

fn icon_menu_class(tone: IconMenuTone, active: bool) -> &'static str {
    match (tone, active) {
        (IconMenuTone::Quiet, false) => {
            "tw:inline-flex tw:h-8 tw:w-8 tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-border-subtle tw:bg-terminal tw:p-0 tw:text-muted-foreground tw:transition-colors tw:hover:border-border-strong tw:hover:text-strong-foreground"
        }
        (IconMenuTone::Quiet, true) => {
            "tw:inline-flex tw:h-8 tw:w-8 tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-border-subtle tw:bg-terminal tw:p-0 tw:text-muted-foreground tw:transition-colors tw:hover:border-border-strong tw:hover:text-strong-foreground"
        }
        (IconMenuTone::Neutral, false) => {
            "tw:inline-flex tw:h-8 tw:w-8 tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-border-subtle tw:bg-page tw:p-0 tw:text-subtle-foreground tw:hover:border-border-strong tw:hover:text-muted-foreground"
        }
        (IconMenuTone::Neutral, true) => {
            "tw:inline-flex tw:h-8 tw:w-8 tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-border-strong tw:bg-card-muted tw:p-0 tw:text-muted-foreground tw:transition-colors tw:hover:text-strong-foreground"
        }
        (IconMenuTone::Accent, false) => {
            "tw:inline-flex tw:h-8 tw:w-8 tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-border-subtle tw:bg-transparent tw:p-0 tw:text-subtle-foreground tw:transition-colors tw:hover:border-accent-border tw:hover:text-accent"
        }
        (IconMenuTone::Accent, true) => {
            "tw:inline-flex tw:h-8 tw:w-8 tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-accent-border tw:bg-transparent tw:p-0 tw:text-accent tw:transition-colors tw:hover:border-status-good-foreground tw:hover:text-status-good-foreground"
        }
        (IconMenuTone::Good, _) => {
            "tw:inline-flex tw:h-8 tw:w-8 tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-status-good-border tw:bg-status-good-bg tw:p-0 tw:text-status-good-foreground tw:transition-colors tw:hover:border-status-good-foreground"
        }
        (IconMenuTone::Working, _) => {
            "tw:inline-flex tw:h-8 tw:w-8 tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-status-working-border tw:bg-status-working-bg tw:p-0 tw:text-status-working-foreground tw:transition-colors tw:hover:border-status-working-foreground"
        }
        (IconMenuTone::Live, _) => {
            "tw:inline-flex tw:h-8 tw:w-8 tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-status-live-border tw:bg-status-live-bg tw:p-0 tw:text-status-live-foreground tw:transition-colors tw:hover:border-status-live-foreground"
        }
        (IconMenuTone::Warning, _) => {
            "tw:inline-flex tw:h-8 tw:w-8 tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-status-warning-border tw:bg-status-warning-bg tw:p-0 tw:text-status-warning-foreground tw:transition-colors tw:hover:border-status-warning-foreground"
        }
        (IconMenuTone::Attention, _) => {
            "tw:inline-flex tw:h-8 tw:w-8 tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-status-attention-border tw:bg-status-attention-bg tw:p-0 tw:text-status-attention-foreground tw:transition-colors tw:hover:border-status-attention-foreground"
        }
        (IconMenuTone::Error, _) => {
            "tw:inline-flex tw:h-8 tw:w-8 tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-status-error-border tw:bg-status-error-bg tw:p-0 tw:text-status-error-foreground tw:transition-colors tw:hover:border-status-error-foreground"
        }
        (IconMenuTone::Bound, _) => {
            "tw:inline-flex tw:h-8 tw:w-8 tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-status-bound-border tw:bg-status-bound-bg tw:p-0 tw:text-status-bound-foreground tw:transition-colors tw:hover:border-status-bound-foreground"
        }
    }
}

fn icon_menu_hover_class(tone: IconMenuTone, active: bool) -> &'static str {
    match (tone, active) {
        (IconMenuTone::Quiet, _) => {
            "tw:inline-flex tw:h-8 tw:w-8 tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-border-strong tw:bg-terminal tw:p-0 tw:text-strong-foreground tw:transition-colors"
        }
        (IconMenuTone::Neutral, false) => {
            "tw:inline-flex tw:h-8 tw:w-8 tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-border-strong tw:bg-page tw:p-0 tw:text-muted-foreground tw:transition-colors"
        }
        (IconMenuTone::Neutral, true) => {
            "tw:inline-flex tw:h-8 tw:w-8 tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-border-strong tw:bg-card-muted tw:p-0 tw:text-strong-foreground tw:transition-colors"
        }
        (IconMenuTone::Accent, false) => {
            "tw:inline-flex tw:h-8 tw:w-8 tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-accent-border tw:bg-transparent tw:p-0 tw:text-accent tw:transition-colors"
        }
        (IconMenuTone::Accent, true) => {
            "tw:inline-flex tw:h-8 tw:w-8 tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-status-good-foreground tw:bg-transparent tw:p-0 tw:text-status-good-foreground tw:transition-colors"
        }
        (IconMenuTone::Good, _) => {
            "tw:inline-flex tw:h-8 tw:w-8 tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-status-good-foreground tw:bg-status-good-bg tw:p-0 tw:text-status-good-foreground tw:transition-colors"
        }
        (IconMenuTone::Working, _) => {
            "tw:inline-flex tw:h-8 tw:w-8 tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-status-working-foreground tw:bg-status-working-bg tw:p-0 tw:text-status-working-foreground tw:transition-colors"
        }
        (IconMenuTone::Live, _) => {
            "tw:inline-flex tw:h-8 tw:w-8 tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-status-live-foreground tw:bg-status-live-bg tw:p-0 tw:text-status-live-foreground tw:transition-colors"
        }
        (IconMenuTone::Warning, _) => {
            "tw:inline-flex tw:h-8 tw:w-8 tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-status-warning-foreground tw:bg-status-warning-bg tw:p-0 tw:text-status-warning-foreground tw:transition-colors"
        }
        (IconMenuTone::Attention, _) => {
            "tw:inline-flex tw:h-8 tw:w-8 tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-status-attention-foreground tw:bg-status-attention-bg tw:p-0 tw:text-status-attention-foreground tw:transition-colors"
        }
        (IconMenuTone::Error, _) => {
            "tw:inline-flex tw:h-8 tw:w-8 tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-status-error-foreground tw:bg-status-error-bg tw:p-0 tw:text-status-error-foreground tw:transition-colors"
        }
        (IconMenuTone::Bound, _) => {
            "tw:inline-flex tw:h-8 tw:w-8 tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-status-bound-foreground tw:bg-status-bound-bg tw:p-0 tw:text-status-bound-foreground tw:transition-colors"
        }
    }
}

fn icon_menu_open_class(tone: IconMenuTone) -> &'static str {
    match tone {
        IconMenuTone::Quiet => {
            "tw:inline-flex tw:h-8 tw:w-8 tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-border-strong tw:bg-terminal tw:p-0 tw:text-strong-foreground"
        }
        IconMenuTone::Neutral => {
            "tw:inline-flex tw:h-8 tw:w-8 tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-border-strong tw:bg-card-subtle tw:p-0 tw:text-strong-foreground"
        }
        IconMenuTone::Accent => {
            "tw:inline-flex tw:h-8 tw:w-8 tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-accent-border tw:bg-transparent tw:p-0 tw:text-accent"
        }
        IconMenuTone::Good => {
            "tw:inline-flex tw:h-8 tw:w-8 tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-status-good-border tw:bg-status-good-bg tw:p-0 tw:text-status-good-foreground"
        }
        IconMenuTone::Working => {
            "tw:inline-flex tw:h-8 tw:w-8 tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-status-working-border tw:bg-status-working-bg tw:p-0 tw:text-status-working-foreground"
        }
        IconMenuTone::Live => {
            "tw:inline-flex tw:h-8 tw:w-8 tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-status-live-border tw:bg-status-live-bg tw:p-0 tw:text-status-live-foreground"
        }
        IconMenuTone::Warning => {
            "tw:inline-flex tw:h-8 tw:w-8 tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-status-warning-border tw:bg-status-warning-bg tw:p-0 tw:text-status-warning-foreground"
        }
        IconMenuTone::Attention => {
            "tw:inline-flex tw:h-8 tw:w-8 tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-status-attention-border tw:bg-status-attention-bg tw:p-0 tw:text-status-attention-foreground"
        }
        IconMenuTone::Error => {
            "tw:inline-flex tw:h-8 tw:w-8 tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-status-error-border tw:bg-status-error-bg tw:p-0 tw:text-status-error-foreground"
        }
        IconMenuTone::Bound => {
            "tw:inline-flex tw:h-8 tw:w-8 tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-status-bound-border tw:bg-status-bound-bg tw:p-0 tw:text-status-bound-foreground"
        }
    }
}
