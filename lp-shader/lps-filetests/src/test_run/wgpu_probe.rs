//! `wgpu.f32` filetest backend: each `// run:` directive is wrapped as a
//! 1-row fragment render on the GPU tier and read back bit-exactly.
//!
//! The probe forks at the GLSL source exactly like the product GPU path
//! (ADR `docs/adr/2026-07-09-gpu-path-forks-at-glsl.md`): the authored test
//! source is appended with a generated `vec4 render(vec2 pos)` that calls
//! the directive's function and encodes the result scalars into `width =
//! ceil(scalars/4)` pixels, selected by `int(pos.x)`. lp-gfx-wgpu assembles
//! the lpfn prelude + prototypes + fragment `main` and translates through
//! naga glsl-in → wgsl-out at IEEE f32 semantics; `GpuShader::probe_f32`
//! renders into the f32 backing texture and returns raw texels — no
//! quantization, so bit patterns survive.
//!
//! **Result encoding**: float scalars pass through; `int`/`uint` lanes are
//! bit-cast (`intBitsToFloat`/`uintBitsToFloat`) and reinterpreted from the
//! read-back bits on the host — `==` exactness survives iff the pipeline
//! preserves NaN payloads (i32 −1 is `0xFFFFFFFF`, a NaN pattern); the
//! adapter-gated `bitcast_edge_values` test in `tests/wgpu_filetests.rs`
//! proves this per host, with MIN/MAX/−1 edge values. Bools encode as
//! 0.0/1.0.
//!
//! Adapter-gated: [`probe_graphics`] returns `None` on hosts without a GPU
//! adapter and the suite skips the target cleanly. Compilation is per
//! directive (each directive is a different wrapper `render`), which is
//! slower than the CPU targets — acceptable for an explicit local gate.
//!
//! Not supported here (surfaced per directive, triaged in the corpus):
//! texture fixtures (not yet bound through the lp-gfx texture registry) and
//! trap-code expectations.

use std::sync::{Arc, Mutex};

use lp_collection::VecMap;
use lp_gfx_lpvm::TargetLpvmGraphics;
use lp_gfx_wgpu::GpuGraphics;
use lpir::Value;
use lps_shared::{LpsModuleSig, LpsType, LpsValueF32, StructMember, TextureBindingSpec};

use crate::test_run::interp::decode_return;

/// Process-wide adapter-gated GPU context. Rebuildable: a corpus shader
/// that hangs the GPU (no fuel on this tier) can lose the device; the next
/// probe then recreates it so one bad shader costs only its own directive.
static PROBE_GRAPHICS: Mutex<ProbeSlot> = Mutex::new(ProbeSlot::Uninit);

/// Serializes compile+render+readback across the harness's parallel file
/// workers. Concurrent probe pipelines on one device deadlocked inside the
/// Metal backend (`Device::wait` never returning under simultaneous
/// submitters); the probe is not throughput-sensitive, so one directive on
/// the GPU at a time is the robust choice.
static PROBE_SERIAL: Mutex<()> = Mutex::new(());

enum ProbeSlot {
    Uninit,
    /// Host has no GPU adapter (sticky — no point retrying per directive).
    NoAdapter,
    Ready(Arc<GpuGraphics>),
}

fn build_graphics() -> Option<GpuGraphics> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface: None,
    }))
    .ok()?;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("lps-filetests wgpu probe"),
        ..Default::default()
    }))
    .ok()?;
    Some(GpuGraphics::new(
        device,
        queue,
        Box::new(TargetLpvmGraphics::new(lp_shader::ShaderFrontend::Naga)),
    ))
}

/// The shared probe `GpuGraphics`, or `None` when the host has no adapter.
pub fn probe_graphics() -> Option<Arc<GpuGraphics>> {
    let mut slot = PROBE_GRAPHICS.lock().expect("probe graphics lock poisoned");
    match &*slot {
        ProbeSlot::Ready(g) => Some(Arc::clone(g)),
        ProbeSlot::NoAdapter => None,
        ProbeSlot::Uninit => match build_graphics() {
            Some(g) => {
                let g = Arc::new(g);
                *slot = ProbeSlot::Ready(Arc::clone(&g));
                Some(g)
            }
            None => {
                *slot = ProbeSlot::NoAdapter;
                None
            }
        },
    }
}

