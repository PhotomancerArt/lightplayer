//! A preset: a name, a colour and the controls it pins.
//!
//! A preset is partial. It pins only the controls it names; every other
//! control shows through from the layers beneath it. A preset holds no time
//! of its own: what turns it on is an [`crate::Activation`].

use std::collections::BTreeMap;

use crate::control::{ControlId, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PresetId(pub u32);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preset {
    pub id: PresetId,
    pub name: String,
    /// The colour the panel draws its chip and its pinned controls in.
    pub colour: &'static str,
    pub pins: BTreeMap<ControlId, Value>,
}
