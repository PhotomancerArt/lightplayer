//! TOML → [`crate::manifest::FxManifest`] with validation.

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use serde::Deserialize;

use crate::error::FxError;
use crate::input::{FxChoice, FxInputDef, FxInputType, FxPresentation, FxValue};
use crate::manifest::{FxManifest, FxMeta, FxResolution};

#[derive(Debug, Deserialize)]
struct RawManifest {
    meta: RawMeta,
    resolution: Option<RawResolution>,
    input: Option<BTreeMap<String, RawInputDef>>,
}

#[derive(Debug, Deserialize)]
struct RawMeta {
    name: String,
    description: Option<String>,
    author: Option<String>,
    tags: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct RawResolution {
    width: Option<u32>,
    height: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct RawInputDef {
    #[serde(rename = "type")]
    input_type: String,
    label: Option<String>,
    default: Option<toml::Value>,
    min: Option<toml::Value>,
    max: Option<toml::Value>,
    /// Simple widgets: `ui = "slider"`. Structured: `ui = { choices = [...] }`.
    ui: Option<RawUi>,
    unit: Option<String>,
    role: Option<String>,
}

/// Deserializes either `ui = "slider"` or `ui = { choices = [...] }`.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawUi {
    Simple(String),
    WithChoices(RawUiChoices),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawUiChoices {
    choices: Vec<RawChoice>,
}

#[derive(Debug, Deserialize)]
struct RawChoice {
    value: i32,
    label: String,
}

const DEFAULT_RES: FxResolution = FxResolution {
    width: 512,
    height: 512,
};

/// Parse and validate `fx.toml` contents.
pub fn parse_manifest(toml_src: &str) -> Result<FxManifest, FxError> {
    let raw: RawManifest = toml::from_str(toml_src)?;
    validate_meta(&raw.meta)?;
    let resolution = raw_resolution(&raw.resolution)?;
    let inputs = validate_inputs(raw.input.unwrap_or_default())?;
    Ok(FxManifest {
        meta: FxMeta {
            name: raw.meta.name,
            description: raw.meta.description,
            author: raw.meta.author,
            tags: raw.meta.tags.unwrap_or_default(),
        },
        resolution,
        inputs,
    })
}

fn validate_meta(meta: &RawMeta) -> Result<(), FxError> {
    if meta.name.trim().is_empty() {
        return Err(FxError::MissingField {
            section: "meta",
            field: "name",
        });
    }
    Ok(())
}

fn raw_resolution(raw: &Option<RawResolution>) -> Result<FxResolution, FxError> {
    let Some(r) = raw else {
        return Ok(DEFAULT_RES);
    };
    let width = r.width.unwrap_or(DEFAULT_RES.width);
    let height = r.height.unwrap_or(DEFAULT_RES.height);
    if width == 0 || height == 0 {
        return Err(FxError::ValidationError(String::from(
            "[resolution] width and height must be non-zero",
        )));
    }
    Ok(FxResolution { width, height })
}

fn validate_inputs(map: BTreeMap<String, RawInputDef>) -> Result<BTreeMap<String, FxInputDef>, FxError> {
    let mut out = BTreeMap::new();
    for (name, raw) in map {
        let def = validate_input(&name, raw)?;
        out.insert(name, def);
    }
    Ok(out)
}

fn validate_input(name: &str, raw: RawInputDef) -> Result<FxInputDef, FxError> {
    let input_type = parse_input_type(name, &raw.input_type)?;
    let (presentation, choices) = resolve_ui(name, raw.ui)?;

    if matches!(presentation, Some(FxPresentation::Choice)) && input_type != FxInputType::I32 {
        return Err(FxError::ValidationError(format!(
            "input `{name}`: `ui = {{ choices = [...] }}` requires type \"i32\""
        )));
    }

    numeric_min_max_only(name, input_type, raw.min.is_some(), raw.max.is_some())?;

    let default = match raw.default {
        None => None,
        Some(v) => Some(value_from_toml(name, input_type, &v, "default")?),
    };

    let min = match raw.min {
        None => None,
        Some(v) => Some(value_from_toml(name, input_type, &v, "min")?),
    };

    let max = match raw.max {
        None => None,
        Some(v) => Some(value_from_toml(name, input_type, &v, "max")?),
    };

    Ok(FxInputDef {
        input_type,
        label: raw.label,
        default,
        min,
        max,
        presentation,
        choices,
        unit: raw.unit,
        role: raw.role,
    })
}

fn resolve_ui(
    name: &str,
    ui: Option<RawUi>,
) -> Result<(Option<FxPresentation>, Option<Vec<FxChoice>>), FxError> {
    match ui {
        None => Ok((None, None)),
        Some(RawUi::Simple(s)) => {
            let p = parse_ui_simple(name, &s)?;
            Ok((Some(p), None))
        }
        Some(RawUi::WithChoices(RawUiChoices { choices })) => {
            if choices.is_empty() {
                return Err(FxError::ValidationError(format!(
                    "input `{name}`: `ui.choices` must be non-empty"
                )));
            }
            let rows = choices
                .into_iter()
                .map(|c| FxChoice {
                    value: c.value,
                    label: c.label,
                })
                .collect();
            Ok((Some(FxPresentation::Choice), Some(rows)))
        }
    }
}

fn numeric_min_max_only(
    name: &str,
    ty: FxInputType,
    has_min: bool,
    has_max: bool,
) -> Result<(), FxError> {
    if !has_min && !has_max {
        return Ok(());
    }
    match ty {
        FxInputType::F32 | FxInputType::I32 => Ok(()),
        _ => Err(FxError::ValidationError(format!(
            "input `{name}`: `min`/`max` are only allowed for `f32` and `i32` inputs"
        ))),
    }
}

fn parse_input_type(input: &str, s: &str) -> Result<FxInputType, FxError> {
    match s {
        "f32" => Ok(FxInputType::F32),
        "i32" => Ok(FxInputType::I32),
        "bool" => Ok(FxInputType::Bool),
        "vec3" => Ok(FxInputType::Vec3),
        "Color" => Ok(FxInputType::Color),
        "Palette" => Ok(FxInputType::Palette),
        _ => Err(FxError::InvalidType {
            input: String::from(input),
            found: String::from(s),
        }),
    }
}

/// Parses `ui = "…"` string form. Choice controls use `ui = { choices = [...] }` instead.
fn parse_ui_simple(input: &str, s: &str) -> Result<FxPresentation, FxError> {
    match s {
        "slider" => Ok(FxPresentation::Slider),
        "toggle" => Ok(FxPresentation::Toggle),
        "colorpicker" | "color_picker" => Ok(FxPresentation::ColorPicker),
        "palettepicker" | "palette_picker" => Ok(FxPresentation::PalettePicker),
        "choice" => Err(FxError::ValidationError(format!(
            "input `{input}`: use `ui = {{ choices = [...] }}` instead of `ui = \"choice\"`"
        ))),
        _ => Err(FxError::InvalidUi {
            input: String::from(input),
            found: String::from(s),
        }),
    }
}

fn value_from_toml(
    input: &str,
    ty: FxInputType,
    v: &toml::Value,
    field: &'static str,
) -> Result<FxValue, FxError> {
    let mismatch = |expected: &'static str| {
        Err(FxError::DefaultTypeMismatch {
            input: String::from(input),
            expected: String::from(expected),
            found: format!("{v:?}"),
        })
    };

    match ty {
        FxInputType::F32 => match v {
            toml::Value::Float(f) => Ok(FxValue::F32(*f as f32)),
            toml::Value::Integer(i) => Ok(FxValue::F32(*i as f32)),
            _ => mismatch("f32"),
        },
        FxInputType::I32 => match v {
            toml::Value::Integer(i) => Ok(FxValue::I32(*i as i32)),
            _ => mismatch("i32"),
        },
        FxInputType::Bool => match v {
            toml::Value::Boolean(b) => Ok(FxValue::Bool(*b)),
            _ => mismatch("bool"),
        },
        FxInputType::Vec3 => vec3_from_toml(input, v, field),
        FxInputType::Color => Err(FxError::ValidationError(format!(
            "input `{input}`: `{field}` for type Color is not supported yet"
        ))),
        FxInputType::Palette => Err(FxError::ValidationError(format!(
            "input `{input}`: `{field}` for type Palette is not supported yet"
        ))),
    }
}

fn vec3_from_toml(
    input: &str,
    v: &toml::Value,
    field: &'static str,
) -> Result<FxValue, FxError> {
    let arr = v.as_array().ok_or_else(|| FxError::DefaultTypeMismatch {
        input: String::from(input),
        expected: String::from("[f32, f32, f32]"),
        found: format!("{v:?}"),
    })?;
    if arr.len() != 3 {
        return Err(FxError::ValidationError(format!(
            "input `{input}`: `{field}` vec3 must have exactly 3 elements",
        )));
    }
    let mut out = [0.0_f32; 3];
    for (i, item) in arr.iter().enumerate() {
        out[i] = match item {
            toml::Value::Float(f) => *f as f32,
            toml::Value::Integer(n) => *n as f32,
            _ => {
                return Err(FxError::DefaultTypeMismatch {
                    input: String::from(input),
                    expected: String::from("number"),
                    found: format!("{item:?}"),
                });
            }
        };
    }
    Ok(FxValue::Vec3(out))
}
