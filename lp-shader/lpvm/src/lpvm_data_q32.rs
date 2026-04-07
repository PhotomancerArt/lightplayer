//! Byte-backed GLSL data with [`LayoutRules`] and path access.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::LpsValueF32;
use crate::data_error::DataError;
use lps_shared::layout::{array_stride, round_up, type_alignment, type_size};
use lps_shared::path_resolve::LpsTypePathExt;
use lps_shared::{LayoutRules, LpsType, StructMember};

/// Shader data as represented in LPVM memory
pub struct LpvmDataQ32 {
    ty: LpsType,
    rules: LayoutRules,
    data: Vec<u8>,
}

impl LpvmDataQ32 {
    pub fn new(ty: LpsType) -> Self {
        Self::with_rules(ty, LayoutRules::Std430).expect("std430 is implemented")
    }

    pub fn with_rules(ty: LpsType, rules: LayoutRules) -> Result<Self, DataError> {
        if !rules.is_implemented() {
            return Err(DataError::LayoutNotImplemented);
        }
        let n = type_size(&ty, rules);
        Ok(Self {
            ty,
            rules,
            data: alloc::vec![0u8; n],
        })
    }

    pub fn from_value(ty: LpsType, value: &LpsValueF32) -> Result<Self, DataError> {
        let mut s = Self::new(ty.clone());
        value_matches_type(&ty, value)?;
        write_value(&ty, s.rules, &mut s.data, value)?;
        Ok(s)
    }

    pub fn from_value_with_rules(
        ty: LpsType,
        rules: LayoutRules,
        value: &LpsValueF32,
    ) -> Result<Self, DataError> {
        if !rules.is_implemented() {
            return Err(DataError::LayoutNotImplemented);
        }
        let n = type_size(&ty, rules);
        let mut data = alloc::vec![0u8; n];
        value_matches_type(&ty, value).map_err(|e| e)?;
        write_value(&ty, rules, &mut data, value)?;
        Ok(Self { ty, rules, data })
    }

