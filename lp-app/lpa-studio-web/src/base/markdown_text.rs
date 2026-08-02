//! [`MarkdownText`]: render UNTRUSTED markdown (model output) as Dioxus
//! nodes.
//!
//! A limited subset only: paragraphs, headings (demoted to small/strong —
//! chat bubbles don't want `h1`), bold/italic/strikethrough, inline code,
//! fenced code blocks (monospace, whitespace preserved, no highlighting),
//! lists, links (`target=_blank rel=noopener`, http(s)/mailto only),
//! blockquotes, hard breaks.
//!
//! Security: raw/inline HTML is rendered as escaped TEXT (Dioxus text
//! nodes escape by construction), link schemes are allowlisted, and no
//! `dangerous_inner_html` exists anywhere here. Parsing happens per render
//! — fine at chat sizes, and it lets streaming text re-render through the
//! same path.

use dioxus::prelude::*;
use pulldown_cmark::{CowStr, Event, Options, Parser, Tag, TagEnd};

/// Markdown rendered inline (agent replies). The container is a block
/// element; inter-block spacing lives on the blocks themselves.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn MarkdownText(text: String) -> Element {
    let nodes = parse_markdown(&text);
    rsx! {
        div { class: "tw:min-w-0 tw:break-words",
            for (index, node) in nodes.iter().enumerate() {
                Fragment { key: "{index}", {render_node(node)} }
            }
        }
    }
}

/// The intermediate tree: pulldown events folded into renderable nodes.
/// Kept host-testable — the event→node mapping (including the HTML-escape
/// and link-scheme rules) is exercised without a browser.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MdNode {
    Paragraph(Vec<MdNode>),
    /// A heading with its source level. Chat rendering demotes every
    /// level to one small strong style; docs rendering keeps the levels.
    Heading {
        level: u8,
        children: Vec<MdNode>,
    },
    Text(String),
    Strong(Vec<MdNode>),
    Emphasis(Vec<MdNode>),
    Strikethrough(Vec<MdNode>),
    CodeInline(String),
    CodeBlock(String),
    List {
        ordered: bool,
        items: Vec<Vec<MdNode>>,
    },
    Link {
        href: String,
        children: Vec<MdNode>,
    },
    BlockQuote(Vec<MdNode>),
    HardBreak,
}

/// Parse markdown into the render tree. Strikethrough is the only enabled
/// extension; everything else stays CommonMark.
pub(crate) fn parse_markdown(text: &str) -> Vec<MdNode> {
    let parser = Parser::new_ext(text, Options::ENABLE_STRIKETHROUGH);
    let mut builder = TreeBuilder::new();
    for event in parser {
        builder.on_event(event);
    }
    builder.finish()
}

/// One open container while folding the event stream.
struct Frame {
    kind: FrameKind,
    children: Vec<MdNode>,
}

enum FrameKind {
    Paragraph,
    Heading {
        level: u8,
    },
    Strong,
    Emphasis,
    Strikethrough,
    CodeBlock,
    List {
        ordered: bool,
        items: Vec<Vec<MdNode>>,
    },
    Item,
    Link {
        href: String,
    },
    BlockQuote,
    /// Container we deliberately flatten (e.g. images → their alt text) or
    /// don't model; children splice into the parent.
    Passthrough,
}

struct TreeBuilder {
    stack: Vec<Frame>,
    root: Vec<MdNode>,
}

impl TreeBuilder {
    fn new() -> Self {
        Self {
            stack: Vec::new(),
            root: Vec::new(),
        }
    }

    fn on_event(&mut self, event: Event<'_>) {
        match event {
            Event::Start(tag) => self.open(frame_kind(tag)),
            Event::End(tag) => self.close(tag),
            Event::Text(text) => self.push_text(text),
            Event::Code(code) => self.push(MdNode::CodeInline(code.into_string())),
            // Untrusted HTML never passes through as markup: it becomes
            // literal text (escaped by the text-node rendering).
            Event::Html(html) | Event::InlineHtml(html) => self.push_text(html),
            Event::SoftBreak => self.push(MdNode::Text(" ".to_string())),
            Event::HardBreak => self.push(MdNode::HardBreak),
            // Outside the subset: rules and footnotes/math/task markers
            // contribute nothing.
            Event::Rule
            | Event::FootnoteReference(_)
            | Event::TaskListMarker(_)
            | Event::InlineMath(_)
            | Event::DisplayMath(_) => {}
        }
    }

    fn open(&mut self, kind: FrameKind) {
        self.stack.push(Frame {
            kind,
            children: Vec::new(),
        });
    }

