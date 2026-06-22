use crate::{UxAction, UxNodeId, UxOp};

pub trait UxNode {
    type Op: UxOp;

    fn node_id(&self) -> UxNodeId;

    fn action(&self, op: Self::Op) -> UxAction {
        UxAction::from_op(self.node_id(), op)
    }

    fn actions_from_ops(&self, ops: impl IntoIterator<Item = Self::Op>) -> Vec<UxAction> {
        ops.into_iter().map(|op| self.action(op)).collect()
    }
}
