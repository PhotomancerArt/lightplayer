use dioxus::prelude::*;

use crate::stories::story::StoryDescriptor;
use crate::ui_base::{StudioIcon, StudioIconName};

const CLOCK_SHAPE_JSON: &str = include_str!("data/node_ui/clock.shape.json");
const CLOCK_SLOTS_JSON: &str = include_str!("data/node_ui/clock.slots.json");
const FIXTURE_SHAPE_JSON: &str = include_str!("data/node_ui/fixture.shape.json");
const FIXTURE_SLOTS_JSON: &str = include_str!("data/node_ui/fixture.slots.json");
const PLAYLIST_SHAPE_JSON: &str = include_str!("data/node_ui/playlist.shape.json");
const PLAYLIST_SLOTS_JSON: &str = include_str!("data/node_ui/playlist.slots.json");
const SHADER_SHAPE_JSON: &str = include_str!("data/node_ui/shader.shape.json");
const SHADER_SLOTS_JSON: &str = include_str!("data/node_ui/shader.slots.json");

pub const STORIES: &[StoryDescriptor] = &[
    StoryDescriptor::new(
        "studio/node-ui/clock-instrument",
        "Node UI Spike",
        "Clock",
        "Basic Clock node using the instrument-window direction.",
    ),
    StoryDescriptor::new(
        "studio/node-ui/clock-compact",
        "Node UI Spike",
        "Clock compact",
        "Basic Clock node using the compact-inspector direction.",
    ),
    StoryDescriptor::new(
        "studio/node-ui/fixture-control-product",
        "Node UI Spike",
        "Fixture",
        "Fixture node with a rough control-product preview and nested mapping.",
    ),
    StoryDescriptor::new(
        "studio/node-ui/shader-visual-product",
        "Node UI Spike",
        "Shader",
        "Shader node with a rough visual-product preview and compact slot rows.",
    ),
    StoryDescriptor::new(
        "studio/node-ui/playlist-children",
        "Node UI Spike",
        "Playlist",
        "Fyeah-inspired Playlist node with active entry and owned child visuals.",
    ),
    StoryDescriptor::new(
        "studio/node-ui/status-indicators",
        "Node UI Spike",
        "Status indicators",
        "Minimal nodes showing Running, Idle, and Error status chrome.",
    ),
    StoryDescriptor::new(
        "studio/node-ui/project-context",
        "Node UI Spike",
        "Project context",
        "Project-root hierarchy with representative node surfaces in context.",
    ),
    StoryDescriptor::new(
        "studio/node-ui/gallery",
        "Node UI Spike",
        "Node gallery",
        "Clock, Fixture, Shader, and Playlist examples on one review surface.",
    ),
];

