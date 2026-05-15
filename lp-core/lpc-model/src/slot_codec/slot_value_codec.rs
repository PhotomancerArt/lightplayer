use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use crate::{LpType, LpValue, ModelStructMember};

use super::{
    SlotValueWriter, SlotWrite, SlotWriteError, SyntaxError, SyntaxEventSource, ValueReader,
};

pub fn read_lp_value<S>(ty: &LpType, value: ValueReader<'_, '_, S>) -> Result<LpValue, SyntaxError>
where
    S: SyntaxEventSource,
{
    match ty {
        LpType::String => value.string().map(LpValue::String),
        LpType::I32 => value.i32().map(LpValue::I32),
        LpType::U32 => value.u32().map(LpValue::U32),
        LpType::F32 => value.f32().map(LpValue::F32),
        LpType::Bool => value.bool().map(LpValue::Bool),
        LpType::Vec2 => read_f32_array::<_, 2>(value).map(LpValue::Vec2),
        LpType::Vec3 => read_f32_array::<_, 3>(value).map(LpValue::Vec3),
        LpType::Vec4 => read_f32_array::<_, 4>(value).map(LpValue::Vec4),
        LpType::IVec2 => read_i32_array::<_, 2>(value).map(LpValue::IVec2),
        LpType::IVec3 => read_i32_array::<_, 3>(value).map(LpValue::IVec3),
        LpType::IVec4 => read_i32_array::<_, 4>(value).map(LpValue::IVec4),
        LpType::UVec2 => read_u32_array::<_, 2>(value).map(LpValue::UVec2),
        LpType::UVec3 => read_u32_array::<_, 3>(value).map(LpValue::UVec3),
        LpType::UVec4 => read_u32_array::<_, 4>(value).map(LpValue::UVec4),
        LpType::BVec2 => read_bool_array::<_, 2>(value).map(LpValue::BVec2),
        LpType::BVec3 => read_bool_array::<_, 3>(value).map(LpValue::BVec3),
        LpType::BVec4 => read_bool_array::<_, 4>(value).map(LpValue::BVec4),
        LpType::Mat2x2 => read_matrix::<_, 2>(value).map(LpValue::Mat2x2),
        LpType::Mat3x3 => read_matrix::<_, 3>(value).map(LpValue::Mat3x3),
        LpType::Mat4x4 => read_matrix::<_, 4>(value).map(LpValue::Mat4x4),
        LpType::Array(item_ty, len) => read_lp_array(value, item_ty, Some(*len)),
        LpType::List(item_ty) => read_lp_array(value, item_ty, None),
        LpType::Struct { name, fields } => read_lp_struct(value, name.clone(), fields),
        LpType::Resource | LpType::Product(_) => Err(SyntaxError::new(
            "",
            None,
            "resource/product slot values need a dedicated codec",
        )),
    }
}

