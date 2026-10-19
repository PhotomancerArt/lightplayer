//! The patch: which lamp lands on which pixel of which port.
//!
//! What is authored is a fixture's **rules**. What they make — the
//! effective patch — is derived every time and never stored (D7), so an
//! edit anywhere (a ring's count, a port's size, a deleted output) moves
//! the patch with it and nothing goes stale.
//!
//! Two kinds of rule, the product's (fit-check §2–§3):
//!
//! - **place** — a wiring edit (D11): one object's lamps, as one run,
//!   starting on a port's pixel, forward or reversed, and rotated inside
//!   its own window. The product's patch row `["/sector/2", out, lamp,
//!   "r", offset]`.
//! - **auto** — every lamp no place rule claims, in fixture order, flows
//!   onto an output. The product's auto flow over a bus channel.
//!
//! How they land, in the product's own arithmetic (`lpc-mapping`'s
//! `resolve_patch`, the engine's `plan_fragments`):
//!
//! 1. An output's ports make **one flat wire**, in key order: port 0's
//!    pixels, then port 1's. A run that runs past a port's end carries on
//!    into the next port. The last port may have no count, and takes the
//!    rest; past the end of a counted last port, lamps light nothing.
//! 2. **Place runs land where they say.** Orientation first, then
//!    rotation: lamp `j` of a run of `n` placed at `c` with rotation `k`
//!    lands on `c + ((j' + k) mod n)`, where `j'` is `n − 1 − j` when the
//!    run is reversed and `j` otherwise.
//! 3. A fixture with place rules AND an auto rule flows its other lamps on
//!    after its own runs on that output.
//! 4. A fixture with only an auto rule flows on after every run on that
//!    output, one fixture after another in **attach order** — each
//!    module's nodes by name, depth first: the product's provider order
//!    (fit-check §5.2). Moving one ahead of another on a wire is a wiring
//!    edit, a place rule, not a move in the tree.
//! 5. A fixture with no auto rule is manual: a lamp no rule places is
//!    dark.
//!
//! A rule that names something gone — an output, a port, an object —
//! places nothing and is reported as a problem; it is never retargeted on
//! its own. Its lamps fall to the next rule, or are left unpatched. Two
//! place rules for one object: the first wins (one object, one window).
//!
//! Two objects may land on the same pixels: nothing refuses it, since a
//! hand patch passes through overlaps on its way to right (plan D7). The
//! patch reports each one, and the board shows the later span, in the
//! patch's order (fixtures in attach order, objects in tree order).
//!
//! Editing the rules by hand is `patch_edits.rs`.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::ops::Range;

use crate::domain::color_order::ColorOrder;
use crate::domain::node::{Node, NodeId};
use crate::domain::notice::Notice;
use crate::domain::object::ObjectId;
use crate::domain::project::Project;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatchRule {
    /// Every lamp no place rule claims flows onto `output`, after what is
    /// already there.
    Auto { output: NodeId },
    /// One object's lamps start on `pixel` of `output`'s port `port`.
    /// `reversed` lays the run down end first; `rotation` turns it inside
    /// its own window, in lamps.
    Place {
        object: ObjectId,
        output: NodeId,
        port: String,
        pixel: u32,
        reversed: bool,
        rotation: u32,
    },
}

impl PatchRule {
    pub fn output(&self) -> &NodeId {
        match self {
            PatchRule::Auto { output } | PatchRule::Place { output, .. } => output,
        }
    }
}

/// Some lamps of one object, landing on consecutive pixels of one port —
/// up the port, or down it when `reversed`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
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

impl Span {
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

/// Lamps that light nothing: past the end of an output's ports, or in a
/// fixture with no rule that places them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unpatched {
    pub fixture: NodeId,
    pub object: ObjectId,
    pub lamps: Range<u32>,
}

