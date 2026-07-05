//! Shape-only resolution of the [`SlotPolicy`] governing a slot path.
//!
//! Unlike the data walkers in [`super::slot_lookup`], this walk consults only
//! the shape tree. It exists for mutate-time enforcement, where a slot edit
//! must be validated before any data exists at its path (missing map entries
//! and inactive enum variants are created or selected when the edit is
//! applied).

use crate::{LpType, SlotPath, SlotPathSegment, SlotPolicy, SlotShapeLookup, SlotShapeView};

/// Policy plus leaf value type resolved for one slot path.
#[derive(Clone, Debug, PartialEq)]
pub struct SlotPolicyResolution {
    /// Policy governing the path. See [`resolve_slot_policy`] for the
    /// inheritance rule.
    pub policy: SlotPolicy,
    /// Value type when the path lands on a value leaf (including custom
    /// shapes that project to a value leaf). `None` for structural targets
    /// (records, maps, options, enums, units).
    pub leaf_type: Option<LpType>,
}

/// Resolve the [`SlotPolicy`] governing `path` within `shape`.
///
/// # Inheritance rule
///
/// Policy is declared per record field ([`crate::SlotFieldShape::policy`]).
/// On the walk from the root to `path`, the innermost record field with a
/// declared policy governs paths into its subtree:
///
/// - A field whose policy differs from [`SlotPolicy::default()`]
///   (`writable_persisted`) counts as declaring a policy; every path into its
///   subtree is governed by it unless a deeper field declares its own.
/// - A field carrying the default policy inherits from the nearest ancestor
///   field with a declared policy. When no field on the walk declares one,
///   the default `writable_persisted` governs.
/// - Non-field segments (map keys, option `some`, enum variants) never carry
///   policy and pass the inherited policy through unchanged.
///
/// Because [`SlotPolicy`] is not optional on the field shape, a field that
/// explicitly declares `writable_persisted` is indistinguishable from one
/// that declares nothing, and therefore inherits.
///
/// The walk is shape-only: enum variant segments resolve against any declared
/// variant (not just the active one) and map key segments resolve for any
/// key. Returns `None` when the path does not resolve in the shape.
pub fn resolve_slot_policy<'s>(
    shape: SlotShapeView<'s>,
    registry: &'s (impl SlotShapeLookup + ?Sized),
    path: &SlotPath,
) -> Option<SlotPolicy> {
    resolve_slot_policy_and_leaf(shape, registry, path).map(|resolution| resolution.policy)
}

/// Resolve the governing [`SlotPolicy`] plus the leaf value type at `path`.
///
/// Same walk and inheritance rule as [`resolve_slot_policy`]; additionally
/// reports the [`LpType`] of the value leaf the path lands on, so callers can
/// type-check assignments without a second traversal.
pub fn resolve_slot_policy_and_leaf<'s>(
    shape: SlotShapeView<'s>,
    registry: &'s (impl SlotShapeLookup + ?Sized),
    path: &SlotPath,
) -> Option<SlotPolicyResolution> {
    walk_policy(shape, registry, path.segments(), SlotPolicy::default())
}

fn walk_policy<'s>(
    shape: SlotShapeView<'s>,
    registry: &'s (impl SlotShapeLookup + ?Sized),
    segments: &[SlotPathSegment],
    inherited: SlotPolicy,
) -> Option<SlotPolicyResolution> {
    let shape = resolve_projected_shape(shape, registry)?;
    let Some((head, tail)) = segments.split_first() else {
        return Some(SlotPolicyResolution {
            policy: inherited,
            leaf_type: shape.value_shape().map(|value| value.ty_owned()),
        });
    };

    match head {
        SlotPathSegment::Field(name) => {
            if let Some((_, field)) = shape.record_field_by_name(name) {
                let declared = field.policy();
                let governing = if declared.is_default() {
                    inherited
                } else {
                    declared
                };
                walk_policy(field.shape(), registry, tail, governing)
            } else if name.as_str() == "some"
                && let Some(some) = shape.option_some()
            {
                walk_policy(some, registry, tail, inherited)
            } else if let Some(variant) = shape.enum_variant_by_name(name) {
                walk_policy(variant.shape(), registry, tail, inherited)
            } else {
                None
            }
        }
        SlotPathSegment::Key(_) => walk_policy(shape.map_value()?, registry, tail, inherited),
    }
}