pub fn write_lp_value<W>(
    value: SlotValueWriter<'_, W>,
    ty: &LpType,
    lp_value: &LpValue,
) -> Result<(), SlotWriteError<W::Error>>
where
    W: SlotWrite,
{
    match (ty, lp_value) {
        (LpType::String, LpValue::String(text)) => value.string(text),
        (LpType::I32, LpValue::I32(number)) => value.i32(*number),
        (LpType::U32, LpValue::U32(number)) => value.u32(*number),
        (LpType::F32, LpValue::F32(number)) => value.f32(*number),
        (LpType::Bool, LpValue::Bool(flag)) => value.bool(*flag),
        (LpType::Vec2, LpValue::Vec2(items)) => value.f32_array(items),
        (LpType::Vec3, LpValue::Vec3(items)) => value.f32_array(items),
        (LpType::Vec4, LpValue::Vec4(items)) => value.f32_array(items),
        (LpType::IVec2, LpValue::IVec2(items)) => write_i32_array(value, items),
        (LpType::IVec3, LpValue::IVec3(items)) => write_i32_array(value, items),
        (LpType::IVec4, LpValue::IVec4(items)) => write_i32_array(value, items),
        (LpType::UVec2, LpValue::UVec2(items)) => write_u32_array(value, items),
        (LpType::UVec3, LpValue::UVec3(items)) => write_u32_array(value, items),
        (LpType::UVec4, LpValue::UVec4(items)) => write_u32_array(value, items),
        (LpType::BVec2, LpValue::BVec2(items)) => write_bool_array(value, items),
        (LpType::BVec3, LpValue::BVec3(items)) => write_bool_array(value, items),
        (LpType::BVec4, LpValue::BVec4(items)) => write_bool_array(value, items),
        (LpType::Mat2x2, LpValue::Mat2x2(matrix)) => write_matrix(value, matrix),
        (LpType::Mat3x3, LpValue::Mat3x3(matrix)) => write_matrix(value, matrix),
        (LpType::Mat4x4, LpValue::Mat4x4(matrix)) => write_matrix(value, matrix),
        (LpType::Array(item_ty, len), LpValue::Array(items)) if items.len() == *len => {
            write_lp_array(value, item_ty, items)
        }
        (LpType::List(item_ty), LpValue::Array(items)) => write_lp_array(value, item_ty, items),
        (LpType::Struct { fields, .. }, LpValue::Struct { fields: values, .. }) => {
            write_lp_struct(value, fields, values)
        }
        _ => Err(SlotWriteError::Serialize),
    }
}

fn read_lp_array<S>(
    value: ValueReader<'_, '_, S>,
    item_ty: &LpType,
    expected_len: Option<usize>,
) -> Result<LpValue, SyntaxError>
where
    S: SyntaxEventSource,
{
    let mut items = Vec::new();
    let mut array = value.array()?;
    while let Some(item) = array.next_item()? {
        items.push(read_lp_value(item_ty, item)?);
    }

    if let Some(expected_len) = expected_len
        && items.len() != expected_len
    {
        return Err(SyntaxError::new(
            "",
            None,
            alloc::format!(
                "expected array of {expected_len} values, found {}",
                items.len()
            ),
        ));
    }

    Ok(LpValue::Array(items))
}

fn read_lp_struct<S>(
    value: ValueReader<'_, '_, S>,
    name: Option<String>,
    fields: &[ModelStructMember],
) -> Result<LpValue, SyntaxError>
where
    S: SyntaxEventSource,
{
    let expected: Vec<&str> = fields.iter().map(|field| field.name.as_str()).collect();
    let mut values = vec![None; fields.len()];
    let mut object = value.object()?;

    while let Some(mut prop) = object.next_prop()? {
        let Some(index) = fields.iter().position(|field| field.name == prop.name()) else {
            return Err(prop.unknown_field(prop.name(), &expected));
        };
        values[index] = Some(read_lp_value(&fields[index].ty, prop.value())?);
    }

    let mut struct_fields = Vec::with_capacity(fields.len());
    for (index, field) in fields.iter().enumerate() {
        let value = values[index]
            .take()
            .ok_or_else(|| object.missing_required_field(&field.name))?;
        struct_fields.push((field.name.clone(), value));
    }

    Ok(LpValue::Struct {
        name,
        fields: struct_fields,
    })
}

fn read_f32_array<S, const N: usize>(value: ValueReader<'_, '_, S>) -> Result<[f32; N], SyntaxError>
where
    S: SyntaxEventSource,
{
    value.f32_array()
}

fn read_i32_array<S, const N: usize>(value: ValueReader<'_, '_, S>) -> Result<[i32; N], SyntaxError>
where
    S: SyntaxEventSource,
{
    read_copy_array(value, |value| value.i32())
}

fn read_u32_array<S, const N: usize>(value: ValueReader<'_, '_, S>) -> Result<[u32; N], SyntaxError>
where
    S: SyntaxEventSource,
{
    read_copy_array(value, |value| value.u32())
}

fn read_bool_array<S, const N: usize>(
    value: ValueReader<'_, '_, S>,
) -> Result<[bool; N], SyntaxError>
where
    S: SyntaxEventSource,
{
    read_copy_array(value, |value| value.bool())
}

