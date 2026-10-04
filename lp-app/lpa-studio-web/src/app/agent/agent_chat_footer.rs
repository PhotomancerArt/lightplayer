//! [`AgentChatFooter`]: the footnote under both agent chats — the model
//! chip (left), any host extras beside it (the shader chat's export
//! buttons), and the session's usage with its cost (right).
//!
//! The cost is core's display string: what the provider reported charging
//! when it reports (OpenRouter, exact), the pricing table's estimate (`~`)
//! otherwise, and nothing at all for a model neither prices — never "$0"
//! for tokens that were spent.

use dioxus::prelude::*;
use lpa_studio_core::{SettingsCommand, UiAgentModelView, UiAgentUsage, UiModelOption};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn AgentChatFooter(
    model: UiAgentModelView,
    busy: bool,
    usage: UiAgentUsage,
    /// Core's display-ready cost (`$0.0042` reported, `~$0.02` estimated).
    cost: Option<String>,
    /// Host extras after the model chip.
    #[props(default)]
    children: Element,
) -> Element {
    // Total prompt tokens (fresh + cache writes + cache reads) — the
    // footnote's honest "in" figure now that prompt caching splits usage
    // into disjoint buckets; the raw `input_tokens` bucket is only the
    // uncached remainder and would read absurdly low.
    let input_tokens = usage.total_input_tokens();
    rsx! {
        div { class: "tw:flex tw:min-w-0 tw:items-center tw:gap-2 tw:border-t tw:border-border-subtle tw:bg-card tw:px-3 tw:py-1",
            ModelChip { model, busy }
            {children}
            span { class: "tw:min-w-0 tw:flex-1" }
            if !usage.is_zero() {
                p { class: "tw:m-0 tw:flex-none tw:text-right tw:text-[10px] tw:text-dim-foreground",
                    "{input_tokens} in · {usage.output_tokens} out tokens this session"
                    if let Some(cost) = cost.as_deref() {
                        span { title: cost_title(cost), " · {cost}" }
                    }
                }
            }
        }
    }
}

/// The cost's tooltip: an estimate (core prefixes it `~`) or what the
/// provider said it charged.
fn cost_title(cost: &str) -> &'static str {
    if cost.starts_with('~') {
        "estimate based on configured rates"
    } else {
        "what the provider reported charging"
    }
}

/// The footnote's compact model selector: shows the session's model
/// without opening settings and switches it in place. Options come from
/// P8's fetched `/models` list (the selector requests a fetch when it
/// opens; the store debounces); a selection dispatches the SAME
/// [`SettingsCommand::SetAgentModel`] mutation the popover's model field
/// uses and applies from the NEXT run (providers rebuild at run start).
/// Custom free-text ids stay a settings-popover affair — a disabled tail
/// option points there. Disabled while a run is in flight (switching
/// mid-run is out of scope); without a configured model the chip is a
/// plain pointer at Settings.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ModelChip(model: UiAgentModelView, busy: bool) -> Element {
    // The settings dispatch context (installed by the web shell; absent
    // under stories, which render the chip inert).
    let on_settings = try_consume_context::<Callback<SettingsCommand>>();
    let Some(effective) = model.effective.clone() else {
        return rsx! {
            span { class: "tw:flex-none tw:text-[10px] tw:font-bold tw:text-status-warning-foreground",
                title: "The provider needs a model id — set one in Settings (the gear icon, top right)",
                "model: set in Settings"
            }
        };
    };
    let options = model_chip_options(&model);
    let request_models = move || {
        if let Some(handler) = on_settings {
            handler.call(SettingsCommand::RequestModels { force: false });
        }
    };
    rsx! {
        // The font classes live on this wrapper; the select's `font:
        // inherit` reset (style.css, base layer) picks them up.
        span { class: "tw:min-w-0 tw:flex-none tw:font-mono tw:text-[10px]",
            select {
                class: model_chip_class(busy),
                disabled: busy,
                title: if busy { "Model for the next run — wait for this run to finish to switch" } else { "Model for the next run — custom ids in Settings" },
                value: "{effective}",
                // Opening the selector is the fetch trigger
                // (store-debounced); pointer and keyboard opens both land
                // here.
                onpointerdown: move |_| request_models(),
                onfocus: move |_| request_models(),
                onchange: move |event| {
                    let value = event.value();
                    if let Some(handler) = on_settings
                        && value != SETTINGS_MODEL_VALUE
                    {
                        handler.call(SettingsCommand::SetAgentModel(Some(value)));
                    }
                },
                for option in options {
                    option {
                        key: "{option.id}",
                        value: "{option.id}",
                        selected: option.id == effective,
                        "{option.label.as_deref().unwrap_or(&option.id)}"
                    }
                }
                option {
                    value: SETTINGS_MODEL_VALUE,
                    disabled: true,
                    if model.loading { "fetching model list…" } else { "Custom / more — Settings" }
                }
            }
        }
    }
}

