//! Memory guardrails for a shader's declared slot interface.
//!
//! A VALVE against authoring accidents, not a per-board memory model: an
//! authored `len` of a billion must fail with a named diagnostic instead
//! of reaching a `Vec` resize or the VMContext sizing. The default is
//! deliberately one round number; board-aware construction can replace
//! `default()` later without touching the enforcement seams
//! (`docs/debt/shader-budget-is-a-fixed-default.md`).

use alloc::string::String;

use crate::{LpType, ShaderSlotDef, ShaderSlotKind, SlotShapeLookup, SlotShapeRegistry};

/// Memory guardrails for one shader's declared slot interface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShaderBudget {
    /// Max total declared slot bytes (consumed + produced) per shader.
    pub max_slot_bytes: u32,
}

impl Default for ShaderBudget {
    /// 10 KiB — small enough to catch every accident, generous enough that
    /// no legitimate effect authored to date comes near it.
    fn default() -> Self {
        Self {
            max_slot_bytes: 10 * 1024,
        }
    }
}

/// A shader's declared slots exceed the budget.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShaderBudgetError {
    /// The slot whose addition crossed the line (accounting is cumulative,
    /// so this is the diagnostic anchor, not the sole offender).
    pub slot: String,
    /// Total declared bytes including `slot`.
    pub bytes: u64,
    /// The budget that refused it.
    pub budget: u32,
}

impl core::fmt::Display for ShaderBudgetError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "shader slots declare {} bytes at slot {:?}, over the {} byte budget",
            self.bytes, self.slot, self.budget
        )
    }
}

impl core::error::Error for ShaderBudgetError {}

/// Validate the total declared bytes of `slots` against `budget`.
///
/// Estimation, not layout accounting: packed byte sizes, no stride or
/// alignment. Exactness is not the goal — catching `len: 1000000000` is —
/// so an unresolvable element type simply contributes zero rather than
/// failing validation on the budget's behalf.
pub fn validate_shader_slot_budget<'a>(
    slots: impl Iterator<Item = (&'a str, &'a ShaderSlotDef)>,
    registry: &SlotShapeRegistry,
    budget: &ShaderBudget,
) -> Result<(), ShaderBudgetError> {
    let mut total: u64 = 0;
    for (name, slot) in slots {
        total = total.saturating_add(slot_bytes_estimate(slot, registry));
        if total > u64::from(budget.max_slot_bytes) {
            return Err(ShaderBudgetError {
                slot: String::from(name),
                bytes: total,
                budget: budget.max_slot_bytes,
            });
        }
    }
    Ok(())
}

/// Declared bytes one slot asks the engine/ABI to carry.
pub fn slot_bytes_estimate(slot: &ShaderSlotDef, registry: &SlotShapeRegistry) -> u64 {
    match slot.kind.value() {
        // One value; 16 B covers the widest scalar family a value slot
        // declares today (vec4).
        ShaderSlotKind::Value
        | ShaderSlotKind::Phasor
        | ShaderSlotKind::Seconds
        | ShaderSlotKind::Palette => 16,
        ShaderSlotKind::Buffer => {
            let lanes = slot
                .value_lp_type()
                .as_ref()
                .and_then(crate::BufferElem::from_lp_type)
                .map_or(1, |elem| u64::from(elem.word_stride()));
            u64::from(slot.buffer_len().unwrap_or(0)) * lanes * 4
        }
        ShaderSlotKind::Map => {
            let len = slot
                .mapping
                .data
                .as_ref()
                .map_or(0, |mapping| u64::from(*mapping.len.value()));
            len * element_bytes_estimate(slot, registry)
        }
    }
}

fn element_bytes_estimate(slot: &ShaderSlotDef, registry: &SlotShapeRegistry) -> u64 {
    let ty = match slot.value.value().as_lp_type() {
        Some(ty) => ty,
        None => {
            let id = crate::SlotShapeId::from_static_name(slot.value.value().as_str());
            match registry
                .get_shape(id)
                .and_then(|shape| shape.value_shape().map(|shape| shape.ty_owned()))
            {
                Some(ty) => ty,
                // Unknown shape: the budget is not the place to diagnose it
                // — header gen / desc build refuse it by name.
                None => return 0,
            }
        }
    };
    lp_type_bytes_estimate(&ty)
}

fn lp_type_bytes_estimate(ty: &LpType) -> u64 {
    match ty {
        LpType::I32 | LpType::U32 | LpType::F32 | LpType::Bool => 4,
        LpType::Vec2 | LpType::IVec2 | LpType::UVec2 | LpType::BVec2 => 8,
        LpType::Vec3 | LpType::IVec3 | LpType::UVec3 | LpType::BVec3 => 12,
        LpType::Vec4 | LpType::IVec4 | LpType::UVec4 | LpType::BVec4 => 16,
        LpType::Mat2x2 => 16,
        LpType::Mat3x3 => 36,
        LpType::Mat4x4 => 64,
        LpType::Buffer { elem, len } => u64::from(*len) * u64::from(elem.word_stride()) * 4,
        LpType::Array(element, len) => lp_type_bytes_estimate(element) * (*len as u64),
        LpType::Struct { fields, .. } => fields
            .iter()
            .map(|field| lp_type_bytes_estimate(&field.ty))
            .sum(),
        // Not shader-declarable data; contributes nothing here.
        LpType::Any
        | LpType::String
        | LpType::List(_)
        | LpType::Enum { .. }
        | LpType::Resource
        | LpType::Product(_) => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ShaderSlotDef, ShaderSlotMappingDef};
    use alloc::string::ToString;

    #[test]
    fn absurd_sentinel_len_fails_the_default_budget() {
        let slot = ShaderSlotDef::map_u32_native(
            "lp::fluid::Emitter",
            ShaderSlotMappingDef::sentinel(1_000_000_000, "id", 0),
        );
        let registry = crate::SlotShapeRegistry::default();

        let err = validate_shader_slot_budget(
            core::iter::once(("emitters", &slot)),
            &registry,
            &ShaderBudget::default(),
        )
        .expect_err("over budget");

        assert_eq!(err.slot, "emitters".to_string());
        assert!(err.bytes > u64::from(err.budget));
    }

    #[test]
    fn budget_is_cumulative_across_slots() {
        // Two buffers, each under the line alone, together over it.
        let a = ShaderSlotDef::buffer_builtin("f32", 1500);
        let b = ShaderSlotDef::buffer_builtin("f32", 1500);
        let registry = crate::SlotShapeRegistry::default();

        assert!(
            validate_shader_slot_budget(
                core::iter::once(("a", &a)),
                &registry,
                &ShaderBudget::default(),
            )
            .is_ok()
        );
        let err = validate_shader_slot_budget(
            [("a", &a), ("b", &b)].into_iter(),
            &registry,
            &ShaderBudget::default(),
        )
        .expect_err("cumulative");
        assert_eq!(err.slot, "b".to_string());
    }

    #[test]
    fn reasonable_declarations_pass() {
        let heat = ShaderSlotDef::buffer_builtin("f32", 300);
        let meteors = ShaderSlotDef::map_u32_native(
            "lp::fluid::Emitter",
            ShaderSlotMappingDef::sentinel(4, "id", 0),
        );
        let registry = crate::SlotShapeRegistry::default();

        assert!(
            validate_shader_slot_budget(
                [("heat", &heat), ("meteors", &meteors)].into_iter(),
                &registry,
                &ShaderBudget::default(),
            )
            .is_ok()
        );
    }
}
