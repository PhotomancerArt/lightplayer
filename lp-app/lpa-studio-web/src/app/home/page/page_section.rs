//! A section of the home page: a small uppercase title with a hairline rule
//! running out from it, and what the section holds underneath (the spike's
//! `.sect`). Every section of the page uses it, so the page reads as one
//! column of equals.

use dioxus::prelude::*;

/// A titled section. `count` is a small number after the title (how many
/// the section holds); `id` is the element id the walks and the Connect
/// hint scroll to.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn PageSection(
    title: &'static str,
    #[props(default)] count: Option<usize>,
    #[props(default)] id: Option<&'static str>,
    children: Element,
) -> Element {
    rsx! {
        section { id, class: "tw:grid tw:min-w-0 tw:content-start tw:gap-3",
            header { class: "tw:flex tw:items-center tw:gap-2.5",
                h2 { class: TITLE_CLASS, "{title}" }
                if let Some(count) = count {
                    span { class: COUNT_CLASS, "{count}" }
                }
                span { class: "tw:h-px tw:flex-1 tw:bg-border-muted", aria_hidden: "true" }
            }
            {children}
        }
    }
}

/// The title: extrabold small caps in the subtle ink. A fold's toggle wears
/// it too (`keys_fold`), so a folded section reads as a section.
pub(crate) const TITLE_CLASS: &str = "tw:m-0 tw:whitespace-nowrap tw:text-[11px] tw:font-extrabold tw:uppercase tw:tracking-[0.08em] tw:text-subtle-foreground";
/// The count after the title.
const COUNT_CLASS: &str = "tw:text-[11px] tw:font-semibold tw:text-dim-foreground";