/// Drop the current device so the next probe rebuilds it (called after
/// errors that indicate a lost/poisoned device).
fn probe_reset() {
    let mut slot = PROBE_GRAPHICS.lock().expect("probe graphics lock poisoned");
    if matches!(&*slot, ProbeSlot::Ready(_)) {
        *slot = ProbeSlot::Uninit;
    }
}

/// Compiled probe module: the authored source (kept for per-directive
/// wrapper assembly) plus its signatures from the naga → LPIR lowering.
pub struct WgpuProbeShader {
    source: Arc<String>,
    sig: Arc<LpsModuleSig>,
    texture_specs: Arc<VecMap<String, TextureBindingSpec>>,
}

/// Per-directive instance: pending `set_uniform` writes overlay the
/// zero-defaults when the wrapper renders.
pub struct WgpuProbeInstance {
    source: Arc<String>,
    sig: Arc<LpsModuleSig>,
    texture_specs: Arc<VecMap<String, TextureBindingSpec>>,
    uniform_writes: Vec<(String, LpsValueF32)>,
}

impl WgpuProbeShader {
    /// Wrap the authored source + its lowered signatures for probing.
    pub fn new(
        source: &str,
        sig: LpsModuleSig,
        texture_specs: &VecMap<String, TextureBindingSpec>,
    ) -> Self {
        Self {
            source: Arc::new(source.to_string()),
            sig: Arc::new(sig),
            texture_specs: Arc::new(texture_specs.clone()),
        }
    }

    /// Module signatures (function lookup for directives).
    pub fn signatures(&self) -> &LpsModuleSig {
        &self.sig
    }

    /// Create a per-directive instance (pending `set_uniform` state only).
    pub fn instantiate(&self) -> WgpuProbeInstance {
        WgpuProbeInstance {
            source: Arc::clone(&self.source),
            sig: Arc::clone(&self.sig),
            texture_specs: Arc::clone(&self.texture_specs),
            uniform_writes: Vec::new(),
        }
    }
}

impl WgpuProbeInstance {
    /// Record a uniform write; applied to the uniform tree at render time.
    pub fn set_uniform(&mut self, path: &str, value: &LpsValueF32) -> Result<(), String> {
        if path.contains('.') {
            return Err(format!(
                "wgpu.f32 probe: dotted set_uniform path `{path}` is not supported yet"
            ));
        }
        self.uniform_writes.push((path.to_string(), value.clone()));
        Ok(())
    }

