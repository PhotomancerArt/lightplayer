//! The Graph's verbs that take values, with their arguments, and the one
//! function each runs through (lean Q77): the patching verbs here, the
//! layout verbs in `graph_layout_edit.rs`.
//!
//! A [`GraphEdit`] is a [`GraphVerb`] that takes values, bound: "set ring
//! r3's port to D1 of main". Whoever presses it — Studio's offer
//! (`project/graph/set-port` with its `subject` and `port`), an inspector
//! row, a key — hands one or more of these to [`Editor::edit`], which runs
//! each through [`Project::apply_graph_edit`] as one undo step. The offer,
//! the row and the key are three doors to the same function; none of them
//! edits the project another way.
//!
//! Every mutation goes through `lpc_mapping::edit` (`set_start`,
//! `set_flow`, `set_override_field`, `clear_override_field`): nothing here
//! pokes a mapping's fields.
//!
//! **Values as text.** [`GraphEdit::bind`] reads the values an offer's
//! press carries, by parameter name, in the forms [`crate::VerbChoice`]s
//! write: a node is `<fixture>:<tree id>` (tree ids hold `/` and `@`, so
//! they never go into an offer's path), a part `<fixture>:<part id>`, a
//! fixture `<fixture>:`, a port `<output>:<key>`, `next-port` or
//! `stays-here`, a number of lamps as a whole number.

use lpc_mapping::edit::DrawnFrom;
use lpc_mapping::edit::{self, OverrideField, OverrideFieldKind};
use lpc_mapping::part::TreeId;
use lpc_mapping::patching::{
    Amount, ChannelOp, Direction, Flow, OutputOp, PatchTarget, PortKey, PortOp, Start,
};
use lpc_mapping::{GridRouting, RingDir};

use crate::domain::color_order::ColorOrder;
use crate::domain::node::NodeId;
use crate::domain::part_ref::{PartRef, TreeRef};
use crate::domain::project::Project;
use crate::ux::editor::Editor;
use crate::ux::graph_layout_edit::{
    DeleteSubject, LatticeValues, Length, Split, Units, bind_layout,
};
use crate::ux::graph_verb::GraphVerb;

/// One patching verb, bound to what it acts on and the values it takes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphEdit {
    /// The output a segment's start names (its port and lamp stay as they
    /// are said).
    SetOutput { subject: TreeRef, output: NodeId },
    /// A segment's port: at a port from its first lamp, the next port from
    /// its first lamp (both put it on the line), or stays here (an explicit
    /// keep).
    SetPort { subject: TreeRef, port: PortChoice },
    /// Where on its port a segment starts. `None` is "stays here": the
    /// cursor stays where it is, whatever the flow says.
    SetStartsAt {
        subject: TreeRef,
        at: Option<Amount>,
    },
    /// Which way a node's lamps run on the line.
    SetDirection {
        subject: TreeRef,
        direction: Direction,
    },
    /// A wrapping shift of a segment's lamps, in lamps.
    SetOffset { subject: TreeRef, lamps: u32 },
    /// Off the line: no pixel shows it, and what follows takes its place.
    TurnOff { subject: TreeRef },
    /// On the line again.
    TurnOn { subject: TreeRef },
    /// Drop what is set here, so the node takes what its flow gives it.
    BackToTheFlow { subject: TreeRef, field: BackField },
    /// How a fixture's or a container's children follow each other.
    SetFlow { subject: RuleSubject, flow: Flow },
    /// Where a fixture starts: a port of a named output, from a lamp.
    SetFixtureStart {
        fixture: NodeId,
        output: NodeId,
        port: String,
        lamps: u32,
    },
    /// Where a part starts, in every copy: follows, a port from a lamp, or
    /// the next port.
    SetPartStart { part: PartRef, start: PartStart },
    /// Take away an override whose node isn't there any more (a copy
    /// counted away, a ring removed).
    RemoveOverride { node: TreeRef },
    // ---- layout (`graph_layout_edit.rs`) ------------------------------
    /// Give a part the id `id`; every override on it follows.
    Rename { part: PartRef, id: String },
    /// Put a part in a new repeat of `count` copies about its fixture's
    /// centre.
    Repeat { part: PartRef, count: u32 },
    /// Keep only the part a repeat repeats.
    Unrepeat { part: PartRef },
    /// Every copy of a repeat a real part.
    Expand { part: PartRef },
    /// A repeat's copies, or a path's, a polygon's or a ring's lamps.
    SetCount { subject: TreeRef, count: u32 },
    /// A ring of `count` lamps at `radius`.
    AddRing {
        circle: PartRef,
        count: u32,
        radius: Units,
    },
    /// A one-lamp ring at the centre.
    AddCentreLamp { circle: PartRef },
    /// A ring taken from its circle (its overrides kept, D12).
    RemoveRing { ring: TreeRef },
    /// One segment, or one per subpath or ring.
    SetSplit { part: PartRef, split: Split },
    /// Where a circle or a path is drawn from.
    SetDrawnFrom { part: PartRef, from: DrawnFrom },
    /// A node, a part or a ring deleted (overrides kept, D12).
    Delete { subject: DeleteSubject },
    /// How far a ring is from its circle's centre.
    SetRadius { ring: TreeRef, radius: Units },
    /// Which way round a circle's rings run.
    SetWayRound { circle: PartRef, dir: RingDir },
    /// A grid's or a filled polygon's numbers; each one not given stays.
    SetLattice {
        part: PartRef,
        values: LatticeValues,
    },
    /// A grid's or a filled polygon's rows back and forth, or all the same
    /// way.
    SetRouting { part: PartRef, routing: GridRouting },
    /// A fixture's box; a side not given stays.
    SetFixtureSize {
        fixture: NodeId,
        width: Option<Length>,
        height: Option<Length>,
    },
    /// Which byte of each of a fixture's lamps is red, green and blue.
    SetColourOrder { fixture: NodeId, order: ColorOrder },
    /// How many lamps the strip on a port has.
    SetPortLamps {
        output: NodeId,
        port: String,
        lamps: u32,
    },
}

