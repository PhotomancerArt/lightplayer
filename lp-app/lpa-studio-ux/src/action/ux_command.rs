use crate::ActionKind;

pub trait UxCommand: Clone + Eq {
    fn action_kind(&self) -> ActionKind;
}