/// Chase `Ref` indirections and `Custom` projections to a concrete shape.
fn resolve_projected_shape<'s>(
    mut shape: SlotShapeView<'s>,
    registry: &'s (impl SlotShapeLookup + ?Sized),
) -> Option<SlotShapeView<'s>> {
    loop {
        if let Some(id) = shape.ref_id() {
            shape = registry.get_shape(id)?;
        } else if let Some(projected) = shape.custom_shape() {
            shape = projected;
        } else {
            return Some(shape);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slot::SlotPersistence;
    use crate::slot::shape::{field, field_with_policy, map, record, value};
    use crate::{
        ClockDef, LpType, SlotMapKeyShape, SlotShape, SlotShapeId, SlotShapeRegistry,
        StaticSlotShape,
    };
    use alloc::vec;

    #[test]
    fn root_field_policy_governs_its_leaf() {
        let (registry, id) = registry_with(record(vec![
            field_with_policy(
                "locked",
                value(LpType::F32),
                SlotPolicy::read_only_persisted(),
            ),
            field("free", value(LpType::Bool)),
        ]));

        let locked = resolve(&registry, id, "locked");
        assert_eq!(locked.policy, SlotPolicy::read_only_persisted());
        assert_eq!(locked.leaf_type, Some(LpType::F32));

        let free = resolve(&registry, id, "free");
        assert_eq!(free.policy, SlotPolicy::writable_persisted());
        assert_eq!(free.leaf_type, Some(LpType::Bool));
    }

    #[test]
    fn nested_composite_member_inherits_from_governing_field() {
        let (registry, id) = registry_with(record(vec![field_with_policy(
            "state",
            record(vec![field("inner", value(LpType::F32))]),
            SlotPolicy::read_only_transient(),
        )]));

        let inner = resolve(&registry, id, "state.inner");
        assert_eq!(inner.policy, SlotPolicy::read_only_transient());
        assert_eq!(inner.leaf_type, Some(LpType::F32));
    }

    #[test]
    fn innermost_declared_policy_overrides_ancestor() {
        let (registry, id) = registry_with(record(vec![field_with_policy(
            "outer",
            record(vec![field_with_policy(
                "inner",
                value(LpType::F32),
                SlotPolicy::writable_transient(),
            )]),
            SlotPolicy::read_only_persisted(),
        )]));

        let inner = resolve(&registry, id, "outer.inner");
        assert_eq!(inner.policy, SlotPolicy::writable_transient());
    }

    #[test]
    fn map_key_segments_pass_policy_through() {
        let (registry, id) = registry_with(record(vec![field_with_policy(
            "params",
            map(SlotMapKeyShape::String, value(LpType::F32)),
            SlotPolicy::read_only_transient(),
        )]));

        let entry = resolve(&registry, id, "params[gain]");
        assert_eq!(entry.policy, SlotPolicy::read_only_transient());
        assert_eq!(entry.leaf_type, Some(LpType::F32));
    }

    #[test]
    fn unresolvable_path_returns_none() {
        let (registry, id) = registry_with(record(vec![field("free", value(LpType::F32))]));
        let shape = registry.get_shape(id).expect("shape");

        assert_eq!(
            resolve_slot_policy(shape, &registry, &SlotPath::parse("missing").unwrap()),
            None
        );
        assert_eq!(
            resolve_slot_policy(shape, &registry, &SlotPath::parse("free.deeper").unwrap()),
            None
        );
    }

    #[test]
    fn clock_controls_fields_resolve_writable_transient() {
        let registry = SlotShapeRegistry::default();
        let shape = registry.get_shape(ClockDef::SHAPE_ID).expect("clock shape");

        let rate = resolve_slot_policy_and_leaf(
            shape,
            &registry,
            &SlotPath::parse("controls.rate").unwrap(),
        )
        .expect("controls.rate resolves");

        assert!(rate.policy.writable);
        assert_eq!(rate.policy.persistence, SlotPersistence::Transient);
        assert_eq!(rate.leaf_type, Some(LpType::F32));
    }

    fn registry_with(shape: SlotShape) -> (SlotShapeRegistry, SlotShapeId) {
        let id = SlotShapeId::from_static_name("test.policy_lookup.root");
        let mut registry = SlotShapeRegistry::default();
        registry.register_dynamic_shape(id, shape).unwrap();
        (registry, id)
    }

    fn resolve(registry: &SlotShapeRegistry, id: SlotShapeId, path: &str) -> SlotPolicyResolution {
        let shape = registry.get_shape(id).expect("root shape");
        resolve_slot_policy_and_leaf(shape, registry, &SlotPath::parse(path).unwrap())
            .expect("path resolves")
    }
}