    fn close(&mut self, _tag: TagEnd) {
        let Some(frame) = self.stack.pop() else {
            return; // unbalanced end; ignore
        };
        let children = frame.children;
        match frame.kind {
            FrameKind::Paragraph => self.push(MdNode::Paragraph(children)),
            FrameKind::Heading { level } => self.push(MdNode::Heading { level, children }),
            FrameKind::Strong => self.push(MdNode::Strong(children)),
            FrameKind::Emphasis => self.push(MdNode::Emphasis(children)),
            FrameKind::Strikethrough => self.push(MdNode::Strikethrough(children)),
            FrameKind::CodeBlock => {
                let code = children
                    .into_iter()
                    .filter_map(|node| match node {
                        MdNode::Text(text) => Some(text),
                        _ => None,
                    })
                    .collect::<String>();
                self.push(MdNode::CodeBlock(code));
            }
            FrameKind::List { ordered, items } => self.push(MdNode::List { ordered, items }),
            FrameKind::Item => {
                // An item's nodes attach to the enclosing list's items.
                if let Some(Frame {
                    kind: FrameKind::List { items, .. },
                    ..
                }) = self.stack.last_mut()
                {
                    items.push(children);
                }
            }
            FrameKind::Link { href } => match safe_href(&href) {
                // Scheme allowlist: anything else renders as plain content.
                true => self.push(MdNode::Link { href, children }),
                false => self.splice(children),
            },
            FrameKind::BlockQuote => self.push(MdNode::BlockQuote(children)),
            FrameKind::Passthrough => self.splice(children),
        }
    }

    fn push_text(&mut self, text: CowStr<'_>) {
        self.push(MdNode::Text(text.into_string()));
    }

    fn push(&mut self, node: MdNode) {
        match self.stack.last_mut() {
            Some(frame) => frame.children.push(node),
            None => self.root.push(node),
        }
    }

    fn splice(&mut self, nodes: Vec<MdNode>) {
        for node in nodes {
            self.push(node);
        }
    }

    fn finish(mut self) -> Vec<MdNode> {
        // Unclosed containers (streaming text mid-block): fold what exists.
        while let Some(frame) = self.stack.pop() {
            let mut children = frame.children;
            match self.stack.last_mut() {
                Some(parent) => parent.children.append(&mut children),
                None => self.root.append(&mut children),
            }
        }
        self.root
    }
}

fn frame_kind(tag: Tag<'_>) -> FrameKind {
    match tag {
        Tag::Paragraph => FrameKind::Paragraph,
        Tag::Heading { level, .. } => FrameKind::Heading { level: level as u8 },
        Tag::Strong => FrameKind::Strong,
        Tag::Emphasis => FrameKind::Emphasis,
        Tag::Strikethrough => FrameKind::Strikethrough,
        Tag::CodeBlock(_) => FrameKind::CodeBlock,
        Tag::List(start) => FrameKind::List {
            ordered: start.is_some(),
            items: Vec::new(),
        },
        Tag::Item => FrameKind::Item,
        Tag::Link { dest_url, .. } => FrameKind::Link {
            href: dest_url.into_string(),
        },
        Tag::BlockQuote(_) => FrameKind::BlockQuote,
        // Images flatten to their alt text; everything unmodeled passes
        // its content through.
        _ => FrameKind::Passthrough,
    }
}

/// Link scheme allowlist for untrusted hrefs.
fn safe_href(href: &str) -> bool {
    let lower = href.trim().to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("mailto:")
}

// -- rendering ------------------------------------------------------------

/// Markdown rendered as a docs article: same parser and hardening as
/// [`MarkdownText`] (untrusted posture stays — escaped HTML, scheme
/// allowlist, no `dangerous_inner_html`), but headings keep their levels
/// with real docs styling instead of the chat demotion.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn MarkdownDocs(text: String) -> Element {
    let nodes = parse_markdown(&text);
    rsx! {
        div { class: "tw:min-w-0 tw:max-w-[72ch] tw:break-words tw:text-sm tw:leading-relaxed",
            for (index, node) in nodes.iter().enumerate() {
                Fragment { key: "{index}", {render_docs_node(node)} }
            }
        }
    }
}

/// Docs-mode node rendering: headings get level styles; every other node
/// shares the chat mapping (headings never nest inside those in practice).
fn render_docs_node(node: &MdNode) -> Element {
    match node {
        MdNode::Heading { level, children } => {
            let class = docs_heading_class(*level);
            match level {
                1 => rsx! { h1 { class: "{class}", {render_children(children)} } },
                2 => rsx! { h2 { class: "{class}", {render_children(children)} } },
                _ => rsx! { h3 { class: "{class}", {render_children(children)} } },
            }
        }
        other => render_node(other),
    }
}

