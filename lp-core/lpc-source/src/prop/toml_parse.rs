//! TOML helpers shared by [`crate::prop::src_value_spec::SrcValueSpec`] and color/palette/gradient parsing.

use alloc::string::String;
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;

use lpc_model::WireValue;

/// Error from `SrcValueSpec::from_toml_for_kind` / `SrcValueSpec::from_toml_for_shape` and their inverses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FromTomlError(pub String);

impl FromTomlError {
    pub(crate) fn msg(s: &'static str) -> Self {
        FromTomlError(String::from(s))
    }
}

impl core::fmt::Display for FromTomlError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<String> for FromTomlError {
    fn from(s: String) -> Self {
        FromTomlError(s)
    }
}

impl core::error::Error for FromTomlError {}

pub(crate) fn toml_f32(v: &toml::Value) -> Result<f32, FromTomlError> {
    match v {
        toml::Value::Float(f) => Ok(*f as f32),
        toml::Value::Integer(i) => Ok(*i as f32),
        _ => Err(FromTomlError::msg(
            "expected a TOML number (float or integer)",
        )),
    }
}

pub(crate) fn toml_i32(v: &toml::Value) -> Result<i32, FromTomlError> {
    v.as_integer()
        .and_then(|i| i32::try_from(i).ok())
        .ok_or_else(|| FromTomlError::msg("expected a TOML integer"))
}

pub(crate) fn vec3_from_toml(v: &toml::Value, _ctx: &str) -> Result<WireValue, FromTomlError> {
    let a = v
        .as_array()
        .ok_or_else(|| FromTomlError::msg("expected a 3-long TOML array"))?;
    if a.len() != 3 {
        return Err(FromTomlError::msg("expected exactly 3 coordinates"));
    }
    let x = toml_f32(&a[0])?;
    let y = toml_f32(&a[1])?;
    let z = toml_f32(&a[2])?;
    Ok(WireValue::Vec3([x, y, z]))
}

pub(crate) fn vec_n_from_toml(
    v: &toml::Value,
    n: usize,
    _ctx: &str,
) -> Result<WireValue, FromTomlError> {
    let a = v
        .as_array()
        .ok_or_else(|| FromTomlError::msg("expected a TOML array for position default"))?;
    if a.len() != n {
        return Err(FromTomlError::msg("position default: wrong array length"));
    }
    if n == 2 {
        let x = toml_f32(&a[0])?;
        let y = toml_f32(&a[1])?;
        return Ok(WireValue::Vec2([x, y]));
    }
    if n == 3 {
        return vec3_from_toml(v, "position3d");
    }
    Err(FromTomlError::msg("internal: bad vec_n"))
}

pub(crate) fn wire_value_audio_level(
    t: &toml::map::Map<String, toml::Value>,
) -> Result<WireValue, FromTomlError> {
    let low = t
        .get("low")
        .ok_or_else(|| FromTomlError::msg("audio_level: missing `low`"))?;
    let mid = t
        .get("mid")
        .ok_or_else(|| FromTomlError::msg("audio_level: missing `mid`"))?;
    let high = t
        .get("high")
        .ok_or_else(|| FromTomlError::msg("audio_level: missing `high`"))?;
    Ok(WireValue::Struct {
        name: Some(String::from("AudioLevel")),
        fields: vec![
            (String::from("low"), WireValue::F32(toml_f32(low)?)),
            (String::from("mid"), WireValue::F32(toml_f32(mid)?)),
            (String::from("high"), WireValue::F32(toml_f32(high)?)),
        ],
    })
}

pub(crate) fn wire_audio_level_to_toml(v: &WireValue) -> Result<toml::Value, FromTomlError> {
    let WireValue::Struct { name, fields } = v else {
        return Err(FromTomlError::msg("AudioLevel must be a struct WireValue"));
    };
    if name.as_deref() != Some("AudioLevel") {
        return Err(FromTomlError::msg("AudioLevel: wrong struct name"));
    }
    let low = find_field_f32(fields, "low")?;
    let mid = find_field_f32(fields, "mid")?;
    let high = find_field_f32(fields, "high")?;
    let mut m: toml::map::Map<String, toml::Value> = toml::map::Map::new();
    m.insert("low".to_string(), toml::Value::Float(f64::from(low)));
    m.insert("mid".to_string(), toml::Value::Float(f64::from(mid)));
    m.insert("high".to_string(), toml::Value::Float(f64::from(high)));
    Ok(toml::Value::Table(m))
}

