//! Byte-backed GLSL data with [`LayoutRules`] and path access.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::GlslValue;
use crate::data_error::GlslDataError;
use crate::layout::{array_stride, round_up};
use crate::metadata::{GlslType, LayoutRules, StructMember};

/// Memory-backed GLSL data: type + layout + bytes (little-endian scalars).
pub struct GlslData {
    ty: GlslType,
    rules: LayoutRules,
    data: Vec<u8>,
}

impl GlslData {
    pub fn new(ty: GlslType) -> Self {
        Self::with_rules(ty, LayoutRules::Std430).expect("std430 is implemented")
    }

    pub fn with_rules(ty: GlslType, rules: LayoutRules) -> Result<Self, GlslDataError> {
        if !rules.is_implemented() {
            return Err(GlslDataError::LayoutNotImplemented);
        }
        let n = ty.size(rules);
        Ok(Self {
            ty,
            rules,
            data: alloc::vec![0u8; n],
        })
    }

    pub fn from_value(ty: GlslType, value: &GlslValue) -> Result<Self, GlslDataError> {
        let mut s = Self::new(ty.clone());
        value_matches_type(&ty, value)?;
        write_value(&ty, s.rules, &mut s.data, value)?;
        Ok(s)
    }

    pub fn from_value_with_rules(
        ty: GlslType,
        rules: LayoutRules,
        value: &GlslValue,
    ) -> Result<Self, GlslDataError> {
        if !rules.is_implemented() {
            return Err(GlslDataError::LayoutNotImplemented);
        }
        let n = ty.size(rules);
        let mut data = alloc::vec![0u8; n];
        value_matches_type(&ty, value).map_err(|e| e)?;
        write_value(&ty, rules, &mut data, value)?;
        Ok(Self { ty, rules, data })
    }

    pub fn ty(&self) -> &GlslType {
        &self.ty
    }

    pub fn rules(&self) -> LayoutRules {
        self.rules
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.data
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.data
    }

    pub fn as_ptr(&self) -> *const u8 {
        self.data.as_ptr()
    }

    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        self.data.as_mut_ptr()
    }

    pub fn to_value(&self) -> Result<GlslValue, GlslDataError> {
        let need = self.ty.size(self.rules);
        if self.data.len() < need {
            return Err(GlslDataError::BufferTooShort {
                need,
                have: self.data.len(),
            });
        }
        read_value(&self.ty, self.rules, &self.data[..need])
    }

    pub fn offset_of(&self, path: &str) -> Result<usize, GlslDataError> {
        Ok(self.ty.offset_for_path(path, self.rules, 0)?)
    }

    pub fn get(&self, path: &str) -> Result<GlslValue, GlslDataError> {
        let off = self.ty.offset_for_path(path, self.rules, 0)?;
        let leaf = self.ty.type_at_path(path)?;
        let need = leaf.size(self.rules);
        let end = off
            .checked_add(need)
            .ok_or_else(|| GlslDataError::type_mismatch("path", "offset overflow"))?;
        if self.data.len() < end {
            return Err(GlslDataError::BufferTooShort {
                need: end,
                have: self.data.len(),
            });
        }
        read_value(&leaf, self.rules, &self.data[off..end])
    }

    pub fn set(&mut self, path: &str, value: GlslValue) -> Result<(), GlslDataError> {
        let off = self.ty.offset_for_path(path, self.rules, 0)?;
        let leaf = self.ty.type_at_path(path)?;
        value_matches_type(&leaf, &value)?;
        let need = leaf.size(self.rules);
        let end = off
            .checked_add(need)
            .ok_or_else(|| GlslDataError::type_mismatch("path", "offset overflow"))?;
        if self.data.len() < end {
            return Err(GlslDataError::BufferTooShort {
                need: end,
                have: self.data.len(),
            });
        }
        write_value(&leaf, self.rules, &mut self.data[off..end], &value)
    }

    pub fn get_f32(&self, path: &str) -> Result<f32, GlslDataError> {
        if !matches!(self.ty.type_at_path(path)?, GlslType::Float) {
            return Err(GlslDataError::BadPathForScalar {
                path: String::from(path),
                expected_ty: String::from("float"),
            });
        }
        match self.get(path)? {
            GlslValue::F32(x) => Ok(x),
            _ => Err(GlslDataError::type_mismatch(
                "float",
                "internal decode error",
            )),
        }
    }

    pub fn set_f32(&mut self, path: &str, val: f32) -> Result<(), GlslDataError> {
        self.set(path, GlslValue::F32(val))
    }

    pub fn get_i32(&self, path: &str) -> Result<i32, GlslDataError> {
        if !matches!(self.ty.type_at_path(path)?, GlslType::Int) {
            return Err(GlslDataError::BadPathForScalar {
                path: String::from(path),
                expected_ty: String::from("int"),
            });
        }
        match self.get(path)? {
            GlslValue::I32(x) => Ok(x),
            _ => Err(GlslDataError::type_mismatch("int", "internal decode error")),
        }
    }

    pub fn set_i32(&mut self, path: &str, val: i32) -> Result<(), GlslDataError> {
        self.set(path, GlslValue::I32(val))
    }
}

