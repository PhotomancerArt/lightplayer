//! Laying out several arrows at once, so their labels keep out of each
//! other's way.
//!
//! Each arrow's line and heads come first: the centreline cubic, trimmed
//! at each head so the line stops at a triangle's base. Then labels are
//! placed greedily, one at a time — every primary label before any quiet
//! one, and within a role in the order given — each taking its first
//! candidate (`label_candidates`) whose box, grown by half the gap, touches
//! no box already placed and no other arrow's head. Among those it prefers
//! one no other arrow's line runs behind, since a box hiding a stranger's
//! line reads as if the label were that line's; if every free spot covers
//! one, it takes the first spot along its own line anyway (threaded on its
//! line, it still reads as its own), but never a box beside it, which
//! would read as the stranger's. A label with no free candidate drops as
//! `Crowded`. Nothing depends on anything but the
//! input, so the same input always gives the same geometry.

use crate::arrow::gutter_path;
use crate::label_candidates::{MOVED_HEIGHTS, crosses, label_candidates};
use crate::{
    Arrow, ArrowEnd, ArrowGeometry, Cubic, DropReason, Gutter, HeadGeometry, LabelFit,
    LabelGeometry, LayoutOptions, OrientedBox, Path, PlacedLabel, Polyline, Rect, Route,
    TextAnchor, TextBaseline, TextMeasure, Vec2,
};

/// Lay out every arrow; one geometry per arrow, in the same order.
pub fn layout_arrows<M: TextMeasure + ?Sized>(
    arrows: &[Arrow],
    measure: &M,
    options: &LayoutOptions,
) -> Vec<ArrowGeometry> {
    let drawn: Vec<(Path, Vec<HeadGeometry>, Polyline, Option<Rect>)> = arrows
        .iter()
        .map(|a| {
            let (path, heads, line) = line_and_heads(a);
            let bounds = Rect::around(line.points().iter().copied());
            (path, heads, line, bounds)
        })
        .collect();

    let mut order: Vec<(crate::LabelRole, usize, usize)> = arrows
        .iter()
        .enumerate()
        .flat_map(|(ai, a)| {
            a.labels
                .iter()
                .enumerate()
                .map(move |(li, l)| (l.role, ai, li))
        })
        .collect();
    order.sort();

    let mut labels: Vec<Vec<Option<LabelGeometry>>> =
        arrows.iter().map(|a| vec![None; a.labels.len()]).collect();
    let mut taken: Vec<OrientedBox> = Vec::new();
    let half_gap = options.gap / 2.0;

    for (role, ai, li) in order {
        let arrow = &arrows[ai];
        let label = &arrow.labels[li];
        let (_, heads, line, _) = &drawn[ai];
        let font_size = options.font_size(role);
        let size = Vec2::new(
            measure.width(&label.text, font_size) + 2.0 * options.padding.x,
            font_size + 2.0 * options.padding.y,
        );
        let free = |frame: &OrientedBox| {
            let grown = frame.inflated(half_gap);
            !taken.iter().any(|t| t.inflated(half_gap).overlaps(&grown))
                && !drawn
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| *i != ai)
                    .flat_map(|(_, d)| &d.1)
                    .any(|h| grown.overlaps_polygon(&h.points))
        };
        let behind_others = |frame: &OrientedBox| {
            let bounds = frame.bounds();
            drawn.iter().enumerate().any(|(i, d)| {
                i != ai && d.3.is_some_and(|b| overlap(&b, &bounds)) && crosses(frame, &d.2)
            })
        };
        let (fit, placed) =
            match label_candidates(arrow, line, heads, size, label.placement, options) {
                Err(reason) => (LabelFit::Dropped(reason), None),
                Ok(candidates) => match candidates
                    .iter()
                    .find(|c| free(&c.frame) && !behind_others(&c.frame))
                    .or_else(|| {
                        candidates
                            .iter()
                            .find(|c| c.fit != LabelFit::Beside && free(&c.frame))
                    }) {
                    None => (LabelFit::Dropped(DropReason::Crowded), None),
                    Some(c) => {
                        taken.push(c.frame);
                        let fit = match c.fit {
                            LabelFit::Along { .. } => LabelFit::Along {
                                moved: c.shift > MOVED_HEIGHTS * size.y,
                            },
                            other => other,
                        };
                        let placed = PlacedLabel {
                            frame: c.frame,
                            text_at: c.frame.centre,
                            angle_deg: c.frame.angle_deg,
                            anchor: TextAnchor::Middle,
                            baseline: TextBaseline::Central,
                            along: c.along,
                        };
                        (fit, Some(placed))
                    }
                },
            };
        labels[ai][li] = Some(LabelGeometry {
            text: label.text.clone(),
            role,
            font_size,
            fit,
            placed,
        });
    }

    drawn
        .into_iter()
        .zip(labels)
        .map(|((path, heads, _, _), labels)| ArrowGeometry {
            path,
            heads,
            labels: labels
                .into_iter()
                .map(|l| l.expect("every label laid out"))
                .collect(),
        })
        .collect()
}

