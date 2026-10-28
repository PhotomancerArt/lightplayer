//! The patch: which lamp lands on which pixel of which port.
//!
//! What is authored is rules on a fixture and its components
//! (`patch_rule.rs`) and overrides on objects (`object_override.rs`). The
//! walker (`walker.rs`) turns them into one run per object; this file lays
//! the runs on the pixels. The result — the effective patch — is derived
//! every time and never stored (D7), so an edit anywhere (a ring's count,
//! a port's size, a deleted output) moves the patch with it and nothing
//! goes stale.
//!
//! How a run lands, in the product's arithmetic: an output's ports make
//! one flat wire, in key order. Orientation first, then rotation: lamp `j`
//! of a run of `n` at `c` with rotation `k` lands on `c + ((j' + k) mod n)`,
//! where `j'` is `n − 1 − j` when the run is reversed and `j` otherwise. A
//! run the walker clipped (an explicit start that doesn't fit its port)
//! lands only the lamps that reach no further than the port's end.
//!
//! Two objects may land on the same pixels: nothing refuses it, since a
//! hand patch passes through overlaps on its way to right (plan D7). The
//! patch reports each one, and the board shows the later span, in the
//! patch's order (fixtures in attach order, objects in tree order).
//!
//! Editing by hand is `patch_edits.rs`.

use std::collections::{BTreeMap, HashMap};
use std::ops::Range;

use crate::domain::color_order::ColorOrder;
use crate::domain::node::{Node, NodeId};
use crate::domain::notice::Notice;
use crate::domain::object::ObjectId;
use crate::domain::patch_target::PatchTarget;
use crate::domain::project::Project;
use crate::domain::walker::Walked;

/// Some lamps of one object, landing on consecutive pixels of one port —
/// up the port, or down it when `reversed`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PixelRun {
    pub fixture: NodeId,
    pub object: ObjectId,
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
    pub object: ObjectId,
    pub lamps: Range<u32>,
}

/// Something the patch can't do, said plainly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchProblem {
    pub fixture: NodeId,
    pub kind: ProblemKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProblemKind {
    /// The fixture's Start says nowhere, so nothing in it lands.
    NoStart,
    /// A rule or an override names an output that is gone.
    GoneOutput { on: PatchTarget, output: NodeId },
    /// …or a port that is gone.
    GonePort {
        on: PatchTarget,
        output: NodeId,
        port: String,
    },
    /// Rules for a component that isn't in the fixture any more.
    GoneComponent(crate::domain::component::ComponentId),
    /// An override for an object that isn't there any more (a copy counted
    /// away, a ring removed): kept, never moved to another.
    GoneObject(ObjectId),
    /// An object whose start was set doesn't fit what is left of its port:
    /// it stays (slots are physical), and lands as far as the port reaches.
    DoesntFit { object: ObjectId, port: String },
    /// The walk ran past the output's last port: this object and everything
    /// after it that only follows on land nowhere, until a new start.
    PastLastPort { object: ObjectId },
    /// An object starts part-way into a pixel: the board, and the
    /// product, speak in whole pixels, so it lands nowhere yet.
    MidPixel { object: ObjectId },
}

/// One object's run on an output's flat wire, as the walker made it: what
/// a save writes when the file can't say the rules themselves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectRun {
    pub fixture: NodeId,
    pub object: ObjectId,
    pub output: NodeId,
    /// Where its window starts on the flat wire.
    pub wire: u32,
    pub count: u32,
    pub reversed: bool,
    pub rotation: u32,
}

