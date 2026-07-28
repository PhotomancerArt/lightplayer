//! Header Studio-settings gear: agent provider selection + credentials.
//!
//! Renders the core-built [`UiSettingsView`] slice and emits
//! [`SettingsCommand`]s — no merge/provenance/guidance logic lives here
//! (the layered store and the provider-guidance table in
//! `lpa-studio-core` own it). The presentational [`AgentSettingsSection`]
//! is pure so stories can render fixtures.

use dioxus::prelude::*;
use lpa_studio_core::app::settings::model_catalog::filter_models;
use lpa_studio_core::{
    AgentProvider, CatalogModel, LocalModelProbeState, ModelCatalogState, ProbeFinding, ProbeLevel,
    SettingsCommand, SettingsLayer, UiAgentSettingsView, UiSettingsView, adopt_model_commands,
};

use crate::base::{IconPopoverButton, PopoverPlacement, StudioIconName};
use crate::local_model_probe::ProbeRequest;
use crate::model_catalog::CatalogQuery;

/// Header gear button opening the Studio settings popover.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn StudioSettingsPopover(
    settings: UiSettingsView,
    on_settings: EventHandler<SettingsCommand>,
) -> Element {
    // The OpenRouter connect wiring (App-provided context; absent under
    // stories, which render the section directly with fixture props).
    let openrouter_error = try_consume_context::<Signal<Option<String>>>();
    // Local-server discovery state. It lives HERE, not in the section: the
    // popover content unmounts when the popover closes, and a result the
    // user just read should still be there when they reopen it.
    let mut probe = use_signal(LocalModelProbeState::default);
    // The provider's model list, kept across popover open/close for the same
    // reason (and re-fetched whenever the provider or address changes).
    let mut catalog = use_signal(ModelCatalogState::default);
    // The effective model, for the "does this server serve it" check — the
    // placeholder IS the effective model whenever one resolves.
    let configured_model = (!settings.agent.model_missing)
        .then(|| settings.agent.model_placeholder.clone())
        .filter(|_| settings.agent.provider == AgentProvider::Custom);
    let catalog_query = CatalogQuery {
        provider: settings.agent.provider,
        base_url: settings.agent.base_url_effective.clone(),
        // Resolved at click time (the view holds only a masked key).
        api_key: None,
    };
    // Which provider+address the picker is currently about. A loaded list
    // whose fingerprint no longer matches is another provider's answer and
    // must not stay on screen.
    let catalog_fingerprint = lpa_studio_core::app::settings::model_catalog::catalog_fingerprint(
        catalog_query.provider,
        catalog_query.base_url.as_deref(),
    );
    let query_fingerprint = catalog_fingerprint.clone();
    rsx! {
        IconPopoverButton {
            class: TRIGGER_CLASS.to_string(),
            open_class: TRIGGER_OPEN_CLASS.to_string(),
            icon: StudioIconName::Settings,
            icon_size: 15,
            label: "Studio settings".to_string(),
            title: "Studio settings".to_string(),
            popup_class: POPUP_CLASS.to_string(),
            chrome_class: "ux-popover-chrome-neutral".to_string(),
            placement: PopoverPlacement::BottomEnd,
            AgentSettingsSection {
                agent: settings.agent,
                on_settings,
                on_connect: move |_| crate::openrouter_oauth::begin_connect(openrouter_error),
                connect_error: openrouter_error.and_then(|error| error()),
                probe: probe(),
                on_probe: move |request: ProbeRequest| {
                    probe.set(crate::local_model_probe::running_state(&request));
                    let model = configured_model.clone();
                    spawn_probe(probe, request, model);
                },
                catalog: catalog(),
                catalog_fingerprint,
                on_browse: move |open: bool| {
                    if !open {
                        catalog.with_mut(|state| state.open = false);
                        return;
                    }
                    // Opening on an already-loaded list is instant; Refresh
                    // (same handler, already open) always re-asks.
                    if !catalog.peek().needs_load(&query_fingerprint) && !catalog.peek().open {
                        catalog.with_mut(|state| state.open = true);
                        return;
                    }
                    let loading = crate::model_catalog::loading_state(&catalog.peek().clone());
                    catalog.set(loading);
                    spawn_catalog_load(catalog, catalog_query.clone());
                },
            }
        }
    }
}

/// Run a probe request into the state signal. wasm-only: host builds (unit
/// tests, stories) render fixtures and never probe, so the request simply
/// leaves the in-flight state showing there.
#[cfg_attr(
    not(target_arch = "wasm32"),
    allow(unused_variables, reason = "host builds never probe")
)]
fn spawn_probe(
    #[allow(unused_mut, reason = "only the wasm body writes the signal")] mut probe: Signal<
        LocalModelProbeState,
    >,
    request: ProbeRequest,
    model: Option<String>,
) {
    #[cfg(target_arch = "wasm32")]
    {
        // The key the agent itself would send, resolved from the two layers
        // this edge owns — the view carries only a masked preview, and a
        // server that wants a key must be tested with it.
        let api_key = crate::settings_io::effective_custom_api_key();
        wasm_bindgen_futures::spawn_local(async move {
            let state = crate::local_model_probe::run(request, api_key, model).await;
            probe.set(state);
        });
    }
}

