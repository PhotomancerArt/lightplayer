//! LPVM trait implementations: [`CraneliftInstance`] and [`lpvm::LpvmModule`] for [`CraneliftModule`].

use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt;

use cranelift_codegen::ir::ArgumentPurpose;
use lpir::FloatMode;
use lps_shared::{LpsModuleSig, LpsType, ParamQualifier};
use lpvm::{LpsValue, LpvmInstance, LpvmModule, VMCTX_HEADER_SIZE, VmContext};

use crate::lpvm_module::CraneliftModule;
use crate::values::{CallError, GlslQ32, decode_q32_return, flatten_q32_arg};

/// Execution error for [`CraneliftInstance`].
#[derive(Debug)]
pub enum InstanceError {
    Call(CallError),
    Unsupported(&'static str),
}

impl fmt::Display for InstanceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InstanceError::Call(e) => e.fmt(f),
            InstanceError::Unsupported(s) => write!(f, "{s}"),
        }
    }
}

impl From<CallError> for InstanceError {
    fn from(value: CallError) -> Self {
        InstanceError::Call(value)
    }
}

/// Per-instance VMContext storage and [`LpvmInstance`] over a finalized [`CraneliftModule`].
pub struct CraneliftInstance {
    module: Arc<CraneliftModule>,
    vmctx_buf: Vec<u8>,
}

impl CraneliftInstance {
    pub(crate) fn new(module: &CraneliftModule) -> Self {
        let mut vmctx_buf = Vec::new();
        vmctx_buf.resize(VMCTX_HEADER_SIZE, 0);
        let header = VmContext::default();
        unsafe {
            core::ptr::write(vmctx_buf.as_mut_ptr().cast(), header);
        }
        Self {
            module: Arc::new(module.clone()),
            vmctx_buf,
        }
    }

    fn vmctx_ptr(&self) -> *const u8 {
        self.vmctx_buf.as_ptr()
    }
}

impl LpvmModule for CraneliftModule {
    type Instance = CraneliftInstance;
    type Error = InstanceError;

    fn signatures(&self) -> &LpsModuleSig {
        self.metadata()
    }

    fn instantiate(&self) -> Result<Self::Instance, Self::Error> {
        Ok(CraneliftInstance::new(self))
    }
}

impl LpvmInstance for CraneliftInstance {
    type Error = InstanceError;

    fn call(&mut self, name: &str, args: &[LpsValue]) -> Result<LpsValue, Self::Error> {
        if self.module.float_mode() != FloatMode::Q32 {
            return Err(InstanceError::Unsupported(
                "CraneliftInstance::call requires FloatMode::Q32; use direct_call for F32 JIT",
            ));
        }

        let gfn = self
            .module
            .metadata()
            .functions
            .iter()
            .find(|f| f.name == name)
            .ok_or_else(|| CallError::MissingMetadata(name.into()))?;

        for p in &gfn.parameters {
            if matches!(p.qualifier, ParamQualifier::Out | ParamQualifier::InOut) {
                return Err(CallError::Unsupported(String::from(
                    "out/inout parameters are not supported for direct calling.",
                ))
                .into());
            }
        }

        if gfn.return_type == LpsType::Void {
            return Err(InstanceError::Unsupported(
                "void return is not represented as LpsValue; use a typed return",
            ));
        }

        if gfn.parameters.len() != args.len() {
            return Err(CallError::Arity {
                expected: gfn.parameters.len(),
                got: args.len(),
            }
            .into());
        }

        let idx = *self
            .module
            .name_to_index
            .get(name)
            .ok_or_else(|| CallError::MissingMetadata(name.into()))?;
        let param_count = self.module.ir_param_counts[idx] as usize;

        let mut flat: Vec<i32> = Vec::new();
        for (p, a) in gfn.parameters.iter().zip(args.iter()) {
            let q = lps_value_to_glsl_q32(&p.ty, a)?;
            flat.extend(flatten_q32_arg(p, &q)?);
        }
        if flat.len() != param_count {
            return Err(CallError::Unsupported(format!(
                "flattened argument count {} does not match IR param_count {}",
                flat.len(),
                param_count
            ))
            .into());
        }

        let sig = self
            .module
            .signature(name)
            .ok_or_else(|| CallError::MissingMetadata(name.into()))?;
        let uses_struct_return = sig
            .params
            .iter()
            .any(|p| p.purpose == ArgumentPurpose::StructReturn);
        let n_ret = self
            .module
            .logical_return_words
            .get(name)
            .copied()
            .unwrap_or_else(|| sig.returns.len());

        let code = self.module.code_ptr(name).ok_or_else(|| {
            CallError::Unsupported(String::from("internal: missing code pointer"))
        })?;

        let words = unsafe {
            crate::invoke::invoke_i32_args_returns(
                code,
                self.vmctx_ptr(),
                flat.as_slice(),
                n_ret,
                uses_struct_return,
            )?
        };

        let gq = decode_q32_return(&gfn.return_type, &words)?;
        glsl_q32_to_lps_value(&gfn.return_type, gq)
    }
}

