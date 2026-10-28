//! The patch: which lamp lands on which pixel of which port.
//!
//! What is authored is each fixture's mapping: its start and flow, its
//! parts' starts and flows, and overrides by node id
//! (`lpc_mapping::part::Mapping`). The patcher (`lpc_mapping::patching`,
//! the one the engine runs) gives each segment its address; this file lays
//! every lamp on the pixels the patcher's landing says. The result — the
//! effective patch — is derived every time and never stored (D7), so an
//! edit anywhere (a ring's count, a port's size, a deleted output) moves
//! the patch with it and nothing goes stale.
//!
//! The ports each fixture is patched against are its port tables
//! (`port_tables.rs`): Studio hands over the engine's own, so the Graph and
//! the board can't disagree.
//!
//! Two segments may land on the same pixels: nothing refuses it, since a
//! hand patch passes through overlaps on its way to right (plan D7). The
//! patch reports each one, and the board shows the later span, in the
//! patch's order (fixtures in attach order, segments in tree order).
//!
//! Editing by hand is `patch_edits.rs`.

use std::collections::BTreeMap;
use std::ops::Range;

use lpc_mapping::patching::{PatchTarget, Problem, patch_mapping};

use crate::domain::color_order::ColorOrder;
use crate::domain::node::{Node, NodeId};
use crate::domain::notice::Notice;
use crate::domain::part_ref::TreeRef;
use crate::domain::project::Project;
use crate::domain::walked::Walked;

/// Some lamps of one segment, landing on consecutive pixels of one port —
/// up the port, or down it when `reversed`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PixelRun {
    pub fixture: NodeId,
    pub segment: TreeRef,
    /// Lamp indices, in data order.
    pub lamps: Range<u32>,
    pub output: NodeId,
    pub port: String,
    /// The port pixel lamp `lamps.start` lands on.
    pub start_pixel: u32,
    /// Each lamp lands one pixel BEFORE the last one, not after.
    pub reversed: bool,
}

impl PixelRun {
    pub fn len(&self) -> u32 {
        self.lamps.len() as u32
    }

    pub fn is_empty(&self) -> bool {
        self.lamps.is_empty()
    }

    /// The pixel one of its lamps lands on.
    pub fn pixel(&self, lamp: u32) -> u32 {
        let i = lamp - self.lamps.start;
        if self.reversed {
            self.start_pixel - i
        } else {
            self.start_pixel + i
        }
    }

    /// The pixels it covers, lowest first.
    pub fn pixels(&self) -> Range<u32> {
        if self.reversed {
            self.start_pixel + 1 - self.len()..self.start_pixel + 1
        } else {
            self.start_pixel..self.start_pixel + self.len()
        }
    }

    /// The lamp on one of its pixels.
    pub fn lamp_at(&self, pixel: u32) -> Option<u32> {
        if !self.pixels().contains(&pixel) {
            return None;
        }
        Some(if self.reversed {
            self.lamps.start + (self.start_pixel - pixel)
        } else {
            self.lamps.start + (pixel - self.start_pixel)
        })
    }
}

/// Lamps that light nothing: past the end of an output's ports, or with
/// nothing up their chain that puts them on the wire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unpatched {
    pub fixture: NodeId,
    pub segment: TreeRef,
    pub lamps: Range<u32>,
}

/// Something the patch can't do, said plainly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchProblem {
    pub fixture: NodeId,
    pub kind: ProblemKind,
}

/// The patcher's problems (`lpc_mapping::patching::Problem`), naming the
/// project's nodes and ports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProblemKind {
    /// The fixture says nowhere to start, so nothing in it lands.
    NoStart,
    /// A start names an output, by name, that isn't there (or that the
    /// fixture can't reach).
    GoneOutput { on: PatchTarget, output: String },
    /// …or a port its output doesn't have.
    GonePort {
        on: PatchTarget,
        output: String,
        port: String,
    },
    /// An override for a node that isn't there any more (a copy counted
    /// away, a ring removed): kept, never moved to another.
    GoneNode(TreeRef),
    /// A segment whose start was set doesn't fit what is left of its port:
    /// it stays (slots are physical), and lands as far as the port reaches.
    DoesntFit { segment: TreeRef, port: String },
    /// The walk ran past the output's last port: this segment and
    /// everything after it that only follows on land nowhere, until a new
    /// start.
    PastLastPort { segment: TreeRef },
    /// A segment starts part-way into a pixel: the board, and the
    /// product, speak in whole pixels, so it lands nowhere yet.
    MidPixel { segment: TreeRef },
    /// A segment's color order doesn't fit its colors.
    OrderMismatch { segment: TreeRef },
}