/// Load the selected provider's model list into the picker's state. Like
/// [`spawn_probe`], wasm-only: stories render fixtures.
#[cfg_attr(
    not(target_arch = "wasm32"),
    allow(unused_variables, reason = "host builds never fetch")
)]
fn spawn_catalog_load(
    #[allow(unused_mut, reason = "only the wasm body writes the signal")] mut catalog: Signal<
        ModelCatalogState,
    >,
    query: CatalogQuery,
) {
    #[cfg(target_arch = "wasm32")]
    {
        let query = CatalogQuery {
            api_key: crate::settings_io::effective_api_key(query.provider),
            ..query
        };
        wasm_bindgen_futures::spawn_local(async move {
            let state = crate::model_catalog::load(query).await;
            catalog.set(state);
        });
    }
}

/// Pure presentation of the agent settings (provider, credentials, model,
/// cost rates), driven entirely by the [`UiAgentSettingsView`] DTO.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn AgentSettingsSection(
    agent: UiAgentSettingsView,
    on_settings: EventHandler<SettingsCommand>,
    /// Starts the OpenRouter PKCE redirect (absent under stories ⇒ the
    /// Connect button renders inert).
    #[props(default)]
    on_connect: Option<EventHandler<()>>,
    /// Transient connect-flow failure to surface under the button.
    #[props(default)]
    connect_error: Option<String>,
    /// Local-server discovery state (Custom provider only).
    #[props(default)]
    probe: LocalModelProbeState,
    /// Test / Scan requests (absent under stories ⇒ the buttons are inert).
    #[props(default)]
    on_probe: Option<EventHandler<ProbeRequest>>,
    /// The provider's model list, for the Model field's picker.
    #[props(default)]
    catalog: ModelCatalogState,
    /// Identity of the provider+address the picker is about, so a list
    /// loaded for a different one is not shown as this one's.
    #[props(default)]
    catalog_fingerprint: String,
    /// Open (`true`, also = refresh) or close the picker (absent under
    /// stories ⇒ the toggle is inert).
    #[props(default)]
    on_browse: Option<EventHandler<bool>>,
) -> Element {
    // Uncontrolled-input mirror for the base-URL field (see its `oninput`):
    // presentational state only — the committed value stays the prop.
    let mut base_url_draft = use_signal(|| None::<String>);
    let key_hint = key_provenance_hint(&agent);
    let model_hint = model_provenance_hint(&agent);
    let model_value = agent.model_override.clone().unwrap_or_default();
    let base_url_value = agent.base_url_override.clone().unwrap_or_default();
    // What Test probes when the field has not been touched this session.
    let base_url_effective = agent.base_url_effective.clone();
    let price_input_value = agent.price_input_override.clone().unwrap_or_default();
    let price_output_value = agent.price_output_override.clone().unwrap_or_default();
    let provider = agent.provider;
    let key_field_provider = provider;
    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:gap-3 tw:p-3",
            div { class: "tw:grid tw:min-w-0 tw:gap-0.5",
                strong { class: "tw:text-sm tw:text-strong-foreground", "Agent settings" }
                span { class: "tw:text-xs tw:font-bold tw:text-subtle-foreground",
                    "Model access for the shader agent"
                }
            }

            // Provider selection.
            div { class: "tw:grid tw:min-w-0 tw:gap-1",
                span { class: LABEL_CLASS, "Provider" }
                div {
                    class: "tw:flex tw:overflow-hidden tw:rounded-sm tw:border tw:border-border-strong",
                    role: "radiogroup",
                    for candidate in AgentProvider::ALL {
                        button {
                            key: "{candidate.label()}",
                            class: provider_button_class(candidate == provider),
                            r#type: "button",
                            role: "radio",
                            aria_checked: "{candidate == provider}",
                            onclick: move |_| {
                                on_settings.call(SettingsCommand::SetAgentProvider(Some(candidate)));
                            },
                            "{candidate.label()}"
                        }
                    }
                }
                if agent.provider_layer == SettingsLayer::Host && !agent.provider_overridden {
                    span { class: HINT_CLASS, "from dev settings" }
                }
            }

            // Per-provider onboarding guidance (core-supplied copy).
            div { class: "tw:grid tw:min-w-0 tw:gap-1 tw:rounded-sm tw:bg-card-muted tw:px-2 tw:py-1.5",
                p { class: "tw:m-0 tw:text-[11px] tw:leading-snug tw:text-muted-foreground",
                    "{agent.guidance.setup}"
                }
                if let Some(note) = agent.guidance.note {
                    p { class: "tw:m-0 tw:text-[11px] tw:leading-snug tw:text-dim-foreground", "{note}" }
                }
                div { class: "tw:flex tw:flex-wrap tw:gap-x-3",
                    for (label , url) in agent.guidance.links {
                        a {
                            key: "{url}",
                            class: "tw:text-[11px] tw:text-accent tw:underline",
                            href: "{url}",
                            target: "_blank",
                            rel: "noopener noreferrer",
                            "{label}"
                        }
                    }
                }
            }

            // Base URL + local-server discovery (Custom provider only).
            if provider == AgentProvider::Custom {
                div { class: "tw:grid tw:min-w-0 tw:gap-1",
                    label { class: LABEL_CLASS, r#for: "studio-settings-base-url", "Base URL" }
                    input {
                        id: "studio-settings-base-url",
                        class: INPUT_CLASS,
                        r#type: "text",
                        value: "{base_url_value}",
                        placeholder: base_url_placeholder(&agent),
                        autocomplete: "off",
                        spellcheck: "false",
                        // The draft mirrors keystrokes so Test probes what
                        // the user is looking at, not the last committed
                        // value (the field commits on blur / Enter).
                        oninput: move |event| base_url_draft.set(Some(event.value())),
                        onchange: move |event| {
                            on_settings.call(SettingsCommand::SetAgentCustomBaseUrl(Some(event.value())));
                        },
                    }
                    div { class: "tw:flex tw:min-w-0 tw:items-baseline tw:gap-2",
                        if agent.base_url_layer == SettingsLayer::Host && agent.base_url_override.is_none() {
                            span { class: HINT_CLASS, "from dev settings" }
                        }
                        if agent.base_url_override.is_some() {
                            button {
                                class: CLEAR_BUTTON_CLASS,
                                r#type: "button",
                                title: "Clear the base URL saved in this browser",
                                onclick: move |_| on_settings.call(SettingsCommand::SetAgentCustomBaseUrl(None)),
                                "Clear"
                            }
                        }
                    }

                    // Test / Scan: the two questions a local setup raises —
                    // "does this address work?" and "where is my server?".
                    div { class: "tw:flex tw:min-w-0 tw:items-center tw:gap-2 tw:pt-0.5",
                        button {
                            class: PROBE_BUTTON_CLASS,
                            r#type: "button",
                            disabled: probe.running,
                            title: "Ask this server for its model list",
                            onclick: move |_| {
                                if let Some(on_probe) = on_probe {
                                    let url = base_url_draft()
                                        .or_else(|| base_url_effective.clone())
                                        .unwrap_or_default();
                                    on_probe.call(ProbeRequest::TestBaseUrl(url));
                                }
                            },
                            "Test connection"
                        }
                        button {
                            class: PROBE_BUTTON_CLASS,
                            r#type: "button",
                            disabled: probe.running,
                            title: "Try the ports Ollama, LM Studio, llama.cpp, vLLM and friends use",
                            onclick: move |_| {
                                if let Some(on_probe) = on_probe {
                                    on_probe.call(ProbeRequest::ScanCommonPorts);
                                }
                            },
                            "Find my server"
                        }
                    }
                    if let Some(running) = probe.running_label.as_deref() {
                        span { class: "tw:text-[11px] tw:text-status-working-foreground", "{running}" }
                    }
                    if let Some(summary) = probe.summary.as_ref() {
                        div { class: "tw:grid tw:min-w-0 tw:gap-0.5",
                            span { class: "tw:text-[11px] tw:font-bold {level_text_class(summary.level)}",
                                "{summary.headline}"
                            }
                            if let Some(detail) = summary.detail.as_deref() {
                                p { class: "tw:m-0 tw:text-[11px] tw:leading-snug tw:text-dim-foreground", "{detail}" }
                            }
                        }
                    }
                    for finding in probe.findings.iter() {
                        ProbeFindingCard {
                            key: "{finding.base_url}",
                            finding: finding.clone(),
                            on_settings,
                        }
                    }
                }
            }

            // Account access: OpenRouter connects via OAuth (no pasted
            // key); every other provider takes a pasted API key.
            if provider == AgentProvider::OpenRouter {
                div { class: "tw:grid tw:min-w-0 tw:gap-1",
                    span { class: LABEL_CLASS, "Account" }
                    if let Some(masked) = agent.api_key_masked.as_deref() {
                        div { class: "tw:flex tw:min-w-0 tw:items-baseline tw:gap-2",
                            code { class: "tw:font-mono tw:text-xs tw:text-muted-foreground", "{masked}" }
                            if let Some(hint) = key_hint {
                                span { class: HINT_CLASS, "{hint}" }
                            }
                            button {
                                class: CLEAR_BUTTON_CLASS,
                                r#type: "button",
                                title: "Forget this key (also revocable at openrouter.ai)",
                                onclick: move |_| {
                                    on_settings
                                        .call(SettingsCommand::SetAgentOpenRouterApiKey(None));
                                },
                                "Disconnect"
                            }
                        }
                    } else {
                        button {
                            class: CONNECT_BUTTON_CLASS,
                            r#type: "button",
                            title: "Sign in on openrouter.ai and come right back — no key to paste",
                            onclick: move |_| {
                                if let Some(on_connect) = on_connect {
                                    on_connect.call(());
                                }
                            },
                            "Connect OpenRouter"
                        }
                    }
                    if let Some(error) = connect_error.as_deref() {
                        span { class: "tw:text-[0.68rem] tw:font-bold tw:text-status-warning-foreground",
                            "{error}"
                        }
                    }
                    p { class: "tw:m-0 tw:text-[11px] tw:leading-snug tw:text-dim-foreground",
                        "Saved in this browser only, unencrypted."
                    }
                }
            } else {
                // API key for the selected provider.
                div { class: "tw:grid tw:min-w-0 tw:gap-1",
                    label { class: LABEL_CLASS, r#for: "studio-settings-api-key",
                        if agent.api_key_optional { "API key (optional)" } else { "API key" }
                    }
                    if let Some(masked) = agent.api_key_masked.as_deref() {
                        div { class: "tw:flex tw:min-w-0 tw:items-baseline tw:gap-2",
                            code { class: "tw:font-mono tw:text-xs tw:text-muted-foreground", "{masked}" }
                            if let Some(hint) = key_hint {
                                span { class: HINT_CLASS, "{hint}" }
                            }
                            if agent.api_key_overridden {
                                button {
                                    class: CLEAR_BUTTON_CLASS,
                                    r#type: "button",
                                    title: "Remove the key saved in this browser",
                                    onclick: move |_| on_settings.call(set_key_command(key_field_provider, None)),
                                    "Clear saved key"
                                }
                            }
                        }
                    }
                    input {
                        id: "studio-settings-api-key",
                        class: INPUT_CLASS,
                        r#type: "password",
                        placeholder: api_key_placeholder(&agent),
                        autocomplete: "off",
                        // paste-and-save: the value commits on change (blur /
                        // Enter) and the field resets to its masked summary on
                        // the next snapshot
                        onchange: move |event| {
                            on_settings.call(set_key_command(key_field_provider, Some(event.value())));
                        },
                    }
                    p { class: "tw:m-0 tw:text-[11px] tw:leading-snug tw:text-dim-foreground",
                        "Saved in this browser only, unencrypted."
                    }
                }
            }

            // Model id — typed, or picked from the provider's own list.
            div { class: "tw:grid tw:min-w-0 tw:gap-1",
                div { class: "tw:flex tw:min-w-0 tw:items-baseline tw:justify-between tw:gap-2",
                    label { class: LABEL_CLASS, r#for: "studio-settings-model", "Model" }
                    button {
                        class: PROBE_BUTTON_CLASS,
                        r#type: "button",
                        aria_expanded: "{catalog.open}",
                        title: "List the models this provider serves",
                        onclick: move |_| {
                            if let Some(on_browse) = on_browse {
                                on_browse.call(!catalog.open);
                            }
                        },
                        if catalog.open { "Hide models" } else { "Browse models ▾" }
                    }
                }
                input {
                    id: "studio-settings-model",
                    class: INPUT_CLASS,
                    r#type: "text",
                    value: "{model_value}",
                    placeholder: "{agent.model_placeholder}",
                    autocomplete: "off",
                    spellcheck: "false",
                    onchange: move |event| {
                        on_settings.call(SettingsCommand::SetAgentModel(Some(event.value())));
                    },
                }
                if catalog.open {
                    ModelPicker {
                        catalog: catalog.clone(),
                        fingerprint: catalog_fingerprint.clone(),
                        current_model: agent.model_placeholder.clone(),
                        model_missing: agent.model_missing,
                        on_settings,
                        on_refresh: move |_| {
                            if let Some(on_browse) = on_browse {
                                on_browse.call(true);
                            }
                        },
                    }
                }
                div { class: "tw:flex tw:min-w-0 tw:items-baseline tw:gap-2",
                    if agent.model_missing {
                        span { class: "tw:text-[0.68rem] tw:font-bold tw:text-status-warning-foreground",
                            "required — the agent stays off until a model id is set"
                        }
                    }
                    if let Some(hint) = model_hint {
                        span { class: HINT_CLASS, "{hint}" }
                    }
                    if agent.model_override.is_some() {
                        button {
                            class: CLEAR_BUTTON_CLASS,
                            r#type: "button",
                            title: "Clear the model override",
                            onclick: move |_| on_settings.call(SettingsCommand::SetAgentModel(None)),
                            if agent.model_default.is_some() { "Use default" } else { "Clear" }
                        }
                    }
                }
            }

            // Cost-estimate rate overrides.
            div { class: "tw:grid tw:min-w-0 tw:gap-1",
                span { class: LABEL_CLASS, "Cost estimate rates ($/MTok)" }
                div { class: "tw:flex tw:min-w-0 tw:items-center tw:gap-2",
                    input {
                        class: "{INPUT_CLASS} tw:flex-1",
                        r#type: "text",
                        inputmode: "decimal",
                        value: "{price_input_value}",
                        placeholder: "in",
                        autocomplete: "off",
                        title: "$ per million input tokens",
                        onchange: move |event| {
                            on_settings.call(SettingsCommand::SetAgentPriceInputPerMtok(Some(event.value())));
                        },
                    }
                    input {
                        class: "{INPUT_CLASS} tw:flex-1",
                        r#type: "text",
                        inputmode: "decimal",
                        value: "{price_output_value}",
                        placeholder: "out",
                        autocomplete: "off",
                        title: "$ per million output tokens",
                        onchange: move |event| {
                            on_settings.call(SettingsCommand::SetAgentPriceOutputPerMtok(Some(event.value())));
                        },
                    }
                }
                p { class: "tw:m-0 tw:text-[11px] tw:leading-snug tw:text-dim-foreground",
                    "Powers the ~$ estimate in the chat. Filled in when you pick a model that "
                    "publishes rates; blank uses built-in rates for known models."
                }
            }
        }
    }
}