fn lps_value_to_glsl_q32(ty: &LpsType, v: &LpsValue) -> Result<GlslQ32, CallError> {
    Ok(match (ty, v) {
        (LpsType::Float, LpsValue::F32(x)) => GlslQ32::Float(f64::from(*x)),
        (LpsType::Int, LpsValue::I32(x)) => GlslQ32::Int(*x),
        (LpsType::UInt, LpsValue::U32(x)) => GlslQ32::UInt(*x),
        (LpsType::Bool, LpsValue::Bool(b)) => GlslQ32::Bool(*b),

        (LpsType::Vec2, LpsValue::Vec2(a)) => GlslQ32::Vec2(f64::from(a[0]), f64::from(a[1])),
        (LpsType::Vec3, LpsValue::Vec3(a)) => {
            GlslQ32::Vec3(f64::from(a[0]), f64::from(a[1]), f64::from(a[2]))
        }
        (LpsType::Vec4, LpsValue::Vec4(a)) => GlslQ32::Vec4(
            f64::from(a[0]),
            f64::from(a[1]),
            f64::from(a[2]),
            f64::from(a[3]),
        ),

        (LpsType::IVec2, LpsValue::IVec2(a)) => GlslQ32::IVec2(a[0], a[1]),
        (LpsType::IVec3, LpsValue::IVec3(a)) => GlslQ32::IVec3(a[0], a[1], a[2]),
        (LpsType::IVec4, LpsValue::IVec4(a)) => GlslQ32::IVec4(a[0], a[1], a[2], a[3]),

        (LpsType::UVec2, LpsValue::UVec2(a)) => GlslQ32::UVec2(a[0], a[1]),
        (LpsType::UVec3, LpsValue::UVec3(a)) => GlslQ32::UVec3(a[0], a[1], a[2]),
        (LpsType::UVec4, LpsValue::UVec4(a)) => GlslQ32::UVec4(a[0], a[1], a[2], a[3]),

        (LpsType::BVec2, LpsValue::BVec2(a)) => GlslQ32::BVec2(a[0], a[1]),
        (LpsType::BVec3, LpsValue::BVec3(a)) => GlslQ32::BVec3(a[0], a[1], a[2]),
        (LpsType::BVec4, LpsValue::BVec4(a)) => GlslQ32::BVec4(a[0], a[1], a[2], a[3]),

        (LpsType::Mat2, LpsValue::Mat2x2(m)) => GlslQ32::Mat2([
            f64::from(m[0][0]),
            f64::from(m[0][1]),
            f64::from(m[1][0]),
            f64::from(m[1][1]),
        ]),
        (LpsType::Mat3, LpsValue::Mat3x3(m)) => GlslQ32::Mat3([
            f64::from(m[0][0]),
            f64::from(m[0][1]),
            f64::from(m[0][2]),
            f64::from(m[1][0]),
            f64::from(m[1][1]),
            f64::from(m[1][2]),
            f64::from(m[2][0]),
            f64::from(m[2][1]),
            f64::from(m[2][2]),
        ]),
        (LpsType::Mat4, LpsValue::Mat4x4(m)) => GlslQ32::Mat4([
            f64::from(m[0][0]),
            f64::from(m[0][1]),
            f64::from(m[0][2]),
            f64::from(m[0][3]),
            f64::from(m[1][0]),
            f64::from(m[1][1]),
            f64::from(m[1][2]),
            f64::from(m[1][3]),
            f64::from(m[2][0]),
            f64::from(m[2][1]),
            f64::from(m[2][2]),
            f64::from(m[2][3]),
            f64::from(m[3][0]),
            f64::from(m[3][1]),
            f64::from(m[3][2]),
            f64::from(m[3][3]),
        ]),

        (LpsType::Array { element, len }, LpsValue::Array(items)) => {
            if items.len() != *len as usize {
                return Err(CallError::TypeMismatch(format!(
                    "array length mismatch: expected {}, got {}",
                    len,
                    items.len()
                )));
            }
            let mut out = Vec::with_capacity(items.len());
            for it in items.iter() {
                out.push(lps_value_to_glsl_q32(element, it)?);
            }
            GlslQ32::Array(out)
        }

        (LpsType::Struct { .. }, LpsValue::Struct { .. }) => {
            return Err(CallError::Unsupported(String::from(
                "struct parameters are not supported by CraneliftInstance::call yet",
            )));
        }

        (expected, _got) => {
            return Err(CallError::TypeMismatch(format!(
                "argument type mismatch: expected {expected:?}, got incompatible LpsValue"
            )));
        }
    })
}

