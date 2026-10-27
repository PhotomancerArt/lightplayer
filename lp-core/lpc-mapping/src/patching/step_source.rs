//! Where a node's step for one cursor field came from: what an inspector
//! says beside a value ("from the flow", "set here").

use crate::patching::patch_target::PatchTarget;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum StepSource {
    /// Nothing set it: the cursor carried on (Keep).
    Follows,
    /// The node's own override.
    Override,
    /// A start: the fixture's, or the part's own.
    Start(PatchTarget),
    /// The flow of what it sits in: the fixture's, or its parent part's.
    Flow(PatchTarget),
}
