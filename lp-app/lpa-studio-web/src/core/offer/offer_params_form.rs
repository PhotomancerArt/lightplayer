//! [`OfferParamsForm`]: any offer's parameters as controls, and
//! [`OfferPressButton`]: the press that carries what they hold.
//!
//! A verb that needs values first (flash *which board*, rename *to what*)
//! declares [`OfferParam`]s in core; a renderer draws them and hands the
//! picked values back as [`OfferArgs`], and [`UiOffer::press`] binds them
//! into the action. The renderer never builds an op: what the press
//! dispatches is the offer's own binding.
//!
//! The form is the generic renderer — a choice as a list of option rows, a
//! text param as a field, a toggle as a checkbox — so any offer can be drawn
//! (the app agent's chat card hands the user the real control this way).
//! A surface with a richer picker for one parameter (the device card's
//! board tiles, its project gallery) draws that parameter itself over the
//! same `Signal<OfferArgs>` and the same two helpers, [`visible_options`]
//! and [`resolved_args`], so both readings agree on what is picked.

use dioxus::prelude::*;
use lpa_studio_core::{OfferArgs, OfferChoice, OfferParam, OfferParamKind, UiAction, UiOffer};

use crate::base::{OPTION_CARD_CHECK_CLASS, StudioIcon, StudioIconName};
use crate::core::action::{ActionButton, ActionButtonVariant};

