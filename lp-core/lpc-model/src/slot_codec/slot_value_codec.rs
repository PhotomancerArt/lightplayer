use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use crate::{
    ControlExtent, ControlProduct, LpType, LpValue, ModelEnumVariant, ModelStructMember, NodeId,
    ProductKind, ProductRef, ResourceDomain, ResourceRef, VisualProduct,
};

use super::{
    SlotValueWriter, SlotWrite, SlotWriteError, SyntaxError, SyntaxEventSource, ValueReader,
};

pub fn read_lp_value<S>(ty: &LpType, value: ValueReader<'_, '_, S>) -> Result<LpValue, SyntaxError>
where
    S: SyntaxEventSource,
{
    match ty {
        LpType::Any => value.lp_value(),
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
        LpType::Enum { variants, .. } => read_lp_enum(value, variants),
        LpType::Resource => read_resource_ref(value).map(LpValue::Resource),
        LpType::Product(kind) => read_product_ref(value, *kind).map(LpValue::Product),
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
        (LpType::Any, value_to_write) => write_untyped_lp_value(value, value_to_write),
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
        (LpType::Enum { variants, .. }, LpValue::Enum { variant, payload }) => {
            write_lp_enum(value, variants, *variant, payload.as_deref())
        }
        (LpType::Resource, LpValue::Resource(resource)) => write_resource_ref(value, resource),
        (LpType::Product(ProductKind::Visual), LpValue::Product(ProductRef::Visual(product))) => {
            write_visual_product(value, product)
        }
        (LpType::Product(ProductKind::Control), LpValue::Product(ProductRef::Control(product))) => {
            write_control_product(value, product)
        }
        _ => Err(SlotWriteError::Serialize),
    }
}

pub fn write_untyped_lp_value<W>(
    value: SlotValueWriter<'_, W>,
    lp_value: &LpValue,
) -> Result<(), SlotWriteError<W::Error>>
where
    W: SlotWrite,
{
    match lp_value {
        LpValue::String(text) => value.string(text),
        LpValue::I32(number) => value.i32(*number),
        LpValue::U32(number) => value.u32(*number),
        LpValue::F32(number) => value.f32(*number),
        LpValue::Bool(flag) => value.bool(*flag),
        LpValue::Vec2(items) => value.f32_array(items),
        LpValue::Vec3(items) => value.f32_array(items),
        LpValue::Vec4(items) => value.f32_array(items),
        LpValue::Array(items) => {
            let mut array = value.array()?;
            for item in items {
                write_untyped_lp_value(array.item()?, item)?;
            }
            array.finish()
        }
        LpValue::Struct { fields, .. } => {
            let mut object = value.object()?;
            for (name, field_value) in fields {
                write_untyped_lp_value(object.prop(name)?, field_value)?;
            }
            object.finish()
        }
        LpValue::Enum { variant, payload } => {
            let mut object = value.object()?;
            object.prop("variant")?.u32(*variant)?;
            if let Some(payload) = payload {
                write_untyped_lp_value(object.prop("payload")?, payload)?;
            }
            object.finish()
        }
        LpValue::Resource(resource) => write_resource_ref(value, resource),
        LpValue::Product(ProductRef::Visual(product)) => write_visual_product(value, product),
        LpValue::Product(ProductRef::Control(product)) => write_control_product(value, product),
        LpValue::IVec2(_)
        | LpValue::IVec3(_)
        | LpValue::IVec4(_)
        | LpValue::UVec2(_)
        | LpValue::UVec3(_)
        | LpValue::UVec4(_)
        | LpValue::BVec2(_)
        | LpValue::BVec3(_)
        | LpValue::BVec4(_)
        | LpValue::Mat2x2(_)
        | LpValue::Mat3x3(_)
        | LpValue::Mat4x4(_) => Err(SlotWriteError::Serialize),
    }
}

fn read_resource_ref<S>(value: ValueReader<'_, '_, S>) -> Result<ResourceRef, SyntaxError>
where
    S: SyntaxEventSource,
{
    const FIELDS: &[&str] = &["domain", "id"];
    let mut resource = ResourceRef::default();
    let mut object = value.object()?;

    while let Some(mut prop) = object.next_prop()? {
        match prop.name() {
            "domain" => resource.domain = read_resource_domain(prop.value())?,
            "id" => resource.id = prop.value().u32()?,
            other => return Err(prop.unknown_field(other, FIELDS)),
        }
    }

    Ok(resource)
}