fn member_key(m: &StructMember, idx: usize) -> String {
    m.name.clone().unwrap_or_else(|| format!("_{idx}"))
}

fn value_matches_type(ty: &GlslType, v: &GlslValue) -> Result<(), GlslDataError> {
    match (ty, v) {
        (GlslType::Float, GlslValue::F32(_)) => Ok(()),
        (GlslType::Int, GlslValue::I32(_)) => Ok(()),
        (GlslType::UInt, GlslValue::U32(_)) => Ok(()),
        (GlslType::Bool, GlslValue::Bool(_)) => Ok(()),
        (GlslType::Vec2, GlslValue::Vec2(_)) => Ok(()),
        (GlslType::Vec3, GlslValue::Vec3(_)) => Ok(()),
        (GlslType::Vec4, GlslValue::Vec4(_)) => Ok(()),
        (GlslType::IVec2, GlslValue::IVec2(_)) => Ok(()),
        (GlslType::IVec3, GlslValue::IVec3(_)) => Ok(()),
        (GlslType::IVec4, GlslValue::IVec4(_)) => Ok(()),
        (GlslType::UVec2, GlslValue::UVec2(_)) => Ok(()),
        (GlslType::UVec3, GlslValue::UVec3(_)) => Ok(()),
        (GlslType::UVec4, GlslValue::UVec4(_)) => Ok(()),
        (GlslType::BVec2, GlslValue::BVec2(_)) => Ok(()),
        (GlslType::BVec3, GlslValue::BVec3(_)) => Ok(()),
        (GlslType::BVec4, GlslValue::BVec4(_)) => Ok(()),
        (GlslType::Mat2, GlslValue::Mat2x2(_)) => Ok(()),
        (GlslType::Mat3, GlslValue::Mat3x3(_)) => Ok(()),
        (GlslType::Mat4, GlslValue::Mat4x4(_)) => Ok(()),
        (GlslType::Array { element, len }, GlslValue::Array(items)) => {
            if items.len() != *len as usize {
                return Err(GlslDataError::type_mismatch(
                    format!("array[{len}]"),
                    format!("got {} elements", items.len()),
                ));
            }
            for it in items.iter() {
                value_matches_type(element, it)?;
            }
            Ok(())
        }
        (GlslType::Struct { members, .. }, GlslValue::Struct { fields, .. }) => {
            if members.len() != fields.len() {
                return Err(GlslDataError::type_mismatch(
                    format!("struct with {} fields", members.len()),
                    format!("got {} fields", fields.len()),
                ));
            }
            for (i, m) in members.iter().enumerate() {
                let key = member_key(m, i);
                let (fname, fv) = &fields[i];
                if fname != &key {
                    return Err(GlslDataError::type_mismatch(
                        format!("field `{key}`"),
                        format!("got field name `{fname}`"),
                    ));
                }
                value_matches_type(&m.ty, fv)?;
            }
            Ok(())
        }
        _ => Err(GlslDataError::type_mismatch(
            format!("{ty:?}"),
            format!("value {v:?}"),
        )),
    }
}

