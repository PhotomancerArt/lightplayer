//! The one small inline action button (the "gesture button" family, grown
//! up): every little icon/text action embedded in rows, faces, panels, and
//! drawers — add/remove entry, set/clear option, bind/unbind, revert —
//! renders through [`InlineButton`] so they all share one look: a fixed
//! `h-6` footprint, a **colored border** in the action's tone family, a
//! clear toned glyph, and the dark terminal background.
//!
//! Tone picks meaning, not decoration: ordinary available actions wear the
//! brand accent (teal); binding actions wear violet (the app-wide bound
//! convention); revert/discard-edit wears the warning amber. Hover fills
//! with the tone's wash/background so the button answers the pointer in its
//! own color. Disabled keeps the identical footprint on the muted surface
//! (rows stay anchored, the control just goes inert).

use dioxus::prelude::*;

use crate::base::{StudioIcon, StudioIconName};

/// Glyph size inside the fixed `h-6 w-6` icon-only shape.
pub const INLINE_ICON_SIZE: u32 = 14;
/// Glyph size beside text in the content-width shape.
pub const INLINE_TEXT_ICON_SIZE: u32 = 12;

/// Tone family for an inline button. The default is [`Self::Accent`]:
/// gestures are available actions, so they wear the brand accent rather
/// than a status family — status tones are opted into only where the action
/// belongs to that family's meaning.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum InlineButtonTone {
    /// Ordinary available actions: brand-accent (teal) border and glyph.
    #[default]
    Accent,
    /// Truly tone-free chrome (cancel/dismiss of a transient UI state).
    Neutral,
    Good,
    Working,
    Live,
    /// Unsaved/edit family (amber): revert/discard-edit actions.
    Warning,
    /// Health-attention family (orange, device/roster vocabulary).
    Attention,
    Error,
    /// Binding/bus family (violet) — bind/unbind/channel actions ONLY, per
    /// the app-wide bound-is-violet convention.
    Bound,
}

/// A small inline action button: fixed `h-6` height, colored tone border,
/// toned glyph and/or compact text, dark background. Always stops event
/// propagation — inline buttons live inside clickable rows/cards, and the
/// gesture must never double as the row's own click.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn InlineButton(
    /// Accessible label (`aria-label`); also the tooltip unless `title`
    /// overrides it.
    label: String,
    #[props(default = None)] icon: Option<StudioIconName>,
    /// Visible text; its presence switches from the fixed `w-6` square to
    /// the content-width shape.
    #[props(default = None)]
    text: Option<String>,
    #[props(default)] tone: InlineButtonTone,
    #[props(default = false)] disabled: bool,
    /// Tooltip override for a longer sentence than the label.
    #[props(default = None)]
    title: Option<String>,
    /// Glyph size override (defaults: [`INLINE_ICON_SIZE`] alone,
    /// [`INLINE_TEXT_ICON_SIZE`] beside text).
    #[props(default = None)]
    icon_size: Option<u32>,
    /// Extra layout-only classes appended to the shared look (e.g.
    /// `tw:ml-auto`) — never restyling.
    #[props(default = None)]
    class: Option<String>,
    on_press: EventHandler<()>,
) -> Element {
    let with_text = text.is_some();
    let size = icon_size.unwrap_or(if with_text {
        INLINE_TEXT_ICON_SIZE
    } else {
        INLINE_ICON_SIZE
    });
    let mut button_class = if with_text {
        inline_text_button_class(tone, disabled)
    } else {
        inline_icon_button_class(tone, disabled)
    };
    if let Some(extra) = class {
        button_class = format!("{button_class} {extra}");
    }
    let title = title.unwrap_or_else(|| label.clone());

    rsx! {
        button {
            class: button_class,
            r#type: "button",
            disabled,
            aria_label: "{label}",
            title: "{title}",
            onclick: move |event| {
                event.stop_propagation();
                if !disabled {
                    on_press.call(());
                }
            },
            if let Some(icon) = icon {
                StudioIcon { name: icon, size }
            }
            if let Some(text) = text.as_deref() {
                span { "{text}" }
            }
        }
    }
}

/// The icon-only shape: one fixed `h-6 w-6` square, so a button
/// appearing/disappearing or swapping state never shifts its row's anchors.
pub fn inline_icon_button_class(tone: InlineButtonTone, disabled: bool) -> String {
    compose(
        "tw:inline-flex tw:h-6 tw:w-6 tw:flex-none tw:appearance-none tw:items-center tw:justify-center tw:rounded-xs tw:border tw:p-0",
        tone,
        disabled,
    )
}

/// The text shape: same height, radius, border, and tone; only the width is
/// content-sized.
pub fn inline_text_button_class(tone: InlineButtonTone, disabled: bool) -> String {
    compose(
        "tw:inline-flex tw:h-6 tw:flex-none tw:appearance-none tw:items-center tw:justify-center tw:gap-1 tw:rounded-xs tw:border tw:px-1.5 tw:text-xs tw:font-medium",
        tone,
        disabled,
    )
}