fn read_resource_domain<S>(value: ValueReader<'_, '_, S>) -> Result<ResourceDomain, SyntaxError>
where
    S: SyntaxEventSource,
{
    let text = value.string()?;
    match text.as_str() {
        "unset" => Ok(ResourceDomain::Unset),
        "runtime_buffer" => Ok(ResourceDomain::RuntimeBuffer),
        _ => Err(SyntaxError::new(
            "",
            None,
            alloc::format!(
                "invalid resource domain {text:?}. Expected one of: unset, runtime_buffer."
            ),
        )),
    }
}

fn read_product_ref<S>(
    value: ValueReader<'_, '_, S>,
    expected_kind: ProductKind,
) -> Result<ProductRef, SyntaxError>
where
    S: SyntaxEventSource,
{
    const FIELDS: &[&str] = &["kind", "node", "output", "preferred_extent"];
    let mut kind = None;
    let mut node = NodeId::default();
    let mut output = 0;
    let mut preferred_extent = ControlExtent::default();
    let mut object = value.object()?;

    while let Some(mut prop) = object.next_prop()? {
        match prop.name() {
            "kind" => kind = Some(read_product_kind(prop.value())?),
            "node" => node = NodeId::new(prop.value().u32()?),
            "output" => output = prop.value().u32()?,
            "preferred_extent" => preferred_extent = read_control_extent(prop.value())?,
            other => return Err(prop.unknown_field(other, FIELDS)),
        }
    }

    let kind = kind.unwrap_or(expected_kind);
    if kind != expected_kind {
        return Err(object.invalid_discriminator_value(
            "kind",
            product_kind_name(kind),
            &[product_kind_name(expected_kind)],
        ));
    }

    match kind {
        ProductKind::Visual => Ok(ProductRef::Visual(VisualProduct::new(node, output))),
        ProductKind::Control => Ok(ProductRef::Control(ControlProduct::new(
            node,
            output,
            preferred_extent,
        ))),
    }
}

fn read_product_kind<S>(value: ValueReader<'_, '_, S>) -> Result<ProductKind, SyntaxError>
where
    S: SyntaxEventSource,
{
    let text = value.string()?;
    match text.as_str() {
        "visual" => Ok(ProductKind::Visual),
        "control" => Ok(ProductKind::Control),
        _ => Err(SyntaxError::new(
            "",
            None,
            alloc::format!("invalid product kind {text:?}. Expected one of: visual, control."),
        )),
    }
}

fn read_control_extent<S>(value: ValueReader<'_, '_, S>) -> Result<ControlExtent, SyntaxError>
where
    S: SyntaxEventSource,
{
    const FIELDS: &[&str] = &["rows", "samples_per_row"];
    let mut rows = 0;
    let mut samples_per_row = 0;
    let mut object = value.object()?;

    while let Some(mut prop) = object.next_prop()? {
        match prop.name() {
            "rows" => rows = prop.value().u32()?,
            "samples_per_row" => samples_per_row = prop.value().u32()?,
            other => return Err(prop.unknown_field(other, FIELDS)),
        }
    }

    Ok(ControlExtent::new(rows, samples_per_row))
}

fn write_resource_ref<W>(
    value: SlotValueWriter<'_, W>,
    resource: &ResourceRef,
) -> Result<(), SlotWriteError<W::Error>>
where
    W: SlotWrite,
{
    let mut object = value.object()?;
    object
        .prop("domain")?
        .string(resource_domain_name(resource.domain))?;
    object.prop("id")?.u32(resource.id)?;
    object.finish()
}

fn write_visual_product<W>(
    value: SlotValueWriter<'_, W>,
    product: &VisualProduct,
) -> Result<(), SlotWriteError<W::Error>>
where
    W: SlotWrite,
{
    let mut object = value.object()?;
    object.prop("kind")?.string("visual")?;
    object.prop("node")?.u32(product.node().as_u32())?;
    object.prop("output")?.u32(product.output())?;
    object.finish()
}

fn write_control_product<W>(
    value: SlotValueWriter<'_, W>,
    product: &ControlProduct,
) -> Result<(), SlotWriteError<W::Error>>
where
    W: SlotWrite,
{
    let mut object = value.object()?;
    object.prop("kind")?.string("control")?;
    object.prop("node")?.u32(product.node().as_u32())?;
    object.prop("output")?.u32(product.output())?;
    write_control_extent(object.prop("preferred_extent")?, product.preferred_extent())?;
    object.finish()
}

fn write_control_extent<W>(
    value: SlotValueWriter<'_, W>,
    extent: ControlExtent,
) -> Result<(), SlotWriteError<W::Error>>
where
    W: SlotWrite,
{
    let mut object = value.object()?;
    object.prop("rows")?.u32(extent.rows)?;
    object
        .prop("samples_per_row")?
        .u32(extent.samples_per_row)?;
    object.finish()
}

