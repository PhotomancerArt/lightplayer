//! A lighting control on the panel, and the values it can hold.
//!
//! Every control a preset can pin is one of these. System settings (access,
//! Wi‑Fi) are not controls and never appear here.

/// A control's name on the panel: `power`, `brightness`, `pattern`, …
pub type ControlId = &'static str;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    /// On or off (`power`).
    On(bool),
    /// A level in percent (`brightness`, `speed`).
    Level(u8),
    /// One of a set of names (`palette`).
    Choice(&'static str),
    /// The playlist's intent (`pattern`): hold one pattern, or cycle through
    /// a set. Where the cycle currently is stays runtime state; what was
    /// asked for is the value.
    Hold(&'static str),
    Cycle(Vec<&'static str>),
}

impl Value {
    pub fn label(&self) -> String {
        match self {
            Value::On(true) => "on".into(),
            Value::On(false) => "off".into(),
            Value::Level(p) => format!("{p} %"),
            Value::Choice(c) | Value::Hold(c) => (*c).into(),
            Value::Cycle(set) => format!("cycle {}", set.join(", ")),
        }
    }
}