/// The sentinel value of the chip's inert Settings-pointer option (never
/// dispatched; the option is disabled and exists as a signpost).
const SETTINGS_MODEL_VALUE: &str = "__settings__";

/// The chip's option list: the fetched models, with the effective id
/// prepended when the list does not carry it (an unlisted override, or no
/// fetch yet — the select must always be able to show the truth).
fn model_chip_options(model: &UiAgentModelView) -> Vec<UiModelOption> {
    let mut options = model.options.clone();
    if let Some(effective) = &model.effective
        && !options.iter().any(|option| &option.id == effective)
    {
        options.insert(
            0,
            UiModelOption {
                id: effective.clone(),
                label: None,
                detail: None,
            },
        );
    }
    options
}

/// Model-chip chrome: an unobtrusive borderless select that reveals its
/// affordance on hover; inert while a run is in flight — constant
/// geometry either way.
fn model_chip_class(busy: bool) -> String {
    let state = if busy {
        "tw:cursor-default tw:opacity-50"
    } else {
        "tw:cursor-pointer tw:hover:border-border-subtle tw:hover:text-muted-foreground"
    };
    format!(
        "tw:min-w-0 tw:max-w-56 tw:flex-none tw:truncate tw:rounded-xs tw:border tw:border-transparent tw:bg-transparent tw:px-1 tw:py-0.5 tw:font-mono tw:text-[10px] tw:text-dim-foreground tw:outline-none tw:transition tw:duration-300 {state}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_chip_options_include_the_unlisted_current_model() {
        let listed = UiModelOption {
            id: "claude-sonnet-5".into(),
            label: Some("Claude Sonnet 5".into()),
            detail: None,
        };
        let mut model = UiAgentModelView {
            effective: Some("claude-sonnet-5".into()),
            options: vec![listed.clone()],
            loading: false,
        };
        // Listed current model: no duplicate.
        assert_eq!(model_chip_options(&model), vec![listed.clone()]);

        // Unlisted override (or no fetch yet): prepended so the select can
        // show the truth.
        model.effective = Some("my-custom-model".into());
        let options = model_chip_options(&model);
        assert_eq!(options.len(), 2);
        assert_eq!(options[0].id, "my-custom-model");
        assert_eq!(options[0].label, None);
        assert_eq!(options[1], listed);
    }

    #[test]
    fn model_chip_keeps_geometry_across_run_states() {
        for busy in [true, false] {
            let class = model_chip_class(busy);
            assert!(class.contains("tw:text-[10px]"));
            assert!(class.contains("tw:transition"));
        }
        assert!(model_chip_class(true).contains("tw:cursor-default"));
        assert!(model_chip_class(false).contains("tw:cursor-pointer"));
    }

    #[test]
    fn the_cost_says_whether_it_is_an_estimate() {
        assert_eq!(cost_title("~$0.02"), "estimate based on configured rates");
        assert_eq!(cost_title("$0.0042"), "what the provider reported charging");
    }
}
