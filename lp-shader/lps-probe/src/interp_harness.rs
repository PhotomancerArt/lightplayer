//! GLSL → `lps-frontend` (naga) → LPIR → `lpir::interpret_entry` harness.
//!
//! Extracted and adapted from `lps-filetests/src/test_run/interp.rs`
//! (`InterpShader`/`InterpInstance`). Differences from the filetests copy:
//!
//! - Compilation errors are returned as diagnostic strings (no `anyhow`),
//!   and the bare source is compiled first so diagnostics stay user-relative
//!   even when the canonical builtin prelude is prepended for execution.
//! - No texture-binding specs (textures are out of scope for probing).
//! - [`InterpInstance::call`] persists VMContext writes: module globals
//!   mutated by one call are observed by the next, approximating
//!   frame-sequential behavior for shaders with global state.
//!
//! Uniforms and module globals execute against a per-instance VMContext
//! image (zero-initialized; `set_uniform` writes into it), and aggregate
//! returns are decoded from a real sret buffer. Transcendental imports
//! (`@glsl::sin` etc.) are evaluated host-side by `StdMathHandler` (libm).

use alloc::format;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt;

use lpir::{FloatMode, InterpLimits, LpirModule, Value, interpret_entry};
use lps_frontend::std_math_handler::StdMathHandler;
use lps_shared::layout::type_size;
use lps_shared::{LayoutRules, LpsModuleSig, LpsType, LpsValueF32};
use lpvm::{encode_uniform_write, glsl_component_count};

use crate::canonical_unit::{execution_unit_source, references_lpfn};

/// Synthesized by `lps-frontend` when globals/uniforms carry declaration
/// initializers; writes them into the VMContext.
const SHADER_INIT_FN: &str = "__shader_init";

/// Compiled shader: LPIR module + signatures, cheaply cloneable (`Arc`s) so
/// callers (e.g. an agent session) can cache it for [`crate::diff`].
#[derive(Clone)]
pub struct CompiledShader {
    ir: Arc<LpirModule>,
    sig: Arc<LpsModuleSig>,
}

/// Interpreter "instance": shares the compiled module and owns a VMContext
/// image (uniforms + globals region) that `set_uniform` writes into and every
/// call executes against. Global writes persist across calls.
pub struct InterpInstance {
    ir: Arc<LpirModule>,
    sig: Arc<LpsModuleSig>,
    vmctx_image: Vec<u8>,
    limits: InterpLimits,
}

impl CompiledShader {
    /// Compile a GLSL snippet for interpretation.
    ///
    /// The bare snippet is compiled first so error positions stay in user
    /// (snippet-relative) coordinates. When the snippet references `lpfn_*`
    /// builtins, the execution module is recompiled from the canonical unit
    /// (builtin sources prepended + `lpfn_` → `lpo_` rename) so the
    /// interpreter can evaluate them as local functions.
    pub fn compile(snippet: &str) -> Result<Self, Vec<String>> {
        // Bare compile: user-relative diagnostics (`lpfn_*` calls lower to
        // `@lpfn::` imports here, which is fine for compilation).
        let (ir, sig) = frontend_compile(snippet)?;
        if !references_lpfn(snippet) {
            return Ok(Self::new(ir, sig));
        }
        // Canonical unit for execution. It only fails if a canonical source
        // itself breaks (a bug, not a user error): report unremapped.
        let unit = execution_unit_source(snippet);
        let (ir, sig) = frontend_compile(&unit).map_err(|ds| {
            ds.into_iter()
                .map(|d| format!("canonical unit: {d}"))
                .collect::<Vec<_>>()
        })?;
        Ok(Self::new(ir, sig))
    }

    /// Wrap an already-lowered module.
    pub fn new(ir: LpirModule, sig: LpsModuleSig) -> Self {
        Self {
            ir: Arc::new(ir),
            sig: Arc::new(sig),
        }
    }

    /// Module signatures (function lookup, uniform layout).
    pub fn signatures(&self) -> &LpsModuleSig {
        &self.sig
    }

    /// Create an instance (cheap; shares the module). The instance owns a
    /// fresh zero-initialized VMContext image; if the module carries the
    /// synthesized `__shader_init` (globals/uniforms with declaration
    /// initializers), it runs once here — before any `set_uniform` writes,
    /// matching the compiled runtimes' instantiation order — and the
    /// initialized image is kept.
    ///
    /// Every interpreter run (including `__shader_init` here) is bounded by
    /// the per-evaluation op budget [`crate::experiment::MAX_OPS_PER_EVAL`]:
    /// probing runs untrusted agent GLSL on the Studio wasm main thread, so
    /// an unbounded infinite loop would hang the tab. Callers that need
    /// unbounded runs must opt out via [`Self::instantiate_with_limits`].
    pub fn instantiate(&self) -> Result<InterpInstance, String> {
        self.instantiate_with_limits(InterpLimits::with_fuel(crate::experiment::MAX_OPS_PER_EVAL))
    }

    /// [`Self::instantiate`] with explicit interpreter limits (each `call`
    /// gets a fresh fuel budget of `limits.fuel`).
    pub fn instantiate_with_limits(&self, limits: InterpLimits) -> Result<InterpInstance, String> {
        let mut vmctx_image = vec![0u8; self.sig.vmctx_buffer_size()];
        if self.ir.functions.values().any(|f| f.name == SHADER_INIT_FN) {
            let mut handler = StdMathHandler::default();
            let out = interpret_entry(
                &self.ir,
                SHADER_INIT_FN,
                &[],
                &mut handler,
                &vmctx_image,
                0,
                limits,
            )
            .map_err(|e| format!("interp: {SHADER_INIT_FN}: {e}"))?;
            vmctx_image = out.vmctx_bytes;
        }
        Ok(InterpInstance {
            ir: Arc::clone(&self.ir),
            sig: Arc::clone(&self.sig),
            vmctx_image,
            limits,
        })
    }
}

