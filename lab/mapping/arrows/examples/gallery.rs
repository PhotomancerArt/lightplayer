//! An SVG gallery of arrow cases, for eyeballing layout:
//!
//! ```bash
//! cargo run -p lab-arrows --example gallery > /tmp/arrows.svg
//! qlmanage -t -s 1600 -o /tmp /tmp/arrows.svg   # a PNG beside it
//! ```
//!
//! Each panel is one case laid out together (so its labels avoid each
//! other). Under each, every label that was moved, set beside, or dropped
//! says so. Text is set in a monospace font so the default
//! `AverageCharWidth` measure is exact.

use std::fmt::Write;

use lab_arrows::{
    Arrow, ArrowGeometry, AverageCharWidth, Bend, EndDirection, Gutter, Head, Label, LabelFit,
    LabelRole, LayoutOptions, Route, Side, Vec2, layout_arrows,
};

const W: f64 = 380.0;
const H: f64 = 290.0;
const COLS: usize = 3;
const BG: &str = "#0d0d13";
const PANEL: &str = "#14141c";
const BOUND: &str = "#a58af7";
const TEXT: &str = "#f2f0e8";
const SUBTLE: &str = "#9c9cab";

fn main() {
    let cases = cases();
    let rows = cases.len().div_ceil(COLS);
    // Square, because Quick Look's thumbnailer crops to a square.
    let side = (COLS as f64 * W).max(rows as f64 * H);
    let (width, height) = (side, side);
    let mut svg = String::new();
    let _ = writeln!(
        svg,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}" font-family="Menlo, monospace">"#
    );
    let _ = writeln!(svg, r#"<rect width="100%" height="100%" fill="{BG}"/>"#);
    let options = LayoutOptions::default();
    for (i, (title, arrows)) in cases.iter().enumerate() {
        let (x, y) = ((i % COLS) as f64 * W, (i / COLS) as f64 * H);
        let geometry = layout_arrows(arrows, &AverageCharWidth::default(), &options);
        panel(&mut svg, x, y, title, arrows, &geometry);
    }
    svg.push_str("</svg>\n");
    print!("{svg}");
}

