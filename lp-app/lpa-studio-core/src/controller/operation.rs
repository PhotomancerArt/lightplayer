use core::any::Any;
use core::fmt;

use crate::{ActionClass, ActionMeta};

pub trait ControllerOp: fmt::Debug + 'static {
    fn default_action_meta(&self) -> ActionMeta;

    /// The sync-engine scheduling class for this operation.
    ///
    /// Declared per op (no default) so a new operation that omits a class is a
    /// compile error rather than silently defaulting. The actor reads this to
    /// decide preemption and to build the pull loop's quiet-gap deadline.
    fn action_class(&self) -> ActionClass;

    fn clone_box(&self) -> Box<dyn ControllerOp>;
    fn eq_op(&self, other: &dyn ControllerOp) -> bool;
    fn as_any(&self) -> &dyn Any;
    fn into_any(self: Box<Self>) -> Box<dyn Any>;
}

impl Clone for Box<dyn ControllerOp> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}
