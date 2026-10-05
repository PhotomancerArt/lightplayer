//! The Projects header's "New" control: a template menu, not a button.
//!
//! `New` used to be a one-shot chip that made a blank project (the D17
//! deviation, 2026-07-27). With pattern projects (module authoring unit,
//! D9/D14/D15) there are three ways to start, so the chip opens a menu
//! whose rows ARE the templates.
//!
//! **Text-first, deliberately.** The design spike (§4) drew visual cards
//! with a rig sketch and a mini created-tree; production round one keeps
//! the picker's flat-list grammar instead — three rows, each a title and a
//! dim one-liner — because the detail card caps at 320px and a three-card
//! grid fights that cap. The sketches and the tree hint are recorded as a
//! future embellishment, not dropped.
//!
//! The menu is a renderer of core's `project/new` offer: its rows are the
//! offer's `template` choices (titles and one-liners from
//! [`lpa_studio_core::ProjectTemplate`]), and a row presses the offer with
//! that template and the typed name — the same press the app agent makes.
//! Adding a template is one arm in the core enum plus one in the file
//! generator, and this menu grows the row for free.
//!
//! **One optional name field** sits above the rows (2026-09-06: Yona could
//! not find how to name a project while setting up a piece). Blank keeps
//! the ruling that a template needs no prompt — the library names the
//! package after the template — and a typed name rides the row's
//! `CreateProject` as the package's name. Not a step: a row is still one
//! click.

use dioxus::prelude::*;
use lpa_studio_core::{
    NEW_PROJECT_NAME_PARAM, NEW_PROJECT_TEMPLATE_PARAM, OfferArgs, OfferChoice, OfferParamKind,
    OfferPath, UiAction, UiOffer,
};

use crate::base::{
    DetailPopover, DetailSection, PopoverCloseHandle, PopoverPlacement, StudioIcon, StudioIconName,
};
use crate::core::{quiet_action_class, use_offer_at};

/// The Projects header's New control. The trigger keeps the quiet-chip
/// look it shares with Import and Paste — only what it opens changed.
///
/// Draws nothing when the view publishes no `project/new` (no project
/// library, or a surface mounted outside the shell's offer tree).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn NewProjectMenu(
    /// A create is already in flight (the header's shared busy flag).
    #[props(default = false)]
    busy: bool,
    /// Open the menu immediately (stories only).
    #[props(default = false)]
    initially_open: bool,
    on_action: EventHandler<UiAction>,
) -> Element {
    let offer = use_offer_at(OfferPath::project().child("new"))();
    let rest = quiet_action_class().to_string();
    let open = format!("{rest} tw:bg-card-muted tw:text-soft-foreground");
    // The optional name, shared by every row: typed once, carried by
    // whichever template is picked.
    let mut name = use_signal(String::new);
    let Some(offer) = offer else {
        return rsx! {};
    };
    let templates = template_choices(&offer);

    rsx! {
        DetailPopover {
            icon: StudioIconName::Add,
            label: "New project".to_string(),
            title: "Start a new project from a template.".to_string(),
            placement: PopoverPlacement::BottomStart,
            initially_open,
            layer_keeps_layout: true,
            trigger: rsx! {
                span { class: "tw:inline-flex tw:h-[15px] tw:w-[15px] tw:items-center tw:justify-center", aria_hidden: "true",
                    StudioIcon { name: StudioIconName::Add, size: 14 }
                }
                span { "New" }
            },
            trigger_class: rest,
            trigger_open_class: open,
            DetailSection { title: Some("New project".to_string()),
                div { class: "tw:grid tw:gap-1.5",
                    input {
                        class: NAME_INPUT_CLASS,
                        r#type: "text",
                        aria_label: "Project name (optional)",
                        placeholder: "Name (optional) \u{2014} else named after the template",
                        value: "{name}",
                        oninput: move |event| name.set(event.value()),
                    }
                    div { class: "tw:grid tw:gap-0.5",
                        for template in templates {
                            TemplateRow {
                                key: "{template.value}",
                                offer: offer.clone(),
                                template,
                                name: name.read().clone(),
                                busy,
                                on_action,
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The offer's `template` choices, in the order core offers them.
fn template_choices(offer: &UiOffer) -> Vec<OfferChoice> {
    offer
        .params()
        .iter()
        .find(|param| param.name == NEW_PROJECT_TEMPLATE_PARAM)
        .and_then(|param| match &param.kind {
            OfferParamKind::Choice { options, .. } => Some(options.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

/// One template row: title over a dim one-liner, pressing `project/new`
/// with this template and the typed name, and closing the menu (a pick is
/// a completed gesture, the add-node picker's rule).
///
/// Bespoke rather than `ActionButton { variant: MenuItem }` only because
/// the row is two lines — the classes and the dispatch shape are the
/// shared menu-row ones.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn TemplateRow(
    offer: UiOffer,
    template: OfferChoice,
    /// The menu's optional name, as typed (core trims it; blank is none).
    #[props(default)]
    name: String,
    #[props(default = false)] busy: bool,
    on_action: EventHandler<UiAction>,
) -> Element {
    let args = OfferArgs::new()
        .with(NEW_PROJECT_TEMPLATE_PARAM, &template.value)
        .with(NEW_PROJECT_NAME_PARAM, name);
    let press = offer.press(&args);
    let summary = match &press {
        Ok(action) => action.meta().summary.clone(),
        Err(refused) => refused.to_string(),
    };
    let close = try_consume_context::<PopoverCloseHandle>();

    rsx! {
        button {
            class: template_row_class(),
            r#type: "button",
            disabled: busy || press.is_err(),
            title: "{summary}",
            onclick: move |event| {
                event.stop_propagation();
                if let Ok(action) = &press {
                    on_action.call(action.clone());
                }
                if let Some(mut close) = close {
                    close.close();
                }
            },
            span { class: "tw:grid tw:min-w-0 tw:gap-px",
                span { class: "tw:text-sm tw:leading-tight tw:text-strong-foreground",
                    "{template.label}"
                }
                span { class: "tw:text-[11px] tw:leading-tight tw:text-dim-foreground",
                    "{template.detail.clone().unwrap_or_default()}"
                }
            }
        }
    }
}

/// The optional name field — the card rename input's dress, at the menu's
/// width.
const NAME_INPUT_CLASS: &str = "tw:min-w-0 tw:rounded tw:border tw:border-border tw:bg-terminal tw:px-2 tw:py-1 tw:text-sm tw:text-strong-foreground";

/// The menu-row treatment, top-aligned for a two-line row (the shared
/// `menu_item_action_class` centers its single line).
fn template_row_class() -> &'static str {
    "tw:flex tw:w-full tw:cursor-pointer tw:appearance-none tw:items-start tw:gap-2 tw:rounded tw:border-none tw:bg-transparent tw:px-2 tw:py-1.5 tw:text-left tw:text-sm tw:text-muted-foreground tw:transition-colors tw:hover:bg-white/5 tw:disabled:cursor-not-allowed tw:disabled:opacity-60"
}