/// Pixels of one port where two objects land.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Overlap {
    pub output: NodeId,
    pub port: String,
    pub pixels: Range<u32>,
    /// The two objects, in the patch's order: the board shows the second.
    pub objects: [ObjectId; 2],
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
    /// What every node of every fixture's object tree did on the walk, and
    /// why: in fixture then tree order.
    pub walked: Vec<Walked>,
    /// Every object's run on the wire, in the order they were laid.
    pub runs: Vec<ObjectRun>,
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

    /// An object's spans, in lamp order.
    pub fn pixel_runs_of<'a>(&'a self, object: &'a ObjectId) -> impl Iterator<Item = &'a PixelRun> {
        self.pixel_runs.iter().filter(move |s| &s.object == object)
    }

    /// Where one of an object's lamps lands, if anywhere.
    pub fn landing(&self, object: &ObjectId, lamp: u32) -> Option<Landing> {
        let s = self
            .pixel_runs_of(object)
            .find(|s| s.lamps.contains(&lamp))?;
        Some(Landing {
            output: s.output.clone(),
            port: s.port.clone(),
            pixel: s.pixel(lamp),
        })
    }

    /// Whether any of an object's lamps lands on a pixel.
    pub fn lands(&self, object: &ObjectId) -> bool {
        self.pixel_runs_of(object).next().is_some()
    }

    /// What an object did on the walk, and why.
    pub fn walked(&self, object: &ObjectId) -> Option<&Walked> {
        self.walked.iter().find(|w| &w.object == object)
    }

    /// Where two spans on one port share pixels.
    fn find_overlaps(&mut self) {
        let mut by_port: BTreeMap<(&NodeId, &str), Vec<&PixelRun>> = BTreeMap::new();
        for s in &self.pixel_runs {
            by_port.entry((&s.output, &s.port)).or_default().push(s);
        }
        let mut found = Vec::new();
        for ((output, port), spans) in by_port {
            for (i, a) in spans.iter().enumerate() {
                for b in &spans[i + 1..] {
                    if a.object == b.object {
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
                        objects: [a.object.clone(), b.object.clone()],
                    });
                }
            }
        }
        self.overlaps = found;
    }
}

/// One object's run on an output's flat wire: its `count` lamps from
/// `start` (in the fixture's own numbering, objects end to end).
#[derive(Debug, Clone)]
pub(crate) struct Run {
    pub fixture: NodeId,
    pub object: ObjectId,
    pub start: u32,
    pub count: u32,
    pub output: NodeId,
    pub wire: u32,
    pub reversed: bool,
    pub rotation: u32,
    /// How many of its places on the wire are really on its port: the
    /// walker clipped it (an explicit start that doesn't fit). `None`: all.
    pub limit: Option<u32>,
}

impl Run {
    pub(crate) fn wire_end(&self) -> u32 {
        self.wire.saturating_add(self.count)
    }

    /// Where the run's lamp `j` lands: orientation first, then rotation.
    /// `None` past its limit.
    fn wire_of(&self, j: u32) -> Option<u32> {
        let oriented = if self.reversed { self.count - 1 - j } else { j };
        let at = (oriented + self.rotation) % self.count.max(1);
        self.limit.is_none_or(|l| at < l).then_some(self.wire + at)
    }

    pub(crate) fn object_run(&self) -> ObjectRun {
        ObjectRun {
            fixture: self.fixture.clone(),
            object: self.object.clone(),
            output: self.output.clone(),
            wire: self.wire,
            count: self.count,
            reversed: self.reversed,
            rotation: self.rotation,
        }
    }
}

/// A fixture's objects, end to end: each one's id, and where its lamps
/// start in the fixture's numbering.
pub struct FixtureLamps {
    pub id: NodeId,
    pub objects: Vec<(ObjectId, u32, u32)>,
    /// Where each object sits in `objects`.
    by_id: HashMap<ObjectId, usize>,
}

impl FixtureLamps {
    pub fn object(&self, id: &ObjectId) -> Option<(u32, u32)> {
        let (_, start, n) = &self.objects[*self.by_id.get(id)?];
        Some((*start, *n))
    }

    /// The object holding fixture lamp `lamp`, and its index there.
    fn locate(&self, lamp: u32) -> Option<(usize, u32)> {
        let i = self
            .objects
            .partition_point(|(_, start, _)| *start <= lamp)
            .checked_sub(1)?;
        let (_, start, n) = &self.objects[i];
        (lamp < start + n).then_some((i, lamp - start))
    }
}

