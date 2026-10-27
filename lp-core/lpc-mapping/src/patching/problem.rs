//! What patching reports instead of guessing (names.html "Problem"). Each
//! names the node or port it is about, and each is reported once.

use crate::part::TreeId;
use crate::patching::output_name::OutputName;
use crate::patching::patch_target::PatchTarget;
use crate::patching::port_key::PortKey;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Problem {
    /// A segment was reached with no output or no port in the cursor: the
    /// fixture says nowhere to start.
    NoStart,
    /// An override names a node that isn't in the tree. It is kept, never
    /// moved to another node.
    GoneNode(TreeId),
    /// A start names an output the patcher wasn't given.
    GoneOutput {
        output: OutputName,
        set_by: PatchTarget,
    },
    /// A start names a port its output doesn't have.
    GonePort {
        output: OutputName,
        port: PortKey,
        set_by: PatchTarget,
    },
    /// A segment set to start somewhere doesn't fit what is left of its
    /// port: it stays where it was set and lands as far as the port reaches.
    DoesntFit { segment: TreeId, port: PortKey },
    /// A segment fell past the output's last port: it, and everything after
    /// it that only follows on, lands nowhere until a port is set again.
    PastLastPort { segment: TreeId },
    /// A segment starts part-way into a lamp. It has its address; the
    /// engine refuses it.
    OffLampBoundary { segment: TreeId },
    /// Two segments' spans share channels on a port.
    Overlap {
        a: TreeId,
        b: TreeId,
        output: OutputName,
        port: PortKey,
    },
    /// A segment's order doesn't fit its colors.
    OrderMismatch { segment: TreeId },
}
