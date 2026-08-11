//! Shared lamp-view geometry: wiring-arrow segments and the neutral lamp
//! fill.
//!
//! Inputs are neutral (positions + spans in caller-chosen view units) so the
//! Studio face renderer (over `ControlLayout2d`) and the editor canvas (over
//! resolved documents) share one implementation — the parent plan's D7/D10:
//! view options are renderer input, renderers interchange behind the same
//! data.

/// Neutral lamp fill when live coloring does not apply.
const NEUTRAL_LAMP_RGB: [u8; 3] = [96, 102, 112];

#[must_use]
pub const fn neutral_lamp_rgb() -> [u8; 3] {
    NEUTRAL_LAMP_RGB
}

/// One wiring arrow segment in the caller's view units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MapArrowSeg {
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
    /// `true` for the dashed hop between consecutive paths.
    pub chain: bool,
}

/// Overlay geometry: viewBox size plus arrow segments.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MapArrowOverlay {
    pub view_width: f32,
    pub view_height: f32,
    pub segs: Vec<MapArrowSeg>,
}

/// Neutral wiring-arrow input.
#[derive(Clone, Copy, Debug)]
pub struct ArrowInput<'a> {
    /// Lamp centers in view units, in wiring order.
    pub positions: &'a [[f32; 2]],
    /// Contiguous `(first_lamp, lamp_count)` spans in wiring order; empty
    /// means "treat all lamps as one run".
    pub spans: &'a [(u32, u32)],
    pub view_width: f32,
    pub view_height: f32,
    /// Pull arrow endpoints off the lamp centers so heads stay visible.
    pub end_gap: f32,
    /// Segments shorter than this draw no arrow (lamps visually touch).
    pub min_len: f32,
}

/// Build wiring arrows: lamps connect in wiring order within each span, and
/// consecutive spans are linked with a dashed chain hop.
#[must_use]
pub fn wiring_arrows(input: &ArrowInput<'_>) -> MapArrowOverlay {
    let spans: Vec<(u32, u32)> = if input.spans.is_empty() {
        vec![(0, input.positions.len() as u32)]
    } else {
        input.spans.to_vec()
    };

    let mut segs = Vec::new();
    let mut push_seg = |a: [f32; 2], b: [f32; 2], chain: bool| {
        let dx = b[0] - a[0];
        let dy = b[1] - a[1];
        let len = (dx * dx + dy * dy).sqrt();
        if len < input.min_len {
            return;
        }
        let ux = dx / len;
        let uy = dy / len;
        segs.push(MapArrowSeg {
            x1: a[0] + ux * input.end_gap,
            y1: a[1] + uy * input.end_gap,
            x2: b[0] - ux * input.end_gap,
            y2: b[1] - uy * input.end_gap,
            chain,
        });
    };

    let point = |index: usize| input.positions.get(index).copied();
    for (first, count) in &spans {
        let start = *first as usize;
        let end = start + *count as usize;
        for index in start..end.saturating_sub(1) {
            if let (Some(a), Some(b)) = (point(index), point(index + 1)) {
                push_seg(a, b, false);
            }
        }
    }
    for pair in spans.windows(2) {
        let tail = pair[0].0 as usize + pair[0].1 as usize;
        let head = pair[1].0 as usize;
        if let (Some(a), Some(b)) = (point(tail.saturating_sub(1)), point(head)) {
            push_seg(a, b, true);
        }
    }

    MapArrowOverlay {
        view_width: input.view_width,
        view_height: input.view_height,
        segs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arrows_connect_within_spans_and_chain_between_them() {
        let overlay = wiring_arrows(&input(
            &[
                [100.0, 500.0],
                [300.0, 500.0],
                [500.0, 500.0],
                [700.0, 500.0],
            ],
            &[(0, 2), (2, 2)],
        ));
        let chains = overlay.segs.iter().filter(|seg| seg.chain).count();
        let wires = overlay.segs.iter().filter(|seg| !seg.chain).count();
        assert_eq!(wires, 2); // 0→1 and 2→3
        assert_eq!(chains, 1); // 1→2
    }

    #[test]
    fn missing_spans_fall_back_to_one_run() {
        let overlay = wiring_arrows(&input(
            &[[100.0, 500.0], [500.0, 500.0], [900.0, 500.0]],
            &[],
        ));
        assert_eq!(overlay.segs.len(), 2);
        assert!(overlay.segs.iter().all(|seg| !seg.chain));
    }

    #[test]
    fn touching_lamps_draw_no_arrow() {
        let overlay = wiring_arrows(&input(&[[500.0, 500.0], [510.0, 500.0]], &[]));
        assert!(overlay.segs.is_empty());
    }

    fn input<'a>(positions: &'a [[f32; 2]], spans: &'a [(u32, u32)]) -> ArrowInput<'a> {
        ArrowInput {
            positions,
            spans,
            view_width: 1000.0,
            view_height: 1000.0,
            end_gap: 16.0,
            min_len: 34.0,
        }
    }
}
