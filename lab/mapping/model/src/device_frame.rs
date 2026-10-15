//! The simulated board: what every port's pixels show right now.
//!
//! The editor is always attached to a running board (D44). Until the lab
//! talks to a real one, the board is simulated here, so every later step is
//! tried against pixels, not just a drawing. What a board shows is a
//! function of the project, the patch, the selection and the time — an
//! ordered stack of layers, the most direct control last (D12):
//!
//! 1. **The pattern.** The lab has no shaders, so a stand-in: each fixture
//!    gets a soft colour, and each object sweeps its hue from its first
//!    lamp to its last, so data order reads on the strip.
//! 2. **Fixture overrides, in lamp space** (through the patch). With
//!    anything selected, every lamp NOT under the selection dims to a
//!    quarter (D16: the selection is always shown). Every object under a
//!    selected thing — a path, a ring, a circle, a group, a fixture — shows
//!    old LightPlayer's pattern (lp2014's `GenericShapeMapper`): its first
//!    lamps blue, its last lamps red, and between them black but for a
//!    white band sweeping from start to end every two seconds, so you see
//!    where its data enters, where it leaves, and which way it runs. A lamp
//!    selected on its own breathes white instead, like the product's
//!    `highlight`.
//! 3. **Output overrides, in channel space.** A selected port (or output)
//!    breathes across its whole pixel range, and a selected pixel on its
//!    own, patched or not — the user can always reach the hardware (D13,
//!    D47).
//!
//! **While drawing** (a path being clicked out, a circle being dragged),
//! the draft counts as already added to the fixture it starts in, under
//! that fixture's own rules, and only its lamps are lit: the port shows
//! exactly how many lamps it makes and where they land.
//!
//! The frame also says which pixel every lamp is ([`DeviceFrame::lamp`]),
//! so the canvas draws each lamp in its pixel's colour — the same frame at
//! the same time — and the drawing and the strip read as one.
//!
//! A lamp outside its own fixture's box is never lit by the pattern or by
//! a selection (D4). Unpatched pixels are black.
//!
//! The model never reads a clock: the caller passes the time, in seconds.

use std::collections::BTreeMap;

use crate::editor::{Editor, Gesture, Tool};
use crate::geom::{Rect, Vec2};
use crate::node::NodeId;
use crate::object::ObjectId;
use crate::patch::{EffectivePatch, Span};
use crate::project::Project;
use crate::rgb::Rgb;
use crate::target::Target;

/// What a lamp not under the selection keeps of its pattern.
pub const DIM: f64 = 0.25;
/// One breath of the selection, in seconds (the product's `highlight`).
pub const BREATH_SECONDS: f64 = 0.75;
/// The breath's floor and crest: never dark, so a selection never reads as
/// dead lamps, and always brighter than a dimmed lamp.
pub const BREATH_FLOOR: u8 = 70;
pub const BREATH_CREST: u8 = 255;
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
/// The pattern's brightness: clearly visible, never glaring.
const PATTERN_VALUE: f64 = 0.55;
const PATTERN_SATURATION: f64 = 0.65;
/// How far an object's hue sweeps from its first lamp to its last.
const PATTERN_SWEEP: f64 = 120.0;

/// Every port's pixels, in output then port order — and, for the canvas,
/// which pixel each lamp is, so a lamp can be drawn in its pixel's colour.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DeviceFrame {
    pub ports: Vec<PortFrame>,
    /// (object, lamp index) → (index into `ports`, pixel). Only lamps that
    /// light: patched, and inside their fixture's box.
    lamps: BTreeMap<(ObjectId, u32), (usize, usize)>,
}

/// One port's pixels: one colour per pixel of the strip on it.
#[derive(Debug, Clone, PartialEq)]
pub struct PortFrame {
    pub output: NodeId,
    pub key: String,
    pub pixels: Vec<Rgb>,
}

impl DeviceFrame {
    pub fn port(&self, output: &NodeId, key: &str) -> Option<&[Rgb]> {
        self.ports
            .iter()
            .find(|p| &p.output == output && p.key == key)
            .map(|p| p.pixels.as_slice())
    }