    /// Execute `name(args)` as a GPU probe render.
    pub fn call(&mut self, name: &str, args: &[LpsValueF32]) -> Result<LpsValueF32, String> {
        if !self.texture_specs.is_empty() {
            return Err(
                "wgpu.f32 probe: texture fixtures are not yet bound through the GPU texture \
                 registry"
                    .to_string(),
            );
        }
        let graphics =
            probe_graphics().ok_or_else(|| "wgpu.f32: no GPU adapter available".to_string())?;

        let gfn = self
            .sig
            .functions
            .iter()
            .find(|f| f.name == name)
            .ok_or_else(|| format!("wgpu probe: function '{name}' not found in signature"))?;
        if gfn.parameters.len() != args.len() {
            return Err(format!(
                "wgpu probe: {name} expects {} args, got {}",
                gfn.parameters.len(),
                args.len()
            ));
        }

        // Scalar plan over the return type: GLSL accessor expression + lane
        // encoding per scalar, in `decode_return` order.
        let mut plan: Vec<(String, Lane)> = Vec::new();
        scalar_plan(&gfn.return_type, "__r", &mut plan)
            .map_err(|e| format!("wgpu probe: {name} return: {e}"))?;

        let call_args: Vec<String> = args.iter().map(glsl_literal).collect::<Result<_, _>>()?;
        let call_expr = format!("{name}({})", call_args.join(", "));

        // The generated wrapper is the assembly entry (`vec4 render(vec2)`);
        // if the authored source defines `render` itself, rename it (and the
        // call) out of the way.
        let (authored, call_expr) = if self.sig.functions.iter().any(|f| f.name == "render") {
            let renamed = rename_word(&self.source, "render", "__authored_render");
            let call = if name == "render" {
                format!("__authored_render({})", call_args.join(", "))
            } else {
                call_expr
            };
            (renamed, call)
        } else {
            (self.source.as_ref().clone(), call_expr)
        };

        let width = plan.len().div_ceil(4).max(1) as u32;
        let mut wrapper = String::new();
        wrapper.push_str("\nvec4 render(vec2 __pos) {\n");
        if matches!(gfn.return_type, LpsType::Void) {
            wrapper.push_str(&format!("    {call_expr};\n"));
        } else {
            wrapper.push_str(&format!(
                "    {} __r = {call_expr};\n",
                glsl_type_name(&gfn.return_type)
                    .map_err(|e| format!("wgpu probe: {name} return: {e}"))?
            ));
        }
        wrapper.push_str("    int __px = int(__pos.x);\n");
        for (px, quad) in plan.chunks(4).enumerate() {
            let mut lanes: Vec<String> = quad.iter().map(|(e, l)| l.encode(e)).collect();
            while lanes.len() < 4 {
                lanes.push(String::from("0.0"));
            }
            wrapper.push_str(&format!(
                "    if (__px == {px}) {{ return vec4({}); }}\n",
                lanes.join(", ")
            ));
        }
        wrapper.push_str("    return vec4(0.0);\n}\n");

        let probe_source = format!("{authored}\n{wrapper}");

        let _gpu_turn = PROBE_SERIAL.lock().expect("probe serial lock poisoned");
        let mut shader = graphics
            .compile_probe_shader(&probe_source, &self.texture_specs)
            .map_err(|e| format!("wgpu probe compile: {e}"))?;

        let uniforms = self.uniform_tree()?;
        let texels = shader.probe_f32(width, &uniforms).map_err(|e| {
            // A timed-out or failed submission can poison the device;
            // rebuild it for the next directive.
            probe_reset();
            format!("wgpu probe render: {e}")
        })?;

        // Reinterpret lanes per the plan and decode like the interp target.
        let scalars: Vec<Value> = plan
            .iter()
            .zip(texels.iter())
            .map(|((_, lane), &f)| lane.decode(f))
            .collect();
        let mut it = scalars.iter().copied();
        let v = decode_return(&gfn.return_type, &mut it)
            .map_err(|e| format!("wgpu probe: {name} return: {e}"))?;
        Ok(v)
    }

    /// Engine uniform tree: zero defaults for every reflected uniform,
    /// overlaid with this directive's `set_uniform` writes. (Uniform
    /// declaration initializers are not applied — directives relying on
    /// them are triaged in the corpus.)
    fn uniform_tree(&self) -> Result<LpsValueF32, String> {
        let mut fields: Vec<(String, LpsValueF32)> = Vec::new();
        if let Some(LpsType::Struct { members, .. }) = &self.sig.uniforms_type {
            for m in members {
                let name = m
                    .name
                    .clone()
                    .ok_or_else(|| "unnamed uniform member".to_string())?;
                fields.push((name, zero_value(&m.ty)?));
            }
        }
        for (path, value) in &self.uniform_writes {
            match fields.iter_mut().find(|(n, _)| n == path) {
                Some((_, slot)) => *slot = value.clone(),
                None => {
                    return Err(format!(
                        "set_uniform `{path}`: no uniform of that name in the module"
                    ));
                }
            }
        }
        Ok(LpsValueF32::Struct { name: None, fields })
    }
}

/// Per-scalar lane encoding through the f32 render target.
#[derive(Clone, Copy)]
enum Lane {
    F32,
    IntBits,
    UintBits,
    Bool,
}

impl Lane {
    fn encode(self, expr: &str) -> String {
        match self {
            Lane::F32 => expr.to_string(),
            Lane::IntBits => format!("intBitsToFloat({expr})"),
            Lane::UintBits => format!("uintBitsToFloat({expr})"),
            Lane::Bool => format!("(({expr}) ? 1.0 : 0.0)"),
        }
    }

