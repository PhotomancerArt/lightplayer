//! The patch surface's verbs, as offers (M6b): every gesture the Patching
//! view writes a patch document with, published on the node it writes
//! about, under that node's `patch` group.
//!
//! - **A fixture's verbs** live at `project/<fixture>/patch/<verb>`:
//!   `assign`, `re-anchor`, `reverse`, `rotate` and `clear` act on one of
//!   its objects (the `subject` choice); `set-flow` and `unmap-all` act on
//!   the whole fixture.
//! - **An output's verbs** live at `project/<output>/patch/<verb>`:
//!   `swap-ports` (a `port` of this output and the port to swap it `with`,
//!   on any output) and `shift-port` (a wire window and how far to move it).
//! - **The verb history** is the project's: `project/patch/undo` and
//!   `project/patch/redo`, published only while there is something to undo
//!   or redo, as Save is only published while there is something to save.
//!
//! Every subject, output and port is a choice built from what the surface
//! already lists ([`UiPatchSurface`]), and every binder captures what the
//! controller needs (the fixtures' artifacts, an unnamed output's
//! auto-name), so a renderer and the agent hand over only the picks. The
//! numbers a gesture carries (a wire lamp, a rotation's steps, a window)
//! are text the binder reads as a whole number, until offers get a number
//! kind (M6c).
//!
//! The `patch` segment keeps these out of the node card's header, which
//! draws the verbs exactly one segment under the node
//! ([`crate::UiOfferTree::verbs_of`]), while the agent's readout and `read`
//! count them as the node's own ([`crate::UiOfferTree::own_verbs_of`]).
//!
//! **Levels.** `clear` and `unmap-all` take entries off the wire: Undoable,
//! because the patch history's Undo puts them back (and Revert to saved
//! does, until a save). Everything else is Routine: assign, re-anchor,
//! reverse, rotate, swap and shift move what is on the wire without taking
//! anything off it, set-flow changes how the fixture's unpatched objects
//! are placed, and undo/redo walk the history both ways.
//!
//! The selection itself (`PatchSelect`) and the live pulse
//! (`PatchPulseOp`) are view and live gestures, not verbs, and stay as they
//! are; so do the free segment's nudges (`[`/`]`, `-`/`=`), which move a
//! window over free space and write nothing.

use lpc_model::NodeId;

use super::agent_project_edits::node_display_name;
use crate::{
    ControllerId, OfferArgError, OfferArgs, OfferBinder, OfferChoice, OfferParam, OfferPath,
    PatchVerbFixture, PatchVerbKind, PatchVerbOp, PatchVerbSubject, PatchVerbWindow,
    ProjectController, ProjectNodeAddress, UiAction, UiOffer, UiOfferTree, UiPatchSurface,
    UiPatchSurfaceFixture, UiPatchSurfaceOutput, UiPatchTarget,
};

/// The group a node's patch verbs live in (`project/<node>/patch/…`), and
/// the project's verb history (`project/patch/…`).
pub const PATCH_GROUP: &str = "patch";

/// Put a fixture's object on an output at a wire lamp.
pub const PATCH_ASSIGN_VERB: &str = "assign";
/// Move an object's start to another wire lamp on the same output.
pub const PATCH_RE_ANCHOR_VERB: &str = "re-anchor";
/// Flip an object's wire direction.
pub const PATCH_REVERSE_VERB: &str = "reverse";
/// Step an object's rotation by its stride.
pub const PATCH_ROTATE_VERB: &str = "rotate";
/// Take an object (or the whole fixture) off the wire.
pub const PATCH_CLEAR_VERB: &str = "clear";
/// Set a fixture's flow: auto-mapped or manual.
pub const PATCH_SET_FLOW_VERB: &str = "set-flow";
/// Take every object of a manual fixture off the wire, in one write.
pub const PATCH_UNMAP_ALL_VERB: &str = "unmap-all";
/// Swap the contents of two ports.
pub const PATCH_SWAP_PORTS_VERB: &str = "swap-ports";
/// Move every entry in a wire window by some lamps.
pub const PATCH_SHIFT_PORT_VERB: &str = "shift-port";
/// Undo the last patch edit.
pub const PATCH_UNDO_VERB: &str = "undo";
/// Redo the last undone patch edit.
pub const PATCH_REDO_VERB: &str = "redo";

/// The object a fixture verb acts on (see [`PATCH_WHOLE_FIXTURE`] and
/// [`UiPatchSurface::patch_subject`] for the values).
pub const PATCH_SUBJECT_PARAM: &str = "subject";
/// The output an assign lands on
/// ([`UiPatchSurfaceOutput::patch_output_value`]).
pub const PATCH_OUTPUT_PARAM: &str = "output";
/// A wire lamp: where an assign or a re-anchor puts the object's start.
pub const PATCH_LAMP_PARAM: &str = "lamp";
/// How many strides a rotate steps (negative steps back).
pub const PATCH_STEPS_PARAM: &str = "steps";
/// A fixture's flow: [`PATCH_FLOW_AUTO`] or [`PATCH_FLOW_MANUAL`].
pub const PATCH_FLOW_PARAM: &str = "flow";
/// One of this output's ports, by its key.
pub const PATCH_PORT_PARAM: &str = "port";
/// The port to swap with, on any output
/// ([`UiPatchSurfaceOutput::patch_port_value`]).
pub const PATCH_WITH_PARAM: &str = "with";
/// A shift window's first wire lamp.
pub const PATCH_START_PARAM: &str = "start";
/// A shift window's size, in lamps.
pub const PATCH_LAMPS_PARAM: &str = "lamps";
/// How many lamps a shift moves the window's entries (negative moves back).
pub const PATCH_DELTA_PARAM: &str = "delta";

/// The subject that names the whole fixture.
pub const PATCH_WHOLE_FIXTURE: &str = "all";
/// [`PATCH_FLOW_PARAM`]'s auto-mapped value.
pub const PATCH_FLOW_AUTO: &str = "auto";
/// [`PATCH_FLOW_PARAM`]'s manual value.
pub const PATCH_FLOW_MANUAL: &str = "manual";