    /// The colour a lamp shows: its pixel's, in this frame. `None` for a
    /// lamp that lights nothing (unpatched, or outside its box).
    pub fn lamp(&self, object: &ObjectId, index: u32) -> Option<Rgb> {
        let (port, pixel) = *self.lamps.get(&(object.clone(), index))?;
        self.ports.get(port)?.pixels.get(pixel).copied()
    }

    fn port_mut(&mut self, output: &NodeId, key: &str) -> Option<&mut Vec<Rgb>> {
        self.ports
            .iter_mut()
            .find(|p| &p.output == output && p.key == key)
            .map(|p| &mut p.pixels)
    }
}

impl Editor {
    /// What the simulated board shows at `time` seconds.
    pub fn device_frame(&self, time: f64) -> DeviceFrame {
        if self.drawing() {
            self.draft_frame()
        } else {
            self.project.device_frame(&self.selection, time)
        }
    }

    /// Whether the board's pixels move on their own right now (a selection's
    /// pattern, a drawing in progress), so the page knows to keep ticking.
    pub fn board_animates(&self) -> bool {
        !self.selection.is_empty() || self.drawing()
    }

    /// A circle being dragged out, or a path being clicked out.
    pub fn drawing(&self) -> bool {
        matches!(self.gesture, Gesture::Drawing { .. })
            || (self.tool == Tool::Path && !self.path_draft.is_empty())
    }

    /// The draft as if it were already drawn: only its lamps lit, full
    /// white, everything else off. A draft that starts outside every
    /// fixture lands nowhere, so every pixel is off.
    fn draft_frame(&self) -> DeviceFrame {
        let mut frame = self.project.dark_frame();
        let Some((fixture, kind)) = self.draft_into() else {
            return frame;
        };
        let mut p = self.project.clone();
        let Some(draft) = p.add(&fixture, kind) else {
            return frame;
        };
        let patch = p.effective_patch();
        p.paint_lamps(&patch, &mut frame, |_, s, _, _| {
            if s.object.component == draft {
                Rgb::WHITE
            } else {
                Rgb::BLACK
            }
        });
        frame
    }
}

impl Project {
    /// What the board shows for this selection at `time` seconds: the
    /// pattern, then the selection in lamp space, then in channel space.
    pub fn device_frame(&self, selection: &[Target], time: f64) -> DeviceFrame {
        let patch = self.effective_patch();
        let mut frame = self.dark_frame();
        let breath = Rgb::grey(breath_level(time));
        let fixtures: Vec<NodeId> = self.fixtures().iter().map(|n| n.id.clone()).collect();
        let shown: Vec<SpanShown> = patch
            .spans
            .iter()
            .map(|s| SpanShown::of(self, selection, s))
            .collect();
        let anything = !selection.is_empty();

        // 1 and 2: the pattern, then the selection in lamp space.
        self.paint_lamps(&patch, &mut frame, |i, s, lamp, n| {
            let hue_base = fixtures.iter().position(|f| *f == s.fixture).unwrap_or(0) as f64;
            let pattern = pattern(hue_base, lamp, n);
            let sh = &shown[i];
            if !anything {
                return pattern;
            }
            if sh.whole {
                return lp2014(lamp as usize, n, time);
            }
            if sh.lamps.contains(&lamp) {
                return breath;
            }
            pattern.scaled(DIM)
        });

        // 3: a selected port, or every port of a selected output, breathes
        // across its whole range; a selected pixel on its own.
        for (output, key) in self.selected_ports(selection) {
            if let Some(pixels) = frame.port_mut(&output, &key) {
                pixels.fill(breath);
            }
        }
        for t in selection {
            if let Target::Pixel { output, key, index } = t
                && let Some(px) = frame
                    .port_mut(output, key)
                    .and_then(|p| p.get_mut(*index as usize))
            {
                *px = breath;
            }
        }
        frame
    }

    /// Every port, every pixel black: a board with nothing to show.
    pub fn dark_frame(&self) -> DeviceFrame {
        let mut ports = Vec::new();
        let patch = self.effective_patch();
        for n in self.outputs() {
            let Some(o) = n.output() else { continue };
            for p in &o.ports {
                let pixels = p.pixels.unwrap_or_else(|| patch.extent(&n.id, &p.key));
                ports.push(PortFrame {
                    output: n.id.clone(),
                    key: p.key.clone(),
                    pixels: vec![Rgb::BLACK; pixels as usize],
                });
            }
        }
        DeviceFrame {
            ports,
            lamps: BTreeMap::new(),
        }
    }