    fn decode(self, f: f32) -> Value {
        match self {
            Lane::F32 => Value::F32(f),
            Lane::IntBits | Lane::UintBits => Value::I32(f.to_bits() as i32),
            Lane::Bool => Value::I32(i32::from(f != 0.0)),
        }
    }
}

/// Flatten a return type into (GLSL accessor, lane) pairs in
/// [`decode_return`] scalar order.
fn scalar_plan(ty: &LpsType, expr: &str, out: &mut Vec<(String, Lane)>) -> Result<(), String> {
    const XYZW: [char; 4] = ['x', 'y', 'z', 'w'];
    let comps = |n: usize, lane: Lane, out: &mut Vec<(String, Lane)>| {
        for &c in XYZW.iter().take(n) {
            out.push((format!("{expr}.{c}"), lane));
        }
    };
    match ty {
        LpsType::Void => {}
        LpsType::Float => out.push((expr.to_string(), Lane::F32)),
        LpsType::Int => out.push((expr.to_string(), Lane::IntBits)),
        LpsType::UInt => out.push((expr.to_string(), Lane::UintBits)),
        LpsType::Bool => out.push((expr.to_string(), Lane::Bool)),
        LpsType::Vec2 => comps(2, Lane::F32, out),
        LpsType::Vec3 => comps(3, Lane::F32, out),
        LpsType::Vec4 => comps(4, Lane::F32, out),
        LpsType::IVec2 => comps(2, Lane::IntBits, out),
        LpsType::IVec3 => comps(3, Lane::IntBits, out),
        LpsType::IVec4 => comps(4, Lane::IntBits, out),
        LpsType::UVec2 => comps(2, Lane::UintBits, out),
        LpsType::UVec3 => comps(3, Lane::UintBits, out),
        LpsType::UVec4 => comps(4, Lane::UintBits, out),
        LpsType::BVec2 => comps(2, Lane::Bool, out),
        LpsType::BVec3 => comps(3, Lane::Bool, out),
        LpsType::BVec4 => comps(4, Lane::Bool, out),
        LpsType::Mat2 | LpsType::Mat3 | LpsType::Mat4 => {
            let n = match ty {
                LpsType::Mat2 => 2,
                LpsType::Mat3 => 3,
                _ => 4,
            };
            // decode_return order is column-major: m[col][row].
            for col in 0..n {
                for row in 0..n {
                    out.push((format!("{expr}[{col}][{row}]"), Lane::F32));
                }
            }
        }
        LpsType::Array { element, len } => {
            for i in 0..*len {
                scalar_plan(element, &format!("{expr}[{i}]"), out)?;
            }
        }
        LpsType::Struct { members, .. } => {
            for m in members {
                let name = m
                    .name
                    .as_ref()
                    .ok_or_else(|| "unnamed struct member in return type".to_string())?;
                scalar_plan(&m.ty, &format!("{expr}.{name}"), out)?;
            }
        }
        LpsType::Texture2D => return Err("texture return type".to_string()),
    }
    Ok(())
}

/// GLSL spelling of a type for the wrapper's result declaration.
fn glsl_type_name(ty: &LpsType) -> Result<String, String> {
    Ok(match ty {
        LpsType::Void => String::from("void"),
        LpsType::Float => String::from("float"),
        LpsType::Int => String::from("int"),
        LpsType::UInt => String::from("uint"),
        LpsType::Bool => String::from("bool"),
        LpsType::Vec2 => String::from("vec2"),
        LpsType::Vec3 => String::from("vec3"),
        LpsType::Vec4 => String::from("vec4"),
        LpsType::IVec2 => String::from("ivec2"),
        LpsType::IVec3 => String::from("ivec3"),
        LpsType::IVec4 => String::from("ivec4"),
        LpsType::UVec2 => String::from("uvec2"),
        LpsType::UVec3 => String::from("uvec3"),
        LpsType::UVec4 => String::from("uvec4"),
        LpsType::BVec2 => String::from("bvec2"),
        LpsType::BVec3 => String::from("bvec3"),
        LpsType::BVec4 => String::from("bvec4"),
        LpsType::Mat2 => String::from("mat2"),
        LpsType::Mat3 => String::from("mat3"),
        LpsType::Mat4 => String::from("mat4"),
        LpsType::Array { element, len } => format!("{}[{len}]", glsl_type_name(element)?),
        LpsType::Struct { name, .. } => name
            .clone()
            .ok_or_else(|| "unnamed struct return type".to_string())?,
        LpsType::Texture2D => return Err("texture type in wrapper".to_string()),
    })
}