/// Publish every patch verb the surface has a target for: each fixture's
/// (one with a patch document and a tree address), each output's (one with
/// ports), then the history's while it has something to walk.
///
/// `selection` is the surface's one selection: a subject it names on a
/// fixture is that fixture's preselected subject, a port it names is its
/// output's preselected port, and a wire it is on is the assign's
/// preselected output — so a press with no values acts on what the user
/// has selected.
pub fn publish_patch_verb_offers(
    offers: &mut UiOfferTree,
    surface: &UiPatchSurface,
    selection: Option<&UiPatchTarget>,
    can_undo: bool,
    can_redo: bool,
) {
    let fixtures = verb_fixtures(surface);
    let outputs = output_picks(surface);
    for fixture in &surface.fixtures {
        let Some(at) = patch_verbs_at(fixture.address.as_deref()) else {
            continue;
        };
        if fixture.patch_artifact.is_none() {
            continue;
        }
        let site = FixtureSite {
            at,
            node: fixture.node,
            fixtures: fixtures.clone(),
            subjects: subject_picks(surface, fixture, selection),
            preselect: selection
                .and_then(|target| surface.patch_subject(target))
                .filter(|(node, _)| *node == fixture.node)
                .map(|(_, value)| value),
        };
        offers.publish(assign_offer(&site, &outputs, selection));
        offers.publish(re_anchor_offer(&site));
        offers.publish(reverse_offer(&site));
        offers.publish(rotate_offer(&site));
        offers.publish(clear_offer(&site));
        offers.publish(set_flow_offer(&site));
        offers.publish(unmap_all_offer(&site, fixture.manual_flow));
    }
    for output in &surface.outputs {
        let Some(at) = patch_verbs_at(output.address.as_deref()) else {
            continue;
        };
        if output.bay.ports.is_empty() {
            continue;
        }
        let selected_port = match selection {
            Some(UiPatchTarget::Port { node, port }) if *node == output.node => Some(*port),
            _ => None,
        };
        offers.publish(swap_ports_offer(
            &at,
            output,
            surface,
            &fixtures,
            selected_port,
        ));
        offers.publish(shift_port_offer(&at, output, &fixtures));
    }
    if can_undo {
        offers.publish(history_offer(PATCH_UNDO_VERB));
    }
    if can_redo {
        offers.publish(history_offer(PATCH_REDO_VERB));
    }
}

/// Where the patch history's verb `verb` lives: `project/patch/<verb>`.
pub fn patch_history_path(verb: &str) -> OfferPath {
    OfferPath::project().child(PATCH_GROUP).child(verb)
}

impl UiPatchSurface {
    /// Where the patch verbs of `node` (a fixture or an output on this
    /// surface) live: `project/<node>/patch`. `None` for a node the
    /// surface does not list, or one it has no tree address for.
    pub fn patch_verbs_of(&self, node: NodeId) -> Option<OfferPath> {
        let address = self
            .fixtures
            .iter()
            .find(|fixture| fixture.node == node)
            .map(|fixture| fixture.address.as_deref())
            .or_else(|| {
                self.outputs
                    .iter()
                    .find(|output| output.node == node)
                    .map(|output| output.address.as_deref())
            })?;
        patch_verbs_at(address)
    }

    /// The fixture a selection names an object of, and that object as the
    /// `subject` value its verbs take: an instance's path (`/sector/2`), a
    /// fixture-relative lamp range (`lamps:30+30`, or `lamps:0+` to the
    /// end), or [`PATCH_WHOLE_FIXTURE`]. A bay cell names the instance
    /// covering it when one does, and its own range otherwise. Wire-side
    /// and context targets (an output, a port, a free segment, a module)
    /// name no object: `None`.
    pub fn patch_subject(&self, target: &UiPatchTarget) -> Option<(NodeId, String)> {
        let (node, subject, _) = resolve_subject(self, target)?;
        Some((node, subject_value(&subject)))
    }
}

impl UiPatchSurfaceOutput {
    /// This output as an assign's `output` value: its node's name path
    /// (`out_a`, `dome/out_b`), the way the agent names nodes. `None`
    /// without a tree address.
    pub fn patch_output_value(&self) -> Option<String> {
        let address = ProjectNodeAddress::parse(self.address.as_deref()?).ok()?;
        Some(node_display_name(&address))
    }

    /// One of this output's ports as a swap's `with` value:
    /// `<output value>:<port key>` (`out_a:1`).
    pub fn patch_port_value(&self, key: u32) -> Option<String> {
        Some(format!("{}:{key}", self.patch_output_value()?))
    }
}

/// `project/<node>/patch` for a surface row's address text.
fn patch_verbs_at(address: Option<&str>) -> Option<OfferPath> {
    let address = ProjectNodeAddress::parse(address?).ok()?;
    Some(OfferPath::project_node(&address).child(PATCH_GROUP))
}

/// What one fixture's verbs share.
struct FixtureSite {
    /// `project/<fixture>/patch`.
    at: OfferPath,
    node: NodeId,
    /// Every fixture's write-target facts (the verb op carries them all).
    fixtures: Vec<PatchVerbFixture>,
    subjects: Vec<SubjectPick>,
    /// The selection's subject, when it names one of this fixture's.
    preselect: Option<String>,
}

/// One `subject` option: its value, what it reads as, and what it means.
#[derive(Clone)]
struct SubjectPick {
    value: String,
    label: String,
    detail: String,
    subject: PatchVerbSubject,
    /// The rotation step: the instance's stride, 1 for anything else.
    stride: u32,
}

/// One `output` option.
#[derive(Clone)]
struct OutputPick {
    value: String,
    label: String,
    detail: String,
    node: NodeId,
    /// What the patch entry names: the output's name, or the name the
    /// first assign gives an unnamed one.
    output_name: Option<String>,
    name_assign: Option<(crate::ProjectSlotAddress, String)>,
}

/// One `port` / `with` option.
#[derive(Clone)]
struct PortPick {
    value: String,
    label: String,
    detail: String,
    window: PatchVerbWindow,
}

/// Every fixture's write-target facts, for the verb ops (those with a
/// patch document).
fn verb_fixtures(surface: &UiPatchSurface) -> Vec<PatchVerbFixture> {
    surface
        .fixtures
        .iter()
        .filter_map(|fixture| {
            Some(PatchVerbFixture {
                node: fixture.node,
                patch_artifact: fixture.patch_artifact.clone()?,
                mapping_artifact: fixture.mapping_artifact.clone(),
                lamp_count: fixture.patch.lamps,
            })
        })
        .collect()
}

