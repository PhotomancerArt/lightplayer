use dioxus::prelude::*;
use lpa_studio_core::{ActionConsequence, ActionEnablement, ActionPriority, UiAction};

use super::armed_confirm_button::use_armed_confirm;
use crate::base::{StudioIcon, action_icon_name};

/// How an action renders in its surrounding context. One action model
/// (label / icon / priority / consequence from
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
    /// A flush section at a row's end (the board card's bar action): no
    /// chip, the row's full height, a hairline on its leading edge, a wash
    /// on hover. The row is one fixed line, so a refusal is the button's
    /// title (and its hidden text), never a line under it.
    RowEnd,
    /// The row's one primary as a flush section (the board card's name
    /// bar, `docs/style/ui.md` "The board card"): [`Self::RowEnd`]'s
    /// geometry, bolder, and the iridescent ring answering hover.
    RowPrimary,
}

impl ActionButtonVariant {
    /// A flush section of a fixed-height row: it fills the row and says a
    /// refusal in its title rather than under itself.
    pub fn is_row_section(self) -> bool {
        matches!(self, Self::RowEnd | Self::RowPrimary)
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ActionButton(
    action: UiAction,
    running: bool,
    #[props(default)] variant: ActionButtonVariant,
    /// Story-only: start with the two-click confirmation already ARMED, so
    /// captures can show the armed dress (and the card's `:has()` marking)
    /// deterministically. Real surfaces never set this.
    #[props(default)]
    armed_preview: bool,
    /// The disabled reason is said once elsewhere on the row (a row of
    /// verbs disabled for one cause), so this button does not repeat it.
    #[props(default)]
    reason_said_elsewhere: bool,
    /// The surface this button sits in IS the question its press answers:
    /// a sheet whose title and body ask, in core's words, what a Lasting
    /// press would otherwise ask on its own button (the layout sheet's
    /// Continue — G1 walk, 2026-10-03, Yona: "they already committed to it
    /// once"). The press acts at once instead of arming. Nothing else
    /// moves: the button keeps its level's tint, and the level itself is
    /// untouched, so the app agent still hands the offer to the user
    /// (`ActionMeta::needs_user`) and every other place that draws it — the
    /// palette, a chat card — still arms it.
    #[props(default)]
    asked_by_surface: bool,
    on_action: EventHandler<UiAction>,
) -> Element {
    let action_to_run = action.clone();
    let meta = action.meta().clone();
    let disabled = running || !meta.enablement.is_enabled();
    // One look per consequence level, in every variant (D7, Q7): Routine is
    // plain, Undoable wears the error tint, Lasting wears it and arms.
    let class = action_class(variant, meta.priority, meta.consequence.wears_error_tint());
    let row_section = variant.is_row_section();
    let refusal = disabled_reason(&meta.enablement)
        .filter(|reason| !reason.is_empty())
        .map(ToString::to_string);
    // A flush row section has no line under it: its refusal is its title
    // and its hidden text. Every other look says it under the button.
    let disabled_reason = refusal
        .clone()
        .filter(|_| !reason_said_elsewhere && !row_section);
    let row_refusal = refusal.filter(|_| row_section);
    let icon = action_icon_name(meta.icon.as_deref());
    let arms = arms_on_press(&meta.consequence, asked_by_surface);
    let copy = meta.consequence.copy().cloned();
    let label = meta.label;
    let summary = meta.summary;
    let icon_px = match variant {
        ActionButtonVariant::Solid => 15,
        ActionButtonVariant::Quiet
        | ActionButtonVariant::Outline
        | ActionButtonVariant::MenuItem => 14,
        ActionButtonVariant::RowEnd => 11,
        ActionButtonVariant::RowPrimary => 12,
    };
    let icon_box = match row_section {
        true => "tw:inline-flex tw:flex-none tw:items-center tw:justify-center",
        false => "tw:inline-flex tw:h-[15px] tw:w-[15px] tw:items-center tw:justify-center",
    };
    let wrapper_class = match row_section {
        true => ROW_SECTION_WRAPPER_CLASS,
        false => "tw:grid tw:min-w-0 tw:gap-1",
    };

    // The two-click arm of a Lasting action: the first click ARMS the
    // button (the button itself asks, wearing "Confirm ⟨verb⟩" — the 2K+
    // reading from the devices-treatments spike gate), the second click
    // dispatches. Arming stands down on blur or after a short window.
    // There is no dialog at any level. The armed dress (reserved width,
    // ramp, knock, quiet drain) lives in `.ux-armed-chip`/`.ux-armed`
    // (style.css); the owning card marks itself via
    // `.ux-armed-scope:has(.ux-armed)`, so no armed state leaves this
    // component.
    let mut confirm = use_armed_confirm(armed_preview);
    let armed = confirm.is_armed();
    let armed_title = copy.as_ref().map(|c| c.message.clone()).unwrap_or_default();
    let (rest_label, armed_label) =
        confirm_chip_labels(&label, copy.as_ref().map(|c| c.confirm_label.as_str()));
    let shown_title = match (&row_refusal, arms && armed) {
        (_, true) => armed_title,
        (Some(reason), false) => reason.clone(),
        (None, false) => summary,
    };
    let shown_class = if arms {
        confirm_chip_class(class, armed)
    } else {
        class.to_string()
    };

    rsx! {
        div { class: wrapper_class,
            button {
                class: shown_class,
                r#type: "button",
                disabled,
                title: "{shown_title}",
                onblur: move |_| {
                    if arms {
                        confirm.disarm();
                    }
                },
                onclick: move |_| {
                    if !arms || confirm.tap() {
                        on_action.call(action_to_run.clone());
                    }
                },
                if let Some(icon) = icon {
                    span { class: icon_box, aria_hidden: "true",
                        StudioIcon {
                            name: icon,
                            size: icon_px,
                        }
                    }
                }
                // RESERVE (device-card-v2 spike §2, gate 2026-09-02): both
                // labels live in ONE grid cell, so the chip is already as
                // wide as its armed reading and arming cannot move it or
                // its neighbours. The armed label is hidden from AT — the
                // armed `title` carries the confirmation message.
                if arms {
                    span { class: armed_labels_class(variant),
                        span { class: "ux-armed-label-rest", "{rest_label}" }
                        span { class: "ux-armed-label-armed", aria_hidden: "true", "{armed_label}" }
                    }
                } else {
                    span { class: "tw:inline-flex", "{rest_label}" }
                }
            }
            // An empty reason is a verb whose reason is said once beside it
            // (a row of verbs disabled for the same cause).
            if let Some(reason) = disabled_reason.as_ref() {
                p { class: "tw:m-0 tw:text-xs tw:leading-snug tw:text-dim-foreground", "{reason}" }
            }
            // A row section's refusal, for a screen reader (its title says
            // it to a pointer).
            if let Some(reason) = row_refusal {
                span { class: "tw:sr-only", "{reason}" }
            }
        }
    }
}

/// The two stacked labels' classes: a menu row's sit at its leading edge
/// (`ux-armed-labels-start`, style.css), as its text does; every other look
/// centres them in the reserved width.
fn armed_labels_class(variant: ActionButtonVariant) -> &'static str {
    match variant {
        ActionButtonVariant::MenuItem => "ux-armed-labels ux-armed-labels-start",
        _ => "ux-armed-labels",
    }
}

/// A row section's own wrapper: it fills the row's height and never
/// shrinks, so the section is flush top to bottom.
const ROW_SECTION_WRAPPER_CLASS: &str = "tw:grid tw:min-w-0 tw:flex-none tw:self-stretch";

/// Whether a press arms rather than acts: a Lasting action arms on its own
/// button, unless the surface around it has already asked (see
/// `ActionButton`'s `asked_by_surface`). Kept as a plain function so the
/// rule is testable without mounting.
fn arms_on_press(consequence: &ActionConsequence, asked_by_surface: bool) -> bool {
    consequence.arms() && !asked_by_surface
}

/// The two labels an arming chip renders AT THE SAME TIME: the
/// resting verb and the armed "Confirm ⟨verb⟩" reading. Both sit in one
/// grid cell (`.ux-armed-labels`), so the chip reserves the armed width
/// and arming never reflows the row — the RESERVE ruling from the
/// device-card-v2 spike (§2, gate 2026-09-02). Without a Lasting copy the
/// armed label is the resting one, and nothing renders it.
///
/// Kept as a plain function so the pair is testable without mounting.
fn confirm_chip_labels(label: &str, confirm_verb: Option<&str>) -> (String, String) {
    let armed = match confirm_verb {
        Some(verb) => format!("Confirm {verb}"),
        None => label.to_string(),
    };
    (label.to_string(), armed)
}

/// The arming chip's classes for its current armed state. The base
/// chip always wears `ux-armed-chip` (reserve mechanics + quiet drain
/// host); arming adds `ux-armed` (error tint, knock, drain running). Kept
/// as a plain function so the composition is testable without mounting.
fn confirm_chip_class(base: &'static str, armed: bool) -> String {
    if armed {
        format!("{base} ux-armed-chip ux-armed")
    } else {
        format!("{base} ux-armed-chip")
    }
}

/// The classes for `variant`; `tinted` is the consequence's error tint
/// (Undoable and Lasting), which every variant wears.
fn action_class(
    variant: ActionButtonVariant,
    priority: ActionPriority,
    tinted: bool,
) -> &'static str {
    match variant {
        ActionButtonVariant::Solid if tinted => SOLID_TINTED_CLASS,
        ActionButtonVariant::Solid => solid_class(priority),
        ActionButtonVariant::Quiet => quiet_class(tinted),
        ActionButtonVariant::Outline => outline_action_class(tinted),
        ActionButtonVariant::MenuItem => menu_item_class(tinted),
        ActionButtonVariant::RowEnd if tinted => ROW_END_TINTED_CLASS,
        ActionButtonVariant::RowEnd => ROW_END_CLASS,
        ActionButtonVariant::RowPrimary if tinted => ROW_PRIMARY_TINTED_CLASS,
        ActionButtonVariant::RowPrimary => ROW_PRIMARY_CLASS,
    }
}

