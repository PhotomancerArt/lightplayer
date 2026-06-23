use crate::LinkState;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LinkSnapshot {
    pub state: LinkState,
}

impl LinkSnapshot {
    pub fn new(state: LinkState) -> Self {
        Self { state }
    }
}
