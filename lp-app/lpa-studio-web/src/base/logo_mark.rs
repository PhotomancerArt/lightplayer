//! The LightPlayer brand: [`LogoMark`], [`LogoLockup`], and [`LogoStacked`].
//!
//! Source of truth for the brand. The mark is a WS2811-style addressable-LED
//! package — slim corner pads, a grown fillet-cornered play triangle filling
//! the package — drawn "datasheet fine": thin silkscreen strokes, near-square
//! corners. Every part takes `currentColor`, so containers theme the whole
//! mark (in-app chrome sets it strong white; mono/print contexts set ink).
//!
//! Motion rules (spike gates 1–3 2026-08-03; gate-4 post-merge collapsed
//! the in-app/marketing split — the full-rainbow brand runs everywhere):
//! - **Lockup**: the whole wordmark sweeps the LED rainbow; the mark —
//!   triangle included — stays quiet white (the word carries the light).
//!   Always on; `prefers-reduced-motion` freezes everything.
//! - **Icon-only forms** (favicon, avatar, app icon): the play triangle
//!   cycles (`LogoMark { animated }`).
//! - **Mono** (`mono: true`): everything `currentColor`, no motion.
//!
//! Derived assets update with this file: brand PNGs come from
//! `logo_mark_stories.rs` captures, and `public/favicon.svg` is generated
//! from the same geometry — `favicon_in_sync` fails on drift; regenerate via
//! `cargo test -p lpa-studio-web favicon_regen -- --ignored`.
//!
//! Design record: `spikes/logo-triangle-chip/index.html` (PR #444 — the
//! 2026-08-24 simplification: lens ring deleted, triangle grown to fillet
//! corners, pads to full ink) on top of `spikes/lightplayer-logo/index.html`
//! (PR #304); heritage motifs from the 2014 Light at Play archive.

use dioxus::prelude::*;

// ---- geometry (shared by the component and the favicon generator) ----
// viewBox is 32×32. Package = "H · Datasheet fine" from the spike: rx 1.9
// (gate-3: "a hair" rounder than 1.3), stroke 1.25, slim pads, no pin-1 dot
// (gate-2: accurate but visual noise).
const PKG: (f32, f32, f32, f32, f32, f32) = (5.5, 5.5, 21.0, 21.0, 1.9, 1.25);
const PADS: [(f32, f32); 4] = [(3.6, 8.2), (26.5, 8.2), (26.5, 19.6), (3.6, 19.6)];
const PAD_SIZE: (f32, f32, f32) = (1.9, 4.2, 0.6);
/// The grown triangle (spike gate-1/2): circumradius, center, and the
/// fillet-radius ratio. The hero uses a tighter ratio (see brand_hero).
const TRI: (f32, f32, f32) = (15.4, 16.0, 7.6);
const TRI_CORNER_RATIO: f32 = 0.16;

/// SVG path for the brand triangle: equilateral, pointing right, centered
/// (cx, cy) with circumradius r. Corners are true circular fillets of
/// radius rho — tangent points on each edge joined by arcs, G1-continuous
/// with the straight edges. Sweep flag 1: the path winds clockwise in
/// SVG's y-down space.
pub(crate) fn fillet_tri_path(cx: f32, cy: f32, r: f32, rho: f32) -> String {
    let vs: [(f32, f32); 3] = std::array::from_fn(|i| {
        let t = (i as f32 * 120.0).to_radians();
        (cx + r * t.cos(), cy + r * t.sin())
    });
    let mut d = String::new();
    for i in 0..3 {
        let v = vs[i];
        let a = vs[(i + 2) % 3];
        let b = vs[(i + 1) % 3];
        let la = ((a.0 - v.0).powi(2) + (a.1 - v.1).powi(2)).sqrt();
        let lb = ((b.0 - v.0).powi(2) + (b.1 - v.1).powi(2)).sqrt();
        let na = ((a.0 - v.0) / la, (a.1 - v.1) / la);
        let nb = ((b.0 - v.0) / lb, (b.1 - v.1) / lb);
        let phi = (na.0 * nb.0 + na.1 * nb.1).clamp(-1.0, 1.0).acos();
        let t = rho / (phi / 2.0).tan();
        let p1 = (v.0 + na.0 * t, v.1 + na.1 * t);
        let p2 = (v.0 + nb.0 * t, v.1 + nb.1 * t);
        let cmd = if i == 0 { 'M' } else { 'L' };
        d.push_str(&format!(
            "{cmd}{:.2} {:.2}A{rho:.2} {rho:.2} 0 0 1 {:.2} {:.2}",
            p1.0, p1.1, p2.0, p2.1
        ));
    }
    d.push('Z');
    d
}