/// A port, as `set-port` takes it (Q26).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PortChoice {
    /// This port of this output, from its first lamp.
    At { output: NodeId, port: String },
    /// The next port, from its first lamp.
    NextPort,
    /// The port the cursor is on, whatever the flow says.
    StaysHere,
}

/// Which of what is set here goes back to the flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackField {
    /// Everything set here, Off included: the override goes.
    All,
    /// Output, port and lamp together (the U key).
    Start,
    Output,
    Port,
    StartsAt,
    Direction,
    Offset,
}

/// What a start and a flow are on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuleSubject {
    Fixture(NodeId),
    Part(PartRef),
}

/// A part's start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PartStart {
    Follows,
    At {
        output: NodeId,
        port: String,
        lamps: u32,
    },
    NextPort,
}

impl GraphEdit {
    /// The verb it is.
    pub fn verb(&self) -> GraphVerb {
        match self {
            GraphEdit::SetOutput { .. } => GraphVerb::SetOutput,
            GraphEdit::SetPort { .. } => GraphVerb::SetPort,
            GraphEdit::SetStartsAt { .. } => GraphVerb::SetStartsAt,
            GraphEdit::SetDirection { .. } => GraphVerb::SetDirection,
            GraphEdit::SetOffset { .. } => GraphVerb::SetOffset,
            GraphEdit::TurnOff { .. } => GraphVerb::TurnOff,
            GraphEdit::TurnOn { .. } => GraphVerb::TurnOn,
            GraphEdit::BackToTheFlow { .. } => GraphVerb::BackToTheFlow,
            GraphEdit::SetFlow { .. } => GraphVerb::SetFlow,
            GraphEdit::SetFixtureStart { .. } => GraphVerb::SetFixtureStart,
            GraphEdit::SetPartStart { .. } => GraphVerb::SetPartStart,
            GraphEdit::RemoveOverride { .. } => GraphVerb::RemoveOverride,
            GraphEdit::Rename { .. } => GraphVerb::Rename,
            GraphEdit::Repeat { .. } => GraphVerb::Repeat,
            GraphEdit::Unrepeat { .. } => GraphVerb::Unrepeat,
            GraphEdit::Expand { .. } => GraphVerb::Expand,
            GraphEdit::SetCount { .. } => GraphVerb::SetCount,
            GraphEdit::AddRing { .. } => GraphVerb::AddRing,
            GraphEdit::AddCentreLamp { .. } => GraphVerb::AddCentreLamp,
            GraphEdit::RemoveRing { .. } => GraphVerb::RemoveRing,
            GraphEdit::SetSplit { .. } => GraphVerb::SetSplit,
            GraphEdit::SetDrawnFrom { .. } => GraphVerb::SetDrawnFrom,
            GraphEdit::Delete { .. } => GraphVerb::Delete,
            GraphEdit::SetRadius { .. } => GraphVerb::SetRadius,
            GraphEdit::SetWayRound { .. } => GraphVerb::SetWayRound,
            GraphEdit::SetLattice { .. } => GraphVerb::SetLattice,
            GraphEdit::SetRouting { .. } => GraphVerb::SetRouting,
            GraphEdit::SetFixtureSize { .. } => GraphVerb::SetFixtureSize,
            GraphEdit::SetColourOrder { .. } => GraphVerb::SetColourOrder,
            GraphEdit::SetPortLamps { .. } => GraphVerb::SetPortLamps,
        }
    }