/// The provider's own model list: filter, pick, refresh.
///
/// The typed field stays the source of truth (ids the picker filters out, or
/// a provider that lists nothing, must remain reachable) — this only fills
/// it in.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ModelPicker(
    catalog: ModelCatalogState,
    /// The provider+address this picker is about; a catalog loaded for
    /// another one is treated as absent (switching provider with the picker
    /// open must not leave the old provider's models listed).
    fingerprint: String,
    /// The effective model id, highlighted in the list.
    current_model: String,
    /// No model is configured yet (so nothing is highlighted).
    model_missing: bool,
    on_settings: EventHandler<SettingsCommand>,
    on_refresh: EventHandler<()>,
) -> Element {
    let mut query = use_signal(String::new);
    let query_text = query();
    let stale = catalog
        .loaded_for
        .as_deref()
        .is_some_and(|loaded| loaded != fingerprint);
    let models = catalog
        .catalog
        .as_ref()
        .filter(|_| !stale)
        .map(|catalog| &catalog.models);
    let total = models.map_or(0, Vec::len);
    // The query only applies while its box is on screen. Without this, a
    // query typed against a long list survives a refresh onto a short one
    // and hides everything with no visible way to clear it.
    let filterable = total > FILTERABLE_MODEL_COUNT;
    // Whether this provider published the scores the order is built on (only
    // OpenRouter does today) — the legend would be a lie otherwise.
    let ranked = models.is_some_and(|models| models.iter().any(|m| m.coding_score.is_some()));
    let matches = models
        .map(|models| {
            if filterable {
                filter_models(models, &query_text)
            } else {
                models.iter().collect()
            }
        })
        .unwrap_or_default();
    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:gap-1 tw:rounded-sm tw:border tw:border-border-strong tw:bg-card-muted tw:p-1.5",
            if catalog.loading {
                span { class: "tw:text-[11px] tw:text-status-working-foreground", "Asking the provider…" }
            }
            if stale && !catalog.loading {
                div { class: "tw:grid tw:min-w-0 tw:gap-1",
                    span { class: HINT_CLASS, "The list you loaded belongs to another provider." }
                    button {
                        class: "{CLEAR_BUTTON_CLASS} tw:justify-self-start",
                        r#type: "button",
                        onclick: move |_| on_refresh.call(()),
                        "List this provider's models"
                    }
                }
            }
            if let Some(error) = catalog.error.as_deref() {
                div { class: "tw:grid tw:min-w-0 tw:gap-1",
                    span { class: "tw:text-[11px] tw:leading-snug tw:text-status-warning-foreground",
                        "{error}"
                    }
                    button {
                        class: "{CLEAR_BUTTON_CLASS} tw:justify-self-start",
                        r#type: "button",
                        onclick: move |_| on_refresh.call(()),
                        "Try again"
                    }
                }
            }
            // A search box only earns its space once the list is long enough
            // to need one (OpenRouter lists hundreds; Anthropic a handful).
            if filterable {
                input {
                    class: INPUT_CLASS,
                    r#type: "text",
                    value: "{query_text}",
                    placeholder: "Filter {total} models",
                    autocomplete: "off",
                    spellcheck: "false",
                    oninput: move |event| query.set(event.value()),
                }
            }
            if models.is_some() {
                if matches.is_empty() {
                    span { class: HINT_CLASS,
                        if total == 0 { "This provider listed no models." } else { "Nothing matches that filter." }
                    }
                } else {
                    if ranked {
                        span { class: HINT_CLASS, "best for code first, by the provider's own score" }
                    }
                    div { class: "tw:grid tw:max-h-52 tw:min-w-0 tw:gap-0.5 tw:overflow-y-auto",
                        for model in matches.iter().take(MAX_PICKER_ROWS) {
                            button {
                                key: "{model.id}",
                                class: model_row_class(!model_missing && model.id == current_model),
                                r#type: "button",
                                title: "Use this model",
                                // Adopting a model also carries its rates
                                // (or clears the last model's).
                                onclick: {
                                    let model = (*model).clone();
                                    move |_| {
                                        for command in adopt_model_commands(&model) {
                                            on_settings.call(command);
                                        }
                                    }
                                },
                                span { class: "tw:min-w-0 tw:truncate tw:font-mono tw:text-[11px]", "{model.id}" }
                                // The score is why this row sits where it
                                // does; showing it keeps the order legible.
                                if let Some(score) = model.coding_score {
                                    span { class: "tw:shrink-0 tw:text-[10px] tw:text-status-good-foreground",
                                        "{score:.0}"
                                    }
                                }
                                if let Some(price) = model.price {
                                    span { class: "tw:shrink-0 tw:text-[10px] tw:text-subtle-foreground", "{price.summary()}" }
                                }
                            }
                        }
                    }
                    if matches.len() > MAX_PICKER_ROWS {
                        span { class: HINT_CLASS,
                            "showing {MAX_PICKER_ROWS} of {matches.len()} — keep typing to narrow it"
                        }
                    }
                }
            }
            div { class: "tw:flex tw:min-w-0 tw:items-baseline tw:justify-between tw:gap-2",
                if let Some(hidden) = catalog.catalog.as_ref().filter(|_| !stale).map(|c| c.hidden).filter(|h| *h > 0) {
                    span { class: HINT_CLASS,
                        "{hidden} {plural(hidden, \"model\")} hidden — no tool calling, or not chat"
                    }
                }
                if catalog.catalog.is_some() && !stale && !catalog.loading {
                    button {
                        class: "{CLEAR_BUTTON_CLASS} tw:ml-auto",
                        r#type: "button",
                        title: "Ask the provider again",
                        onclick: move |_| on_refresh.call(()),
                        "Refresh"
                    }
                }
            }
        }
    }
}