fn read_value(ty: &GlslType, rules: LayoutRules, data: &[u8]) -> Result<GlslValue, GlslDataError> {
    let need = ty.size(rules);
    if data.len() < need {
        return Err(GlslDataError::BufferTooShort {
            need,
            have: data.len(),
        });
    }
    let data = &data[..need];
    Ok(match ty {
        GlslType::Void => {
            return Err(GlslDataError::type_mismatch(
                "void",
                "cannot load void value",
            ));
        }
        GlslType::Float => GlslValue::F32(f32_from_bytes(data)),
        GlslType::Int => GlslValue::I32(i32_from_bytes(data)),
        GlslType::UInt => GlslValue::U32(u32_from_bytes(data)),
        GlslType::Bool => GlslValue::Bool(i32_from_bytes(data) != 0),
        GlslType::Vec2 => {
            GlslValue::Vec2([f32_from_bytes(&data[0..4]), f32_from_bytes(&data[4..8])])
        }
        GlslType::Vec3 => GlslValue::Vec3([
            f32_from_bytes(&data[0..4]),
            f32_from_bytes(&data[4..8]),
            f32_from_bytes(&data[8..12]),
        ]),
        GlslType::Vec4 => GlslValue::Vec4([
            f32_from_bytes(&data[0..4]),
            f32_from_bytes(&data[4..8]),
            f32_from_bytes(&data[8..12]),
            f32_from_bytes(&data[12..16]),
        ]),
        GlslType::IVec2 => {
            GlslValue::IVec2([i32_from_bytes(&data[0..4]), i32_from_bytes(&data[4..8])])
        }
        GlslType::IVec3 => GlslValue::IVec3([
            i32_from_bytes(&data[0..4]),
            i32_from_bytes(&data[4..8]),
            i32_from_bytes(&data[8..12]),
        ]),
        GlslType::IVec4 => GlslValue::IVec4([
            i32_from_bytes(&data[0..4]),
            i32_from_bytes(&data[4..8]),
            i32_from_bytes(&data[8..12]),
            i32_from_bytes(&data[12..16]),
        ]),
        GlslType::UVec2 => {
            GlslValue::UVec2([u32_from_bytes(&data[0..4]), u32_from_bytes(&data[4..8])])
        }
        GlslType::UVec3 => GlslValue::UVec3([
            u32_from_bytes(&data[0..4]),
            u32_from_bytes(&data[4..8]),
            u32_from_bytes(&data[8..12]),
        ]),
        GlslType::UVec4 => GlslValue::UVec4([
            u32_from_bytes(&data[0..4]),
            u32_from_bytes(&data[4..8]),
            u32_from_bytes(&data[8..12]),
            u32_from_bytes(&data[12..16]),
        ]),
        GlslType::BVec2 => GlslValue::BVec2([
            i32_from_bytes(&data[0..4]) != 0,
            i32_from_bytes(&data[4..8]) != 0,
        ]),
        GlslType::BVec3 => GlslValue::BVec3([
            i32_from_bytes(&data[0..4]) != 0,
            i32_from_bytes(&data[4..8]) != 0,
            i32_from_bytes(&data[8..12]) != 0,
        ]),
        GlslType::BVec4 => GlslValue::BVec4([
            i32_from_bytes(&data[0..4]) != 0,
            i32_from_bytes(&data[4..8]) != 0,
            i32_from_bytes(&data[8..12]) != 0,
            i32_from_bytes(&data[12..16]) != 0,
        ]),
        GlslType::Mat2 => {
            let c0 = [f32_from_bytes(&data[0..4]), f32_from_bytes(&data[4..8])];
            let c1 = [f32_from_bytes(&data[8..12]), f32_from_bytes(&data[12..16])];
            GlslValue::Mat2x2([c0, c1])
        }
        GlslType::Mat3 => {
            let mut m = [[0f32; 3]; 3];
            for col in 0..3 {
                let base = col * 12;
                m[col] = [
                    f32_from_bytes(&data[base..base + 4]),
                    f32_from_bytes(&data[base + 4..base + 8]),
                    f32_from_bytes(&data[base + 8..base + 12]),
                ];
            }
            GlslValue::Mat3x3(m)
        }
        GlslType::Mat4 => {
            let mut m = [[0f32; 4]; 4];
            for col in 0..4 {
                let base = col * 16;
                m[col] = [
                    f32_from_bytes(&data[base..base + 4]),
                    f32_from_bytes(&data[base + 4..base + 8]),
                    f32_from_bytes(&data[base + 8..base + 12]),
                    f32_from_bytes(&data[base + 12..base + 16]),
                ];
            }
            GlslValue::Mat4x4(m)
        }
        GlslType::Array { element, len } => {
            let stride = array_stride(element, rules);
            let esz = element.size(rules);
            let mut elems = Vec::with_capacity(*len as usize);
            for i in 0..(*len as usize) {
                let base = i * stride;
                elems.push(read_value(element, rules, &data[base..base + esz])?);
            }
            GlslValue::Array(elems.into_boxed_slice())
        }
        GlslType::Struct { name, members } => {
            let mut cursor = 0usize;
            let mut fields = Vec::with_capacity(members.len());
            for (i, m) in members.iter().enumerate() {
                let a = m.ty.alignment(rules);
                cursor = round_up(cursor, a);
                let msz = m.ty.size(rules);
                let v = read_value(&m.ty, rules, &data[cursor..cursor + msz])?;
                fields.push((member_key(m, i), v));
                cursor += msz;
            }
            GlslValue::Struct {
                name: name.clone(),
                fields,
            }
        }
    })
}

