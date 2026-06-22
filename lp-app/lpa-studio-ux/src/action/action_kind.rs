#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ActionKind {
    pub scope: &'static str,
    pub name: &'static str,
}

impl ActionKind {
    pub const fn new(scope: &'static str, name: &'static str) -> Self {
        Self { scope, name }
    }
}
