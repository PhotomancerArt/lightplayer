use crate::{ConnectFlowState, ServerSnapshot};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceSnapshot {
    pub flow: ConnectFlowState,
    pub server: ServerSnapshot,
}

impl DeviceSnapshot {
    pub fn new(flow: ConnectFlowState, server: ServerSnapshot) -> Self {
        Self { flow, server }
    }
}
