//! Fixture overrides: what the selection does to a fixture's lamp colours,
//! after it has sampled its render product and before it publishes them
//! as its control product (D12, D55).
//!
//! Because they act on the control product, whatever they do shows the
//! same on the canvas (the lamps) and on every output the fixture reaches
//! (its pixels). Output-side selections are output overrides
//! (`output_overrides.rs`) and never come through here.
//!
//! - **A selection inside a fixture** (a lamp, a path, a ring, a circle,
//!   a group, a fixture, a module): every lamp NOT under it dims to
//!   [`DIM`], in every fixture (D16: the selection is always shown). Every
//!   object under a selected thing shows old LightPlayer's pattern
//!   (lp2014's `GenericShapeMapper`): its first lamps blue, its last lamps
//!   red, and between them black but for a white band sweeping from start
//!   to end every two seconds, so you see where its data enters, where it
//!   leaves, and which way it runs. A lamp selected on its own breathes
//!   instead, like the product's `highlight`.
//! - **Drawing** (a path being clicked out, a circle being dragged): the
//!   draft counts as already drawn, its lamps full white and every other
//!   lamp off, so its pixels show exactly where it will land.
//!
//! A lamp outside its fixture's box has no colour at all (D4): no
//! override gives it one.

use crate::domain::breath::{DIM, breath};
use crate::domain::drawn_segment::DrawnSegment;
use crate::domain::node::NodeKind;
use crate::domain::part_ref::PartRef;
use crate::domain::project::Project;
use crate::domain::rgb::Rgb;
use crate::domain::target::Target;

/// How long lp2014's white band takes from an object's first lamp to its
/// last.
pub const BAND_SECONDS: f64 = 2.0;
/// The band's steepness: lp2014's 7, a linear falloff about 2/7 of the
/// object long.
pub const BAND_SHARPNESS: f64 = 7.0;
/// The most lamps at each end the lead-in and lead-out take: lp2014's 15,
/// on panels of ~120 lamps. Shorter objects take an eighth of their
/// length, at least one.
pub const LEAD_LAMPS: usize = 15;
/// lp2014's blue lead-in and red lead-out.
pub const START_COLOUR: Rgb = Rgb::new(0, 0, 255);
pub const END_COLOUR: Rgb = Rgb::new(255, 0, 0);

/// The override rules for one frame, worked out once from the selection.
#[derive(Debug, Clone, PartialEq)]
pub enum FixtureOverrides {
    /// Nothing inside a fixture is selected: lamps keep their colours.
    None,
    /// These fixture-side targets are selected.
    Selection(Vec<Target>),
    /// A draft is being drawn: only its lamps light. `None` when it starts
    /// outside every fixture, so it lands nowhere and nothing lights.
    Drawing(Option<PartRef>),
}

/// How the selection touches one object.
struct Shown {
    /// The whole object is under the selection: lp2014's pattern.
    whole: bool,
    /// Its lamps selected one by one.
    lamps: Vec<u32>,
}

impl FixtureOverrides {
    /// The rules a selection makes: only its fixture-side targets count.
    pub fn of(project: &Project, selection: &[Target]) -> Self {
        let inside: Vec<Target> = selection
            .iter()
            .filter(|t| is_fixture_side(project, t))
            .cloned()
            .collect();
        if inside.is_empty() {
            FixtureOverrides::None
        } else {
            FixtureOverrides::Selection(inside)
        }
    }

    /// Apply the rules to one object's lamp colours (data order, `None`
    /// where the lamp has no colour). `inside` says which lamps are inside
    /// their fixture's box; a lamp outside it stays dark whatever is
    /// selected.
    pub fn apply(
        &self,
        project: &Project,
        object: &DrawnSegment,
        inside: &[bool],
        lamps: &mut [Option<Rgb>],
        time: f64,
    ) {
        let n = lamps.len();
        match self {
            FixtureOverrides::None => {}
            FixtureOverrides::Drawing(draft) => {
                let lit = draft.as_ref() == Some(&object.id.part());
                for (c, inside) in lamps.iter_mut().zip(inside) {
                    if *inside {
                        *c = Some(if lit { Rgb::WHITE } else { Rgb::BLACK });
                    }
                }
            }
            FixtureOverrides::Selection(selected) => {
                let shown = Shown::of(project, selected, object);
                let breath = breath(time);
                for (i, (c, inside)) in lamps.iter_mut().zip(inside).enumerate() {
                    if !*inside {
                        continue;
                    }
                    *c = if shown.whole {
                        Some(lp2014(i, n, time))
                    } else if shown.lamps.contains(&(i as u32)) {
                        Some(breath)
                    } else {
                        c.map(|c| c.scaled(DIM))
                    };
                }
            }
        }
    }
}

impl Shown {
    fn of(p: &Project, selected: &[Target], object: &DrawnSegment) -> Self {
        let target = p.tree_target(&object.id);
        let mut out = Shown {
            whole: false,
            lamps: Vec::new(),
        };
        for t in selected {
            match t {
                Target::Lamp { segment: o, index } if *o == object.id => out.lamps.push(*index),
                Target::Lamp { .. } => {}
                _ => out.whole |= p.is_ancestor_or_self(t, &target),
            }
        }
        out
    }
}

/// A target inside a fixture, or holding fixtures: a lamp, an object, a
/// component, a fixture, a module. Not an output, its ports or pixels, a
/// render source or a card the lab doesn't simulate.
fn is_fixture_side(p: &Project, t: &Target) -> bool {
    match t {
        Target::Port { .. } | Target::Span { .. } | Target::Pixel { .. } => false,
        Target::Node(id) => matches!(
            p.node(id).map(|n| &n.kind),
            Some(NodeKind::Fixture(_) | NodeKind::Module { .. })
        ),
        Target::Part(_) | Target::Generated(_) | Target::Lamp { .. } => true,
    }
}

/// How many lamps at each end lp2014's pattern colours, for an object of
/// `n` lamps: an eighth of it, at least one, at most [`LEAD_LAMPS`].
pub fn lead_lamps(n: usize) -> usize {
    (n / 8).clamp(1, LEAD_LAMPS)
}

/// lp2014's selected-shape pattern (`GenericShapeMapper.render`): the first
/// lamps blue, the last red, and between them black but for a white band.
/// With `t` how far through its sweep the band is (0–1, every
/// [`BAND_SECONDS`]) and `f` how far along the object this lamp is, the
/// band's brightness is `1 − |(f − t)·7 − 1|`, clamped: a linear falloff
/// that runs from start to end.
pub fn lp2014(lamp: usize, n: usize, time: f64) -> Rgb {
    let lead = lead_lamps(n);
    if lamp < lead {
        return START_COLOUR;
    }
    if lamp + lead >= n {
        return END_COLOUR;
    }
    let t = time.rem_euclid(BAND_SECONDS) / BAND_SECONDS;
    let f = lamp as f64 / n as f64;
    let b = (1.0 - ((f - t) * BAND_SHARPNESS - 1.0).abs()).clamp(0.0, 1.0);
    Rgb::grey((b * 255.0).round() as u8)
}
