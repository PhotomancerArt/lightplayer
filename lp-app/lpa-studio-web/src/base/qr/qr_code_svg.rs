//! A QR symbol as inline SVG: dark modules on a light ground, with the
//! standard's four-module quiet zone.
//!
//! Always dark-on-light whatever the theme — a camera reads contrast, and
//! an inverted symbol is one many phone scanners refuse. Each row's dark
//! runs are one `rect` apiece, and `shape-rendering: crispEdges` keeps the
//! module edges from blurring at fractional scales.

use dioxus::prelude::*;

use super::QrCode;

/// Light modules and the quiet zone.
const LIGHT: &str = "#fffaf0";
/// Dark modules.
const DARK: &str = "#0c1114";
/// The quiet zone the standard asks for, in modules.
const QUIET: usize = 4;

/// Draw `text`'s QR at `size_px` square. Renders nothing when the text is
/// too long for version 10 (the callers' links never are).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn QrCodeSvg(text: String, size_px: u32, #[props(default)] label: Option<String>) -> Element {
    let Some(code) = QrCode::encode(text.as_bytes()) else {
        return rsx! {};
    };
    let span = code.side() + 2 * QUIET;
    let runs = dark_runs(&code);
    let label = label.unwrap_or_else(|| "QR code".to_string());
    rsx! {
        svg {
            width: "{size_px}",
            height: "{size_px}",
            view_box: "0 0 {span} {span}",
            role: "img",
            "aria-label": "{label}",
            shape_rendering: "crispEdges",
            class: "tw:block tw:flex-none tw:rounded-sm",
            rect { width: "{span}", height: "{span}", fill: LIGHT }
            for (row , col , len) in runs {
                rect {
                    x: "{col + QUIET}",
                    y: "{row + QUIET}",
                    width: "{len}",
                    height: "1",
                    fill: DARK,
                }
            }
        }
    }
}

/// Every horizontal run of dark modules, as (row, first column, length).
fn dark_runs(code: &QrCode) -> Vec<(usize, usize, usize)> {
    let mut runs = Vec::new();
    for (row, modules) in code.modules.iter().enumerate() {
        let mut col = 0;
        while col < modules.len() {
            if !modules[col] {
                col += 1;
                continue;
            }
            let start = col;
            while col < modules.len() && modules[col] {
                col += 1;
            }
            runs.push((row, start, col - start));
        }
    }
    runs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_runs_cover_exactly_the_dark_modules() {
        let code = QrCode::encode(b"https://lightplayer.app/unlock").unwrap();
        let mut drawn = vec![vec![false; code.side()]; code.side()];
        for (row, col, len) in dark_runs(&code) {
            for cell in &mut drawn[row][col..col + len] {
                assert!(!*cell, "runs never overlap");
                *cell = true;
            }
        }
        assert_eq!(drawn, code.modules);
    }
}
