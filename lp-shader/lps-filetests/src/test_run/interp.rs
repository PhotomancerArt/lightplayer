//! `interp.f32` filetest backend: GLSL → `lps-frontend` (naga) → LPIR →
//! `lpir::interpret` in native f32, reusing the M2 conformance-oracle path.
//!
//! `lps-frontend` reserves the `lpfn_` prefix (calls lower to `@lpfn::`
//! imports the interpreter cannot evaluate), so when the test source
//! references `lpfn_*` the canonical GLSL builtin sources are prepended and
//! the whole unit gets the oracle's `lpfn_` → `lpo_` rename — the canonical
//! bodies then compile as ordinary local functions with f32 semantics
//! (see `crate::conformance::oracle`). Transcendental imports (`@glsl::sin`
//! etc.) are evaluated host-side by `StdMathHandler` (libm).
//!
//! Not supported here (fails per directive, surfaced by the P2 sweep):
//! uniforms (`set_uniform`), texture fixtures (no instance memory), and
//! trap-code expectations (the interpreter reports errors, not RV32 traps).

use std::sync::Arc;

use lp_collection::VecMap;
use lpir::{CompilerConfig, FloatMode, LpirModule, Value, interpret_entry};
use lps_frontend::std_math_handler::StdMathHandler;
use lps_shared::layout::type_size;
use lps_shared::{LayoutRules, LpsModuleSig, LpsType, LpsValueF32, TextureBindingSpec};
use lpvm::{encode_uniform_write, glsl_component_count};

use crate::conformance::oracle::{canonical_unit_source, rename_lpfn_prefix};

/// Compiled interp module: LPIR + signatures for one test file.
pub struct InterpShader {
    ir: Arc<LpirModule>,
    sig: Arc<LpsModuleSig>,
}

/// Per-`// run:` "instance": shares the compiled module and owns a VMContext
/// image (uniforms + globals region, zero-initialized) that `set_uniform`
/// writes into and every call executes against.
pub struct InterpInstance {
    ir: Arc<LpirModule>,
    sig: Arc<LpsModuleSig>,
    vmctx_image: Vec<u8>,
}

