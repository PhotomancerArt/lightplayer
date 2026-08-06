//! [`MarkdownText`]: render UNTRUSTED markdown (model output) as Dioxus
//! nodes.
//!
//! A limited subset only: paragraphs, headings (demoted to small/strong —
//! chat bubbles don't want `h1`), bold/italic/strikethrough, inline code,
//! fenced code blocks (monospace, whitespace preserved, no highlighting),
//! lists, links (`target=_blank rel=noopener`, http(s)/mailto only),
//! blockquotes, hard breaks.
//!
//! [`MarkdownDocs`] renders the same tree for docs content: headings keep
//! their real level (`h1`/`h2`/`h3`, with slugified `id` anchors — see
//! [`slugify`] and [`heading_anchors`]) instead of the chat demotion, and it
//! optionally resolves `embed` directive fences (see below) into live
//! components.
//!
//! ## Embed directive fences
//!
//! A fenced code block whose info string's first word is `embed` —
//! ` ```embed knobs sim=disc mode=interactive ``` ` — parses into
//! [`MdNode::Embed`] instead of [`MdNode::CodeBlock`]. Grammar: `embed
//! <name> [key=value]...`; the fence body is reserved for future use and is
//! preserved on the node but never rendered directly. A fence with `embed`
//! but no name is malformed and parses as an ordinary [`MdNode::CodeBlock`]
//! (the raw fence text stays visible so the author sees the mistake — it is
//! never dropped or panicked on). Every other fence (no `embed` prefix)
//! parses exactly as before.
//!
//! Security: raw/inline HTML is rendered as escaped TEXT (Dioxus text
//! nodes escape by construction), link schemes are allowlisted, and no
//! `dangerous_inner_html` exists anywhere here. Parsing happens per render
//! — fine at chat sizes, and it lets streaming text re-render through the
//! same path.
//!
//! Embeds extend that posture rather than loosen it: resolving an `embed`
//! fence into a live component is opt in and docs-only. [`MarkdownDocs`]
//! takes an *optional* `embeds` resolver prop; when it is absent, `Embed`
//! nodes render as plain (inert) code blocks. [`MarkdownText`] — the chat
//! path, which renders genuinely untrusted model output — has no such prop
//! at all, so it can never summon a live component no matter what the
//! model emits; `Embed` nodes always fall back to a plain code block there.
//! See the `chat_mode_never_resolves_embeds` test.

use dioxus::prelude::*;
use pulldown_cmark::{CodeBlockKind, CowStr, Event, Options, Parser, Tag, TagEnd};
use std::collections::HashMap;

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
    /// A ` ```embed <name> [key=value]... ``` ` fence. Resolution is a
    /// docs-mode-only concern (see the module doc); parsing and holding
    /// this node is safe everywhere — it only ever *becomes* a live
    /// component through [`MarkdownDocs`]'s optional `embeds` resolver.
    Embed {
        name: String,
        args: Vec<(String, String)>,
        /// Fence body, reserved for future use; never rendered directly.
        body: String,
    },
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
    /// `info` is the fence's info string (`Some` for fenced blocks, `None`
    /// for indented ones) — carried through so the closing side can spot
    /// an `embed` directive.
    CodeBlock {
        info: Option<String>,
    },
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
            FrameKind::CodeBlock { info } => {
                let code = children
                    .into_iter()
                    .filter_map(|node| match node {
                        MdNode::Text(text) => Some(text),
                        _ => None,
                    })
                    .collect::<String>();
                self.push(code_block_or_embed(info, code));
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
        Tag::CodeBlock(kind) => FrameKind::CodeBlock {
            info: match kind {
                CodeBlockKind::Fenced(info) => Some(info.into_string()),
                CodeBlockKind::Indented => None,
            },
        },
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

/// Classify a fenced code block's body as a plain code block or an `embed`
/// directive. The `embed` keyword must be the info string's first
/// whitespace-delimited word; a bare `embed` with no following name is
/// malformed and degrades to a plain code block (never dropped, never a
/// panic) rather than an `Embed` node with an empty name.
fn code_block_or_embed(info: Option<String>, body: String) -> MdNode {
    if let Some(info) = info {
        let mut words = info.split_whitespace();
        if words.next() == Some("embed") {
            if let Some(name) = words.next() {
                let args = words
                    .filter_map(|word| {
                        word.split_once('=')
                            .map(|(k, v)| (k.to_string(), v.to_string()))
                    })
                    .collect();
                return MdNode::Embed {
                    name: name.to_string(),
                    args,
                    body,
                };
            }
        }
    }
    MdNode::CodeBlock(body)
}

/// Degraded-mode text for an [`MdNode::Embed`] that has no resolver in
/// scope: reconstruct the directive line so the fence stays visibly
/// meaningful instead of rendering as an empty block.
fn embed_fallback_text(name: &str, args: &[(String, String)], body: &str) -> String {
    let mut text = format!("embed {name}");
    for (key, value) in args {
        text.push(' ');
        text.push_str(key);
        text.push('=');
        text.push_str(value);
    }
    if !body.is_empty() {
        text.push('\n');
        text.push_str(body);
    }
    text
}

/// A parsed `embed` directive, handed to [`MarkdownDocs`]'s optional
/// `embeds` resolver. Never constructed for [`MarkdownText`] — chat mode
/// has no resolver prop to hand it to (see the module doc).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MdEmbedRef {
    pub name: String,
    pub args: Vec<(String, String)>,
    pub body: String,
}