/// Heading classes by level; deeper than h3 clamps to the h3 style.
fn docs_heading_class(level: u8) -> &'static str {
    match level {
        1 => "tw:m-0 tw:mb-3 tw:text-lg tw:font-bold tw:text-strong-foreground",
        2 => "tw:m-0 tw:mt-5 tw:mb-2 tw:first:mt-0 tw:text-[15px] tw:font-bold tw:text-heading",
        _ => {
            "tw:m-0 tw:mt-4 tw:mb-1.5 tw:first:mt-0 tw:text-sm tw:font-bold tw:text-strong-foreground"
        }
    }
}

fn render_node(node: &MdNode) -> Element {
    match node {
        MdNode::Paragraph(children) => rsx! {
            p { class: "tw:m-0 tw:mb-1.5 tw:last:mb-0", {render_children(children)} }
        },
        MdNode::Heading { children, .. } => rsx! {
            // Demoted heading: a small strong block, never h1-h6 chrome.
            p { class: "tw:m-0 tw:mt-2 tw:mb-1 tw:first:mt-0 tw:text-sm tw:font-bold tw:text-strong-foreground",
                {render_children(children)}
            }
        },
        MdNode::Text(text) => rsx! { "{text}" },
        MdNode::Strong(children) => rsx! {
            strong { class: "tw:font-bold tw:text-strong-foreground", {render_children(children)} }
        },
        MdNode::Emphasis(children) => rsx! {
            em { {render_children(children)} }
        },
        MdNode::Strikethrough(children) => rsx! {
            s { {render_children(children)} }
        },
        MdNode::CodeInline(code) => rsx! {
            code { class: "tw:rounded-xs tw:bg-card-muted tw:px-1 tw:font-mono tw:text-[0.85em]",
                "{code}"
            }
        },
        MdNode::CodeBlock(code) => rsx! {
            pre { class: "tw:m-0 tw:mb-1.5 tw:last:mb-0 tw:overflow-auto tw:rounded-xs tw:border tw:border-border-subtle tw:bg-card-muted tw:px-2 tw:py-1.5 tw:font-mono tw:text-xs tw:leading-snug tw:whitespace-pre-wrap tw:break-words",
                "{code}"
            }
        },
        MdNode::List { ordered, items } => {
            let item_nodes = items.iter().enumerate().map(|(index, item)| {
                rsx! {
                    li { key: "{index}", class: "tw:mb-0.5", {render_children(item)} }
                }
            });
            if *ordered {
                rsx! {
                    ol { class: "tw:m-0 tw:mb-1.5 tw:list-decimal tw:pl-5 tw:last:mb-0", {item_nodes} }
                }
            } else {
                rsx! {
                    ul { class: "tw:m-0 tw:mb-1.5 tw:list-disc tw:pl-5 tw:last:mb-0", {item_nodes} }
                }
            }
        }
        MdNode::Link { href, children } => rsx! {
            a {
                class: "tw:text-accent tw:underline",
                href: "{href}",
                target: "_blank",
                rel: "noopener noreferrer",
                {render_children(children)}
            }
        },
        MdNode::BlockQuote(children) => rsx! {
            blockquote { class: "tw:m-0 tw:mb-1.5 tw:border-l-2 tw:border-border-strong tw:pl-2 tw:text-subtle-foreground tw:last:mb-0",
                {render_children(children)}
            }
        },
        MdNode::HardBreak => rsx! {
            br {}
        },
    }
}

