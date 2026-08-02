//! [`LogoMark`]: the interim LightPlayer mark.
//!
//! A WS2812 addressable-LED package — corner pads, lens, play triangle in
//! the lens — drawn as a single-color `currentColor` SVG so it inherits
//! whatever foreground its container sets (the chrome uses the teal accent).
//!
//! This is a deliberate placeholder: a human-designed logo is in progress
//! and will replace it. Keep the mark self-contained here so the swap is a
//! one-file change. Design record: `spikes/top-bar/index.html` §1 (mark E).

use dioxus::prelude::*;

/// The interim brand mark at `size`×`size` CSS pixels.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn LogoMark(size: u32) -> Element {
    rsx! {
        svg {
            width: "{size}",
            height: "{size}",
            view_box: "0 0 32 32",
            xmlns: "http://www.w3.org/2000/svg",
            "aria-hidden": "true",
            // Package outline.
            rect {
                x: "5.5",
                y: "5.5",
                width: "21",
                height: "21",
                rx: "2.4",
                stroke: "currentColor",
                stroke_width: "1.7",
                fill: "none",
            }
            // Corner solder pads.
            g { fill: "currentColor", opacity: "0.75",
                rect { x: "3.2", y: "8", width: "2.3", height: "4.6", rx: "1" }
                rect { x: "3.2", y: "19.4", width: "2.3", height: "4.6", rx: "1" }
                rect { x: "26.5", y: "8", width: "2.3", height: "4.6", rx: "1" }
                rect { x: "26.5", y: "19.4", width: "2.3", height: "4.6", rx: "1" }
            }
            // Lens.
            circle {
                cx: "16",
                cy: "16",
                r: "7.6",
                stroke: "currentColor",
                stroke_width: "1.5",
                fill: "none",
            }
            // Play, in the lens.
            path {
                d: "M13.9 13v6c0 .58.64.94 1.14.64l4.9-3a.75.75 0 0 0 0-1.28l-4.9-3a.75.75 0 0 0-1.14.64z",
                fill: "currentColor",
            }
        }
    }
}
