use crate::HardwareAddress;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonEventKind {
    Pressed,
    Released,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ButtonEvent {
    source: HardwareAddress,
    sequence: u32,
    kind: ButtonEventKind,
}

impl ButtonEvent {
    pub fn new(source: HardwareAddress, sequence: u32, kind: ButtonEventKind) -> Self {
        Self {
            source,
            sequence,
            kind,
        }
    }

    pub fn source(&self) -> &HardwareAddress {
        &self.source
    }

    pub fn sequence(&self) -> u32 {
        self.sequence
    }

    pub fn kind(&self) -> ButtonEventKind {
        self.kind
    }
}
