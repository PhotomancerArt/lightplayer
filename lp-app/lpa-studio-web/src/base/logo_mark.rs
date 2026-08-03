//! The LightPlayer brand: [`LogoMark`], [`LogoLockup`], and [`LogoStacked`].
//!
//! Source of truth for the brand. The mark is a WS2811-style addressable-LED
//! package — slim corner pads, lens ring, play triangle in the lens — drawn
//! "datasheet fine": thin silkscreen strokes, near-square corners. Every part
//! takes `currentColor`, so containers theme the whole mark (in-app chrome
//! sets it strong white; mono/print contexts set ink).
//!
//! Motion rules (spike gates 1–3, 2026-08-03):
//! - **In-app lockup**: the mark is static all-white; ONLY the word "Light"
//!   sweeps the LED rainbow ("D3" — the chip is hardware, the light is in
//!   the name). Always on; `prefers-reduced-motion` freezes it.
//! - **Icon-only forms** (favicon, avatar, app icon): no word to carry the
//!   light, so the play triangle cycles instead (`LogoMark { animated }`).
//! - **Marketing surfaces**: the loud full-rainbow wordmark
//!   (`marketing: true`).
//!
//! Derived assets update with this file: brand PNGs come from
//! `logo_mark_stories.rs` captures, and `public/favicon.svg` is generated
//! from the same geometry — `favicon_in_sync` fails on drift; regenerate via
//! `cargo test -p lpa-studio-web favicon_regen -- --ignored`.
//!
//! Design record: `spikes/lightplayer-logo/index.html` (PR #304); heritage
//! motifs from the 2014 Light at Play archive.

use dioxus::prelude::*;

// ---- geometry (shared by the component and the favicon generator) ----
// viewBox is 32×32. Package = "H · Datasheet fine" from the spike: rx 1.9
// (gate-3: "a hair" rounder than 1.3), stroke 1.25, slim pads, no pin-1 dot
// (gate-2: accurate but visual noise).
const PKG: (f32, f32, f32, f32, f32, f32) = (5.5, 5.5, 21.0, 21.0, 1.9, 1.25);
const PADS: [(f32, f32); 4] = [(3.6, 8.2), (26.5, 8.2), (26.5, 19.6), (3.6, 19.6)];
const PAD_SIZE: (f32, f32, f32) = (1.9, 4.2, 0.6);
const LENS: (f32, f32, f32, f32) = (16.0, 16.0, 7.6, 1.05);
const PLAY_D: &str =
    "M13.9 13v6c0 .58.64.94 1.14.64l4.9-3a.75.75 0 0 0 0-1.28l-4.9-3a.75.75 0 0 0-1.14.64z";

/// The brand mark at `size`×`size` CSS pixels, entirely `currentColor`.
/// `animated` is the icon-only form: the play triangle cycles the LED
/// rainbow (starts on the brand accent, so frozen frames look canonical).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn LogoMark(size: u32, #[props(default = false)] animated: bool) -> Element {
    let (px, py, pw, ph, prx, psw) = PKG;
    let (lcx, lcy, lr, lsw) = LENS;
    let (padw, padh, padrx) = PAD_SIZE;
    let play_class = if animated { "lp-brand-play-anim" } else { "" };
    rsx! {
        svg {
            width: "{size}",
            height: "{size}",
            view_box: "0 0 32 32",
            xmlns: "http://www.w3.org/2000/svg",
            "aria-hidden": "true",
            rect {
                x: "{px}", y: "{py}", width: "{pw}", height: "{ph}", rx: "{prx}",
                stroke: "currentColor", stroke_width: "{psw}", fill: "none",
            }
            g { fill: "currentColor", opacity: "0.75",
                for (x, y) in PADS.iter() {
                    rect { x: "{x}", y: "{y}", width: "{padw}", height: "{padh}", rx: "{padrx}" }
                }
            }
            circle {
                cx: "{lcx}", cy: "{lcy}", r: "{lr}",
                stroke: "currentColor", stroke_width: "{lsw}", fill: "none", opacity: "0.85",
            }
            path { class: "{play_class}", fill: "currentColor", d: PLAY_D }
        }
    }
}

/// The wordmark text, shared by the lockup forms. `mono` renders everything
/// `currentColor` (print/one-color contexts); `marketing` runs the full
/// rainbow sweep; otherwise "Light" alone carries the rainbow (D3).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn BrandWord(
    word_px: u32,
    #[props(default = false)] mono: bool,
    #[props(default = false)] marketing: bool,
) -> Element {
    let style = format!("font-size:{word_px}px");
    if mono {
        return rsx! {
            span {
                class: "tw:font-extrabold tw:leading-none tw:tracking-[-0.015em]",
                style: "{style}",
                "LightPlayer"
            }
        };
    }
    let light_class = if marketing {
        "lp-brand-word-full"
    } else {
        "lp-brand-light"
    };
    let player_class = if marketing {
        "lp-brand-word-full"
    } else {
        "tw:text-strong"
    };
    rsx! {
        span {
            class: "tw:font-extrabold tw:leading-none tw:tracking-[-0.015em]",
            style: "{style}",
            span { class: "{light_class}", "Light" }
            span { class: "{player_class}", "Player" }
        }
    }
}

