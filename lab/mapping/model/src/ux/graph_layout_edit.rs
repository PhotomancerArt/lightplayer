//! The Graph's layout verbs with their values (mapping rebuild M4 P07), and
//! the one function each runs through: [`Project::apply_layout_edit`],
//! reached from [`Project::apply_graph_edit`] like the patching verbs.
//!
//! Rename, repeat, unrepeat, expand, a count, a ring added or removed, a
//! centre lamp, a split, where a shape is drawn from, and delete; and the
//! numbers the inspector's rows set (P08): a ring's radius, a circle's way
//! round, a lattice's numbers and how its rows run, a fixture's size and
//! colour order, a port's lamps. Each is a
//! [`GraphEdit`] the offer (`project/graph/<verb>`), the inspector's rows
//! and the keys all make with [`Editor::edit`], one undo step. Every
//! mutation is `lpc_mapping::edit`'s, so the references an edit rewrites
//! (D6: a rename's overrides, an expand's `x@k` → `x-k`) are rewritten by
//! the model's rules, and what an edit can't keep is kept as gone (D12)
//! and said: "2 overrides on sector's copies are kept; they show as
//! problems".
//!
//! **Drawing stays a gesture.** Drawing a shape, dragging handles and
//! moving parts are pointer input the page reports; choosing the tool is
//! a key verb (`circle-tool`, `path-tool`).
//!
//! **Values as text** (`graph_edit.rs`): a part is `<fixture>:<part id>`,
//! a ring `<fixture>:<part id>/<key>`, a node by its id alone (`power`), a
//! fixture `<fixture>:`, a port `<output>:<key>`, a number of lamps or
//! copies a whole number, a radius, a spacing, an angle or a side a
//! number.
//!
//! A fixture's size and colour order and a port's lamps are the project's,
//! not its mappings': they are set on the fixture and the output; the rest
//! go through `lpc_mapping::edit`.

use lpc_mapping::edit::{self, DrawnFrom, LatticeEdit};
use lpc_mapping::part::{CircleSplit, PartId, PartKind, PathSplit, Shape};
use lpc_mapping::{GridRouting, RingDir};

use crate::domain::color_order::ColorOrder;
use crate::domain::fixture::Fixture;

use crate::domain::node::NodeId;
use crate::domain::notice::Notice;
use crate::domain::part_ref::{PartRef, TreeRef};
use crate::domain::project::Project;
use crate::ux::editor::Editor;
use crate::ux::graph_edit::{
    GraphEdit, PortChoice, RuleSubject, parse_lamps, parse_node, parse_port, parse_rule_subject,
};
use crate::ux::graph_verb::GraphVerb;

/// A length in the layout's units (a ring's radius). Equal when its bits
/// are, so an edit holding one is `Eq`.
#[derive(Debug, Clone, Copy)]
pub struct Units(pub f32);

impl PartialEq for Units {
    fn eq(&self, other: &Self) -> bool {
        self.0.to_bits() == other.0.to_bits()
    }
}

impl Eq for Units {}

/// A length on the canvas (a fixture's side). Equal when its bits are.
#[derive(Debug, Clone, Copy)]
pub struct Length(pub f64);

impl PartialEq for Length {
    fn eq(&self, other: &Self) -> bool {
        self.0.to_bits() == other.0.to_bits()
    }
}

impl Eq for Length {}

/// A lattice's numbers, as `set-lattice` takes them: each `None` stays.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LatticeValues {
    /// A grid's columns.
    pub columns: Option<u32>,
    /// A grid's rows.
    pub rows: Option<u32>,
    /// Lamp to lamp.
    pub spacing: Option<Units>,
    /// A filled polygon's lattice turned, in degrees.
    pub angle: Option<Units>,
}

impl LatticeValues {
    /// As `lpc_mapping::edit` takes it.
    fn edit(self) -> LatticeEdit {
        LatticeEdit {
            columns: self.columns,
            rows: self.rows,
            pitch: self.spacing.map(|u| u.0),
            angle_deg: self.angle.map(|u| u.0),
            routing: None,
        }
    }
}

/// Every way round, in the order `set-way-round` offers them.
pub const WAYS_ROUND: [RingDir; 2] = [RingDir::Cw, RingDir::Ccw];

/// A way round as a value.
pub fn way_round_name(dir: RingDir) -> &'static str {
    match dir {
        RingDir::Cw => "clockwise",
        RingDir::Ccw => "counter-clockwise",
    }
}

