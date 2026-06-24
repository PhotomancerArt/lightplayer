//! Exploratory node UI stories.
//!
//! These stories are grounded in real project shape/slot JSON, but they are
//! still design spike surfaces. Keeping them under `ui_exploration` makes the
//! generated `exploration` story family honest without creating a parallel
//! source tree beside `ui_base`, `ui_core`, and `ui_studio`.

use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;

use crate::ui_base::{IconPopoverButton, PopoverPlacement, StudioIcon, StudioIconName};

const CLOCK_SHAPE_JSON: &str = include_str!("story_data/clock.shape.json");
const CLOCK_SLOTS_JSON: &str = include_str!("story_data/clock.slots.json");
const FIXTURE_SHAPE_JSON: &str = include_str!("story_data/fixture.shape.json");
const FIXTURE_SLOTS_JSON: &str = include_str!("story_data/fixture.slots.json");
const PLAYLIST_SHAPE_JSON: &str = include_str!("story_data/playlist.shape.json");
const PLAYLIST_SLOTS_JSON: &str = include_str!("story_data/playlist.slots.json");
const SHADER_SHAPE_JSON: &str = include_str!("story_data/shader.shape.json");
const SHADER_SLOTS_JSON: &str = include_str!("story_data/shader.slots.json");

#[story(description = "Instrument-window direction for a simple produced-value node.")]
fn clock_instrument() -> Element {
    rsx! {
        NodeUiStoryCanvas {
            NodeWindow {
                node: clock_node(),
                variant: NodeUiVariant::Instrument,
            }
        }
    }
}

#[story(description = "Compact-inspector direction with the same Clock data.")]
fn clock_compact() -> Element {
    rsx! {
        NodeUiStoryCanvas {
            NodeWindow {
                node: clock_node(),
                variant: NodeUiVariant::Compact,
            }
        }
    }
}

#[story(
    description = "Control product node with a rough probed-output box and one-level mapping detail."
)]
fn fixture_control_product() -> Element {
    rsx! {
        NodeUiStoryCanvas {
            NodeWindow {
                node: fixture_node(),
                variant: NodeUiVariant::Instrument,
            }
        }
    }
}

#[story(description = "Visual product node with a rough render preview surface.")]
fn shader_visual_product() -> Element {
    rsx! {
        NodeUiStoryCanvas {
            NodeWindow {
                node: shader_node(),
                variant: NodeUiVariant::Instrument,
            }
        }
    }
}

#[story(
    description = "Fyeah-inspired Playlist node with active child ownership and product output."
)]
fn playlist_children() -> Element {
    rsx! {
        NodeUiStoryCanvas {
            NodeWindow {
                node: playlist_node(),
                variant: NodeUiVariant::Instrument,
            }
        }
    }
}

#[story(
    description = "Minimal node windows for checking status color, icon, tint, and the details popup."
)]
fn status_indicators() -> Element {
    rsx! {
        NodeUiStatusStory {}
    }
}

#[story(
    description = "The intended hierarchy: project root scopes every ordinary node beneath it."
)]
fn project_context() -> Element {
    rsx! {
        NodeUiProjectContext {}
    }
}