fn render_children(children: &[MdNode]) -> Element {
    rsx! {
        for (index, child) in children.iter().enumerate() {
            Fragment { key: "{index}", {render_node(child)} }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paragraphs_and_inline_styles_map() {
        let nodes = parse_markdown("Hello **bold** and *soft* and ~~gone~~ and `code`.");
        let MdNode::Paragraph(children) = &nodes[0] else {
            panic!("expected paragraph: {nodes:?}");
        };
        assert!(children.contains(&MdNode::Strong(vec![MdNode::Text("bold".into())])));
        assert!(children.contains(&MdNode::Emphasis(vec![MdNode::Text("soft".into())])));
        assert!(children.contains(&MdNode::Strikethrough(vec![MdNode::Text("gone".into())])));
        assert!(children.contains(&MdNode::CodeInline("code".into())));
    }

    #[test]
    fn headings_keep_their_level_in_the_tree() {
        for (source, level) in [("# Title", 1), ("### Title", 3)] {
            let nodes = parse_markdown(source);
            assert_eq!(
                nodes,
                vec![MdNode::Heading {
                    level,
                    children: vec![MdNode::Text("Title".into())]
                }]
            );
        }
    }

    #[test]
    fn docs_heading_classes_step_down_by_level() {
        // h1 is the article title; deeper levels step down and clamp.
        assert_ne!(docs_heading_class(1), docs_heading_class(2));
        assert_eq!(docs_heading_class(4), docs_heading_class(6));
    }

    #[test]
    fn fenced_code_blocks_keep_their_text() {
        let nodes = parse_markdown("```glsl\nvec4 render(vec2 p) {\n  return vec4(1.0);\n}\n```");
        assert_eq!(
            nodes,
            vec![MdNode::CodeBlock(
                "vec4 render(vec2 p) {\n  return vec4(1.0);\n}\n".into()
            )]
        );
    }

    #[test]
    fn lists_collect_items() {
        let nodes = parse_markdown("1. one\n2. two\n\n- a\n- b");
        let MdNode::List { ordered, items } = &nodes[0] else {
            panic!("expected list: {nodes:?}");
        };
        assert!(*ordered);
        assert_eq!(items.len(), 2);
        let MdNode::List { ordered, items } = &nodes[1] else {
            panic!("expected list: {nodes:?}");
        };
        assert!(!*ordered);
        assert_eq!(items[1], vec![MdNode::Text("b".into())]);
    }

    #[test]
    fn http_links_survive_but_scripty_schemes_are_stripped() {
        let nodes = parse_markdown("[ok](https://example.com) [bad](javascript:alert(1))");
        let MdNode::Paragraph(children) = &nodes[0] else {
            panic!("expected paragraph: {nodes:?}");
        };
        assert!(children.iter().any(|node| matches!(
            node,
            MdNode::Link { href, .. } if href == "https://example.com"
        )));
        // The dangerous link degrades to its text; no Link node carries it.
        assert!(children.contains(&MdNode::Text("bad".into())));
        assert_eq!(
            children
                .iter()
                .filter(|node| matches!(node, MdNode::Link { .. }))
                .count(),
            1
        );
    }

    #[test]
    fn raw_html_becomes_escaped_text_not_markup() {
        let nodes = parse_markdown("before <img src=x onerror=alert(1)> after");
        let MdNode::Paragraph(children) = &nodes[0] else {
            panic!("expected paragraph: {nodes:?}");
        };
        // The HTML arrives as a literal text node (Dioxus escapes text on
        // render), never as a modeled element.
        assert!(
            children.contains(&MdNode::Text("<img src=x onerror=alert(1)>".into())),
            "{children:?}"
        );

        let nodes = parse_markdown("<script>alert(1)</script>");
        assert!(
            nodes
                .iter()
                .all(|node| !matches!(node, MdNode::Link { .. })),
        );
        fn all_text_only(nodes: &[MdNode]) -> bool {
            nodes.iter().all(|node| match node {
                MdNode::Text(_) => true,
                MdNode::Paragraph(children) => all_text_only(children),
                _ => false,
            })
        }
        assert!(all_text_only(&nodes), "{nodes:?}");
    }

    #[test]
    fn blockquotes_and_hard_breaks_map() {
        let nodes = parse_markdown("> quoted");
        assert_eq!(
            nodes,
            vec![MdNode::BlockQuote(vec![MdNode::Paragraph(vec![
                MdNode::Text("quoted".into())
            ])])]
        );
        let nodes = parse_markdown("line one  \nline two");
        let MdNode::Paragraph(children) = &nodes[0] else {
            panic!("expected paragraph: {nodes:?}");
        };
        assert!(children.contains(&MdNode::HardBreak));
    }

    #[test]
    fn streaming_prefix_with_unclosed_blocks_still_yields_content() {
        // Mid-stream text: an unterminated bold + fence must not drop text.
        let nodes = parse_markdown("Shifting the **base col");
        fn contains_text(nodes: &[MdNode], needle: &str) -> bool {
            nodes.iter().any(|node| match node {
                MdNode::Text(text) => text.contains(needle),
                MdNode::Paragraph(c)
                | MdNode::Heading { children: c, .. }
                | MdNode::Strong(c)
                | MdNode::Emphasis(c)
                | MdNode::Strikethrough(c)
                | MdNode::BlockQuote(c)
                | MdNode::Link { children: c, .. } => contains_text(c, needle),
                _ => false,
            })
        }
        assert!(contains_text(&nodes, "base col"), "{nodes:?}");
    }

    #[test]
    fn images_flatten_to_alt_text() {
        let nodes = parse_markdown("![alt words](https://example.com/x.png)");
        let MdNode::Paragraph(children) = &nodes[0] else {
            panic!("expected paragraph: {nodes:?}");
        };
        assert_eq!(children, &vec![MdNode::Text("alt words".into())]);
    }
}
