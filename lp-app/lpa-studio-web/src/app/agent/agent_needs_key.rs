//! [`AgentNeedsKey`]: what an agent chat shows before any provider is set
//! up — never a dead end.
//!
//! The one-click OpenRouter Connect CTA leads (success switches the
//! provider, so it leads regardless of the current selection), then the
//! selected provider's onboarding guidance (core-supplied copy, same source
//! as the settings popover) plus the pointer at the settings gear. Both
//! chats mount it; each names itself.

use dioxus::prelude::*;
use lpa_studio_core::AgentProviderGuidance;

use crate::core::outline_action_class;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn AgentNeedsKey(
    /// The chat's name ("Shader agent").
    title: &'static str,
    /// What it does, in one sentence.
    lede: &'static str,
    guidance: Option<AgentProviderGuidance>,
    #[props(default)] on_connect: Option<EventHandler<()>>,
    #[props(default)] connect_error: Option<String>,
) -> Element {
    let provider_label = guidance.map(|g| g.label).unwrap_or("a provider");
    rsx! {
        div { class: "tw:flex tw:flex-col tw:items-center tw:gap-1.5 tw:bg-card tw:px-6 tw:py-10 tw:text-center",
            p { class: "tw:m-0 tw:text-sm tw:font-bold tw:text-strong-foreground", "{title}" }
            p { class: "tw:m-0 tw:max-w-96 tw:text-sm tw:text-muted-foreground", "{lede}" }
            if let Some(on_connect) = on_connect {
                button {
                    class: "{outline_action_class(false)} tw:mt-2",
                    r#type: "button",
                    title: "Sign in on openrouter.ai and come right back — no key to paste",
                    onclick: move |_| on_connect.call(()),
                    "Connect OpenRouter — use your own account"
                }
                p { class: "tw:m-0 tw:max-w-96 tw:text-xs tw:text-dim-foreground",
                    "Pay-as-you-go from your OpenRouter credits; every major model."
                }
            }
            if let Some(error) = connect_error.as_deref() {
                p { class: "tw:m-0 tw:max-w-96 tw:text-xs tw:font-bold tw:text-status-warning-foreground",
                    "{error}"
                }
            }
            p { class: "tw:m-0 tw:mt-2 tw:max-w-96 tw:text-sm tw:text-muted-foreground",
                "Or finish setting up {provider_label} in Settings (the gear icon, top right)."
            }
            if let Some(guidance) = guidance {
                p { class: "tw:m-0 tw:max-w-96 tw:text-xs tw:text-muted-foreground", "{guidance.setup}" }
                if let Some(note) = guidance.note {
                    p { class: "tw:m-0 tw:max-w-96 tw:text-xs tw:text-dim-foreground", "{note}" }
                }
                div { class: "tw:flex tw:flex-wrap tw:justify-center tw:gap-x-3",
                    for (label , url) in guidance.links {
                        a {
                            key: "{url}",
                            class: "tw:text-xs tw:text-muted-foreground tw:underline tw:transition-colors tw:hover:text-strong-foreground",
                            href: "{url}",
                            target: "_blank",
                            rel: "noopener noreferrer",
                            "{label}"
                        }
                    }
                }
            }
            p { class: "tw:m-0 tw:text-xs tw:text-dim-foreground",
                "Keys are stored in this browser and sent only to the provider you configure."
            }
        }
    }
}