/// Pixels of one port where two segments land.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Overlap {
    pub output: NodeId,
    pub port: String,
    pub pixels: Range<u32>,
    /// The two segments, in the patch's order: the board shows the second.
    pub segments: [TreeRef; 2],
}

/// Where one lamp lands: an output's port, and the pixel on it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Landing {
    pub output: NodeId,
    pub port: String,
    pub pixel: u32,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct EffectivePatch {
    pub pixel_runs: Vec<PixelRun>,
    pub unpatched: Vec<Unpatched>,
    pub problems: Vec<PatchProblem>,
    pub overlaps: Vec<Overlap>,
    /// What every node of every fixture's tree did on the walk, and why:
    /// in fixture then tree order.
    pub walked: Vec<Walked>,
}

impl EffectivePatch {
    /// Pixels used on one port.
    pub fn used(&self, output: &NodeId, port: &str) -> u32 {
        self.on_port(output, port).map(PixelRun::len).sum()
    }

    /// How far a port's strip reaches: one past its highest lit pixel.
    pub fn extent(&self, output: &NodeId, port: &str) -> u32 {
        self.on_port(output, port)
            .map(|s| s.pixels().end)
            .max()
            .unwrap_or(0)
    }

    fn on_port<'a>(
        &'a self,
        output: &'a NodeId,
        port: &'a str,
    ) -> impl Iterator<Item = &'a PixelRun> {
        self.pixel_runs
            .iter()
            .filter(move |s| &s.output == output && s.port == port)
    }

    /// A segment's runs, in lamp order.
    pub fn pixel_runs_of<'a>(&'a self, segment: &'a TreeRef) -> impl Iterator<Item = &'a PixelRun> {
        self.pixel_runs
            .iter()
            .filter(move |s| &s.segment == segment)
    }

    /// Where one of a segment's lamps lands, if anywhere.
    pub fn landing(&self, segment: &TreeRef, lamp: u32) -> Option<Landing> {
        let s = self
            .pixel_runs_of(segment)
            .find(|s| s.lamps.contains(&lamp))?;
        Some(Landing {
            output: s.output.clone(),
            port: s.port.clone(),
            pixel: s.pixel(lamp),
        })
    }

    /// Whether any of a segment's lamps lands on a pixel.
    pub fn lands(&self, segment: &TreeRef) -> bool {
        self.pixel_runs_of(segment).next().is_some()
    }

    /// What a node did on the walk, and why.
    pub fn walked(&self, node: &TreeRef) -> Option<&Walked> {
        self.walked.iter().find(|w| &w.node == node)
    }

    /// Where two runs on one port share pixels.
    fn find_overlaps(&mut self) {
        let mut by_port: BTreeMap<(&NodeId, &str), Vec<&PixelRun>> = BTreeMap::new();
        for s in &self.pixel_runs {
            by_port.entry((&s.output, &s.port)).or_default().push(s);
        }
        let mut found = Vec::new();
        for ((output, port), spans) in by_port {
            for (i, a) in spans.iter().enumerate() {
                for b in &spans[i + 1..] {
                    if a.segment == b.segment {
                        continue;
                    }
                    let (pa, pb) = (a.pixels(), b.pixels());
                    let pixels = pa.start.max(pb.start)..pa.end.min(pb.end);
                    if pixels.is_empty() {
                        continue;
                    }
                    found.push(Overlap {
                        output: output.clone(),
                        port: port.to_string(),
                        pixels,
                        segments: [a.segment.clone(), b.segment.clone()],
                    });
                }
            }
        }
        self.overlaps = found;
    }
}