/// Slugify heading text into an anchor id: lowercase, ASCII alphanumerics
/// kept, any run of other characters collapses to a single `-`, and the
/// result is trimmed (no leading/trailing `-`).
///
/// Kept intentionally tiny and pure: P3's build-time TOC/link-check script
/// needs bit-identical output but can't depend on this crate from
/// `build.rs`, so it duplicates this algorithm verbatim — see that crate
/// for a sample-parity test asserting the two never drift apart. If you
/// change this function, change both.
pub(crate) fn slugify(text: &str) -> String {
    let mut slug = String::with_capacity(text.len());
    let mut needs_dash = false;
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() {
            if needs_dash && !slug.is_empty() {
                slug.push('-');
            }
            needs_dash = false;
            slug.push(ch.to_ascii_lowercase());
        } else {
            needs_dash = true;
        }
    }
    slug
}

/// Flatten a heading's inline children into plain text for slugging.
/// Headings only ever contain inline content (text, emphasis/strong/code,
/// links, hard breaks), never block-level children.
fn heading_plain_text(children: &[MdNode]) -> String {
    let mut text = String::new();
    collect_heading_text(children, &mut text);
    text
}

fn collect_heading_text(children: &[MdNode], out: &mut String) {
    for child in children {
        match child {
            MdNode::Text(text) | MdNode::CodeInline(text) => out.push_str(text),
            MdNode::Strong(children)
            | MdNode::Emphasis(children)
            | MdNode::Strikethrough(children)
            | MdNode::Link { children, .. } => collect_heading_text(children, out),
            MdNode::HardBreak => out.push(' '),
            // Headings never contain these in practice; ignore rather than
            // guess at text content.
            MdNode::Paragraph(_)
            | MdNode::Heading { .. }
            | MdNode::CodeBlock(_)
            | MdNode::Embed { .. }
            | MdNode::List { .. }
            | MdNode::BlockQuote(_) => {}
        }
    }
}

/// Per-node anchor slugs: `Some(slug)` for each top-level [`MdNode::Heading`]
/// (deduped in document order with `-2`, `-3`, ... suffixes), `None` for
/// every other node. Shared by [`heading_anchors`] and [`MarkdownDocs`] so
/// the ids assigned in rendering and the ids reported for reuse never
/// diverge.
fn heading_anchor_by_node(nodes: &[MdNode]) -> Vec<Option<String>> {
    let mut seen: HashMap<String, u32> = HashMap::new();
    nodes
        .iter()
        .map(|node| match node {
            MdNode::Heading { children, .. } => {
                let slug = slugify(&heading_plain_text(children));
                let count = seen.entry(slug.clone()).or_insert(0);
                *count += 1;
                Some(if *count == 1 {
                    slug
                } else {
                    format!("{slug}-{count}")
                })
            }
            _ => None,
        })
        .collect()
}

/// Anchor ids for the top-level `h1`/`h2`/`h3` headings in `text`, in
/// document order — the same ids [`MarkdownDocs`] assigns as heading `id`
/// attributes. Host-testable so callers (e.g. a future table of contents)
/// don't need to re-derive the slugging/dedup rules themselves.
#[allow(
    dead_code,
    reason = "consumed by the generated docs checks (test-only) and a docs TOC in a later phase"
)]
pub(crate) fn heading_anchors(text: &str) -> Vec<String> {
    heading_anchor_by_node(&parse_markdown(text))
        .into_iter()
        .flatten()
        .collect()
}