/// Every parameter of `offer`, in its declared order, over `args`: each
/// control reads its value from `args` (falling back to the parameter's own
/// default) and writes what the user picks or types back into it.
///
/// `args` is the caller's, so the caller decides where the values start —
/// empty for a fresh pick, pre-filled when someone (the app agent) already
/// chose — and reads them back for the press ([`OfferPressButton`]).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn OfferParamsForm(
    offer: UiOffer,
    args: Signal<OfferArgs>,
    /// Parameters the caller draws itself (a richer picker for one of
    /// them); the form skips them.
    #[props(default)]
    skip: Vec<String>,
) -> Element {
    let current = args.read().clone();
    let params: Vec<OfferParam> = offer
        .params()
        .iter()
        .filter(|param| !skip.contains(&param.name))
        .cloned()
        .collect();
    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:gap-2.5",
            for param in params {
                {
                    let name = param.name.clone();
                    match &param.kind {
                        OfferParamKind::Choice { .. } => {
                            let picked = picked_choice(&offer, &param, &current);
                            let options: Vec<OfferChoice> = visible_options(&offer, &param, &current)
                                .into_iter()
                                .cloned()
                                .collect();
                            rsx! {
                                fieldset { key: "{name}", class: FIELDSET_CLASS,
                                    legend { class: LABEL_CLASS, "{param.label}" }
                                    if let Some(note) = param.note.clone() {
                                        p { class: NOTE_CLASS, "{note}" }
                                    }
                                    div { class: OPTIONS_CLASS,
                                    for option in options {
                                        {
                                            let selected = picked.as_deref() == Some(option.value.as_str());
                                            let value = option.value.clone();
                                            let name = name.clone();
                                            rsx! {
                                                button {
                                                    key: "{option.value}",
                                                    class: option_class(selected),
                                                    r#type: "button",
                                                    disabled: option.disabled.is_some(),
                                                    title: option.disabled.clone().unwrap_or_default(),
                                                    aria_pressed: "{selected}",
                                                    onclick: move |_| args.write().insert(name.clone(), value.clone()),
                                                    // A pick made before the list was drawn (a long
                                                    // list, scrolled) starts in view.
                                                    onmounted: move |event: MountedEvent| async move {
                                                        if selected {
                                                            // After the panel's entrance has placed it: it
                                                            // mounts, then moves into the top layer, and a
                                                            // scroll before that lands nowhere (measured: an
                                                            // immediate or 0 ms scroll left the pick out of
                                                            // view).
                                                            gloo_timers::future::TimeoutFuture::new(150).await;
                                                            let _ = event.data().scroll_to_with_options(PICK_IN_VIEW).await;
                                                        }
                                                    },
                                                    if selected {
                                                        span { class: OPTION_CARD_CHECK_CLASS, aria_hidden: "true",
                                                            StudioIcon { name: StudioIconName::StepComplete, size: 10 }
                                                        }
                                                    }
                                                    span { class: OPTION_TITLE_CLASS, "{option.label}" }
                                                    if let Some(detail) = option.disabled.clone().or(option.detail.clone()) {
                                                        span { class: OPTION_DETAIL_CLASS, "{detail}" }
                                                    }
                                                    if let Some(warning) = option.warning.clone().filter(|_| option.disabled.is_none()) {
                                                        span { class: OPTION_WARNING_CLASS, "{warning}" }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    }
                                }
                            }
                        }
                        // A secret (a Wi‑Fi password): a password field
                        // that never autofills a saved login, with a
                        // show/hide toggle. Never echoed anywhere else.
                        OfferParamKind::Text { placeholder, secret: true, .. } => {
                            let value = current.get(&name).unwrap_or_default().to_string();
                            rsx! {
                                SecretField {
                                    key: "{name}",
                                    label: param.label.clone(),
                                    placeholder: placeholder.clone(),
                                    value,
                                    on_input: move |text: String| args.write().insert(name.clone(), text),
                                }
                            }
                        }
                        OfferParamKind::Text { placeholder, max_len, .. } => {
                            let value = current.get(&name).unwrap_or_default().to_string();
                            rsx! {
                                label { key: "{name}", class: FIELD_CLASS,
                                    span { class: LABEL_CLASS, "{param.label}" }
                                    input {
                                        class: INPUT_CLASS,
                                        r#type: "text",
                                        placeholder: "{placeholder}",
                                        maxlength: max_len.map(|limit| limit.to_string()),
                                        value: "{value}",
                                        oninput: move |event| args.write().insert(name.clone(), event.value()),
                                    }
                                }
                            }
                        }
                        OfferParamKind::Toggle { .. } => {
                            let on = toggle_value(&offer, &current, &name);
                            rsx! {
                                label { key: "{name}", class: TOGGLE_CLASS,
                                    input {
                                        r#type: "checkbox",
                                        checked: on,
                                        onchange: move |event| {
                                            args.write().insert(name.clone(), event.checked().to_string())
                                        },
                                    }
                                    "{param.label}"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// A secret text parameter's field: `type="password"` (or text while the
/// user holds it shown), never autofilled from a saved login, never
/// spell-checked (a spell checker may send the text off the page).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn SecretField(
    label: String,
    placeholder: String,
    value: String,
    on_input: EventHandler<String>,
) -> Element {
    let mut shown = use_signal(|| false);
    let (kind, toggle) = if shown() {
        ("text", "Hide")
    } else {
        ("password", "Show")
    };
    rsx! {
        label { class: FIELD_CLASS,
            span { class: LABEL_CLASS, "{label}" }
            span { class: "tw:flex tw:min-w-0 tw:items-center tw:gap-1.5",
                input {
                    class: "{INPUT_CLASS} tw:flex-1",
                    r#type: kind,
                    autocomplete: "new-password",
                    spellcheck: "false",
                    autocapitalize: "off",
                    placeholder: "{placeholder}",
                    value: "{value}",
                    oninput: move |event| on_input.call(event.value()),
                }
                button {
                    class: SECRET_TOGGLE_CLASS,
                    r#type: "button",
                    aria_pressed: "{shown()}",
                    onclick: move |_| shown.toggle(),
                    "{toggle}"
                }
            }
        }
    }
}

/// The offer's press with `args`: an [`ActionButton`] wearing the bound
/// action (so a Lasting binding arms on the button itself, and a Routine one
/// is one click), or — when the values do not bind yet — the offer's own
/// verb, disabled with the refusal in plain words ("`board` is required:
/// choose a board").
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn OfferPressButton(
    offer: UiOffer,
    args: OfferArgs,
    #[props(default)] variant: ActionButtonVariant,
    /// Stories only: start armed.
    #[props(default)]
    armed_preview: bool,
    /// Don't print why the press is refused under the button (the form
    /// already shows what is missing, e.g. an empty field).
    #[props(default)]
    hide_refusal: bool,
    on_action: EventHandler<UiAction>,
) -> Element {
    let action = pressed_or_refused(&offer, &args);
    rsx! {
        ActionButton {
            action,
            running: false,
            variant,
            armed_preview,
            reason_said_elsewhere: hide_refusal,
            on_action,
        }
    }
}

/// What a press with `args` dispatches, or the offer's verb disabled with
/// why it would be refused.
pub fn pressed_or_refused(offer: &UiOffer, args: &OfferArgs) -> UiAction {
    match offer.press(&resolved_args(offer, args)) {
        Ok(action) => action,
        Err(error) => offer.action.clone().disabled(error.to_string()),
    }
}

/// The options of choice `param` a renderer draws, given `args`: every
/// option, except those offered only with a toggle that is off (the
/// chip-narrowed board list hides the other boards until `all_boards`).
pub fn visible_options<'a>(
    offer: &UiOffer,
    param: &'a OfferParam,
    args: &OfferArgs,
) -> Vec<&'a OfferChoice> {
    let OfferParamKind::Choice { options, .. } = &param.kind else {
        return Vec::new();
    };
    options
        .iter()
        .filter(|option| match &option.only_with {
            Some(toggle) => toggle_value(offer, args, toggle),
            None => true,
        })
        .collect()
}

/// `args` as a press should carry them: a picked option the renderer no
/// longer draws (the list changed under it, or the toggle that widened it
/// was turned off) is dropped, so the offer's own preselect stands in
/// rather than a value that is gone — the stale-pick guard.
pub fn resolved_args(offer: &UiOffer, args: &OfferArgs) -> OfferArgs {
    let mut resolved = OfferArgs::new();
    for (name, value) in args.iter() {
        let stale = offer.params().iter().any(|param| {
            param.name == name
                && matches!(param.kind, OfferParamKind::Choice { .. })
                && !visible_options(offer, param, args)
                    .iter()
                    .any(|option| option.value == value)
        });
        if !stale {
            resolved.insert(name, value);
        }
    }
    resolved
}

/// The option a choice reads as picked: the (still drawn) value in `args`,
/// else the parameter's preselect.
pub(crate) fn picked_choice(
    offer: &UiOffer,
    param: &OfferParam,
    args: &OfferArgs,
) -> Option<String> {
    resolved_args(offer, args)
        .choice(&param.name)
        .map(str::to_string)
        .or_else(|| param.default_value())
}

/// A toggle's value: what `args` says, else its current state.
pub(crate) fn toggle_value(offer: &UiOffer, args: &OfferArgs, name: &str) -> bool {
    args.toggle(name).unwrap_or_else(|| {
        offer
            .params()
            .iter()
            .find(|param| param.name == name)
            .and_then(|param| match param.kind {
                OfferParamKind::Toggle { value } => Some(value),
                _ => None,
            })
            .unwrap_or(false)
    })
}

/// Scroll a pick into view only as far as needed, at once.
const PICK_IN_VIEW: ScrollToOptions = ScrollToOptions {
    behavior: ScrollBehavior::Instant,
    vertical: ScrollLogicalPosition::Nearest,
    horizontal: ScrollLogicalPosition::Nearest,
};

const FIELDSET_CLASS: &str = "tw:m-0 tw:grid tw:min-w-0 tw:border-0 tw:p-0";

/// The options' grid: as many columns as fit, and a long list (a version
/// list with "All versions" on) scrolls inside about six rows, so the
/// controls under it stay in view.
const OPTIONS_CLASS: &str = "tw:grid tw:max-h-[18rem] tw:min-w-0 tw:grid-cols-[repeat(auto-fill,minmax(140px,1fr))] tw:gap-1.5 tw:overflow-y-auto tw:overscroll-contain";

/// A parameter's note: one quiet line under its label.
const NOTE_CLASS: &str = "tw:m-0 tw:mb-1.5 tw:text-[10.5px] tw:text-dim-foreground";

const LABEL_CLASS: &str =
    "tw:mb-1 tw:p-0 tw:text-[11px] tw:font-semibold tw:text-subtle-foreground";

const FIELD_CLASS: &str = "tw:grid tw:min-w-0 tw:gap-1";

const INPUT_CLASS: &str = "tw:min-w-0 tw:appearance-none tw:rounded-xs tw:border tw:border-border tw:bg-card tw:px-2 tw:py-1 tw:text-[11.5px] tw:text-strong-foreground";

const SECRET_TOGGLE_CLASS: &str = "tw:flex-none tw:cursor-pointer tw:rounded-xs tw:border tw:border-border tw:bg-transparent tw:px-1.5 tw:py-1 tw:text-[10.5px] tw:font-semibold tw:text-subtle-foreground tw:hover:bg-white/5";

const TOGGLE_CLASS: &str =
    "tw:flex tw:cursor-pointer tw:items-center tw:gap-1.5 tw:text-[11px] tw:text-muted-foreground";

const OPTION_TITLE_CLASS: &str = "tw:min-w-0 tw:truncate tw:text-xs tw:font-semibold";

const OPTION_DETAIL_CLASS: &str = "tw:min-w-0 tw:truncate tw:text-[10.5px] tw:text-dim-foreground";

/// An option's caution, in the muted warning tone.
const OPTION_WARNING_CLASS: &str =
    "tw:min-w-0 tw:truncate tw:text-[10.5px] tw:text-status-warning-foreground";

/// One option row, in the option-card grammar the device pickers use:
/// selected wears the static ring, the selection wash and the check badge.
fn option_class(selected: bool) -> &'static str {
    if selected {
        "ux-sel-ring tw:relative tw:grid tw:min-w-0 tw:cursor-pointer tw:appearance-none tw:content-start tw:gap-0.5 tw:rounded-sm tw:border tw:border-transparent tw:bg-selection-bg tw:p-2 tw:text-left tw:text-strong-foreground"
    } else {
        "tw:relative tw:grid tw:min-w-0 tw:cursor-pointer tw:appearance-none tw:content-start tw:gap-0.5 tw:rounded-sm tw:border tw:border-border-subtle tw:bg-transparent tw:p-2 tw:text-left tw:text-muted-foreground tw:hover:border-border-strong tw:hover:text-strong-foreground tw:disabled:cursor-not-allowed tw:disabled:opacity-60"
    }
}

#[cfg(test)]
mod tests {
    use lpa_studio_core::{
        ActionEnablement, DeviceId, FLASH_ALL_BOARDS_PARAM, FLASH_BOARD_PARAM, OfferPath,
        flash_pending_offer,
    };

    use super::*;

    #[test]
    fn a_widened_option_is_drawn_only_with_its_toggle_on() {
        let offer = blank_c6_flash();
        let board = &offer.params()[0];
        let narrowed = visible_options(&offer, board, &OfferArgs::new());
        let widened = visible_options(
            &offer,
            board,
            &OfferArgs::new().with(FLASH_ALL_BOARDS_PARAM, "true"),
        );
        assert!(!narrowed.is_empty());
        assert!(widened.len() > narrowed.len(), "show all widens the list");
        assert!(narrowed.iter().all(|option| option.only_with.is_none()));
    }

    #[test]
    fn a_stale_pick_falls_back_to_the_offers_own_default() {
        let offer = blank_c6_flash();
        let board = &offer.params()[0];
        let wider = visible_options(
            &offer,
            board,
            &OfferArgs::new().with(FLASH_ALL_BOARDS_PARAM, "true"),
        )
        .into_iter()
        .find(|option| option.only_with.is_some())
        .expect("a board outside the chip")
        .value
        .clone();

        // Picked while widened, then show-all turned off: the pick is gone.
        let args = OfferArgs::new()
            .with(FLASH_BOARD_PARAM, &wider)
            .with(FLASH_ALL_BOARDS_PARAM, "false");
        assert_eq!(resolved_args(&offer, &args).choice(FLASH_BOARD_PARAM), None);
        assert_eq!(picked_choice(&offer, board, &args), board.default_value());

        // Still widened: it stands, and the press binds it.
        let args = OfferArgs::new()
            .with(FLASH_BOARD_PARAM, &wider)
            .with(FLASH_ALL_BOARDS_PARAM, "true");
        assert_eq!(picked_choice(&offer, board, &args), Some(wider));
        assert!(
            pressed_or_refused(&offer, &args)
                .meta()
                .enablement
                .is_enabled()
        );
    }

    #[test]
    fn a_press_that_does_not_bind_reads_as_the_verb_disabled_with_why() {
        let offer = flash_pending_offer(&pending(None), prefix()).unwrap();
        let action = pressed_or_refused(&offer, &OfferArgs::new());
        assert_eq!(action.meta().label, offer.label());
        assert!(matches!(
            &action.meta().enablement,
            ActionEnablement::Disabled { reason } if reason.contains("choose a board")
        ));
    }

    fn blank_c6_flash() -> UiOffer {
        flash_pending_offer(&pending(Some("esp32c6")), prefix()).expect("a blank chip flashes")
    }

    fn prefix() -> OfferPath {
        OfferPath::board(&lpa_studio_core::BoardRef::New(3))
    }

    fn pending(chip: Option<&str>) -> lpa_studio_core::PendingLinkView {
        lpa_studio_core::PendingLinkView {
            link: lpa_studio_core::DeviceLinkId(4),
            device: DeviceId(3),
            title: "New device".to_string(),
            state_label: "New device found".to_string(),
            detail: None,
            can_adopt: true,
            firmware_face: lpa_studio_core::DeviceFirmwareFace::Blank,
            detected_chip: chip.map(str::to_string),
            mac: None,
            firmware_blocked: None,
            escapes: vec![lpa_studio_core::DeviceEscape::Forget],
        }
    }
}
