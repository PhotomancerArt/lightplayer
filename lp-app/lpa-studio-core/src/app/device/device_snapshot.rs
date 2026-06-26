use crate::{LinkSnapshot, ServerSnapshot};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceSnapshot {
    pub link: LinkSnapshot,
    pub server: ServerSnapshot,
}

impl DeviceSnapshot {
    pub fn new(link: LinkSnapshot, server: ServerSnapshot) -> Self {
        Self { link, server }
    }
}