/// One probed address: what happened, how to fix it, and — when it works —
/// one-click adoption of the server and model it serves.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ProbeFindingCard(finding: ProbeFinding, on_settings: EventHandler<SettingsCommand>) -> Element {
    let base_url = finding.base_url.clone();
    let adopt_url = base_url.clone();
    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:gap-1 tw:rounded-sm tw:border {level_border_class(finding.level)} tw:bg-card-muted tw:px-2 tw:py-1.5",
            div { class: "tw:flex tw:min-w-0 tw:flex-wrap tw:items-baseline tw:gap-x-2",
                code { class: "tw:font-mono tw:text-[11px] tw:text-muted-foreground", "{base_url}" }
                if let Some(label) = finding.server_label.as_deref() {
                    span { class: HINT_CLASS, "{label}" }
                }
            }
            span { class: "tw:text-[11px] tw:font-bold tw:leading-snug {level_text_class(finding.level)}",
                "{finding.headline}"
            }
            if let Some(detail) = finding.detail.as_deref() {
                p { class: "tw:m-0 tw:text-[11px] tw:leading-snug tw:text-dim-foreground", "{detail}" }
            }
            if let Some(fix) = finding.fix.as_deref() {
                p { class: "tw:m-0 tw:font-mono tw:text-[11px] tw:leading-snug tw:text-soft-foreground tw:select-all tw:[overflow-wrap:anywhere]",
                    "{fix}"
                }
            }
            // A model chip adopts the whole answer in one click: this
            // server's address AND the id it actually serves.
            if !finding.models.is_empty() {
                div { class: "tw:flex tw:min-w-0 tw:flex-wrap tw:gap-1",
                    for model in finding.models.iter().take(MAX_LISTED_MODELS) {
                        button {
                            key: "{model}",
                            class: MODEL_CHIP_CLASS,
                            r#type: "button",
                            title: "Use this server and model",
                            onclick: {
                                let url = base_url.clone();
                                // A local server publishes no rates, so this
                                // also clears any carried over from a priced
                                // model (see `adopt_model_commands`).
                                let model = CatalogModel::new(model.clone());
                                move |_| {
                                    on_settings.call(SettingsCommand::SetAgentCustomBaseUrl(Some(url.clone())));
                                    for command in adopt_model_commands(&model) {
                                        on_settings.call(command);
                                    }
                                }
                            },
                            "{model}"
                        }
                    }
                    if finding.models.len() > MAX_LISTED_MODELS {
                        span { class: HINT_CLASS, "+{finding.models.len() - MAX_LISTED_MODELS} more" }
                    }
                }
            } else if finding.level != ProbeLevel::Error {
                // Nothing to pick, but the address itself is worth keeping
                // (the CORS case: right port, one server setting away).
                button {
                    class: "{CLEAR_BUTTON_CLASS} tw:justify-self-start",
                    r#type: "button",
                    title: "Save this address as the base URL",
                    onclick: move |_| {
                        on_settings.call(SettingsCommand::SetAgentCustomBaseUrl(Some(adopt_url.clone())));
                    },
                    "Use this address"
                }
            }
        }
    }
}