/// The wide brand lockup: mark + wordmark, one unit. Defaults to the in-app
/// D3 treatment (white mark, "Light" animates). `compact` drops the wordmark
/// and switches to the icon-only rule (animated triangle). `marketing` runs
/// the loud form: full-rainbow word AND the triangle cycles with it (accent
/// when frozen). `mono` is the one-color form for print/light contexts —
/// set `color` on a parent.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn LogoLockup(
    #[props(default = 22)] size: u32,
    #[props(default = false)] compact: bool,
    #[props(default = false)] mono: bool,
    #[props(default = false)] marketing: bool,
) -> Element {
    // Optical norm: mark→word gap ≈ 0.5× wordmark cap height measured from
    // the mark's visual edge; the viewBox carries ~9% right-side air (pad
    // overhang), so 0.12×size lands on the norm.
    let gap = ((size as f32) * 0.12).round().max(2.0) as u32;
    let word_px = ((size as f32) * 0.61).round() as u32;
    let tone = if mono { "" } else { "tw:text-strong" };
    rsx! {
        span {
            class: "tw:flex tw:flex-none tw:cursor-default tw:items-center {tone}",
            style: "gap:{gap}px",
            title: "LightPlayer",
            LogoMark { size, animated: (compact || marketing) && !mono }
            if !compact {
                span { class: "tw:max-[560px]:hidden tw:flex",
                    BrandWord { word_px, mono, marketing }
                }
            }
        }
    }
}

/// The stacked/square form: mark above wordmark, centered. For app-icon
/// tiles, social cards, and splash surfaces.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn LogoStacked(
    #[props(default = 64)] size: u32,
    #[props(default = false)] mono: bool,
    #[props(default = false)] marketing: bool,
) -> Element {
    let gap = ((size as f32) * 0.14).round() as u32;
    let word_px = ((size as f32) * 0.28).round() as u32;
    let tone = if mono { "" } else { "tw:text-strong" };
    rsx! {
        span {
            class: "tw:flex tw:flex-none tw:cursor-default tw:flex-col tw:items-center {tone}",
            style: "gap:{gap}px",
            title: "LightPlayer",
            LogoMark { size }
            BrandWord { word_px, mono, marketing }
        }
    }
}

/// `public/favicon.svg`, generated from the same geometry as [`LogoMark`].
/// Ink follows the browser's color scheme; the play triangle holds the brand
/// accent (the static frame of the icon-only animation).
pub fn favicon_svg() -> String {
    let (px, py, pw, ph, prx, psw) = PKG;
    let (lcx, lcy, lr, lsw) = LENS;
    let (padw, padh, padrx) = PAD_SIZE;
    let pads = PADS
        .iter()
        .map(|(x, y)| {
            format!(
                "    <rect class=\"i\" x=\"{x}\" y=\"{y}\" width=\"{padw}\" height=\"{padh}\" rx=\"{padrx}\"/>\n"
            )
        })
        .collect::<String>();
    format!(
        "<!-- GENERATED from src/base/logo_mark.rs (favicon_svg) — do not edit by hand.\n     \
         Regenerate: cargo test -p lpa-studio-web favicon_regen -- --ignored -->\n\
         <svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 32 32\">\n  \
         <style>\n    \
         .s {{ stroke: #14181d; }} .i {{ fill: #14181d; }}\n    \
         @media (prefers-color-scheme: dark) {{ .s {{ stroke: #fffaf0; }} .i {{ fill: #fffaf0; }} }}\n  \
         </style>\n  \
         <rect class=\"s\" x=\"{px}\" y=\"{py}\" width=\"{pw}\" height=\"{ph}\" rx=\"{prx}\" fill=\"none\" stroke-width=\"{psw}\"/>\n  \
         <g opacity=\"0.75\">\n{pads}  </g>\n  \
         <circle class=\"s\" cx=\"{lcx}\" cy=\"{lcy}\" r=\"{lr}\" fill=\"none\" stroke-width=\"{lsw}\" opacity=\"0.85\"/>\n  \
         <path fill=\"#7be0b2\" d=\"{PLAY_D}\"/>\n\
         </svg>\n"
    )
}

#[cfg(test)]
mod tests {
    use super::favicon_svg;
    use std::path::PathBuf;

    fn favicon_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("public/favicon.svg")
    }

    /// Drift gate: the committed favicon must match the generated one, so a
    /// geometry change here cannot silently leave the favicon behind.
    #[test]
    fn favicon_in_sync() {
        let on_disk = std::fs::read_to_string(favicon_path()).expect("read public/favicon.svg");
        assert_eq!(
            on_disk,
            favicon_svg(),
            "public/favicon.svg is stale. Regenerate:\n  \
             cargo test -p lpa-studio-web favicon_regen -- --ignored"
        );
    }

    /// Regenerator (opt-in): rewrites `public/favicon.svg` from the source
    /// geometry. Run after changing the mark.
    #[test]
    #[ignore = "writes public/favicon.svg; run explicitly after geometry changes"]
    fn favicon_regen() {
        std::fs::write(favicon_path(), favicon_svg()).expect("write public/favicon.svg");
    }
}