/// A rule that can't do what it says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchProblem {
    pub fixture: NodeId,
    /// Its index in the fixture's rules.
    pub rule: usize,
    /// The output it names.
    pub missing: NodeId,
    /// What is gone: the output, its port, or the object it places.
    pub gone: Gone,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Gone {
    Output,
    Port(String),
    Object(ObjectId),
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
    pub spans: Vec<Span>,
    pub unpatched: Vec<Unpatched>,
    pub problems: Vec<PatchProblem>,
    pub overlaps: Vec<Overlap>,
}

impl EffectivePatch {
    /// Pixels used on one port.
    pub fn used(&self, output: &NodeId, port: &str) -> u32 {
        self.on_port(output, port).map(Span::len).sum()
    }

    /// How far a port's strip reaches: one past its highest lit pixel.
    pub fn extent(&self, output: &NodeId, port: &str) -> u32 {
        self.on_port(output, port)
            .map(|s| s.pixels().end)
            .max()
            .unwrap_or(0)
    }

    fn on_port<'a>(&'a self, output: &'a NodeId, port: &'a str) -> impl Iterator<Item = &'a Span> {
        self.spans
            .iter()
            .filter(move |s| &s.output == output && s.port == port)
    }

    /// An object's spans, in lamp order.
    pub fn spans_of<'a>(&'a self, object: &'a ObjectId) -> impl Iterator<Item = &'a Span> {
        self.spans.iter().filter(move |s| &s.object == object)
    }

    /// Where one of an object's lamps lands, if anywhere.
    pub fn landing(&self, object: &ObjectId, lamp: u32) -> Option<Landing> {
        let s = self.spans_of(object).find(|s| s.lamps.contains(&lamp))?;
        Some(Landing {
            output: s.output.clone(),
            port: s.port.clone(),
            pixel: s.pixel(lamp),
        })
    }

    /// Whether any of an object's lamps lands on a pixel.
    pub fn lands(&self, object: &ObjectId) -> bool {
        self.spans_of(object).next().is_some()
    }

    /// Where two spans on one port share pixels.
    fn find_overlaps(&mut self) {
        let mut by_port: BTreeMap<(&NodeId, &str), Vec<&Span>> = BTreeMap::new();
        for s in &self.spans {
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

/// One run on an output's flat wire: `count` of a fixture's lamps from
/// `start` (in the fixture's own numbering, objects end to end).
struct Run {
    fixture: NodeId,
    start: u32,
    count: u32,
    output: NodeId,
    wire: u32,
    reversed: bool,
    rotation: u32,
}

impl Run {
    fn wire_end(&self) -> u32 {
        self.wire.saturating_add(self.count)
    }

    /// Where the run's lamp `j` lands: orientation first, then rotation.
    fn wire_of(&self, j: u32) -> u32 {
        let oriented = if self.reversed { self.count - 1 - j } else { j };
        self.wire + (oriented + self.rotation) % self.count.max(1)
    }
}

/// A fixture's objects, end to end: each one's id, and where its lamps
/// start in the fixture's numbering.
struct FixtureLamps {
    id: NodeId,
    objects: Vec<(ObjectId, u32, u32)>,
    /// Where each object sits in `objects`.
    by_id: HashMap<ObjectId, usize>,
    total: u32,
}

impl FixtureLamps {
    fn object(&self, id: &ObjectId) -> Option<(u32, u32)> {
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
    /// What the rules make. Kept until the project changes (`derived.rs`),
    /// so it is derived afresh after every edit and never stored.
    pub fn effective_patch(&self) -> &EffectivePatch {
        self.kept_patch()
    }

    /// Work the patch out from the rules.
    pub(crate) fn compute_patch(&self) -> EffectivePatch {
        let mut patch = EffectivePatch::default();
        let fixtures: Vec<FixtureLamps> =
            self.fixtures().iter().map(|n| lamps_of(self, n)).collect();
        let by_id: BTreeMap<&NodeId, &FixtureLamps> = fixtures.iter().map(|f| (&f.id, f)).collect();

        let mut runs: Vec<Run> = Vec::new();
        // Fixtures that only auto-flow, in attach order, and their output.
        let mut flows: Vec<(&FixtureLamps, NodeId)> = Vec::new();
        for n in self.attach_order() {
            let (Some(f), Some(lamps)) = (n.fixture(), by_id.get(&n.id)) else {
                continue;
            };
            let mut auto: Option<NodeId> = None;
            let mut placed: BTreeSet<&ObjectId> = BTreeSet::new();
            let mut own: Vec<Run> = Vec::new();
            for (i, rule) in f.rules.iter().enumerate() {
                let problem = |gone| PatchProblem {
                    fixture: n.id.clone(),
                    rule: i,
                    missing: rule.output().clone(),
                    gone,
                };
                let Some(out) = self.output(rule.output()) else {
                    patch.problems.push(problem(Gone::Output));
                    continue;
                };
                match rule {
                    PatchRule::Auto { output } => {
                        auto.get_or_insert_with(|| output.clone());
                    }
                    PatchRule::Place {
                        object,
                        output,
                        port,
                        pixel,
                        reversed,
                        rotation,
                    } => {
                        let Some(port_start) = out.port_start(port) else {
                            patch.problems.push(problem(Gone::Port(port.clone())));
                            continue;
                        };
                        let Some((start, count)) = lamps.object(object) else {
                            patch.problems.push(problem(Gone::Object(object.clone())));
                            continue;
                        };
                        if count == 0 || !placed.insert(object) {
                            continue;
                        }
                        own.push(Run {
                            fixture: n.id.clone(),
                            start,
                            count,
                            output: output.clone(),
                            wire: port_start.saturating_add(*pixel),
                            reversed: *reversed,
                            rotation: rotation % count,
                        });
                    }
                }
            }
            match auto {
                Some(output) if !own.is_empty() => {
                    let reflow = reflow(lamps, &own, &output);
                    runs.extend(own);
                    runs.extend(reflow);
                }
                Some(output) => {
                    runs.extend(own);
                    flows.push((lamps, output));
                }
                None => runs.extend(own),
            }
        }
        // Fixtures that only auto-flow go on after every run on their
        // output, one after another.
        let mut cursors: BTreeMap<NodeId, u32> = BTreeMap::new();
        for r in &runs {
            let c = cursors.entry(r.output.clone()).or_default();
            *c = (*c).max(r.wire_end());
        }
        for (lamps, output) in flows {
            let cursor = cursors.entry(output.clone()).or_default();
            runs.push(Run {
                fixture: lamps.id.clone(),
                start: 0,
                count: lamps.total,
                output,
                wire: *cursor,
                reversed: false,
                rotation: 0,
            });
            *cursor = cursor.saturating_add(lamps.total);
        }
        self.cut_into_spans(&fixtures, &runs, &mut patch);
        patch.find_overlaps();
        patch
    }

    /// A port's pixels no span covers, cut into runs of at most `size`
    /// (lp2014's channel groups, D14): the places on the wire a hand
    /// patch can still put an object. A last port with no count takes the
    /// rest, so it always has one more run of `size` past its end.
    pub fn free_segments(&self, output: &NodeId, port: &str, size: u32) -> Vec<Range<u32>> {
        self.free_segments_for(output, port, size, None)
    }

    /// The same, counting `ignore`'s own pixels as free: where an object
    /// could go, including where it partly is now.
    pub fn free_segments_for(
        &self,
        output: &NodeId,
        port: &str,
        size: u32,
        ignore: Option<&ObjectId>,
    ) -> Vec<Range<u32>> {
        let size = size.max(1);
        let spans: Vec<&Span> = self
            .effective_patch()
            .on_port(output, port)
            .filter(|s| Some(&s.object) != ignore)
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
                objects[o][i as usize] = out
                    .port_at(r.wire_of(j))
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
            patch.spans.push(Span {
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
        for s in &self.effective_patch().spans {
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

    /// A rule in words: `auto → main`, `sector/2 → 1 port 3 @0 · reversed`.
    pub fn rule_label(&self, rule: &PatchRule) -> String {
        let to = |output: &NodeId| match self.node(output) {
            Some(n) => n.name.clone(),
            None => format!("{output} (gone)"),
        };
        match rule {
            PatchRule::Auto { output } => format!("auto → {}", to(output)),
            PatchRule::Place {
                object,
                output,
                port,
                pixel,
                reversed,
                rotation,
            } => {
                let port_name = self
                    .output(output)
                    .and_then(|o| o.port(port))
                    .map_or_else(|| format!("port {port}"), |p| p.name.clone());
                let mut s = format!(
                    "{} → {} {port_name} @{pixel}",
                    self.object_words(object),
                    to(output)
                );
                if *reversed {
                    s.push_str(" · reversed");
                }
                if *rotation != 0 {
                    s.push_str(&format!(" · turned {rotation}"));
                }
                s
            }
        }
    }

    /// An object as a rule names it: `door`, `sector/2`.
    pub fn object_words(&self, o: &ObjectId) -> String {
        match self.component(&o.component) {
            Some(c) if c.is_single_object() => o.component.to_string(),
            _ => o.to_string(),
        }
    }
}

/// A fixture's lamps, objects end to end, in authoring order.
fn lamps_of(p: &Project, n: &Node) -> FixtureLamps {
    let mut objects = Vec::new();
    let mut by_id = HashMap::new();
    let mut total = 0u32;
    for o in p.objects_in(&n.id) {
        let count = o.lamps.len() as u32;
        by_id.insert(o.id.clone(), objects.len());
        objects.push((o.id.clone(), total, count));
        total += count;
    }
    FixtureLamps {
        id: n.id.clone(),
        objects,
        by_id,
        total,
    }
}

/// A fixture's lamps that no place rule claims, flowed onto `output` in
/// fixture order after its own runs there — one run per unclaimed stretch.
fn reflow(lamps: &FixtureLamps, own: &[Run], output: &NodeId) -> Vec<Run> {
    let mut claimed: Vec<(u32, u32)> = own.iter().map(|r| (r.start, r.start + r.count)).collect();
    claimed.sort_unstable();
    let mut cursor = own
        .iter()
        .filter(|r| &r.output == output)
        .map(Run::wire_end)
        .max()
        .unwrap_or(0);
    let mut out = Vec::new();
    let mut lamp = 0u32;
    for (start, end) in claimed.into_iter().chain([(lamps.total, lamps.total)]) {
        if start > lamp {
            out.push(Run {
                fixture: lamps.id.clone(),
                start: lamp,
                count: start - lamp,
                output: output.clone(),
                wire: cursor,
                reversed: false,
                rotation: 0,
            });
            cursor += start - lamp;
        }
        lamp = lamp.max(end);
    }
    out
}

impl PatchProblem {
    /// The problem in rustc style: what, why, what to do.
    pub fn notice(&self, p: &Project) -> Notice {
        let fixture = p.node_name(&self.fixture);
        let (what, why) = match &self.gone {
            Gone::Output => (
                format!(
                    "{fixture}'s rule {} patches to {}, which no longer exists",
                    self.rule + 1,
                    self.missing
                ),
                format!(
                    "its output was deleted, and a rule is never pointed somewhere else on its own; ⌘Z brings {} back",
                    self.missing
                ),
            ),
            Gone::Port(port) => (
                format!(
                    "{fixture}'s rule {} places lamps on port {port} of {}, which is gone",
                    self.rule + 1,
                    p.node_name(&self.missing)
                ),
                "its port was deleted, and a rule is never moved to another port on its own"
                    .to_string(),
            ),
            Gone::Object(o) => (
                format!(
                    "{fixture}'s rule {} places {o}, which no longer exists",
                    self.rule + 1
                ),
                "what it placed was deleted".to_string(),
            ),
        };
        Notice::refused(
            what,
            why,
            format!("until then {fixture}'s lamps fall to its next rule, or light nothing"),
        )
    }
}