impl Project {
    /// What the rules and overrides make. Kept until the project changes
    /// (`derived.rs`), so it is derived afresh after every edit and never
    /// stored.
    pub fn effective_patch(&self) -> &EffectivePatch {
        self.kept_patch()
    }

    /// Walk every fixture, in attach order (the order the board shows
    /// overlaps in), and lay the runs.
    pub(crate) fn compute_patch(&self) -> EffectivePatch {
        let mut patch = EffectivePatch::default();
        let fixtures: Vec<FixtureLamps> =
            self.fixtures().iter().map(|n| lamps_of(self, n)).collect();
        let by_id: BTreeMap<&NodeId, &FixtureLamps> = fixtures.iter().map(|f| (&f.id, f)).collect();
        let mut runs: Vec<Run> = Vec::new();
        for n in self.attach_order() {
            let (Some(_), Some(lamps)) = (n.fixture(), by_id.get(&n.id)) else {
                continue;
            };
            let walk = self.walk_fixture(n, lamps);
            runs.extend(walk.runs);
            patch.walked.extend(walk.walked);
            patch.problems.extend(walk.problems);
        }
        patch.runs = runs.iter().map(Run::object_run).collect();
        self.cut_into_spans(&fixtures, &runs, &mut patch);
        patch.find_overlaps();
        patch
    }

    /// A port's pixels no span covers, cut into runs of at most `size`
    /// (lp2014's channel groups, D14): the places on the wire a hand
    /// patch can still put an object. A last port with no count takes the
    /// rest, so it always has one more run of `size` past its end.
    pub fn free_segments(&self, output: &NodeId, port: &str, size: u32) -> Vec<Range<u32>> {
        self.free_segments_for(output, port, size, &[])
    }