pub(crate) fn find_field_f32(
    fields: &[(String, WireValue)],
    key: &str,
) -> Result<f32, FromTomlError> {
    let v = fields
        .iter()
        .find(|(k, _)| k == key)
        .ok_or_else(|| FromTomlError::msg("missing f32 field"))?
        .1
        .clone();
    match v {
        WireValue::F32(f) => Ok(f),
        _ => Err(FromTomlError::msg("not F32")),
    }
}

pub(crate) fn find_field_i32(
    fields: &[(String, WireValue)],
    key: &str,
) -> Result<i32, FromTomlError> {
    let v = fields
        .iter()
        .find(|(k, _)| k == key)
        .ok_or_else(|| FromTomlError::msg("missing I32 field"))?
        .1
        .clone();
    match v {
        WireValue::I32(i) => Ok(i),
        _ => Err(FromTomlError::msg("not I32")),
    }
}

fn find_field_vec3(fields: &[(String, WireValue)], key: &str) -> Result<WireValue, FromTomlError> {
    let v = fields
        .iter()
        .find(|(k, _)| k == key)
        .ok_or_else(|| FromTomlError::msg("missing field"))?
        .1
        .clone();
    if matches!(&v, WireValue::Vec3(_)) {
        return Ok(v);
    }
    Err(FromTomlError::msg("not Vec3"))
}

pub(crate) fn find_field_vec3_value(
    fields: &[(String, WireValue)],
    key: &str,
) -> Result<[f32; 3], FromTomlError> {
    let v = find_field_vec3(fields, key)?;
    match v {
        WireValue::Vec3(a) => Ok(a),
        _ => Err(FromTomlError::msg("not Vec3")),
    }
}

pub(crate) fn find_field_array(
    fields: &[(String, WireValue)],
    key: &str,
) -> Result<Vec<WireValue>, FromTomlError> {
    let v = fields
        .iter()
        .find(|(k, _)| k == key)
        .ok_or_else(|| FromTomlError::msg("missing array field"))?
        .1
        .clone();
    match v {
        WireValue::Array(b) => Ok(b.iter().cloned().collect()),
        _ => Err(FromTomlError::msg("not array")),
    }
}

pub(crate) fn slice_to_vec3_toml(s: &[WireValue]) -> Result<Vec<toml::Value>, FromTomlError> {
    let mut out = Vec::with_capacity(s.len());
    for e in s {
        let WireValue::Vec3(a) = e else {
            return Err(FromTomlError::msg("entry not Vec3"));
        };
        out.push(vec3_to_toml_array(a)?);
    }
    Ok(out)
}

pub(crate) fn vec2_to_toml_value(v: &WireValue) -> Result<toml::Value, FromTomlError> {
    let WireValue::Vec2(a) = v else {
        return Err(FromTomlError::msg(
            "position2d literal must be Vec2 wire value",
        ));
    };
    Ok(toml::Value::Array(vec![
        toml::Value::Float(f64::from(a[0])),
        toml::Value::Float(f64::from(a[1])),
    ]))
}

pub(crate) fn vec3_to_toml_value(v: &WireValue) -> Result<toml::Value, FromTomlError> {
    let WireValue::Vec3(a) = v else {
        return Err(FromTomlError::msg(
            "position3d literal must be Vec3 wire value",
        ));
    };
    vec3_to_toml_array(a)
}

pub(crate) fn vec3_to_toml_array(a: &[f32; 3]) -> Result<toml::Value, FromTomlError> {
    Ok(toml::Value::Array(vec![
        toml::Value::Float(f64::from(a[0])),
        toml::Value::Float(f64::from(a[1])),
        toml::Value::Float(f64::from(a[2])),
    ]))
}
