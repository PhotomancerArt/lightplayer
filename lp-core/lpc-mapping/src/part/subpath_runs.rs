//! Where a split path's lamps fall into its subpaths.
//!
//! A path split by subpaths renders the unsplit path's lamps, cut at the
//! jumpers. Where it is cut is today's answer, [`path_gap_breaks`] (a lamp
//! exactly on a jumper stays with the run before it). Which subpath each run
//! is on is read from the drawn lengths, walked the way `path_gap_breaks`
//! walks them (data order, each line's length summed in turn, a jumper
//! ending one subpath at the length walked so far): the run's first lamp
//! sits on the first subpath, with any length, whose end it hasn't passed.
//! A subpath with no lamps makes no run.
//!
//! The walk is repeated here rather than shared, on purpose: the resolver's
//! private helpers are inlined into today's resolver, and a new caller of
//! one changes the firmware image. The walk only names runs; every lamp
//! position and every cut is today's.

use alloc::vec::Vec;

use crate::map2d_doc::PathShape;
use crate::map2d_resolve::path_gap_breaks;
use crate::part::path::Path;

/// One run of a split path: the subpath it lies on, and its lamps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SubpathRun {
    /// The subpath's place in the drawing, from 1 (its key is `s<drawn>`).
    pub drawn: u32,
    /// Its first lamp, as an offset into the path's lamps (data order).
    pub start: u32,
    pub count: u32,
}

/// The runs of a path's lamps, one per subpath that holds any, in data
/// order (last drawn first when the path is reversed). `today` is the
/// path's own [`Path::to_path_shape`].
pub(crate) fn subpath_runs(path: &Path, today: &PathShape) -> Vec<SubpathRun> {
    if today.count == 0 || today.points.len() < 2 {
        return Vec::new();
    }
    // Line `i` runs from point `i` to point `i + 1`; it is a jumper when the
    // command that ends it is a move. In data order: reversed, last first.
    let mut lines: Vec<([f32; 2], [f32; 2], bool)> = path
        .commands
        .windows(2)
        .map(|pair| (pair[0].point(), pair[1].point(), pair[1].is_move()))
        .collect();
    if today.reversed {
        lines.reverse();
        for line in &mut lines {
            *line = (line.1, line.0, line.2);
        }
    }

    // Each subpath's stretch of drawn length, in data order.
    let mut stretches: Vec<(f32, f32)> = Vec::new();
    let mut from = 0.0;
    let mut walked = 0.0;
    for (a, b, jumper) in &lines {
        if *jumper {
            stretches.push((from, walked));
            from = walked;
        } else {
            walked += length(*a, *b);
        }
    }
    stretches.push((from, walked));
    let total = walked;
    let subpaths = stretches.len() as u32;

    let mut starts = Vec::with_capacity(stretches.len());
    starts.push(0);
    starts.extend(path_gap_breaks(today));
    let mut runs = Vec::with_capacity(starts.len());
    for (i, &start) in starts.iter().enumerate() {
        let end = starts.get(i + 1).copied().unwrap_or(today.count);
        // Where `path_gap_breaks` puts lamp `start`, in the same arithmetic.
        let at = if today.count < 2 {
            0.0
        } else {
            total * (start as f32 / (today.count - 1) as f32)
        };
        let drawn_lit = stretches
            .iter()
            .enumerate()
            .filter(|(_, (a, b))| b - a > f32::EPSILON);
        let data_index = drawn_lit
            .clone()
            .find(|(_, (_, b))| at <= *b)
            .or_else(|| drawn_lit.last())
            .map_or(0, |(i, _)| i as u32);
        let drawn = if today.reversed {
            subpaths - 1 - data_index
        } else {
            data_index
        };
        runs.push(SubpathRun {
            drawn: drawn + 1,
            start,
            count: end - start,
        });
    }
    runs
}

/// A line's length, as the resolver measures it.
fn length(a: [f32; 2], b: [f32; 2]) -> f32 {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    libm::sqrtf(dx * dx + dy * dy)
}