const TRIGGER_CLASS: &str = "tw:inline-flex tw:h-7 tw:w-7 tw:items-center tw:justify-center tw:rounded-full tw:border tw:border-status-neutral-border tw:bg-status-neutral-bg tw:p-0 tw:text-status-neutral-foreground";
const TRIGGER_OPEN_CLASS: &str = "tw:inline-flex tw:h-7 tw:w-7 tw:items-center tw:justify-center tw:rounded-full tw:border tw:border-status-neutral-border tw:bg-card-raised tw:p-0 tw:text-status-neutral-foreground";
const POPUP_CLASS: &str = "tw:grid tw:w-[min(340px,calc(100vw-24px))] tw:overflow-hidden tw:rounded-md tw:border tw:border-status-neutral-border tw:bg-card tw:bg-[linear-gradient(90deg,var(--studio-status-neutral-bg),transparent_74%)] tw:text-sm tw:text-muted-foreground tw:shadow-lg";
const INPUT_CLASS: &str = "tw:h-7 tw:w-full tw:rounded-sm tw:border tw:border-border-strong tw:bg-card-muted tw:px-1.5 tw:font-mono tw:text-xs tw:text-muted-foreground";
const CLEAR_BUTTON_CLASS: &str = "tw:rounded-sm tw:border tw:border-border-strong tw:bg-card-muted tw:px-1.5 tw:py-0.5 tw:text-[11px] tw:text-muted-foreground tw:hover:text-soft-foreground";
const CONNECT_BUTTON_CLASS: &str = "tw:justify-self-start tw:cursor-pointer tw:rounded-xs tw:border tw:border-accent-border tw:bg-transparent tw:px-3 tw:py-1.5 tw:text-xs tw:font-bold tw:text-accent tw:transition tw:duration-300 tw:hover:bg-accent-wash";
const LABEL_CLASS: &str = "tw:text-[0.68rem] tw:font-bold tw:uppercase tw:text-subtle-foreground";
const HINT_CLASS: &str = "tw:text-[0.68rem] tw:text-subtle-foreground";
const PROBE_BUTTON_CLASS: &str = "tw:cursor-pointer tw:rounded-xs tw:border tw:border-border-strong tw:bg-card-muted tw:px-2 tw:py-1 tw:text-[11px] tw:font-bold tw:text-muted-foreground tw:transition tw:duration-200 tw:hover:text-strong-foreground tw:disabled:cursor-default tw:disabled:text-dim-foreground";
const MODEL_CHIP_CLASS: &str = "tw:cursor-pointer tw:rounded-xs tw:border tw:border-accent-border tw:bg-transparent tw:px-1.5 tw:py-0.5 tw:font-mono tw:text-[11px] tw:text-accent tw:hover:bg-accent-wash";
/// How many served model ids a finding lists before collapsing the rest.
const MAX_LISTED_MODELS: usize = 6;
/// Above this many models the picker earns a filter box.
const FILTERABLE_MODEL_COUNT: usize = 8;
/// How many picker rows render at once (OpenRouter lists hundreds; the
/// filter is the way through them, not a thousand-row DOM).
const MAX_PICKER_ROWS: usize = 40;

