//! Catalog-wide structural guardrails (binding/bus roadmap M5 phase 0,
//! ADR 2026-07-09-declarative-default-bindings).
//!
//! Walks every shape in the generated static catalog and asserts model
//! coherence rules that binding policy relies on. These rules catch metadata
//! rot at test time instead of letting a mislabeled slot silently generate
//! wrong wiring (the pre-M5 state: `ShaderState.output` sat `Local` for
//! months because nothing checked).

use std::collections::HashSet;

use lpc_model::slot_shapes::{static_slot_shape, static_slot_shape_ids, static_slot_shape_name};
use lpc_model::{
    BindingRef, SlotDirection, SlotShapeId, StaticLpType, StaticSlotFieldShape,
    StaticSlotShapeDescriptor,
};

/// Every field whose value carries a product (visual/control) must declare a
/// direction: products are dataflow endpoints, never `Local` bookkeeping.
/// Produced outputs and consumed inputs (`FixtureDef.input`) are both fine.
#[test]
fn product_carrying_fields_declare_a_direction() {
    let mut offenders = Vec::new();
    for &id in static_slot_shape_ids() {
        let Some(shape) = static_slot_shape(id) else {
            continue;
        };
        let name = static_slot_shape_name(id)
            .map(str::to_string)
            .unwrap_or_else(|| format!("{id:?}"));
        let mut visited = HashSet::new();
        walk(shape, &name, &mut visited, &mut |context, field| {
            if field_carries_product(field.shape, &mut HashSet::new())
                && field.semantics.direction == SlotDirection::Local
            {
                offenders.push(format!("{context}.{}", field.name));
            }
        });
    }
    assert!(
        offenders.is_empty(),
        "product-carrying slots must be #[slot(produced)] or #[slot(consumed)], found Local: {offenders:?}"
    );
}

/// Resolve whether a field's own shape is (or wraps) a product value.
/// Follows `Ref` through the catalog and unwraps `Option`/`Custom`; does NOT
/// descend into records or maps — nested records declare their own fields.
fn field_carries_product(
    shape: &'static StaticSlotShapeDescriptor,
    seen: &mut HashSet<SlotShapeId>,
) -> bool {
    match shape {
        StaticSlotShapeDescriptor::Value { shape } => {
            matches!(shape.ty, StaticLpType::Product(_))
        }
        StaticSlotShapeDescriptor::Ref { id } => {
            if !seen.insert(*id) {
                return false;
            }
            static_slot_shape(*id).is_some_and(|inner| field_carries_product(inner, seen))
        }
        StaticSlotShapeDescriptor::Option { some, .. } => field_carries_product(some, seen),
        StaticSlotShapeDescriptor::Custom { shape, .. } => field_carries_product(shape, seen),
        _ => false,
    }
}

/// Depth-first walk over a static descriptor tree, invoking `check` for every
/// record field (with a dotted context path for error messages). Follows
/// `Ref` nodes through the catalog once each (cycle-safe).
fn walk(
    shape: &'static StaticSlotShapeDescriptor,
    context: &str,
    visited: &mut HashSet<SlotShapeId>,
    check: &mut impl FnMut(&str, &'static StaticSlotFieldShape),
) {
    match shape {
        StaticSlotShapeDescriptor::Record { fields, .. } => {
            for field in *fields {
                check(context, field);
                walk(
                    field.shape,
                    &format!("{context}.{}", field.name),
                    visited,
                    check,
                );
            }
        }
        StaticSlotShapeDescriptor::Map { value, .. } => {
            walk(value, &format!("{context}[]"), visited, check);
        }
        StaticSlotShapeDescriptor::Option { some, .. } => walk(some, context, visited, check),
        StaticSlotShapeDescriptor::Enum { variants, .. } => {
            for variant in *variants {
                walk(
                    variant.shape,
                    &format!("{context}::{}", variant.name),
                    visited,
                    check,
                );
            }
        }
        StaticSlotShapeDescriptor::Custom { shape, .. } => walk(shape, context, visited, check),
        StaticSlotShapeDescriptor::Ref { id } => {
            if visited.insert(*id)
                && let Some(inner) = static_slot_shape(*id)
            {
                walk(inner, context, visited, check);
            }
        }
        StaticSlotShapeDescriptor::Unit { .. } | StaticSlotShapeDescriptor::Value { .. } => {}
    }
}

/// Every declared `default_bind` must parse as a `bus:` endpoint with the
/// real grammar (the derive macro can only check lexically — it cannot
/// depend on this crate).
#[test]
fn default_binds_parse_as_bus_endpoints() {
    let mut offenders = Vec::new();
    for &id in static_slot_shape_ids() {
        let Some(shape) = static_slot_shape(id) else {
            continue;
        };
        let name = static_slot_shape_name(id)
            .map(str::to_string)
            .unwrap_or_else(|| format!("{id:?}"));
        let mut visited = HashSet::new();
        walk(shape, &name, &mut visited, &mut |context, field| {
            if let Some(endpoint) = field.default_bind
                && !matches!(BindingRef::parse(endpoint), Ok(BindingRef::Bus(_)))
            {
                offenders.push(format!("{context}.{} = `{endpoint}`", field.name));
            }
        });
    }
    assert!(
        offenders.is_empty(),
        "default_bind endpoints must parse as bus refs: {offenders:?}"
    );
}

/// `default_bind` only makes sense on dataflow slots that the loader can
/// wire: it must accompany a declared direction.
#[test]
fn default_binds_require_a_declared_direction() {
    let mut offenders = Vec::new();
    for &id in static_slot_shape_ids() {
        let Some(shape) = static_slot_shape(id) else {
            continue;
        };
        let name = static_slot_shape_name(id)
            .map(str::to_string)
            .unwrap_or_else(|| format!("{id:?}"));
        let mut visited = HashSet::new();
        walk(shape, &name, &mut visited, &mut |context, field| {
            if field.default_bind.is_some() && field.semantics.direction == SlotDirection::Local {
                offenders.push(format!("{context}.{}", field.name));
            }
        });
    }
    assert!(
        offenders.is_empty(),
        "default_bind requires #[slot(produced)] or #[slot(consumed)]: {offenders:?}"
    );
}