fn glsl_q32_to_lps_value(ty: &LpsType, v: GlslQ32) -> Result<LpsValue, InstanceError> {
    let bad = || {
        InstanceError::Call(CallError::TypeMismatch(format!(
            "return shape mismatch for type {ty:?}"
        )))
    };

    Ok(match (ty, v) {
        (LpsType::Float, GlslQ32::Float(x)) => LpsValue::F32(x as f32),
        (LpsType::Int, GlslQ32::Int(x)) => LpsValue::I32(x),
        (LpsType::UInt, GlslQ32::UInt(x)) => LpsValue::U32(x),
        (LpsType::Bool, GlslQ32::Bool(b)) => LpsValue::Bool(b),

        (LpsType::Vec2, GlslQ32::Vec2(a, b)) => LpsValue::Vec2([a as f32, b as f32]),
        (LpsType::Vec3, GlslQ32::Vec3(a, b, c)) => LpsValue::Vec3([a as f32, b as f32, c as f32]),
        (LpsType::Vec4, GlslQ32::Vec4(a, b, c, d)) => {
            LpsValue::Vec4([a as f32, b as f32, c as f32, d as f32])
        }

        (LpsType::IVec2, GlslQ32::IVec2(a, b)) => LpsValue::IVec2([a, b]),
        (LpsType::IVec3, GlslQ32::IVec3(a, b, c)) => LpsValue::IVec3([a, b, c]),
        (LpsType::IVec4, GlslQ32::IVec4(a, b, c, d)) => LpsValue::IVec4([a, b, c, d]),

        (LpsType::UVec2, GlslQ32::UVec2(a, b)) => LpsValue::UVec2([a, b]),
        (LpsType::UVec3, GlslQ32::UVec3(a, b, c)) => LpsValue::UVec3([a, b, c]),
        (LpsType::UVec4, GlslQ32::UVec4(a, b, c, d)) => LpsValue::UVec4([a, b, c, d]),

        (LpsType::BVec2, GlslQ32::BVec2(a, b)) => LpsValue::BVec2([a, b]),
        (LpsType::BVec3, GlslQ32::BVec3(a, b, c)) => LpsValue::BVec3([a, b, c]),
        (LpsType::BVec4, GlslQ32::BVec4(a, b, c, d)) => LpsValue::BVec4([a, b, c, d]),

        (LpsType::Mat2, GlslQ32::Mat2(a)) => {
            LpsValue::Mat2x2([[a[0] as f32, a[1] as f32], [a[2] as f32, a[3] as f32]])
        }
        (LpsType::Mat3, GlslQ32::Mat3(a)) => LpsValue::Mat3x3([
            [a[0] as f32, a[1] as f32, a[2] as f32],
            [a[3] as f32, a[4] as f32, a[5] as f32],
            [a[6] as f32, a[7] as f32, a[8] as f32],
        ]),
        (LpsType::Mat4, GlslQ32::Mat4(a)) => LpsValue::Mat4x4([
            [a[0] as f32, a[1] as f32, a[2] as f32, a[3] as f32],
            [a[4] as f32, a[5] as f32, a[6] as f32, a[7] as f32],
            [a[8] as f32, a[9] as f32, a[10] as f32, a[11] as f32],
            [a[12] as f32, a[13] as f32, a[14] as f32, a[15] as f32],
        ]),

        (LpsType::Array { element, len }, GlslQ32::Array(items)) => {
            if items.len() != *len as usize {
                return Err(bad());
            }
            let mut elems = Vec::with_capacity(items.len());
            for g in items {
                elems.push(glsl_q32_to_lps_value(element, g)?);
            }
            LpsValue::Array(elems.into_boxed_slice())
        }

        _ => return Err(bad()),
    })
}