    /// Paint every patched lamp onto its pixel, with the colour `colour`
    /// gives it from (span index, span, lamp index, lamps in its object).
    /// A lamp outside its own fixture's box stays black, whatever it is
    /// given (D4).
    fn paint_lamps(
        &self,
        patch: &EffectivePatch,
        frame: &mut DeviceFrame,
        mut colour: impl FnMut(usize, &Span, u32, usize) -> Rgb,
    ) {
        let layout = self.layout();
        let objects: BTreeMap<ObjectId, (NodeId, Vec<Vec2>)> = self
            .placed_objects()
            .into_iter()
            .map(|(f, o)| (o.id, (f, o.lamps.iter().map(|l| l.pos).collect())))
            .collect();
        for (i, s) in patch.spans.iter().enumerate() {
            let Some((fixture, lamps)) = objects.get(&s.object) else {
                continue;
            };
            let bounds: Option<Rect> = layout.rects.get(fixture).copied();
            let n = lamps.len();
            let Some(port) = frame
                .ports
                .iter()
                .position(|p| p.output == s.output && p.key == s.port)
            else {
                continue;
            };
            for lamp in s.lamps.clone() {
                let pixel = s.pixel(lamp) as usize;
                let inside = lamps
                    .get(lamp as usize)
                    .is_some_and(|p| bounds.is_some_and(|b| b.contains(*p)));
                let Some(px) = frame.ports[port].pixels.get_mut(pixel) else {
                    continue;
                };
                if inside {
                    *px = colour(i, s, lamp, n);
                    frame.lamps.insert((s.object.clone(), lamp), (port, pixel));
                } else {
                    *px = Rgb::BLACK;
                }
            }
        }
    }

    /// The ports a selection names in channel space: a port, or every port
    /// of a selected output.
    fn selected_ports(&self, selection: &[Target]) -> Vec<(NodeId, String)> {
        let mut out = Vec::new();
        for t in selection {
            match t {
                Target::Port { output, key } => out.push((output.clone(), key.clone())),
                Target::Node(id) => {
                    if let Some(o) = self.output(id) {
                        out.extend(o.ports.iter().map(|p| (id.clone(), p.key.clone())));
                    }
                }
                _ => {}
            }
        }
        out
    }
}

/// How the selection touches one span's object.
struct SpanShown {
    /// The whole object is under the selection (it, or something above it,
    /// is selected): it shows lp2014's pattern.
    whole: bool,
    /// Lamps of this object selected one by one.
    lamps: Vec<u32>,
}

impl SpanShown {
    fn of(p: &Project, selection: &[Target], s: &Span) -> Self {
        let object = p.object_target(&s.object);
        let mut out = SpanShown {
            whole: false,
            lamps: Vec::new(),
        };
        for t in selection {
            match t {
                Target::Lamp { object: o, index } if *o == s.object => out.lamps.push(*index),
                Target::Lamp { .. } | Target::Port { .. } | Target::Pixel { .. } => {}
                _ => {
                    if p.is_ancestor_or_self(t, &object) {
                        out.whole = true;
                    }
                }
            }
        }
        out
    }
}

/// The breath at `time`: a raised cosine from the floor (at 0) to the crest
/// (half a breath in).
pub fn breath_level(time: f64) -> u8 {
    let phase = 0.5 - 0.5 * (std::f64::consts::TAU * time / BREATH_SECONDS).cos();
    let span = (BREATH_CREST - BREATH_FLOOR) as f64;
    BREATH_FLOOR + (span * phase).round() as u8
}

/// The stand-in pattern: the fixture's hue, swept along the object.
fn pattern(fixture_index: f64, lamp: u32, n: usize) -> Rgb {
    let t = if n > 1 {
        lamp as f64 / (n - 1) as f64
    } else {
        0.0
    };
    // The golden angle keeps neighbouring fixtures' hues apart.
    let hue = 30.0 + fixture_index * 137.5 + PATTERN_SWEEP * t;
    Rgb::from_hsv(hue, PATTERN_SATURATION, PATTERN_VALUE)
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