/// Where one lamp of a segment landed: its output, the port's index on it,
/// and the pixel.
type LampAt = Option<(NodeId, usize, u32)>;

impl Project {
    /// What the mappings make. Kept until the project changes
    /// (`derived.rs`), so it is derived afresh after every edit and never
    /// stored.
    pub fn effective_patch(&self) -> &EffectivePatch {
        self.kept_patch()
    }

    /// Patch every fixture, in attach order (the order the board shows
    /// overlaps in), and lay its lamps on the pixels.
    pub(crate) fn compute_patch(&self) -> EffectivePatch {
        let mut patch = EffectivePatch::default();
        let placed = self.placed();
        for n in self.attach_order() {
            let (Some(f), Some(tree)) = (n.fixture(), placed.tree(&n.id)) else {
                continue;
            };
            let ports = self.port_tables(&n.id);
            let patching = patch_mapping(&f.mapping, tree, &ports);
            for w in &patching.walked {
                patch.walked.push(Walked::of(self, &n.id, w, &patching));
            }
            for problem in &patching.problems {
                if let Some(kind) = self.problem_kind(&n.id, problem) {
                    patch.problems.push(PatchProblem {
                        fixture: n.id.clone(),
                        kind,
                    });
                }
            }
            for segment in tree.segments() {
                let id = TreeRef::new(&n.id, segment.id.clone());
                let lamps: Vec<LampAt> = (0..segment.count())
                    .map(|lamp| {
                        let at = patching.landing(&segment.id, lamp)?;
                        let per = patching.segment(&segment.id)?.channels_per_lamp.max(1);
                        if at.channel % per != 0 {
                            return None;
                        }
                        let output = self.output_named(at.output.as_str())?;
                        let key = at.port.0.to_string();
                        let port = self
                            .output(&output)?
                            .ports
                            .iter()
                            .position(|p| p.key == key)?;
                        Some((output, port, at.channel / per))
                    })
                    .collect();
                self.gather(&n.id, &id, &lamps, &mut patch);
            }
        }
        patch.find_overlaps();
        patch
    }

    /// The patcher's problem in the project's terms; `None` for what the
    /// patch reports its own way (overlaps).
    fn problem_kind(&self, fixture: &NodeId, problem: &Problem) -> Option<ProblemKind> {
        let at = |id: &lpc_mapping::part::TreeId| TreeRef::new(fixture, id.clone());
        Some(match problem {
            Problem::NoStart => ProblemKind::NoStart,
            Problem::GoneNode(id) => ProblemKind::GoneNode(at(id)),
            Problem::GoneOutput { output, set_by } => ProblemKind::GoneOutput {
                on: set_by.clone(),
                output: output.as_str().to_string(),
            },
            Problem::GonePort {
                output,
                port,
                set_by,
            } => ProblemKind::GonePort {
                on: set_by.clone(),
                output: output.as_str().to_string(),
                port: port.0.to_string(),
            },
            Problem::DoesntFit { segment, port } => ProblemKind::DoesntFit {
                segment: at(segment),
                port: port.0.to_string(),
            },
            Problem::PastLastPort { segment } => ProblemKind::PastLastPort {
                segment: at(segment),
            },
            Problem::OffLampBoundary { segment } => ProblemKind::MidPixel {
                segment: at(segment),
            },
            Problem::OrderMismatch { segment } => ProblemKind::OrderMismatch {
                segment: at(segment),
            },
            Problem::Overlap { .. } => return None,
        })
    }

    /// A port's pixels no run covers, cut into runs of at most `size`
    /// (lp2014's channel groups, D14): the places on the wire a hand
    /// patch can still put a segment. A last port with no count takes the
    /// rest, so it always has one more run of `size` past its end.
    pub fn free_segments(&self, output: &NodeId, port: &str, size: u32) -> Vec<Range<u32>> {
        self.free_segments_for(output, port, size, &[])
    }