/// `model` / `models` for a count.
fn plural(count: usize, noun: &str) -> String {
    if count == 1 {
        noun.to_string()
    } else {
        format!("{noun}s")
    }
}

/// One picker row; the configured model reads as chosen.
fn model_row_class(current: bool) -> &'static str {
    if current {
        "tw:flex tw:min-w-0 tw:cursor-default tw:items-baseline tw:justify-between tw:gap-2 tw:rounded-xs tw:border tw:border-accent-border tw:bg-accent-wash tw:px-1.5 tw:py-0.5 tw:text-left tw:text-accent"
    } else {
        "tw:flex tw:min-w-0 tw:cursor-pointer tw:items-baseline tw:justify-between tw:gap-2 tw:rounded-xs tw:border tw:border-transparent tw:bg-transparent tw:px-1.5 tw:py-0.5 tw:text-left tw:text-muted-foreground tw:hover:border-border-strong tw:hover:text-strong-foreground"
    }
}

/// Status foreground for a probe level.
fn level_text_class(level: ProbeLevel) -> &'static str {
    match level {
        ProbeLevel::Ok => "tw:text-status-good-foreground",
        ProbeLevel::Warn => "tw:text-status-warning-foreground",
        ProbeLevel::Error => "tw:text-status-error-foreground",
    }
}

