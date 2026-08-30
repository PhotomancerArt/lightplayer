use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;
use lpa_studio_core::{ActionEnablement, ActionPriority, UiAction};

use crate::base::{StudioIcon, action_icon_name};

/// How long a two-click confirmation stays ARMED before it stands down on
/// its own. Long enough to read the changed label, short enough that a
/// forgotten armed button cannot ambush a later stray click.
const ARMED_CONFIRM_WINDOW_MS: u32 = 4_000;

/// How an action renders in its surrounding context. One action model
/// (label / icon / priority / destructive / confirmation from
/// [`ActionMeta`](lpa_studio_core::ActionMeta)), several visual homes.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ActionButtonVariant {
    /// The standing action-strip button (priority-tiered chrome).
    #[default]
    Solid,
    /// A compact bordered chip for section headers and toolbars.
    Quiet,
    /// The neutral outline CTA (the "Connect/Save/Add" family): strong
    /// border and text, the spectrum ring answering hover — for a card's
    /// standing call-to-action where the Primary gradient reads too loud.
    Outline,
    /// A full-width left-aligned row inside a menu popup.
    MenuItem,
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ActionButton(
    action: UiAction,
    running: bool,
    #[props(default)] variant: ActionButtonVariant,
    on_action: EventHandler<UiAction>,
) -> Element {
    let action_to_run = action.clone();
    let meta = action.meta().clone();
    let disabled = running || !meta.enablement.is_enabled();
    let class = action_class(variant, meta.priority, meta.destructive);
    let disabled_reason = disabled_reason(&meta.enablement).map(ToString::to_string);
    let icon = action_icon_name(meta.icon.as_deref());
    let confirmation = meta.confirmation.clone();
    let label = meta.label;
    let summary = meta.summary;
    let icon_px = match variant {
        ActionButtonVariant::Solid => 15,
        ActionButtonVariant::Quiet
        | ActionButtonVariant::Outline
        | ActionButtonVariant::MenuItem => 14,
    };

    // The two-click confirmation: the first click ARMS the button (the
    // button itself asks, wearing the confirm label), the second click
    // dispatches. Arming stands down on blur or after a short window —
    // the native dialog stays for confirmations not marked inline.
    let inline_confirm = confirmation.as_ref().is_some_and(|c| c.inline);
    let mut armed = use_signal(|| false);
    let mut arm_generation = use_signal(|| 0u64);
    let armed_label = confirmation
        .as_ref()
        .map(|c| format!("{}?", c.confirm_label))
        .unwrap_or_default();
    let armed_title = confirmation
        .as_ref()
        .map(|c| c.message.clone())
        .unwrap_or_default();
    let shown_label = if inline_confirm && armed() {
        armed_label
    } else {
        label
    };
    let shown_title = if inline_confirm && armed() {
        armed_title
    } else {
        summary
    };
    let shown_class = if inline_confirm && armed() {
        format!("{class} tw:bg-status-error-bg tw:border-status-error-border")
    } else {
        class.to_string()
    };

    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:gap-1",
            button {
                class: shown_class,
                r#type: "button",
                disabled,
                title: "{shown_title}",
                onblur: move |_| {
                    if inline_confirm {
                        armed.set(false);
                    }
                },
                onclick: move |_| {
                    if inline_confirm {
                        if armed() {
                            armed.set(false);
                            on_action.call(action_to_run.clone());
                        } else {
                            armed.set(true);
                            let generation = arm_generation() + 1;
                            arm_generation.set(generation);
                            spawn(async move {
                                TimeoutFuture::new(ARMED_CONFIRM_WINDOW_MS).await;
                                if arm_generation() == generation {
                                    armed.set(false);
                                }
                            });
                        }
                    } else if confirmation_confirmed(confirmation.as_ref()) {
                        on_action.call(action_to_run.clone());
                    }
                },
                if let Some(icon) = icon {
                    span { class: "tw:inline-flex tw:h-[15px] tw:w-[15px] tw:items-center tw:justify-center", aria_hidden: "true",
                        StudioIcon {
                            name: icon,
                            size: icon_px,
                        }
                    }
                }
                span { "{shown_label}" }
            }
            if let Some(reason) = disabled_reason.as_ref() {
                p { class: "tw:m-0 tw:text-xs tw:leading-snug tw:text-dim-foreground", "{reason}" }
            }
        }
    }
}