/// The verb subject a selection names, with its fixture and rotation
/// stride (see [`UiPatchSurface::patch_subject`]).
fn resolve_subject(
    surface: &UiPatchSurface,
    target: &UiPatchTarget,
) -> Option<(NodeId, PatchVerbSubject, u32)> {
    let fixture_of = |node: NodeId| surface.fixtures.iter().find(|f| f.node == node);
    match target {
        UiPatchTarget::Instance { node, path } => {
            let fixture = fixture_of(*node)?;
            let instance = fixture
                .instances
                .iter()
                .find(|instance| instance.path == *path);
            if path.is_empty() {
                // An id-less object is addressed by its lamps.
                let instance = instance?;
                return Some((
                    *node,
                    range_subject(instance.start, Some(instance.lamps)),
                    1,
                ));
            }
            Some((
                *node,
                path_subject(path),
                instance.map_or(1, |instance| instance.stride),
            ))
        }
        UiPatchTarget::Fixture { node } => {
            fixture_of(*node)?;
            Some((*node, PatchVerbSubject::default(), 1))
        }
        UiPatchTarget::Cell { id } => {
            // Cell ids are `node:output:source:wire` (the bay's format).
            let node = NodeId::new(id.split(':').next()?.parse().ok()?);
            let fixture = fixture_of(node)?;
            let cell = fixture.patch.cells.iter().find(|cell| cell.id == *id)?;
            // Prefer the instance covering the cell: path entries match by
            // path, and the instance's stride is the honest rotation step.
            if let Some(instance) = fixture.instances.iter().find(|instance| {
                cell.source_start >= instance.start
                    && cell.source_start < instance.start + instance.lamps
            }) {
                return Some(if instance.path.is_empty() {
                    (node, range_subject(instance.start, Some(instance.lamps)), 1)
                } else {
                    (node, path_subject(&instance.path), instance.stride)
                });
            }
            Some((node, range_subject(cell.source_start, Some(cell.lamps)), 1))
        }
        UiPatchTarget::Range { node, start, count } => {
            fixture_of(*node)?;
            Some((*node, range_subject(*start, *count), 1))
        }
        UiPatchTarget::Output { .. }
        | UiPatchTarget::Port { .. }
        | UiPatchTarget::Segment { .. }
        | UiPatchTarget::Module { .. } => None,
    }
}

fn path_subject(path: &str) -> PatchVerbSubject {
    PatchVerbSubject {
        path: Some(path.to_string()),
        range: None,
    }
}

fn range_subject(start: u32, count: Option<u32>) -> PatchVerbSubject {
    PatchVerbSubject {
        path: None,
        range: Some((start, count)),
    }
}

/// A subject as its `subject` value.
fn subject_value(subject: &PatchVerbSubject) -> String {
    match (&subject.path, subject.range) {
        (Some(path), _) => path.clone(),
        (None, Some((start, Some(count)))) => format!("lamps:{start}+{count}"),
        (None, Some((start, None))) => format!("lamps:{start}+"),
        (None, None) => PATCH_WHOLE_FIXTURE.to_string(),
    }
}

/// A subject as the user reads it.
fn subject_label(subject: &PatchVerbSubject) -> String {
    match (&subject.path, subject.range) {
        (Some(path), _) => path.clone(),
        (None, Some((start, Some(count)))) => format!(
            "lamps {start} to {}",
            start.saturating_add(count).saturating_sub(1)
        ),
        (None, Some((start, None))) => format!("lamps {start} to the end"),
        (None, None) => "the whole fixture".to_string(),
    }
}

/// The fixture's `subject` options: the whole fixture, each object (by its
/// path, or by its lamps when it has no id), the whole strand of a fixture
/// with no objects, every run on the wire no object covers, and the
/// selection's own subject when none of those is it.
fn subject_picks(
    surface: &UiPatchSurface,
    fixture: &UiPatchSurfaceFixture,
    selection: Option<&UiPatchTarget>,
) -> Vec<SubjectPick> {
    let mut picks: Vec<SubjectPick> = Vec::new();
    let push = |picks: &mut Vec<SubjectPick>, pick: SubjectPick| {
        if !picks.iter().any(|known| known.value == pick.value) {
            picks.push(pick);
        }
    };
    let whole = PatchVerbSubject::default();
    push(
        &mut picks,
        SubjectPick {
            value: subject_value(&whole),
            label: subject_label(&whole),
            detail: format!("{} lamps", fixture.patch.lamps),
            subject: whole,
            stride: 1,
        },
    );
    for instance in &fixture.instances {
        let (subject, stride) = if instance.path.is_empty() {
            (range_subject(instance.start, Some(instance.lamps)), 1)
        } else {
            (path_subject(&instance.path), instance.stride)
        };
        push(
            &mut picks,
            SubjectPick {
                value: subject_value(&subject),
                label: instance.label.clone(),
                detail: format!(
                    "{} lamps, {}",
                    instance.lamps,
                    if instance.placed {
                        "on a wire"
                    } else {
                        "not on a wire"
                    }
                ),
                subject,
                stride,
            },
        );
    }
    if fixture.instances.is_empty() {
        let strand = range_subject(0, None);
        push(
            &mut picks,
            SubjectPick {
                value: subject_value(&strand),
                label: subject_label(&strand),
                detail: "the whole strand, as one run".to_string(),
                subject: strand,
                stride: 1,
            },
        );
    }
    for cell in &fixture.patch.cells {
        let covered = fixture.instances.iter().any(|instance| {
            cell.source_start >= instance.start
                && cell.source_start < instance.start + instance.lamps
        });
        if covered {
            continue;
        }
        let run = range_subject(cell.source_start, Some(cell.lamps));
        push(
            &mut picks,
            SubjectPick {
                value: subject_value(&run),
                label: subject_label(&run),
                detail: format!("{} lamps on a wire", cell.lamps),
                subject: run,
                stride: 1,
            },
        );
    }
    if let Some((node, subject, stride)) =
        selection.and_then(|target| resolve_subject(surface, target))
        && node == fixture.node
    {
        push(
            &mut picks,
            SubjectPick {
                value: subject_value(&subject),
                label: subject_label(&subject),
                detail: "selected".to_string(),
                subject,
                stride,
            },
        );
    }
    picks
}

/// Every output an assign can land on.
fn output_picks(surface: &UiPatchSurface) -> Vec<OutputPick> {
    surface
        .outputs
        .iter()
        .filter_map(|output| {
            let value = output.patch_output_value()?;
            let lamps: u32 = output.bay.ports.iter().map(|port| port.lamps).sum();
            let detail = match (&output.name, &output.name_assign) {
                (None, Some((_, name))) => {
                    format!("{lamps} lamps; unnamed, the first assign names it \"{name}\"")
                }
                _ => format!("{lamps} lamps"),
            };
            Some(OutputPick {
                value,
                label: output.display_name().to_string(),
                detail,
                node: output.node,
                output_name: output
                    .name
                    .clone()
                    .or_else(|| output.name_assign.as_ref().map(|(_, name)| name.clone())),
                name_assign: output.name_assign.clone(),
            })
        })
        .collect()
}

/// One output's ports as `port` options (the value is the key).
fn own_port_picks(output: &UiPatchSurfaceOutput) -> Vec<PortPick> {
    output
        .bay
        .ports
        .iter()
        .map(|port| PortPick {
            value: port.key.to_string(),
            label: port.pin_label.clone(),
            detail: window_detail(port.start, port.lamps),
            window: PatchVerbWindow {
                output_name: output.name.clone(),
                start: port.start,
                lamps: port.lamps,
            },
        })
        .collect()
}