    /// The edit `verb` makes with the values `arg` reads by parameter name
    /// (module docs for their forms), or why those values don't make one.
    pub fn bind(verb: GraphVerb, arg: &dyn Fn(&str) -> Option<String>) -> Result<Self, String> {
        let need = |name: &str| arg(name).ok_or_else(|| format!("choose a {name}"));
        let node = || parse_node(&need("subject")?);
        let lamps = || parse_lamps(&need("lamps")?);
        Ok(match verb {
            GraphVerb::SetOutput => GraphEdit::SetOutput {
                subject: node()?,
                output: NodeId::new(need("output")?),
            },
            GraphVerb::SetPort => GraphEdit::SetPort {
                subject: node()?,
                port: parse_port(&need("port")?)?,
            },
            GraphVerb::SetStartsAt => GraphEdit::SetStartsAt {
                subject: node()?,
                at: Some(Amount::lamps(lamps()?)),
            },
            GraphVerb::SetDirection => GraphEdit::SetDirection {
                subject: node()?,
                direction: match need("direction")?.as_str() {
                    "forward" => Direction::Forward,
                    "reverse" => Direction::Reverse,
                    other => return Err(format!("no direction {other}")),
                },
            },
            GraphVerb::SetOffset => GraphEdit::SetOffset {
                subject: node()?,
                lamps: lamps()?,
            },
            GraphVerb::TurnOff => GraphEdit::TurnOff { subject: node()? },
            GraphVerb::TurnOn => GraphEdit::TurnOn { subject: node()? },
            GraphVerb::BackToTheFlow => GraphEdit::BackToTheFlow {
                subject: node()?,
                field: parse_back_field(&need("field")?)?,
            },
            GraphVerb::SetFlow => GraphEdit::SetFlow {
                subject: parse_rule_subject(&need("subject")?)?,
                flow: parse_flow(&need("flow")?)?,
            },
            GraphVerb::SetFixtureStart => {
                let RuleSubject::Fixture(fixture) = parse_rule_subject(&need("subject")?)? else {
                    return Err("a fixture's start is set on a fixture".into());
                };
                let PortChoice::At { output, port } = parse_port(&need("port")?)? else {
                    return Err("a fixture starts at a port".into());
                };
                GraphEdit::SetFixtureStart {
                    fixture,
                    output,
                    port,
                    lamps: lamps()?,
                }
            }
            GraphVerb::SetPartStart => {
                let RuleSubject::Part(part) = parse_rule_subject(&need("subject")?)? else {
                    return Err("a part's start is set on a part".into());
                };
                let start = match need("start")?.as_str() {
                    "follows" => PartStart::Follows,
                    "next-port" => PartStart::NextPort,
                    other => match parse_port(other)? {
                        PortChoice::At { output, port } => PartStart::At {
                            output,
                            port,
                            lamps: arg("lamps").map_or(Ok(0), |l| parse_lamps(&l))?,
                        },
                        _ => return Err(format!("no start {other}")),
                    },
                };
                GraphEdit::SetPartStart { part, start }
            }
            GraphVerb::RemoveOverride => GraphEdit::RemoveOverride { node: node()? },
            layout if GraphVerb::LAYOUT.contains(&layout) => return bind_layout(layout, arg),
            other => return Err(format!("{} takes no values", other.name())),
        })
    }
}