/// Run an action's optional [`ActionConfirmation`](lpa_studio_core::ActionConfirmation)
/// through the native confirm dialog. Shared by every generic action
/// renderer ([`ActionButton`], the pane header's action buttons) so
/// confirmation semantics never fork per surface.
pub(crate) fn confirmation_confirmed(
    confirmation: Option<&lpa_studio_core::ActionConfirmation>,
) -> bool {
    let Some(confirmation) = confirmation else {
        return true;
    };
    let message = format!("{}\n\n{}", confirmation.title, confirmation.message);
    web_sys::window()
        .and_then(|window| window.confirm_with_message(&message).ok())
        .unwrap_or(false)
}

fn action_class(
    variant: ActionButtonVariant,
    priority: ActionPriority,
    destructive: bool,
) -> &'static str {
    match variant {
        ActionButtonVariant::Solid => solid_class(priority),
        ActionButtonVariant::Quiet => quiet_class(destructive),
        ActionButtonVariant::Outline => outline_action_class(destructive),
        ActionButtonVariant::MenuItem => menu_item_class(destructive),
    }
}

fn solid_class(priority: ActionPriority) -> &'static str {
    match priority {
        // Devices-treatments spike gate (2026-08-31, "1F for the primary
        // for now"): the gradient FILL stands down — rainbow-bg with dark
        // text didn't work — and Primary is the standing spectrum OUTLINE.
        // `.ux-spectrum-cta` (style.css) carries ring, text and hover glow
        // together, and owns its own ring pseudo — so no `ux-ir-ring` here:
        // composing the two would fight over `::before`.
        ActionPriority::Primary => {
            concat!(
                "tw:inline-flex tw:min-h-9 tw:max-w-full tw:items-center tw:justify-center tw:gap-2 tw:rounded-sm tw:border tw:px-3 tw:text-sm tw:font-bold tw:leading-none tw:break-words tw:disabled:cursor-not-allowed tw:disabled:opacity-60",
                " ux-spectrum-cta ux-focus-ring ux-press-flare"
            )
        }
        ActionPriority::Secondary => {
            concat!(
                "tw:inline-flex tw:min-h-9 tw:max-w-full tw:items-center tw:justify-center tw:gap-2 tw:rounded-sm tw:border tw:px-3 tw:text-sm tw:font-bold tw:leading-none tw:break-words tw:disabled:cursor-not-allowed tw:disabled:opacity-60",
                " tw:border-border-strong tw:bg-card-raised tw:text-soft-foreground tw:hover:bg-card-raised-strong",
                " ux-ir-ring ux-focus-ring ux-press-flare"
            )
        }
        // Tertiary is the quiet tier: focus ring and press flare, no ring —
        // a transparent chip that grows a rainbow edge reads louder than
        // the Secondary next to it.
        ActionPriority::Tertiary => {
            concat!(
                "tw:inline-flex tw:min-h-9 tw:max-w-full tw:items-center tw:justify-center tw:gap-2 tw:rounded-sm tw:border tw:px-3 tw:text-sm tw:font-bold tw:leading-none tw:break-words tw:disabled:cursor-not-allowed tw:disabled:opacity-60",
                " tw:border-border-strong tw:bg-transparent tw:text-muted-foreground tw:hover:bg-card-muted",
                " ux-focus-ring ux-press-flare"
            )
        }
    }
}

/// The compact toolbar chip. All priorities share one quiet look — the
/// header is not a hierarchy; destructive still wears the error tint.
/// Shared with non-action toolbar controls (e.g. the import file-input
/// label) via [`quiet_action_class`].
fn quiet_class(destructive: bool) -> &'static str {
    if destructive {
        "tw:inline-flex tw:cursor-pointer tw:items-center tw:gap-1.5 tw:rounded tw:border tw:border-border tw:bg-transparent tw:px-2.5 tw:py-1 tw:text-xs tw:font-semibold tw:text-status-error-foreground tw:transition-colors tw:hover:border-status-error-border tw:disabled:cursor-not-allowed tw:disabled:opacity-60 ux-focus-ring"
    } else {
        "tw:inline-flex tw:cursor-pointer tw:items-center tw:gap-1.5 tw:rounded tw:border tw:border-border tw:bg-transparent tw:px-2.5 tw:py-1 tw:text-xs tw:font-semibold tw:text-muted-foreground tw:transition-colors tw:hover:border-border-strong tw:hover:text-strong-foreground tw:disabled:cursor-not-allowed tw:disabled:opacity-60 ux-focus-ring"
    }
}

