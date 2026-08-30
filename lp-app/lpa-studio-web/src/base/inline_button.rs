//! The one small inline action button (the "gesture button" family, grown
//! up): every little icon/text action embedded in rows, faces, panels, and
//! drawers — add/remove entry, set/clear option, bind/unbind, revert —
//! renders through [`InlineButton`] so they all share one look: a fixed
//! `h-6` footprint, a **colored border** in the action's tone family, a
//! clear toned glyph, and the dark terminal background.
//!
//! Tone picks meaning, not decoration: ordinary available actions are
//! bright neutral (the accent reckoning, D1 2026-08-30 — chrome holds no
//! hue at rest); binding actions wear violet (the app-wide bound
//! convention); revert/discard-edit wears the warning amber. Status tones
//! answer the pointer by filling with their own wash/background. Disabled
//! keeps the identical footprint on the muted surface (rows stay anchored,
//! the control just goes inert).
//!
//! Aurora R2 (2026-08-29) adds one split on top of that: the two
//! decoration-free tones (Action, Neutral) also take the iridescent hover
//! ring, and every enabled tone takes the app-wide focus ring. Status tones
//! deliberately do NOT take the ring — see [`tone_takes_the_ring`].

use dioxus::prelude::*;

use crate::base::interaction_light::{focus_ring_class, ir_ring_class};
use crate::base::{StudioIcon, StudioIconName};

/// Glyph size inside the fixed `h-6 w-6` icon-only shape.
pub const INLINE_ICON_SIZE: u32 = 14;
/// Glyph size beside text in the content-width shape.
pub const INLINE_TEXT_ICON_SIZE: u32 = 12;

/// Tone family for an inline button. The default is [`Self::Action`]:
/// gestures are available actions, so they wear the bright-neutral action
/// tone rather than a status family — status tones are opted into only
/// where the action belongs to that family's meaning.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum InlineButtonTone {
    /// Ordinary available actions: bright neutral — one visible lightness
    /// step above [`Self::Neutral`] (border-strong + strong glyph). The
    /// difference between the two hue-less tones is lightness, never hue.
    #[default]
    Action,
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
        let mut class = format!(
            "{base} tw:cursor-pointer tw:transition-colors tw:bg-terminal {} {}",
            focus_ring_class(),
            tone_class(tone)
        );
        if tone_takes_the_ring(tone) {
            class.push(' ');
            class.push_str(ir_ring_class());
        }
        class
    }
}

/// Whether a tone answers the pointer with the iridescent ring or with its
/// own hue. Only the two decoration-free tones take the ring: a STATUS tone
/// means something, and "status never relies on color alone" also means a
/// status control must not flash rainbow on hover. The ring is an
/// absolutely-positioned pseudo-element, so this choice never moves a
/// button's footprint either way.
fn tone_takes_the_ring(tone: InlineButtonTone) -> bool {
    matches!(tone, InlineButtonTone::Action | InlineButtonTone::Neutral)
}

/// Tone fragment: colored border + toned glyph at rest, tone-filled on
/// hover. Every tone shares the dark terminal background at rest — actions
/// are dark chips with a colored edge; filled surfaces stay the language of
/// state, not gestures.
fn tone_class(tone: InlineButtonTone) -> &'static str {
    match tone {
        InlineButtonTone::Action => {
            "tw:border-border-strong tw:text-strong-foreground tw:hover:border-selection-border tw:hover:bg-card-raised"
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
        InlineButtonTone::Action,
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
        let class = inline_icon_button_class(InlineButtonTone::Action, true);
        assert!(!class.contains("tw:cursor-pointer"), "{class}");
        assert!(!class.contains("tw:hover:"), "{class}");
        assert!(class.contains("tw:bg-card-muted"), "{class}");
    }

    #[test]
    fn only_the_decoration_free_tones_take_the_iridescent_ring() {
        // A status tone means something; the spectrum ring is decoration.
        // Letting Error or Bound flash rainbow on hover would put a second,
        // meaningless color on a control whose color IS the message.
        for tone in ALL_TONES {
            let class = inline_icon_button_class(tone, false);
            let expected = matches!(tone, InlineButtonTone::Action | InlineButtonTone::Neutral);
            assert_eq!(class.contains("ux-ir-ring"), expected, "{class}");
        }
    }

    #[test]
    fn enabled_buttons_are_keyboard_visible_and_disabled_ones_are_inert() {
        for tone in ALL_TONES {
            assert!(
                inline_icon_button_class(tone, false).contains("ux-focus-ring"),
                "{tone:?}"
            );
        }
        // Disabled buttons are not focusable and take no interaction light.
        let class = inline_icon_button_class(InlineButtonTone::Action, true);
        assert!(!class.contains("ux-ir-ring"), "{class}");
        assert!(!class.contains("ux-focus-ring"), "{class}");
    }

    #[test]
    fn violet_stays_the_binding_convention() {
        // Bound violet is the app-wide binding signal: no other tone may
        // borrow it, and the default tone is Action — an ordinary gesture
        // never accidentally reads as bound.
        assert_eq!(InlineButtonTone::default(), InlineButtonTone::Action);
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