/// [`ActionButtonVariant::RowEnd`]: a flush section the row's height, its
/// leading hairline (`ux-row-flush`, style.css: the row's own edge colour),
/// a faint wash, and a stronger one on hover. Tailwind preflight is not
/// loaded, so the UA button chrome is reset here.
const ROW_END_CLASS: &str = concat!(
    "tw:inline-flex tw:h-full tw:flex-none tw:cursor-pointer tw:appearance-none tw:items-center tw:gap-1 tw:whitespace-nowrap tw:border-0 tw:border-l tw:border-solid tw:bg-white/[0.035] tw:px-[11px] tw:text-[11px] tw:font-extrabold tw:leading-none tw:text-strong-foreground tw:transition-colors tw:hover:bg-white/10 tw:disabled:cursor-not-allowed tw:disabled:bg-transparent tw:disabled:text-dim-foreground",
    " ux-row-flush ux-focus-ring ux-press-flare"
);

/// [`ROW_END_CLASS`] wearing the error tint (Undoable, Lasting).
const ROW_END_TINTED_CLASS: &str = concat!(
    "tw:inline-flex tw:h-full tw:flex-none tw:cursor-pointer tw:appearance-none tw:items-center tw:gap-1 tw:whitespace-nowrap tw:border-0 tw:border-l tw:border-solid tw:bg-transparent tw:px-[11px] tw:text-[11px] tw:font-extrabold tw:leading-none tw:text-status-error-foreground tw:transition-colors tw:hover:bg-status-error-bg tw:disabled:cursor-not-allowed tw:disabled:opacity-60",
    " ux-row-flush ux-focus-ring"
);