/// One row of a menu popup. Shared with non-action rows (e.g. web-side
/// export) via [`menu_item_action_class`]. Tailwind preflight is not
/// loaded, so the row must reset the UA button chrome (gray fill, 3D
/// border) itself — the rest is text plus a hover wash.
fn menu_item_class(destructive: bool) -> &'static str {
    if destructive {
        "tw:flex tw:w-full tw:cursor-pointer tw:appearance-none tw:items-center tw:gap-2 tw:rounded tw:border-none tw:bg-transparent tw:px-2 tw:py-1.5 tw:text-left tw:text-sm tw:text-status-error-foreground tw:transition-colors tw:hover:bg-status-error-bg tw:disabled:cursor-not-allowed tw:disabled:opacity-60 ux-focus-ring"
    } else {
        "tw:flex tw:w-full tw:cursor-pointer tw:appearance-none tw:items-center tw:gap-2 tw:rounded tw:border-none tw:bg-transparent tw:px-2 tw:py-1.5 tw:text-left tw:text-sm tw:text-muted-foreground tw:transition-colors tw:hover:bg-white/5 tw:hover:text-strong-foreground tw:disabled:cursor-not-allowed tw:disabled:opacity-60 ux-focus-ring"
    }
}

/// The quiet-chip classes, for toolbar controls that cannot be `UiAction`s
/// (file-input labels) but must read identically.
pub fn quiet_action_class() -> &'static str {
    quiet_class(false)
}

/// The quiet-chip classes' danger tone, for toolbar/sheet controls that
/// cannot be `UiAction`s but must wear the refusal treatment (P4
/// consolidation: `CardSheetButton`'s destructive tone).
pub fn quiet_destructive_action_class() -> &'static str {
    quiet_class(true)
}

/// The menu-row classes, for popup rows that cannot be `UiAction`s
/// (web-side handlers like export) but must read identically.
pub fn menu_item_action_class() -> &'static str {
    menu_item_class(false)
}

/// The destructive menu-row classes, for popup rows that cannot be
/// `UiAction`s but must wear the danger treatment (P3 rich-object
/// codification: danger-zone rows without an action model).
pub fn menu_item_destructive_action_class() -> &'static str {
    menu_item_class(true)
}

/// The solid-tier classes (Primary/Secondary/Tertiary), for standing CTA
/// buttons that cannot be `UiAction`s but should wear the exact action-strip
/// look — including the interaction light (P4 consolidation: banner/CTA
/// buttons that used to hand-roll a close approximation of this tier).
pub fn solid_action_class(priority: ActionPriority) -> &'static str {
    solid_class(priority)
}

/// A quiet outline CTA: transparent fill, neutral strong border and text,
/// the iridescent ring answering hover — the "Connect"/"Save"/"Add" family
/// that recurred as near-identical hand-rolled strings across the settings
/// popover, account page, agent chat, and share panel (P4 consolidation).
/// Neutral at rest per the accent reckoning (D1, 2026-08-30): chrome holds
/// no hue; the spectrum ring is what says "this answers your pointer".
/// Smaller than the Solid tier's `min-h-9`, for compact rows and popovers
/// that cannot carry a full action-strip button. `destructive` swaps to the
/// refusal tone (e.g. "Stop").
pub fn outline_action_class(destructive: bool) -> &'static str {
    if destructive {
        "tw:cursor-pointer tw:rounded-xs tw:border tw:border-status-error-border tw:bg-transparent tw:px-3 tw:py-1.5 tw:text-xs tw:font-bold tw:text-status-error-foreground tw:transition-colors tw:hover:bg-status-error-bg tw:disabled:cursor-not-allowed tw:disabled:opacity-60 ux-focus-ring"
    } else {
        "tw:cursor-pointer tw:rounded-xs tw:border tw:border-border-strong tw:bg-transparent tw:px-3 tw:py-1.5 tw:text-xs tw:font-bold tw:text-strong-foreground tw:transition tw:duration-300 tw:hover:border-selection-border tw:disabled:cursor-not-allowed tw:disabled:opacity-60 ux-focus-ring ux-ir-ring"
    }
}

