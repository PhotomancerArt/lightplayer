//! What one node of a fixture's tree did on the patcher's walk, and why,
//! in the project's terms: the patcher's own record
//! (`lpc_mapping::patching::Walked`), its cursor naming the project's
//! output node and port key, and where a segment really landed.
//!
//! The patcher's rules are `lpc_mapping`'s and are tested there; the Graph
//! only shows what it answered.

use lpc_mapping::patching::{self, Direction, OutputName, Patching, PortKey, Start, StepSource};

use crate::domain::node::NodeId;
use crate::domain::part_ref::TreeRef;
use crate::domain::project::Project;

/// Where the walk is: an output, a port on it, a channel on that port.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Cursor {
    /// The output node, when one by the cursor's name is there.
    pub output: Option<NodeId>,
    /// The output's name, as the patcher holds it.
    pub output_name: Option<String>,
    /// The port's key.
    pub port: Option<String>,
    pub channel: u32,
}

/// What one node did on the walk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Walked {
    pub fixture: NodeId,
    pub node: TreeRef,
    /// The step it took for each field.
    pub steps: Start,
    /// Where each step came from: output, port, channel.
    pub sources: [StepSource; 3],
    /// The cursor once its steps were taken.
    pub at: Cursor,
    /// For a segment: where its run really started, after any wrap.
    pub landed: Option<Cursor>,
    /// For a segment: it moved on to the next port because it didn't fit.
    pub wrapped: bool,
    pub off: bool,
    pub direction: Direction,
    pub offset: u32,
}

impl Walked {
    /// The patcher's record of one node, in the project's terms.
    pub(crate) fn of(
        project: &Project,
        fixture: &NodeId,
        walked: &patching::Walked,
        patching: &Patching,
    ) -> Walked {
        let segment = patching.segment(&walked.id);
        Walked {
            fixture: fixture.clone(),
            node: TreeRef::new(fixture, walked.id.clone()),
            steps: walked.steps.clone(),
            sources: walked.sources.clone(),
            at: cursor(
                project,
                walked.at.output.as_ref(),
                walked.at.port,
                walked.at.channel,
            ),
            landed: segment.map(|s| {
                cursor(
                    project,
                    Some(&s.address.output),
                    Some(s.address.port),
                    s.address.channel,
                )
            }),
            wrapped: segment.is_some_and(|s| s.wrapped),
            off: walked.off,
            direction: walked.direction,
            offset: walked.offset,
        }
    }
}

fn cursor(
    project: &Project,
    output: Option<&OutputName>,
    port: Option<PortKey>,
    channel: u32,
) -> Cursor {
    Cursor {
        output: output.and_then(|o| project.output_named(o.as_str())),
        output_name: output.map(|o| o.as_str().to_string()),
        port: port.map(|p| p.0.to_string()),
        channel,
    }
}