fn write_value(
    ty: &GlslType,
    rules: LayoutRules,
    data: &mut [u8],
    value: &GlslValue,
) -> Result<(), GlslDataError> {
    let need = ty.size(rules);
    if data.len() < need {
        return Err(GlslDataError::BufferTooShort {
            need,
            have: data.len(),
        });
    }
    let data = &mut data[..need];
    match (ty, value) {
        (GlslType::Float, GlslValue::F32(x)) => write_f32(data, *x),
        (GlslType::Int, GlslValue::I32(x)) => write_i32(data, *x),
        (GlslType::UInt, GlslValue::U32(x)) => write_u32(data, *x),
        (GlslType::Bool, GlslValue::Bool(b)) => write_i32(data, if *b { 1 } else { 0 }),
        (GlslType::Vec2, GlslValue::Vec2(a)) => {
            write_f32(&mut data[0..4], a[0]);
            write_f32(&mut data[4..8], a[1]);
        }
        (GlslType::Vec3, GlslValue::Vec3(a)) => {
            write_f32(&mut data[0..4], a[0]);
            write_f32(&mut data[4..8], a[1]);
            write_f32(&mut data[8..12], a[2]);
        }
        (GlslType::Vec4, GlslValue::Vec4(a)) => {
            write_f32(&mut data[0..4], a[0]);
            write_f32(&mut data[4..8], a[1]);
            write_f32(&mut data[8..12], a[2]);
            write_f32(&mut data[12..16], a[3]);
        }
        (GlslType::IVec2, GlslValue::IVec2(a)) => {
            write_i32(&mut data[0..4], a[0]);
            write_i32(&mut data[4..8], a[1]);
        }
        (GlslType::IVec3, GlslValue::IVec3(a)) => {
            write_i32(&mut data[0..4], a[0]);
            write_i32(&mut data[4..8], a[1]);
            write_i32(&mut data[8..12], a[2]);
        }
        (GlslType::IVec4, GlslValue::IVec4(a)) => {
            write_i32(&mut data[0..4], a[0]);
            write_i32(&mut data[4..8], a[1]);
            write_i32(&mut data[8..12], a[2]);
            write_i32(&mut data[12..16], a[3]);
        }
        (GlslType::UVec2, GlslValue::UVec2(a)) => {
            write_u32(&mut data[0..4], a[0]);
            write_u32(&mut data[4..8], a[1]);
        }
        (GlslType::UVec3, GlslValue::UVec3(a)) => {
            write_u32(&mut data[0..4], a[0]);
            write_u32(&mut data[4..8], a[1]);
            write_u32(&mut data[8..12], a[2]);
        }
        (GlslType::UVec4, GlslValue::UVec4(a)) => {
            write_u32(&mut data[0..4], a[0]);
            write_u32(&mut data[4..8], a[1]);
            write_u32(&mut data[8..12], a[2]);
            write_u32(&mut data[12..16], a[3]);
        }
        (GlslType::BVec2, GlslValue::BVec2(a)) => {
            write_i32(&mut data[0..4], if a[0] { 1 } else { 0 });
            write_i32(&mut data[4..8], if a[1] { 1 } else { 0 });
        }
        (GlslType::BVec3, GlslValue::BVec3(a)) => {
            write_i32(&mut data[0..4], if a[0] { 1 } else { 0 });
            write_i32(&mut data[4..8], if a[1] { 1 } else { 0 });
            write_i32(&mut data[8..12], if a[2] { 1 } else { 0 });
        }
        (GlslType::BVec4, GlslValue::BVec4(a)) => {
            write_i32(&mut data[0..4], if a[0] { 1 } else { 0 });
            write_i32(&mut data[4..8], if a[1] { 1 } else { 0 });
            write_i32(&mut data[8..12], if a[2] { 1 } else { 0 });
            write_i32(&mut data[12..16], if a[3] { 1 } else { 0 });
        }
        (GlslType::Mat2, GlslValue::Mat2x2(m)) => {
            write_f32(&mut data[0..4], m[0][0]);
            write_f32(&mut data[4..8], m[0][1]);
            write_f32(&mut data[8..12], m[1][0]);
            write_f32(&mut data[12..16], m[1][1]);
        }
        (GlslType::Mat3, GlslValue::Mat3x3(m)) => {
            for col in 0..3 {
                let base = col * 12;
                write_f32(&mut data[base..base + 4], m[col][0]);
                write_f32(&mut data[base + 4..base + 8], m[col][1]);
                write_f32(&mut data[base + 8..base + 12], m[col][2]);
            }
        }
        (GlslType::Mat4, GlslValue::Mat4x4(m)) => {
            for col in 0..4 {
                let base = col * 16;
                write_f32(&mut data[base..base + 4], m[col][0]);
                write_f32(&mut data[base + 4..base + 8], m[col][1]);
                write_f32(&mut data[base + 8..base + 12], m[col][2]);
                write_f32(&mut data[base + 12..base + 16], m[col][3]);
            }
        }
        (GlslType::Array { element, len }, GlslValue::Array(items)) => {
            debug_assert_eq!(items.len(), *len as usize);
            let stride = array_stride(element, rules);
            let esz = element.size(rules);
            for (i, it) in items.iter().enumerate() {
                let base = i * stride;
                write_value(element, rules, &mut data[base..base + esz], it)?;
            }
        }
        (GlslType::Struct { members, .. }, GlslValue::Struct { fields, .. }) => {
            debug_assert_eq!(members.len(), fields.len());
            let mut cursor = 0usize;
            for (i, m) in members.iter().enumerate() {
                let a = m.ty.alignment(rules);
                cursor = round_up(cursor, a);
                let msz = m.ty.size(rules);
                write_value(&m.ty, rules, &mut data[cursor..cursor + msz], &fields[i].1)?;
                cursor += msz;
            }
        }
        _ => {
            return Err(GlslDataError::type_mismatch(
                format!("{ty:?}"),
                format!("value {value:?}"),
            ));
        }
    }
    Ok(())
}