impl Project {
    /// Make one edit that takes values: the one function every door to the
    /// verb runs. A refused edit says why; [`Editor::edit`] puts everything
    /// back when one is refused. Choosing a port or an output with no name
    /// names it (lean Q96, `output_naming.rs`).
    pub fn apply_graph_edit(&mut self, e: &GraphEdit) -> Result<(), String> {
        match e {
            GraphEdit::SetOutput { subject, output } => {
                let name = self.name_output(output)?;
                self.put_field(subject, OverrideField::Output(OutputOp::Set(name)))
            }
            GraphEdit::SetPort { subject, port } => match port {
                PortChoice::At { output, port } => {
                    let start = self.start_at_port(output, port, 0)?;
                    self.put_node_start(subject, start)?;
                    self.put_field(subject, OverrideField::Off(false))
                }
                PortChoice::NextPort => {
                    self.put_node_start(subject, Start::next_port())?;
                    self.put_field(subject, OverrideField::Off(false))
                }
                PortChoice::StaysHere => {
                    self.put_field(subject, OverrideField::Output(OutputOp::Keep))?;
                    self.put_field(subject, OverrideField::Port(PortOp::Keep))
                }
            },
            GraphEdit::SetStartsAt { subject, at } => {
                let op = match at {
                    Some(amount) => ChannelOp::Set(*amount),
                    None => ChannelOp::Keep,
                };
                self.put_field(subject, OverrideField::Channel(op))
            }
            GraphEdit::SetDirection { subject, direction } => {
                self.put_field(subject, OverrideField::Direction(*direction))
            }
            GraphEdit::SetOffset { subject, lamps } => {
                self.put_field(subject, OverrideField::Offset(*lamps))
            }
            GraphEdit::TurnOff { subject } => self.put_field(subject, OverrideField::Off(true)),
            GraphEdit::TurnOn { subject } => self.put_field(subject, OverrideField::Off(false)),
            GraphEdit::BackToTheFlow { subject, field } => {
                for kind in field.kinds() {
                    self.drop_field(subject, *kind);
                }
                Ok(())
            }
            GraphEdit::SetFlow { subject, flow } => {
                let (fixture, target) = subject.target();
                let m = self.mapping_mut(&fixture)?;
                edit::set_flow(m, target, flow.clone()).map_err(|e| e.to_string())
            }
            GraphEdit::SetFixtureStart {
                fixture,
                output,
                port,
                lamps,
            } => {
                let start = self.start_at_port(output, port, *lamps)?;
                let m = self.mapping_mut(fixture)?;
                edit::set_start(m, PatchTarget::Fixture, start).map_err(|e| e.to_string())
            }
            GraphEdit::SetPartStart { part, start } => {
                let start = match start {
                    PartStart::Follows => Start::KEEP,
                    PartStart::NextPort => Start::next_port(),
                    PartStart::At {
                        output,
                        port,
                        lamps,
                    } => self.start_at_port(output, port, *lamps)?,
                };
                let m = self.mapping_mut(&part.fixture)?;
                edit::set_start(m, PatchTarget::Part(part.part.clone()), start)
                    .map_err(|e| e.to_string())
            }
            GraphEdit::RemoveOverride { node } => {
                if self.placed().node(node).is_some() {
                    return Err(format!("{} is there: its override isn't gone", node.id));
                }
                if self.override_of(node).is_none() {
                    return Err(format!("nothing is set on {}", node.id));
                }
                for kind in BackField::All.kinds() {
                    self.drop_field(node, *kind);
                }
                Ok(())
            }
            layout => self.apply_layout_edit(layout),
        }
    }

    /// Turning a segment by `lamps` (the K and L keys, the Offset row's
    /// − and +), as the verb it is: its new offset, or back to the flow at
    /// none. `None` for a segment that isn't there.
    pub fn turn_edit(&self, segment: &TreeRef, lamps: i64) -> Option<GraphEdit> {
        Some(match self.turned_offset(segment, lamps)? {
            0 => GraphEdit::BackToTheFlow {
                subject: segment.clone(),
                field: BackField::Offset,
            },
            lamps => GraphEdit::SetOffset {
                subject: segment.clone(),
                lamps,
            },
        })
    }