impl fmt::Debug for CompiledShader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CompiledShader")
            .field("functions", &self.sig.functions.len())
            .finish_non_exhaustive()
    }
}

/// Compile + lower one composed GLSL unit; errors become diagnostic strings.
fn frontend_compile(unit: &str) -> Result<(LpirModule, LpsModuleSig), Vec<String>> {
    let naga = lps_frontend::compile(unit).map_err(|e| vec![e.to_string()])?;
    lps_frontend::lower(&naga).map_err(|e| vec![e.to_string()])
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

    /// Module signatures (function lookup, uniform layout).
    pub fn signatures(&self) -> &LpsModuleSig {
        &self.sig
    }

    /// Execute `name` with f32 argument marshaling via [`interpret_entry`].
    ///
    /// Global writes performed by the call are kept in the instance's
    /// VMContext image, so consecutive calls see each other's state (the
    /// probe determinism contract's frame-sequential approximation).
    pub fn call(&mut self, name: &str, args: &[LpsValueF32]) -> Result<LpsValueF32, String> {
        let gfn = self
            .sig
            .functions
            .iter()
            .find(|f| f.name == name)
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

        let mut handler = StdMathHandler::default();
        let out = interpret_entry(
            &self.ir,
            &gfn.name,
            &flat,
            &mut handler,
            &self.vmctx_image,
            sret_size,
            self.limits,
        )
        .map_err(|e| format!("interp: {name}: {e}"))?;

        // Persist global writes for the next call (frame-sequential state).
        self.vmctx_image = out.vmctx_bytes;

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
/// literal kinds where callers are looser than the GLSL type (e.g. `1` for a
/// `float` parameter).
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
            return Err("texture arguments are not supported by lps-probe".to_string());
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
    fn push_scalars(
        ty: &LpsType,
        words: &mut impl Iterator<Item = u32>,
        out: &mut Vec<Value>,
    ) -> Result<(), String> {
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
        return Err(format!(
            "sret buffer length {} not word-aligned",
            bytes.len()
        ));
    }
    let mut words = bytes
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes(c.try_into().expect("chunks_exact(4)")));
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
            return Err("texture return values are not supported by lps-probe".to_string());
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn executes_simple_function() {
        let shader = CompiledShader::compile("float add(float a, float b) { return a + b; }")
            .expect("compile");
        let mut inst = shader.instantiate().expect("instantiate");
        let out = inst
            .call("add", &[LpsValueF32::F32(1.5), LpsValueF32::F32(2.25)])
            .expect("call");
        assert!(matches!(out, LpsValueF32::F32(x) if x == 3.75));
    }

    #[test]
    fn executes_lpfn_via_canonicals() {
        let shader = CompiledShader::compile("float f(float x) { return lpfn_saturate(x); }")
            .expect("compile");
        let mut inst = shader.instantiate().expect("instantiate");
        let out = inst.call("f", &[LpsValueF32::F32(1.5)]).expect("call");
        assert!(matches!(out, LpsValueF32::F32(x) if x == 1.0));
    }

    #[test]
    fn vec_return() {
        let shader =
            CompiledShader::compile("vec3 mk(float a) { return vec3(a, a + 1.0, a + 2.0); }")
                .expect("compile");
        let mut inst = shader.instantiate().expect("instantiate");
        let out = inst.call("mk", &[LpsValueF32::F32(1.0)]).expect("call");
        assert!(matches!(out, LpsValueF32::Vec3(a) if a == [1.0, 2.0, 3.0]));
    }

    #[test]
    fn global_state_persists_across_calls() {
        let shader = CompiledShader::compile(
            "float acc = 0.0;\nfloat bump(float d) { acc = acc + d; return acc; }",
        )
        .expect("compile");
        let mut inst = shader.instantiate().expect("instantiate");
        let a = inst.call("bump", &[LpsValueF32::F32(1.0)]).expect("call 1");
        let b = inst.call("bump", &[LpsValueF32::F32(2.0)]).expect("call 2");
        assert!(matches!(a, LpsValueF32::F32(x) if x == 1.0));
        assert!(matches!(b, LpsValueF32::F32(x) if x == 3.0));
    }

    #[test]
    fn infinite_loop_call_errors_instead_of_hanging() {
        let shader = CompiledShader::compile(
            "float spin(float x) { while (x >= 0.0) { x = x + 0.0; } return x; }",
        )
        .expect("compile");
        let mut inst = shader.instantiate().expect("instantiate");
        let err = inst
            .call("spin", &[LpsValueF32::F32(1.0)])
            .expect_err("must exhaust the op budget");
        assert!(err.contains("op budget"), "{err}");
    }

    #[test]
    fn instantiate_with_limits_overrides_the_fuel_default() {
        let shader = CompiledShader::compile("float id(float x) { return x; }").expect("compile");
        let mut inst = shader
            .instantiate_with_limits(InterpLimits::with_fuel(1))
            .expect("instantiate");
        let err = inst
            .call("id", &[LpsValueF32::F32(1.0)])
            .expect_err("1 op of fuel cannot finish a call");
        assert!(err.contains("op budget"), "{err}");
    }

    #[test]
    fn compile_error_is_user_relative() {
        let diags = CompiledShader::compile("float ok() { return 1.0; }\nfloat bad = ;\n")
            .expect_err("must fail");
        let joined = diags.join("\n");
        assert!(joined.contains("glsl:2:"), "user line expected:\n{joined}");
    }
}