    /// The same, counting `ignore`'s own pixels as free: where objects
    /// could go, including where they partly are now.
    pub fn free_segments_for(
        &self,
        output: &NodeId,
        port: &str,
        size: u32,
        ignore: &[ObjectId],
    ) -> Vec<Range<u32>> {
        let size = size.max(1);
        let spans: Vec<&PixelRun> = self
            .effective_patch()
            .on_port(output, port)
            .filter(|s| !ignore.contains(&s.object))
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

    /// The first of a fixture's objects, in tree order, with no lamp on
    /// any pixel: the one a hand patch puts on the wire next.
    pub fn next_unplaced(&self, fixture: &NodeId) -> Option<ObjectId> {
        let patch = self.effective_patch();
        self.objects_in(fixture)
            .into_iter()
            .find(|o| !o.lamps.is_empty() && !patch.lands(&o.id))
            .map(|o| o.id.clone())
    }

    /// Fixtures in the order the product attaches them: each module's
    /// nodes by name, depth first.
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

    /// Lay every run's lamps on its output's ports, and gather them into
    /// spans: per object, in tree order, a span for each stretch of lamps
    /// on consecutive pixels of one port.
    fn cut_into_spans(&self, fixtures: &[FixtureLamps], runs: &[Run], patch: &mut EffectivePatch) {
        // Per fixture, per object, per lamp: (output, port index, pixel).
        type Landing = Option<(NodeId, usize, u32)>;
        let mut landed: BTreeMap<&NodeId, Vec<Vec<Landing>>> = fixtures
            .iter()
            .map(|f| {
                let per = f
                    .objects
                    .iter()
                    .map(|(_, _, n)| vec![None; *n as usize])
                    .collect();
                (&f.id, per)
            })
            .collect();
        let by_id: BTreeMap<&NodeId, &FixtureLamps> = fixtures.iter().map(|f| (&f.id, f)).collect();
        for r in runs {
            let (Some(out), Some(lamps), Some(objects)) = (
                self.output(&r.output),
                by_id.get(&r.fixture),
                landed.get_mut(&r.fixture),
            ) else {
                continue;
            };
            for j in 0..r.count {
                let Some((o, i)) = lamps.locate(r.start + j) else {
                    continue;
                };
                objects[o][i as usize] = r
                    .wire_of(j)
                    .and_then(|w| out.port_at(w))
                    .map(|(port, pixel)| (r.output.clone(), port, pixel));
            }
        }
        for f in fixtures {
            let Some(objects) = landed.get(&f.id) else {
                continue;
            };
            for ((object, _, _), lamps) in f.objects.iter().zip(objects) {
                self.gather(f, object, lamps, patch);
            }
        }
    }

    /// One object's lamps, as spans and unpatched stretches.
    fn gather(
        &self,
        f: &FixtureLamps,
        object: &ObjectId,
        lamps: &[Option<(NodeId, usize, u32)>],
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
                    fixture: f.id.clone(),
                    object: object.clone(),
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
                fixture: f.id.clone(),
                object: object.clone(),
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

    /// An object as a rule names it: `door`, `sector/2`.
    pub fn object_words(&self, o: &ObjectId) -> String {
        match self.component(&o.component) {
            Some(c) if c.is_single_object() => o.component.to_string(),
            _ => o.to_string(),
        }
    }
}

/// A fixture's lamps, objects end to end, in tree order.
pub(crate) fn lamps_of(p: &Project, n: &Node) -> FixtureLamps {
    let mut objects = Vec::new();
    let mut by_id = HashMap::new();
    let mut total = 0u32;
    for o in p.objects_in(&n.id) {
        let count = o.lamps.len() as u32;
        by_id.insert(o.id.clone(), objects.len());
        objects.push((o.id.clone(), total, count));
        total += count;
    }
    let _ = total;
    FixtureLamps {
        id: n.id.clone(),
        objects,
        by_id,
    }
}

impl PatchProblem {
    /// The problem in rustc style: what, why, what to do.
    pub fn notice(&self, p: &Project) -> Notice {
        let fixture = p.node_name(&self.fixture);
        let on = |t: &PatchTarget| match t {
            PatchTarget::Fixture => fixture.clone(),
            PatchTarget::Component(c) => c.to_string(),
            PatchTarget::Object(o) => p.object_words(o),
        };
        let (text, why, help) = match &self.kind {
            ProblemKind::NoStart => (
                format!("nothing says where {fixture} starts"),
                "every fixture starts somewhere of its own: nothing flows in from another fixture"
                    .to_string(),
                "set its Start to a port of an output",
            ),
            ProblemKind::GoneOutput { on: t, output } => (
                format!("{} is patched to {output}, which no longer exists", on(t)),
                format!(
                    "its output was deleted, and patching is never pointed somewhere else on its own; ⌘Z brings {output} back"
                ),
                "until then it lands nowhere",
            ),
            ProblemKind::GonePort { on: t, output, port } => (
                format!(
                    "{} is patched to port {port} of {}, which is gone",
                    on(t),
                    p.node_name(output)
                ),
                "its port was deleted, and patching never moves to another port on its own"
                    .to_string(),
                "until then it lands nowhere",
            ),
            ProblemKind::GoneComponent(c) => (
                format!("{fixture} has rules for {c}, which no longer exists"),
                "what they were on was deleted".to_string(),
                "they do nothing",
            ),
            ProblemKind::GoneObject(o) => (
                format!("{fixture} has an override for {o}, which no longer exists"),
                "what it was on was deleted, or counted away; an override never moves to another object"
                    .to_string(),
                "it does nothing; it comes back into use if the object does",
            ),
            ProblemKind::DoesntFit { object, port } => (
                format!(
                    "{} doesn't fit what is left of port {port}",
                    p.object_words(object)
                ),
                "its start was set there, so it stays: ports are physical".to_string(),
                "the lamps past the port's end light nothing",
            ),
            ProblemKind::PastLastPort { object } => (
                format!(
                    "{} runs past the output's last port",
                    p.object_words(object)
                ),
                "outputs have no order, so it never moves on to another output".to_string(),
                "it, and everything after it that follows on, lights nothing until something sets a new start",
            ),
            ProblemKind::MidPixel { object } => (
                format!(
                    "{} starts part-way into a pixel",
                    p.object_words(object)
                ),
                "the board speaks in whole pixels".to_string(),
                "it lands nowhere until it starts on a pixel",
            ),
        };
        Notice::refused(text, why, help)
    }
}