    /// The same, counting `ignore`'s own pixels as free: where segments
    /// could go, including where they partly are now.
    pub fn free_segments_for(
        &self,
        output: &NodeId,
        port: &str,
        size: u32,
        ignore: &[TreeRef],
    ) -> Vec<Range<u32>> {
        let size = size.max(1);
        let spans: Vec<&PixelRun> = self
            .effective_patch()
            .on_port(output, port)
            .filter(|s| !ignore.contains(&s.segment))
            .collect();
        let rest = self
            .output(output)
            .and_then(|o| o.port(port))
            .is_some_and(|p| p.pixels.is_none());
        let count = if rest {
            spans.iter().map(|s| s.pixels().end).max().unwrap_or(0) + size
        } else {
            self.port_pixels(output, port)
        };
        let mut taken = vec![false; count as usize];
        for s in spans {
            for px in s.pixels() {
                if let Some(t) = taken.get_mut(px as usize) {
                    *t = true;
                }
            }
        }
        let mut out = Vec::new();
        let mut px = 0u32;
        while px < count {
            if taken[px as usize] {
                px += 1;
                continue;
            }
            let start = px;
            while px < count && !taken[px as usize] && px - start < size {
                px += 1;
            }
            out.push(start..px);
        }
        out
    }

    /// The first of a fixture's segments, in tree order, with no lamp on
    /// any pixel: the one a hand patch puts on the wire next.
    pub fn next_unplaced(&self, fixture: &NodeId) -> Option<TreeRef> {
        let patch = self.effective_patch();
        self.segments_in(fixture)
            .into_iter()
            .find(|o| !o.lamps.is_empty() && !patch.lands(&o.id))
            .map(|o| o.id.clone())
    }

    /// Nodes in the order the product attaches them: each module's nodes
    /// by name, depth first.
    pub fn attach_order(&self) -> Vec<&Node> {
        fn walk<'a>(nodes: &'a [Node], out: &mut Vec<&'a Node>) {
            let mut sorted: Vec<&Node> = nodes.iter().collect();
            sorted.sort_by(|a, b| a.name.cmp(&b.name));
            for n in sorted {
                out.push(n);
                walk(n.children(), out);
            }
        }
        let mut out = Vec::new();
        walk(self.nodes(), &mut out);
        out
    }

    /// One segment's lamps, as runs and unpatched stretches.
    fn gather(
        &self,
        fixture: &NodeId,
        segment: &TreeRef,
        lamps: &[LampAt],
        patch: &mut EffectivePatch,
    ) {
        let mut i = 0usize;
        while i < lamps.len() {
            let Some((output, port, pixel)) = &lamps[i] else {
                let start = i;
                while i < lamps.len() && lamps[i].is_none() {
                    i += 1;
                }
                patch.unpatched.push(Unpatched {
                    fixture: fixture.clone(),
                    segment: segment.clone(),
                    lamps: start as u32..i as u32,
                });
                continue;
            };
            // Follow the next lamps while they land on the next pixel of
            // the same port, one way.
            let follows = |k: usize, step: i64| match &lamps[k] {
                Some((o, p, x)) => {
                    o == output && p == port && *x as i64 == *pixel as i64 + step * (k - i) as i64
                }
                None => false,
            };
            let reversed = i + 1 < lamps.len() && follows(i + 1, -1);
            let step = if reversed { -1 } else { 1 };
            let mut end = i + 1;
            while end < lamps.len() && follows(end, step) {
                end += 1;
            }
            let key = self
                .output(output)
                .and_then(|o| o.ports.get(*port))
                .map(|p| p.key.clone())
                .unwrap_or_default();
            patch.pixel_runs.push(PixelRun {
                fixture: fixture.clone(),
                segment: segment.clone(),
                lamps: i as u32..end as u32,
                output: output.clone(),
                port: key,
                start_pixel: *pixel,
                reversed,
            });
            i = end;
        }
    }

    /// How many pixels a port has: its count, or — for a last port that
    /// takes the rest — how far the patch reaches on it.
    pub fn port_pixels(&self, output: &NodeId, port: &str) -> u32 {
        match self.output(output).and_then(|o| o.port(port)) {
            Some(p) => match p.pixels {
                Some(n) => n,
                None => self.effective_patch().extent(output, port),
            },
            None => 0,
        }
    }

    /// How many pixels an output has, every port together.
    pub fn output_pixels(&self, output: &NodeId) -> u32 {
        let Some(o) = self.output(output) else {
            return 0;
        };
        o.ports
            .iter()
            .map(|p| self.port_pixels(output, &p.key))
            .sum()
    }

    /// How full a port is: (pixels used, pixels it has).
    pub fn port_fill(&self, output: &NodeId, port: &str) -> (u32, u32) {
        (
            self.effective_patch().used(output, port),
            self.port_pixels(output, port),
        )
    }

    /// The colour orders of the fixtures whose lamps land on a port, in
    /// patch order: one is the usual; two on one strip is worth a look.
    pub fn color_orders_on(&self, output: &NodeId, port: &str) -> Vec<ColorOrder> {
        let mut out = Vec::new();
        for s in &self.effective_patch().pixel_runs {
            if &s.output == output
                && s.port == port
                && let Some(f) = self.fixture(&s.fixture)
                && !out.contains(&f.color_order)
            {
                out.push(f.color_order);
            }
        }
        out
    }

    /// A node as a person names it: `door`, `circle1/r3`, `panel@2`.
    pub fn segment_words(&self, o: &TreeRef) -> String {
        o.id.to_string()
    }

    /// What a patch target names, in words.
    pub fn target_words(&self, fixture: &NodeId, t: &PatchTarget) -> String {
        match t {
            PatchTarget::Fixture => self.node_name(fixture),
            PatchTarget::Part(part) => part.to_string(),
            PatchTarget::Node(id) => id.to_string(),
        }
    }
}

