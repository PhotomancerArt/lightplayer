//! The simulated board: a small recreation of the engine's dataflow (D55).
//!
//! The editor is always attached to a running board (D44). Until the lab
//! talks to a real one, the board is simulated here, as the product's
//! engine runs it — nothing too crazy, and outside the engine's
//! constraints:
//!
//! 1. **A render source** (a shader) publishes a render product on a
//!    visual channel: a picture to sample at a point and a time
//!    (`render_product.rs`; a stand-in pattern, since the lab runs no
//!    GLSL).
//! 2. **A fixture** reads that channel, samples the picture at each of its
//!    lamps (in its own box, as the unit square), applies its **fixture
//!    overrides** — the selection inside it, or a draft being drawn
//!    (`fixture_overrides.rs`) — and publishes the result as its
//!    **control product**: one colour per lamp, in data order
//!    (`control_product.rs`).
//! 3. **An output** reads its control channel: every fixture writing it
//!    arrives as its own control product, the patch places each on the
//!    output's pixels, and then the output applies its **output
//!    overrides** — a selected output, port or pixel — to its own pixels
//!    only (`output_overrides.rs`).
//!
//! Evaluation is pulled, like the engine's: each output asks for the
//! control products it needs, each fixture asked for samples its render
//! product once, and what has been worked out is kept for the rest of the
//! frame. The canvas is the last consumer: it draws every fixture's lamps
//! in their control product's colours, so a lamp never shows an output
//! override — selecting a pixel changes the pixel, not the lamp.
//!
//! The model never reads a clock: the caller passes the time, in seconds.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

use crate::domain::bindings::Binding;
use crate::domain::control_product::ControlProduct;
use crate::domain::derived::Wiring;
use crate::domain::fixture_overrides::FixtureOverrides;
use crate::domain::node::NodeId;
use crate::domain::object::ObjectId;
use crate::domain::output_overrides::OutputOverrides;
use crate::domain::project::Project;
use crate::domain::rgb::Rgb;
use crate::domain::target::Target;

/// One frame of the board: every fixture's control product and every
/// port's pixels.
#[derive(Debug, Clone, Default)]
pub struct BoardFrame {
    /// Every port's pixels, in output then port order.
    pub ports: Vec<PortFrame>,
    /// Per output, the fixtures whose control products it took, in the
    /// order they start on its wire.
    pub inputs: Vec<(NodeId, Vec<NodeId>)>,
    /// Every lamp's colour in its fixture's control product, all fixtures'
    /// lamps end to end (the order `derived.rs` keeps them in).
    lamps: Vec<Option<Rgb>>,
    /// Each fixture's stretch of `lamps`.
    fixtures: HashMap<NodeId, Range<usize>>,
    /// Which pixel each lamp lands on, shared by every frame made from one
    /// project (`derived.rs`).
    wiring: Arc<Wiring>,
}

impl PartialEq for BoardFrame {
    fn eq(&self, other: &Self) -> bool {
        self.ports == other.ports
            && self.inputs == other.inputs
            && self.lamps == other.lamps
            && self.fixtures == other.fixtures
            && (Arc::ptr_eq(&self.wiring, &other.wiring) || self.wiring == other.wiring)
    }
}

/// One port's pixels: one colour per pixel of the strip on it.
#[derive(Debug, Clone, PartialEq)]
pub struct PortFrame {
    pub output: NodeId,
    pub key: String,
    pub pixels: Vec<Rgb>,
}

impl BoardFrame {
    /// One port's pixels, after its output's overrides.
    pub fn port(&self, output: &NodeId, key: &str) -> Option<&[Rgb]> {
        self.ports
            .iter()
            .find(|p| &p.output == output && p.key == key)
            .map(|p| p.pixels.as_slice())
    }

    /// A fixture's control product in this frame.
    pub fn control_product<'a>(&'a self, fixture: &'a NodeId) -> Option<ControlProduct<'a>> {
        let range = self.fixtures.get(fixture)?.clone();
        Some(ControlProduct {
            fixture,
            lamps: &self.lamps[range],
        })
    }

    /// The fixtures whose control products an output took, in the order
    /// they start on its wire.
    pub fn inputs(&self, output: &NodeId) -> &[NodeId] {
        self.inputs
            .iter()
            .find(|(o, _)| o == output)
            .map_or(&[], |(_, f)| f.as_slice())
    }

    /// The colour a lamp shows on the canvas: its colour in its fixture's
    /// control product. `None` for a lamp with no colour (outside its box,
    /// or nothing renders for its fixture).
    pub fn lamp(&self, object: &ObjectId, index: u32) -> Option<Rgb> {
        let first = *self.wiring.first_lamp.get(object)?;
        *self.lamps.get(first + index as usize)?
    }

    /// The colours of an object's first `lamps` lamps, lamp 0 first, as
    /// [`BoardFrame::lamp`] gives them: what the canvas draws, one lookup
    /// per object rather than per lamp.
    pub fn object_lamps<'a>(
        &'a self,
        object: &ObjectId,
        lamps: usize,
    ) -> impl Iterator<Item = Option<Rgb>> + 'a {
        let first = self.wiring.first_lamp.get(object).copied();
        (0..lamps).map(move |i| *self.lamps.get(first? + i)?)
    }

    /// Whether a lamp is patched to a pixel that exists: whether anything
    /// on the board shows it.
    pub fn lands(&self, object: &ObjectId, index: u32) -> bool {
        self.wiring.lamp(object, index).is_some()
    }
}