/// A way round in words.
pub fn way_round_label(dir: RingDir) -> &'static str {
    match dir {
        RingDir::Cw => "Clockwise",
        RingDir::Ccw => "Counter-clockwise",
    }
}

/// How a lattice's rows can run, in the order `set-routing` offers them.
pub const ROUTINGS: [GridRouting; 2] = [GridRouting::Snake, GridRouting::Raster];

/// How rows run, as a value.
pub fn routing_name(routing: GridRouting) -> &'static str {
    match routing {
        GridRouting::Snake => "back-and-forth",
        GridRouting::Raster => "same-way",
    }
}

/// How rows run, in words.
pub fn routing_label(routing: GridRouting) -> &'static str {
    match routing {
        GridRouting::Snake => "Back and forth (snake)",
        GridRouting::Raster => "All the same way",
    }
}

/// How a shape splits into segments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Split {
    /// One segment for the whole shape.
    One,
    /// A path: one per subpath.
    Subpaths,
    /// A circle: one per ring.
    Rings,
}

/// What `delete` takes: a node of the project, a part, or a ring.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeleteSubject {
    Node(NodeId),
    Part(PartRef),
    Ring(TreeRef),
}

impl Split {
    pub const ALL: [Split; 3] = [Split::One, Split::Subpaths, Split::Rings];

    /// As a value.
    pub fn name(self) -> &'static str {
        match self {
            Split::One => "one",
            Split::Subpaths => "subpaths",
            Split::Rings => "rings",
        }
    }

    /// In words.
    pub fn label(self) -> &'static str {
        match self {
            Split::One => "One segment",
            Split::Subpaths => "One per subpath",
            Split::Rings => "One per ring",
        }
    }
}

/// Every "drawn from", in the order `set-drawn-from` offers them.
pub const DRAWN_FROM: [DrawnFrom; 4] = [
    DrawnFrom::Outside,
    DrawnFrom::Inside,
    DrawnFrom::FirstPoint,
    DrawnFrom::LastPoint,
];

/// A "drawn from" as a value.
pub fn drawn_from_name(from: DrawnFrom) -> &'static str {
    match from {
        DrawnFrom::Outside => "outside",
        DrawnFrom::Inside => "inside",
        DrawnFrom::FirstPoint => "first-point",
        DrawnFrom::LastPoint => "last-point",
    }
}

/// A "drawn from" in words (names.html: "drawn from the inside").
pub fn drawn_from_label(from: DrawnFrom) -> &'static str {
    match from {
        DrawnFrom::Outside => "The outside (a circle)",
        DrawnFrom::Inside => "The inside (a circle)",
        DrawnFrom::FirstPoint => "Its first point (a path)",
        DrawnFrom::LastPoint => "Its last point (a path)",
    }
}

/// A part as a value: `<fixture>:<part id>`.
pub fn part_value(part: &PartRef) -> String {
    format!("{}:{}", part.fixture, part.part)
}

/// What `delete` takes, as a value.
pub fn delete_value(subject: &DeleteSubject) -> String {
    match subject {
        DeleteSubject::Node(id) => id.to_string(),
        DeleteSubject::Part(part) => part_value(part),
        DeleteSubject::Ring(ring) => format!("{}:{}", ring.fixture, ring.id),
    }
}