/// Format a directive argument as a GLSL literal expression.
fn glsl_literal(v: &LpsValueF32) -> Result<String, String> {
    fn f(x: f32) -> String {
        if x.is_infinite() {
            return format!("({} 1.0 / 0.0)", if x < 0.0 { "-" } else { "" });
        }
        let s = format!("{x}");
        if s.contains('.') || s.contains('e') || s.contains("inf") || s.contains("NaN") {
            s
        } else {
            format!("{s}.0")
        }
    }
    fn join(parts: Vec<String>) -> String {
        parts.join(", ")
    }
    Ok(match v {
        LpsValueF32::F32(x) => f(*x),
        LpsValueF32::I32(x) => format!("{x}"),
        LpsValueF32::U32(x) => format!("{x}u"),
        LpsValueF32::Bool(b) => format!("{b}"),
        LpsValueF32::Vec2(a) => format!("vec2({})", join(a.iter().map(|&x| f(x)).collect())),
        LpsValueF32::Vec3(a) => format!("vec3({})", join(a.iter().map(|&x| f(x)).collect())),
        LpsValueF32::Vec4(a) => format!("vec4({})", join(a.iter().map(|&x| f(x)).collect())),
        LpsValueF32::IVec2(a) => {
            format!("ivec2({})", join(a.iter().map(|x| x.to_string()).collect()))
        }
        LpsValueF32::IVec3(a) => {
            format!("ivec3({})", join(a.iter().map(|x| x.to_string()).collect()))
        }
        LpsValueF32::IVec4(a) => {
            format!("ivec4({})", join(a.iter().map(|x| x.to_string()).collect()))
        }
        LpsValueF32::UVec2(a) => {
            format!(
                "uvec2({})",
                join(a.iter().map(|x| format!("{x}u")).collect())
            )
        }
        LpsValueF32::UVec3(a) => {
            format!(
                "uvec3({})",
                join(a.iter().map(|x| format!("{x}u")).collect())
            )
        }
        LpsValueF32::UVec4(a) => {
            format!(
                "uvec4({})",
                join(a.iter().map(|x| format!("{x}u")).collect())
            )
        }
        LpsValueF32::BVec2(a) => {
            format!("bvec2({})", join(a.iter().map(|x| x.to_string()).collect()))
        }
        LpsValueF32::BVec3(a) => {
            format!("bvec3({})", join(a.iter().map(|x| x.to_string()).collect()))
        }
        LpsValueF32::BVec4(a) => {
            format!("bvec4({})", join(a.iter().map(|x| x.to_string()).collect()))
        }
        LpsValueF32::Mat2x2(m) => format!(
            "mat2({})",
            join(m.iter().flatten().map(|&x| f(x)).collect())
        ),
        LpsValueF32::Mat3x3(m) => format!(
            "mat3({})",
            join(m.iter().flatten().map(|&x| f(x)).collect())
        ),
        LpsValueF32::Mat4x4(m) => format!(
            "mat4({})",
            join(m.iter().flatten().map(|&x| f(x)).collect())
        ),
        LpsValueF32::Array(items) => {
            let inner: Vec<String> = items.iter().map(glsl_literal).collect::<Result<_, _>>()?;
            let elem_ty = items
                .first()
                .map(lps_value_type_name)
                .transpose()?
                .ok_or_else(|| "empty array literal".to_string())?;
            format!("{elem_ty}[{}]({})", items.len(), join(inner))
        }
        LpsValueF32::Struct { name, fields } => {
            let ctor = name
                .clone()
                .ok_or_else(|| "unnamed struct literal".to_string())?;
            let inner: Vec<String> = fields
                .iter()
                .map(|(_, v)| glsl_literal(v))
                .collect::<Result<_, _>>()?;
            format!("{ctor}({})", join(inner))
        }
        LpsValueF32::Texture2D(_) => return Err("texture argument".to_string()),
    })
}