// -- rendering ------------------------------------------------------------

/// Markdown rendered as a docs article: same parser and hardening as
/// [`MarkdownText`] (untrusted posture stays — escaped HTML, scheme
/// allowlist, no `dangerous_inner_html`), but headings keep their levels
/// (with slugified `id` anchors) with real docs styling instead of the chat
/// demotion, and `embed` fences resolve through the optional `embeds` prop.
///
/// `embeds` is the whole resolver seam: pass `None` (or nothing — it
/// defaults to `None`) for the same inert-code-block fallback
/// [`MarkdownText`] always uses; pass `Some(callback)` to turn `embed`
/// fences into live components. Only this component can ever do that.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn MarkdownDocs(
    text: String,
    #[props(default)] embeds: Option<Callback<MdEmbedRef, Element>>,
) -> Element {
    let nodes = parse_markdown(&text);
    let anchors = heading_anchor_by_node(&nodes);
    rsx! {
        div { class: "tw:min-w-0 tw:max-w-[72ch] tw:break-words tw:text-sm tw:leading-relaxed",
            for (index, node) in nodes.iter().enumerate() {
                Fragment { key: "{index}", {render_docs_node(node, anchors[index].as_deref(), embeds)} }
            }
        }
    }
}

/// Docs-mode node rendering: headings get level styles + an anchor `id`;
/// `Embed` nodes resolve through `embeds` when present, else fall back to
/// the same inert code block chat mode uses; every other node shares the
/// chat mapping (headings and embeds never nest inside those in practice —
/// mirroring the pre-existing heading-demotion note above).
fn render_docs_node(
    node: &MdNode,
    anchor: Option<&str>,
    embeds: Option<Callback<MdEmbedRef, Element>>,
) -> Element {
    match node {
        MdNode::Heading { level, children } => {
            let class = docs_heading_class(*level);
            let id = anchor.unwrap_or_default();
            match level {
                1 => rsx! { h1 { id: "{id}", class: "{class}", {render_children(children)} } },
                2 => rsx! { h2 { id: "{id}", class: "{class}", {render_children(children)} } },
                _ => rsx! { h3 { id: "{id}", class: "{class}", {render_children(children)} } },
            }
        }
        MdNode::Embed { name, args, body } => match embeds {
            Some(resolve) => resolve.call(MdEmbedRef {
                name: name.clone(),
                args: args.clone(),
                body: body.clone(),
            }),
            // No resolver in scope: same inert fallback as chat mode.
            None => render_node(node),
        },
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
        MdNode::CodeBlock(code) => render_code_block(code),
        // Chat mode (and the docs no-resolver fallback) never resolve
        // embeds: always the same inert code block, reconstructing the
        // directive line so the fence stays visibly meaningful.
        MdNode::Embed { name, args, body } => {
            render_code_block(&embed_fallback_text(name, args, body))
        }
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

/// Shared code-block chrome: used for real fenced code and for the inert
/// `embed` fallback (chat mode always, docs mode when no resolver applies).
fn render_code_block(code: &str) -> Element {
    rsx! {
        pre { class: "tw:m-0 tw:mb-1.5 tw:last:mb-0 tw:overflow-auto tw:rounded-xs tw:border tw:border-border-subtle tw:bg-card-muted tw:px-2 tw:py-1.5 tw:font-mono tw:text-xs tw:leading-snug tw:whitespace-pre-wrap tw:break-words",
            "{code}"
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

    #[test]
    fn well_formed_embed_fence_parses_into_embed_node() {
        let nodes = parse_markdown("```embed knobs sim=disc mode=interactive\n```");
        assert_eq!(
            nodes,
            vec![MdNode::Embed {
                name: "knobs".into(),
                args: vec![
                    ("sim".into(), "disc".into()),
                    ("mode".into(), "interactive".into()),
                ],
                body: String::new(),
            }]
        );
    }

    #[test]
    fn embed_fence_preserves_its_body_unrendered() {
        let nodes = parse_markdown("```embed knobs\nreserved for later\n```");
        assert_eq!(
            nodes,
            vec![MdNode::Embed {
                name: "knobs".into(),
                args: vec![],
                body: "reserved for later\n".into(),
            }]
        );
    }

    #[test]
    fn malformed_embed_fence_without_a_name_is_a_plain_code_block() {
        // "embed" alone, no name: never panics, never drops the fence
        // text — it just degrades to an ordinary code block.
        let nodes = parse_markdown("```embed\nsome code\n```");
        assert_eq!(nodes, vec![MdNode::CodeBlock("some code\n".into())]);
    }

    #[test]
    fn info_string_must_have_embed_as_its_own_first_word() {
        // "embedded-note" starts with the substring "embed" but is a
        // single word, not the `embed <name>` grammar — must stay a
        // normal fence, not be sniffed as a (malformed) directive.
        let nodes = parse_markdown("```embedded-note\nx\n```");
        assert_eq!(nodes, vec![MdNode::CodeBlock("x\n".into())]);
    }

    #[test]
    fn non_embed_fences_still_render_exactly_as_before() {
        // Guards against regressions from threading the info string
        // through FrameKind::CodeBlock.
        let nodes = parse_markdown("```glsl\nvec4 render(vec2 p) {\n  return vec4(1.0);\n}\n```");
        assert_eq!(
            nodes,
            vec![MdNode::CodeBlock(
                "vec4 render(vec2 p) {\n  return vec4(1.0);\n}\n".into()
            )]
        );
        // Indented code blocks (no info string at all) are unaffected too.
        let nodes = parse_markdown("    indented code\n");
        assert_eq!(nodes, vec![MdNode::CodeBlock("indented code\n".into())]);
    }

    #[test]
    fn embed_fallback_text_reconstructs_the_directive_line() {
        assert_eq!(
            embed_fallback_text("knobs", &[("sim".into(), "disc".into())], ""),
            "embed knobs sim=disc"
        );
        assert_eq!(
            embed_fallback_text("knobs", &[], "body line"),
            "embed knobs\nbody line"
        );
    }

    #[test]
    fn slugify_lowercases_and_collapses_punctuation_runs() {
        assert_eq!(slugify("What's a shader?"), "what-s-a-shader");
        assert_eq!(slugify("  Leading and trailing  "), "leading-and-trailing");
        assert_eq!(slugify("Already-Slugged"), "already-slugged");
        assert_eq!(slugify(""), "");
        assert_eq!(slugify("---"), "");
    }

    #[test]
    fn heading_anchors_dedupes_repeated_slugs_in_document_order() {
        let anchors = heading_anchors("# Notes\n\ntext\n\n## Notes\n\nmore\n\n## Notes\n");
        assert_eq!(anchors, vec!["notes", "notes-2", "notes-3"]);
    }

    #[test]
    fn heading_anchors_only_covers_top_level_headings() {
        // A heading nested inside a blockquote never gets rendered with an
        // id (see render_docs_node's doc comment) — heading_anchors must
        // match what actually gets an id, not every Heading node anywhere
        // in the tree.
        let anchors = heading_anchors("# Title\n\n> ## Nested, ignored\n");
        assert_eq!(anchors, vec!["title"]);
    }

    #[test]
    fn chat_mode_never_resolves_embeds() {
        // `MarkdownText` (chat) has no `embeds` prop at all — the type
        // signature is the primary guarantee (there is nothing to pass a
        // resolver through), but this pins the fallback path parsing
        // produces so a future refactor can't quietly wire chat mode up to
        // a resolver: an `embed` fence must remain indistinguishable, at
        // the tree level, from "just some node with no live-component
        // hook", i.e. it always carries only inert String/Vec data.
        let nodes = parse_markdown("```embed knobs sim=disc\n```");
        let [MdNode::Embed { name, args, body }] = nodes.as_slice() else {
            panic!("expected a single Embed node: {nodes:?}");
        };
        assert_eq!(name, "knobs");
        assert_eq!(args, &vec![("sim".to_string(), "disc".to_string())]);
        assert_eq!(body, "");

        // MarkdownText's render path (render_node) maps every Embed to the
        // same inert fallback text CodeBlock rendering uses — never a
        // distinct "live component" arm. Assert that by construction: the
        // fallback text for this node is exactly what a plain code block
        // fence with the same visible content would show.
        assert_eq!(
            embed_fallback_text(name, args, body),
            "embed knobs sim=disc"
        );
    }
}