/// Status border for a finding card.
fn level_border_class(level: ProbeLevel) -> &'static str {
    match level {
        ProbeLevel::Ok => "tw:border-status-good-border",
        ProbeLevel::Warn => "tw:border-status-warning-border",
        ProbeLevel::Error => "tw:border-status-error-border",
    }
}

/// One segmented provider button.
fn provider_button_class(active: bool) -> &'static str {
    if active {
        "tw:flex-1 tw:cursor-default tw:border-0 tw:bg-card-subtle tw:px-2 tw:py-1 tw:text-xs tw:font-bold tw:text-strong-foreground"
    } else {
        "tw:flex-1 tw:cursor-pointer tw:border-0 tw:bg-transparent tw:px-2 tw:py-1 tw:text-xs tw:text-muted-foreground tw:hover:bg-card-muted tw:hover:text-strong-foreground"
    }
}

/// The key mutation for the selected provider's key field.
fn set_key_command(provider: AgentProvider, value: Option<String>) -> SettingsCommand {
    match provider {
        AgentProvider::Anthropic => SettingsCommand::SetAgentAnthropicApiKey(value),
        AgentProvider::OpenAi => SettingsCommand::SetAgentOpenAiApiKey(value),
        AgentProvider::Custom => SettingsCommand::SetAgentCustomApiKey(value),
        AgentProvider::OpenRouter => SettingsCommand::SetAgentOpenRouterApiKey(value),
    }
}