/// The layout edit `verb` makes with the values `arg` reads.
pub(crate) fn bind_layout(
    verb: GraphVerb,
    arg: &dyn Fn(&str) -> Option<String>,
) -> Result<GraphEdit, String> {
    let need = |name: &str| arg(name).ok_or_else(|| format!("choose a {name}"));
    let part = || parse_part(&need("subject")?);
    let count = || parse_count(&need("count")?);
    Ok(match verb {
        GraphVerb::Rename => GraphEdit::Rename {
            part: part()?,
            id: need("id")?.trim().to_string(),
        },
        GraphVerb::Repeat => GraphEdit::Repeat {
            part: part()?,
            count: count()?,
        },
        GraphVerb::Unrepeat => GraphEdit::Unrepeat { part: part()? },
        GraphVerb::Expand => GraphEdit::Expand { part: part()? },
        GraphVerb::SetCount => GraphEdit::SetCount {
            subject: parse_node(&need("subject")?)?,
            count: count()?,
        },
        GraphVerb::AddRing => GraphEdit::AddRing {
            circle: part()?,
            count: count()?,
            radius: parse_radius(&need("radius")?)?,
        },
        GraphVerb::AddCentreLamp => GraphEdit::AddCentreLamp { circle: part()? },
        GraphVerb::RemoveRing => GraphEdit::RemoveRing {
            ring: parse_ring(&need("subject")?)?,
        },
        GraphVerb::SetSplit => GraphEdit::SetSplit {
            part: part()?,
            split: {
                let text = need("split")?;
                Split::ALL
                    .into_iter()
                    .find(|s| s.name() == text)
                    .ok_or_else(|| format!("no split {text}"))?
            },
        },
        GraphVerb::SetDrawnFrom => GraphEdit::SetDrawnFrom {
            part: part()?,
            from: {
                let text = need("from")?;
                DRAWN_FROM
                    .into_iter()
                    .find(|f| drawn_from_name(*f) == text)
                    .ok_or_else(|| format!("nothing is drawn from {text}"))?
            },
        },
        GraphVerb::Delete => GraphEdit::Delete {
            subject: parse_delete(&need("subject")?)?,
        },
        GraphVerb::SetRadius => GraphEdit::SetRadius {
            ring: parse_ring(&need("subject")?)?,
            radius: parse_radius(&need("radius")?)?,
        },
        GraphVerb::SetWayRound => GraphEdit::SetWayRound {
            circle: part()?,
            dir: {
                let text = need("way")?;
                WAYS_ROUND
                    .into_iter()
                    .find(|d| way_round_name(*d) == text)
                    .ok_or_else(|| format!("no way round {text}"))?
            },
        },
        GraphVerb::SetLattice => {
            let given = |name: &str| arg(name).filter(|t| !t.trim().is_empty());
            let values = LatticeValues {
                columns: given("columns").map(|t| parse_count(&t)).transpose()?,
                rows: given("rows").map(|t| parse_count(&t)).transpose()?,
                spacing: given("spacing").map(|t| parse_spacing(&t)).transpose()?,
                angle: given("angle").map(|t| parse_angle(&t)).transpose()?,
            };
            if values == LatticeValues::default() {
                return Err("give columns, rows, a spacing or an angle".into());
            }
            GraphEdit::SetLattice {
                part: part()?,
                values,
            }
        }
        GraphVerb::SetRouting => GraphEdit::SetRouting {
            part: part()?,
            routing: {
                let text = need("run")?;
                ROUTINGS
                    .into_iter()
                    .find(|r| routing_name(*r) == text)
                    .ok_or_else(|| format!("rows don't run {text}"))?
            },
        },
        GraphVerb::SetFixtureSize => {
            let given = |name: &str| arg(name).filter(|t| !t.trim().is_empty());
            let width = given("width").map(|t| parse_side(&t)).transpose()?;
            let height = given("height").map(|t| parse_side(&t)).transpose()?;
            if width.is_none() && height.is_none() {
                return Err("give a width, a height or both".into());
            }
            GraphEdit::SetFixtureSize {
                fixture: fixture_of(&need("subject")?)?,
                width,
                height,
            }
        }
        GraphVerb::SetColourOrder => GraphEdit::SetColourOrder {
            fixture: fixture_of(&need("subject")?)?,
            order: {
                let text = need("order")?;
                ColorOrder::from_key(&text).ok_or_else(|| format!("no colour order {text}"))?
            },
        },
        GraphVerb::SetPortLamps => {
            let PortChoice::At { output, port } = parse_port(&need("subject")?)? else {
                return Err("a port is <output>:<port>".into());
            };
            GraphEdit::SetPortLamps {
                output,
                port,
                lamps: parse_count(&need("lamps")?)?,
            }
        }
        other => return Err(format!("{} isn't a layout verb", other.name())),
    })
}

