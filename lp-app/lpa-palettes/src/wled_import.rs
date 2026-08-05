//! WLED custom-palette JSON import (M3 scope: the conversion function and
//! its tests only — the paste/drop UI affordance is M4's).
//!
//! WLED's `palette0.json`..`palette9.json` custom-palette files hold one
//! flat array under a `"palette"` key, in one of two forms (both stop
//! positions and RGB components are `0..=255`):
//!
//! - `[stop, R, G, B, stop, R, G, B, ...]` — 4 numbers per stop.
//! - `[stop, "RRGGBB", stop, "RRGGBB", ...]` — a stop number then a hex
//!   string, 2 elements per stop.
//!
//! The form is auto-detected from the second array element's JSON type.

use lpc_model::{Colorspace, Gradient, GradientError, GradientStop, InterpMethod};
use serde_json::Value;

/// Why a WLED custom-palette JSON payload could not be imported.
#[derive(Debug)]
pub enum WledImportError {
    /// The payload was not valid JSON.
    Json(serde_json::Error),
    /// Neither a `{"palette": [...]}` object nor a bare `[...]` array.
    NotAnArray,
    /// The array was empty.
    Empty,
    /// The array's length isn't a multiple of the detected form's chunk
    /// size (4 for the RGB form, 2 for the hex form).
    MalformedLength { len: usize, chunk_size: usize },
    /// The stop-position element (chunk index 0) at this stop index wasn't
    /// a JSON number.
    BadStopPosition(usize),
    /// The color element(s) at this stop index weren't the expected shape
    /// (three JSON numbers for the RGB form, one 6-hex-digit string for the
    /// hex form).
    BadColor(usize),
    /// The converted [`Gradient`] failed [`Gradient::validate`] (e.g. more
    /// than [`lpc_model::MAX_GRADIENT_STOPS`] stops, or a stop position
    /// outside `[0, 255]` in the source data).
    Invalid(GradientError),
}

impl core::fmt::Display for WledImportError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Json(error) => write!(f, "invalid JSON: {error}"),
            Self::NotAnArray => {
                write!(
                    f,
                    "expected a {{\"palette\": [...]}} object or a bare array"
                )
            }
            Self::Empty => write!(f, "palette array is empty"),
            Self::MalformedLength { len, chunk_size } => write!(
                f,
                "palette array length {len} is not a multiple of {chunk_size}"
            ),
            Self::BadStopPosition(index) => write!(f, "stop {index}: position is not a number"),
            Self::BadColor(index) => write!(f, "stop {index}: malformed color"),
            Self::Invalid(error) => write!(f, "converted gradient is invalid: {error}"),
        }
    }
}

impl std::error::Error for WledImportError {}

impl From<serde_json::Error> for WledImportError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

/// Import a WLED custom-palette JSON payload (either array form) into a
/// validated [`Gradient`] (`space: Srgb, method: Linear` — the import
/// fidelity convention; WLED itself only ever linearly interpolates
/// `CRGBPalette16`-derived stops).
pub fn import_wled_custom_palette(json: &str) -> Result<Gradient, WledImportError> {
    let value: Value = serde_json::from_str(json)?;
    let array = extract_array(&value)?;
    if array.is_empty() {
        return Err(WledImportError::Empty);
    }

    let hex_form = matches!(array.get(1), Some(Value::String(_)));
    let chunk_size = if hex_form { 2 } else { 4 };
    if array.len() % chunk_size != 0 {
        return Err(WledImportError::MalformedLength {
            len: array.len(),
            chunk_size,
        });
    }

    let mut stops = Vec::with_capacity(array.len() / chunk_size);
    for (index, chunk) in array.chunks(chunk_size).enumerate() {
        let position = chunk[0]
            .as_f64()
            .ok_or(WledImportError::BadStopPosition(index))?;
        let (r, g, b) = if hex_form {
            let hex = chunk[1].as_str().ok_or(WledImportError::BadColor(index))?;
            parse_hex_rgb(hex).ok_or(WledImportError::BadColor(index))?
        } else {
            let r = chunk[1].as_f64().ok_or(WledImportError::BadColor(index))?;
            let g = chunk[2].as_f64().ok_or(WledImportError::BadColor(index))?;
            let b = chunk[3].as_f64().ok_or(WledImportError::BadColor(index))?;
            (r, g, b)
        };

        stops.push(GradientStop {
            at: (position / 255.0) as f32,
            c: [(r / 255.0) as f32, (g / 255.0) as f32, (b / 255.0) as f32],
        });
    }

    let gradient = Gradient {
        space: Colorspace::Srgb,
        method: InterpMethod::Linear,
        stops,
    };
    gradient.validate().map_err(WledImportError::Invalid)?;
    Ok(gradient)
}