/// GLSL type name of a value (array element constructor spelling).
fn lps_value_type_name(v: &LpsValueF32) -> Result<String, String> {
    Ok(match v {
        LpsValueF32::F32(_) => String::from("float"),
        LpsValueF32::I32(_) => String::from("int"),
        LpsValueF32::U32(_) => String::from("uint"),
        LpsValueF32::Bool(_) => String::from("bool"),
        LpsValueF32::Vec2(_) => String::from("vec2"),
        LpsValueF32::Vec3(_) => String::from("vec3"),
        LpsValueF32::Vec4(_) => String::from("vec4"),
        LpsValueF32::IVec2(_) => String::from("ivec2"),
        LpsValueF32::IVec3(_) => String::from("ivec3"),
        LpsValueF32::IVec4(_) => String::from("ivec4"),
        LpsValueF32::UVec2(_) => String::from("uvec2"),
        LpsValueF32::UVec3(_) => String::from("uvec3"),
        LpsValueF32::UVec4(_) => String::from("uvec4"),
        LpsValueF32::BVec2(_) => String::from("bvec2"),
        LpsValueF32::BVec3(_) => String::from("bvec3"),
        LpsValueF32::BVec4(_) => String::from("bvec4"),
        LpsValueF32::Mat2x2(_) => String::from("mat2"),
        LpsValueF32::Mat3x3(_) => String::from("mat3"),
        LpsValueF32::Mat4x4(_) => String::from("mat4"),
        other => return Err(format!("unsupported array element {other:?}")),
    })
}

/// Zero value of a type (uniform defaults for reflected globals).
fn zero_value(ty: &LpsType) -> Result<LpsValueF32, String> {
    Ok(match ty {
        LpsType::Float => LpsValueF32::F32(0.0),
        LpsType::Int => LpsValueF32::I32(0),
        LpsType::UInt => LpsValueF32::U32(0),
        LpsType::Bool => LpsValueF32::Bool(false),
        LpsType::Vec2 => LpsValueF32::Vec2([0.0; 2]),
        LpsType::Vec3 => LpsValueF32::Vec3([0.0; 3]),
        LpsType::Vec4 => LpsValueF32::Vec4([0.0; 4]),
        LpsType::IVec2 => LpsValueF32::IVec2([0; 2]),
        LpsType::IVec3 => LpsValueF32::IVec3([0; 3]),
        LpsType::IVec4 => LpsValueF32::IVec4([0; 4]),
        LpsType::UVec2 => LpsValueF32::UVec2([0; 2]),
        LpsType::UVec3 => LpsValueF32::UVec3([0; 3]),
        LpsType::UVec4 => LpsValueF32::UVec4([0; 4]),
        LpsType::BVec2 => LpsValueF32::BVec2([false; 2]),
        LpsType::BVec3 => LpsValueF32::BVec3([false; 3]),
        LpsType::BVec4 => LpsValueF32::BVec4([false; 4]),
        LpsType::Mat2 => LpsValueF32::Mat2x2([[0.0; 2]; 2]),
        LpsType::Mat3 => LpsValueF32::Mat3x3([[0.0; 3]; 3]),
        LpsType::Mat4 => LpsValueF32::Mat4x4([[0.0; 4]; 4]),
        LpsType::Array { element, len } => {
            let mut items = Vec::with_capacity(*len as usize);
            for _ in 0..*len {
                items.push(zero_value(element)?);
            }
            LpsValueF32::Array(items.into_boxed_slice())
        }
        LpsType::Struct { name, members } => LpsValueF32::Struct {
            name: name.clone(),
            fields: members
                .iter()
                .map(|m: &StructMember| {
                    Ok((m.name.clone().unwrap_or_default(), zero_value(&m.ty)?))
                })
                .collect::<Result<_, String>>()?,
        },
        LpsType::Void | LpsType::Texture2D => {
            return Err(format!("cannot zero-default a {ty:?} uniform"));
        }
    })
}