/// Every port on the surface as `with` options.
fn every_port_pick(surface: &UiPatchSurface) -> Vec<PortPick> {
    surface
        .outputs
        .iter()
        .flat_map(|output| {
            output.bay.ports.iter().filter_map(move |port| {
                Some(PortPick {
                    value: output.patch_port_value(port.key)?,
                    label: format!("{} {}", output.display_name(), port.pin_label),
                    detail: window_detail(port.start, port.lamps),
                    window: PatchVerbWindow {
                        output_name: output.name.clone(),
                        start: port.start,
                        lamps: port.lamps,
                    },
                })
            })
        })
        .collect()
}

fn window_detail(start: u32, lamps: u32) -> String {
    format!(
        "wire lamps {start} to {}",
        start.saturating_add(lamps).saturating_sub(1)
    )
}

/// The `subject` parameter, preselected with the selection's.
fn subject_param(site: &FixtureSite) -> OfferParam {
    OfferParam::choice(
        PATCH_SUBJECT_PARAM,
        "subject",
        site.subjects
            .iter()
            .map(|pick| OfferChoice::new(&pick.value, &pick.label).with_detail(&pick.detail))
            .collect(),
        site.preselect.clone(),
    )
}

/// A wire-lamp parameter.
fn lamp_param() -> OfferParam {
    OfferParam::text(
        PATCH_LAMP_PARAM,
        "wire lamp",
        "the output's lamp number, from 0, where the object starts",
    )
}

/// The action that runs `verb` on `subject` of the fixture at `node`.
fn subject_action(
    node: NodeId,
    fixtures: &[PatchVerbFixture],
    subject: PatchVerbSubject,
    verb: PatchVerbKind,
) -> UiAction {
    patch_action(PatchVerbOp {
        subject_fixture: Some(node),
        subject,
        fixtures: fixtures.to_vec(),
        assign_output_name: None,
        verb,
    })
}

/// A verb op made into an action for the project controller.
fn patch_action(op: PatchVerbOp) -> UiAction {
    UiAction::from_op(ControllerId::new(ProjectController::NODE_ID), op)
}

/// The picked subject. [`UiOffer::press`] checks the value against the
/// options first, so only a press with no subject at all misses.
fn picked_subject<'a>(
    subjects: &'a [SubjectPick],
    args: &OfferArgs,
) -> Result<&'a SubjectPick, OfferArgError> {
    let picked = args.choice(PATCH_SUBJECT_PARAM).unwrap_or_default();
    subjects
        .iter()
        .find(|pick| pick.value == picked)
        .ok_or_else(|| missing(PATCH_SUBJECT_PARAM, "subject"))
}

/// `…/assign`: put the subject on an output at a wire lamp. The armed
/// click's completion, the panel's pickers, and the agent's link all press
/// this.
fn assign_offer(
    site: &FixtureSite,
    outputs: &[OutputPick],
    selection: Option<&UiPatchTarget>,
) -> UiOffer {
    // A selection on an output's wire (a free segment, a port, the output)
    // is the output an assign most likely means.
    let preselect_output = selection
        .and_then(|target| match target {
            UiPatchTarget::Segment { node, .. }
            | UiPatchTarget::Port { node, .. }
            | UiPatchTarget::Output { node } => Some(*node),
            _ => None,
        })
        .and_then(|node| outputs.iter().find(|pick| pick.node == node))
        .map(|pick| pick.value.clone());
    let output_param = OfferParam::choice(
        PATCH_OUTPUT_PARAM,
        "wire",
        outputs
            .iter()
            .map(|pick| OfferChoice::new(&pick.value, &pick.label).with_detail(&pick.detail))
            .collect(),
        preselect_output,
    );
    let unbound = subject_action(
        site.node,
        &site.fixtures,
        PatchVerbSubject::default(),
        PatchVerbKind::Assign {
            output_name: None,
            lamp: 0,
        },
    )
    .with_label("Assign")
    .with_summary("Put an object of this fixture on an output, starting at a wire lamp.");
    let subjects = site.subjects.clone();
    let outputs = outputs.to_vec();
    let fixtures = site.fixtures.clone();
    let node = site.node;
    let binder = OfferBinder::new(move |args: &OfferArgs| {
        let subject = picked_subject(&subjects, args)?;
        let picked = args.choice(PATCH_OUTPUT_PARAM).unwrap_or_default();
        let output = outputs
            .iter()
            .find(|pick| pick.value == picked)
            .ok_or_else(|| missing(PATCH_OUTPUT_PARAM, "wire"))?;
        let lamp = whole_number(args, PATCH_LAMP_PARAM, "a wire lamp number, from 0")?;
        Ok(patch_action(PatchVerbOp {
            subject_fixture: Some(node),
            subject: subject.subject.clone(),
            fixtures: fixtures.clone(),
            assign_output_name: output.name_assign.clone(),
            verb: PatchVerbKind::Assign {
                output_name: output.output_name.clone(),
                lamp,
            },
        })
        .with_label(format!(
            "Assign {} to {} at lamp {lamp}",
            subject.label, output.label
        ))
        .with_summary("Put the object on the output's wire, starting at that lamp."))
    });
    UiOffer::with_params(
        site.at.clone().child(PATCH_ASSIGN_VERB),
        "edit",
        vec![subject_param(site), output_param, lamp_param()],
        binder,
        unbound,
    )
}

/// `…/re-anchor`: move the subject's start to another lamp, same output.
fn re_anchor_offer(site: &FixtureSite) -> UiOffer {
    let unbound = subject_action(
        site.node,
        &site.fixtures,
        PatchVerbSubject::default(),
        PatchVerbKind::ReAnchor { lamp: 0 },
    )
    .with_label("Re-anchor")
    .with_summary("Move an object to start at another lamp of the same output.");
    let subjects = site.subjects.clone();
    let fixtures = site.fixtures.clone();
    let node = site.node;
    let binder = OfferBinder::new(move |args: &OfferArgs| {
        let subject = picked_subject(&subjects, args)?;
        let lamp = whole_number(args, PATCH_LAMP_PARAM, "a wire lamp number, from 0")?;
        Ok(subject_action(
            node,
            &fixtures,
            subject.subject.clone(),
            PatchVerbKind::ReAnchor { lamp },
        )
        .with_label(format!("Re-anchor {} at lamp {lamp}", subject.label))
        .with_summary("Move the object to start at that lamp of the same output."))
    });
    UiOffer::with_params(
        site.at.clone().child(PATCH_RE_ANCHOR_VERB),
        "edit",
        vec![subject_param(site), lamp_param()],
        binder,
        unbound,
    )
}

/// `…/reverse`: flip the subject's wire direction.
fn reverse_offer(site: &FixtureSite) -> UiOffer {
    subject_only_offer(
        site,
        PATCH_REVERSE_VERB,
        "Reverse",
        "Flip the object's direction along the wire.",
        PatchVerbKind::Reverse,
        false,
    )
}