impl PatchProblem {
    /// The problem in rustc style: what, why, what to do.
    pub fn notice(&self, p: &Project) -> Notice {
        let fixture = p.node_name(&self.fixture);
        let on = |t: &PatchTarget| p.target_words(&self.fixture, t);
        let (text, why, help) = match &self.kind {
            ProblemKind::NoStart => (
                format!("nothing says where {fixture} starts"),
                "every fixture starts somewhere of its own: nothing flows in from another fixture"
                    .to_string(),
                "set its Start to a port of an output",
            ),
            ProblemKind::GoneOutput { on: t, output } => (
                format!("{} is patched to {output}, which isn't there", on(t)),
                format!(
                    "no output named {output} reads this fixture, and patching is never pointed somewhere else on its own; ⌘Z brings a deleted one back"
                ),
                "until then it lands nowhere",
            ),
            ProblemKind::GonePort { on: t, output, port } => (
                format!("{} is patched to port {port} of {output}, which is gone", on(t)),
                "its port was deleted, and patching never moves to another port on its own"
                    .to_string(),
                "until then it lands nowhere",
            ),
            ProblemKind::GoneNode(o) => (
                format!("{fixture} has an override for {o}, which no longer exists"),
                "what it was on was deleted, or counted away; an override never moves to another one"
                    .to_string(),
                "it does nothing; it comes back into use if what it was on does",
            ),
            ProblemKind::DoesntFit { segment, port } => (
                format!(
                    "{} doesn't fit what is left of port {port}",
                    p.segment_words(segment)
                ),
                "its start was set there, so it stays: ports are physical".to_string(),
                "the lamps past the port's end light nothing",
            ),
            ProblemKind::PastLastPort { segment } => (
                format!(
                    "{} runs past the output's last port",
                    p.segment_words(segment)
                ),
                "outputs have no order, so it never moves on to another output".to_string(),
                "it, and everything after it that follows on, lights nothing until something sets a new start",
            ),
            ProblemKind::MidPixel { segment } => (
                format!("{} starts part-way into a pixel", p.segment_words(segment)),
                "the board speaks in whole pixels".to_string(),
                "it lands nowhere until it starts on a pixel",
            ),
            ProblemKind::OrderMismatch { segment } => (
                format!(
                    "{}'s color order doesn't fit its colors",
                    p.segment_words(segment)
                ),
                "an order names each of a lamp's colors once".to_string(),
                "give it an order that fits",
            ),
        };
        Notice::refused(text, why, help)
    }
}
