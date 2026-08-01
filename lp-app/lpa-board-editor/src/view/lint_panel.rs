//! The lint panel: every finding for the def as it stands, recomputed on
//! each edit. Errors are what stand between the def and check-in; warns are
//! authoring guidance; infos are context.

use dioxus::prelude::*;

use crate::editor_core::editor_doc::EditorDoc;
use crate::editor_core::lint::{LintLevel, lint_board};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn LintPanel(doc: Signal<EditorDoc>) -> Element {
    let findings = lint_board(&doc.read().board);
    let errors = findings
        .iter()
        .filter(|finding| finding.level == LintLevel::Error)
        .count();
    let warns = findings
        .iter()
        .filter(|finding| finding.level == LintLevel::Warn)
        .count();

    rsx! {
        section { class: "lpb-ed-section lpb-ed-lint",
            div { class: "lpb-ed-section-head",
                h2 { "Lint" }
                if errors > 0 {
                    span { class: "lpb-ed-lint-count lpb-ed-lint-count--error", "{errors} errors" }
                }
                if warns > 0 {
                    span { class: "lpb-ed-lint-count lpb-ed-lint-count--warn", "{warns} warnings" }
                }
                if errors == 0 && warns == 0 {
                    span { class: "lpb-ed-lint-count lpb-ed-lint-count--clean", "clean" }
                }
            }
            ul { class: "lpb-ed-lint-list",
                for (index, finding) in findings.iter().enumerate() {
                    li {
                        key: "{index}",
                        class: "lpb-ed-lint-item lpb-ed-lint-item--{finding.level.css_suffix()}",
                        span { class: "lpb-ed-lint-dot" }
                        "{finding.message}"
                    }
                }
            }
        }
    }
}
