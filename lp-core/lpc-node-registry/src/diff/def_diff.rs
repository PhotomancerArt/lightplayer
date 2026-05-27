//! Slot-tree diff between two parsed node defs.

use alloc::collections::BTreeSet;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use lpc_model::{
    LpValue, NodeDef, Revision, SlotAccess, SlotDataAccess, SlotMapKey, SlotName, SlotPath,
    SlotShapeLookup, SlotShapeRegistry, SlotShapeView, lookup_slot_data_and_shape,
};

use crate::ParseCtx;
use crate::edit_model::SlotEdit;
use crate::registry::apply_ops_to_node_def;

use super::DiffError;

pub fn diff_node_defs(
    base: &NodeDef,
    target: &NodeDef,
    ctx: &ParseCtx<'_>,
) -> Result<Vec<SlotEdit>, DiffError> {
    if base.kind() == target.kind() && authored_defs_equivalent(base, target, ctx)? {
        return Ok(Vec::new());
    }
    let mut ops = Vec::new();
    let mut current = base.clone();
    if current.kind() != target.kind() {
        push_ensure_present(
            &mut current,
            &SlotPath::root().child(SlotName::parse(target.variant_name()).map_err(|err| {
                DiffError::Diff {
                    message: alloc::format!("root variant name: {err}"),
                }
            })?),
            ctx,
            &mut ops,
        )?;
    }
    diff_at_path(&mut current, base, target, &SlotPath::root(), ctx, &mut ops)?;
    let mut verify = base.clone();
    apply_ops_to_node_def(&mut verify, &ops, ctx, Revision::new(1)).map_err(|err| {
        DiffError::Diff {
            message: alloc::format!("verify apply failed: {err}"),
        }
    })?;
    if !authored_defs_equivalent(&verify, target, ctx)? {
        return Err(DiffError::Diff {
            message: String::from("slot diff verify mismatch"),
        });
    }
    Ok(ops)
}

fn authored_defs_equivalent(
    left: &NodeDef,
    right: &NodeDef,
    ctx: &ParseCtx<'_>,
) -> Result<bool, DiffError> {
    let left_text = NodeDef::write_toml(left, ctx.shapes).map_err(|err| DiffError::Diff {
        message: err.to_string(),
    })?;
    let right_text = NodeDef::write_toml(right, ctx.shapes).map_err(|err| DiffError::Diff {
        message: err.to_string(),
    })?;
    Ok(left_text == right_text)
}