impl Project {
    /// What the board shows for this selection at `time` seconds.
    pub fn board_frame(&self, selection: &[Target], time: f64) -> BoardFrame {
        let fixture = FixtureOverrides::of(self, selection);
        let output = OutputOverrides::of(self, selection, &fixture);
        self.evaluate(fixture, output, time)
    }

    /// Run the dataflow once: pull from every output, then from the canvas
    /// (every fixture's lamps).
    pub(crate) fn evaluate(
        &self,
        fixture_rules: FixtureOverrides,
        output_rules: OutputOverrides,
        time: f64,
    ) -> BoardFrame {
        let mut board = Board {
            project: self,
            wiring: self.wiring(),
            bindings: self.bindings(),
            fixture_rules,
            output_rules,
            time,
            lamps: vec![None; self.lamp_count()],
            done: HashMap::new(),
        };
        let mut ports = Vec::new();
        let mut inputs = Vec::new();
        for n in self.outputs() {
            let (p, i) = board.output(&n.id);
            ports.extend(p);
            inputs.push((n.id.clone(), i));
        }
        for n in self.fixtures() {
            board.control_product(&n.id);
        }
        BoardFrame {
            ports,
            inputs,
            lamps: board.lamps,
            fixtures: board.done,
            wiring: board.wiring,
        }
    }
}

/// One frame's evaluation, keeping what it has worked out.
struct Board<'a> {
    project: &'a Project,
    wiring: Arc<Wiring>,
    bindings: Vec<Binding>,
    fixture_rules: FixtureOverrides,
    output_rules: OutputOverrides,
    time: f64,
    lamps: Vec<Option<Rgb>>,
    /// The fixtures whose control products are worked out, and where.
    done: HashMap<NodeId, Range<usize>>,
}

impl Board<'_> {
    /// A fixture's control product: its render product sampled at its
    /// lamps, then its overrides. Worked out once a frame.
    fn control_product(&mut self, fixture: &NodeId) {
        if self.done.contains_key(fixture) {
            return;
        }
        let p = self.project;
        let placed = p.placed();
        let (Some(range), Some(fx)) = (placed.fixture_lamps(fixture), p.fixture(fixture)) else {
            return;
        };
        let render = p.render_product(fixture);
        let origin = p.origin(fixture);
        for (i, (f, object)) in placed.list.iter().enumerate() {
            if f != fixture {
                continue;
            }
            let first = placed.first_lamp(i);
            let n = object.lamps.len();
            let inside = &placed.inside()[first..first + n];
            let lamps = &mut self.lamps[first..first + n];
            for ((c, l), inside) in lamps.iter_mut().zip(&object.lamps).zip(inside) {
                *c = match (&render, inside) {
                    (Some(r), true) => Some(r.sample(fx.uv(l.pos - origin), self.time)),
                    _ => None,
                };
            }
            self.fixture_rules
                .apply(p, object, inside, lamps, self.time);
        }
        self.done.insert(fixture.clone(), range);
    }

    /// An output's pixels: the control products bound to it, placed by
    /// the patch, then its overrides. Also which fixtures it took, in the
    /// order they start on its wire.
    fn output(&mut self, output: &NodeId) -> (Vec<PortFrame>, Vec<NodeId>) {
        let feeders: Vec<NodeId> = self
            .bindings
            .iter()
            .filter(|b| &b.to == output)
            .map(|b| b.from.clone())
            .collect();
        for f in &feeders {
            self.control_product(f);
        }
        let wiring = self.wiring.clone();
        let first_port = wiring.ports.iter().position(|(o, _, _)| o == output);
        let mut ports: Vec<PortFrame> = wiring
            .ports
            .iter()
            .filter(|(o, _, _)| o == output)
            .map(|(o, key, pixels)| PortFrame {
                output: o.clone(),
                key: key.clone(),
                pixels: vec![Rgb::BLACK; *pixels],
            })
            .collect();
        // Where each feeder starts on the wire: (port, pixel).
        let mut starts: HashMap<&NodeId, (usize, u32)> = HashMap::new();
        let patch = self.project.effective_patch();
        for (s, w) in patch.spans.iter().zip(&wiring.spans) {
            let (Some(w), Some(first_port)) = (w, first_port) else {
                continue;
            };
            if &s.output != output {
                continue;
            }
            let Some(port) = ports.get_mut(w.port - first_port) else {
                continue;
            };
            for &(lamp, pixel) in &w.lamps {
                port.pixels[pixel as usize] = self
                    .lamps
                    .get(w.first_lamp + lamp as usize)
                    .copied()
                    .flatten()
                    .unwrap_or(Rgb::BLACK);
                let at = (w.port, pixel);
                starts
                    .entry(&s.fixture)
                    .and_modify(|s| *s = (*s).min(at))
                    .or_insert(at);
            }
        }
        for port in &mut ports {
            self.output_rules
                .apply(output, &port.key, &mut port.pixels, self.time);
        }
        let mut inputs = feeders.clone();
        inputs.sort_by_key(|f| starts.get(f).copied().unwrap_or((usize::MAX, u32::MAX)));
        (ports, inputs)
    }
}