fn f32_from_bytes(b: &[u8]) -> f32 {
    f32::from_le_bytes(b.try_into().expect("len 4"))
}

fn i32_from_bytes(b: &[u8]) -> i32 {
    i32::from_le_bytes(b.try_into().expect("len 4"))
}

fn u32_from_bytes(b: &[u8]) -> u32 {
    u32::from_le_bytes(b.try_into().expect("len 4"))
}

fn write_f32(buf: &mut [u8], x: f32) {
    buf.copy_from_slice(&x.to_le_bytes());
}

fn write_i32(buf: &mut [u8], x: i32) {
    buf.copy_from_slice(&x.to_le_bytes());
}

fn write_u32(buf: &mut [u8], x: u32) {
    buf.copy_from_slice(&x.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::StructMember;
    use alloc::boxed::Box;
    use alloc::vec;

    #[test]
    fn round_trip_struct_vec3_float() {
        let ty = GlslType::Struct {
            name: Some(String::from("S")),
            members: vec![
                StructMember {
                    name: Some(String::from("v")),
                    ty: GlslType::Vec3,
                },
                StructMember {
                    name: Some(String::from("s")),
                    ty: GlslType::Float,
                },
            ],
        };
        let v = GlslValue::Struct {
            name: Some(String::from("S")),
            fields: vec![
                (String::from("v"), GlslValue::Vec3([1.0, 2.0, 3.0])),
                (String::from("s"), GlslValue::F32(4.0)),
            ],
        };
        let d = GlslData::from_value(ty, &v).unwrap();
        assert_eq!(d.len(), 16);
        assert!((d.get_f32("s").unwrap() - 4.0).abs() < 1e-6);
        let got = d.get("v").unwrap();
        assert!(got.approx_eq_default(&GlslValue::Vec3([1.0, 2.0, 3.0])));
        assert!(d.to_value().unwrap().approx_eq_default(&v));
    }

    #[test]
    fn path_into_array() {
        let ty = GlslType::Array {
            element: Box::new(GlslType::Float),
            len: 3,
        };
        let mut d = GlslData::new(ty);
        d.set_f32("[1]", 9.0).unwrap();
        assert!((d.get_f32("[1]").unwrap() - 9.0).abs() < 1e-6);
    }
}
