//! Every spot one label could take on its arrow, best first.
//!
//! **Along the line.** The label's box is `w` wide, so it wants a stretch
//! of the line `w` long that is nearly straight: the line strays from the
//! stretch's chord by no more than [`LayoutOptions::straightness`] of half
//! the box's height, so it stays behind the box from end to end. The box
//! sits centred on the line at the middle of that stretch, turned to the
//! chord's angle, then flipped if need be so the text reads upright. The
//! stretch keeps [`LayoutOptions::clearance`] (plus a head's length) from
//! either end. Spots are tried from the preferred one outward, a quarter of
//! the box's height at a time, nearer before farther.
//!
//! **Beside it.** After every spot along, a level box just off the
//! arrow's midpoint, on the upper side (the right side, for a steep
//! arrow), then the other side: for arrows too bent or too short to carry
//! their label.
//!
//! The overlap pass ([`layout_arrows`](crate::layout_arrows)) takes the
//! first candidate nothing else is in the way of. A label counts as moved
//! once it sits more than half its box's height from where it preferred.

use crate::{
    Arrow, DropReason, HeadGeometry, LabelFit, LayoutOptions, OrientedBox, Placement, Polyline,
    Vec2,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Candidate {
    pub frame: OrientedBox,
    /// 0–1 along the arrow.
    pub along: f64,
    /// How far, along the line, from the spot it preferred.
    pub shift: f64,
    pub fit: LabelFit,
}

/// A label counts as moved once it is more than this many box heights
/// from the spot it preferred: nudges smaller than that are just fitting.
pub(crate) const MOVED_HEIGHTS: f64 = 0.5;

/// The spots for a label whose box is `size` (width along the text,
/// height across) on `arrow`, whose centreline is `line` and whose heads
/// are `heads`. Never empty: no spot at all is the reason it drops.
pub(crate) fn label_candidates(
    arrow: &Arrow,
    line: &Polyline,
    heads: &[HeadGeometry],
    size: Vec2,
    placement: Placement,
    options: &LayoutOptions,
) -> Result<Vec<Candidate>, DropReason> {
    let (w, h) = (size.x, size.y);
    let total = line.length();
    let head_len = |head: Option<crate::Head>| head.map(|h| h.length).unwrap_or(0.0);
    let lo = options.clearance + head_len(arrow.start_head) + w / 2.0;
    let hi = total - options.clearance - head_len(arrow.end_head) - w / 2.0;
    let clear = |frame: &OrientedBox| clear_of_ends(frame, arrow, heads, options.clearance);
    let mut out = Vec::new();
    let mut long_enough = false;

    if hi >= lo && total > 0.0 {
        long_enough = true;
        let pref = (placement.fraction() * total).clamp(lo, hi);
        let step = (h / 4.0).max(total / 512.0);
        let tolerance = options.straightness * h / 2.0;
        let mut k = 0usize;
        loop {
            let ahead = pref + k as f64 * step;
            let behind = pref - k as f64 * step;
            if ahead > hi && behind < lo {
                break;
            }
            let spots: &[f64] = if k == 0 { &[ahead] } else { &[ahead, behind] };
            for &s in spots {
                if s < lo || s > hi {
                    continue;
                }
                let (a, b, stray) = line.chord(s - w / 2.0, s + w / 2.0);
                if stray > tolerance {
                    continue;
                }
                let frame = OrientedBox::new(line.point_at(s), w, h, upright((b - a).angle_deg()));
                if clear(&frame) {
                    out.push(Candidate {
                        frame,
                        along: s / total,
                        shift: (s - pref).abs(),
                        fit: LabelFit::Along {
                            moved: (s - pref).abs() > MOVED_HEIGHTS * h,
                        },
                    });
                }
            }
            k += 1;
        }
    }

    if options.beside && total >= h {
        let mid = line.point_at(total / 2.0);
        let t = line.tangent_at(total / 2.0);
        let n = t.perp();
        let first = if t.x.abs() >= t.y.abs() {
            if n.y <= 0.0 { n } else { -n }
        } else if n.x >= 0.0 {
            n
        } else {
            -n
        };
        for side in [first, -first] {
            let reach = (w / 2.0) * side.x.abs() + (h / 2.0) * side.y.abs() + options.gap;
            for k in 0..4 {
                let centre = mid + side * (reach + k as f64 * h / 2.0);
                let frame = OrientedBox::new(centre, w, h, 0.0);
                if clear(&frame) && !crosses(&frame.inflated(options.gap / 2.0), line) {
                    out.push(Candidate {
                        frame,
                        along: 0.5,
                        shift: 0.0,
                        fit: LabelFit::Beside,
                    });
                    break;
                }
            }
        }
    }

    if out.is_empty() {
        Err(if long_enough {
            DropReason::TooCurved
        } else {
            DropReason::TooShort
        })
    } else {
        Ok(out)
    }
}

/// An angle turned so text at it reads upright: in (−90°, 90°].
pub(crate) fn upright(deg: f64) -> f64 {
    let mut d = deg;
    while d > 90.0 {
        d -= 180.0;
    }
    while d <= -90.0 {
        d += 180.0;
    }
    d
}

/// The box keeps `clearance` from the arrow's two ends and its heads.
fn clear_of_ends(
    frame: &OrientedBox,
    arrow: &Arrow,
    heads: &[HeadGeometry],
    clearance: f64,
) -> bool {
    let grown = frame.inflated(clearance);
    !grown.contains(arrow.from)
        && !grown.contains(arrow.to)
        && !heads.iter().any(|h| grown.overlaps_polygon(&h.points))
}

/// Some piece of the line runs through the box.
pub(crate) fn crosses(frame: &OrientedBox, line: &Polyline) -> bool {
    line.points()
        .windows(2)
        .any(|w| frame.overlaps_polygon(&[w[0], w[1]]))
}
