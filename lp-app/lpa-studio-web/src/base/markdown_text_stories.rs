//! Stories for the safe markdown renderer.

use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;

use crate::base::markdown_text::MarkdownText;

#[story(
    description = "The full supported subset: demoted headings, inline styles, code, lists, a link, a blockquote — and raw HTML rendered as escaped text, never markup."
)]
pub(crate) fn markdown_subset() -> Element {
    let text = "\
# Big heading (demoted)\n\
### Small heading (same style)\n\n\
Inline **bold**, *italic*, ~~struck~~, and `code`.\n\n\
```glsl\nvec4 render_2d(vec2 pos) {\n    return vec4(pos, 0.0, 1.0);\n}\n```\n\n\
1. First step\n\
2. Second step\n\n\
- Unordered too\n\
- With a [link](https://example.com)\n\n\
> A quoted aside.\n\n\
Raw HTML stays text: <b>not bold</b> <script>alert(1)</script>\n\n\
A [javascript link](javascript:alert(1)) degrades to plain text.";
    rsx! {
        div { class: "tw:w-full tw:max-w-xl tw:rounded-md tw:border tw:border-border tw:bg-card tw:p-3 tw:text-sm tw:text-muted-foreground",
            MarkdownText { text }
        }
    }
}
