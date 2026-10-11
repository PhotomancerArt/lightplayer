//! Which thing a click selects: the Figma-style drill-down (D17), over
//! areas (`area.rs`).
//!
//! A click works at the level you are in. With nothing selected, or a
//! top-level thing selected, that is the top level. With something deeper
//! selected, it is the selection's own level — and if nothing at that level
//! is under the cursor, the level above, and so on up. Of the things at that
//! level under the cursor, the nearest wins. Then:
//!
//! - **the nearest is already the (only) selection:** a click goes one level
//!   in — "click again to go in";
//! - **⌘-click:** the deepest thing, directly;
//! - **⌥-click:** the next thing at that level under the cursor, for when
//!   things overlap. Pressed again, the one after that.
//!
//! Hover shows exactly what a click would select. That is the whole
//! teaching device, so it is ONE function used by both.

use crate::camera::Camera;
use crate::fixture::Fixture;
use crate::geom::Vec2;
use crate::target::Target;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickMode {
    /// What a plain click selects, going in when you click the selection.
    Click,
    /// What a press selects before it knows whether it is a drag: never
    /// goes in (pressing the selection keeps it, so you can drag it).
    Press,
    /// ⌘-click: the deepest thing under the cursor.
    Deep,
    /// ⌥-click: the next overlapping thing at the same level.
    Next,
}

/// The things at the click's level under `p`, nearest first.
pub fn level_candidates(f: &Fixture, cam: &Camera, selection: &[Target], p: Vec2) -> Vec<Target> {
    let under = f.things_at(cam, p);
    let mut level = selection.first().and_then(|s| f.parent(s));
    loop {
        let mut cands: Vec<Target> = f
            .children(level.as_ref())
            .into_iter()
            .filter(|t| under.contains(t))
            .collect();
        if !cands.is_empty() {
            cands.sort_by(|a, b| {
                f.distance_to(cam, a, p)
                    .total_cmp(&f.distance_to(cam, b, p))
            });
            return cands;
        }
        match level {
            Some(l) => level = f.parent(&l),
            None => return Vec::new(),
        }
    }
}

pub fn pick(
    f: &Fixture,
    cam: &Camera,
    selection: &[Target],
    p: Vec2,
    mode: PickMode,
) -> Option<Target> {
    let cands = level_candidates(f, cam, selection, p);
    let nearest = cands.first()?.clone();
    match mode {
        PickMode::Press => Some(nearest),
        PickMode::Deep => Some(deepest_from(f, cam, nearest, p)),
        PickMode::Click => {
            if selection.len() == 1 && selection[0] == nearest {
                Some(nearest_child(f, cam, &nearest, p).unwrap_or(nearest))
            } else {
                Some(nearest)
            }
        }
        PickMode::Next => {
            let at = selection
                .first()
                .and_then(|s| cands.iter().position(|c| c == s));
            Some(match at {
                Some(i) => cands[(i + 1) % cands.len()].clone(),
                None => nearest,
            })
        }
    }
}

fn nearest_child(f: &Fixture, cam: &Camera, t: &Target, p: Vec2) -> Option<Target> {
    let under = f.things_at(cam, p);
    f.children(Some(t))
        .into_iter()
        .filter(|c| under.contains(c))
        .min_by(|a, b| {
            f.distance_to(cam, a, p)
                .total_cmp(&f.distance_to(cam, b, p))
        })
}

fn deepest_from(f: &Fixture, cam: &Camera, mut t: Target, p: Vec2) -> Target {
    while let Some(child) = nearest_child(f, cam, &t, p) {
        t = child;
    }
    t
}
