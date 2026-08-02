//! Shape-only resolution of the [`SlotRole`] governing a slot path.
//!
//! Unlike the data walkers in [`super::slot_lookup`], this walk consults only
//! the shape tree. It exists for mutate-time enforcement, where a slot edit
//! must be validated before any data exists at its path (missing map entries
//! and inactive enum variants are created or selected when the edit is
//! applied).

use crate::{
    LpType, SlotDirection, SlotPath, SlotPathSegment, SlotRole, SlotSemantics, SlotShapeLookup,
    SlotShapeView,
    slot::{SlotPersistence, effective_persistence, effective_writable},
};

/// Role, governing direction, and leaf value type resolved for one slot path.
#[derive(Clone, Debug, PartialEq)]
pub struct SlotRoleResolution {
    /// Role governing the path. See [`resolve_slot_role`] for the
    /// inheritance rule.
    pub role: SlotRole,
    /// Dataflow direction governing the path, inherited the same way as
    /// `role`. A produced direction always wins over the role: see
    /// [`Self::is_writable`].
    pub direction: SlotDirection,
    /// Value type when the path lands on a value leaf (including custom
    /// shapes that project to a value leaf). `None` for structural targets
    /// (records, maps, options, enums, units).
    pub leaf_type: Option<LpType>,
}

impl SlotRoleResolution {
    /// Whether a client may request mutation of this path: the role allows
    /// it and the path is not produced (D1 — direction implies read-only
    /// regardless of role).
    pub fn is_writable(&self) -> bool {
        effective_writable(self.role, self.direction)
    }

    /// The persistence classification governing this path
    /// ([`effective_persistence`] of its role and direction).
    ///
    /// This is the **one** classifier both sides of an edit's life must
    /// consult — the studio for display/dirty accounting, the registry for
    /// commit retention — so a Debug override and an authored edit can never
    /// swap places between client and server. Paths that resolve in no shape
    /// take [`SlotPersistence::for_unresolved_edit`] instead.
    pub fn persistence(&self) -> SlotPersistence {
        effective_persistence(self.role, self.direction)
    }
}

/// Resolve the [`SlotRole`] plus governing direction and leaf type for `path`
/// within `shape`.
///
/// # Inheritance rule
///
/// Role and direction are declared per record field
/// ([`crate::SlotFieldShape::role`], [`crate::SlotFieldShape::semantics`]).
/// On the walk from the root to `path`, the innermost record field with a
/// declared value governs paths into its subtree, tracked independently for
/// role and direction:
///
/// - A field whose role differs from [`SlotRole::default()`] (`Setting`)
///   counts as declaring a role; every path into its subtree is governed by
///   it unless a deeper field declares its own. The same rule applies to
///   direction against [`SlotDirection::default()`] (`Local`).
/// - A field carrying the default role/direction inherits from the nearest
///   ancestor field with a declared value. When no field on the walk
///   declares one, the defaults govern.
/// - Non-field segments (map keys, option `some`, enum variants) never carry
///   role or direction and pass the inherited values through unchanged.
///
/// Because neither [`SlotRole`] nor [`SlotDirection`] is optional on the
/// field shape, a field that explicitly declares the default value is
/// indistinguishable from one that declares nothing, and therefore inherits.
///
/// The walk is shape-only: enum variant segments resolve against any declared
/// variant (not just the active one) and map key segments resolve for any
/// key. Returns `None` when the path does not resolve in the shape.
pub fn resolve_slot_role<'s>(
    shape: SlotShapeView<'s>,
    registry: &'s (impl SlotShapeLookup + ?Sized),
    path: &SlotPath,
) -> Option<SlotRoleResolution> {
    walk_role(
        shape,
        registry,
        path.segments(),
        SlotRole::default(),
        SlotDirection::default(),
    )
}

