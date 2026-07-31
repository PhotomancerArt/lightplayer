//! The five axes of [`Target`], and the values each one can take.
//!
//! Axis names and axis values are **derived from the target model**, not from a
//! parallel table: an axis name is a field of [`Target`], and an axis value is
//! the `Display` form of that field's enum. Adding an enum variant fails to
//! compile in [`display`](super::display) until it has a name, and
//! [`Axis::values`] is proved complete against [`ALL_TARGETS`] by test.
//!
//! This is what stops the annotation vocabulary from drifting away from the
//! targets it describes.

use super::{Backend, ExecMode, FloatMode, Frontend, Isa, Target};

/// One selectable axis of a [`Target`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Axis {
    /// GLSL frontend (`frontend=naga`, `frontend=lp`).
    Frontend,
    /// Compilation/execution backend (`backend=wasm`, `backend=interp`, …).
    Backend,
    /// Float representation (`float_mode=q32`, `float_mode=f32`).
    FloatMode,
    /// Instruction set (`isa=riscv32`, `isa=xtensa`, `isa=wasm32`, `isa=host`).
    Isa,
    /// How the artifact executes (`exec_mode=emulator`, `interpreter`, `gpu`).
    ExecMode,
}

/// Every axis, in the field order of [`Target`].
pub const ALL_AXES: &[Axis] = &[
    Axis::Frontend,
    Axis::Backend,
    Axis::FloatMode,
    Axis::Isa,
    Axis::ExecMode,
];

impl Axis {
    /// The axis keyword used in annotations (`frontend`, `backend`, …).
    pub fn key(self) -> &'static str {
        match self {
            Axis::Frontend => "frontend",
            Axis::Backend => "backend",
            Axis::FloatMode => "float_mode",
            Axis::Isa => "isa",
            Axis::ExecMode => "exec_mode",
        }
    }

    /// Parse an axis keyword.
    pub fn from_key(s: &str) -> Option<Axis> {
        ALL_AXES.iter().copied().find(|a| a.key() == s)
    }

    /// Every legal value of this axis, in declaration order.
    pub fn values(self) -> Vec<AxisValue> {
        match self {
            Axis::Frontend => Frontend::ALL
                .iter()
                .copied()
                .map(AxisValue::Frontend)
                .collect(),
            Axis::Backend => Backend::ALL
                .iter()
                .copied()
                .map(AxisValue::Backend)
                .collect(),
            Axis::FloatMode => FloatMode::ALL
                .iter()
                .copied()
                .map(AxisValue::FloatMode)
                .collect(),
            Axis::Isa => Isa::ALL.iter().copied().map(AxisValue::Isa).collect(),
            Axis::ExecMode => ExecMode::ALL
                .iter()
                .copied()
                .map(AxisValue::ExecMode)
                .collect(),
        }
    }

    /// Parse a value of this axis; `None` if no such value exists.
    pub fn value_from_str(self, s: &str) -> Option<AxisValue> {
        self.values().into_iter().find(|v| v.name() == s)
    }

    /// Comma-separated list of legal values, for error messages.
    pub fn value_list(self) -> String {
        self.values()
            .iter()
            .map(|v| v.name())
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Comma-separated list of legal axis keys, for error messages.
    pub fn key_list() -> String {
        ALL_AXES
            .iter()
            .map(|a| a.key())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// A value on one axis, tagged with the axis it belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AxisValue {
    /// A [`Frontend`] value.
    Frontend(Frontend),
    /// A [`Backend`] value.
    Backend(Backend),
    /// A [`FloatMode`] value.
    FloatMode(FloatMode),
    /// An [`Isa`] value.
    Isa(Isa),
    /// An [`ExecMode`] value.
    ExecMode(ExecMode),
}

impl AxisValue {
    /// The axis this value belongs to.
    pub fn axis(self) -> Axis {
        match self {
            AxisValue::Frontend(_) => Axis::Frontend,
            AxisValue::Backend(_) => Axis::Backend,
            AxisValue::FloatMode(_) => Axis::FloatMode,
            AxisValue::Isa(_) => Axis::Isa,
            AxisValue::ExecMode(_) => Axis::ExecMode,
        }
    }

    /// The value's spelling in annotations (its `Display` form).
    pub fn name(self) -> String {
        match self {
            AxisValue::Frontend(v) => v.to_string(),
            AxisValue::Backend(v) => v.to_string(),
            AxisValue::FloatMode(v) => v.to_string(),
            AxisValue::Isa(v) => v.to_string(),
            AxisValue::ExecMode(v) => v.to_string(),
        }
    }

    /// True if `target` carries this value on this value's axis.
    pub fn holds_for(self, target: &Target) -> bool {
        match self {
            AxisValue::Frontend(v) => target.frontend == v,
            AxisValue::Backend(v) => target.backend == v,
            AxisValue::FloatMode(v) => target.float_mode == v,
            AxisValue::Isa(v) => target.isa == v,
            AxisValue::ExecMode(v) => target.exec_mode == v,
        }
    }
}

impl std::fmt::Display for AxisValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.name())
    }
}

/// Read an axis value off a concrete target.
pub fn axis_value_of(axis: Axis, target: &Target) -> AxisValue {
    match axis {
        Axis::Frontend => AxisValue::Frontend(target.frontend),
        Axis::Backend => AxisValue::Backend(target.backend),
        Axis::FloatMode => AxisValue::FloatMode(target.float_mode),
        Axis::Isa => AxisValue::Isa(target.isa),
        Axis::ExecMode => AxisValue::ExecMode(target.exec_mode),
    }
}

#[cfg(test)]
mod tests {
    use super::super::ALL_TARGETS;
    use super::*;

    #[test]
    fn every_axis_key_round_trips() {
        for &axis in ALL_AXES {
            assert_eq!(Axis::from_key(axis.key()), Some(axis), "{}", axis.key());
        }
        assert_eq!(Axis::from_key("nope"), None);
    }

    #[test]
    fn every_axis_value_round_trips() {
        for &axis in ALL_AXES {
            for value in axis.values() {
                assert_eq!(value.axis(), axis);
                assert_eq!(
                    axis.value_from_str(&value.name()),
                    Some(value),
                    "{}={}",
                    axis.key(),
                    value.name()
                );
            }
        }
    }

    /// The vocabulary must cover the model: every axis value actually present in
    /// [`ALL_TARGETS`] has to be namable. A new enum variant that reaches a
    /// registered target but never reaches `Axis::values` fails here rather than
    /// by rejecting a legitimate annotation at parse time.
    #[test]
    fn every_registered_target_is_fully_namable() {
        for target in ALL_TARGETS {
            for &axis in ALL_AXES {
                let actual = axis_value_of(axis, target);
                assert!(
                    axis.values().contains(&actual),
                    "{} carries {}={} which Axis::values does not list",
                    target.name(),
                    axis.key(),
                    actual.name()
                );
                assert!(actual.holds_for(target));
            }
        }
    }

    /// Values are unique within an axis; two variants sharing a `Display` name
    /// would make one of them unselectable.
    #[test]
    fn axis_values_are_unique_within_an_axis() {
        for &axis in ALL_AXES {
            let mut names: Vec<String> = axis.values().iter().map(|v| v.name()).collect();
            let total = names.len();
            names.sort();
            names.dedup();
            assert_eq!(names.len(), total, "duplicate value name on {}", axis.key());
        }
    }
}
