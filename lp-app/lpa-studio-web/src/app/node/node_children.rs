use dioxus::prelude::*;
use lpa_studio_core::{UiAction, UiNodeChild, UiNodeHeader, UiNodeTab, UiNodeView};

use crate::app::node::NodePane;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn NodeChildren(
    items: Vec<UiNodeChild>,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:gap-3 tw:border-l tw:border-border-muted tw:pl-4",
            for child in items {
                NodePane {
                    key: "{child.label}",
                    view: child_node_view(child),
                    on_action,
                }
            }
        }
    }
}

fn child_node_view(child: UiNodeChild) -> UiNodeView {
    let header = UiNodeHeader::new(
        child.label.clone(),
        child.kind.clone(),
        child.detail.clone(),
    )
    .with_status(child.status.clone());
    let header = if let Some(summary) = child.summary {
        header.with_summary(summary)
    } else {
        header
    };
    let mut view = UiNodeView::new(header, vec![UiNodeTab::main(child.sections)])
        .with_node_id(format!("child:{}", child.label))
        .with_children(child.children);
    view.focused = child.focused || child.active;
    view.action = child.action;
    view
}