fn panel(
    svg: &mut String,
    x: f64,
    y: f64,
    title: &str,
    arrows: &[Arrow],
    geometry: &[ArrowGeometry],
) {
    let _ = writeln!(svg, r#"<g transform="translate({x} {y})">"#);
    let _ = writeln!(
        svg,
        r#"<rect x="6" y="6" width="{}" height="{}" rx="6" fill="{PANEL}"/>"#,
        W - 12.0,
        H - 12.0
    );
    let _ = writeln!(
        svg,
        r#"<text x="16" y="24" font-size="11" fill="{SUBTLE}">{title}</text>"#
    );
    // Ends, so you can see where each arrow was asked to go.
    for a in arrows {
        for p in [a.from, a.to] {
            let _ = writeln!(
                svg,
                r#"<circle cx="{}" cy="{}" r="2" fill="{SUBTLE}" opacity="0.6"/>"#,
                p.x, p.y
            );
        }
    }
    // Every line and head first, then every label, so a label's box hides
    // any line that passes behind it, its own or another's.
    for g in geometry {
        let _ = writeln!(
            svg,
            r#"<path d="{}" fill="none" stroke="{BOUND}" stroke-width="1.4" opacity="0.8"/>"#,
            g.path.to_svg()
        );
        for h in &g.heads {
            if h.closed {
                let _ = writeln!(svg, r#"<path d="{}" fill="{BOUND}"/>"#, h.to_svg());
            } else {
                let _ = writeln!(
                    svg,
                    r#"<path d="{}" fill="none" stroke="{BOUND}" stroke-width="1.4" stroke-linejoin="round" stroke-linecap="round"/>"#,
                    h.to_svg()
                );
            }
        }
    }
    let mut notes = Vec::new();
    for (ai, g) in geometry.iter().enumerate() {
        for l in &g.labels {
            match l.fit {
                LabelFit::Along { moved: true } => {
                    notes.push(format!("#{ai} \"{}\": moved", l.text))
                }
                LabelFit::Beside => notes.push(format!("#{ai} \"{}\": beside", l.text)),
                LabelFit::Dropped(r) => notes.push(format!("#{ai} \"{}\": dropped, {r:?}", l.text)),
                LabelFit::Along { moved: false } => {}
            }
            let Some(p) = l.placed else { continue };
            let f = p.frame;
            let dash = if l.fit == LabelFit::Beside {
                r#" stroke-dasharray="2 2""#
            } else {
                ""
            };
            let (fill, opacity) = match l.role {
                LabelRole::Primary => (TEXT, 1.0),
                LabelRole::Quiet => (BOUND, 0.85),
            };
            let _ = writeln!(
                svg,
                r#"<g transform="{}"><rect x="{}" y="{}" width="{}" height="{}" rx="2.5" fill="{PANEL}" stroke="{BOUND}" stroke-opacity="0.45" stroke-width="0.8"{dash}/><text x="{}" y="{}" font-size="{}" fill="{fill}" opacity="{opacity}" text-anchor="middle" dominant-baseline="central">{}</text></g>"#,
                p.svg_transform(),
                f.centre.x - f.width / 2.0,
                f.centre.y - f.height / 2.0,
                f.width,
                f.height,
                p.text_at.x,
                p.text_at.y,
                l.font_size,
                escape(&l.text)
            );
        }
    }
    for (i, n) in notes.iter().enumerate() {
        let _ = writeln!(
            svg,
            r#"<text x="16" y="{}" font-size="8.5" fill="{SUBTLE}">{}</text>"#,
            H - 18.0 - (notes.len() - 1 - i) as f64 * 11.0,
            escape(n)
        );
    }
    svg.push_str("</g>\n");
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn v(x: f64, y: f64) -> Vec2 {
    Vec2::new(x, y)
}

/// The lab's edge: leave heading toward the target, arrive from the left.
fn edge(from: Vec2, to: Vec2) -> Arrow {
    Arrow::new(from, to)
        .leave(EndDirection::Direct)
        .arrive(EndDirection::Side(Side::Left))
}

fn cases() -> Vec<(&'static str, Vec<Arrow>)> {
    let lane = |x: f64| {
        Route::Gutter(Gutter {
            lane_x: x,
            radius: 8.0,
        })
    };
    vec![
        (
            "gutter: the stack's edges, out and in on lanes",
            vec![
                Arrow::new(v(320.0, 40.0), v(320.0, 150.0))
                    .route(lane(60.0))
                    .label(Label::quiet("visual.out")),
                Arrow::new(v(320.0, 150.0), v(320.0, 220.0))
                    .route(lane(90.0))
                    .label(Label::quiet("control.out")),
            ],
        ),
        (
            "straight",
            vec![
                Arrow::new(v(40.0, 120.0), v(340.0, 120.0))
                    .straight()
                    .label(Label::primary("lamps 0–49 at pixel 0")),
            ],
        ),
        (
            "gentle S, primary + quiet near the end",
            vec![
                Arrow::new(v(40.0, 70.0), v(340.0, 180.0))
                    .leave(EndDirection::Side(Side::Right))
                    .arrive(EndDirection::Side(Side::Left))
                    .label(Label::primary("tail → GPIO 10"))
                    .label(Label::quiet("control.out").near_end()),
            ],
        ),
        (
            "tight curve",
            vec![
                Arrow::new(v(80.0, 200.0), v(200.0, 70.0))
                    .route(Route::Curve(Bend {
                        reach: 0.4,
                        min: 110.0,
                    }))
                    .leave(EndDirection::Side(Side::Top))
                    .arrive(EndDirection::Side(Side::Left))
                    .label(Label::primary("ring display")),
                Arrow::new(v(250.0, 200.0), v(300.0, 80.0))
                    .route(Route::Curve(Bend {
                        reach: 0.4,
                        min: 140.0,
                    }))
                    .leave(EndDirection::Side(Side::Right))
                    .arrive(EndDirection::Side(Side::Left))
                    .label(Label::primary("hairpin")),
            ],
        ),
        (
            "steep and vertical",
            vec![
                Arrow::new(v(120.0, 40.0), v(130.0, 210.0))
                    .straight()
                    .label(Label::primary("visual.out")),
                Arrow::new(v(250.0, 210.0), v(260.0, 40.0))
                    .straight()
                    .label(Label::primary("going up")),
                Arrow::new(v(300.0, 40.0), v(340.0, 210.0))
                    .arrive(EndDirection::Side(Side::Top))
                    .label(Label::quiet("steep")),
            ],
        ),
        (
            "right to left (never upside down)",
            vec![
                Arrow::new(v(340.0, 70.0), v(40.0, 110.0))
                    .arrive(EndDirection::Side(Side::Right))
                    .label(Label::primary("back to the shader")),
                Arrow::new(v(340.0, 200.0), v(40.0, 150.0))
                    .straight()
                    .label(Label::primary("leftward, rising")),
            ],
        ),
        (
            "very short arrows",
            vec![
                Arrow::new(v(120.0, 120.0), v(150.0, 120.0))
                    .straight()
                    .label(Label::primary("a long label")),
                Arrow::new(v(260.0, 150.0), v(268.0, 150.0))
                    .straight()
                    .label(Label::primary("tiny")),
            ],
        ),
        (
            "converging on neighbouring pixels",
            (0..6)
                .map(|i| {
                    let from = v(40.0 + (i % 2) as f64 * 30.0, 40.0 + i as f64 * 32.0);
                    let to = v(330.0, 90.0 + i as f64 * 11.0);
                    let mut a = edge(from, to).label(Label::primary(format!(
                        "lamps {}–{}",
                        i * 10,
                        i * 10 + 9
                    )));
                    if i == 0 {
                        a = a.label(Label::quiet("control.out").near_end());
                    }
                    a
                })
                .collect(),
        ),
        (
            "long label",
            vec![
                Arrow::new(v(40.0, 90.0), v(340.0, 110.0))
                    .straight()
                    .label(Label::primary("a long label that almost fills it")),
                edge(v(60.0, 200.0), v(300.0, 150.0)).label(Label::primary(
                    "a far longer label than this curve can carry",
                )),
            ],
        ),
        (
            "primary and quiet together",
            vec![
                edge(v(40.0, 80.0), v(340.0, 160.0))
                    .label(Label::primary("strip → D1 · lamps 0–29"))
                    .label(Label::quiet("control.out").near_start())
                    .label(Label::quiet("30 px").near_end()),
            ],
        ),
        (
            "heads: chevron, both ends, none",
            vec![
                Arrow::new(v(40.0, 60.0), v(340.0, 60.0))
                    .straight()
                    .end_head(Some(Head::chevron(8.0, 9.0)))
                    .label(Label::quiet("chevron")),
                Arrow::new(v(40.0, 120.0), v(340.0, 140.0))
                    .start_head(Some(Head::default()))
                    .label(Label::primary("both ways")),
                Arrow::new(v(40.0, 200.0), v(340.0, 190.0))
                    .end_head(None)
                    .label(Label::quiet("no head").near_end()),
            ],
        ),
        (
            "the patch: one edge per object",
            (0..8)
                .map(|i| {
                    let from = v(50.0 + ((i * 37) % 60) as f64, 40.0 + i as f64 * 23.0);
                    let to = v(330.0, 70.0 + i as f64 * 14.0);
                    edge(from, to).label(Label::primary(format!(
                        "lamps {}–{} at px {}",
                        i * 8,
                        i * 8 + 7,
                        i * 8
                    )))
                })
                .collect(),
        ),
        (
            "a binding: shader card → fixture",
            vec![
                Arrow::new(v(60.0, 180.0), v(320.0, 60.0))
                    .leave(EndDirection::Side(Side::Right))
                    .arrive(EndDirection::Side(Side::Left))
                    .label(Label::quiet("visual.out")),
                Arrow::new(v(60.0, 200.0), v(320.0, 200.0))
                    .leave(EndDirection::Side(Side::Right))
                    .arrive(EndDirection::Side(Side::Left))
                    .label(Label::quiet("visual.out")),
            ],
        ),
    ]
}