fn resource_domain_name(domain: ResourceDomain) -> &'static str {
    match domain {
        ResourceDomain::Unset => "unset",
        ResourceDomain::RuntimeBuffer => "runtime_buffer",
    }
}

fn product_kind_name(kind: ProductKind) -> &'static str {
    match kind {
        ProductKind::Visual => "visual",
        ProductKind::Control => "control",
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

fn read_lp_enum<S>(
    value: ValueReader<'_, '_, S>,
    variants: &[ModelEnumVariant],
) -> Result<LpValue, SyntaxError>
where
    S: SyntaxEventSource,
{
    const FIELDS: &[&str] = &["kind", "payload"];
    let expected = enum_variant_names(variants);
    let mut object = value.object()?;
    let kind = object.expect_discriminator("kind", &expected)?;
    let variant_index = variants
        .iter()
        .position(|variant| variant.name == kind)
        .ok_or_else(|| SyntaxError::new("", None, "validated enum variant was not found"))?;
    let variant = &variants[variant_index];
    let payload = match &variant.payload {
        Some(payload_ty) => {
            let Some(mut prop) = object.next_prop()? else {
                return Err(object.missing_required_field("payload"));
            };
            match prop.name() {
                "payload" => Some(Box::new(read_lp_value(payload_ty, prop.value())?)),
                other => return Err(prop.unknown_field(other, FIELDS)),
            }
        }
        None => {
            if let Some(mut prop) = object.next_prop()? {
                let name = prop.name().to_string();
                if name == "payload" {
                    prop.value().skip_value()?;
                    return Err(SyntaxError::new(
                        "",
                        None,
                        alloc::format!("enum variant {:?} does not accept a payload", variant.name),
                    ));
                }
                return Err(prop.unknown_field(&name, FIELDS));
            }
            None
        }
    };
    object.finish()?;

    Ok(LpValue::Enum {
        variant: variant_index as u32,
        payload,
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

fn write_lp_enum<W>(
    value: SlotValueWriter<'_, W>,
    variants: &[ModelEnumVariant],
    variant_index: u32,
    payload: Option<&LpValue>,
) -> Result<(), SlotWriteError<W::Error>>
where
    W: SlotWrite,
{
    let Some(variant) = variants.get(variant_index as usize) else {
        return Err(SlotWriteError::Serialize);
    };
    let mut object = value.object()?;
    object.prop("kind")?.string(&variant.name)?;
    match (&variant.payload, payload) {
        (Some(payload_ty), Some(payload)) => {
            write_lp_value(object.prop("payload")?, payload_ty, payload)?
        }
        (Some(_), None) | (None, Some(_)) => return Err(SlotWriteError::Serialize),
        (None, None) => {}
    }
    object.finish()
}

fn enum_variant_names(variants: &[ModelEnumVariant]) -> Vec<&str> {
    variants
        .iter()
        .map(|variant| variant.name.as_str())
        .collect()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ModelEnumVariant, RuntimeBufferId, VisualProduct,
        slot_codec::{JsonSyntaxSource, SlotReader, SlotWriter, TomlSyntaxSource},
    };
    use alloc::boxed::Box;
    use alloc::string::String;
    use alloc::vec;
    use alloc::vec::Vec;

    #[test]
    fn slot_value_codec_reads_and_writes_resource_refs() {
        let value = read_json_value(LpType::Resource, r#"{"domain":"runtime_buffer","id":7}"#);

        assert_eq!(
            value,
            LpValue::Resource(ResourceRef::runtime_buffer(RuntimeBufferId::new(7)))
        );
        assert_eq!(
            read_json_value(LpType::Resource, r#"{"domain":"unset","id":0}"#),
            LpValue::Resource(ResourceRef::default())
        );
        assert_eq!(
            write_json_value(&LpType::Resource, &value),
            r#"{"domain":"runtime_buffer","id":7}"#
        );
    }

    #[test]
    fn slot_value_codec_reads_and_writes_visual_products() {
        let ty = LpType::Product(ProductKind::Visual);
        let value = read_json_value(ty.clone(), r#"{"kind":"visual","node":2,"output":1}"#);

        assert_eq!(
            value,
            LpValue::Product(ProductRef::visual(VisualProduct::new(NodeId::new(2), 1)))
        );
        assert_eq!(
            write_json_value(&ty, &value),
            r#"{"kind":"visual","node":2,"output":1}"#
        );
    }

    #[test]
    fn slot_value_codec_reads_and_writes_control_products() {
        let ty = LpType::Product(ProductKind::Control);
        let value = read_json_value(
            ty.clone(),
            r#"{"kind":"control","node":3,"output":2,"preferred_extent":{"rows":4,"samples_per_row":12}}"#,
        );

        assert_eq!(
            value,
            LpValue::Product(ProductRef::control(ControlProduct::new(
                NodeId::new(3),
                2,
                ControlExtent::new(4, 12)
            )))
        );
        assert_eq!(
            write_json_value(&ty, &value),
            r#"{"kind":"control","node":3,"output":2,"preferred_extent":{"rows":4,"samples_per_row":12}}"#
        );
    }

    #[test]
    fn slot_value_codec_rejects_wrong_product_kind() {
        let registry = crate::SlotShapeRegistry::default();
        let mut reader = SlotReader::new(
            JsonSyntaxSource::new(r#"{"kind":"control","node":3,"output":2}"#).unwrap(),
            &registry,
        );

        let error =
            read_lp_value(&LpType::Product(ProductKind::Visual), reader.value()).unwrap_err();

        assert!(error.message().contains("control"));
        assert!(error.message().contains("visual"));
    }

    #[test]
    fn slot_value_codec_reads_product_from_toml_source() {
        let toml = toml::toml! {
            kind = "control"
            node = 3
            output = 2

            [preferred_extent]
            rows = 4
            samples_per_row = 12
        };
        let toml = toml::Value::Table(toml);
        let registry = crate::SlotShapeRegistry::default();
        let mut reader = SlotReader::new(TomlSyntaxSource::new(&toml).unwrap(), &registry);

        assert_eq!(
            read_lp_value(&LpType::Product(ProductKind::Control), reader.value()).unwrap(),
            LpValue::Product(ProductRef::control(ControlProduct::new(
                NodeId::new(3),
                2,
                ControlExtent::new(4, 12)
            )))
        );
    }

    #[test]
    fn slot_value_codec_reads_and_writes_enum_values() {
        let ty = endpoint_ty();

        assert_eq!(
            read_json_value(ty.clone(), r#"{"kind":"Unset"}"#),
            LpValue::Enum {
                variant: 0,
                payload: None,
            }
        );

        let value = read_json_value(ty.clone(), r#"{"kind":"Value","payload":0.75}"#);
        assert_eq!(
            value,
            LpValue::Enum {
                variant: 1,
                payload: Some(Box::new(LpValue::F32(0.75))),
            }
        );
        assert_eq!(
            write_json_value(&ty, &value),
            r#"{"kind":"Value","payload":0.75}"#
        );
    }

    #[test]
    fn slot_value_codec_reports_enum_discriminator_errors() {
        let registry = crate::SlotShapeRegistry::default();
        let mut reader = SlotReader::new(
            JsonSyntaxSource::new(r#"{"kind":"Blark12"}"#).unwrap(),
            &registry,
        );

        let error = read_lp_value(&endpoint_ty(), reader.value()).unwrap_err();

        assert!(error.message().contains("Blark12"));
        assert!(error.message().contains("Unset"));
        assert!(error.message().contains("Value"));
    }

    #[test]
    fn slot_value_codec_reports_enum_payload_errors() {
        let registry = crate::SlotShapeRegistry::default();
        let mut reader = SlotReader::new(
            JsonSyntaxSource::new(r#"{"kind":"Value"}"#).unwrap(),
            &registry,
        );

        let error = read_lp_value(&endpoint_ty(), reader.value()).unwrap_err();

        assert!(error.message().contains("payload"));
    }

    fn read_json_value(ty: LpType, json: &str) -> LpValue {
        let registry = crate::SlotShapeRegistry::default();
        let mut reader = SlotReader::new(JsonSyntaxSource::new(json).unwrap(), &registry);
        read_lp_value(&ty, reader.value()).unwrap()
    }

    fn write_json_value(ty: &LpType, value: &LpValue) -> String {
        let mut out = Vec::new();
        let mut writer = SlotWriter::new(&mut out);
        write_lp_value(writer.value(), ty, value).unwrap();
        String::from_utf8(out).unwrap()
    }

    fn endpoint_ty() -> LpType {
        LpType::Enum {
            name: Some(String::from("Endpoint")),
            variants: vec![
                ModelEnumVariant {
                    name: String::from("Unset"),
                    payload: None,
                },
                ModelEnumVariant {
                    name: String::from("Value"),
                    payload: Some(LpType::F32),
                },
            ],
        }
    }
}