fn walk_role<'s>(
    shape: SlotShapeView<'s>,
    registry: &'s (impl SlotShapeLookup + ?Sized),
    segments: &[SlotPathSegment],
    inherited_role: SlotRole,
    inherited_direction: SlotDirection,
) -> Option<SlotRoleResolution> {
    let shape = resolve_projected_shape(shape, registry)?;
    let Some((head, tail)) = segments.split_first() else {
        return Some(SlotRoleResolution {
            role: inherited_role,
            direction: inherited_direction,
            leaf_type: shape.value_shape().map(|value| value.ty_owned()),
        });
    };

    match head {
        SlotPathSegment::Field(name) => {
            if let Some((_, field)) = shape.record_field_by_name(name) {
                let declared_role = field.role();
                let governing_role = if declared_role.is_default() {
                    inherited_role
                } else {
                    declared_role
                };
                let declared_direction = field.semantics().direction;
                let governing_direction =
                    if declared_direction == SlotSemantics::default().direction {
                        inherited_direction
                    } else {
                        declared_direction
                    };
                walk_role(
                    field.shape(),
                    registry,
                    tail,
                    governing_role,
                    governing_direction,
                )
            } else if name.as_str() == "some"
                && let Some(some) = shape.option_some()
            {
                walk_role(some, registry, tail, inherited_role, inherited_direction)
            } else if let Some(variant) = shape.enum_variant_by_name(name) {
                walk_role(
                    variant.shape(),
                    registry,
                    tail,
                    inherited_role,
                    inherited_direction,
                )
            } else {
                None
            }
        }
        SlotPathSegment::Key(_) => walk_role(
            shape.map_value()?,
            registry,
            tail,
            inherited_role,
            inherited_direction,
        ),
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
    use crate::slot::shape::{field, field_with_role, map, record, value};
    use crate::{
        ClockDef, LpType, SlotMapKeyShape, SlotShape, SlotShapeId, SlotShapeRegistry,
        StaticSlotShape, slot::effective_persistence,
    };
    use alloc::vec;

    #[test]
    fn root_field_role_governs_its_leaf() {
        let (registry, id) = registry_with(record(vec![
            field_with_role("locked", value(LpType::F32), SlotRole::Fixed),
            field("free", value(LpType::Bool)),
        ]));

        let locked = resolve(&registry, id, "locked");
        assert_eq!(locked.role, SlotRole::Fixed);
        assert_eq!(locked.leaf_type, Some(LpType::F32));

        let free = resolve(&registry, id, "free");
        assert_eq!(free.role, SlotRole::Setting);
        assert_eq!(free.leaf_type, Some(LpType::Bool));
    }

    #[test]
    fn nested_composite_member_inherits_from_governing_field() {
        let (registry, id) = registry_with(record(vec![field_with_role(
            "state",
            record(vec![field("inner", value(LpType::F32))]),
            SlotRole::Fixed,
        )]));

        let inner = resolve(&registry, id, "state.inner");
        assert_eq!(inner.role, SlotRole::Fixed);
        assert_eq!(inner.leaf_type, Some(LpType::F32));
    }

    #[test]
    fn innermost_declared_role_overrides_ancestor() {
        let (registry, id) = registry_with(record(vec![field_with_role(
            "outer",
            record(vec![field_with_role(
                "inner",
                value(LpType::F32),
                SlotRole::Debug,
            )]),
            SlotRole::Fixed,
        )]));

        let inner = resolve(&registry, id, "outer.inner");
        assert_eq!(inner.role, SlotRole::Debug);
    }

    #[test]
    fn map_key_segments_pass_role_through() {
        let (registry, id) = registry_with(record(vec![field_with_role(
            "params",
            map(SlotMapKeyShape::String, value(LpType::F32)),
            SlotRole::Debug,
        )]));

        let entry = resolve(&registry, id, "params[gain]");
        assert_eq!(entry.role, SlotRole::Debug);
        assert_eq!(entry.leaf_type, Some(LpType::F32));
    }

    #[test]
    fn unresolvable_path_returns_none() {
        let (registry, id) = registry_with(record(vec![field("free", value(LpType::F32))]));
        let shape = registry.get_shape(id).expect("shape");

        assert_eq!(
            resolve_slot_role(shape, &registry, &SlotPath::parse("missing").unwrap()),
            None
        );
        assert_eq!(
            resolve_slot_role(shape, &registry, &SlotPath::parse("free.deeper").unwrap()),
            None
        );
    }

    #[test]
    fn clock_controls_fields_resolve_debug_role() {
        let registry = SlotShapeRegistry::default();
        let shape = registry.get_shape(ClockDef::SHAPE_ID).expect("clock shape");

        let rate = resolve_slot_role(shape, &registry, &SlotPath::parse("controls.rate").unwrap())
            .expect("controls.rate resolves");

        assert_eq!(rate.role, SlotRole::Debug);
        assert!(rate.is_writable());
        assert_eq!(rate.leaf_type, Some(LpType::F32));
    }

    #[test]
    fn produced_state_field_resolves_read_only_and_transient() {
        let (registry, id) = registry_with(record(vec![state_field("output", value(LpType::F32))]));

        let output = resolve(&registry, id, "output");
        assert_eq!(output.direction, SlotDirection::Produced);
        assert_eq!(output.role, SlotRole::State);
        assert!(!output.is_writable(), "produced fields are never writable");
        assert_eq!(output.persistence(), SlotPersistence::Transient);
    }

    #[test]
    fn produced_state_is_inherited_by_nested_leaves() {
        let (registry, id) = registry_with(record(vec![state_field(
            "output",
            record(vec![field("inner", value(LpType::F32))]),
        )]));

        let inner = resolve(&registry, id, "output.inner");
        assert_eq!(inner.direction, SlotDirection::Produced);
        assert_eq!(inner.role, SlotRole::State);
        assert!(!inner.is_writable());
        assert_eq!(inner.persistence(), SlotPersistence::Transient);
    }

    /// The G2 amendment must not move any existing classification: a
    /// `State`/`Produced` field resolves exactly as an unmarked produced
    /// field used to.
    #[test]
    fn state_role_classifies_like_the_former_unmarked_produced_field() {
        assert_eq!(
            effective_writable(SlotRole::State, SlotDirection::Produced),
            effective_writable(SlotRole::Setting, SlotDirection::Produced),
        );
        assert_eq!(
            effective_persistence(SlotRole::State, SlotDirection::Produced),
            effective_persistence(SlotRole::Setting, SlotDirection::Produced),
        );
    }

    /// Produced state that forgets to declare `State` (the old
    /// direction-implied spelling) no longer reaches the registry.
    #[test]
    fn registering_an_unmarked_produced_field_fails_loudly() {
        let id = SlotShapeId::from_static_name("test.role_lookup.unmarked");
        let mut registry = SlotShapeRegistry::default();
        let error = registry
            .register_dynamic_shape(
                id,
                record(vec![crate::slot::shape::field_with_semantics(
                    "output",
                    value(LpType::F32),
                    crate::SlotSemantics::produced(),
                )]),
            )
            .expect_err("an unmarked produced field is rejected");
        assert!(
            matches!(
                error,
                crate::SlotShapeRegistryError::RoleDirectionMismatch(bad, _) if bad == id
            ),
            "{error:?}"
        );
    }

    /// And the mirror image: `State` on something the runtime does not
    /// produce.
    #[test]
    fn registering_a_state_role_without_produced_fails_loudly() {
        let id = SlotShapeId::from_static_name("test.role_lookup.mislabeled");
        let mut registry = SlotShapeRegistry::default();
        let error = registry
            .register_dynamic_shape(
                id,
                record(vec![field_with_role(
                    "output",
                    value(LpType::F32),
                    SlotRole::State,
                )]),
            )
            .expect_err("a State role without a produced direction is rejected");
        assert!(
            matches!(
                error,
                crate::SlotShapeRegistryError::RoleDirectionMismatch(bad, _) if bad == id
            ),
            "{error:?}"
        );
    }

    /// A produced state field, spelled the only way the model now accepts.
    fn state_field(name: &str, shape: SlotShape) -> crate::SlotFieldShape {
        crate::slot::shape::field_with_semantics_and_role(
            name,
            shape,
            crate::SlotSemantics::produced(),
            SlotRole::State,
        )
    }

    fn registry_with(shape: SlotShape) -> (SlotShapeRegistry, SlotShapeId) {
        let id = SlotShapeId::from_static_name("test.role_lookup.root");
        let mut registry = SlotShapeRegistry::default();
        registry.register_dynamic_shape(id, shape).unwrap();
        (registry, id)
    }

    fn resolve(registry: &SlotShapeRegistry, id: SlotShapeId, path: &str) -> SlotRoleResolution {
        let shape = registry.get_shape(id).expect("root shape");
        resolve_slot_role(shape, registry, &SlotPath::parse(path).unwrap()).expect("path resolves")
    }
}