fn compose(base: &str, tone: InlineButtonTone, disabled: bool) -> String {
    if disabled {
        // Identical footprint on the muted surface: inert, still anchored.
        format!("{base} tw:border-border-muted tw:bg-card-muted tw:text-subtle-foreground")
    } else {
        format!(
            "{base} tw:cursor-pointer tw:transition-colors tw:bg-terminal {}",
            tone_class(tone)
        )
    }
}

/// Tone fragment: colored border + toned glyph at rest, tone-filled on
/// hover. Every tone shares the dark terminal background at rest — actions
/// are dark chips with a colored edge; filled surfaces stay the language of
/// state, not gestures.
fn tone_class(tone: InlineButtonTone) -> &'static str {
    match tone {
        InlineButtonTone::Accent => {
            "tw:border-accent-border tw:text-accent tw:hover:border-accent tw:hover:bg-accent-wash"
        }
        InlineButtonTone::Neutral => {
            "tw:border-border-strong tw:text-muted-foreground tw:hover:border-selection-border tw:hover:text-strong-foreground"
        }
        InlineButtonTone::Good => {
            "tw:border-status-good-border tw:text-status-good-foreground tw:hover:border-status-good-foreground tw:hover:bg-status-good-bg"
        }
        InlineButtonTone::Working => {
            "tw:border-status-working-border tw:text-status-working-foreground tw:hover:border-status-working-foreground tw:hover:bg-status-working-bg"
        }
        InlineButtonTone::Live => {
            "tw:border-status-live-border tw:text-status-live-foreground tw:hover:border-status-live-foreground tw:hover:bg-status-live-bg"
        }
        InlineButtonTone::Warning => {
            "tw:border-status-warning-border tw:text-status-warning-foreground tw:hover:border-status-warning-foreground tw:hover:bg-status-warning-bg"
        }
        InlineButtonTone::Attention => {
            "tw:border-status-attention-border tw:text-status-attention-foreground tw:hover:border-status-attention-foreground tw:hover:bg-status-attention-bg"
        }
        InlineButtonTone::Error => {
            "tw:border-status-error-border tw:text-status-error-foreground tw:hover:border-status-error-foreground tw:hover:bg-status-error-bg"
        }
        InlineButtonTone::Bound => {
            "tw:border-status-bound-border tw:text-status-bound-foreground tw:hover:border-status-bound-foreground tw:hover:bg-status-bound-bg"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_TONES: [InlineButtonTone; 9] = [
        InlineButtonTone::Accent,
        InlineButtonTone::Neutral,
        InlineButtonTone::Good,
        InlineButtonTone::Working,
        InlineButtonTone::Live,
        InlineButtonTone::Warning,
        InlineButtonTone::Attention,
        InlineButtonTone::Error,
        InlineButtonTone::Bound,
    ];

    #[test]
    fn every_state_holds_the_fixed_footprint() {
        // Buttons appear/disappear and swap disabled state in place; the
        // footprint tokens must be identical in every tone × state so row
        // anchors never move.
        for tone in ALL_TONES {
            for disabled in [false, true] {
                let icon = inline_icon_button_class(tone, disabled);
                for token in [
                    "tw:h-6",
                    "tw:w-6",
                    "tw:flex-none",
                    "tw:rounded-xs",
                    "tw:border",
                ] {
                    assert!(icon.contains(token), "{token} missing: {icon}");
                }
                let text = inline_text_button_class(tone, disabled);
                for token in ["tw:h-6", "tw:flex-none", "tw:rounded-xs", "tw:border"] {
                    assert!(text.contains(token), "{token} missing: {text}");
                }
            }
        }
    }

    #[test]
    fn enabled_buttons_wear_a_colored_border_on_the_dark_background() {
        // The family's look: dark terminal background + a tone border that
        // is never the drab grey `border-border-subtle` rest tone the old
        // per-file forks used (Neutral opts into the strong grey border,
        // still brighter than the old subtle one).
        for tone in ALL_TONES {
            let class = inline_icon_button_class(tone, false);
            assert!(class.contains("tw:bg-terminal"), "{class}");
            assert!(class.contains("tw:cursor-pointer"), "{class}");
            assert!(!class.contains("tw:border-border-subtle"), "{class}");
        }
    }

    #[test]
    fn disabled_keeps_the_footprint_and_drops_the_affordance() {
        let class = inline_icon_button_class(InlineButtonTone::Accent, true);
        assert!(!class.contains("tw:cursor-pointer"), "{class}");
        assert!(!class.contains("tw:hover:"), "{class}");
        assert!(class.contains("tw:bg-card-muted"), "{class}");
    }

    #[test]
    fn violet_stays_the_binding_convention() {
        // Bound violet is the app-wide binding signal: no other tone may
        // borrow it, and the default tone is Accent — an ordinary gesture
        // never accidentally reads as bound.
        assert_eq!(InlineButtonTone::default(), InlineButtonTone::Accent);
        for tone in ALL_TONES {
            let class = inline_icon_button_class(tone, false);
            assert_eq!(
                class.contains("status-bound"),
                tone == InlineButtonTone::Bound,
                "{class}"
            );
        }
    }
}