pub fn render_story(id: &str) -> Option<Element> {
    match id {
        "studio/node-ui/clock-instrument" => Some(rsx! {
            NodeUiStoryCanvas {
                title: "Clock",
                note: "Instrument-window direction for a simple produced-value node.",
                NodeWindow {
                    node: clock_node(),
                    variant: NodeUiVariant::Instrument,
                }
            }
        }),
        "studio/node-ui/clock-compact" => Some(rsx! {
            NodeUiStoryCanvas {
                title: "Clock compact",
                note: "Compact-inspector direction with the same Clock data.",
                NodeWindow {
                    node: clock_node(),
                    variant: NodeUiVariant::Compact,
                }
            }
        }),
        "studio/node-ui/fixture-control-product" => Some(rsx! {
            NodeUiStoryCanvas {
                title: "Fixture",
                note: "Control product node with a rough probed-output box and one-level mapping detail.",
                NodeWindow {
                    node: fixture_node(),
                    variant: NodeUiVariant::Instrument,
                }
            }
        }),
        "studio/node-ui/shader-visual-product" => Some(rsx! {
            NodeUiStoryCanvas {
                title: "Shader",
                note: "Visual product node with a rough render preview surface.",
                NodeWindow {
                    node: shader_node(),
                    variant: NodeUiVariant::Instrument,
                }
            }
        }),
        "studio/node-ui/playlist-children" => Some(rsx! {
            NodeUiStoryCanvas {
                title: "Playlist",
                note: "Fyeah-inspired Playlist node with active child ownership and product output.",
                NodeWindow {
                    node: playlist_node(),
                    variant: NodeUiVariant::Instrument,
                }
            }
        }),
        "studio/node-ui/status-indicators" => Some(rsx! {
            NodeUiStatusStory {}
        }),
        "studio/node-ui/project-context" => Some(rsx! {
            NodeUiProjectContext {}
        }),
        "studio/node-ui/gallery" => Some(rsx! {
            NodeUiGallery {}
        }),
        _ => None,
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn NodeUiStoryCanvas(title: &'static str, note: &'static str, children: Element) -> Element {
    rsx! {
        section { class: "ux-node-ui-story",
            header { class: "ux-node-ui-story-heading",
                h2 { "{title}" }
                p { "{note}" }
            }
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
            title: "Node gallery",
            note: "All representative node presentations on one review surface.",
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
            title: "Status indicators",
            note: "Minimal node windows for checking status color, icon, tint, and the details popup.",
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
            title: "Project context",
            note: "The intended hierarchy: project root scopes every ordinary node beneath it.",
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
                }
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
            if !children.is_empty() {
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
) -> Element {
    rsx! {
        header { class: "ux-node-ui-header",
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
    let mut open = use_signal(|| initially_open);
    let trigger_class = if open() {
        format!(
            "{} ux-node-ui-status-button-open",
            status.indicator_class_name()
        )
    } else {
        status.indicator_class_name().to_string()
    };
    rsx! {
        div { class: "ux-node-ui-status-control",
            button {
                class: "{trigger_class}",
                r#type: "button",
                aria_label: "{status.label} status details",
                title: "{status.label} status details",
                aria_expanded: "{open()}",
                onclick: move |_| open.set(!open()),
                span { aria_hidden: "true",
                    StudioIcon {
                        name: status.icon_name(),
                        size: 14,
                    }
                }
            }
            if open() {
                div { class: "{status.popup_class_name()}",
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
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn NodeMainTabPanel(
    presentation: Vec<NodeUiPresentationItem>,
    values: Vec<NodeUiValueGroup>,
    variant: NodeUiVariant,
) -> Element {
    rsx! {
        if !presentation.is_empty() {
            NodePresentation {
                items: presentation,
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
fn NodePresentation(items: Vec<NodeUiPresentationItem>, variant: NodeUiVariant) -> Element {
    if let [NodeUiPresentationItem::Product(product)] = items.as_slice() {
        return rsx! {
            NodeProductTile { product: *product }
        };
    }

    let class = match variant {
        NodeUiVariant::Instrument => "ux-node-ui-presentation ux-node-ui-presentation-instrument",
        NodeUiVariant::Compact => "ux-node-ui-presentation ux-node-ui-presentation-compact",
    };
    rsx! {
        section { class,
            for item in items {
                match item {
                    NodeUiPresentationItem::Metric(metric) => rsx! {
                        NodePresentationMetric { metric }
                    },
                    NodeUiPresentationItem::Product(product) => rsx! {
                        NodeProductTile { product }
                    },
                }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn NodePresentationMetric(metric: NodeUiMetric) -> Element {
    rsx! {
        div { class: "ux-node-ui-metric",
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
    rsx! {
        NodeProductPreviewBox { product }
        div { class,
            footer { class: "ux-node-ui-product-meta",
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
    rsx! {
        div { class: "ux-node-ui-row",
            SlotSourceButton { source: row.source }
            span { class: "ux-node-ui-row-label", "{row.label}" }
            span { class: "ux-node-ui-row-value", "{row.value}" }
            if let Some(nested) = row.nested {
                NodeNestedValue { nested }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn SlotSourceButton(source: NodeUiValueSource) -> Element {
    rsx! {
        span {
            class: "{source.class_name()}",
            title: "{source.title()}",
            aria_label: "{source.title()}",
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
            ol {
                for child in items {
                    li { class: if child.active { "ux-node-ui-child ux-node-ui-child-active" } else { "ux-node-ui-child" },
                        div {
                            strong { "{child.label}" }
                            span { "{child.detail}" }
                        }
                        small { "{child.state}" }
                    }
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
            }),
            NodeUiPresentationItem::Metric(NodeUiMetric {
                label: "Delta",
                value: "0.033",
                detail: Some("seconds/frame"),
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
            }),
            NodeUiPresentationItem::Metric(NodeUiMetric {
                label: "Entry time",
                value: "3.333",
                detail: Some("seconds"),
            }),
            NodeUiPresentationItem::Metric(NodeUiMetric {
                label: "Active",
                value: "idle",
                detail: Some("entry 1"),
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
                detail: "./idle.toml",
                state: "active, fade_after 0.12 s",
                active: true,
            },
            NodeUiChild {
                label: "blast",
                detail: "./blast.toml",
                state: "duration 10 s, trigger bus#trigger",
                active: false,
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
        nested: None,
    }
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct NodeUiMetric {
    label: &'static str,
    value: &'static str,
    detail: Option<&'static str>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct NodeUiProduct {
    kind: NodeUiProductKind,
    name: &'static str,
    size: Option<&'static str>,
    preview_cells: usize,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct NodeUiChild {
    label: &'static str,
    detail: &'static str,
    state: &'static str,
    active: bool,
}
