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
//!    quarter; the selected lamps breathe white, like the product's
//!    `highlight` (D16: the selection is always shown). A selected line,
//!    ring or circle also shows its direction, old-LightPlayer style:
//!    its first lamps blue, its last lamps red, and a white dot running
//!    from start to end.
//! 3. **Output overrides, in channel space.** A selected port (or output)
//!    breathes across its whole pixel range, patched or not — the user can
//!    always reach the hardware (D13).
//!
//! **While drawing**, the draft counts as already added to the fixture the
//! drag started in, under that fixture's own rules, and only its lamps are
//! lit: the port shows exactly how many lamps the drag makes and where
//! they land.
//!
//! A lamp outside its own fixture's box is never lit by the pattern or by
//! a selection (D4). Unpatched pixels are black.
//!
//! The model never reads a clock: the caller passes the time, in seconds.

use std::collections::BTreeMap;

use crate::editor::{Editor, Gesture};
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
/// How long the direction dot takes from the first lamp to the last.
pub const DOT_SECONDS: f64 = 2.0;
/// How many lamps at each end show the direction (fewer on short objects).
pub const END_LAMPS: usize = 3;
pub const START_COLOUR: Rgb = Rgb::new(40, 90, 255);
pub const END_COLOUR: Rgb = Rgb::new(255, 45, 35);
/// The pattern's brightness: clearly visible, never glaring.
const PATTERN_VALUE: f64 = 0.55;
const PATTERN_SATURATION: f64 = 0.65;
/// How far an object's hue sweeps from its first lamp to its last.
const PATTERN_SWEEP: f64 = 120.0;

/// Every port's pixels, in output then port order.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DeviceFrame {
    pub ports: Vec<PortFrame>,
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
        match &self.gesture {
            Gesture::Drawing { start_world } => self.draft_frame(*start_world),
            _ => self.project.device_frame(&self.selection, time),
        }
    }

    /// Whether the board's pixels move on their own right now (a breathing
    /// selection, a drag being drawn), so the page knows to keep ticking.
    pub fn board_animates(&self) -> bool {
        !self.selection.is_empty() || matches!(self.gesture, Gesture::Drawing { .. })
    }

    /// The draft as if it were already drawn: only its lamps lit, full
    /// white, everything else off. A drag that starts outside every
    /// fixture lands nowhere, so every pixel is off.
    fn draft_frame(&self, start: Vec2) -> DeviceFrame {
        let mut frame = self.project.dark_frame();
        let Some(now) = self.pointer.map(|p| self.camera.to_world(p)) else {
            return frame;
        };
        let Some((fixture, kind)) = self.drawn_into(start, now) else {
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
            if sh.directed {
                return direction(lamp as usize, n, time, breath);
            }
            if sh.whole || sh.lamps.contains(&lamp) {
                return breath;
            }
            pattern.scaled(DIM)
        });

        // 3: a selected port, or every port of a selected output, breathes
        // across its whole range.
        for (output, key) in self.selected_ports(selection) {
            if let Some(pixels) = frame.port_mut(&output, &key) {
                pixels.fill(breath);
            }
        }
        frame
    }

    /// Every port, every pixel black: a board with nothing to show.
    pub fn dark_frame(&self) -> DeviceFrame {
        let mut ports = Vec::new();
        for n in self.outputs() {
            let Some(o) = n.output() else { continue };
            for p in &o.ports {
                ports.push(PortFrame {
                    output: n.id.clone(),
                    key: p.key.clone(),
                    pixels: vec![Rgb::BLACK; p.pixels as usize],
                });
            }
        }
        DeviceFrame { ports }
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
            let Some(pixels) = frame.port_mut(&s.output, &s.port) else {
                continue;
            };
            for lamp in s.lamps.clone() {
                let pixel = (s.start_pixel + lamp - s.lamps.start) as usize;
                let inside = lamps
                    .get(lamp as usize)
                    .is_some_and(|p| bounds.is_some_and(|b| b.contains(*p)));
                if let Some(px) = pixels.get_mut(pixel) {
                    *px = if inside {
                        colour(i, s, lamp, n)
                    } else {
                        Rgb::BLACK
                    };
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
    /// is selected).
    whole: bool,
    /// The line, ring or circle itself is selected: show its direction.
    directed: bool,
    /// Lamps of this object selected one by one.
    lamps: Vec<u32>,
}

impl SpanShown {
    fn of(p: &Project, selection: &[Target], s: &Span) -> Self {
        let object = p.object_target(&s.object);
        let circle = Target::Component(s.object.component.clone());
        let mut out = SpanShown {
            whole: false,
            directed: false,
            lamps: Vec::new(),
        };
        for t in selection {
            match t {
                Target::Lamp { object: o, index } if *o == s.object => out.lamps.push(*index),
                Target::Lamp { .. } | Target::Port { .. } => {}
                _ => {
                    if *t == object || *t == circle {
                        out.directed = true;
                    }
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

/// A selected line or ring: blue start, red end, a white dot running from
/// start to end, the rest breathing.
fn direction(lamp: usize, n: usize, time: f64, breath: Rgb) -> Rgb {
    let ends = END_LAMPS.min(n / 3);
    let phase = time.rem_euclid(DOT_SECONDS) / DOT_SECONDS;
    let dot = (phase * n.saturating_sub(1) as f64).round() as usize;
    if lamp == dot {
        Rgb::WHITE
    } else if lamp < ends {
        START_COLOUR
    } else if lamp + ends >= n {
        END_COLOUR
    } else {
        breath
    }
}
