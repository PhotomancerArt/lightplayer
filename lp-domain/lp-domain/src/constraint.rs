//! Constraint: what values are legal for a Slot. See docs/design/lightplayer/quantity.md §5.
//!
//! TODO(quantity widening): Constraint currently F32-only; widen to LpsValue when LpsValueF32 gets serde.

use alloc::string::String;
use alloc::vec::Vec;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_constraint_round_trips() {
        let c = Constraint::Free;
        let s = serde_json::to_string(&c).unwrap();
        let back: Constraint = serde_json::from_str(&s).unwrap();
        assert_eq!(c, back);
    }

    #[test]
    fn range_constraint_round_trips() {
        let c = Constraint::Range {
            min: 0.0,
            max: 5.0,
            step: Some(0.1),
        };
        let s = serde_json::to_string(&c).unwrap();
        let back: Constraint = serde_json::from_str(&s).unwrap();
        assert_eq!(c, back);
    }

    #[test]
    fn range_omits_step_when_none() {
        let c = Constraint::Range {
            min: 0.0,
            max: 1.0,
            step: None,
        };
        let s = serde_json::to_string(&c).unwrap();
        assert!(!s.contains("step"));
    }

    #[test]
    fn choice_round_trips() {
        let c = Constraint::Choice {
            values: alloc::vec![0.0, 1.0, 2.0],
            labels: alloc::vec![
                String::from("low"),
                String::from("med"),
                String::from("high"),
            ],
        };
        let s = serde_json::to_string(&c).unwrap();
        let back: Constraint = serde_json::from_str(&s).unwrap();
        assert_eq!(c, back);
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Constraint {
    Free,
    Range {
        min: f32,
        max: f32,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        step: Option<f32>,
    },
    Choice {
        values: Vec<f32>,
        labels: Vec<String>,
    },
}