fn diff_at_path(
    current: &mut NodeDef,
    base: &NodeDef,
    target: &NodeDef,
    path: &SlotPath,
    ctx: &ParseCtx<'_>,
    ops: &mut Vec<SlotEdit>,
) -> Result<(), DiffError> {
    let shapes = ctx.shapes;
    let slot_kind = {
        let (cur_data, cur_shape) =
            lookup_slot_data_and_shape(current as &dyn SlotAccess, shapes, path).map_err(
                |err| DiffError::Diff {
                    message: alloc::format!("lookup current `{path}`: {err}"),
                },
            )?;
        let (tgt_data, tgt_shape) =
            lookup_slot_data_and_shape(target as &dyn SlotAccess, shapes, path).map_err(|err| {
                DiffError::Diff {
                    message: alloc::format!("lookup target `{path}`: {err}"),
                }
            })?;
        let cur_shape = resolve_shape(cur_shape, shapes)?;
        let tgt_shape = resolve_shape(tgt_shape, shapes)?;
        if cur_shape.ref_id().is_some() || tgt_shape.ref_id().is_some() {
            return diff_at_path(current, base, target, path, ctx, ops);
        }
        classify_slot(cur_data, tgt_data, cur_shape, tgt_shape)?
    };

    match slot_kind {
        SlotKind::Value { target_value } => {
            push_assign_value(current, path, target_value, ctx, ops)?;
        }
        SlotKind::Enum { variant } => {
            let variant_name = SlotName::parse(&variant).map_err(|err| DiffError::Diff {
                message: alloc::format!("enum variant `{path}`: {err}"),
            })?;
            push_ensure_present(current, &path.child(variant_name.clone()), ctx, ops)?;
            diff_at_path(current, base, target, &path.child(variant_name), ctx, ops)?;
        }
        SlotKind::EnumBody { variant } => {
            let variant_name = SlotName::parse(&variant).map_err(|err| DiffError::Diff {
                message: alloc::format!("enum variant `{path}`: {err}"),
            })?;
            diff_at_path(current, base, target, &path.child(variant_name), ctx, ops)?;
        }
        SlotKind::Record { field_names } => {
            for name in field_names {
                diff_at_path(current, base, target, &path.child(name), ctx, ops)?;
            }
        }
        SlotKind::Map {
            remove_keys,
            insert_keys,
            shared_keys,
        } => {
            for key in remove_keys {
                push_remove(current, &path.child_key(key), ctx, ops)?;
            }
            for key in insert_keys {
                push_ensure_present(current, &path.child_key(key.clone()), ctx, ops)?;
                diff_at_path(current, base, target, &path.child_key(key), ctx, ops)?;
            }
            for key in shared_keys {
                diff_at_path(current, base, target, &path.child_key(key), ctx, ops)?;
            }
        }
        SlotKind::Option { present, has_body } => {
            if present {
                push_ensure_present(current, path, ctx, ops)?;
            } else {
                push_remove(current, path, ctx, ops)?;
            }
            if has_body {
                diff_at_path(
                    current,
                    base,
                    target,
                    &path.child(SlotName::parse("some").expect("valid slot name")),
                    ctx,
                    ops,
                )?;
            }
        }
        SlotKind::OptionBody => {
            diff_at_path(
                current,
                base,
                target,
                &path.child(SlotName::parse("some").expect("valid slot name")),
                ctx,
                ops,
            )?;
        }
        SlotKind::Same => {}
    }
    Ok(())
}

enum SlotKind {
    Same,
    Value {
        target_value: LpValue,
    },
    Enum {
        variant: String,
    },
    EnumBody {
        variant: String,
    },
    Record {
        field_names: Vec<SlotName>,
    },
    Map {
        remove_keys: Vec<SlotMapKey>,
        insert_keys: Vec<SlotMapKey>,
        shared_keys: Vec<SlotMapKey>,
    },
    Option {
        present: bool,
        has_body: bool,
    },
    OptionBody,
}

fn classify_slot(
    cur_data: SlotDataAccess<'_>,
    tgt_data: SlotDataAccess<'_>,
    _cur_shape: SlotShapeView<'_>,
    tgt_shape: SlotShapeView<'_>,
) -> Result<SlotKind, DiffError> {
    match (cur_data, tgt_data) {
        (SlotDataAccess::Value(cur), SlotDataAccess::Value(tgt)) => {
            if cur.value() == tgt.value() {
                Ok(SlotKind::Same)
            } else {
                Ok(SlotKind::Value {
                    target_value: tgt.value(),
                })
            }
        }
        (SlotDataAccess::Enum(cur), SlotDataAccess::Enum(tgt)) => {
            if cur.variant() != tgt.variant() {
                Ok(SlotKind::Enum {
                    variant: String::from(tgt.variant()),
                })
            } else {
                Ok(SlotKind::EnumBody {
                    variant: String::from(tgt.variant()),
                })
            }
        }
        (SlotDataAccess::Record(_), SlotDataAccess::Record(_)) => {
            let field_count = tgt_shape
                .record_fields_len()
                .ok_or_else(|| DiffError::Diff {
                    message: String::from("record shape missing fields"),
                })?;
            let mut field_names = Vec::new();
            for index in 0..field_count {
                let field = tgt_shape
                    .record_field(index)
                    .ok_or_else(|| DiffError::Diff {
                        message: alloc::format!("record field {index} missing"),
                    })?;
                field_names.push(SlotName::parse(field.name_str()).map_err(|err| {
                    DiffError::Diff {
                        message: alloc::format!("field name: {err}"),
                    }
                })?);
            }
            Ok(SlotKind::Record { field_names })
        }
        (SlotDataAccess::Map(cur), SlotDataAccess::Map(tgt)) => {
            let mut cur_set = BTreeSet::new();
            for key in cur.keys() {
                cur_set.insert(key);
            }
            let mut tgt_set = BTreeSet::new();
            for key in tgt.keys() {
                tgt_set.insert(key);
            }
            Ok(SlotKind::Map {
                remove_keys: cur_set.difference(&tgt_set).cloned().collect(),
                insert_keys: tgt_set.difference(&cur_set).cloned().collect(),
                shared_keys: cur_set.intersection(&tgt_set).cloned().collect(),
            })
        }
        (SlotDataAccess::Option(cur), SlotDataAccess::Option(tgt)) => {
            let cur_present = cur.data().is_some();
            let tgt_present = tgt.data().is_some();
            if cur_present != tgt_present {
                Ok(SlotKind::Option {
                    present: tgt_present,
                    has_body: tgt_present,
                })
            } else if tgt_present {
                Ok(SlotKind::OptionBody)
            } else {
                Ok(SlotKind::Same)
            }
        }
        (SlotDataAccess::Custom(_), SlotDataAccess::Custom(_)) => Err(DiffError::Diff {
            message: String::from("custom slot diff is not supported"),
        }),
        _ => Err(DiffError::Diff {
            message: alloc::format!(
                "shape/data mismatch: {} vs {}",
                data_kind(cur_data),
                data_kind(tgt_data)
            ),
        }),
    }
}