    /// Moving a segment's start `pixels` along its port (the [ and ]
    /// keys), as verbs: its port where it lands now, then the lamp it moves
    /// to. It can't leave the port: past either end is refused.
    pub fn shift_edits(
        &self,
        segment: &TreeRef,
        pixels: i64,
    ) -> Result<Vec<GraphEdit>, crate::domain::patch_edits::PortEdge> {
        use crate::domain::patch_edits::PortEdge;
        let w = self.effective_patch().walked(segment);
        let Some(at) = w.and_then(|w| w.landed.clone()) else {
            return Err(PortEdge::Start);
        };
        if !self.effective_patch().lands(segment) {
            return Err(PortEdge::Start);
        }
        let (Some(output), Some(port)) = (at.output, at.port) else {
            return Err(PortEdge::Start);
        };
        let pixel = at.channel / self.channels_per_lamp_of(segment);
        let to = pixel as i64 + pixels;
        let counted = self
            .output(&output)
            .and_then(|o| o.port(&port))
            .and_then(|p| p.pixels);
        if to < 0 {
            return Err(PortEdge::Start);
        }
        if counted.is_some_and(|n| to >= n as i64) {
            return Err(PortEdge::End);
        }
        Ok(vec![
            GraphEdit::SetPort {
                subject: segment.clone(),
                port: PortChoice::At { output, port },
            },
            GraphEdit::SetStartsAt {
                subject: segment.clone(),
                at: Some(Amount::lamps(to as u32)),
            },
        ])
    }

    /// The start ops for `port` of `output`, from `lamps`, naming the
    /// output when it has no name to say it by.
    fn start_at_port(&mut self, output: &NodeId, port: &str, lamps: u32) -> Result<Start, String> {
        if self.output(output).and_then(|o| o.port(port)).is_none() {
            return Err(format!("{} has no port {port}", self.output_words(output)));
        }
        let key: u32 = port
            .parse()
            .map_err(|_| format!("{port} isn't a port of {}", self.output_words(output)))?;
        let name = self.name_output(output)?;
        Ok(Start {
            output: OutputOp::Set(name),
            port: PortOp::set(key),
            channel: ChannelOp::Set(Amount::lamps(lamps)),
        })
    }

    pub(crate) fn mapping_mut(
        &mut self,
        fixture: &NodeId,
    ) -> Result<&mut lpc_mapping::part::Mapping, String> {
        self.fixture_mut(fixture)
            .map(|f| &mut f.mapping)
            .ok_or_else(|| format!("there is no fixture {fixture}"))
    }

    /// One node's start (its override's output, port and lamp).
    fn put_node_start(&mut self, node: &TreeRef, start: Start) -> Result<(), String> {
        let m = self.mapping_mut(&node.fixture)?;
        edit::set_start(m, PatchTarget::Node(node.id.clone()), start).map_err(|e| e.to_string())
    }

    fn put_field(&mut self, node: &TreeRef, field: OverrideField) -> Result<(), String> {
        let m = self.mapping_mut(&node.fixture)?;
        edit::set_override_field(m, &node.id, field).map_err(|e| e.to_string())
    }

    fn drop_field(&mut self, node: &TreeRef, kind: OverrideFieldKind) {
        if let Ok(m) = self.mapping_mut(&node.fixture) {
            // Clearing never refuses.
            let _ = edit::clear_override_field(m, &node.id, kind);
        }
    }
}

impl Editor {
    /// Make `edits` as one undo step: each through
    /// [`Project::apply_graph_edit`], all or nothing. Refused, nothing
    /// changes and the notice says why.
    pub fn edit(&mut self, edits: &[GraphEdit]) -> Result<(), String> {
        if edits.is_empty() {
            return Ok(());
        }
        let before = self.project.clone();
        let gone_before = self.project.gone_overrides();
        self.checkpoint();
        for e in edits {
            if let Err(why) = self.project.apply_graph_edit(e) {
                self.project = before;
                self.undo_pop();
                self.notice = Some(crate::domain::notice::Notice::refused(
                    format!("{} refused", e.verb().label()),
                    why.clone(),
                    "",
                ));
                return Err(why);
            }
        }
        self.after_layout_edits(edits, gone_before);
        self.after_edit();
        Ok(())
    }
}