    pub fn ty(&self) -> &LpsType {
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

    pub fn to_value(&self) -> Result<LpsValueF32, DataError> {
        let need = type_size(&self.ty, self.rules);
        if self.data.len() < need {
            return Err(DataError::BufferTooShort {
                need,
                have: self.data.len(),
            });
        }
        read_value(&self.ty, self.rules, &self.data[..need])
    }

    pub fn offset_of(&self, path: &str) -> Result<usize, DataError> {
        Ok(self.ty.offset_for_path(path, self.rules, 0)?)
    }

    pub fn get(&self, path: &str) -> Result<LpsValueF32, DataError> {
        let off = self.ty.offset_for_path(path, self.rules, 0)?;
        let leaf = self.ty.type_at_path(path)?;
        let need = type_size(&leaf, self.rules);
        let end = off
            .checked_add(need)
            .ok_or_else(|| DataError::type_mismatch("path", "offset overflow"))?;
        if self.data.len() < end {
            return Err(DataError::BufferTooShort {
                need: end,
                have: self.data.len(),
            });
        }
        read_value(&leaf, self.rules, &self.data[off..end])
    }

    pub fn set(&mut self, path: &str, value: LpsValueF32) -> Result<(), DataError> {
        let off = self.ty.offset_for_path(path, self.rules, 0)?;
        let leaf = self.ty.type_at_path(path)?;
        value_matches_type(&leaf, &value)?;
        let need = type_size(&leaf, self.rules);
        let end = off
            .checked_add(need)
            .ok_or_else(|| DataError::type_mismatch("path", "offset overflow"))?;
        if self.data.len() < end {
            return Err(DataError::BufferTooShort {
                need: end,
                have: self.data.len(),
            });
        }
        write_value(&leaf, self.rules, &mut self.data[off..end], &value)
    }

    pub fn get_f32(&self, path: &str) -> Result<f32, DataError> {
        if !matches!(self.ty.type_at_path(path)?, LpsType::Float) {
            return Err(DataError::BadPathForScalar {
                path: String::from(path),
                expected_ty: String::from("float"),
            });
        }
        match self.get(path)? {
            LpsValueF32::F32(x) => Ok(x),
            _ => Err(DataError::type_mismatch("float", "internal decode error")),
        }
    }

    pub fn set_f32(&mut self, path: &str, val: f32) -> Result<(), DataError> {
        self.set(path, LpsValueF32::F32(val))
    }

    pub fn get_i32(&self, path: &str) -> Result<i32, DataError> {
        if !matches!(self.ty.type_at_path(path)?, LpsType::Int) {
            return Err(DataError::BadPathForScalar {
                path: String::from(path),
                expected_ty: String::from("int"),
            });
        }
        match self.get(path)? {
            LpsValueF32::I32(x) => Ok(x),
            _ => Err(DataError::type_mismatch("int", "internal decode error")),
        }
    }

    pub fn set_i32(&mut self, path: &str, val: i32) -> Result<(), DataError> {
        self.set(path, LpsValueF32::I32(val))
    }
}

fn member_key(m: &StructMember, idx: usize) -> String {
    m.name.clone().unwrap_or_else(|| format!("_{idx}"))
}

fn value_matches_type(ty: &LpsType, v: &LpsValueF32) -> Result<(), DataError> {
    match (ty, v) {
        (LpsType::Float, LpsValueF32::F32(_)) => Ok(()),
        (LpsType::Int, LpsValueF32::I32(_)) => Ok(()),
        (LpsType::UInt, LpsValueF32::U32(_)) => Ok(()),
        (LpsType::Bool, LpsValueF32::Bool(_)) => Ok(()),
        (LpsType::Vec2, LpsValueF32::Vec2(_)) => Ok(()),
        (LpsType::Vec3, LpsValueF32::Vec3(_)) => Ok(()),
        (LpsType::Vec4, LpsValueF32::Vec4(_)) => Ok(()),
        (LpsType::IVec2, LpsValueF32::IVec2(_)) => Ok(()),
        (LpsType::IVec3, LpsValueF32::IVec3(_)) => Ok(()),
        (LpsType::IVec4, LpsValueF32::IVec4(_)) => Ok(()),
        (LpsType::UVec2, LpsValueF32::UVec2(_)) => Ok(()),
        (LpsType::UVec3, LpsValueF32::UVec3(_)) => Ok(()),
        (LpsType::UVec4, LpsValueF32::UVec4(_)) => Ok(()),
        (LpsType::BVec2, LpsValueF32::BVec2(_)) => Ok(()),
        (LpsType::BVec3, LpsValueF32::BVec3(_)) => Ok(()),
        (LpsType::BVec4, LpsValueF32::BVec4(_)) => Ok(()),
        (LpsType::Mat2, LpsValueF32::Mat2x2(_)) => Ok(()),
        (LpsType::Mat3, LpsValueF32::Mat3x3(_)) => Ok(()),
        (LpsType::Mat4, LpsValueF32::Mat4x4(_)) => Ok(()),
        (LpsType::Array { element, len }, LpsValueF32::Array(items)) => {
            if items.len() != *len as usize {
                return Err(DataError::type_mismatch(
                    format!("array[{len}]"),
                    format!("got {} elements", items.len()),
                ));
            }
            for it in items.iter() {
                value_matches_type(element, it)?;
            }
            Ok(())
        }
        (LpsType::Struct { members, .. }, LpsValueF32::Struct { fields, .. }) => {
            if members.len() != fields.len() {
                return Err(DataError::type_mismatch(
                    format!("struct with {} fields", members.len()),
                    format!("got {} fields", fields.len()),
                ));
            }
            for (i, m) in members.iter().enumerate() {
                let key = member_key(m, i);
                let (fname, fv) = &fields[i];
                if fname != &key {
                    return Err(DataError::type_mismatch(
                        format!("field `{key}`"),
                        format!("got field name `{fname}`"),
                    ));
                }
                value_matches_type(&m.ty, fv)?;
            }
            Ok(())
        }
        _ => Err(DataError::type_mismatch(
            format!("{ty:?}"),
            format!("value {v:?}"),
        )),
    }
}

fn read_value(ty: &LpsType, rules: LayoutRules, data: &[u8]) -> Result<LpsValueF32, DataError> {
    let need = type_size(ty, rules);
    if data.len() < need {
        return Err(DataError::BufferTooShort {
            need,
            have: data.len(),
        });
    }
    let data = &data[..need];
    Ok(match ty {
        LpsType::Void => {
            return Err(DataError::type_mismatch("void", "cannot load void value"));
        }
        LpsType::Float => LpsValueF32::F32(f32_from_bytes(data)),
        LpsType::Int => LpsValueF32::I32(i32_from_bytes(data)),
        LpsType::UInt => LpsValueF32::U32(u32_from_bytes(data)),
        LpsType::Bool => LpsValueF32::Bool(i32_from_bytes(data) != 0),
        LpsType::Vec2 => {
            LpsValueF32::Vec2([f32_from_bytes(&data[0..4]), f32_from_bytes(&data[4..8])])
        }
        LpsType::Vec3 => LpsValueF32::Vec3([
            f32_from_bytes(&data[0..4]),
            f32_from_bytes(&data[4..8]),
            f32_from_bytes(&data[8..12]),
        ]),
        LpsType::Vec4 => LpsValueF32::Vec4([
            f32_from_bytes(&data[0..4]),
            f32_from_bytes(&data[4..8]),
            f32_from_bytes(&data[8..12]),
            f32_from_bytes(&data[12..16]),
        ]),
        LpsType::IVec2 => {
            LpsValueF32::IVec2([i32_from_bytes(&data[0..4]), i32_from_bytes(&data[4..8])])
        }
        LpsType::IVec3 => LpsValueF32::IVec3([
            i32_from_bytes(&data[0..4]),
            i32_from_bytes(&data[4..8]),
            i32_from_bytes(&data[8..12]),
        ]),
        LpsType::IVec4 => LpsValueF32::IVec4([
            i32_from_bytes(&data[0..4]),
            i32_from_bytes(&data[4..8]),
            i32_from_bytes(&data[8..12]),
            i32_from_bytes(&data[12..16]),
        ]),
        LpsType::UVec2 => {
            LpsValueF32::UVec2([u32_from_bytes(&data[0..4]), u32_from_bytes(&data[4..8])])
        }
        LpsType::UVec3 => LpsValueF32::UVec3([
            u32_from_bytes(&data[0..4]),
            u32_from_bytes(&data[4..8]),
            u32_from_bytes(&data[8..12]),
        ]),
        LpsType::UVec4 => LpsValueF32::UVec4([
            u32_from_bytes(&data[0..4]),
            u32_from_bytes(&data[4..8]),
            u32_from_bytes(&data[8..12]),
            u32_from_bytes(&data[12..16]),
        ]),
        LpsType::BVec2 => LpsValueF32::BVec2([
            i32_from_bytes(&data[0..4]) != 0,
            i32_from_bytes(&data[4..8]) != 0,
        ]),
        LpsType::BVec3 => LpsValueF32::BVec3([
            i32_from_bytes(&data[0..4]) != 0,
            i32_from_bytes(&data[4..8]) != 0,
            i32_from_bytes(&data[8..12]) != 0,
        ]),
        LpsType::BVec4 => LpsValueF32::BVec4([
            i32_from_bytes(&data[0..4]) != 0,
            i32_from_bytes(&data[4..8]) != 0,
            i32_from_bytes(&data[8..12]) != 0,
            i32_from_bytes(&data[12..16]) != 0,
        ]),
        LpsType::Mat2 => {
            let c0 = [f32_from_bytes(&data[0..4]), f32_from_bytes(&data[4..8])];
            let c1 = [f32_from_bytes(&data[8..12]), f32_from_bytes(&data[12..16])];
            LpsValueF32::Mat2x2([c0, c1])
        }
        LpsType::Mat3 => {
            let mut m = [[0f32; 3]; 3];
            for col in 0..3 {
                let base = col * 12;
                m[col] = [
                    f32_from_bytes(&data[base..base + 4]),
                    f32_from_bytes(&data[base + 4..base + 8]),
                    f32_from_bytes(&data[base + 8..base + 12]),
                ];
            }
            LpsValueF32::Mat3x3(m)
        }
        LpsType::Mat4 => {
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
            LpsValueF32::Mat4x4(m)
        }
        LpsType::Array { element, len } => {
            let stride = array_stride(element, rules);
            let esz = type_size(element, rules);
            let mut elems = Vec::with_capacity(*len as usize);
            for i in 0..(*len as usize) {
                let base = i * stride;
                elems.push(read_value(element, rules, &data[base..base + esz])?);
            }
            LpsValueF32::Array(elems.into_boxed_slice())
        }
        LpsType::Struct { name, members } => {
            let mut cursor = 0usize;
            let mut fields = Vec::with_capacity(members.len());
            for (i, m) in members.iter().enumerate() {
                let a = type_alignment(&m.ty, rules);
                cursor = round_up(cursor, a);
                let msz = type_size(&m.ty, rules);
                let v = read_value(&m.ty, rules, &data[cursor..cursor + msz])?;
                fields.push((member_key(m, i), v));
                cursor += msz;
            }
            LpsValueF32::Struct {
                name: name.clone(),
                fields,
            }
        }
    })
}

fn write_value(
    ty: &LpsType,
    rules: LayoutRules,
    data: &mut [u8],
    value: &LpsValueF32,
) -> Result<(), DataError> {
    let need = type_size(ty, rules);
    if data.len() < need {
        return Err(DataError::BufferTooShort {
            need,
            have: data.len(),
        });
    }
    let data = &mut data[..need];
    match (ty, value) {
        (LpsType::Float, LpsValueF32::F32(x)) => write_f32(data, *x),
        (LpsType::Int, LpsValueF32::I32(x)) => write_i32(data, *x),
        (LpsType::UInt, LpsValueF32::U32(x)) => write_u32(data, *x),
        (LpsType::Bool, LpsValueF32::Bool(b)) => write_i32(data, if *b { 1 } else { 0 }),
        (LpsType::Vec2, LpsValueF32::Vec2(a)) => {
            write_f32(&mut data[0..4], a[0]);
            write_f32(&mut data[4..8], a[1]);
        }
        (LpsType::Vec3, LpsValueF32::Vec3(a)) => {
            write_f32(&mut data[0..4], a[0]);
            write_f32(&mut data[4..8], a[1]);
            write_f32(&mut data[8..12], a[2]);
        }
        (LpsType::Vec4, LpsValueF32::Vec4(a)) => {
            write_f32(&mut data[0..4], a[0]);
            write_f32(&mut data[4..8], a[1]);
            write_f32(&mut data[8..12], a[2]);
            write_f32(&mut data[12..16], a[3]);
        }
        (LpsType::IVec2, LpsValueF32::IVec2(a)) => {
            write_i32(&mut data[0..4], a[0]);
            write_i32(&mut data[4..8], a[1]);
        }
        (LpsType::IVec3, LpsValueF32::IVec3(a)) => {
            write_i32(&mut data[0..4], a[0]);
            write_i32(&mut data[4..8], a[1]);
            write_i32(&mut data[8..12], a[2]);
        }
        (LpsType::IVec4, LpsValueF32::IVec4(a)) => {
            write_i32(&mut data[0..4], a[0]);
            write_i32(&mut data[4..8], a[1]);
            write_i32(&mut data[8..12], a[2]);
            write_i32(&mut data[12..16], a[3]);
        }
        (LpsType::UVec2, LpsValueF32::UVec2(a)) => {
            write_u32(&mut data[0..4], a[0]);
            write_u32(&mut data[4..8], a[1]);
        }
        (LpsType::UVec3, LpsValueF32::UVec3(a)) => {
            write_u32(&mut data[0..4], a[0]);
            write_u32(&mut data[4..8], a[1]);
            write_u32(&mut data[8..12], a[2]);
        }
        (LpsType::UVec4, LpsValueF32::UVec4(a)) => {
            write_u32(&mut data[0..4], a[0]);
            write_u32(&mut data[4..8], a[1]);
            write_u32(&mut data[8..12], a[2]);
            write_u32(&mut data[12..16], a[3]);
        }
        (LpsType::BVec2, LpsValueF32::BVec2(a)) => {
            write_i32(&mut data[0..4], if a[0] { 1 } else { 0 });
            write_i32(&mut data[4..8], if a[1] { 1 } else { 0 });
        }
        (LpsType::BVec3, LpsValueF32::BVec3(a)) => {
            write_i32(&mut data[0..4], if a[0] { 1 } else { 0 });
            write_i32(&mut data[4..8], if a[1] { 1 } else { 0 });
            write_i32(&mut data[8..12], if a[2] { 1 } else { 0 });
        }
        (LpsType::BVec4, LpsValueF32::BVec4(a)) => {
            write_i32(&mut data[0..4], if a[0] { 1 } else { 0 });
            write_i32(&mut data[4..8], if a[1] { 1 } else { 0 });
            write_i32(&mut data[8..12], if a[2] { 1 } else { 0 });
            write_i32(&mut data[12..16], if a[3] { 1 } else { 0 });
        }
        (LpsType::Mat2, LpsValueF32::Mat2x2(m)) => {
            write_f32(&mut data[0..4], m[0][0]);
            write_f32(&mut data[4..8], m[0][1]);
            write_f32(&mut data[8..12], m[1][0]);
            write_f32(&mut data[12..16], m[1][1]);
        }
        (LpsType::Mat3, LpsValueF32::Mat3x3(m)) => {
            for col in 0..3 {
                let base = col * 12;
                write_f32(&mut data[base..base + 4], m[col][0]);
                write_f32(&mut data[base + 4..base + 8], m[col][1]);
                write_f32(&mut data[base + 8..base + 12], m[col][2]);
            }
        }
        (LpsType::Mat4, LpsValueF32::Mat4x4(m)) => {
            for col in 0..4 {
                let base = col * 16;
                write_f32(&mut data[base..base + 4], m[col][0]);
                write_f32(&mut data[base + 4..base + 8], m[col][1]);
                write_f32(&mut data[base + 8..base + 12], m[col][2]);
                write_f32(&mut data[base + 12..base + 16], m[col][3]);
            }
        }
        (LpsType::Array { element, len }, LpsValueF32::Array(items)) => {
            debug_assert_eq!(items.len(), *len as usize);
            let stride = array_stride(element, rules);
            let esz = type_size(element, rules);
            for (i, it) in items.iter().enumerate() {
                let base = i * stride;
                write_value(element, rules, &mut data[base..base + esz], it)?;
            }
        }
        (LpsType::Struct { members, .. }, LpsValueF32::Struct { fields, .. }) => {
            debug_assert_eq!(members.len(), fields.len());
            let mut cursor = 0usize;
            for (i, m) in members.iter().enumerate() {
                let a = type_alignment(&m.ty, rules);
                cursor = round_up(cursor, a);
                let msz = type_size(&m.ty, rules);
                write_value(&m.ty, rules, &mut data[cursor..cursor + msz], &fields[i].1)?;
                cursor += msz;
            }
        }
        _ => {
            return Err(DataError::type_mismatch(
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
    use alloc::boxed::Box;
    use alloc::vec;
    use lps_shared::StructMember;

    #[test]
    fn round_trip_struct_vec3_float() {
        let ty = LpsType::Struct {
            name: Some(String::from("S")),
            members: vec![
                StructMember {
                    name: Some(String::from("v")),
                    ty: LpsType::Vec3,
                },
                StructMember {
                    name: Some(String::from("s")),
                    ty: LpsType::Float,
                },
            ],
        };
        let v = LpsValueF32::Struct {
            name: Some(String::from("S")),
            fields: vec![
                (String::from("v"), LpsValueF32::Vec3([1.0, 2.0, 3.0])),
                (String::from("s"), LpsValueF32::F32(4.0)),
            ],
        };
        let d = LpvmDataQ32::from_value(ty, &v).unwrap();
        assert_eq!(d.len(), 16);
        assert!((d.get_f32("s").unwrap() - 4.0).abs() < 1e-6);
        let got = d.get("v").unwrap();
        assert!(got.approx_eq_default(&LpsValueF32::Vec3([1.0, 2.0, 3.0])));
        assert!(d.to_value().unwrap().approx_eq_default(&v));
    }

    #[test]
    fn path_into_array() {
        let ty = LpsType::Array {
            element: Box::new(LpsType::Float),
            len: 3,
        };
        let mut d = LpvmDataQ32::new(ty);
        d.set_f32("[1]", 9.0).unwrap();
        assert!((d.get_f32("[1]").unwrap() - 9.0).abs() < 1e-6);
    }
}