fn push_ensure_present(
    current: &mut NodeDef,
    path: &SlotPath,
    ctx: &ParseCtx<'_>,
    ops: &mut Vec<SlotEdit>,
) -> Result<(), DiffError> {
    let op = SlotEdit::EnsurePresent { path: path.clone() };
    apply_ops_to_node_def(current, &[op.clone()], ctx, Revision::new(1)).map_err(|err| {
        DiffError::Diff {
            message: err.to_string(),
        }
    })?;
    ops.push(op);
    Ok(())
}

fn push_assign_value(
    current: &mut NodeDef,
    path: &SlotPath,
    value: LpValue,
    ctx: &ParseCtx<'_>,
    ops: &mut Vec<SlotEdit>,
) -> Result<(), DiffError> {
    let op = SlotEdit::AssignValue {
        path: path.clone(),
        value,
    };
    apply_ops_to_node_def(current, &[op.clone()], ctx, Revision::new(1)).map_err(|err| {
        DiffError::Diff {
            message: err.to_string(),
        }
    })?;
    ops.push(op);
    Ok(())
}

fn push_remove(
    current: &mut NodeDef,
    path: &SlotPath,
    ctx: &ParseCtx<'_>,
    ops: &mut Vec<SlotEdit>,
) -> Result<(), DiffError> {
    let op = SlotEdit::Remove { path: path.clone() };
    apply_ops_to_node_def(current, &[op.clone()], ctx, Revision::new(1)).map_err(|err| {
        DiffError::Diff {
            message: err.to_string(),
        }
    })?;
    ops.push(op);
    Ok(())
}

fn resolve_shape<'a>(
    mut shape: SlotShapeView<'a>,
    shapes: &'a SlotShapeRegistry,
) -> Result<SlotShapeView<'a>, DiffError> {
    while let Some(id) = shape.ref_id() {
        shape = shapes.get_shape(id).ok_or_else(|| DiffError::Diff {
            message: alloc::format!("missing referenced shape {id}"),
        })?;
    }
    Ok(shape)
}

fn data_kind(data: SlotDataAccess<'_>) -> &'static str {
    match data {
        SlotDataAccess::Unit(_) => "unit",
        SlotDataAccess::Value(_) => "value",
        SlotDataAccess::Record(_) => "record",
        SlotDataAccess::Map(_) => "map",
        SlotDataAccess::Enum(_) => "enum",
        SlotDataAccess::Option(_) => "option",
        SlotDataAccess::Custom(_) => "custom",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_model::SlotShapeRegistry;

    #[test]
    fn diff_shader_from_default() {
        let shapes = SlotShapeRegistry::default();
        let ctx = ParseCtx { shapes: &shapes };
        let text = include_str!("../../../../examples/basic/shader.toml");
        let target = NodeDef::read_toml(&shapes, text).unwrap();
        let base = NodeDef::default();
        diff_node_defs(&base, &target, &ctx).expect("shader diff");
    }
}
