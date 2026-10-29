//! What one node of the tree did on the walk, and why: for an inspector,
//! and for tests.

use crate::part::TreeId;
use crate::patching::cursor::Cursor;
use crate::patching::direction::Direction;
use crate::patching::start::Start;
use crate::patching::step_source::StepSource;

/// One node's walk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Walked {
    pub id: TreeId,
    /// The step it took for each field (not taken when it is off).
    pub steps: Start,
    /// Where each step came from: output, port, channel.
    pub sources: [StepSource; 3],
    /// The cursor once its steps were taken.
    pub at: Cursor,
    pub off: bool,
    pub direction: Direction,
    pub offset: u32,
}