/// The arrow's line, trimmed at its heads, the heads themselves, and its
/// whole centreline measured for placing labels.
fn line_and_heads(arrow: &Arrow) -> (Path, Vec<HeadGeometry>, Polyline) {
    if let Route::Gutter(g) = arrow.route {
        return gutter_line_and_heads(arrow, g);
    }
    let curve = arrow.centreline();
    let straight = arrow.route == Route::Straight;
    let centreline = Path::of_cubic(&curve, straight).polyline();
    let mut heads = Vec::new();
    let (mut t0, mut t1) = (0.0, 1.0);
    if let Some(h) = arrow.start_head {
        let base = curve.param_at_distance(h.length, true);
        // The start head points back, out of the line, onto `from`.
        let toward = (curve.start - curve.point(base)).normalized();
        heads.push(HeadGeometry::new(
            ArrowEnd::Start,
            h.shape,
            curve.start,
            toward,
            h.length,
            h.width,
        ));
        t0 = curve.param_at_distance(h.inset(), true);
    }
    if let Some(h) = arrow.end_head {
        let base = curve.param_at_distance(h.length, false);
        let toward = (curve.end - curve.point(base)).normalized();
        heads.push(HeadGeometry::new(
            ArrowEnd::End,
            h.shape,
            curve.end,
            toward,
            h.length,
            h.width,
        ));
        t1 = curve.param_at_distance(h.inset(), false);
    }
    let path = if t1 > t0 {
        Path::of_cubic(&trimmed(&curve, t0, t1), straight)
    } else {
        Path::default()
    };
    (path, heads, centreline)
}

/// A gutter route's line, heads and centreline. Its ends are sideways
/// stretches, so a head sits on one, pointing along it, and the line stops
/// at the head's base.
fn gutter_line_and_heads(arrow: &Arrow, g: Gutter) -> (Path, Vec<HeadGeometry>, Polyline) {
    let centreline = gutter_path(arrow.from, arrow.to, g).polyline();
    let out = Vec2::new((g.lane_x - arrow.from.x).signum(), 0.0);
    let inward = Vec2::new((arrow.to.x - g.lane_x).signum(), 0.0);
    let (mut from, mut to) = (arrow.from, arrow.to);
    let mut heads = Vec::new();
    if let Some(h) = arrow.start_head {
        heads.push(HeadGeometry::new(
            ArrowEnd::Start,
            h.shape,
            arrow.from,
            -out,
            h.length,
            h.width,
        ));
        from = from + out * h.inset();
    }
    if let Some(h) = arrow.end_head {
        heads.push(HeadGeometry::new(
            ArrowEnd::End,
            h.shape,
            arrow.to,
            inward,
            h.length,
            h.width,
        ));
        to = to - inward * h.inset();
    }
    (gutter_path(from, to, g), heads, centreline)
}

fn trimmed(curve: &Cubic, t0: f64, t1: f64) -> Cubic {
    if t0 <= 0.0 && t1 >= 1.0 {
        *curve
    } else {
        curve.between(t0, t1)
    }
}

fn overlap(a: &Rect, b: &Rect) -> bool {
    a.min.x <= b.max.x && b.min.x <= a.max.x && a.min.y <= b.max.y && b.min.y <= a.max.y
}