/// A borderless, full-width in-row link button: icon + label, no chip, a
/// text-color shift on hover. The share/node-detail "Copy JSON" and
/// project-share rows duplicated this string byte-for-byte (P4
/// consolidation) — `disabled` swaps to the inert dim reading used where
/// the row explains itself through its own title rather than the native
/// attribute.
pub fn inline_link_row_class(disabled: bool) -> &'static str {
    if disabled {
        "tw:flex tw:w-full tw:min-w-0 tw:cursor-not-allowed tw:items-center tw:gap-2 tw:rounded-xs tw:border-0 tw:bg-transparent tw:px-0 tw:py-0.5 tw:text-left tw:text-xs tw:text-subtle-foreground tw:opacity-60"
    } else {
        "tw:flex tw:w-full tw:min-w-0 tw:cursor-pointer tw:items-center tw:gap-2 tw:rounded-xs tw:border-0 tw:bg-transparent tw:px-0 tw:py-0.5 tw:text-left tw:text-xs tw:text-muted-foreground tw:transition-colors tw:hover:text-strong-foreground ux-focus-ring"
    }
}

fn disabled_reason(enablement: &ActionEnablement) -> Option<&str> {
    match enablement {
        ActionEnablement::Enabled => None,
        ActionEnablement::Disabled { reason } => Some(reason.as_str()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PRIORITIES: [ActionPriority; 3] = [
        ActionPriority::Primary,
        ActionPriority::Secondary,
        ActionPriority::Tertiary,
    ];

    #[test]
    fn every_solid_tier_keeps_the_same_geometry() {
        // The interaction light is pseudo-elements and outlines only: a
        // tier swap must never resize a button, so the geometry tokens are
        // identical across tiers.
        for priority in PRIORITIES {
            let class = solid_class(priority);
            for token in [
                "tw:min-h-9",
                "tw:rounded-sm",
                "tw:border",
                "tw:px-3",
                "tw:text-sm",
            ] {
                assert!(class.contains(token), "{token} missing: {class}");
            }
        }
    }

    #[test]
    fn the_ring_stops_at_the_quiet_tiers() {
        // Secondary takes the hover ring; Primary owns a STANDING ring of
        // its own (`ux-spectrum-cta` — self-contained, so composing
        // `ux-ir-ring` on top would fight over `::before`). Transparent
        // chips and menu rows keep their own wash (a rainbow edge on a
        // menu row is noise, and the destructive rows must stay
        // unmistakably red).
        assert!(!solid_class(ActionPriority::Primary).contains("ux-ir-ring"));
        assert!(solid_class(ActionPriority::Secondary).contains("ux-ir-ring"));
        for class in [
            solid_class(ActionPriority::Tertiary),
            quiet_class(false),
            quiet_class(true),
            menu_item_class(false),
            menu_item_class(true),
        ] {
            assert!(!class.contains("ux-ir-ring"), "{class}");
        }
    }

    #[test]
    fn every_action_button_is_keyboard_visible() {
        for class in PRIORITIES.map(solid_class) {
            assert!(class.contains("ux-focus-ring"), "{class}");
        }
        for class in [
            quiet_class(false),
            quiet_class(true),
            menu_item_class(false),
            menu_item_class(true),
        ] {
            assert!(class.contains("ux-focus-ring"), "{class}");
        }
    }

    #[test]
    fn the_primary_voice_is_the_spectrum_outline() {
        // Devices-treatments spike gate (2026-08-31, "1F for the primary
        // for now"): the standing spectrum ring succeeded the gradient
        // fill, and the class is self-contained — never composed with the
        // hover ring, never an accent fill.
        let class = solid_class(ActionPriority::Primary);
        assert!(class.contains("ux-spectrum-cta"), "{class}");
        assert!(!class.contains("ux-primary-gradient"), "{class}");
        assert!(!class.contains("accent"), "{class}");
    }

    #[test]
    fn the_outline_cta_is_neutral_with_the_ring() {
        // Accent reckoning D1 (2026-08-30): no hue on resting chrome. The
        // outline CTA is a neutral chip whose interaction answer is the
        // spectrum ring, not a colored wash; destructive keeps its full
        // semantic red and, like every status tone, refuses the ring.
        let plain = outline_action_class(false);
        assert!(!plain.contains("accent"), "{plain}");
        assert!(plain.contains("ux-ir-ring"), "{plain}");
        assert!(plain.contains("ux-focus-ring"), "{plain}");
        let destructive = outline_action_class(true);
        assert!(destructive.contains("status-error"), "{destructive}");
        assert!(!destructive.contains("ux-ir-ring"), "{destructive}");
    }
}
