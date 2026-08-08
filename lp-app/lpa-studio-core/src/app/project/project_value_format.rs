use lpc_model::{
    FromLpValue, Gradient, GradientConfig, LpValue, ProductRef, ResourceRef, SlotMapKey,
};

pub fn format_lp_value(value: &LpValue) -> String {
    // A gradient's storage is a stop array plus tags; printed field by
    // field it drowns every row it lands in. Every text surface says what
    // the palette IS instead (the strips are the picture — M4 P2).
    if let Some(config) = gradient_config_value(value) {
        return format_gradient_summary(&config);
    }
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

/// The palette a value holds, whether it is stored as a `GradientConfig`
/// record or a bare `Gradient` (which reads as [`GradientConfig::Static`] —
/// one palette, held).
///
/// `None` for anything else, so ordinary structs keep their generic
/// display. This is the ONE place a value is recognized as a palette: every
/// gradient surface (slot rows, wiring value boxes, probe rows, pending-edit
/// rows) asks here rather than sniffing struct names of its own.
#[must_use]
pub fn gradient_config_value(value: &LpValue) -> Option<GradientConfig> {
    let LpValue::Struct { name, .. } = value else {
        return None;
    };
    match name.as_deref()? {
        "GradientConfig" => GradientConfig::from_lp_value(value).ok(),
        "Gradient" => Gradient::from_lp_value(value)
            .ok()
            .map(GradientConfig::Static),
        _ => None,
    }
}

/// One dense line describing a palette, for the text surfaces that have no
/// room for a strip (pending-edit rows) and as the meta line under the ones
/// that do.
///
/// A static palette reads `space · method · N stops`; a cycle reads
/// `cycle · N palettes · every <step> s · <fade> s fade` — the Step voice
/// the M4 P6 gate picked (plain seconds, the number the slider actually
/// moves through) — and a frozen cycle says `held` instead of a rate it
/// does not have.
#[must_use]
pub fn format_gradient_summary(config: &GradientConfig) -> String {
    match config {
        GradientConfig::Static(gradient) => format_static_gradient(gradient),
        GradientConfig::Cycle {
            set,
            step_seconds,
            fade_seconds,
        } => {
            let count = set.len();
            if config.is_frozen() {
                return format!("cycle \u{b7} {count} palettes \u{b7} held");
            }
            format!(
                "cycle \u{b7} {count} palettes \u{b7} every {} s \u{b7} {} s fade",
                format_step_seconds(*step_seconds),
                format_float(*fade_seconds)
            )
        }
    }
}

/// The compact palette readout a PANEL control shows beside its strips —
/// the dense summary trimmed to what fits in a control's value slot.
///
/// A held palette states its stop count (`5 stops`); a cycle states its
/// size and step (`\u{21bb} 4 \u{b7} 4 s` — the P6 gate's Step voice,
/// trimmed of the row's `every`), or `\u{21bb} 4 \u{b7} held` when the
/// step is frozen. The colorspace, interpolation method, and
/// fade — everything [`format_gradient_summary`] says and this does not —
/// stay one click away in the control's detail popup, because a control
/// panel is read at a glance while it is being played.
#[must_use]
pub fn format_gradient_chip(config: &GradientConfig) -> String {
    match config {
        GradientConfig::Static(gradient) => {
            let stops = gradient.stops.len();
            let unit = if stops == 1 { "stop" } else { "stops" };
            format!("{stops} {unit}")
        }
        GradientConfig::Cycle {
            set, step_seconds, ..
        } => {
            let count = set.len();
            if config.is_frozen() {
                return format!("\u{21bb} {count} \u{b7} held");
            }
            format!(
                "\u{21bb} {count} \u{b7} {} s",
                format_step_seconds(*step_seconds)
            )
        }
    }
}

/// The `period_seconds` inside a `PhasorConfig`-shaped struct value — the
/// one number a phasor speed knob displays and tracks. `None` for anything
/// that is not that record, so ordinary structs keep their no-live-display
/// posture.
pub fn phasor_config_period(value: &LpValue) -> Option<f32> {
    let LpValue::Struct {
        name: Some(name),
        fields,
    } = value
    else {
        return None;
    };
    if name != "PhasorConfig" {
        return None;
    }
    match fields
        .iter()
        .find(|(field, _)| field == "period_seconds")?
        .1
    {
        LpValue::F32(period) if period.is_finite() => Some(period),
        _ => None,
    }
}

/// A bus reading formatted for live display on a panel control (P6 item 1),
/// including the records that HAVE a panel presentation — the two of them:
/// a `PhasorConfig`, which displays as its period (the speed knob's
/// tracking value), and a `GradientConfig`, which displays as the palette
/// summary its swatch sits under (M4 P3).
///
/// A palette reading is the dense [`format_gradient_summary`] line rather
/// than the compact chip: this string is what the popup and the readout's
/// tooltip carry, and a channel-driven palette is exactly the case where
/// "which palette is this" needs to be answerable in words.
///
/// This is the TEXT half only. The strip shows the driven config itself,
/// which reaches the control structurally beside this string
/// ([`crate::UiPanelControl::live_gradient`]) — a `GradientConfig` cannot be
/// recovered from a summary, so a swatch fed only this could never draw the
/// palette it had just written.
pub fn format_live_panel_value(value: &LpValue) -> Option<String> {
    if let Some(config) = gradient_config_value(value) {
        return Some(format_gradient_summary(&config));
    }
    match phasor_config_period(value) {
        Some(period) => format_live_scalar(&LpValue::F32(period)),
        None => format_live_scalar(value),
    }
}

/// A scalar bus reading formatted for live display on a panel control
/// (P6 item 1). Floats are QUANTIZED to at most 2 decimals BEFORE the
/// string enters any DTO, so a slowly-drifting channel only dirties the
/// whole-DTO change gate when the displayed reading actually moves.
/// Non-scalar values (vectors, products, structs) have no panel-control
/// presentation and return `None`.
pub fn format_live_scalar(value: &LpValue) -> Option<String> {
    match value {
        LpValue::F32(value) if value.is_finite() => {
            let rounded = (value * 100.0).round() / 100.0;
            Some(if rounded.fract() == 0.0 {
                format!("{rounded:.1}")
            } else {
                rounded.to_string()
            })
        }
        LpValue::I32(value) => Some(value.to_string()),
        LpValue::U32(value) => Some(value.to_string()),
        LpValue::Bool(value) => Some(value.to_string()),
        // A `Kind::Choice` channel carries its variant's wire tag — the
        // clock's `play_state` is the first one a control reads back (P8),
        // and without this branch a run/pause panel write had no echo path
        // at all: the button would have stayed on the authored default
        // until a probe happened to land.
        LpValue::String(value) => Some(value.clone()),
        _ => None,
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
        ProductRef::Time(product) => {
            format!(
                "time product node {} output {}",
                product.node(),
                product.output()
            )
        }
    }
}

/// `space · method · N stops` — what one held palette is.
fn format_static_gradient(gradient: &Gradient) -> String {
    let stops = gradient.stops.len();
    let unit = if stops == 1 { "stop" } else { "stops" };
    format!(
        "{} \u{b7} {} \u{b7} {stops} {unit}",
        gradient.space.as_str(),
        gradient.method.as_str()
    )
}

/// A cycle step in the gate-picked Step voice: whole seconds print bare
/// (`4`), sub-second and fractional steps keep their (trimmed) decimals
/// (`0.5`), matching the decision render's `every 4 s` / `every 0.5 s`.
fn format_step_seconds(value: f32) -> String {
    if value.is_finite() && value.fract() == 0.0 {
        format!("{}", value as i64)
    } else {
        let rounded = (value * 1000.0).round() / 1000.0;
        rounded.to_string()
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
    use lpc_model::{
        Colorspace, ControlExtent, ControlProduct, Gradient, GradientStop, InterpMethod, LpValue,
        NodeId, ProductRef, ToLpValue, VisualProduct,
    };

    use super::*;

    fn ramp(stops: usize) -> Gradient {
        Gradient {
            space: Colorspace::Oklab,
            method: InterpMethod::Linear,
            stops: (0..stops)
                .map(|index| GradientStop {
                    at: index as f32 / (stops - 1) as f32,
                    c: [index as f32 / stops as f32, 0.1, -0.1],
                })
                .collect(),
        }
    }

    /// Both storage forms read as a palette: the `GradientConfig` record and
    /// a bare `Gradient` (which is one palette, held).
    #[test]
    fn recognizes_both_gradient_storage_forms() {
        assert_eq!(
            gradient_config_value(&ramp(3).to_lp_value()),
            Some(GradientConfig::Static(ramp(3)))
        );
        let cycle = GradientConfig::Cycle {
            set: vec![ramp(2), ramp(3)],
            step_seconds: 20.0,
            fade_seconds: 0.5,
        };
        assert_eq!(gradient_config_value(&cycle.to_lp_value()), Some(cycle));
        // Ordinary structs keep their generic display.
        assert_eq!(
            gradient_config_value(&LpValue::Struct {
                name: Some("Dim2u".to_string()),
                fields: vec![("width".to_string(), LpValue::U32(16))],
            }),
            None
        );
        assert_eq!(gradient_config_value(&LpValue::F32(1.0)), None);
    }

    /// The summary is what every TEXT surface shows for a palette — never
    /// the raw storage dump.
    #[test]
    fn summarizes_palettes_instead_of_dumping_storage() {
        assert_eq!(
            format_lp_value(&ramp(3).to_lp_value()),
            "oklab \u{b7} linear \u{b7} 3 stops"
        );
        assert_eq!(
            format_lp_value(
                &GradientConfig::Cycle {
                    set: vec![ramp(2), ramp(3), ramp(4)],
                    step_seconds: 20.0,
                    fade_seconds: 0.5,
                }
                .to_lp_value()
            ),
            // The step speaks the gate-picked plain-seconds voice.
            "cycle \u{b7} 3 palettes \u{b7} every 20 s \u{b7} 0.5 s fade"
        );
        // A frozen cycle has no rate to state.
        assert_eq!(
            format_gradient_summary(&GradientConfig::Cycle {
                set: vec![ramp(2), ramp(2)],
                step_seconds: 0.0,
                fade_seconds: 0.0,
            }),
            "cycle \u{b7} 2 palettes \u{b7} held"
        );
    }

    /// The panel chip is the summary trimmed to a control's value slot: a
    /// held palette counts stops, a cycle counts members and states the
    /// auto-denominated step rate.
    #[test]
    fn the_panel_chip_is_short_enough_to_sit_beside_a_swatch() {
        assert_eq!(
            format_gradient_chip(&GradientConfig::Static(ramp(5))),
            "5 stops"
        );
        assert_eq!(
            format_gradient_chip(&GradientConfig::Cycle {
                set: vec![ramp(2), ramp(3), ramp(4), ramp(5)],
                step_seconds: 20.0,
                fade_seconds: 0.5,
            }),
            "\u{21bb} 4 \u{b7} 20 s"
        );
        // A frozen cycle states no rate here either.
        assert_eq!(
            format_gradient_chip(&GradientConfig::Cycle {
                set: vec![ramp(2), ramp(3)],
                step_seconds: 0.0,
                fade_seconds: 0.0,
            }),
            "\u{21bb} 2 \u{b7} held"
        );
    }

    /// A palette channel HAS a panel presentation, so its live reading must
    /// not fall through the scalar filter the way any other struct does.
    #[test]
    fn a_palette_channel_reading_reaches_the_panel_readout() {
        assert_eq!(
            format_live_panel_value(&ramp(3).to_lp_value()).as_deref(),
            Some("oklab \u{b7} linear \u{b7} 3 stops")
        );
        assert_eq!(
            format_live_panel_value(
                &GradientConfig::Cycle {
                    set: vec![ramp(2), ramp(3)],
                    step_seconds: 20.0,
                    fade_seconds: 0.5,
                }
                .to_lp_value()
            )
            .as_deref(),
            Some("cycle \u{b7} 2 palettes \u{b7} every 20 s \u{b7} 0.5 s fade")
        );
        // The phasor presentation is untouched, and every other struct still
        // has no panel readout at all.
        assert_eq!(
            format_live_panel_value(&LpValue::Struct {
                name: Some("Dim2u".to_string()),
                fields: vec![("width".to_string(), LpValue::U32(16))],
            }),
            None
        );
    }

    #[test]
    fn live_scalar_quantizes_floats_and_skips_non_scalars() {
        assert_eq!(
            format_live_scalar(&LpValue::F32(2.71828)).as_deref(),
            Some("2.72")
        );
        assert_eq!(
            format_live_scalar(&LpValue::F32(3.0)).as_deref(),
            Some("3.0")
        );
        assert_eq!(format_live_scalar(&LpValue::U32(7)).as_deref(), Some("7"));
        assert_eq!(
            format_live_scalar(&LpValue::Bool(true)).as_deref(),
            Some("true")
        );
        assert_eq!(format_live_scalar(&LpValue::F32(f32::NAN)), None);
        assert_eq!(format_live_scalar(&LpValue::Vec2([1.0, 2.0])), None);
    }

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