/// [`ActionButtonVariant::RowPrimary`]: the name bar's flush section — no
/// fill at rest, the iridescent ring (inset, hugging the section's own
/// edges) and a wash on hover.
const ROW_PRIMARY_CLASS: &str = concat!(
    "tw:inline-flex tw:h-full tw:flex-none tw:cursor-pointer tw:appearance-none tw:items-center tw:gap-1.5 tw:whitespace-nowrap tw:border-0 tw:border-l tw:border-solid tw:bg-transparent tw:px-4 tw:text-[12.5px] tw:font-extrabold tw:leading-none tw:text-strong-foreground tw:transition-colors tw:hover:bg-white/[0.07] tw:disabled:cursor-not-allowed tw:disabled:text-dim-foreground",
    " ux-row-flush ux-ir-ring ux-ir-ring-inset ux-focus-ring ux-press-flare"
);

/// [`ROW_PRIMARY_CLASS`] wearing the error tint: no ring (a status tone
/// refuses the spectrum).
const ROW_PRIMARY_TINTED_CLASS: &str = concat!(
    "tw:inline-flex tw:h-full tw:flex-none tw:cursor-pointer tw:appearance-none tw:items-center tw:gap-1.5 tw:whitespace-nowrap tw:border-0 tw:border-l tw:border-solid tw:bg-transparent tw:px-4 tw:text-[12.5px] tw:font-extrabold tw:leading-none tw:text-status-error-foreground tw:transition-colors tw:hover:bg-status-error-bg tw:disabled:cursor-not-allowed tw:disabled:opacity-60",
    " ux-row-flush ux-focus-ring"
);