impl BackField {
    /// The override's fields it clears.
    pub fn kinds(self) -> &'static [OverrideFieldKind] {
        use OverrideFieldKind as K;
        match self {
            BackField::All => &[
                K::Output,
                K::Port,
                K::Channel,
                K::Off,
                K::Direction,
                K::Offset,
            ],
            BackField::Start => &[K::Output, K::Port, K::Channel],
            BackField::Output => &[K::Output],
            BackField::Port => &[K::Port],
            BackField::StartsAt => &[K::Channel],
            BackField::Direction => &[K::Direction],
            BackField::Offset => &[K::Offset],
        }
    }

    /// Every field, in the order `back-to-the-flow` offers them.
    pub const ALL: [BackField; 7] = [
        BackField::All,
        BackField::Start,
        BackField::Output,
        BackField::Port,
        BackField::StartsAt,
        BackField::Direction,
        BackField::Offset,
    ];

    /// As a value: `all`, `start`, `starts-at`.
    pub fn name(self) -> &'static str {
        match self {
            BackField::All => "all",
            BackField::Start => "start",
            BackField::Output => "output",
            BackField::Port => "port",
            BackField::StartsAt => "starts-at",
            BackField::Direction => "direction",
            BackField::Offset => "offset",
        }
    }

    /// In words.
    pub fn label(self) -> &'static str {
        match self {
            BackField::All => "Everything set here",
            BackField::Start => "Output, port and lamp",
            BackField::Output => "Output",
            BackField::Port => "Port",
            BackField::StartsAt => "Starts at",
            BackField::Direction => "Direction",
            BackField::Offset => "Offset",
        }
    }
}

impl RuleSubject {
    /// The fixture, and what in it the start or flow is on.
    pub fn target(&self) -> (NodeId, PatchTarget) {
        match self {
            RuleSubject::Fixture(f) => (f.clone(), PatchTarget::Fixture),
            RuleSubject::Part(p) => (p.fixture.clone(), PatchTarget::Part(p.part.clone())),
        }
    }

    /// As a value: `<fixture>:` or `<fixture>:<part>`.
    pub fn value(&self) -> String {
        match self {
            RuleSubject::Fixture(f) => format!("{f}:"),
            RuleSubject::Part(p) => format!("{}:{}", p.fixture, p.part),
        }
    }
}

/// A node as a value: `<fixture>:<tree id>`.
pub fn node_value(node: &TreeRef) -> String {
    format!("{}:{}", node.fixture, node.id)
}

/// A port as a value: `<output>:<key>`.
pub fn port_value(output: &NodeId, port: &str) -> String {
    format!("{output}:{port}")
}

/// A flow as a value: `end-to-end`, `one-per-port:<key>`.
pub fn flow_value(flow: &Flow) -> String {
    match flow {
        Flow::EndToEnd => "end-to-end".into(),
        Flow::OnePerPort { from } => format!("one-per-port:{}", from.0),
    }
}

pub(crate) fn parse_node(text: &str) -> Result<TreeRef, String> {
    let (fixture, id) = text
        .rsplit_once(':')
        .ok_or_else(|| format!("{text} isn't <fixture>:<id>"))?;
    let id = TreeId::parse(id).ok_or_else(|| format!("{id} isn't an id"))?;
    Ok(TreeRef::new(&NodeId::new(fixture), id))
}

pub(crate) fn parse_rule_subject(text: &str) -> Result<RuleSubject, String> {
    let (fixture, part) = text
        .rsplit_once(':')
        .ok_or_else(|| format!("{text} isn't <fixture>: or <fixture>:<part>"))?;
    let fixture = NodeId::new(fixture);
    if part.is_empty() {
        return Ok(RuleSubject::Fixture(fixture));
    }
    PartRef::parse(&fixture, part)
        .map(RuleSubject::Part)
        .ok_or_else(|| format!("{part} isn't a part's id"))
}

pub(crate) fn parse_port(text: &str) -> Result<PortChoice, String> {
    match text {
        "next-port" => Ok(PortChoice::NextPort),
        "stays-here" => Ok(PortChoice::StaysHere),
        _ => {
            let (output, port) = text
                .rsplit_once(':')
                .ok_or_else(|| format!("{text} isn't <output>:<port>"))?;
            Ok(PortChoice::At {
                output: NodeId::new(output),
                port: port.to_string(),
            })
        }
    }
}

fn parse_flow(text: &str) -> Result<Flow, String> {
    if text == "end-to-end" {
        return Ok(Flow::EndToEnd);
    }
    let key = text
        .strip_prefix("one-per-port:")
        .and_then(|k| k.parse().ok())
        .ok_or_else(|| format!("no flow {text}"))?;
    Ok(Flow::OnePerPort { from: PortKey(key) })
}

fn parse_back_field(text: &str) -> Result<BackField, String> {
    BackField::ALL
        .into_iter()
        .find(|f| f.name() == text)
        .ok_or_else(|| format!("no field {text}"))
}

pub(crate) fn parse_lamps(text: &str) -> Result<u32, String> {
    text.trim()
        .parse()
        .map_err(|_| format!("{text} isn't a whole number of lamps"))
}