impl Project {
    /// Make one layout edit, through `lpc_mapping::edit`. A refused edit
    /// says why.
    pub(crate) fn apply_layout_edit(&mut self, e: &GraphEdit) -> Result<(), String> {
        let said = |e: edit::EditError| e.to_string();
        match e {
            GraphEdit::Rename { part, id } => {
                let new = PartId::new(id).map_err(|why| format!("{id:?} can't be an id: {why}"))?;
                let m = self.mapping_mut(&part.fixture)?;
                edit::rename_part(m, &part.part, new).map_err(said)
            }
            GraphEdit::Repeat { part, count } => {
                let center = self.fixture_centre(&part.fixture)?;
                let m = self.mapping_mut(&part.fixture)?;
                edit::wrap_in_repeat(m, &part.part, *count, center)
                    .map(drop)
                    .map_err(said)
            }
            GraphEdit::Unrepeat { part } => {
                let m = self.mapping_mut(&part.fixture)?;
                edit::unrepeat(m, &part.part).map(drop).map_err(said)
            }
            GraphEdit::Expand { part } => {
                let m = self.mapping_mut(&part.fixture)?;
                edit::expand_repeat(m, &part.part).map(drop).map_err(said)
            }
            GraphEdit::SetCount { subject, count } => {
                // A copy's count is its part's (D11).
                let node = subject.original();
                let repeat = matches!(
                    self.part(&node.part()).map(|p| &p.kind),
                    Some(PartKind::Repeat(_))
                );
                let m = self.mapping_mut(&node.fixture)?;
                if repeat && node.id.key.is_none() {
                    edit::set_repeat_count(m, &node.id.part, *count).map_err(said)
                } else {
                    edit::set_lamp_count(m, &node.id.part, node.id.key.as_ref(), *count)
                        .map_err(said)
                }
            }
            GraphEdit::AddRing {
                circle,
                count,
                radius,
            } => {
                let m = self.mapping_mut(&circle.fixture)?;
                edit::add_ring(m, &circle.part, radius.0, *count)
                    .map(drop)
                    .map_err(said)
            }
            GraphEdit::AddCentreLamp { circle } => {
                if self
                    .circle(circle)
                    .is_some_and(|c| c.rings.iter().any(|r| r.is_centre()))
                {
                    return Err(format!("{} has its centre lamp already", circle.part));
                }
                let m = self.mapping_mut(&circle.fixture)?;
                edit::add_ring(m, &circle.part, 0.0, 1)
                    .map(drop)
                    .map_err(said)
            }
            GraphEdit::RemoveRing { ring } => self.remove_ring_of(ring),
            GraphEdit::SetSplit { part, split } => {
                let kind = self.part(part).map(|p| &p.kind);
                let (path, circle) = (kind.is_some_and(is_path), kind.is_some_and(is_circle));
                let m = self.mapping_mut(&part.fixture)?;
                match (path, circle, split) {
                    (true, _, Split::One) => {
                        edit::set_path_split(m, &part.part, PathSplit::One).map_err(said)
                    }
                    (true, _, Split::Subpaths) => {
                        edit::set_path_split(m, &part.part, PathSplit::Subpaths).map_err(said)
                    }
                    (_, true, Split::One) => {
                        edit::set_circle_split(m, &part.part, CircleSplit::One).map_err(said)
                    }
                    (_, true, Split::Rings) => {
                        edit::set_circle_split(m, &part.part, CircleSplit::Rings).map_err(said)
                    }
                    _ => Err(format!(
                        "{}: a path splits into subpaths, a circle into rings",
                        part.part
                    )),
                }
            }
            GraphEdit::SetDrawnFrom { part, from } => {
                let m = self.mapping_mut(&part.fixture)?;
                edit::set_drawn_from(m, &part.part, *from).map_err(said)
            }
            GraphEdit::Delete { subject } => match subject {
                DeleteSubject::Node(id) => self
                    .remove_node(id)
                    .map(drop)
                    .ok_or_else(|| format!("there is no node {id}")),
                DeleteSubject::Part(part) => {
                    let m = self.mapping_mut(&part.fixture)?;
                    edit::remove_part(m, &part.part).map(drop).map_err(said)
                }
                DeleteSubject::Ring(ring) => self.remove_ring_of(ring),
            },
            GraphEdit::SetRadius { ring, radius } => {
                let key = ring
                    .id
                    .key
                    .clone()
                    .filter(|_| ring.id.copies.is_empty())
                    .ok_or_else(|| format!("{} isn't a ring", ring.id))?;
                let m = self.mapping_mut(&ring.fixture)?;
                edit::set_ring_radius(m, &ring.id.part, &key, radius.0).map_err(said)
            }
            GraphEdit::SetWayRound { circle, dir } => {
                let m = self.mapping_mut(&circle.fixture)?;
                edit::set_circle_dir(m, &circle.part, *dir).map_err(said)
            }
            GraphEdit::SetLattice { part, values } => {
                let m = self.mapping_mut(&part.fixture)?;
                edit::set_lattice(m, &part.part, &values.edit()).map_err(said)
            }
            GraphEdit::SetRouting { part, routing } => {
                let m = self.mapping_mut(&part.fixture)?;
                let change = LatticeEdit {
                    routing: Some(*routing),
                    ..LatticeEdit::default()
                };
                edit::set_lattice(m, &part.part, &change).map_err(said)
            }
            GraphEdit::SetFixtureSize {
                fixture,
                width,
                height,
            } => {
                let f = self.editable_fixture(fixture)?;
                f.size.x = width.map_or(f.size.x, |w| w.0);
                f.size.y = height.map_or(f.size.y, |h| h.0);
                Ok(())
            }
            GraphEdit::SetColourOrder { fixture, order } => {
                self.editable_fixture(fixture)?.color_order = *order;
                Ok(())
            }
            GraphEdit::SetPortLamps {
                output,
                port,
                lamps,
            } => {
                let words = self.output_words(output);
                let p = self
                    .output_mut(output)
                    .and_then(|o| o.port_mut(port))
                    .ok_or_else(|| format!("{words} has no port {port}"))?;
                p.pixels = Some(*lamps);
                Ok(())
            }
            other => Err(format!("{} isn't a layout edit", other.verb().name())),
        }
    }

