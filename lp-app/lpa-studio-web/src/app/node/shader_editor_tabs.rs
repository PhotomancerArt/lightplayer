//! The shader editor region's `Agent | Code` tab strip.
//!
//! Mounted by the config-slot row wherever a GLSL inline editor carries an
//! agent chat DTO ([`UiAssetEditor::agent`]). Tab selection is view-local
//! (editing-model ADR D9 spirit — like editor text and the chat draft, it
//! never enters core state). The default tab follows availability: Agent
//! when a key is configured, Code otherwise.
//!
//! The wrapper also owns the content fetch while the Code tab (and its
//! [`AssetEditor`]) is unmounted: the agent needs the resolved source too,
//! so an unresolved body dispatches the same guarded fetch the editor
//! would.

use dioxus::prelude::*;
use lpa_studio_core::{UiAction, UiAgentAvailability, UiAssetEditor as UiAssetEditorData};
use std::cell::RefCell;
use std::rc::Rc;

use crate::app::node::AssetEditor;
use crate::app::node::agent_chat::AgentChatPane;
use crate::base::Platform;

/// Which editor-region tab is showing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShaderEditorTab {
    Agent,
    Code,
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ShaderEditorTabs(
    editor: UiAssetEditorData,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
    /// Pin the initial tab (stories); defaults from agent availability.
    #[props(default = None)]
    initial_tab: Option<ShaderEditorTab>,
    /// Platform for shortcut-hint display; stories pin it.
    #[props(default)]
    platform: Option<Platform>,
    /// Open tool rows expanded on first render (stories).
    #[props(default = false)]
    tool_rows_expanded: bool,
) -> Element {
    let Some(agent) = editor.agent.clone() else {
        // No agent DTO (non-decorated contexts): plain editor, no strip.
        return rsx! {
            AssetEditor { editor, on_action, platform }
        };
    };

    let default_tab = initial_tab.unwrap_or(match agent.availability {
        UiAgentAvailability::Ready => ShaderEditorTab::Agent,
        UiAgentAvailability::NeedsKey => ShaderEditorTab::Code,
    });
    let mut active = use_signal(|| default_tab);
    // An explicit tab click wins over availability-driven defaults.
    let user_touched = use_hook(|| Rc::new(RefCell::new(false)));
    // Configuring a key mid-session flips the default to Agent (the
    // "key set → Agent tab appears" flow), unless the user chose a tab.
    let last_availability = use_hook(|| Rc::new(RefCell::new(agent.availability)));
    {
        let mut last = last_availability.borrow_mut();
        if *last != agent.availability {
            *last = agent.availability;
            if agent.availability == UiAgentAvailability::Ready
                && !*user_touched.borrow()
                && initial_tab.is_none()
            {
                active.set(ShaderEditorTab::Agent);
            }
        }
    }

    // The agent tab needs the resolved source as much as the editor does;
    // while the AssetEditor is unmounted this wrapper dispatches the same
    // guarded fetch (once per artifact until it resolves).
    let fetch_requested = use_hook(|| Rc::new(RefCell::new(None::<String>)));
    if editor.content.is_some() {
        *fetch_requested.borrow_mut() = None;
    } else {
        let uri = editor.artifact.to_uri();
        let mut requested = fetch_requested.borrow_mut();
        if requested.as_deref() != Some(uri.as_str()) {
            *requested = Some(uri);
            if let Some(handler) = on_action {
                let fetch = editor.fetch_action();
                spawn(async move {
                    handler.call(fetch);
                });
            }
        }
    }

    // The unsaved-edit affordance must read from the Agent tab too: the
    // Code tab label wears the standard warning dirty dot whenever the
    // applied content is unsaved (same signal the save panel's "Unsaved"
    // word uses).
    let dirty = editor.content.as_ref().is_some_and(|content| content.dirty);

    rsx! {
        section { class: "tw:grid tw:min-w-0 tw:border-t tw:border-border-muted",
            div {
                class: "tw:flex tw:items-stretch tw:overflow-hidden tw:border-b tw:border-border-muted tw:bg-card-muted",
                role: "tablist",
                TabButton {
                    label: "Agent",
                    active: active() == ShaderEditorTab::Agent,
                    on_select: {
                        let user_touched = Rc::clone(&user_touched);
                        move |_| {
                            *user_touched.borrow_mut() = true;
                            active.set(ShaderEditorTab::Agent);
                        }
                    },
                }
                TabButton {
                    label: "Code",
                    active: active() == ShaderEditorTab::Code,
                    dirty,
                    on_select: {
                        let user_touched = Rc::clone(&user_touched);
                        move |_| {
                            *user_touched.borrow_mut() = true;
                            active.set(ShaderEditorTab::Code);
                        }
                    },
                }
            }
            match active() {
                ShaderEditorTab::Agent => {
                    // The OpenRouter connect wiring (App-provided context;
                    // absent under stories, which render the CTA inert).
                    let openrouter_error = try_consume_context::<Signal<Option<String>>>();
                    rsx! {
                        AgentChatPane {
                            view: agent,
                            source_resolved: editor.content.is_some(),
                            tool_rows_expanded,
                            on_action,
                            on_connect: move |_| crate::openrouter_oauth::begin_connect(openrouter_error),
                            connect_error: openrouter_error.and_then(|error| error()),
                        }
                    }
                },
                ShaderEditorTab::Code => rsx! {
                    AssetEditor { editor: editor.clone(), on_action, platform }
                },
            }
        }
    }
}

/// One strip tab, styled after the node pane's header tabs. `dirty` adds
/// the warning-toned unsaved dot (the save panel's dirty convention —
/// yellow is the edit color; green stays reserved for valid).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn TabButton(
    label: &'static str,
    active: bool,
    #[props(default = false)] dirty: bool,
    on_select: EventHandler<()>,
) -> Element {
    rsx! {
        button {
            class: if active {
                "tw:inline-flex tw:min-h-full tw:items-center tw:gap-1.5 tw:border-0 tw:border-r tw:border-border-muted tw:bg-card-subtle tw:px-4 tw:py-1.5 tw:text-xs tw:font-bold tw:text-strong-foreground"
            } else {
                "tw:inline-flex tw:min-h-full tw:items-center tw:gap-1.5 tw:border-0 tw:border-r tw:border-border-muted tw:bg-transparent tw:px-4 tw:py-1.5 tw:text-xs tw:font-bold tw:text-muted-foreground tw:hover:bg-card-subtle tw:hover:text-strong-foreground"
            },
            r#type: "button",
            role: "tab",
            aria_selected: "{active}",
            title: if dirty { "{label} — unsaved changes" } else { "{label}" },
            onclick: move |event| {
                event.stop_propagation();
                on_select.call(());
            },
            "{label}"
            if dirty {
                span { class: "tw:h-1.5 tw:w-1.5 tw:flex-none tw:rounded-full tw:bg-status-warning-foreground" }
            }
        }
    }
}