/// `…/clear`: take the subject off the wire. Undoable: the patch history's
/// Undo puts it back.
fn clear_offer(site: &FixtureSite) -> UiOffer {
    subject_only_offer(
        site,
        PATCH_CLEAR_VERB,
        "Unmap",
        "Take the object off the wire (every entry of the fixture, for `all`).",
        PatchVerbKind::Clear,
        true,
    )
}

/// A verb whose only value is its subject.
fn subject_only_offer(
    site: &FixtureSite,
    verb: &str,
    label: &'static str,
    summary: &'static str,
    kind: PatchVerbKind,
    undoable: bool,
) -> UiOffer {
    let level = move |action: UiAction| {
        if undoable { action.undoable() } else { action }
    };
    let unbound = level(
        subject_action(
            site.node,
            &site.fixtures,
            PatchVerbSubject::default(),
            kind.clone(),
        )
        .with_label(label)
        .with_summary(summary),
    );
    let subjects = site.subjects.clone();
    let fixtures = site.fixtures.clone();
    let node = site.node;
    let binder = OfferBinder::new(move |args: &OfferArgs| {
        let subject = picked_subject(&subjects, args)?;
        Ok(level(
            subject_action(node, &fixtures, subject.subject.clone(), kind.clone())
                .with_label(format!("{label} {}", subject.label))
                .with_summary(summary),
        ))
    });
    UiOffer::with_params(
        site.at.clone().child(verb),
        if undoable { "remove" } else { "edit" },
        vec![subject_param(site)],
        binder,
        unbound,
    )
}

/// `…/rotate`: step the subject's rotation by `steps` of its stride (an
/// object's authored stride; one lamp for a run).
fn rotate_offer(site: &FixtureSite) -> UiOffer {
    let unbound = subject_action(
        site.node,
        &site.fixtures,
        PatchVerbSubject::default(),
        PatchVerbKind::Rotate {
            steps: 1,
            stride: 1,
        },
    )
    .with_label("Rotate")
    .with_summary("Step the object's rotation along the wire by its stride.");
    let subjects = site.subjects.clone();
    let fixtures = site.fixtures.clone();
    let node = site.node;
    let binder = OfferBinder::new(move |args: &OfferArgs| {
        let subject = picked_subject(&subjects, args)?;
        let steps = signed_number(args, PATCH_STEPS_PARAM, "a whole number of strides")?;
        let stride = subject.stride;
        Ok(subject_action(
            node,
            &fixtures,
            subject.subject.clone(),
            PatchVerbKind::Rotate { steps, stride },
        )
        .with_label(format!(
            "Rotate {} {} {}",
            subject.label,
            if steps < 0 { "back" } else { "forward" },
            match steps.unsigned_abs() {
                1 => "one stride".to_string(),
                n => format!("{n} strides"),
            }
        ))
        .with_summary(format!("Step the rotation by {steps} × {stride} lamps.")))
    });
    UiOffer::with_params(
        site.at.clone().child(PATCH_ROTATE_VERB),
        "edit",
        vec![
            subject_param(site),
            OfferParam::text(
                PATCH_STEPS_PARAM,
                "number of steps",
                "strides to step: 1 forward, -1 back",
            ),
        ],
        binder,
        unbound,
    )
}

/// `…/set-flow`: auto-mapped (objects place themselves) or manual (only
/// what is patched lights up).
fn set_flow_offer(site: &FixtureSite) -> UiOffer {
    let action = |manual: bool| {
        subject_action(
            site.node,
            &site.fixtures,
            PatchVerbSubject::default(),
            PatchVerbKind::SetFlow { manual },
        )
    };
    let unbound = action(false).with_label("Set flow").with_summary(
        "Choose whether the fixture's objects place themselves or wait to be patched.",
    );
    let auto = action(false).with_label("Set flow: auto-mapped");
    let manual = action(true).with_label("Set flow: manual");
    let binder = OfferBinder::new(
        move |args: &OfferArgs| match args.choice(PATCH_FLOW_PARAM) {
            Some(PATCH_FLOW_MANUAL) => Ok(manual.clone()),
            Some(PATCH_FLOW_AUTO) => Ok(auto.clone()),
            _ => Err(missing(PATCH_FLOW_PARAM, "flow")),
        },
    );
    UiOffer::with_params(
        site.at.clone().child(PATCH_SET_FLOW_VERB),
        "edit",
        vec![OfferParam::choice(
            PATCH_FLOW_PARAM,
            "flow",
            vec![
                OfferChoice::new(PATCH_FLOW_AUTO, "auto-mapped")
                    .with_detail("objects place themselves along the wire"),
                OfferChoice::new(PATCH_FLOW_MANUAL, "manual")
                    .with_detail("only what you patch lights up; unmapped stays dark"),
            ],
            None,
        )],
        binder,
        unbound,
    )
}

/// `…/unmap-all`: take every object of the fixture off the wire in one
/// write. Undoable. Only a manual fixture can be unmapped: an auto-mapped
/// one flows its objects straight back on.
fn unmap_all_offer(site: &FixtureSite, manual: bool) -> UiOffer {
    let action = subject_action(
        site.node,
        &site.fixtures,
        PatchVerbSubject::default(),
        PatchVerbKind::UnmapAll,
    )
    .with_label("Unmap all")
    .with_summary("Take every object of this fixture off the wire.")
    .undoable();
    UiOffer::new(
        site.at.clone().child(PATCH_UNMAP_ALL_VERB),
        "remove",
        if manual {
            action
        } else {
            action.disabled("the fixture is auto-mapped: set its flow to manual first")
        },
    )
}