/// True if `src` references an `lpfn_*` identifier (at identifier boundary).
fn references_lpfn(src: &str) -> bool {
    let bytes = src.as_bytes();
    let mut from = 0;
    while let Some(pos) = src[from..].find("lpfn_") {
        let i = from + pos;
        let at_boundary = i == 0 || !(bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_');
        if at_boundary {
            return true;
        }
        from = i + 1;
    }
    false
}

/// Lower a filetest GLSL source for the interp backend.
///
/// Returns the LPIR module and its module signature (used for arg/return
/// marshaling and directive function lookup).
pub fn lower_for_interp(
    source: &str,
    texture_specs: &VecMap<String, TextureBindingSpec>,
    compiler_config: &CompilerConfig,
) -> anyhow::Result<(LpirModule, LpsModuleSig)> {
    // Prepend the canonical builtin sources only when needed: the rename
    // makes `lpfn_*` calls bind to the compiled canonical bodies instead of
    // uninterpretable `@lpfn::` imports.
    let unit = if references_lpfn(source) {
        canonical_unit_source(source)
    } else {
        source.to_string()
    };
    let naga =
        lps_frontend::compile(&unit).map_err(|e| anyhow::anyhow!("interp GLSL compile: {e:?}"))?;
    let options = lps_frontend::LowerOptions {
        texture_specs: texture_specs.clone(),
        texel_fetch_bounds: compiler_config.texture.texel_fetch_bounds,
    };
    let (ir, meta) = lps_frontend::lower_with_options(&naga, &options)
        .map_err(|e| anyhow::anyhow!("interp lower: {e}"))?;
    Ok((ir, meta))
}

impl InterpShader {
    /// Wrap a lowered module.
    pub fn new(ir: LpirModule, sig: LpsModuleSig) -> Self {
        Self {
            ir: Arc::new(ir),
            sig: Arc::new(sig),
        }
    }

    /// Module signatures (function lookup for directives).
    pub fn signatures(&self) -> &LpsModuleSig {
        &self.sig
    }

    /// Create a per-directive instance (cheap; shares the module). The
    /// instance owns a fresh zero-initialized VMContext image.
    pub fn instantiate(&self) -> InterpInstance {
        InterpInstance {
            ir: Arc::clone(&self.ir),
            sig: Arc::clone(&self.sig),
            vmctx_image: vec![0u8; self.sig.vmctx_buffer_size()],
        }
    }

    /// Borrow the LPIR module (debug printing).
    pub fn lpir_module(&self) -> &LpirModule {
        &self.ir
    }
}

impl InterpInstance {
    /// Write a uniform value into this instance's VMContext image; later
    /// calls observe it (mirrors the compiled runtimes' `set_uniform`).
    pub fn set_uniform(&mut self, path: &str, value: &LpsValueF32) -> Result<(), String> {
        let (off, bytes) = encode_uniform_write(&self.sig, path, value, FloatMode::F32)
            .map_err(|e| format!("set_uniform `{path}`: {e}"))?;
        let end = off
            .checked_add(bytes.len())
            .filter(|&e| e <= self.vmctx_image.len())
            .ok_or_else(|| {
                format!(
                    "set_uniform `{path}`: write [{off}, {off}+{}) exceeds vmctx image ({})",
                    bytes.len(),
                    self.vmctx_image.len()
                )
            })?;
        self.vmctx_image[off..end].copy_from_slice(&bytes);
        Ok(())
    }

    /// Execute `name` with f32 argument marshaling via `lpir::interpret_entry`.
    pub fn call(&mut self, name: &str, args: &[LpsValueF32]) -> Result<LpsValueF32, String> {
        // Directives may call an `lpfn_*` wrapper directly; the compiled unit
        // renamed those to `lpo_*`.
        let lookup: String = rename_lpfn_prefix(name);
        let gfn = self
            .sig
            .functions
            .iter()
            .find(|f| f.name == lookup || f.name == name)
            .ok_or_else(|| format!("interp: function '{name}' not found in module signature"))?;

        if gfn.parameters.len() != args.len() {
            return Err(format!(
                "interp: {name} expects {} args, got {}",
                gfn.parameters.len(),
                args.len()
            ));
        }

        let mut flat: Vec<Value> = Vec::new();
        for (p, a) in gfn.parameters.iter().zip(args) {
            flatten_arg(&p.ty, a, &mut flat)
                .map_err(|e| format!("interp: {name} arg '{}': {e}", p.name))?;
        }

        // Aggregate (struct/array) returns use the sret convention: the
        // callee writes std430 bytes through a hidden destination pointer and
        // returns no scalars. Size the destination from the return type; the
        // entry point validates the choice against the function's actual ABI.
        let sret_size = if type_returns_via_sret(&gfn.return_type) {
            sret_dense_size(&gfn.return_type)?
        } else {
            0
        };

        // The VMContext image (zero-initialized uniforms + globals, plus any
        // `set_uniform` writes) backs vmctx-relative loads/stores. Zero is
        // the correct default for the transcendental corpus (uniforms
        // laundered through `u_runtime_zero`) and for globals written before
        // being read.
        let mut handler = StdMathHandler::default();
        let out = interpret_entry(
            &self.ir,
            &gfn.name,
            &flat,
            &mut handler,
            &self.vmctx_image,
            sret_size,
            lpir::DEFAULT_MAX_DEPTH,
        )
        .map_err(|e| format!("interp: {name}: {e}"))?;

        let scalars: Vec<Value> = if sret_size > 0 {
            if !out.values.is_empty() {
                return Err(format!(
                    "interp: {name}: sret function also returned {} scalar(s)",
                    out.values.len()
                ));
            }
            sret_bytes_to_values(&gfn.return_type, &out.sret_bytes)
                .map_err(|e| format!("interp: {name} sret: {e}"))?
        } else {
            out.values
        };

        let mut it = scalars.iter().copied();
        let v = decode_return(&gfn.return_type, &mut it)
            .map_err(|e| format!("interp: {name} return: {e}"))?;
        if it.next().is_some() {
            return Err(format!(
                "interp: {name} returned more scalars than {:?} holds",
                gfn.return_type
            ));
        }
        Ok(v)
    }
}

/// Flatten one typed argument into interpreter scalars, coercing numeric
/// literal kinds where the directive parser is looser than the GLSL type
/// (e.g. `1` for a `float` parameter).
fn flatten_arg(ty: &LpsType, v: &LpsValueF32, out: &mut Vec<Value>) -> Result<(), String> {
    match (ty, v) {
        (LpsType::Float, LpsValueF32::F32(x)) => out.push(Value::F32(*x)),
        (LpsType::Float, LpsValueF32::I32(x)) => out.push(Value::F32(*x as f32)),
        (LpsType::Float, LpsValueF32::U32(x)) => out.push(Value::F32(*x as f32)),
        (LpsType::Int, LpsValueF32::I32(x)) => out.push(Value::I32(*x)),
        (LpsType::Int, LpsValueF32::U32(x)) => out.push(Value::I32(*x as i32)),
        (LpsType::UInt, LpsValueF32::U32(x)) => out.push(Value::I32(*x as i32)),
        (LpsType::UInt, LpsValueF32::I32(x)) => out.push(Value::I32(*x)),
        (LpsType::Bool, LpsValueF32::Bool(b)) => out.push(Value::I32(i32::from(*b))),
        (LpsType::Bool, LpsValueF32::I32(x)) => out.push(Value::I32(i32::from(*x != 0))),
        (LpsType::Vec2, LpsValueF32::Vec2(a)) => out.extend(a.iter().map(|&x| Value::F32(x))),
        (LpsType::Vec3, LpsValueF32::Vec3(a)) => out.extend(a.iter().map(|&x| Value::F32(x))),
        (LpsType::Vec4, LpsValueF32::Vec4(a)) => out.extend(a.iter().map(|&x| Value::F32(x))),
        (LpsType::IVec2, LpsValueF32::IVec2(a)) => out.extend(a.iter().map(|&x| Value::I32(x))),
        (LpsType::IVec3, LpsValueF32::IVec3(a)) => out.extend(a.iter().map(|&x| Value::I32(x))),
        (LpsType::IVec4, LpsValueF32::IVec4(a)) => out.extend(a.iter().map(|&x| Value::I32(x))),
        (LpsType::UVec2, LpsValueF32::UVec2(a)) => {
            out.extend(a.iter().map(|&x| Value::I32(x as i32)));
        }
        (LpsType::UVec3, LpsValueF32::UVec3(a)) => {
            out.extend(a.iter().map(|&x| Value::I32(x as i32)));
        }
        (LpsType::UVec4, LpsValueF32::UVec4(a)) => {
            out.extend(a.iter().map(|&x| Value::I32(x as i32)));
        }
        (LpsType::BVec2, LpsValueF32::BVec2(a)) => {
            out.extend(a.iter().map(|&b| Value::I32(i32::from(b))));
        }
        (LpsType::BVec3, LpsValueF32::BVec3(a)) => {
            out.extend(a.iter().map(|&b| Value::I32(i32::from(b))));
        }
        (LpsType::BVec4, LpsValueF32::BVec4(a)) => {
            out.extend(a.iter().map(|&b| Value::I32(i32::from(b))));
        }
        (LpsType::Mat2, LpsValueF32::Mat2x2(m)) => {
            out.extend(m.iter().flatten().map(|&x| Value::F32(x)));
        }
        (LpsType::Mat3, LpsValueF32::Mat3x3(m)) => {
            out.extend(m.iter().flatten().map(|&x| Value::F32(x)));
        }
        (LpsType::Mat4, LpsValueF32::Mat4x4(m)) => {
            out.extend(m.iter().flatten().map(|&x| Value::F32(x)));
        }
        (LpsType::Array { element, len }, LpsValueF32::Array(items)) => {
            if items.len() != *len as usize {
                return Err(format!(
                    "array length mismatch: type [{len}], value {}",
                    items.len()
                ));
            }
            for it in items.iter() {
                flatten_arg(element, it, out)?;
            }
        }
        (LpsType::Struct { members, .. }, LpsValueF32::Struct { fields, .. }) => {
            if members.len() != fields.len() {
                return Err("struct field count mismatch".to_string());
            }
            for (m, (_, fv)) in members.iter().zip(fields.iter()) {
                flatten_arg(&m.ty, fv, out)?;
            }
        }
        (LpsType::Texture2D, _) => {
            return Err("texture arguments are not supported on interp.f32".to_string());
        }
        (ty, v) => {
            return Err(format!(
                "cannot marshal {v:?} as {ty:?} for the interpreter"
            ));
        }
    }
    Ok(())
}

/// Aggregate returns (struct/array) go through the sret convention.
fn type_returns_via_sret(ty: &LpsType) -> bool {
    matches!(ty, LpsType::Array { .. } | LpsType::Struct { .. })
}

/// std430 byte size of an sret aggregate, requiring the dense layout the
/// scalar-walk decode assumes (every scalar at a 4-byte stride, no padding).
/// Mirrors the wasm host marshalling's restriction (`aggregate_abi.rs`).
fn sret_dense_size(ty: &LpsType) -> Result<usize, String> {
    let size = type_size(ty, LayoutRules::Std430);
    let dense = glsl_component_count(ty) * 4;
    if size != dense {
        return Err(format!(
            "sret return `{ty:?}` is not densely packed in std430 \
             (size {size}, scalars need {dense}); decode unsupported"
        ));
    }
    Ok(size)
}

/// Reinterpret a dense std430 sret buffer as typed interpreter scalars, in
/// the same order [`decode_return`] consumes them.
fn sret_bytes_to_values(ty: &LpsType, bytes: &[u8]) -> Result<Vec<Value>, String> {
    fn push_scalars(ty: &LpsType, words: &mut impl Iterator<Item = u32>, out: &mut Vec<Value>) -> Result<(), String> {
        let mut take = |n: usize, float: bool, out: &mut Vec<Value>| -> Result<(), String> {
            for _ in 0..n {
                let w = words.next().ok_or("sret buffer exhausted")?;
                out.push(if float {
                    Value::F32(f32::from_bits(w))
                } else {
                    Value::I32(w as i32)
                });
            }
            Ok(())
        };
        match ty {
            LpsType::Void => Ok(()),
            LpsType::Float => take(1, true, out),
            LpsType::Int | LpsType::UInt | LpsType::Bool => take(1, false, out),
            LpsType::Vec2 => take(2, true, out),
            LpsType::Vec3 => take(3, true, out),
            LpsType::Vec4 => take(4, true, out),
            LpsType::IVec2 | LpsType::UVec2 | LpsType::BVec2 => take(2, false, out),
            LpsType::IVec3 | LpsType::UVec3 | LpsType::BVec3 => take(3, false, out),
            LpsType::IVec4 | LpsType::UVec4 | LpsType::BVec4 => take(4, false, out),
            LpsType::Mat2 => take(4, true, out),
            LpsType::Mat3 => take(9, true, out),
            LpsType::Mat4 => take(16, true, out),
            LpsType::Array { element, len } => {
                for _ in 0..*len {
                    push_scalars(element, words, out)?;
                }
                Ok(())
            }
            LpsType::Struct { members, .. } => {
                for m in members {
                    push_scalars(&m.ty, words, out)?;
                }
                Ok(())
            }
            LpsType::Texture2D => Err("texture in sret return".to_string()),
        }
    }

    if bytes.len() % 4 != 0 {
        return Err(format!("sret buffer length {} not word-aligned", bytes.len()));
    }
    let mut words = bytes
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes(c.try_into().unwrap()));
    let mut out = Vec::new();
    push_scalars(ty, &mut words, &mut out)?;
    Ok(out)
}