fn read_copy_array<S, T, const N: usize>(
    value: ValueReader<'_, '_, S>,
    mut read_item: impl FnMut(ValueReader<'_, '_, S>) -> Result<T, SyntaxError>,
) -> Result<[T; N], SyntaxError>
where
    S: SyntaxEventSource,
    T: Copy + Default,
{
    let mut array = value.array()?;
    let mut values = [T::default(); N];
    let mut count = 0;

    while let Some(item) = array.next_item()? {
        if count >= N {
            item.skip_value()?;
            return Err(SyntaxError::new(
                "",
                None,
                alloc::format!("expected array of {N} values, found more"),
            ));
        }
        values[count] = read_item(item)?;
        count += 1;
    }

    if count != N {
        return Err(SyntaxError::new(
            "",
            None,
            alloc::format!("expected array of {N} values, found {count}"),
        ));
    }

    Ok(values)
}

fn read_matrix<S, const N: usize>(
    value: ValueReader<'_, '_, S>,
) -> Result<[[f32; N]; N], SyntaxError>
where
    S: SyntaxEventSource,
{
    let mut array = value.array()?;
    let mut rows = [[0.0; N]; N];
    let mut count = 0;

    while let Some(item) = array.next_item()? {
        if count >= N {
            item.skip_value()?;
            return Err(SyntaxError::new(
                "",
                None,
                alloc::format!("expected matrix with {N} rows, found more"),
            ));
        }
        rows[count] = item.f32_array()?;
        count += 1;
    }

    if count != N {
        return Err(SyntaxError::new(
            "",
            None,
            alloc::format!("expected matrix with {N} rows, found {count}"),
        ));
    }

    Ok(rows)
}

fn write_lp_array<W>(
    value: SlotValueWriter<'_, W>,
    item_ty: &LpType,
    items: &[LpValue],
) -> Result<(), SlotWriteError<W::Error>>
where
    W: SlotWrite,
{
    let mut array = value.array()?;
    for item in items {
        write_lp_value(array.item()?, item_ty, item)?;
    }
    array.finish()
}

fn write_lp_struct<W>(
    value: SlotValueWriter<'_, W>,
    fields: &[ModelStructMember],
    values: &[(String, LpValue)],
) -> Result<(), SlotWriteError<W::Error>>
where
    W: SlotWrite,
{
    let values: BTreeMap<&str, &LpValue> = values
        .iter()
        .map(|(name, value)| (name.as_str(), value))
        .collect();
    let mut object = value.object()?;
    for field in fields {
        let Some(field_value) = values.get(field.name.as_str()) else {
            return Err(SlotWriteError::Serialize);
        };
        write_lp_value(object.prop(&field.name)?, &field.ty, field_value)?;
    }
    object.finish()
}

fn write_i32_array<W, const N: usize>(
    value: SlotValueWriter<'_, W>,
    items: &[i32; N],
) -> Result<(), SlotWriteError<W::Error>>
where
    W: SlotWrite,
{
    let mut array = value.array()?;
    for item in items {
        array.item()?.i32(*item)?;
    }
    array.finish()
}

fn write_u32_array<W, const N: usize>(
    value: SlotValueWriter<'_, W>,
    items: &[u32; N],
) -> Result<(), SlotWriteError<W::Error>>
where
    W: SlotWrite,
{
    let mut array = value.array()?;
    for item in items {
        array.item()?.u32(*item)?;
    }
    array.finish()
}

fn write_bool_array<W, const N: usize>(
    value: SlotValueWriter<'_, W>,
    items: &[bool; N],
) -> Result<(), SlotWriteError<W::Error>>
where
    W: SlotWrite,
{
    let mut array = value.array()?;
    for item in items {
        array.item()?.bool(*item)?;
    }
    array.finish()
}

fn write_matrix<W, const N: usize>(
    value: SlotValueWriter<'_, W>,
    matrix: &[[f32; N]; N],
) -> Result<(), SlotWriteError<W::Error>>
where
    W: SlotWrite,
{
    let mut array = value.array()?;
    for row in matrix {
        array.item()?.f32_array(row)?;
    }
    array.finish()
}