#[story(description = "All representative node presentations on one review surface.")]
fn gallery() -> Element {
    rsx! {
        NodeUiGallery {}
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn NodeUiStoryCanvas(children: Element) -> Element {
    rsx! {
        section { class: "ux-node-ui-story",
            {children}
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn NodeUiGallery() -> Element {
    let nodes = vec![clock_node(), fixture_node(), shader_node(), playlist_node()];
    rsx! {
        NodeUiStoryCanvas {
            div { class: "ux-node-ui-gallery",
                for node in nodes {
                    NodeWindow {
                        key: "{node.path}",
                        node,
                        variant: NodeUiVariant::Instrument,
                    }
                }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn NodeUiStatusStory() -> Element {
    let nodes = vec![
        status_demo_node(
            "Running node",
            "Clock",
            Some("clock.toml"),
            NodeUiStatus::running(),
            Some("0.34 ms frame"),
            true,
        ),
        status_demo_node(
            "Idle node",
            "Fixture",
            Some("fixture.toml"),
            NodeUiStatus::idle(Some("Last ran at frame 96")),
            None,
            true,
        ),
        status_demo_node(
            "Error node",
            "Shader",
            Some("rainbow.glsl"),
            NodeUiStatus::error(
                Some("Shader compile failed"),
                Some(
                    "error[E_SHADER]: failed to compile rainbow.glsl\n  --> rainbow.glsl:18:14\n   |\n18 | color = sample(uv2);\n   |              ^^^ unknown identifier `uv2`",
                ),
            ),
            Some("64 ms compile"),
            true,
        ),
    ];
    rsx! {
        NodeUiStoryCanvas {
            div { class: "ux-node-ui-status-gallery",
                for node in nodes {
                    NodeWindow {
                        key: "{node.title}",
                        node,
                        variant: NodeUiVariant::Compact,
                    }
                }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn NodeUiProjectContext() -> Element {
    rsx! {
        NodeUiStoryCanvas {
            div { class: "ux-node-ui-project-layout",
                aside { class: "ux-node-ui-project-tree",
                    p { class: "ux-node-ui-tree-heading", "fyeah_sign.show" }
                    ol {
                        li { class: "ux-node-ui-tree-item ux-node-ui-tree-root", "Project" }
                        li { class: "ux-node-ui-tree-item ux-node-ui-tree-depth-1", "Clock" }
                        li { class: "ux-node-ui-tree-item ux-node-ui-tree-depth-1", "Playlist" }
                        li { class: "ux-node-ui-tree-item ux-node-ui-tree-depth-2", "idle" }
                        li { class: "ux-node-ui-tree-item ux-node-ui-tree-depth-2 ux-node-ui-tree-active", "blast" }
                        li { class: "ux-node-ui-tree-item ux-node-ui-tree-depth-1", "Fixture" }
                        li { class: "ux-node-ui-tree-item ux-node-ui-tree-depth-1", "Output" }
                    }
                }
                div { class: "ux-node-ui-project-nodes",
                    NodeWindow {
                        node: playlist_node(),
                        variant: NodeUiVariant::Instrument,
                    }
                    NodeWindow {
                        node: fixture_node(),
                        variant: NodeUiVariant::Compact,
                    }
                }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn NodeWindow(node: NodeUiNode, variant: NodeUiVariant) -> Element {
    let mut active_tab = use_signal(|| 0_usize);
    let mut collapsed = use_signal(|| false);
    let base_class = match variant {
        NodeUiVariant::Instrument => "ux-node-ui-window ux-node-ui-window-instrument",
        NodeUiVariant::Compact => "ux-node-ui-window ux-node-ui-window-compact",
    };
    let class = format!("{base_class} {}", node.status.window_class_name());
    let tabs = node.tabs.clone();
    let active_index = active_tab().min(tabs.len().saturating_sub(1));
    let active_content = tabs
        .get(active_index)
        .map(|tab| tab.content)
        .unwrap_or(NodeUiTabContent::None);
    let presentation = node.presentation.clone();
    let values = node.values.clone();
    let children = node.children.clone();
    rsx! {
        div { class: "ux-node-ui-node-stack",
            article { class,
                NodeHeader {
                    title: node.title,
                    kind: node.kind,
                    source: node.source,
                    status: node.status,
                    initially_open: node.status_details_open,
                    perf: node.perf,
                    tabs: tabs.clone(),
                    active_index,
                    on_select: move |index| active_tab.set(index),
                    collapsed: collapsed(),
                    on_toggle_collapsed: move |_| collapsed.set(!collapsed()),
                }
                if !collapsed() {
                    match active_content {
                        NodeUiTabContent::None => rsx! {
                            NodeMainTabPanel {
                                presentation,
                                values,
                                variant,
                            }
                        },
                        NodeUiTabContent::Json { title, body } => rsx! {
                            NodeJsonTabPanel {
                                title,
                                body,
                            }
                        },
                    }
                }
            }
            if !collapsed() && !children.is_empty() {
                NodeChildren {
                    items: children,
                }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn NodeHeader(
    title: &'static str,
    kind: &'static str,
    source: Option<&'static str>,
    status: NodeUiStatus,
    initially_open: bool,
    perf: Option<&'static str>,
    tabs: Vec<NodeUiTab>,
    active_index: usize,
    on_select: EventHandler<usize>,
    collapsed: bool,
    on_toggle_collapsed: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        header { class: "ux-node-ui-header",
            button {
                class: "ux-node-ui-collapse-button",
                r#type: "button",
                aria_label: if collapsed { "Expand node" } else { "Collapse node" },
                title: if collapsed { "Expand node" } else { "Collapse node" },
                onclick: move |event| on_toggle_collapsed.call(event),
                StudioIcon {
                    name: if collapsed { StudioIconName::Collapsed } else { StudioIconName::Expanded },
                    size: 14,
                }
            }
            NodeStatusIndicator {
                kind,
                source,
                status,
                initially_open,
                perf,
            }
            div { class: "ux-node-ui-title",
                h3 {
                    span { "{title}" }
                    if let Some(summary) = status.header_summary() {
                        small { class: "ux-node-ui-status-summary", " - {summary}" }
                    }
                }
            }
            if !tabs.is_empty() {
                NodeTabList {
                    tabs,
                    active_index,
                    on_select,
                }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn NodeStatusIndicator(
    kind: &'static str,
    source: Option<&'static str>,
    status: NodeUiStatus,
    initially_open: bool,
    perf: Option<&'static str>,
) -> Element {
    let open_class = format!(
        "{} ux-node-ui-status-button-open",
        status.indicator_class_name()
    );
    rsx! {
        div { class: "ux-node-ui-status-control",
            IconPopoverButton {
                class: status.indicator_class_name().to_string(),
                open_class,
                icon: status.icon_name(),
                icon_size: 14,
                label: format!("{} status details", status.label),
                title: format!("{} status details", status.label),
                popup_class: status.popup_class_name().to_string(),
                placement: PopoverPlacement::BottomStart,
                initially_open,
                div { class: "ux-node-ui-status-popup-summary",
                    div { class: "ux-node-ui-status-popup-line",
                        strong { class: "ux-node-ui-status-popup-kind", "{kind}" }
                        span { class: "ux-node-ui-status-popup-perf",
                            if let Some(perf) = perf {
                                "{perf}"
                            } else {
                                "{status.label}"
                            }
                        }
                    }
                    if let Some(source) = source {
                        div { class: "ux-node-ui-status-popup-source", "{source}" }
                    }
                }
                if let Some(detail) = status.error_detail() {
                    pre { class: "ux-node-ui-status-popup-error-detail",
                        code { "{detail}" }
                    }
                }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn NodeMainTabPanel(
    presentation: Vec<NodeUiPresentationItem>,
    values: Vec<NodeUiValueGroup>,
    variant: NodeUiVariant,
) -> Element {
    let (products, metrics) = split_presentation_items(presentation);
    rsx! {
        if !products.is_empty() || !metrics.is_empty() {
            NodeProducedSection {
                products,
                metrics,
                variant,
            }
        }
        if !values.is_empty() {
            NodeValueGroups {
                groups: values,
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn NodeProducedSection(
    products: Vec<NodeUiProduct>,
    metrics: Vec<NodeUiMetric>,
    variant: NodeUiVariant,
) -> Element {
    let class = match variant {
        NodeUiVariant::Instrument => "ux-node-ui-produced ux-node-ui-produced-instrument",
        NodeUiVariant::Compact => "ux-node-ui-produced ux-node-ui-produced-compact",
    };
    rsx! {
        section { class,
            if !products.is_empty() {
                div { class: "ux-node-ui-products",
                    for product in products {
                        NodeProductTile { product }
                    }
                }
            }
            if !metrics.is_empty() {
                div { class: "ux-node-ui-produced-values",
                    for metric in metrics {
                        NodePresentationMetric { metric }
                    }
                }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn NodePresentationMetric(metric: NodeUiMetric) -> Element {
    let bindings = metric.bindings.clone();
    rsx! {
        div { class: "ux-node-ui-metric",
            ProducedBindingButton {
                label: metric.label,
                type_label: "produced value",
                bindings,
                revision: metric.revision,
            }
            span { class: "ux-node-ui-metric-label", "{metric.label}" }
            strong { "{metric.value}" }
            if let Some(detail) = metric.detail {
                small { "{detail}" }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn NodeProductTile(product: NodeUiProduct) -> Element {
    let class = match product.kind {
        NodeUiProductKind::Visual => "ux-node-ui-product ux-node-ui-product-visual",
        NodeUiProductKind::Control => "ux-node-ui-product ux-node-ui-product-control",
    };
    let bindings = product.bindings.clone();
    rsx! {
        NodeProductPreviewBox { product: product.clone() }
        div { class,
            footer { class: "ux-node-ui-product-meta",
                ProducedBindingButton {
                    label: product.name,
                    type_label: product.kind.product_label(),
                    bindings,
                    revision: product.revision,
                }
                em { "{product.name}" }
                span { "{product.kind.label()}" }
                if let Some(size) = product.size {
                    small { "{size}" }
                }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ProducedBindingButton(
    label: &'static str,
    type_label: &'static str,
    bindings: NodeUiProducedBindings,
    revision: u32,
) -> Element {
    let bus_target = bindings.bus_target;
    let target_bindings = bindings.target_bindings.clone();
    let consumers = bindings.consumers.clone();
    let trigger_class = if bindings.has_any() {
        "ux-node-ui-popup-trigger ux-node-ui-popup-trigger-routed"
    } else {
        "ux-node-ui-popup-trigger"
    };
    let open_class = "ux-node-ui-popup-trigger ux-node-ui-popup-trigger-open";
    rsx! {
        IconPopoverButton {
            class: trigger_class.to_string(),
            open_class: open_class.to_string(),
            icon: StudioIconName::BoundValue,
            icon_size: 13,
            label: format!("{label} bindings"),
            title: format!("{label} bindings"),
            popup_class: "ux-node-ui-popup ux-node-ui-route-popup".to_string(),
            placement: PopoverPlacement::BottomEnd,
            div { class: "ux-node-ui-popup-kicker", "{type_label}" }
            strong { "{label}" }
            div { class: "ux-node-ui-binding-section ux-node-ui-bus-binding-section",
                div { class: "ux-node-ui-binding-heading", "bus binding" }
                if let Some(bus_target) = bus_target {
                    div { class: "ux-node-ui-bus-binding-row",
                        span { "bus#" }
                        code { "{bus_binding_name(bus_target)}" }
                        button { r#type: "button", "del" }
                    }
                } else {
                    div { class: "ux-node-ui-bus-binding-row ux-node-ui-bus-binding-row-empty",
                        span { "bus#" }
                        code { "not assigned" }
                        button { r#type: "button", "add" }
                    }
                }
            }
            if !target_bindings.is_empty() {
                div { class: "ux-node-ui-binding-section",
                    div { class: "ux-node-ui-binding-heading", "target bindings" }
                    for target in target_bindings {
                        div { class: "ux-node-ui-binding-item",
                            span { class: "ux-node-ui-binding-arrow", "->" }
                            code { "{target}" }
                            button { r#type: "button", "del" }
                        }
                    }
                    button { class: "ux-node-ui-binding-add", r#type: "button", "add" }
                }
            }
            if !consumers.is_empty() {
                div { class: "ux-node-ui-binding-section",
                    div { class: "ux-node-ui-binding-heading", "consumed by" }
                    for consumer in consumers {
                        div { class: "ux-node-ui-binding-item ux-node-ui-binding-item-readonly",
                            code { "{consumer}" }
                        }
                    }
                }
            }
            small { "rev {revision}" }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn NodeProductPreviewBox(product: NodeUiProduct) -> Element {
    let class = match product.kind {
        NodeUiProductKind::Visual => "ux-node-ui-preview ux-node-ui-preview-visual",
        NodeUiProductKind::Control => "ux-node-ui-preview ux-node-ui-preview-control",
    };
    rsx! {
        div { class,
            div { class: "ux-node-ui-preview-grid",
                for index in 0..product.preview_cells {
                    span { key: "{index}" }
                }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn NodeValueGroups(groups: Vec<NodeUiValueGroup>) -> Element {
    rsx! {
        section { class: "ux-node-ui-values",
            for group in groups {
                for row in group.rows {
                    NodeValueRow {
                        key: "{row.label}",
                        row,
                    }
                }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn NodeValueRow(row: NodeUiValueRow) -> Element {
    let class = if row.source == NodeUiValueSource::Bound {
        "ux-node-ui-row ux-node-ui-row-bound"
    } else {
        "ux-node-ui-row"
    };
    rsx! {
        div { class,
            SlotSourceButton {
                source: row.source,
                label: row.label,
                value: row.value,
                binding_target: row.binding_target,
                revision: row.revision,
            }
            span { class: "ux-node-ui-row-label", "{row.label}" }
            span { class: "ux-node-ui-row-value",
                if let Some(target) = row.binding_target {
                    span { class: "ux-node-ui-row-binding", "{target}" }
                } else {
                    "{row.value}"
                }
            }
            if let Some(nested) = row.nested {
                NodeNestedValue { nested }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn SlotSourceButton(
    source: NodeUiValueSource,
    label: &'static str,
    value: &'static str,
    binding_target: Option<&'static str>,
    revision: u32,
) -> Element {
    let open_class = format!("{} ux-node-ui-popup-trigger-open", source.class_name());
    rsx! {
        IconPopoverButton {
            class: source.class_name().to_string(),
            open_class,
            icon: source.icon_name(),
            icon_size: 13,
            title: source.title().to_string(),
            label: format!("{label} source"),
            popup_class: "ux-node-ui-popup ux-node-ui-slot-popup".to_string(),
            placement: PopoverPlacement::BottomStart,
            div { class: "ux-node-ui-popup-kicker", "consumed value" }
            strong { "{label}" }
            p {
                if let Some(target) = binding_target {
                    "source {target}"
                } else {
                    "assigned value {value}"
                }
            }
            div { class: "ux-node-ui-popup-actions",
                button {
                    class: if source == NodeUiValueSource::Direct { "is-active" } else { "" },
                    r#type: "button",
                    "assigned value"
                }
                button {
                    class: if source == NodeUiValueSource::Bound { "is-active" } else { "" },
                    r#type: "button",
                    "source binding"
                }
            }
            small { "rev {revision}" }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn NodeNestedValue(nested: NodeUiNestedValue) -> Element {
    rsx! {
        div { class: "ux-node-ui-nested",
            div { class: "ux-node-ui-nested-heading",
                span { "{nested.title}" }
                small { "{nested.summary}" }
            }
            dl {
                for item in nested.items {
                    div {
                        dt { "{item.label}" }
                        dd { "{item.value}" }
                    }
                }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn NodeChildren(items: Vec<NodeUiChild>) -> Element {
    rsx! {
        section { class: "ux-node-ui-children",
            h4 { "Children" }
            div { class: "ux-node-ui-child-nodes",
                for child in items {
                    NodeChildWindow { child }
                }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn NodeChildWindow(child: NodeUiChild) -> Element {
    let mut collapsed = use_signal(|| true);
    let status = if child.active {
        NodeUiStatus::running()
    } else {
        NodeUiStatus::idle(Some("Waiting for playlist entry"))
    };
    let class = format!(
        "ux-node-ui-window ux-node-ui-child-node {}",
        status.window_class_name()
    );
    rsx! {
        article { class,
            NodeHeader {
                title: child.label,
                kind: child.kind,
                source: Some(child.detail),
                status,
                initially_open: false,
                perf: Some(child.state),
                tabs: Vec::new(),
                active_index: 0,
                on_select: move |_| {},
                collapsed: collapsed(),
                on_toggle_collapsed: move |_| collapsed.set(!collapsed()),
            }
            if !collapsed() {
                NodeMainTabPanel {
                    presentation: child.presentation,
                    values: child.values,
                    variant: NodeUiVariant::Compact,
                }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn NodeTabList(
    tabs: Vec<NodeUiTab>,
    active_index: usize,
    on_select: EventHandler<usize>,
) -> Element {
    rsx! {
        div { class: "ux-node-ui-header-tabs", role: "tablist",
            for (index, tab) in tabs.clone().into_iter().enumerate() {
                button {
                    class: if index == active_index { "ux-node-ui-tab ux-node-ui-tab-active" } else { "ux-node-ui-tab" },
                    r#type: "button",
                    role: "tab",
                    aria_selected: "{index == active_index}",
                    onclick: move |_| on_select.call(index),
                    "{tab.label}"
                }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn NodeJsonTabPanel(title: &'static str, body: &'static str) -> Element {
    rsx! {
        div { class: "ux-node-ui-tab-panel", role: "tabpanel",
            div { class: "ux-node-ui-json-heading", "{title}" }
            pre { class: "ux-node-ui-json",
                code { "{body}" }
            }
        }
    }
}

fn clock_node() -> NodeUiNode {
    NodeUiNode {
        title: "Clock",
        kind: "Clock",
        source: Some("clock.toml"),
        path: "/fyeah_sign.show/clock.clock",
        status: NodeUiStatus::running(),
        status_details_open: false,
        perf: Some("936 fps"),
        presentation: vec![
            NodeUiPresentationItem::Metric(NodeUiMetric {
                label: "Seconds",
                value: "3.333",
                detail: Some("project time"),
                bindings: produced_bindings(Some("bus#time.seconds"), &[], &["Playlist.time"]),
                revision: 102,
            }),
            NodeUiPresentationItem::Metric(NodeUiMetric {
                label: "Delta",
                value: "0.033",
                detail: Some("seconds/frame"),
                bindings: produced_bindings(None, &[], &[]),
                revision: 102,
            }),
        ],
        values: vec![NodeUiValueGroup {
            rows: vec![
                value_row(NodeUiValueSource::Direct, "Running", "true"),
                value_row(NodeUiValueSource::Direct, "Rate", "1"),
                value_row(NodeUiValueSource::Direct, "Scrub offset", "0.0 s"),
            ],
        }],
        tabs: node_json_tabs(CLOCK_SHAPE_JSON, CLOCK_SLOTS_JSON),
        children: Vec::new(),
    }
}

fn shader_node() -> NodeUiNode {
    NodeUiNode {
        title: "blast",
        kind: "Shader",
        source: Some("blast.glsl"),
        path: "/fyeah_sign.show/playlist.playlist/blast.shader",
        status: NodeUiStatus::running(),
        status_details_open: false,
        perf: Some("64 ms compile"),
        presentation: vec![NodeUiPresentationItem::Product(NodeUiProduct {
            kind: NodeUiProductKind::Visual,
            name: "output",
            size: Some("128 x 72"),
            preview_cells: 24,
            bindings: produced_bindings(None, &[], &["Playlist.entry.visual"]),
            revision: 42,
        })],
        values: vec![
            NodeUiValueGroup {
                rows: vec![
                    value_row(NodeUiValueSource::Bound, "Time", "../playlist#entry_time"),
                    value_row(
                        NodeUiValueSource::Bound,
                        "Progress",
                        "../playlist#entry_progress",
                    ),
                ],
            },
            NodeUiValueGroup {
                rows: vec![
                    value_row(NodeUiValueSource::Direct, "Shader", "blast.glsl"),
                    value_row(NodeUiValueSource::Direct, "Render order", "0"),
                ],
            },
        ],
        tabs: node_json_tabs(SHADER_SHAPE_JSON, SHADER_SLOTS_JSON),
        children: Vec::new(),
    }
}

fn fixture_node() -> NodeUiNode {
    NodeUiNode {
        title: "Fixture",
        kind: "Fixture",
        source: Some("fixture.toml"),
        path: "/fyeah_sign.show/fixture.fixture",
        status: NodeUiStatus::running(),
        status_details_open: false,
        perf: Some("657 samples"),
        presentation: vec![NodeUiPresentationItem::Product(NodeUiProduct {
            kind: NodeUiProductKind::Control,
            name: "output",
            size: Some("1 x 657"),
            preview_cells: 30,
            bindings: produced_bindings(Some("bus#control.fixture"), &[], &["Output.main"]),
            revision: 44,
        })],
        values: vec![
            NodeUiValueGroup {
                rows: vec![
                    value_row(NodeUiValueSource::Direct, "Render size", "16 x 16"),
                    value_row(NodeUiValueSource::Direct, "Color order", "RGB"),
                    value_row(NodeUiValueSource::Direct, "Brightness", "255"),
                    NodeUiValueRow {
                        source: NodeUiValueSource::Direct,
                        label: "Mapping",
                        value: "SvgPath",
                        binding_target: None,
                        revision: 42,
                        nested: Some(NodeUiNestedValue {
                            title: "fyeah-mapping.svg",
                            summary: "sample diameter 2.0",
                            items: vec![
                                NodeUiNestedItem {
                                    label: "source",
                                    value: "./fyeah-mapping.svg",
                                },
                                NodeUiNestedItem {
                                    label: "sampling",
                                    value: "direct",
                                },
                            ],
                        }),
                    },
                ],
            },
            NodeUiValueGroup {
                rows: vec![value_row(
                    NodeUiValueSource::Bound,
                    "Visual",
                    "../playlist#output",
                )],
            },
        ],
        tabs: node_json_tabs(FIXTURE_SHAPE_JSON, FIXTURE_SLOTS_JSON),
        children: Vec::new(),
    }
}

fn playlist_node() -> NodeUiNode {
    NodeUiNode {
        title: "Playlist",
        kind: "Playlist",
        source: Some("playlist.toml"),
        path: "/fyeah_sign.show/playlist.playlist",
        status: NodeUiStatus::running(),
        status_details_open: false,
        perf: Some("entry 1"),
        presentation: vec![
            NodeUiPresentationItem::Product(NodeUiProduct {
                kind: NodeUiProductKind::Visual,
                name: "output",
                size: Some("128 x 72"),
                preview_cells: 18,
                bindings: produced_bindings(Some("bus#visual.out"), &[], &["Fixture.visual"]),
                revision: 104,
            }),
            NodeUiPresentationItem::Metric(NodeUiMetric {
                label: "Entry time",
                value: "3.333",
                detail: Some("seconds"),
                bindings: produced_bindings(None, &[], &["idle.Time", "blast.Time"]),
                revision: 104,
            }),
            NodeUiPresentationItem::Metric(NodeUiMetric {
                label: "Active",
                value: "idle",
                detail: Some("entry 1"),
                bindings: produced_bindings(None, &[], &[]),
                revision: 104,
            }),
        ],
        values: vec![
            NodeUiValueGroup {
                rows: vec![
                    value_row(NodeUiValueSource::Bound, "Time", "bus#time.seconds"),
                    value_row(NodeUiValueSource::Direct, "Idle entry", "1"),
                    value_row(NodeUiValueSource::Direct, "Default fade", "0.35 s"),
                    value_row(NodeUiValueSource::Direct, "Active entry", "1"),
                ],
            },
            NodeUiValueGroup {
                rows: vec![
                    value_row(NodeUiValueSource::Child, "idle", "./idle.shader"),
                    value_row(NodeUiValueSource::Child, "blast", "./blast.shader"),
                    value_row(NodeUiValueSource::Bound, "blast.trigger", "bus#trigger"),
                ],
            },
        ],
        tabs: node_json_tabs(PLAYLIST_SHAPE_JSON, PLAYLIST_SLOTS_JSON),
        children: vec![
            NodeUiChild {
                label: "idle",
                kind: "Shader",
                detail: "./idle.toml",
                state: "active, fade_after 0.12 s",
                active: true,
                presentation: vec![NodeUiPresentationItem::Product(NodeUiProduct {
                    kind: NodeUiProductKind::Visual,
                    name: "output",
                    size: Some("128 x 72"),
                    preview_cells: 12,
                    bindings: produced_bindings(None, &["../playlist#output"], &[]),
                    revision: 103,
                })],
                values: vec![NodeUiValueGroup {
                    rows: vec![
                        value_row(NodeUiValueSource::Bound, "Time", "../playlist#entry_time"),
                        value_row(NodeUiValueSource::Direct, "Shader", "idle.glsl"),
                    ],
                }],
            },
            NodeUiChild {
                label: "blast",
                kind: "Shader",
                detail: "./blast.toml",
                state: "duration 10 s, trigger bus#trigger",
                active: false,
                presentation: vec![NodeUiPresentationItem::Product(NodeUiProduct {
                    kind: NodeUiProductKind::Visual,
                    name: "output",
                    size: Some("128 x 72"),
                    preview_cells: 12,
                    bindings: produced_bindings(None, &["../playlist#output"], &[]),
                    revision: 98,
                })],
                values: vec![NodeUiValueGroup {
                    rows: vec![
                        value_row(NodeUiValueSource::Bound, "Time", "../playlist#entry_time"),
                        value_row(NodeUiValueSource::Bound, "Trigger", "bus#trigger"),
                        value_row(NodeUiValueSource::Direct, "Shader", "blast.glsl"),
                    ],
                }],
            },
        ],
    }
}

fn status_demo_node(
    title: &'static str,
    kind: &'static str,
    source: Option<&'static str>,
    status: NodeUiStatus,
    perf: Option<&'static str>,
    status_details_open: bool,
) -> NodeUiNode {
    NodeUiNode {
        title,
        kind,
        source,
        path: "/status.demo",
        status,
        status_details_open,
        perf,
        presentation: Vec::new(),
        values: vec![NodeUiValueGroup {
            rows: vec![
                value_row(NodeUiValueSource::Direct, "Frame", "128"),
                value_row(NodeUiValueSource::Direct, "Last duration", "0.34 ms"),
                value_row(NodeUiValueSource::Direct, "Output", "ready"),
            ],
        }],
        tabs: Vec::new(),
        children: Vec::new(),
    }
}

fn node_json_tabs(shape_json: &'static str, slots_json: &'static str) -> Vec<NodeUiTab> {
    vec![
        NodeUiTab {
            label: "main",
            content: NodeUiTabContent::None,
        },
        NodeUiTab {
            label: "shape",
            content: NodeUiTabContent::Json {
                title: "Shape JSON",
                body: shape_json,
            },
        },
        NodeUiTab {
            label: "slots",
            content: NodeUiTabContent::Json {
                title: "Slot value JSON",
                body: slots_json,
            },
        },
    ]
}

fn value_row(
    source: NodeUiValueSource,
    label: &'static str,
    value: &'static str,
) -> NodeUiValueRow {
    NodeUiValueRow {
        source,
        label,
        value,
        binding_target: (source == NodeUiValueSource::Bound).then_some(value),
        revision: match source {
            NodeUiValueSource::Direct => 42,
            NodeUiValueSource::Bound => 104,
            NodeUiValueSource::Child => 0,
        },
        nested: None,
    }
}

fn produced_bindings(
    bus_target: Option<&'static str>,
    target_bindings: &[&'static str],
    consumers: &[&'static str],
) -> NodeUiProducedBindings {
    NodeUiProducedBindings {
        bus_target,
        target_bindings: target_bindings.to_vec(),
        consumers: consumers.to_vec(),
    }
}

fn bus_binding_name(binding_ref: &'static str) -> &'static str {
    binding_ref.strip_prefix("bus#").unwrap_or(binding_ref)
}

fn split_presentation_items(
    items: Vec<NodeUiPresentationItem>,
) -> (Vec<NodeUiProduct>, Vec<NodeUiMetric>) {
    let mut products = Vec::new();
    let mut metrics = Vec::new();
    for item in items {
        match item {
            NodeUiPresentationItem::Product(product) => products.push(product),
            NodeUiPresentationItem::Metric(metric) => metrics.push(metric),
        }
    }
    (products, metrics)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NodeUiVariant {
    Instrument,
    Compact,
}

#[derive(Clone, Debug, PartialEq)]
struct NodeUiNode {
    title: &'static str,
    kind: &'static str,
    source: Option<&'static str>,
    path: &'static str,
    status: NodeUiStatus,
    status_details_open: bool,
    perf: Option<&'static str>,
    presentation: Vec<NodeUiPresentationItem>,
    values: Vec<NodeUiValueGroup>,
    tabs: Vec<NodeUiTab>,
    children: Vec<NodeUiChild>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct NodeUiStatus {
    label: &'static str,
    tone: NodeUiStatusTone,
    summary: Option<&'static str>,
    detail: Option<&'static str>,
}

impl NodeUiStatus {
    const fn running() -> Self {
        Self {
            label: "Running",
            tone: NodeUiStatusTone::Running,
            summary: None,
            detail: Some("Node has run recently with no reported errors."),
        }
    }

    const fn idle(summary: Option<&'static str>) -> Self {
        Self {
            label: "Idle",
            tone: NodeUiStatusTone::Idle,
            summary,
            detail: Some("Node has no current error, but has not run recently."),
        }
    }

    const fn error(summary: Option<&'static str>, detail: Option<&'static str>) -> Self {
        Self {
            label: "Error",
            tone: NodeUiStatusTone::Error,
            summary,
            detail,
        }
    }

    fn window_class_name(self) -> &'static str {
        match self.tone {
            NodeUiStatusTone::Running => "ux-node-ui-window-status-running",
            NodeUiStatusTone::Idle => "ux-node-ui-window-status-idle",
            NodeUiStatusTone::Error => "ux-node-ui-window-status-error",
        }
    }

    fn indicator_class_name(self) -> &'static str {
        match self.tone {
            NodeUiStatusTone::Running => {
                "ux-node-ui-status-button ux-node-ui-status-button-running"
            }
            NodeUiStatusTone::Idle => "ux-node-ui-status-button ux-node-ui-status-button-idle",
            NodeUiStatusTone::Error => "ux-node-ui-status-button ux-node-ui-status-button-error",
        }
    }

    fn icon_name(self) -> StudioIconName {
        match self.tone {
            NodeUiStatusTone::Running => StudioIconName::StatusRunning,
            NodeUiStatusTone::Idle => StudioIconName::StatusIdle,
            NodeUiStatusTone::Error => StudioIconName::StatusError,
        }
    }

    fn popup_class_name(self) -> &'static str {
        match self.tone {
            NodeUiStatusTone::Running => "ux-node-ui-status-popup ux-node-ui-status-popup-running",
            NodeUiStatusTone::Idle => "ux-node-ui-status-popup ux-node-ui-status-popup-idle",
            NodeUiStatusTone::Error => "ux-node-ui-status-popup ux-node-ui-status-popup-error",
        }
    }

    fn error_detail(self) -> Option<&'static str> {
        match self.tone {
            NodeUiStatusTone::Error => self.detail,
            NodeUiStatusTone::Running | NodeUiStatusTone::Idle => None,
        }
    }

    fn header_summary(self) -> Option<&'static str> {
        match self.tone {
            NodeUiStatusTone::Error => self.summary,
            NodeUiStatusTone::Running | NodeUiStatusTone::Idle => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NodeUiStatusTone {
    Running,
    Idle,
    Error,
}

#[derive(Clone, Debug, PartialEq)]
enum NodeUiPresentationItem {
    Metric(NodeUiMetric),
    Product(NodeUiProduct),
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct NodeUiMetric {
    label: &'static str,
    value: &'static str,
    detail: Option<&'static str>,
    bindings: NodeUiProducedBindings,
    revision: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct NodeUiProduct {
    kind: NodeUiProductKind,
    name: &'static str,
    size: Option<&'static str>,
    preview_cells: usize,
    bindings: NodeUiProducedBindings,
    revision: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct NodeUiProducedBindings {
    bus_target: Option<&'static str>,
    target_bindings: Vec<&'static str>,
    consumers: Vec<&'static str>,
}

impl NodeUiProducedBindings {
    fn has_any(&self) -> bool {
        self.bus_target.is_some() || !self.target_bindings.is_empty() || !self.consumers.is_empty()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NodeUiProductKind {
    Visual,
    Control,
}

impl NodeUiProductKind {
    fn label(self) -> &'static str {
        match self {
            Self::Visual => "visual",
            Self::Control => "control",
        }
    }

    fn product_label(self) -> &'static str {
        match self {
            Self::Visual => "visual product",
            Self::Control => "control product",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct NodeUiValueGroup {
    rows: Vec<NodeUiValueRow>,
}

#[derive(Clone, Debug, PartialEq)]
struct NodeUiValueRow {
    source: NodeUiValueSource,
    label: &'static str,
    value: &'static str,
    binding_target: Option<&'static str>,
    revision: u32,
    nested: Option<NodeUiNestedValue>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NodeUiValueSource {
    Direct,
    Bound,
    Child,
}

impl NodeUiValueSource {
    fn class_name(self) -> &'static str {
        match self {
            Self::Direct => "ux-node-ui-source ux-node-ui-source-direct",
            Self::Bound => "ux-node-ui-source ux-node-ui-source-bound",
            Self::Child => "ux-node-ui-source ux-node-ui-source-child",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::Direct => "direct value",
            Self::Bound => "bound value",
            Self::Child => "child node",
        }
    }

    fn icon_name(self) -> StudioIconName {
        match self {
            Self::Direct => StudioIconName::AssignedValue,
            Self::Bound => StudioIconName::BoundValue,
            Self::Child => StudioIconName::ChildValue,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct NodeUiNestedValue {
    title: &'static str,
    summary: &'static str,
    items: Vec<NodeUiNestedItem>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct NodeUiNestedItem {
    label: &'static str,
    value: &'static str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct NodeUiTab {
    label: &'static str,
    content: NodeUiTabContent,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NodeUiTabContent {
    None,
    Json {
        title: &'static str,
        body: &'static str,
    },
}

#[derive(Clone, Debug, PartialEq)]
struct NodeUiChild {
    label: &'static str,
    kind: &'static str,
    detail: &'static str,
    state: &'static str,
    active: bool,
    presentation: Vec<NodeUiPresentationItem>,
    values: Vec<NodeUiValueGroup>,
}
