//! Starts and flows as they are written: one op per cursor field, amounts
//! with their unit, flows by name.
//!
//! Externally tagged throughout (the firmware's serde rule): a unit variant
//! is a bare string (`"keep"`, `"end_to_end"`), a data variant an object of
//! one key (`{"set": 0}`, `{"one_per_port": {"from": 0}}`), an amount
//! `{"lamps": 4}` or `{"channels": 12}`.

use alloc::string::String;

use serde::{Deserialize, Serialize};

/// A start: each field absent is Keep.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub(crate) struct StartDoc {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<OutputOpDoc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<PortOpDoc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel: Option<ChannelOpDoc>,
}

impl StartDoc {
    pub fn is_keep(&self) -> bool {
        self.output.is_none() && self.port.is_none() && self.channel.is_none()
    }
}

/// `"keep"` or `{"set": "main"}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum OutputOpDoc {
    Keep,
    Set(String),
}

/// `"keep"`, `{"set": 0}` or `{"add": 1}`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PortOpDoc {
    Keep,
    Set(u32),
    Add(u32),
}

/// `"keep"`, `{"set": {"lamps": 0}}` or `{"add": {"channels": 13}}`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ChannelOpDoc {
    Keep,
    Set(AmountDoc),
    Add(AmountDoc),
}

/// `{"lamps": n}` or `{"channels": n}`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AmountDoc {
    Lamps(u32),
    Channels(u32),
}

/// `"end_to_end"` or `{"one_per_port": {"from": 0}}`.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FlowDoc {
    #[default]
    EndToEnd,
    OnePerPort {
        from: u32,
    },
}

impl FlowDoc {
    pub fn is_end_to_end(&self) -> bool {
        *self == FlowDoc::EndToEnd
    }
}