fn extract_array(value: &Value) -> Result<&[Value], WledImportError> {
    match value {
        Value::Array(array) => Ok(array),
        Value::Object(map) => match map.get("palette") {
            Some(Value::Array(array)) => Ok(array),
            _ => Err(WledImportError::NotAnArray),
        },
        _ => Err(WledImportError::NotAnArray),
    }
}

/// Parse a `"RRGGBB"` (optionally `#RRGGBB`) hex string into `0..=255`
/// float components.
fn parse_hex_rgb(hex: &str) -> Option<(f64, f64, f64)> {
    let hex = hex.strip_prefix('#').unwrap_or(hex);
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some((f64::from(r), f64::from(g), f64::from(b)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_the_rgb_quadruple_form() {
        let gradient =
            import_wled_custom_palette(r#"{"palette":[0,255,0,0,127,0,255,0,255,0,0,255]}"#)
                .unwrap();

        assert_eq!(gradient.space, Colorspace::Srgb);
        assert_eq!(gradient.method, InterpMethod::Linear);
        assert_eq!(gradient.stops.len(), 3);
        assert_eq!(gradient.stops[0].at, 0.0);
        assert_eq!(gradient.stops[0].c, [1.0, 0.0, 0.0]);
        assert_eq!(gradient.stops[2].at, 1.0);
        assert_eq!(gradient.stops[2].c, [0.0, 0.0, 1.0]);
        assert_eq!(gradient.validate(), Ok(()));
    }

    #[test]
    fn imports_the_hex_string_form() {
        let gradient =
            import_wled_custom_palette(r#"{"palette":[0,"FF0000",255,"0000FF"]}"#).unwrap();

        assert_eq!(gradient.stops.len(), 2);
        assert_eq!(gradient.stops[0].c, [1.0, 0.0, 0.0]);
        assert_eq!(gradient.stops[1].c, [0.0, 0.0, 1.0]);
    }

    #[test]
    fn accepts_a_hex_string_with_a_leading_hash() {
        let gradient = import_wled_custom_palette(r##"[0,"#FF8800",255,"#0044FF"]"##).unwrap();
        assert_eq!(
            gradient.stops[0].c,
            [1.0, 136.0 / 255.0, 0.0],
            "0xFF8800 rgb"
        );
    }

    #[test]
    fn accepts_a_bare_array_without_the_palette_wrapper() {
        let gradient = import_wled_custom_palette(r#"[0,0,0,0,255,255,255,255]"#).unwrap();
        assert_eq!(gradient.stops.len(), 2);
    }

    #[test]
    fn rejects_malformed_json() {
        assert!(matches!(
            import_wled_custom_palette("not json"),
            Err(WledImportError::Json(_))
        ));
    }

    #[test]
    fn rejects_a_payload_that_is_neither_object_nor_array() {
        assert!(matches!(
            import_wled_custom_palette("42"),
            Err(WledImportError::NotAnArray)
        ));
    }

    #[test]
    fn rejects_an_empty_palette() {
        assert!(matches!(
            import_wled_custom_palette(r#"{"palette":[]}"#),
            Err(WledImportError::Empty)
        ));
    }

    #[test]
    fn rejects_a_length_that_does_not_match_the_detected_form() {
        // Second element is a number => RGB form (chunk size 4); 5 is not a
        // multiple of 4.
        assert!(matches!(
            import_wled_custom_palette(r#"{"palette":[0,255,0,0,255]}"#),
            Err(WledImportError::MalformedLength {
                len: 5,
                chunk_size: 4
            })
        ));
    }

    #[test]
    fn rejects_a_malformed_hex_color() {
        assert!(matches!(
            import_wled_custom_palette(r#"{"palette":[0,"ZZZZZZ",255,"FFFFFF"]}"#),
            Err(WledImportError::BadColor(0))
        ));
    }

    #[test]
    fn rejects_more_than_max_gradient_stops_rather_than_truncating() {
        // 25 stops in RGB form: never silently truncate (color.md rule).
        let mut values = Vec::new();
        for index in 0..25 {
            values.push((index * 10).to_string());
            values.push("255".to_string());
            values.push("0".to_string());
            values.push("0".to_string());
        }
        let json = format!(r#"{{"palette":[{}]}}"#, values.join(","));

        assert!(matches!(
            import_wled_custom_palette(&json),
            Err(WledImportError::Invalid(GradientError::TooManyStops(25)))
        ));
    }

    #[test]
    fn wled_stop_range_0_to_255_maps_onto_0_to_1() {
        let gradient =
            import_wled_custom_palette(r#"{"palette":[0,0,0,0,64,10,10,10,255,20,20,20]}"#)
                .unwrap();
        assert_eq!(gradient.stops[1].at, 64.0 / 255.0);
    }
}