/// Word-boundary identifier rename (used to move an authored `render`
/// definition out of the generated wrapper's way).
fn rename_word(src: &str, from: &str, to: &str) -> String {
    let bytes = src.as_bytes();
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    while let Some(pos) = src[i..].find(from) {
        let at = i + pos;
        let before_ok =
            at == 0 || !(bytes[at - 1].is_ascii_alphanumeric() || bytes[at - 1] == b'_');
        let after = at + from.len();
        let after_ok =
            after >= bytes.len() || !(bytes[after].is_ascii_alphanumeric() || bytes[after] == b'_');
        out.push_str(&src[i..at]);
        if before_ok && after_ok {
            out.push_str(to);
        } else {
            out.push_str(from);
        }
        i = after;
    }
    out.push_str(&src[i..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The probe's int/uint encoding is a bit-cast through an f32 render
    /// target; this proves the pipeline preserves the bit patterns that are
    /// NaN payloads as floats (i32 −1 = 0xFFFFFFFF, i32::MAX = 0x7FFFFFFF).
    /// If a host's GPU canonicalizes NaNs, this test fails and the encoding
    /// must switch to a two-lane 16-bit split. Adapter-gated.
    #[test]
    fn bitcast_edge_values_survive_the_pipeline() {
        if probe_graphics().is_none() {
            eprintln!("SKIP: no GPU adapter available");
            return;
        }
        let source = "int edge_min() { return -2147483647 - 1; }\n\
                      int edge_max() { return 2147483647; }\n\
                      int edge_neg_one() { return -1; }\n\
                      uint edge_uint_max() { return 4294967295u; }\n";
        let (_, sig) = crate::test_run::interp::lower_for_interp(
            source,
            &VecMap::new(),
            &lpir::CompilerConfig::default(),
        )
        .expect("lower");
        let shader = WgpuProbeShader::new(source, sig, &VecMap::new());
        let mut inst = shader.instantiate();
        let cases: [(&str, LpsValueF32); 4] = [
            ("edge_min", LpsValueF32::I32(i32::MIN)),
            ("edge_max", LpsValueF32::I32(i32::MAX)),
            ("edge_neg_one", LpsValueF32::I32(-1)),
            ("edge_uint_max", LpsValueF32::U32(u32::MAX)),
        ];
        for (name, expected) in &cases {
            let got = inst.call(name, &[]).expect(name);
            assert!(
                got.eq(expected),
                "{name}: got {got:?}, expected {expected:?} — GPU does not preserve \
                 NaN-payload bit patterns; switch the probe to a 16-bit split encoding"
            );
        }
    }

    #[test]
    fn rename_word_respects_boundaries() {
        assert_eq!(
            rename_word(
                "vec4 render(vec2 p) { rendered(); render(); }",
                "render",
                "__r"
            ),
            "vec4 __r(vec2 p) { rendered(); __r(); }"
        );
    }

    #[test]
    fn scalar_plan_orders_mat2_column_major() {
        let mut plan = Vec::new();
        scalar_plan(&LpsType::Mat2, "r", &mut plan).unwrap();
        let exprs: Vec<&str> = plan.iter().map(|(e, _)| e.as_str()).collect();
        assert_eq!(exprs, ["r[0][0]", "r[0][1]", "r[1][0]", "r[1][1]"]);
    }

    #[test]
    fn glsl_literal_round_trips_scalars() {
        assert_eq!(glsl_literal(&LpsValueF32::F32(2.0)).unwrap(), "2.0");
        assert_eq!(glsl_literal(&LpsValueF32::I32(-7)).unwrap(), "-7");
        assert_eq!(glsl_literal(&LpsValueF32::U32(3)).unwrap(), "3u");
        assert_eq!(
            glsl_literal(&LpsValueF32::Vec2([1.5, -2.0])).unwrap(),
            "vec2(1.5, -2.0)"
        );
    }
}