/// The brand mark at `size`×`size` CSS pixels, entirely `currentColor`.
/// `animated` is the icon-only form: the play triangle cycles the LED
/// rainbow (starts on the brand accent, so frozen frames look canonical).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn LogoMark(size: u32, #[props(default = false)] animated: bool) -> Element {
    let (px, py, pw, ph, prx, psw) = PKG;
    let (padw, padh, padrx) = PAD_SIZE;
    let play_class = if animated { "lp-brand-play-anim" } else { "" };
    let tri_d = fillet_tri_path(TRI.0, TRI.1, TRI.2, TRI.2 * TRI_CORNER_RATIO);
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
            g { fill: "currentColor",
                for (x, y) in PADS.iter() {
                    rect { x: "{x}", y: "{y}", width: "{padw}", height: "{padh}", rx: "{padrx}" }
                }
            }
            path { class: "{play_class}", fill: "currentColor", d: "{tri_d}" }
        }
    }
}

/// The wordmark text, shared by the lockup forms. `mono` renders it
/// `currentColor` (print/one-color contexts); otherwise the whole word
/// sweeps the LED rainbow.
///
/// `pub(crate)` for the landing hero (`app::home::brand_hero`), which
/// stacks the word under a shader triangle rather than under the mark:
/// one wordmark, one set of type metrics, one rainbow — a forked copy
/// would drift the moment either changed.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn BrandWord(word_px: u32, #[props(default = false)] mono: bool) -> Element {
    let style = format!("font-size:{word_px}px");
    let word_class = if mono { "" } else { "lp-brand-word" };
    rsx! {
        span {
            class: "tw:font-extrabold tw:leading-none tw:tracking-[-0.015em] {word_class}",
            style: "{style}",
            "LightPlayer"
        }
    }
}

/// The wide brand lockup: mark + wordmark, one unit. The rainbow lives in
/// the wordmark; the mark — triangle included — stays quiet `currentColor`.
/// `compact` drops the wordmark and switches to the icon-only rule (the
/// triangle cycles instead). `mono` is the one-color form for print/light
/// contexts — set `color` on a parent. `href` renders the lockup as a
/// link (the app chrome points it home); without it the lockup stays an
/// inert span for print/story surfaces.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn LogoLockup(
    #[props(default = 22)] size: u32,
    #[props(default = false)] compact: bool,
    #[props(default = false)] mono: bool,
    #[props(default)] href: Option<String>,
    /// A crowded host (the site chrome while it carries a session control)
    /// gives the word up a rung early — the mark alone is the brand there.
    #[props(default = false)]
    early_word_yield: bool,
) -> Element {
    // Optical norm: mark→word gap ≈ 0.5× wordmark cap height measured from
    // the mark's visual edge; the viewBox carries ~9% right-side air (pad
    // overhang), so 0.12×size lands on the norm.
    let gap = ((size as f32) * 0.12).round().max(2.0) as u32;
    let word_px = ((size as f32) * 0.61).round() as u32;
    let tone = if mono { "" } else { "tw:text-strong" };
    // The word yields at narrow widths; the mark stays. Container query
    // when a container encloses the lockup (the site chrome bar), viewport
    // fallback everywhere else.
    let word_wrap = if early_word_yield {
        "tw:max-[680px]:hidden tw:@max-[680px]:hidden tw:flex"
    } else {
        "tw:max-[560px]:hidden tw:@max-[560px]:hidden tw:flex"
    };
    let body = rsx! {
        LogoMark { size, animated: compact && !mono }
        if !compact {
            span { class: "{word_wrap}",
                BrandWord { word_px, mono }
            }
        }
    };
    match href {
        Some(href) => rsx! {
            a {
                class: "tw:flex tw:flex-none tw:items-center tw:no-underline {tone}",
                style: "gap:{gap}px",
                href: "{href}",
                title: "LightPlayer — home",
                {body}
            }
        },
        None => rsx! {
            span {
                class: "tw:flex tw:flex-none tw:cursor-default tw:items-center {tone}",
                style: "gap:{gap}px",
                title: "LightPlayer",
                {body}
            }
        },
    }
}