/// The solid tier wearing the error tint, whatever its priority: the same
/// geometry as every tier, the error border and text, and no ring — a status
/// tone refuses the spectrum (see [`outline_action_class`]).
const SOLID_TINTED_CLASS: &str = concat!(
    "tw:inline-flex tw:min-h-9 tw:max-w-full tw:items-center tw:justify-center tw:gap-2 tw:rounded-sm tw:border tw:px-3 tw:text-sm tw:font-bold tw:leading-none tw:break-words tw:disabled:cursor-not-allowed tw:disabled:opacity-60",
    " tw:border-status-error-border tw:bg-transparent tw:text-status-error-foreground tw:hover:bg-status-error-bg",
    " ux-focus-ring ux-press-flare"
);

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

/// The classes `variant` draws a button with (its secondary tier where the
/// variant has tiers), for a control that is not an [`ActionButton`] but
/// must read as one: a picker's trigger in the same row as a press.
pub fn action_variant_class(variant: ActionButtonVariant, tinted: bool) -> &'static str {
    action_class(variant, ActionPriority::Secondary, tinted)
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

    /// A Lasting press arms on its own button — and acts at once inside the
    /// surface that already asked (the layout sheet's Continue, G1 walk
    /// 2026-10-03). Nothing that does not arm starts arming.
    #[test]
    fn a_lasting_press_arms_unless_its_surface_already_asked() {
        let lasting = ActionConsequence::Lasting(lpa_studio_core::ActionConfirmation::new(
            "Rewrite this board now?",
            "It goes.",
            "continue",
        ));
        assert!(arms_on_press(&lasting, false));
        assert!(!arms_on_press(&lasting, true));
        for level in [ActionConsequence::Routine, ActionConsequence::Undoable] {
            assert!(!arms_on_press(&level, false));
            assert!(!arms_on_press(&level, true));
        }
    }

    #[test]
    fn every_solid_tier_keeps_the_same_geometry() {
        // The interaction light is pseudo-elements and outlines only: a
        // tier swap must never resize a button, so the geometry tokens are
        // identical across tiers, the tinted one included.
        for class in PRIORITIES
            .map(solid_class)
            .into_iter()
            .chain([SOLID_TINTED_CLASS])
        {
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
            SOLID_TINTED_CLASS,
            quiet_class(false),
            quiet_class(true),
            menu_item_class(false),
            menu_item_class(true),
        ] {
            assert!(!class.contains("ux-ir-ring"), "{class}");
        }
    }

    #[test]
    fn every_variant_wears_the_error_tint_for_a_consequence() {
        // One look per level, whichever variant draws it (D7, Q7): Undoable
        // and Lasting wear the error tint in every home, Routine never does.
        for variant in [
            ActionButtonVariant::Solid,
            ActionButtonVariant::Quiet,
            ActionButtonVariant::Outline,
            ActionButtonVariant::MenuItem,
            ActionButtonVariant::RowEnd,
            ActionButtonVariant::RowPrimary,
        ] {
            for priority in PRIORITIES {
                let tinted = action_class(variant, priority, true);
                assert!(tinted.contains("status-error"), "{variant:?}: {tinted}");
                let plain = action_class(variant, priority, false);
                assert!(!plain.contains("status-error"), "{variant:?}: {plain}");
            }
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

    /// A row section fills its fixed row and never grows it: the row's
    /// height, no padding on the block axis, and no line under it (the
    /// refusal is the title). Only the primary takes the ring, and never
    /// tinted.
    #[test]
    fn a_row_section_is_flush_and_only_the_primary_takes_the_ring() {
        for tinted in [false, true] {
            for variant in [ActionButtonVariant::RowEnd, ActionButtonVariant::RowPrimary] {
                let class = action_class(variant, ActionPriority::Secondary, tinted);
                for token in ["tw:h-full", "tw:border-l", "ux-row-flush", "ux-focus-ring"] {
                    assert!(class.contains(token), "{token} missing: {class}");
                }
                assert!(!class.contains("tw:py-"), "{class}");
                assert!(!class.contains("rounded"), "{class}");
            }
            let end = action_class(ActionButtonVariant::RowEnd, ActionPriority::Primary, tinted);
            assert!(!end.contains("ux-ir-ring"), "{end}");
        }
        assert!(
            action_class(
                ActionButtonVariant::RowPrimary,
                ActionPriority::Primary,
                false
            )
            .contains("ux-ir-ring ux-ir-ring-inset")
        );
        assert!(
            !action_class(
                ActionButtonVariant::RowPrimary,
                ActionPriority::Primary,
                true
            )
            .contains("ux-ir-ring")
        );
        assert!(ActionButtonVariant::RowEnd.is_row_section());
        assert!(!ActionButtonVariant::MenuItem.is_row_section());
        assert!(ROW_SECTION_WRAPPER_CLASS.contains("tw:self-stretch"));
    }

    #[test]
    fn the_armed_chip_renders_both_labels_at_once() {
        // RESERVE (device-card-v2 spike §2, gate 2026-09-02): the chip
        // renders its resting verb AND "Confirm ⟨verb⟩" in one grid cell,
        // so its width is the armed width in both states. The exact
        // "Confirm ⟨verb⟩" reading is the 2026-08-31 gate's ruling and
        // survives the mechanism change.
        let (rest, armed) = confirm_chip_labels("Forget", Some("Forget"));
        assert_eq!(rest, "Forget");
        assert_eq!(armed, "Confirm Forget");

        let (rest, armed) = confirm_chip_labels("Factory reset", Some("Erase everything"));
        assert_eq!(rest, "Factory reset");
        assert_eq!(armed, "Confirm Erase everything");

        // No Lasting copy: nothing renders the pair, and the armed label
        // must not invent a verb.
        let (rest, armed) = confirm_chip_labels("Disconnect", None);
        assert_eq!(rest, "Disconnect");
        assert_eq!(armed, "Disconnect");
    }

    #[test]
    fn the_armed_chip_composes_the_armed_dress_over_its_base() {
        // 2K+ (devices-treatments gate): the arming chip always
        // hosts the reserve/drain mechanics; arming adds the tint/knock
        // class. The spectrum ring never reaches a destructive chip.
        let idle = confirm_chip_class(quiet_class(true), false);
        assert!(idle.contains("ux-armed-chip"), "{idle}");
        assert!(!idle.contains("ux-armed "), "{idle}");
        assert!(!idle.ends_with("ux-armed"), "{idle}");
        let armed = confirm_chip_class(quiet_class(true), true);
        assert!(armed.contains("ux-armed-chip"), "{armed}");
        assert!(armed.ends_with("ux-armed"), "{armed}");
        for class in [&idle, &armed] {
            assert!(!class.contains("ux-ir-ring"), "{class}");
            assert!(!class.contains("ux-spectrum-cta"), "{class}");
        }
    }

    /// A Lasting menu row keeps its words at its leading edge, armed or
    /// not (`ux-armed-labels-start`, style.css); every other look centres
    /// them in the reserved width.
    #[test]
    fn a_lasting_menu_rows_labels_sit_at_its_leading_edge() {
        assert_eq!(
            armed_labels_class(ActionButtonVariant::MenuItem),
            "ux-armed-labels ux-armed-labels-start"
        );
        for variant in [
            ActionButtonVariant::Solid,
            ActionButtonVariant::Quiet,
            ActionButtonVariant::Outline,
            ActionButtonVariant::RowEnd,
            ActionButtonVariant::RowPrimary,
        ] {
            assert_eq!(
                armed_labels_class(variant),
                "ux-armed-labels",
                "{variant:?}"
            );
        }
        let css = include_str!("../../style.css");
        let rule = ".ux-armed-labels.ux-armed-labels-start > span {";
        let at = css.find(rule).expect("the leading-edge rule");
        let body = &css[at..at + css[at..].find('}').expect("closes")];
        assert!(body.contains("justify-content: flex-start"), "{body}");
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
