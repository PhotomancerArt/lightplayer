use core::any::Any;
use core::fmt;

use crate::ActionMeta;

pub trait UxOp: fmt::Debug + 'static {
    fn default_action_meta(&self) -> ActionMeta;
    fn clone_box(&self) -> Box<dyn UxOp>;
    fn eq_op(&self, other: &dyn UxOp) -> bool;
    fn as_any(&self) -> &dyn Any;
    fn into_any(self: Box<Self>) -> Box<dyn Any>;
}

impl Clone for Box<dyn UxOp> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}