/// The stacked/square form: mark above wordmark, centered. For app-icon
/// tiles, social cards, and splash surfaces.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn LogoStacked(
    #[props(default = 64)] size: u32,
    #[props(default = false)] mono: bool,
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
            BrandWord { word_px, mono }
        }
    }
}

/// `public/favicon.svg`, generated from the same geometry as [`LogoMark`].
/// Ink follows the browser's color scheme; the play triangle holds the brand
/// accent (the static frame of the icon-only animation).
pub fn favicon_svg() -> String {
    let (px, py, pw, ph, prx, psw) = PKG;
    let (padw, padh, padrx) = PAD_SIZE;
    let tri_d = fillet_tri_path(TRI.0, TRI.1, TRI.2, TRI.2 * TRI_CORNER_RATIO);
    let pads = PADS
        .iter()
        .map(|(x, y)| {
            format!(
                "    <rect class=\"i\" x=\"{x}\" y=\"{y}\" width=\"{padw}\" height=\"{padh}\" rx=\"{padrx}\"/>\n"
            )
        })
        .collect::<String>();
    // The comment must be legal STRICT XML, because the file doubles as the
    // shell loader's `<img>` brand mark and SVG-as-image goes through the
    // strict XML parser (the favicon `<link>` path is lenient and hid both
    // sins for a while): nothing may precede `<svg>`, and a literal `--`
    // (as in the regen command's `-- --ignored`) may not appear inside an
    // XML comment at all — hence the paraphrased regen instruction.
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 32 32\">\n  \
         <!-- GENERATED from src/base/logo_mark.rs (favicon_svg); do not edit by hand.\n       \
         Regenerate: cargo test -p lpa-studio-web favicon_regen (with the ignored flag) -->\n  \
         <style>\n    \
         .s {{ stroke: #14181d; }} .i {{ fill: #14181d; }}\n    \
         @media (prefers-color-scheme: dark) {{ .s {{ stroke: #fffaf0; }} .i {{ fill: #fffaf0; }} }}\n  \
         </style>\n  \
         <rect class=\"s\" x=\"{px}\" y=\"{py}\" width=\"{pw}\" height=\"{ph}\" rx=\"{prx}\" fill=\"none\" stroke-width=\"{psw}\"/>\n  \
         <g>\n{pads}  </g>\n  \
         <path fill=\"#7be0b2\" d=\"{tri_d}\"/>\n\
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

    /// Pins the exact fillet-triangle `d` string, so float-formatting drift
    /// or an accidental construction change fails loudly instead of quietly
    /// nudging the mark.
    #[test]
    fn brand_triangle_path_is_stable() {
        assert_eq!(
            super::fillet_tri_path(15.4, 16.0, 7.6, 7.6 * 0.16),
            "M21.18 14.95A1.22 1.22 0 0 1 21.18 17.05L13.42 21.53A1.22 1.22 0 0 1 \
             11.60 20.48L11.60 11.52A1.22 1.22 0 0 1 13.42 10.47Z"
        );
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