    /// How many overrides name a node that isn't there: what an edit
    /// left kept and gone (D12).
    pub fn gone_overrides(&self) -> usize {
        let placed = self.placed();
        self.fixtures()
            .iter()
            .filter_map(|n| self.fixture(&n.id).map(|f| (&n.id, f)))
            .map(|(id, f)| {
                f.mapping
                    .overrides
                    .keys()
                    .filter(|key| placed.node(&TreeRef::new(id, (*key).clone())).is_none())
                    .count()
            })
            .sum()
    }

    /// A ring taken from its circle; never the last (delete the circle).
    fn remove_ring_of(&mut self, ring: &TreeRef) -> Result<(), String> {
        let circle = ring.part();
        let key = ring
            .id
            .key
            .clone()
            .filter(|_| ring.id.copies.is_empty())
            .ok_or_else(|| format!("{} isn't a ring", ring.id))?;
        if self.circle(&circle).is_some_and(|c| c.rings.len() <= 1) {
            return Err(format!(
                "it is {}'s last ring: delete {} itself",
                circle.part, circle.part
            ));
        }
        let m = self.mapping_mut(&ring.fixture)?;
        edit::remove_ring(m, &circle.part, &key)
            .map(drop)
            .map_err(|e| e.to_string())
    }

    /// A fixture the Graph can edit: one it holds whole.
    fn editable_fixture(&mut self, fixture: &NodeId) -> Result<&mut Fixture, String> {
        let f = self
            .fixture_mut(fixture)
            .ok_or_else(|| format!("there is no fixture {fixture}"))?;
        if f.unsupported.is_some() {
            return Err(format!(
                "{fixture} opens as its box alone here: edit it elsewhere"
            ));
        }
        Ok(f)
    }

    /// The middle of a fixture's box, in its mapping's coordinates: where
    /// a new repeat turns its copies about.
    fn fixture_centre(&self, fixture: &NodeId) -> Result<[f32; 2], String> {
        let f = self
            .fixture(fixture)
            .ok_or_else(|| format!("there is no fixture {fixture}"))?;
        let [x, y] = f.mapping.canvas.map_or([0.0, 0.0], |c| [c[0], c[1]]);
        Ok([(x + f.size.x as f32) / 2.0, (y + f.size.y as f32) / 2.0])
    }
}

impl Editor {
    /// After layout edits: a renamed part stays selected under its new
    /// id, and what an edit left kept and gone is said (D12).
    pub(crate) fn after_layout_edits(&mut self, edits: &[GraphEdit], gone_before: usize) {
        for e in edits {
            if let GraphEdit::Rename { part, id } = e
                && let Ok(new) = PartId::new(id)
            {
                let renamed = PartRef::new(&part.fixture, &new);
                for t in &mut self.selection {
                    if matches!(t, crate::domain::target::Target::Part(p) if p == part) {
                        *t = crate::domain::target::Target::Part(renamed.clone());
                    }
                }
            }
        }
        let kept = self.project.gone_overrides().saturating_sub(gone_before);
        let [last] = edits else {
            return;
        };
        if kept == 0 || !GraphVerb::LAYOUT.contains(&last.verb()) {
            return;
        }
        let (done, on) = match last {
            GraphEdit::SetCount { subject, count } => (
                format!("{} makes {count}", subject.id.part),
                format!("{}'s copies", subject.id.part),
            ),
            GraphEdit::Unrepeat { part } => (
                format!("Unrepeated {}", part.part),
                format!("{}'s copies", part.part),
            ),
            GraphEdit::RemoveRing { ring }
            | GraphEdit::Delete {
                subject: DeleteSubject::Ring(ring),
            } => (format!("Removed {}", ring.id), "it".to_string()),
            GraphEdit::Delete {
                subject: DeleteSubject::Part(part),
            } => (format!("Deleted {}", part.part), "it".to_string()),
            other => (
                format!("{} done", other.verb().label()),
                "what went".to_string(),
            ),
        };
        let (s, are, they, show, problems) = if kept == 1 {
            ("", "is", "it", "shows", "a problem")
        } else {
            ("s", "are", "they", "show", "problems")
        };
        self.notice = Some(Notice {
            why: Some(format!(
                "{kept} override{s} on {on} {are} kept; {they} {show} as {problems}"
            )),
            help: Some("nothing was pointed anywhere else; ⌘Z brings it back".into()),
            ..Notice::info(done)
        });
    }
}