/// Decode interpreter output scalars into a typed [`LpsValueF32`].
fn decode_return(
    ty: &LpsType,
    it: &mut impl Iterator<Item = Value>,
) -> Result<LpsValueF32, String> {
    fn next_f32(it: &mut impl Iterator<Item = Value>) -> Result<f32, String> {
        match it.next() {
            Some(Value::F32(x)) => Ok(x),
            Some(Value::I32(x)) => Err(format!("expected f32 scalar, got i32 {x}")),
            None => Err("missing result scalar".to_string()),
        }
    }
    fn next_i32(it: &mut impl Iterator<Item = Value>) -> Result<i32, String> {
        match it.next() {
            Some(Value::I32(x)) => Ok(x),
            Some(Value::F32(x)) => Err(format!("expected i32 scalar, got f32 {x}")),
            None => Err("missing result scalar".to_string()),
        }
    }
    fn f32s<const N: usize>(it: &mut impl Iterator<Item = Value>) -> Result<[f32; N], String> {
        let mut a = [0.0f32; N];
        for slot in &mut a {
            *slot = next_f32(it)?;
        }
        Ok(a)
    }
    fn i32s<const N: usize>(it: &mut impl Iterator<Item = Value>) -> Result<[i32; N], String> {
        let mut a = [0i32; N];
        for slot in &mut a {
            *slot = next_i32(it)?;
        }
        Ok(a)
    }

    Ok(match ty {
        LpsType::Void => LpsValueF32::F32(0.0),
        LpsType::Float => LpsValueF32::F32(next_f32(it)?),
        LpsType::Int => LpsValueF32::I32(next_i32(it)?),
        LpsType::UInt => LpsValueF32::U32(next_i32(it)? as u32),
        LpsType::Bool => LpsValueF32::Bool(next_i32(it)? != 0),
        LpsType::Vec2 => LpsValueF32::Vec2(f32s::<2>(it)?),
        LpsType::Vec3 => LpsValueF32::Vec3(f32s::<3>(it)?),
        LpsType::Vec4 => LpsValueF32::Vec4(f32s::<4>(it)?),
        LpsType::IVec2 => LpsValueF32::IVec2(i32s::<2>(it)?),
        LpsType::IVec3 => LpsValueF32::IVec3(i32s::<3>(it)?),
        LpsType::IVec4 => LpsValueF32::IVec4(i32s::<4>(it)?),
        LpsType::UVec2 => {
            let a = i32s::<2>(it)?;
            LpsValueF32::UVec2([a[0] as u32, a[1] as u32])
        }
        LpsType::UVec3 => {
            let a = i32s::<3>(it)?;
            LpsValueF32::UVec3([a[0] as u32, a[1] as u32, a[2] as u32])
        }
        LpsType::UVec4 => {
            let a = i32s::<4>(it)?;
            LpsValueF32::UVec4([a[0] as u32, a[1] as u32, a[2] as u32, a[3] as u32])
        }
        LpsType::BVec2 => {
            let a = i32s::<2>(it)?;
            LpsValueF32::BVec2([a[0] != 0, a[1] != 0])
        }
        LpsType::BVec3 => {
            let a = i32s::<3>(it)?;
            LpsValueF32::BVec3([a[0] != 0, a[1] != 0, a[2] != 0])
        }
        LpsType::BVec4 => {
            let a = i32s::<4>(it)?;
            LpsValueF32::BVec4([a[0] != 0, a[1] != 0, a[2] != 0, a[3] != 0])
        }
        LpsType::Mat2 => {
            let a = f32s::<4>(it)?;
            LpsValueF32::Mat2x2([[a[0], a[1]], [a[2], a[3]]])
        }
        LpsType::Mat3 => {
            let a = f32s::<9>(it)?;
            LpsValueF32::Mat3x3([[a[0], a[1], a[2]], [a[3], a[4], a[5]], [a[6], a[7], a[8]]])
        }
        LpsType::Mat4 => {
            let a = f32s::<16>(it)?;
            LpsValueF32::Mat4x4([
                [a[0], a[1], a[2], a[3]],
                [a[4], a[5], a[6], a[7]],
                [a[8], a[9], a[10], a[11]],
                [a[12], a[13], a[14], a[15]],
            ])
        }
        LpsType::Array { element, len } => {
            let mut items = Vec::with_capacity(*len as usize);
            for _ in 0..*len {
                items.push(decode_return(element, it)?);
            }
            LpsValueF32::Array(items.into_boxed_slice())
        }
        LpsType::Struct { name, members } => {
            let mut fields = Vec::with_capacity(members.len());
            for m in members {
                let v = decode_return(&m.ty, it)?;
                fields.push((m.name.clone().unwrap_or_default(), v));
            }
            LpsValueF32::Struct {
                name: name.clone(),
                fields,
            }
        }
        LpsType::Texture2D => {
            return Err("texture return values are not supported on interp.f32".to_string());
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn references_lpfn_boundary() {
        assert!(references_lpfn("float f() { return lpfn_saturate(2.0); }"));
        assert!(!references_lpfn("float my_lpfn_thing() { return 1.0; }"));
        assert!(!references_lpfn("float f() { return 1.0; }"));
    }

    #[test]
    fn interp_executes_simple_function() {
        let (ir, sig) = lower_for_interp(
            "float add(float a, float b) { return a + b; }",
            &VecMap::new(),
            &CompilerConfig::default(),
        )
        .expect("lower");
        let shader = InterpShader::new(ir, sig);
        let mut inst = shader.instantiate();
        let out = inst
            .call("add", &[LpsValueF32::F32(1.5), LpsValueF32::F32(2.25)])
            .expect("call");
        assert!(matches!(out, LpsValueF32::F32(x) if x == 3.75));
    }

    #[test]
    fn interp_executes_lpfn_via_canonicals() {
        let (ir, sig) = lower_for_interp(
            "float f(float x) { return lpfn_saturate(x); }",
            &VecMap::new(),
            &CompilerConfig::default(),
        )
        .expect("lower");
        let shader = InterpShader::new(ir, sig);
        let mut inst = shader.instantiate();
        let out = inst.call("f", &[LpsValueF32::F32(1.5)]).expect("call");
        assert!(matches!(out, LpsValueF32::F32(x) if x == 1.0));
    }

    #[test]
    fn interp_vec_return() {
        let (ir, sig) = lower_for_interp(
            "vec3 mk(float a) { return vec3(a, a + 1.0, a + 2.0); }",
            &VecMap::new(),
            &CompilerConfig::default(),
        )
        .expect("lower");
        let shader = InterpShader::new(ir, sig);
        let mut inst = shader.instantiate();
        let out = inst.call("mk", &[LpsValueF32::F32(1.0)]).expect("call");
        assert!(matches!(out, LpsValueF32::Vec3(a) if a == [1.0, 2.0, 3.0]));
    }
}
