use crate::{ActionMeta, UxCommand};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AvailableAction<A> {
    pub command: A,
    pub meta: ActionMeta,
}

impl<A> AvailableAction<A> {
    pub fn new(command: A, meta: ActionMeta) -> Self {
        Self { command, meta }
    }

    pub fn map<B>(self, f: impl FnOnce(A) -> B) -> AvailableAction<B> {
        AvailableAction {
            command: f(self.command),
            meta: self.meta,
        }
    }
}

impl<A> AvailableAction<A>
where
    A: UxCommand,
{
    pub fn from_command(command: A, meta: ActionMeta) -> Self {
        debug_assert_eq!(command.action_kind(), meta.kind);
        Self { command, meta }
    }
}
