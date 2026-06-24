use crate::{ControllerId, ControllerOp, UiAction};

//
//
pub trait Controller {
    type Op: ControllerOp;

    fn node_id(&self) -> ControllerId;

    fn action(&self, op: Self::Op) -> UiAction {
        UiAction::from_op(self.node_id(), op)
    }

    fn actions_from_ops(&self, ops: impl IntoIterator<Item = Self::Op>) -> Vec<UiAction> {
        ops.into_iter().map(|op| self.action(op)).collect()
    }
}
