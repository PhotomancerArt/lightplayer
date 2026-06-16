use crate::HwAddress;

/// Debounced state transition for a button input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonEventKind {
    /// Button became stably pressed.
    Pressed,
    /// Button became stably released.
    Released,
}

/// Event produced by [`crate::ButtonInput`].
///
/// The sequence counter is local to the opened input and increments on each
/// debounced transition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ButtonEvent {
    source: HwAddress,
    sequence: u32,
    kind: ButtonEventKind,
}

impl ButtonEvent {
    pub fn new(source: HwAddress, sequence: u32, kind: ButtonEventKind) -> Self {
        Self {
            source,
            sequence,
            kind,
        }
    }

    pub fn source(&self) -> &HwAddress {
        &self.source
    }

    pub fn sequence(&self) -> u32 {
        self.sequence
    }

    pub fn kind(&self) -> ButtonEventKind {
        self.kind
    }
}
