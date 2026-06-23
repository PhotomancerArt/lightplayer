use dioxus::prelude::*;

use crate::stories::story::StoryDescriptor;

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
fn NodeUiProjectContext() -> Element {
    rsx! {
        NodeUiStoryCanvas {
            title: "Project context",
            note: "The intended hierarchy: project root scopes every ordinary node beneath it.",
            div { class: "ux-node-ui-project-layout",
                aside { class: "ux-node-ui-project-tree",
                    p { class: "ux-node-ui-tree-heading", "fyeah-sign.project" }
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
    let class = match variant {
        NodeUiVariant::Instrument => "ux-node-ui-window ux-node-ui-window-instrument",
        NodeUiVariant::Compact => "ux-node-ui-window ux-node-ui-window-compact",
    };
    let children = node.children.clone();
    rsx! {
        div { class: "ux-node-ui-node-stack",
            article { class,
                NodeHeader {
                    title: node.title,
                    kind: node.kind,
                    status: node.status,
                    perf: node.perf,
                }
                if !node.presentation.is_empty() {
                    NodePresentation {
                        items: node.presentation,
                        variant,
                    }
                }
                if !node.values.is_empty() {
                    NodeValueGroups {
                        groups: node.values,
                    }
                }
                if !node.tabs.is_empty() {
                    NodeTabStrip {
                        tabs: node.tabs,
                    }
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
    status: NodeUiStatus,
    perf: Option<&'static str>,
) -> Element {
    rsx! {
        header { class: "ux-node-ui-header",
            div { class: "ux-node-ui-title",
                h3 {
                    "{title}"
                    span { class: "ux-node-ui-title-kind", "{kind}" }
                }
            }
            div { class: "ux-node-ui-header-meta",
                if let Some(perf) = perf {
                    span { class: "ux-node-ui-perf", "{perf}" }
                }
                span { class: "{status.class_name()}", "{status.label}" }
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
fn NodeTabStrip(tabs: Vec<NodeUiTab>) -> Element {
    let mut active = use_signal(|| 0_usize);
    let active_index = active().min(tabs.len().saturating_sub(1));
    rsx! {
        section { class: "ux-node-ui-tabs",
            div { class: "ux-node-ui-tab-list", role: "tablist",
                for (index, tab) in tabs.clone().into_iter().enumerate() {
                    button {
                        class: if index == active_index { "ux-node-ui-tab ux-node-ui-tab-active" } else { "ux-node-ui-tab" },
                        r#type: "button",
                        role: "tab",
                        aria_selected: "{index == active_index}",
                        onclick: move |_| active.set(index),
                        "{tab.label}"
                    }
                }
            }
        }
    }
}

fn clock_node() -> NodeUiNode {
    NodeUiNode {
        title: "Clock",
        kind: "Clock",
        path: "/fyeah-sign.project/clock.clock",
        status: NodeUiStatus::good("Running"),
        perf: Some("936 fps"),
        presentation: vec![
            NodeUiPresentationItem::Metric(NodeUiMetric {
                label: "Seconds",
                value: "3.43",
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
                value_row(NodeUiValueSource::Direct, "Rate", "1.0"),
                value_row(NodeUiValueSource::Direct, "Scrub offset", "0.0 s"),
            ],
        }],
        tabs: vec![NodeUiTab { label: "main" }, NodeUiTab { label: "debug" }],
        children: Vec::new(),
    }
}

fn shader_node() -> NodeUiNode {
    NodeUiNode {
        title: "blast",
        kind: "Shader",
        path: "/fyeah-sign.project/playlist.playlist/blast.shader",
        status: NodeUiStatus::good("Running"),
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
                    value_row(NodeUiValueSource::Direct, "Brightness", "0.72"),
                    value_row(NodeUiValueSource::Direct, "Center", "(0.5, 0.5)"),
                ],
            },
            NodeUiValueGroup {
                rows: vec![
                    value_row(NodeUiValueSource::Direct, "Shader", "blast.glsl"),
                    value_row(NodeUiValueSource::Direct, "Render order", "10"),
                ],
            },
        ],
        tabs: vec![
            NodeUiTab { label: "main" },
            NodeUiTab { label: "source" },
            NodeUiTab { label: "debug" },
        ],
        children: Vec::new(),
    }
}

fn fixture_node() -> NodeUiNode {
    NodeUiNode {
        title: "Fixture",
        kind: "Fixture",
        path: "/fyeah-sign.project/fixture.fixture",
        status: NodeUiStatus::good("Running"),
        perf: Some("241 LEDs"),
        presentation: vec![NodeUiPresentationItem::Product(NodeUiProduct {
            kind: NodeUiProductKind::Control,
            name: "output",
            size: Some("1 x 241"),
            preview_cells: 30,
        })],
        values: vec![
            NodeUiValueGroup {
                rows: vec![
                    value_row(NodeUiValueSource::Direct, "Render size", "16 x 16"),
                    value_row(NodeUiValueSource::Direct, "Color order", "RGB"),
                    value_row(NodeUiValueSource::Direct, "Brightness", "64"),
                    NodeUiValueRow {
                        source: NodeUiValueSource::Direct,
                        label: "Mapping",
                        value: "PathPoints",
                        nested: Some(NodeUiNestedValue {
                            title: "paths[0].RingArray",
                            summary: "concentric sign ring",
                            items: vec![
                                NodeUiNestedItem {
                                    label: "center",
                                    value: "(0.5, 0.5)",
                                },
                                NodeUiNestedItem {
                                    label: "diameter",
                                    value: "1.0",
                                },
                                NodeUiNestedItem {
                                    label: "rings",
                                    value: "0..8",
                                },
                                NodeUiNestedItem {
                                    label: "lamp counts",
                                    value: "1, 8, 12, 18, 24, 30, 42, 106",
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
        tabs: vec![
            NodeUiTab { label: "main" },
            NodeUiTab { label: "source" },
            NodeUiTab { label: "debug" },
        ],
        children: Vec::new(),
    }
}

fn playlist_node() -> NodeUiNode {
    NodeUiNode {
        title: "Playlist",
        kind: "Playlist",
        path: "/fyeah-sign.project/playlist.playlist",
        status: NodeUiStatus::good("Running"),
        perf: Some("entry 2"),
        presentation: vec![
            NodeUiPresentationItem::Product(NodeUiProduct {
                kind: NodeUiProductKind::Visual,
                name: "output",
                size: Some("128 x 72"),
                preview_cells: 18,
            }),
            NodeUiPresentationItem::Metric(NodeUiMetric {
                label: "Entry time",
                value: "1.52",
                detail: Some("seconds"),
            }),
            NodeUiPresentationItem::Metric(NodeUiMetric {
                label: "Progress",
                value: "15%",
                detail: Some("blast"),
            }),
        ],
        values: vec![
            NodeUiValueGroup {
                rows: vec![
                    value_row(NodeUiValueSource::Bound, "Time", "bus#time.seconds"),
                    value_row(NodeUiValueSource::Direct, "Idle entry", "1"),
                    value_row(NodeUiValueSource::Direct, "Default fade", "0.35 s"),
                    value_row(NodeUiValueSource::Direct, "Active entry", "2"),
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
        tabs: vec![
            NodeUiTab { label: "main" },
            NodeUiTab { label: "source" },
            NodeUiTab { label: "debug" },
        ],
        children: vec![
            NodeUiChild {
                label: "idle",
                detail: "./idle.toml",
                state: "fade_after 0.12 s",
                active: false,
            },
            NodeUiChild {
                label: "blast",
                detail: "./blast.toml",
                state: "active, trigger bus#trigger",
                active: true,
            },
        ],
    }
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
    path: &'static str,
    status: NodeUiStatus,
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
}

impl NodeUiStatus {
    const fn good(label: &'static str) -> Self {
        Self {
            label,
            tone: NodeUiStatusTone::Good,
        }
    }

    fn class_name(self) -> &'static str {
        match self.tone {
            NodeUiStatusTone::Good => "ux-node-ui-status ux-node-ui-status-good",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NodeUiStatusTone {
    Good,
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
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct NodeUiChild {
    label: &'static str,
    detail: &'static str,
    state: &'static str,
    active: bool,
}
