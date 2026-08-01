//! What an annotation selects: every target, one named target, or an axis predicate.
//!
//! Three forms, one vocabulary:
//!
//! ```text
//! @unsupported(*)                         every target
//! @broken(wasm.q32)                       one target, by canonical name
//! @unimplemented(float_mode=f32)          every f32 target
//! @unsupported(frontend!=lp, isa=xtensa)  conjunction; `!=` negates one term
//! ```
//!
//! When several annotations match a target, the **most specific** one decides —
//! see [`TargetSelector::specificity`] and `directive_disposition`.

use super::Target;
use super::target_axis::{Axis, AxisValue};

/// One `axis=value` / `axis!=value` term of a predicate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AxisPredicate {
    /// The value being tested for.
    pub value: AxisValue,
    /// `true` for `axis!=value`.
    pub negated: bool,
}

impl AxisPredicate {
    /// True if `target` satisfies this term.
    pub fn matches(&self, target: &Target) -> bool {
        self.value.holds_for(target) != self.negated
    }

    /// The axis this term constrains.
    pub fn axis(&self) -> Axis {
        self.value.axis()
    }
}

impl std::fmt::Display for AxisPredicate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let op = if self.negated { "!=" } else { "=" };
        write!(f, "{}{op}{}", self.axis().key(), self.value)
    }
}

/// The set of targets an annotation applies to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetSelector {
    /// `*` — every target, present and future.
    All,
    /// A canonical target name from [`Target::name`], e.g. `wasm.q32`.
    Name(String),
    /// A conjunction of axis terms; every term must hold. Never empty.
    Predicate(Vec<AxisPredicate>),
}

impl TargetSelector {
    /// True if this selector applies to `target`.
    pub fn matches(&self, target: &Target) -> bool {
        match self {
            TargetSelector::All => true,
            TargetSelector::Name(name) => *name == target.name(),
            TargetSelector::Predicate(terms) => terms.iter().all(|t| t.matches(target)),
        }
    }

    /// How specific this selector is; higher wins when several match.
    ///
    /// An exact target name is maximally specific — it names one target and
    /// nothing else, so it must be able to carve an exception out of any
    /// predicate (`@unimplemented(float_mode=f32)` for the family,
    /// `@broken(wasm.f32)` for the one that is differently wrong). A predicate
    /// scores its term count, so a longer conjunction beats a shorter one.
    /// `*` scores zero: it is the fallback every other form outranks.
    pub fn specificity(&self) -> u32 {
        match self {
            TargetSelector::All => 0,
            TargetSelector::Predicate(terms) => terms.len() as u32,
            TargetSelector::Name(_) => u32::MAX,
        }
    }
}

impl std::fmt::Display for TargetSelector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TargetSelector::All => f.write_str("*"),
            TargetSelector::Name(n) => f.write_str(n),
            TargetSelector::Predicate(terms) => {
                let parts: Vec<String> = terms.iter().map(|t| t.to_string()).collect();
                f.write_str(&parts.join(", "))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::targets::{ALL_TARGETS, Backend, FloatMode, Frontend};

    fn t(name: &str) -> &'static Target {
        Target::from_name(name).expect("registered target")
    }

    fn pred(value: AxisValue, negated: bool) -> TargetSelector {
        TargetSelector::Predicate(vec![AxisPredicate { value, negated }])
    }

    #[test]
    fn all_matches_every_registered_target() {
        for target in ALL_TARGETS {
            assert!(TargetSelector::All.matches(target), "{}", target.name());
        }
    }

    #[test]
    fn name_matches_exactly_one_target() {
        let sel = TargetSelector::Name("wasm.q32".to_string());
        let hits: Vec<String> = ALL_TARGETS
            .iter()
            .filter(|t| sel.matches(t))
            .map(|t| t.name())
            .collect();
        assert_eq!(hits, vec!["wasm.q32".to_string()]);
    }

    #[test]
    fn positive_predicate_matches_the_axis_family() {
        let sel = pred(AxisValue::FloatMode(FloatMode::F32), false);
        for target in ALL_TARGETS {
            assert_eq!(
                sel.matches(target),
                target.float_mode == FloatMode::F32,
                "{}",
                target.name()
            );
        }
    }

    #[test]
    fn negated_predicate_is_the_complement() {
        let sel = pred(AxisValue::Frontend(Frontend::Lp), true);
        assert!(sel.matches(t("wasm.q32")));
        assert!(sel.matches(t("wgpu.f32")));
        assert!(!sel.matches(t("rv32lpn.q32")));
        assert!(!sel.matches(t("xtlpn.q32")));
    }

    #[test]
    fn conjunction_requires_every_term() {
        let sel = TargetSelector::Predicate(vec![
            AxisPredicate {
                value: AxisValue::Frontend(Frontend::Lp),
                negated: true,
            },
            AxisPredicate {
                value: AxisValue::Backend(Backend::Wgpu),
                negated: true,
            },
        ]);
        assert!(sel.matches(t("wasm.q32")));
        assert!(sel.matches(t("interp.f32")));
        assert!(!sel.matches(t("wgpu.f32")), "wgpu excluded by second term");
        assert!(!sel.matches(t("rv32lpn.q32")), "lp excluded by first term");
    }

    #[test]
    fn specificity_orders_name_above_predicate_above_star() {
        let name = TargetSelector::Name("wasm.q32".to_string());
        let two = TargetSelector::Predicate(vec![
            AxisPredicate {
                value: AxisValue::Frontend(Frontend::Lp),
                negated: true,
            },
            AxisPredicate {
                value: AxisValue::Backend(Backend::Wgpu),
                negated: true,
            },
        ]);
        let one = pred(AxisValue::FloatMode(FloatMode::F32), false);
        assert!(name.specificity() > two.specificity());
        assert!(two.specificity() > one.specificity());
        assert!(one.specificity() > TargetSelector::All.specificity());
    }

    #[test]
    fn display_round_trips_the_written_form() {
        assert_eq!(TargetSelector::All.to_string(), "*");
        assert_eq!(
            TargetSelector::Name("xtn.q32".to_string()).to_string(),
            "xtn.q32"
        );
        assert_eq!(
            TargetSelector::Predicate(vec![
                AxisPredicate {
                    value: AxisValue::FloatMode(FloatMode::F32),
                    negated: false,
                },
                AxisPredicate {
                    value: AxisValue::Backend(Backend::Interp),
                    negated: true,
                },
            ])
            .to_string(),
            "float_mode=f32, backend!=interp"
        );
    }
}
