//! Code-figure stories: the highlight-tone matrix, and the real registered
//! plasma figure an article shows.

use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;

use crate::app::docs::code_figures::code_figure;
use crate::base::code_figure::{CodeFigure, CodeHighlight, CodeHighlightTone};

/// A short GLSL-ish sample: enough shape to read, short enough to fit.
const SAMPLE: &str = "layout(binding = 0) uniform vec2 outputSize;\n\
                      layout(binding = 1) uniform float speed;\n\
                      \n\
                      vec4 render(vec2 pos) {\n    \
                          vec2 uv = pos / outputSize;\n    \
                          float v = sin(uv.x * 6.2831853 + speed);\n    \
                          return vec4(vec3(v), 1.0);\n\
                      }\n";

fn bound(lines: std::ops::RangeInclusive<usize>, label: Option<&str>) -> CodeHighlight {
    CodeHighlight {
        lines,
        tone: CodeHighlightTone::Bound,
        label: label.map(str::to_string),
    }
}

#[story(
    description = "Plain listing: quiet gutter, no highlights, titled with the file it came from."
)]
fn no_highlights() -> Element {
    rsx! {
        div { class: "tw:w-[520px]",
            CodeFigure { code: SAMPLE.to_string(), title: "shader.glsl" }
        }
    }
}

#[story(
    description = "The money beat: one violet range on the uniform a knob drives, labeled with the knob's name. Violet is Studio's binding color — never green."
)]
fn one_bound_range() -> Element {
    rsx! {
        div { class: "tw:w-[520px]",
            CodeFigure {
                code: SAMPLE.to_string(),
                title: "shader.glsl",
                highlights: vec![bound(2..=2, Some("the Speed knob"))],
            }
        }
    }
}

#[story(
    description = "Several ranges at once: a labeled violet binding line, a multi-line violet range, and a neutral Note range that makes no status claim."
)]
fn several_ranges() -> Element {
    rsx! {
        div { class: "tw:w-[520px]",
            CodeFigure {
                code: SAMPLE.to_string(),
                title: "shader.glsl",
                highlights: vec![
                    bound(2..=2, Some("the Speed knob")),
                    CodeHighlight {
                        lines: 4..=5,
                        tone: CodeHighlightTone::Note,
                        label: Some("every frame, every pixel".to_string()),
                    },
                    bound(6..=6, None),
                ],
            }
        }
    }
}

#[story(
    description = "Long and wide code scrolls inside the figure — vertically past the cap, horizontally past the frame — so an article never scrolls the page sideways. The label chip stays stuck to the right edge."
)]
fn long_code_scrolls_inside() -> Element {
    let mut code = String::from(
        "// A listing far taller than the figure's cap, with one line wide \
         enough to force horizontal scrolling inside the frame.\n",
    );
    for index in 0..40 {
        code.push_str(&format!("    float field{index} = sin(uv.x * {index}.0 + speed * {index}.0) + cos(uv.y * {index}.0);\n"));
    }

    rsx! {
        div { class: "tw:w-[520px]",
            CodeFigure {
                code,
                title: "long.glsl",
                highlights: vec![bound(3..=4, Some("bound rows stay findable"))],
            }
        }
    }
}

#[story(
    description = "The registered `plasma-shader` figure, compiled in from examples/plasma/shader.glsl with its hand-authored range on the uniform the Scale knob drives."
)]
fn registered_plasma_figure() -> Element {
    let figure = code_figure("plasma-shader").expect("the plasma figure is registered");
    rsx! {
        div { class: "tw:w-[560px]",
            CodeFigure {
                code: figure.code.to_string(),
                title: figure.title,
                highlights: figure.highlights(),
            }
        }
    }
}
