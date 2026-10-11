//! Properties, as data. The inspector is drawn from these and nothing else.
//!
//! A component *describes* the properties of each thing it makes — an id, a
//! label, a kind (count, number, flag) — and reads and writes their values.
//! The inspector knows nothing about lines or circles: it lays out whatever
//! descriptors it is handed. A new component kind adds descriptors, never
//! inspector code.
//!
//! **Many things selected:** the inspector shows the properties they ALL
//! have (same id), with a combined value ("mixed 24–40"), and an edit
//! applies to every one. Select five lines, press +, and each gains a lamp.
//!
//! Lamps have no properties of their own; a lamp's count is its line's.

use crate::fixture::Fixture;
use crate::target::Target;

/// What a property is: the data the inspector lays out.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PropDesc {
    /// Stable id. Two things share a property when their ids match.
    pub id: &'static str,
    pub label: &'static str,
    pub kind: PropKind,
    /// The − / = keys nudge this property.
    pub nudge: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PropKind {
    /// A whole number at least `min` (lamp count, ring count).
    Count { min: i64 },
    /// A measurement in fixture units.
    Number { min: f64, step: f64 },
    /// On or off.
    Flag,
}

/// One target's value for one property.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PropRaw {
    Count(i64),
    Number(f64),
    Flag(bool),
}

/// An edit, applied to every selected thing that has the property.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PropEdit {
    /// Add `n` steps (a count by `n`, a number by `n × step`).
    Nudge(i64),
    SetCount(i64),
    SetNumber(f64),
    SetFlag(bool),
}

/// A property's value across the whole selection.
#[derive(Debug, Clone, PartialEq)]
pub enum PropValue {
    /// `min == max` means everything agrees.
    Count {
        min: i64,
        max: i64,
    },
    Number {
        min: f64,
        max: f64,
    },
    /// Both nonzero means mixed.
    Flag {
        on: usize,
        off: usize,
    },
}

impl PropValue {
    pub fn is_mixed(&self) -> bool {
        match self {
            PropValue::Count { min, max } => min != max,
            PropValue::Number { min, max } => (max - min).abs() > 1e-9,
            PropValue::Flag { on, off } => *on > 0 && *off > 0,
        }
    }

    /// How the inspector writes it: `40`, `mixed 24–40`, `yes`, `mixed`.
    pub fn describe(&self) -> String {
        match self {
            PropValue::Count { min, max } if min == max => min.to_string(),
            PropValue::Count { min, max } => format!("mixed {min}–{max}"),
            PropValue::Number { min, max } if !self.is_mixed() => format!("{}", round1(*min)),
            PropValue::Number { min, max } => format!("mixed {}–{}", round1(*min), round1(*max)),
            PropValue::Flag { on, off } => match (on, off) {
                (_, 0) => "yes".into(),
                (0, _) => "no".into(),
                _ => "mixed".into(),
            },
        }
    }
}

fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

/// A property as the inspector shows it for the current selection.
#[derive(Debug, Clone, PartialEq)]
pub struct Prop {
    pub desc: PropDesc,
    pub value: PropValue,
    /// How many selected things it covers (all of them, for a common prop).
    pub applies_to: usize,
}

/// Properties every target in `selection` has, in the first one's order.
pub fn common_props(fixture: &Fixture, selection: &[Target]) -> Vec<Prop> {
    let Some(first) = selection.first() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (desc, _) in fixture.props_of(first) {
        let raws: Vec<PropRaw> = selection
            .iter()
            .filter_map(|t| {
                fixture
                    .props_of(t)
                    .into_iter()
                    .find(|(d, _)| d.id == desc.id)
                    .map(|(_, r)| r)
            })
            .collect();
        if raws.len() != selection.len() {
            continue;
        }
        out.push(Prop {
            desc,
            value: combine(&raws),
            applies_to: raws.len(),
        });
    }
    out
}

fn combine(raws: &[PropRaw]) -> PropValue {
    match raws[0] {
        PropRaw::Count(_) => {
            let (min, max) = raws.iter().fold((i64::MAX, i64::MIN), |(a, b), r| match r {
                PropRaw::Count(n) => (a.min(*n), b.max(*n)),
                _ => (a, b),
            });
            PropValue::Count { min, max }
        }
        PropRaw::Number(_) => {
            let (min, max) = raws.iter().fold((f64::MAX, f64::MIN), |(a, b), r| match r {
                PropRaw::Number(n) => (a.min(*n), b.max(*n)),
                _ => (a, b),
            });
            PropValue::Number { min, max }
        }
        PropRaw::Flag(_) => {
            let on = raws
                .iter()
                .filter(|r| matches!(r, PropRaw::Flag(true)))
                .count();
            PropValue::Flag {
                on,
                off: raws.len() - on,
            }
        }
    }
}

/// The new raw value an edit produces from `current`, clamped by `desc`.
pub fn edited(desc: &PropDesc, current: PropRaw, edit: PropEdit) -> PropRaw {
    match (desc.kind, current, edit) {
        (PropKind::Count { min }, PropRaw::Count(n), PropEdit::Nudge(d)) => {
            PropRaw::Count((n + d).max(min))
        }
        (PropKind::Count { min }, PropRaw::Count(_), PropEdit::SetCount(v)) => {
            PropRaw::Count(v.max(min))
        }
        (PropKind::Number { min, step }, PropRaw::Number(v), PropEdit::Nudge(d)) => {
            PropRaw::Number((v + d as f64 * step).max(min))
        }
        (PropKind::Number { min, .. }, PropRaw::Number(_), PropEdit::SetNumber(v)) => {
            PropRaw::Number(v.max(min))
        }
        (PropKind::Flag, PropRaw::Flag(_), PropEdit::SetFlag(v)) => PropRaw::Flag(v),
        _ => current,
    }
}