/// `…/swap-ports`: swap one of this output's ports with another port. The
/// armed swap's second click presses it with both.
fn swap_ports_offer(
    at: &OfferPath,
    output: &UiPatchSurfaceOutput,
    surface: &UiPatchSurface,
    fixtures: &[PatchVerbFixture],
    selected_port: Option<u32>,
) -> UiOffer {
    let ports = own_port_picks(output);
    let others = every_port_pick(surface);
    let action = {
        let fixtures = fixtures.to_vec();
        move |a: PatchVerbWindow, b: PatchVerbWindow| {
            patch_action(PatchVerbOp {
                subject_fixture: None,
                subject: PatchVerbSubject::default(),
                fixtures: fixtures.clone(),
                assign_output_name: None,
                verb: PatchVerbKind::SwapPorts { a, b },
            })
        }
    };
    let first = |picks: &[PortPick]| {
        picks
            .first()
            .map(|pick| pick.window.clone())
            .unwrap_or(PatchVerbWindow {
                output_name: output.name.clone(),
                start: 0,
                lamps: 0,
            })
    };
    let unbound = no_fixture_reason(
        action(first(&ports), first(&others))
            .with_label("Swap ports")
            .with_summary("Swap what two ports carry, across every fixture on them."),
        fixtures,
    );
    let params = vec![
        OfferParam::choice(
            PATCH_PORT_PARAM,
            "port",
            port_choices(&ports),
            selected_port.map(|key| key.to_string()),
        ),
        OfferParam::choice(
            PATCH_WITH_PARAM,
            "port to swap with",
            port_choices(&others),
            None,
        ),
    ];
    let name = output.display_name().to_string();
    let any_fixture = !fixtures.is_empty();
    let binder = OfferBinder::new(move |args: &OfferArgs| {
        let picked = args.choice(PATCH_PORT_PARAM).unwrap_or_default();
        let a = ports
            .iter()
            .find(|pick| pick.value == picked)
            .ok_or_else(|| missing(PATCH_PORT_PARAM, "port"))?;
        let picked = args.choice(PATCH_WITH_PARAM).unwrap_or_default();
        let b = others
            .iter()
            .find(|pick| pick.value == picked)
            .ok_or_else(|| missing(PATCH_WITH_PARAM, "port to swap with"))?;
        if a.window == b.window {
            return Err(OfferArgError::Invalid {
                name: PATCH_WITH_PARAM.to_string(),
                reason: "it is the same port".to_string(),
            });
        }
        let bound = action(a.window.clone(), b.window.clone())
            .with_label(format!("Swap {name} {} with {}", a.label, b.label))
            .with_summary("Swap what the two ports carry, across every fixture on them.");
        Ok(if any_fixture {
            bound
        } else {
            bound.disabled(NO_FIXTURE)
        })
    });
    UiOffer::with_params(
        at.clone().child(PATCH_SWAP_PORTS_VERB),
        "edit",
        params,
        binder,
        unbound,
    )
}

fn port_choices(picks: &[PortPick]) -> Vec<OfferChoice> {
    picks
        .iter()
        .map(|pick| OfferChoice::new(&pick.value, &pick.label).with_detail(&pick.detail))
        .collect()
}

/// `…/shift-port`: move every entry in a wire window of this output by
/// `delta` lamps (the run under a selected object, or any window).
fn shift_port_offer(
    at: &OfferPath,
    output: &UiPatchSurfaceOutput,
    fixtures: &[PatchVerbFixture],
) -> UiOffer {
    let output_name = output.name.clone();
    let action = {
        let fixtures = fixtures.to_vec();
        move |window: PatchVerbWindow, delta: i32| {
            patch_action(PatchVerbOp {
                subject_fixture: None,
                subject: PatchVerbSubject::default(),
                fixtures: fixtures.clone(),
                assign_output_name: None,
                verb: PatchVerbKind::ShiftPort { window, delta },
            })
        }
    };
    let unbound = no_fixture_reason(
        action(
            PatchVerbWindow {
                output_name: output_name.clone(),
                start: 0,
                lamps: 0,
            },
            0,
        )
        .with_label("Shift")
        .with_summary("Move every entry in a window of this output's wire by some lamps."),
        fixtures,
    );
    let any_fixture = !fixtures.is_empty();
    let binder = OfferBinder::new(move |args: &OfferArgs| {
        let start = whole_number(args, PATCH_START_PARAM, "a wire lamp number, from 0")?;
        let lamps = whole_number(args, PATCH_LAMPS_PARAM, "a number of lamps, 1 or more")?;
        if lamps == 0 {
            return Err(OfferArgError::Invalid {
                name: PATCH_LAMPS_PARAM.to_string(),
                reason: "a window holds at least one lamp".to_string(),
            });
        }
        let delta = signed_number(args, PATCH_DELTA_PARAM, "a whole number of lamps")?;
        let bound = action(
            PatchVerbWindow {
                output_name: output_name.clone(),
                start,
                lamps,
            },
            delta,
        )
        .with_label(format!(
            "Shift lamps {start} to {} by {delta}",
            start.saturating_add(lamps).saturating_sub(1)
        ))
        .with_summary("Move every entry in the window along the wire.");
        Ok(if any_fixture {
            bound
        } else {
            bound.disabled(NO_FIXTURE)
        })
    });
    UiOffer::with_params(
        at.clone().child(PATCH_SHIFT_PORT_VERB),
        "edit",
        vec![
            OfferParam::text(
                PATCH_START_PARAM,
                "window start",
                "the window's first wire lamp, from 0",
            ),
            OfferParam::text(PATCH_LAMPS_PARAM, "window size", "how many lamps it spans"),
            OfferParam::text(PATCH_DELTA_PARAM, "shift", "lamps to move: 1 on, -1 back"),
        ],
        binder,
        unbound,
    )
}

/// Why a port verb cannot run on a surface with no fixture to sweep.
const NO_FIXTURE: &str = "no fixture's patch document is on this surface";

/// A port verb with no fixture to sweep is disabled with why.
fn no_fixture_reason(action: UiAction, fixtures: &[PatchVerbFixture]) -> UiAction {
    if fixtures.is_empty() {
        action.disabled(NO_FIXTURE)
    } else {
        action
    }
}

/// `project/patch/undo` or `project/patch/redo`.
fn history_offer(verb: &str) -> UiOffer {
    let (kind, label, summary) = if verb == PATCH_UNDO_VERB {
        (
            PatchVerbKind::Undo,
            "Undo patch edit",
            "Put the patch documents back as they were before the last patch edit.",
        )
    } else {
        (
            PatchVerbKind::Redo,
            "Redo patch edit",
            "Apply the last undone patch edit again.",
        )
    };
    UiOffer::new(
        patch_history_path(verb),
        "revert",
        patch_action(PatchVerbOp {
            subject_fixture: None,
            subject: PatchVerbSubject::default(),
            fixtures: Vec::new(),
            assign_output_name: None,
            verb: kind,
        })
        .with_label(label)
        .with_summary(summary),
    )
}

/// A text parameter read as a whole number, 0 or more.
fn whole_number(args: &OfferArgs, name: &str, what: &str) -> Result<u32, OfferArgError> {
    let text = args.text(name).unwrap_or_default();
    text.parse().map_err(|_| OfferArgError::Invalid {
        name: name.to_string(),
        reason: format!("`{text}` is not {what}"),
    })
}

/// A text parameter read as a whole number other than 0.
fn signed_number(args: &OfferArgs, name: &str, what: &str) -> Result<i32, OfferArgError> {
    let text = args.text(name).unwrap_or_default();
    match text.parse::<i32>() {
        Ok(0) => Err(OfferArgError::Invalid {
            name: name.to_string(),
            reason: "0 would change nothing".to_string(),
        }),
        Ok(value) => Ok(value),
        Err(_) => Err(OfferArgError::Invalid {
            name: name.to_string(),
            reason: format!("`{text}` is not {what}"),
        }),
    }
}