/// Provenance line for the masked key summary.
fn key_provenance_hint(agent: &UiAgentSettingsView) -> Option<&'static str> {
    match agent.api_key_layer {
        SettingsLayer::Host => Some("from dev settings"),
        SettingsLayer::User => Some("saved in this browser"),
        SettingsLayer::Default => None,
    }
}

/// Provenance line under the model input (only shown while no user
/// override exists — an override speaks for itself as the input value).
fn model_provenance_hint(agent: &UiAgentSettingsView) -> Option<&'static str> {
    if agent.model_override.is_some() {
        return None;
    }
    match agent.model_layer {
        SettingsLayer::Host => Some("from dev settings"),
        SettingsLayer::Default | SettingsLayer::User => None,
    }
}

/// The key input's placeholder: invite a paste, or signal replaceability
/// when a key already exists.
fn api_key_placeholder(agent: &UiAgentSettingsView) -> &'static str {
    if agent.api_key_masked.is_some() {
        return "Paste a key to replace";
    }
    match agent.provider {
        AgentProvider::Anthropic => "sk-ant-…",
        AgentProvider::OpenAi => "sk-…",
        AgentProvider::Custom => "usually not needed",
        AgentProvider::OpenRouter => "use Connect below",
    }
}

/// The base-URL input's placeholder: the host-supplied value when one
/// exists, the Ollama example otherwise.
fn base_url_placeholder(agent: &UiAgentSettingsView) -> String {
    match (&agent.base_url_effective, &agent.base_url_override) {
        (Some(effective), None) => effective.clone(),
        _ => "http://localhost:11434/v1".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent_view() -> UiAgentSettingsView {
        UiSettingsView::default().agent
    }

    #[test]
    fn key_hints_follow_provenance() {
        let mut agent = agent_view();
        assert_eq!(key_provenance_hint(&agent), None);
        agent.api_key_layer = SettingsLayer::Host;
        assert_eq!(key_provenance_hint(&agent), Some("from dev settings"));
        agent.api_key_layer = SettingsLayer::User;
        assert_eq!(key_provenance_hint(&agent), Some("saved in this browser"));
    }

    #[test]
    fn model_hint_only_shows_for_host_without_override() {
        let mut agent = agent_view();
        assert_eq!(model_provenance_hint(&agent), None);
        agent.model_layer = SettingsLayer::Host;
        assert_eq!(model_provenance_hint(&agent), Some("from dev settings"));
        agent.model_override = Some("user-model".to_string());
        agent.model_layer = SettingsLayer::User;
        assert_eq!(model_provenance_hint(&agent), None);
    }

    #[test]
    fn key_placeholder_tracks_provider_and_existing_key() {
        let mut agent = agent_view();
        assert_eq!(api_key_placeholder(&agent), "sk-ant-…");
        agent.provider = AgentProvider::OpenAi;
        assert_eq!(api_key_placeholder(&agent), "sk-…");
        agent.provider = AgentProvider::Custom;
        assert_eq!(api_key_placeholder(&agent), "usually not needed");
        agent.api_key_masked = Some("•••••mnop".to_string());
        assert_eq!(api_key_placeholder(&agent), "Paste a key to replace");
    }

    #[test]
    fn key_commands_route_to_the_selected_provider_field() {
        assert!(matches!(
            set_key_command(AgentProvider::Anthropic, Some("k".into())),
            SettingsCommand::SetAgentAnthropicApiKey(Some(_))
        ));
        assert!(matches!(
            set_key_command(AgentProvider::OpenAi, None),
            SettingsCommand::SetAgentOpenAiApiKey(None)
        ));
        assert!(matches!(
            set_key_command(AgentProvider::Custom, Some("k".into())),
            SettingsCommand::SetAgentCustomApiKey(Some(_))
        ));
    }

    #[test]
    fn base_url_placeholder_prefers_the_host_value() {
        let mut agent = agent_view();
        assert_eq!(base_url_placeholder(&agent), "http://localhost:11434/v1");
        agent.base_url_effective = Some("http://box:8080/v1".to_string());
        assert_eq!(base_url_placeholder(&agent), "http://box:8080/v1");
        // With a user override, the input has a value; the example returns.
        agent.base_url_override = Some("http://box:8080/v1".to_string());
        assert_eq!(base_url_placeholder(&agent), "http://localhost:11434/v1");
    }
}