fn is_path(kind: &PartKind) -> bool {
    matches!(kind, PartKind::Shape(Shape::Path(_)))
}

fn is_circle(kind: &PartKind) -> bool {
    matches!(kind, PartKind::Shape(Shape::Circle(_)))
}

/// A part: `<fixture>:<part id>`.
fn parse_part(text: &str) -> Result<PartRef, String> {
    let node = parse_node(text)?;
    if !node.is_part_node() {
        return Err(format!(
            "{} is one node of {}, not a part",
            node.id, node.id.part
        ));
    }
    Ok(node.part())
}

/// A ring: `<fixture>:<part id>/<key>`.
fn parse_ring(text: &str) -> Result<TreeRef, String> {
    let node = parse_node(text)?;
    if node.id.key.is_none() || !node.id.copies.is_empty() {
        return Err(format!("{} isn't a ring", node.id));
    }
    Ok(node)
}

/// What `delete` takes: a part or a ring (`<fixture>:…`), else a node.
fn parse_delete(text: &str) -> Result<DeleteSubject, String> {
    if !text.contains(':') {
        return Ok(DeleteSubject::Node(NodeId::new(text)));
    }
    let node = parse_node(text)?;
    if node.is_part_node() {
        return Ok(DeleteSubject::Part(node.part()));
    }
    if !node.id.copies.is_empty() {
        return Err(format!(
            "{} is a copy, made by its repeat's count: set the count, or delete the repeat",
            node.id
        ));
    }
    Ok(DeleteSubject::Ring(node))
}

/// A whole number of copies or lamps, at least one.
fn parse_count(text: &str) -> Result<u32, String> {
    let n = parse_lamps(text).map_err(|_| format!("{text} isn't a whole number"))?;
    if n == 0 {
        return Err("at least one".to_string());
    }
    Ok(n)
}

/// A fixture: `<fixture>:`.
fn fixture_of(text: &str) -> Result<NodeId, String> {
    match parse_rule_subject(text)? {
        RuleSubject::Fixture(f) => Ok(f),
        RuleSubject::Part(p) => Err(format!("{} is a part, not a fixture", p.part)),
    }
}

/// A lamp-to-lamp spacing, more than 0.
fn parse_spacing(text: &str) -> Result<Units, String> {
    let v: f32 = text
        .trim()
        .parse()
        .map_err(|_| format!("{text} isn't a spacing"))?;
    if !v.is_finite() || v <= 0.0 {
        return Err(format!("a spacing is more than 0, not {text}"));
    }
    Ok(Units(v))
}

/// An angle, in degrees.
fn parse_angle(text: &str) -> Result<Units, String> {
    let v: f32 = text
        .trim()
        .parse()
        .map_err(|_| format!("{text} isn't a number of degrees"))?;
    if !v.is_finite() {
        return Err(format!("{text} isn't a number of degrees"));
    }
    Ok(Units(v))
}

/// A fixture's side, at least [`Fixture::MIN_SIDE`].
fn parse_side(text: &str) -> Result<Length, String> {
    let v: f64 = text
        .trim()
        .parse()
        .map_err(|_| format!("{text} isn't a length"))?;
    if !v.is_finite() || v < Fixture::MIN_SIDE {
        return Err(format!(
            "a fixture's side is at least {}, not {text}",
            Fixture::MIN_SIDE
        ));
    }
    Ok(Length(v))
}

fn parse_radius(text: &str) -> Result<Units, String> {
    let r: f32 = text
        .trim()
        .parse()
        .map_err(|_| format!("{text} isn't a radius"))?;
    if !r.is_finite() || r <= 0.0 {
        return Err(format!(
            "a ring's radius is more than 0, not {text} (Add centre lamp puts one at the centre)"
        ));
    }
    Ok(Units(r))
}
