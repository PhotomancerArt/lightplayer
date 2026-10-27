//! Where a split path's lamps fall into its subpaths.
//!
//! A path split by subpaths renders the unsplit path's lamps, cut at the
//! jumpers. Where it is cut is [`path_gap_breaks`]'s answer, today's (a lamp
//! exactly on a jumper stays with the run before it). Which subpath each run
//! is comes from the same walk over the drawn lengths: the run's first lamp
//! sits on the first subpath, with any length, whose end it hasn't passed.
//! A subpath with no lamps makes no run.

use alloc::vec::Vec;

use crate::map2d_doc::PathShape;
use crate::map2d_resolve::{active_length, distance, inert_segments, is_inert, path_gap_breaks};

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
/// order (last drawn first when the path is reversed).
pub(crate) fn subpath_runs(path: &PathShape) -> Vec<SubpathRun> {
    if path.count == 0 || path.points.len() < 2 {
        return Vec::new();
    }
    let mut points = path.points.clone();
    let inert = inert_segments(path);
    if path.reversed {
        points.reverse();
    }
    let total = active_length(&points, &inert);

    // Each subpath's active stretch, in data order, walked as
    // `path_gap_breaks` walks it: a jumper ends one subpath at the distance
    // walked so far and starts the next.
    let mut stretches: Vec<(f32, f32)> = Vec::new();
    let mut from = 0.0;
    let mut walked = 0.0;
    for (index, pair) in points.windows(2).enumerate() {
        if is_inert(&inert, index) {
            stretches.push((from, walked));
            from = walked;
        } else {
            walked += distance(pair[0], pair[1]);
        }
    }
    stretches.push((from, walked));
    let subpaths = stretches.len() as u32;

    let mut starts = Vec::with_capacity(stretches.len());
    starts.push(0);
    starts.extend(path_gap_breaks(path));
    let mut runs = Vec::with_capacity(starts.len());
    for (i, &start) in starts.iter().enumerate() {
        let end = starts.get(i + 1).copied().unwrap_or(path.count);
        let at = if path.count < 2 {
            0.0
        } else {
            total * (start as f32 / (path.count - 1) as f32)
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
        let drawn = if path.reversed {
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