/// A binder handed no value for a required choice. [`UiOffer::press`]
/// checks the value against the options first, so only a press with no
/// value at all gets here.
fn missing(name: &str, label: &str) -> OfferArgError {
    OfferArgError::Missing {
        name: name.to_string(),
        label: label.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use lpc_model::ArtifactLocation;

    use super::*;
    use crate::{
        ActionConsequence, UiFixturePatch, UiPatchBay, UiPatchCell, UiPatchInstance, UiPatchPort,
    };

    #[test]
    fn every_patch_target_publishes_its_verbs_under_its_patch_group() {
        let offers = published(&surface(), None, false, false);
        let paths: Vec<String> = offers.iter().map(|offer| offer.path.to_string()).collect();
        assert_eq!(
            paths,
            [
                "project/demo.module/dome.fixture/patch/assign",
                "project/demo.module/dome.fixture/patch/re-anchor",
                "project/demo.module/dome.fixture/patch/reverse",
                "project/demo.module/dome.fixture/patch/rotate",
                "project/demo.module/dome.fixture/patch/clear",
                "project/demo.module/dome.fixture/patch/set-flow",
                "project/demo.module/dome.fixture/patch/unmap-all",
                "project/demo.module/out_a.output/patch/swap-ports",
                "project/demo.module/out_a.output/patch/shift-port",
            ],
            "nothing to undo or redo publishes no history"
        );
        let dome = OfferPath::parse("project/demo.module/dome.fixture").unwrap();
        assert_eq!(
            offers.verbs_of(&dome).count(),
            0,
            "a card's header never draws them"
        );
        assert_eq!(offers.own_verbs_of(&dome).count(), 7);

        let offers = published(&surface(), None, true, true);
        assert!(offers.get(&patch_history_path(PATCH_UNDO_VERB)).is_some());
        assert!(offers.get(&patch_history_path(PATCH_REDO_VERB)).is_some());
    }

    #[test]
    fn a_subject_verb_takes_an_object_and_the_selection_preselects_it() {
        let surface = surface();
        let offers = published(&surface, None, false, false);
        let reverse = verb(&offers, "dome.fixture", PATCH_REVERSE_VERB);
        assert!(!reverse.is_enabled(), "no selection, no subject");
        let pressed = reverse
            .press(&OfferArgs::new().with(PATCH_SUBJECT_PARAM, "/sector/2"))
            .expect("sector 2 is an object of the dome");
        let op = pressed.op_as::<PatchVerbOp>().unwrap();
        assert_eq!(op.subject_fixture, Some(dome()));
        assert_eq!(op.subject, path_subject("/sector/2"));
        assert_eq!(op.verb, PatchVerbKind::Reverse);
        assert_eq!(op.fixtures.len(), 1);
        assert!(pressed.meta().consequence.is_routine());
        assert!(
            reverse
                .press(&OfferArgs::new().with(PATCH_SUBJECT_PARAM, "/sector/9"))
                .is_err(),
            "an object the surface does not list is refused"
        );

        // A selected object is the default subject.
        let selected = UiPatchTarget::Instance {
            node: dome(),
            path: "/sector/1".to_string(),
        };
        let offers = published(&surface, Some(&selected), false, false);
        let pressed = verb(&offers, "dome.fixture", PATCH_REVERSE_VERB)
            .press(&OfferArgs::new())
            .expect("the selection is the subject");
        assert_eq!(
            pressed.op_as::<PatchVerbOp>().unwrap().subject,
            path_subject("/sector/1")
        );
    }

    #[test]
    fn rotate_steps_by_the_objects_stride_and_clear_is_undoable() {
        let offers = published(&surface(), None, false, false);
        let rotate = verb(&offers, "dome.fixture", PATCH_ROTATE_VERB);
        let pressed = rotate
            .press(
                &OfferArgs::new()
                    .with(PATCH_SUBJECT_PARAM, "/sector/1")
                    .with(PATCH_STEPS_PARAM, "-2"),
            )
            .unwrap();
        assert_eq!(
            pressed.op_as::<PatchVerbOp>().unwrap().verb,
            PatchVerbKind::Rotate {
                steps: -2,
                stride: 10
            },
            "the instance's authored stride"
        );
        for (steps, wanted) in [("0", "0 would change nothing"), ("two", "`two` is not")] {
            let refused = rotate
                .press(
                    &OfferArgs::new()
                        .with(PATCH_SUBJECT_PARAM, "/sector/1")
                        .with(PATCH_STEPS_PARAM, steps),
                )
                .unwrap_err()
                .to_string();
            assert!(refused.contains(wanted), "{refused}");
        }

        let clear = verb(&offers, "dome.fixture", PATCH_CLEAR_VERB);
        assert_eq!(clear.consequence(), &ActionConsequence::Undoable);
        let pressed = clear
            .press(&OfferArgs::new().with(PATCH_SUBJECT_PARAM, PATCH_WHOLE_FIXTURE))
            .unwrap();
        assert_eq!(
            pressed.op_as::<PatchVerbOp>().unwrap().subject,
            PatchVerbSubject::default(),
            "`all` is the whole fixture"
        );
        assert_eq!(pressed.meta().consequence, ActionConsequence::Undoable);
    }

    #[test]
    fn assign_names_an_unnamed_output_and_lands_at_the_lamp() {
        let segment = UiPatchTarget::Segment {
            node: output(),
            port: 0,
            start: 30,
            lamps: 30,
        };
        let offers = published(&surface(), Some(&segment), false, false);
        let assign = verb(&offers, "dome.fixture", PATCH_ASSIGN_VERB);
        let pressed = assign
            .press(
                &OfferArgs::new()
                    .with(PATCH_SUBJECT_PARAM, "/sector/2")
                    .with(PATCH_LAMP_PARAM, "30"),
            )
            .expect("the selected segment's output is preselected");
        let op = pressed.op_as::<PatchVerbOp>().unwrap();
        assert_eq!(
            op.verb,
            PatchVerbKind::Assign {
                output_name: Some("1".to_string()),
                lamp: 30
            }
        );
        assert_eq!(
            op.assign_output_name
                .as_ref()
                .map(|(_, name)| name.as_str()),
            Some("1"),
            "the first assign names the unnamed output"
        );
        assert_eq!(pressed.meta().label, "Assign sector 2 to out_a at lamp 30");
        let refused = assign
            .press(
                &OfferArgs::new()
                    .with(PATCH_SUBJECT_PARAM, "/sector/2")
                    .with(PATCH_LAMP_PARAM, "-1"),
            )
            .unwrap_err()
            .to_string();
        assert!(refused.contains("not a wire lamp number"), "{refused}");
    }

    #[test]
    fn swap_takes_both_ports_and_refuses_the_same_one_twice() {
        let mut surface = surface();
        surface.outputs[0]
            .bay
            .ports
            .push(port(1, 60, 20, Vec::new()));
        let offers = published(&surface, None, false, false);
        let swap = verb(&offers, "out_a.output", PATCH_SWAP_PORTS_VERB);
        let pressed = swap
            .press(
                &OfferArgs::new()
                    .with(PATCH_PORT_PARAM, "0")
                    .with(PATCH_WITH_PARAM, "out_a:1"),
            )
            .unwrap();
        let PatchVerbKind::SwapPorts { a, b } = &pressed.op_as::<PatchVerbOp>().unwrap().verb
        else {
            panic!("a swap");
        };
        assert_eq!((a.start, a.lamps, b.start, b.lamps), (0, 60, 60, 20));
        assert!(
            swap.press(
                &OfferArgs::new()
                    .with(PATCH_PORT_PARAM, "1")
                    .with(PATCH_WITH_PARAM, "out_a:1"),
            )
            .unwrap_err()
            .to_string()
            .contains("the same port")
        );

        let shift = verb(&offers, "out_a.output", PATCH_SHIFT_PORT_VERB);
        let pressed = shift
            .press(
                &OfferArgs::new()
                    .with(PATCH_START_PARAM, "0")
                    .with(PATCH_LAMPS_PARAM, "30")
                    .with(PATCH_DELTA_PARAM, "-10"),
            )
            .unwrap();
        assert_eq!(
            pressed.op_as::<PatchVerbOp>().unwrap().verb,
            PatchVerbKind::ShiftPort {
                window: PatchVerbWindow {
                    output_name: None,
                    start: 0,
                    lamps: 30
                },
                delta: -10
            }
        );
    }

    #[test]
    fn flow_is_a_choice_and_unmap_all_waits_for_manual() {
        let mut surface = surface();
        surface.fixtures[0].manual_flow = false;
        let offers = published(&surface, None, false, false);
        let unmap = verb(&offers, "dome.fixture", PATCH_UNMAP_ALL_VERB);
        assert!(
            !unmap.is_enabled(),
            "an auto-mapped fixture has nothing to unmap"
        );
        assert_eq!(unmap.consequence(), &ActionConsequence::Undoable);
        let pressed = verb(&offers, "dome.fixture", PATCH_SET_FLOW_VERB)
            .press(&OfferArgs::new().with(PATCH_FLOW_PARAM, PATCH_FLOW_MANUAL))
            .unwrap();
        assert_eq!(
            pressed.op_as::<PatchVerbOp>().unwrap().verb,
            PatchVerbKind::SetFlow { manual: true }
        );

        surface.fixtures[0].manual_flow = true;
        let offers = published(&surface, None, false, false);
        assert!(verb(&offers, "dome.fixture", PATCH_UNMAP_ALL_VERB).is_enabled());
    }

    #[test]
    fn a_selection_reads_as_its_fixtures_subject() {
        let surface = surface();
        let at = |target: UiPatchTarget| surface.patch_subject(&target);
        assert_eq!(
            at(UiPatchTarget::Fixture { node: dome() }),
            Some((dome(), PATCH_WHOLE_FIXTURE.to_string()))
        );
        assert_eq!(
            at(UiPatchTarget::Cell {
                id: "2:0".to_string()
            }),
            Some((dome(), "/sector/1".to_string())),
            "a cell names the instance covering it"
        );
        assert_eq!(
            at(UiPatchTarget::Range {
                node: dome(),
                start: 5,
                count: None,
            }),
            Some((dome(), "lamps:5+".to_string()))
        );
        assert_eq!(
            at(UiPatchTarget::Port {
                node: output(),
                port: 0
            }),
            None,
            "a port is not an object"
        );
        assert_eq!(
            surface.patch_verbs_of(output()).map(|at| at.to_string()),
            Some("project/demo.module/out_a.output/patch".to_string())
        );
        assert_eq!(
            surface.outputs[0].patch_port_value(0).as_deref(),
            Some("out_a:0")
        );
    }

    fn published(
        surface: &UiPatchSurface,
        selection: Option<&UiPatchTarget>,
        undo: bool,
        redo: bool,
    ) -> UiOfferTree {
        let mut offers = UiOfferTree::new();
        publish_patch_verb_offers(&mut offers, surface, selection, undo, redo);
        offers
    }

    fn verb<'a>(offers: &'a UiOfferTree, node: &str, verb: &str) -> &'a UiOffer {
        let path = OfferPath::parse(&format!("project/demo.module/{node}/patch/{verb}")).unwrap();
        offers
            .get(&path)
            .unwrap_or_else(|| panic!("`{path}` is published"))
    }

    fn dome() -> NodeId {
        NodeId::new(2)
    }

    fn output() -> NodeId {
        NodeId::new(10)
    }

    fn port(key: u32, start: u32, lamps: u32, cells: Vec<UiPatchCell>) -> UiPatchPort {
        UiPatchPort {
            key,
            pin_label: format!("IO{key}"),
            start,
            lamps,
            cells,
        }
    }

    /// A manual dome with two sectors (one on the wire) and one unnamed
    /// output with one port.
    fn surface() -> UiPatchSurface {
        let cell = UiPatchCell {
            id: "2:0".to_string(),
            source_start: 0,
            lamps: 30,
            wire_start: 0,
            ..Default::default()
        };
        let instance = |path: &str, label: &str, start: u32, placed: bool| UiPatchInstance {
            path: path.to_string(),
            label: label.to_string(),
            start,
            lamps: 30,
            stride: 10,
            placed,
        };
        let mut output = UiPatchSurfaceOutput {
            node: output(),
            label: "out_a".to_string(),
            address: Some("/demo.module/out_a.output".to_string()),
            bay: UiPatchBay {
                ports: vec![port(0, 0, 60, vec![cell.clone()])],
                ..Default::default()
            },
            ..Default::default()
        };
        output.name_assign = Some((
            crate::ProjectSlotAddress::new(
                ProjectNodeAddress::parse("/demo.module/out_a.output").unwrap(),
                crate::ProjectSlotRoot::Def,
                lpc_model::SlotPath::parse("name.some").unwrap(),
            ),
            "1".to_string(),
        ));
        UiPatchSurface {
            fixtures: vec![UiPatchSurfaceFixture {
                node: dome(),
                label: "dome".to_string(),
                address: Some("/demo.module/dome.fixture".to_string()),
                patch: UiFixturePatch {
                    lamps: 60,
                    cells: vec![cell],
                    ..Default::default()
                },
                patch_artifact: Some(ArtifactLocation::file("/dome.patch.json")),
                manual_flow: true,
                instances: vec![
                    instance("/sector/1", "sector 1", 0, true),
                    instance("/sector/2", "sector 2", 30, false),
                ],
                ..Default::default()
            }],
            outputs: vec![output],
            ..Default::default()
        }
    }
}
