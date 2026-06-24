use lpc_model::{LpValue, ProductRef, ResourceRef, SlotMapKey};

pub fn format_lp_value(value: &LpValue) -> String {
    match value {
        LpValue::Unset => "unset".to_string(),
        LpValue::String(value) => value.clone(),
        LpValue::I32(value) => value.to_string(),
        LpValue::U32(value) => value.to_string(),
        LpValue::F32(value) => format_float(*value),
        LpValue::Bool(value) => value.to_string(),
        LpValue::Vec2(value) => format_float_array(value),
        LpValue::Vec3(value) => format_float_array(value),
        LpValue::Vec4(value) => format_float_array(value),
        LpValue::IVec2(value) => format_int_array(value),
        LpValue::IVec3(value) => format_int_array(value),
        LpValue::IVec4(value) => format_int_array(value),
        LpValue::UVec2(value) => format_int_array(value),
        LpValue::UVec3(value) => format_int_array(value),
        LpValue::UVec4(value) => format_int_array(value),
        LpValue::BVec2(value) => format_int_array(value),
        LpValue::BVec3(value) => format_int_array(value),
        LpValue::BVec4(value) => format_int_array(value),
        LpValue::Mat2x2(value) => format_matrix(value),
        LpValue::Mat3x3(value) => format_matrix(value),
        LpValue::Mat4x4(value) => format_matrix(value),
        LpValue::Array(values) => {
            let values = values
                .iter()
                .map(format_lp_value)
                .collect::<Vec<_>>()
                .join(", ");
            format!("[{values}]")
        }
        LpValue::Struct { name, fields } => {
            let fields = fields
                .iter()
                .map(|(name, value)| format!("{name}: {}", format_lp_value(value)))
                .collect::<Vec<_>>()
                .join(", ");
            match name {
                Some(name) => format!("{name} {{ {fields} }}"),
                None => format!("{{ {fields} }}"),
            }
        }
        LpValue::Enum { variant, payload } => match payload {
            Some(payload) => format!("variant {variant}({})", format_lp_value(payload)),
            None => format!("variant {variant}"),
        },
        LpValue::Resource(resource) => format_resource_ref(*resource),
        LpValue::Product(product) => format_product_ref(*product),
    }
}

pub fn format_slot_map_key(key: &SlotMapKey) -> String {
    match key {
        SlotMapKey::String(value) => value.clone(),
        SlotMapKey::I32(value) => value.to_string(),
        SlotMapKey::U32(value) => value.to_string(),
    }
}

fn format_resource_ref(resource: ResourceRef) -> String {
    format!("resource {:?}:{}", resource.domain, resource.id)
}

fn format_product_ref(product: ProductRef) -> String {
    match product {
        ProductRef::Visual(product) => {
            format!(
                "visual product node {} output {}",
                product.node(),
                product.output()
            )
        }
        ProductRef::Control(product) => {
            let extent = product.preferred_extent();
            format!(
                "control product node {} output {} ({}x{})",
                product.node(),
                product.output(),
                extent.rows,
                extent.samples_per_row
            )
        }
    }
}

fn format_float(value: f32) -> String {
    if value.is_finite() {
        let rounded = (value * 1000.0).round() / 1000.0;
        if rounded.fract() == 0.0 {
            format!("{rounded:.1}")
        } else {
            rounded.to_string()
        }
    } else {
        value.to_string()
    }
}

fn format_float_array<const N: usize>(value: &[f32; N]) -> String {
    let values = value
        .iter()
        .map(|value| format_float(*value))
        .collect::<Vec<_>>()
        .join(", ");
    format!("({values})")
}

fn format_int_array<T: ToString, const N: usize>(value: &[T; N]) -> String {
    let values = value
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    format!("({values})")
}

fn format_matrix<const R: usize, const C: usize>(value: &[[f32; C]; R]) -> String {
    let rows = value
        .iter()
        .map(format_float_array)
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{rows}]")
}

#[cfg(test)]
mod tests {
    use lpc_model::{ControlExtent, ControlProduct, LpValue, NodeId, ProductRef, VisualProduct};

    use super::*;

    #[test]
    fn formats_scalars_vectors_and_products() {
        assert_eq!(format_lp_value(&LpValue::Bool(true)), "true");
        assert_eq!(format_lp_value(&LpValue::F32(0.33333334)), "0.333");
        assert_eq!(
            format_lp_value(&LpValue::Vec3([1.0, 2.5, 3.0])),
            "(1.0, 2.5, 3.0)"
        );
        assert_eq!(
            format_lp_value(&LpValue::Product(ProductRef::visual(VisualProduct::new(
                NodeId::new(4),
                1,
            )))),
            "visual product node 4 output 1"
        );
        assert_eq!(
            format_lp_value(&LpValue::Product(ProductRef::control(ControlProduct::new(
                NodeId::new(5),
                2,
                ControlExtent::new(3, 12),
            )))),
            "control product node 5 output 2 (3x12)"
        );
    }
}
