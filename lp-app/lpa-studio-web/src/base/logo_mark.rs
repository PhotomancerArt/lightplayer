//! [`LogoMark`] and [`LogoLockup`]: the interim LightPlayer brand.
//!
//! The mark is a WS2812 addressable-LED package — corner pads, lens, play
//! triangle in the lens. It is deliberately two-tone: the package (its
//! "chip outline") takes `currentColor` so containers theme it, while the
//! play triangle stays the bright brand accent. The wordmark then matches
//! the package tone, so mark and text read as ONE lockup rather than an
//! icon sitting next to a label.
//!
//! Two sizes ship: [`LogoLockup`] (wide — mark + wordmark) and the bare
//! [`LogoMark`] (small — the mark alone, for narrow bars and favicons).
//!
//! This is a deliberate placeholder: a human-designed logo is in progress
//! and will replace it. Keep the brand self-contained here so the swap is a
//! one-file change. Design record: `spikes/top-bar/index.html` §1 (mark E).

use dioxus::prelude::*;

/// The interim brand mark at `size`×`size` CSS pixels. The package inherits
/// `currentColor`; the play triangle is always the accent.
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
                opacity: "0.85",
            }
            // Play, in the lens — the one bright element in the lockup.
            path {
                class: "tw:fill-accent",
                d: "M13.9 13v6c0 .58.64.94 1.14.64l4.9-3a.75.75 0 0 0 0-1.28l-4.9-3a.75.75 0 0 0-1.14.64z",
            }
        }
    }
}

/// The wide brand lockup: mark + wordmark in one tone, tightly set so it
/// reads as a single unit. `compact` drops the wordmark (the small form).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn LogoLockup(
    #[props(default = 22)] size: u32,
    #[props(default = false)] compact: bool,
) -> Element {
    rsx! {
        span {
            class: "tw:flex tw:flex-none tw:cursor-default tw:items-center tw:gap-1.5 tw:text-heading",
            title: "LightPlayer",
            LogoMark { size }
            if !compact {
                span {
                    class: "tw:text-[13px] tw:font-extrabold tw:leading-none tw:tracking-[-0.015em] tw:max-[560px]:hidden",
                    "LightPlayer"
                }
            }
        }
    }
}
