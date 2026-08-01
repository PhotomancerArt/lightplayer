//! Prune unused LPIR imports and map declarations to `builtins` WASM names.

use alloc::collections::BTreeSet;
use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use lpir::FloatMode;
use lpir::{CalleeRef, ImportDecl, ImportId, IrFunction, IrType, LpirModule, LpirOp};
use lps_builtin_ids::{
    BuiltinId, GlslParamKind, Mode, glsl_lpfn_builtin_id, glsl_math_builtin_id, lpir_builtin_id,
    texture_builtin_id, vm_builtin_id,
};

/// After pruning: WASM import function index `i` corresponds to `filtered[i]`.
pub(crate) struct FilteredImports {
    pub decls: Vec<ImportDecl>,
    /// `remap[old_index] = Some(wasm_import_func_index)` for kept imports, else `None`.
    pub remap: Vec<Option<u32>>,
}

fn collect_used_import_indices(ir: &LpirModule, mode: FloatMode) -> BTreeSet<u32> {
    let n = ir.imports.len() as u32;
    let mut used = BTreeSet::new();

    for f in ir.functions.values() {
        for op in &f.body {
            if let LpirOp::Call { callee, .. } = op {
                if let CalleeRef::Import(ImportId(i)) = callee {
                    // Inline-lowered builtins never become wasm imports.
                    if (*i as u32) < n && !import_is_inline_get_fuel(ir, *i as usize, mode) {
                        used.insert(*i as u32);
                    }
                }
            }
        }
    }

    // Add imports for LPIR ops that call builtins **in Q32 mode only**.
    //
    // `Fsqrt`/`Fnearest` lower to `@lpir::sqrt`/`@glsl::round` calls under Q32,
    // but under F32 `ops.rs` emits the native `f32.sqrt`/`f32.nearest`
    // instructions and never calls anything. Marking the helper imports used
    // anyway declared a Q32 import no call site referenced — dead weight that
    // additionally made the module unemittable once f32 import resolution
    // started rejecting Q32-typed float imports. `with_missing_helper_imports`
    // already had this `mode != Q32` guard; this scan did not, and the two must
    // agree or the import section and the code section disagree.
    if mode == FloatMode::Q32 {
        let (needs_lpir_sqrt, needs_glsl_round) = scan_helper_op_needs(ir);
        if needs_lpir_sqrt {
            if let Some(idx) = find_import_index(ir, "lpir", "sqrt") {
                used.insert(idx);
            }
        }
        if needs_glsl_round {
            if let Some(idx) = find_import_index(ir, "glsl", "round") {
                used.insert(idx);
            }
        }
    }

    used
}

/// True if the import resolves to [`BuiltinId::LpVmGetFuel`].
///
/// The wasm emitter inlines this builtin as a direct load of the vmctx fuel
/// low word (see the `LpirOp::Call` arm in `ops.rs`) instead of calling an
/// import: the native `__lp_vm_get_fuel` reads the header through a
/// pointer, which can never work on the wasm hosts — there the vmctx block
/// sits at linear-memory offset 0, and Rust rejects address-0 dereferences
/// (a hard "null pointer dereference" trap in debug builds, UB in release).
/// Inlined calls are not "used" imports, so the import section omits the
/// entry entirely.
///
/// `mode` is threaded rather than assumed: every resolution in this module is
/// mode-scoped, and a call site that quietly picks one mode is how the f32 path
/// broke the first time.
pub(crate) fn import_is_inline_get_fuel(
    ir: &LpirModule,
    import_idx: usize,
    mode: FloatMode,
) -> bool {
    ir.imports.get(import_idx).is_some_and(|d| {
        resolve_builtin_id_for_mode(d, mode).is_ok_and(|id| id == BuiltinId::LpVmGetFuel)
    })
}

/// True if any function calls the inline-lowered `__lp_get_fuel` builtin —
/// the emitted load touches linear memory, so the module needs `env.memory`
/// even when nothing else does.
pub(crate) fn module_inlines_get_fuel(ir: &LpirModule, mode: FloatMode) -> bool {
    for f in ir.functions.values() {
        for op in &f.body {
            if let LpirOp::Call {
                callee: CalleeRef::Import(ImportId(i)),
                ..
            } = op
            {
                if import_is_inline_get_fuel(ir, *i as usize, mode) {
                    return true;
                }
            }
        }
    }
    false
}

/// LPIR ops whose Q32 lowering resolves to a builtin import call at emit time:
/// `Fsqrt` → `@lpir::sqrt`, `Fnearest` → `@glsl::round`.
fn scan_helper_op_needs(ir: &LpirModule) -> (bool, bool) {
    let mut needs_lpir_sqrt = false;
    let mut needs_glsl_round = false;
    for f in ir.functions.values() {
        for op in &f.body {
            match op {
                LpirOp::Fsqrt { .. } => needs_lpir_sqrt = true,
                LpirOp::Fnearest { .. } => needs_glsl_round = true,
                _ => {}
            }
        }
    }
    (needs_lpir_sqrt, needs_glsl_round)
}

/// The naga-based `lps-frontend` pre-registers the helper imports Q32 op
/// lowering calls, but `lps-glsl` emits `Fsqrt`/`Fnearest` directly without
/// declaring them. Returns a copy of `ir` with the missing helper import
/// decls appended, or `None` when `ir` already has everything emission needs.
pub(crate) fn with_missing_helper_imports(ir: &LpirModule, mode: FloatMode) -> Option<LpirModule> {
    if mode != FloatMode::Q32 {
        return None;
    }
    let (needs_lpir_sqrt, needs_glsl_round) = scan_helper_op_needs(ir);
    let mut missing = Vec::new();
    if needs_lpir_sqrt && find_import_index(ir, "lpir", "sqrt").is_none() {
        missing.push(helper_import_decl("lpir", "sqrt"));
    }
    if needs_glsl_round && find_import_index(ir, "glsl", "round").is_none() {
        missing.push(helper_import_decl("glsl", "round"));
    }
    if missing.is_empty() {
        return None;
    }
    let mut out = ir.clone();
    out.imports.extend(missing);
    Some(out)
}

/// `f32 -> f32` helper import decl (the shape of both Q32 helper builtins).
fn helper_import_decl(module: &str, func_name: &str) -> ImportDecl {
    ImportDecl {
        module_name: String::from(module),
        func_name: String::from(func_name),
        param_types: vec![IrType::F32],
        return_types: vec![IrType::F32],
        lpfn_glsl_params: None,
        needs_vmctx: false,
        sret: false,
    }
}

fn find_import_index(ir: &LpirModule, module: &str, func_name: &str) -> Option<u32> {
    ir.imports
        .iter()
        .enumerate()
        .find(|(_, d)| d.module_name == module && d.func_name == func_name)
        .map(|(i, _)| i as u32)
}

fn ir_params_to_glsl_kinds(params: &[IrType]) -> Vec<GlslParamKind> {
    params
        .iter()
        .map(|t| match t {
            IrType::F32 => GlslParamKind::Float,
            IrType::I32 | IrType::Pointer => GlslParamKind::UInt,
        })
        .collect()
}

fn lpfn_glsl_kinds_from_decl(decl: &ImportDecl) -> Result<Vec<GlslParamKind>, String> {
    if let Some(ref enc) = decl.lpfn_glsl_params {
        parse_lpfn_glsl_params_csv(enc)
    } else {
        Ok(ir_params_to_glsl_kinds(&decl.param_types))
    }
}

fn parse_lpfn_glsl_params_csv(enc: &str) -> Result<Vec<GlslParamKind>, String> {
    if enc.is_empty() {
        return Ok(Vec::new());
    }
    enc.split(',')
        .map(|t| match t.trim() {
            "Float" => Ok(GlslParamKind::Float),
            "Int" => Ok(GlslParamKind::Int),
            "UInt" => Ok(GlslParamKind::UInt),
            "Vec2" => Ok(GlslParamKind::Vec2),
            "Vec3" => Ok(GlslParamKind::Vec3),
            "Vec4" => Ok(GlslParamKind::Vec4),
            "IVec2" => Ok(GlslParamKind::IVec2),
            "IVec3" => Ok(GlslParamKind::IVec3),
            "IVec4" => Ok(GlslParamKind::IVec4),
            "UVec2" => Ok(GlslParamKind::UVec2),
            "UVec3" => Ok(GlslParamKind::UVec3),
            "UVec4" => Ok(GlslParamKind::UVec4),
            "BVec2" => Ok(GlslParamKind::BVec2),
            "BVec3" => Ok(GlslParamKind::BVec3),
            "BVec4" => Ok(GlslParamKind::BVec4),
            other => Err(format!("unknown LPFX glsl param tag `{other}`")),
        })
        .collect()
}

/// `FloatMode` → the builtin table's own mode enum.
fn builtin_mode(mode: FloatMode) -> Mode {
    match mode {
        FloatMode::Q32 => Mode::Q32,
        FloatMode::F32 => Mode::F32,
    }
}

/// Resolve a builtin import for `mode`.
///
/// **The mode is not advisory.** Every resolver is mode-total: in Float mode
/// only f32 (and genuinely mode-independent) ids resolve, and there is no
/// fallback to the Q32 id. That fallback is the bug this signature exists to
/// make impossible, and it has two distinct failure shapes:
///
/// 1. *Loud.* A float-typed import declared `(i32) -> i32` against call sites
///    pushing `f32` locals produces an invalid module whose only symptom is
///    wasmtime's `failed to compile: wasm[0]::function[N]` — 41 corpus files,
///    one indistinguishable error.
/// 2. *Silent, and worse.* A builtin taking a vector receives an
///    `IrType::Pointer`. Both sides are `i32`, so the module validates, links,
///    and runs — while a Q32 builtin reinterprets the f32 bit patterns in
///    shader memory as Q16.16. Nine corpus files "passed" through exactly this
///    hole during the wasm f32 bring-up.
///
/// Shape 2 is why the rule is stated as a property of the *resolver* rather
/// than a check on the signature: no inspection of the ABI can tell you a
/// pointer is safe. See `docs/design/float.md` §6.
fn resolve_builtin_id_for_mode(decl: &ImportDecl, mode: FloatMode) -> Result<BuiltinId, String> {
    let m = builtin_mode(mode);
    let unsupported = |detail: alloc::string::String| {
        format!(
            "builtin import `@{}::{}` has no {} implementation ({detail})",
            decl.module_name,
            decl.func_name,
            match mode {
                FloatMode::Q32 => "q32",
                FloatMode::F32 => "f32",
            }
        )
    };

    match decl.module_name.as_str() {
        "glsl" => {
            let ac = decl.param_types.len();
            glsl_math_builtin_id(decl.func_name.as_str(), ac, m)
                .ok_or_else(|| unsupported(format!("arg count {ac}")))
        }
        "lpir" => {
            let ac = decl.param_types.len();
            lpir_builtin_id(decl.func_name.as_str(), ac, m)
                .ok_or_else(|| unsupported(format!("arg count {ac}")))
        }
        "lpfn" => {
            let base = lpfn_strip_suffix(&decl.func_name)?;
            let kinds = lpfn_glsl_kinds_from_decl(decl)?;
            glsl_lpfn_builtin_id(base, &kinds, m).ok_or_else(|| unsupported(format!("{kinds:?}")))
        }
        "vm" => {
            let ac = decl.param_types.len();
            vm_builtin_id(decl.func_name.as_str(), ac, m)
                .ok_or_else(|| unsupported(format!("arg count {ac}")))
        }
        "texture" => {
            let base = texture_strip_suffix(&decl.func_name)?;
            let ac = decl.param_types.len();
            texture_builtin_id(base, ac, m).ok_or_else(|| unsupported(format!("arg count {ac}")))
        }
        m => Err(format!("unsupported import module `{m}`")),
    }
}

/// `lpfn_saturate_3` → `lpfn_saturate`.
fn lpfn_strip_suffix(func_name: &str) -> Result<&str, String> {
    strip_trailing_numeric_import_suffix(func_name, "lpfn")
}

fn texture_strip_suffix(func_name: &str) -> Result<&str, String> {
    Ok(strip_optional_numeric_import_suffix(func_name))
}

fn strip_trailing_numeric_import_suffix<'a>(
    func_name: &'a str,
    module: &str,
) -> Result<&'a str, String> {
    let (base, tail) = func_name
        .rsplit_once('_')
        .ok_or_else(|| format!("malformed {module} import name `{func_name}`"))?;
    tail.parse::<u32>()
        .map_err(|_| format!("malformed {module} import name `{func_name}`"))?;
    Ok(base)
}

fn strip_optional_numeric_import_suffix(func_name: &str) -> &str {
    let Some((base, tail)) = func_name.rsplit_once('_') else {
        return func_name;
    };
    if tail.parse::<u32>().is_ok() {
        base
    } else {
        func_name
    }
}

pub(crate) fn build_filtered_imports(
    ir: &LpirModule,
    mode: FloatMode,
) -> Result<FilteredImports, String> {
    let used = collect_used_import_indices(ir, mode);
    let mut remap = vec![None; ir.imports.len()];
    let mut decls = Vec::new();
    let mut next_wasm = 0u32;
    for (i, decl) in ir.imports.iter().enumerate() {
        if !used.contains(&(i as u32)) {
            continue;
        }
        let _ = resolve_builtin_id_for_mode(decl, mode)?;
        remap[i] = Some(next_wasm);
        decls.push(decl.clone());
        next_wasm += 1;
    }
    Ok(FilteredImports { decls, remap })
}

/// True if any user function calls an import that uses the result-pointer WASM ABI
/// (non-scalar return via hidden pointer; callee has zero WASM results).
pub(crate) fn module_needs_result_ptr_calls(ir: &LpirModule, mode: FloatMode) -> bool {
    let n = ir.imports.len() as u32;
    for f in ir.functions.values() {
        for op in &f.body {
            if let LpirOp::Call { callee, .. } = op {
                if let CalleeRef::Import(ImportId(i)) = callee {
                    if (*i as u32) < n && import_uses_result_pointer_abi(ir, *i as usize, mode) {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// Whether `ir.imports[import_idx]` uses result-pointer calling convention in WASM.
///
/// Mode-dependent by nature: the q32 and f32 builtins behind one import name
/// are separate symbols with separately generated wasm signatures, so which one
/// returns via a hidden pointer is a per-mode fact.
pub(crate) fn import_uses_result_pointer_abi(
    ir: &LpirModule,
    import_idx: usize,
    mode: FloatMode,
) -> bool {
    let decl = match ir.imports.get(import_idx) {
        Some(d) => d,
        None => return false,
    };
    if decl.return_types.is_empty() {
        return false;
    }
    let Ok(bid) = resolve_builtin_id_for_mode(decl, mode) else {
        return false;
    };
    let (_params, wasm_results) = super::builtin_wasm_import_types::wasm_import_val_types(bid);
    wasm_results.is_empty()
}

/// Max byte size of temporary result buffer needed for result-pointer builtin calls in `f`.
pub(crate) fn max_result_ptr_buffer_bytes(ir: &LpirModule, f: &IrFunction, mode: FloatMode) -> u32 {
    let n = ir.imports.len() as u32;
    let mut max_b = 0u32;
    for op in &f.body {
        if let LpirOp::Call {
            callee, results, ..
        } = op
        {
            if let CalleeRef::Import(ImportId(i)) = callee {
                if (*i as u32) < n && import_uses_result_pointer_abi(ir, *i as usize, mode) {
                    let count = f.pool_slice(*results).len() as u32;
                    max_b = max_b.max(count.saturating_mul(4));
                }
            }
        }
    }
    max_b
}

pub(crate) fn import_decl_val_types(
    decl: &ImportDecl,
    mode: FloatMode,
) -> Result<(Vec<wasm_encoder::ValType>, Vec<wasm_encoder::ValType>), String> {
    let bid = resolve_builtin_id_for_mode(decl, mode)?;
    Ok(super::builtin_wasm_import_types::wasm_import_val_types(bid))
}

pub(crate) fn builtins_wasm_name(
    decl: &ImportDecl,
    mode: FloatMode,
) -> Result<&'static str, String> {
    Ok(resolve_builtin_id_for_mode(decl, mode)?.name())
}

pub(crate) fn import_callee(
    ir: &LpirModule,
    module: &str,
    func_name: &str,
) -> Result<CalleeRef, String> {
    ir.imports
        .iter()
        .enumerate()
        .find(|(_, d)| d.module_name == module && d.func_name == func_name)
        .map(|(i, _)| CalleeRef::Import(ImportId(i as u16)))
        .ok_or_else(|| format!("missing import @{module}::{func_name}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    use lps_builtin_ids::Mode as BMode;

    #[test]
    fn q32_mode_resolves_a_float_import() {
        let d = decl("glsl", "sin", &[IrType::F32], &[IrType::F32]);
        let id = resolve_builtin_id_for_mode(&d, FloatMode::Q32).unwrap();
        assert_eq!(id.mode(), Some(BMode::Q32));
    }

    /// The f32 half of the same import. Before f32 builtins existed this was a
    /// refusal; the property being defended never changed — an f32 module must
    /// not import a Q32-signature builtin — only the remedy did, from "fail"
    /// to "resolve to the f32 symbol".
    #[test]
    fn f32_mode_resolves_a_float_import_to_the_f32_builtin() {
        let d = lpfn_decl(
            "lpfn_random_2",
            "Vec2,UInt",
            &[IrType::F32; 3],
            &[IrType::F32],
        );
        let id = resolve_builtin_id_for_mode(&d, FloatMode::F32).unwrap();
        assert_eq!(id.mode(), Some(BMode::F32));
        assert!(id.name().ends_with("_f32"), "{}", id.name());
    }

    /// **The load-bearing one.** A pointer parameter is an `i32` on both sides,
    /// so nothing in the type system distinguishes the two modes here: the
    /// pointee is shader memory the builtin decodes with its own float
    /// encoding, and a Q32 builtin reading an f32 module's memory reinterprets
    /// f32 bits as Q16.16 and returns garbage with no error anywhere. Wasm
    /// validation cannot catch it; nine corpus files "passed" through this hole
    /// during the wasm f32 bring-up.
    ///
    /// So the invariant is about the *resolver*, not the signature: a
    /// pointer-carrying import must still come back as the f32 symbol.
    #[test]
    fn f32_mode_resolves_pointer_params_to_the_f32_builtin() {
        // `lpfn_saturate(vec3)` returns through a result pointer: the wasm ABI
        // is `(i32 out, f32, f32, f32) -> ()`, and the out-pointer is the lane
        // a Q32 builtin would fill with Q16.16 words.
        let d = lpfn_decl(
            "lpfn_saturate_3",
            "Vec3",
            &[IrType::Pointer, IrType::F32, IrType::F32, IrType::F32],
            &[IrType::F32],
        );
        let id = resolve_builtin_id_for_mode(&d, FloatMode::F32).unwrap();
        assert_eq!(
            id.mode(),
            Some(BMode::F32),
            "a pointer-carrying import resolved to {id:?} in f32 mode — the \
             pointee would be reinterpreted as Q16.16"
        );
    }

    /// Generalization of the above over every import this emitter can see:
    /// nothing, in either direction, ever resolves across modes.
    #[test]
    fn no_import_ever_resolves_to_the_other_modes_builtin() {
        let cases = [
            decl("glsl", "sin", &[IrType::F32], &[IrType::F32]),
            decl("glsl", "pow", &[IrType::F32, IrType::F32], &[IrType::F32]),
            decl("lpir", "sqrt", &[IrType::F32], &[IrType::F32]),
            decl("vm", "__lp_get_fuel", &[], &[IrType::I32]),
            lpfn_decl(
                "lpfn_saturate_3",
                "Vec3",
                &[IrType::Pointer, IrType::F32, IrType::F32, IrType::F32],
                &[IrType::F32],
            ),
            lpfn_decl(
                "lpfn_random_2",
                "Vec2,UInt",
                &[IrType::F32; 3],
                &[IrType::F32],
            ),
        ];
        for d in &cases {
            for (mode, forbidden) in [(FloatMode::Q32, BMode::F32), (FloatMode::F32, BMode::Q32)] {
                if let Ok(id) = resolve_builtin_id_for_mode(d, mode) {
                    assert_ne!(
                        id.mode(),
                        Some(forbidden),
                        "@{}::{} resolved to {id:?} under {mode:?}",
                        d.module_name,
                        d.func_name
                    );
                }
            }
        }
    }

    /// Integer-only builtins carry no float representation either way, so one
    /// implementation serves both modes. `__lp_get_fuel` is `(i32) -> u32`; it
    /// used to be named `*_q32`, which made the f32 resolver miss it entirely
    /// and would have dropped fuel metering from every Float-mode shader.
    #[test]
    fn mode_independent_imports_resolve_in_both_modes() {
        let d = decl("vm", "__lp_get_fuel", &[], &[IrType::I32]);
        let q = resolve_builtin_id_for_mode(&d, FloatMode::Q32).unwrap();
        let f = resolve_builtin_id_for_mode(&d, FloatMode::F32).unwrap();
        assert_eq!(q, f);
        assert_eq!(q, BuiltinId::LpVmGetFuel);
        assert_eq!(q.mode(), None);
    }

    /// A name with no implementation still fails by name — the difference
    /// between a triage-able corpus and N identical opaque wasm errors.
    #[test]
    fn an_unknown_import_errors_by_name_rather_than_resolving() {
        let d = decl("glsl", "frexp", &[IrType::F32], &[IrType::F32]);
        let err = resolve_builtin_id_for_mode(&d, FloatMode::F32).unwrap_err();
        assert!(err.contains("@glsl::frexp"), "{err}");
        assert!(err.contains("no f32 implementation"), "{err}");
    }

    fn decl(module: &str, func: &str, params: &[IrType], returns: &[IrType]) -> ImportDecl {
        ImportDecl {
            module_name: module.to_string(),
            func_name: func.to_string(),
            param_types: params.to_vec(),
            return_types: returns.to_vec(),
            lpfn_glsl_params: None,
            needs_vmctx: false,
            sret: false,
        }
    }

    /// An `@lpfn::` import. Overload resolution is keyed by the *GLSL*
    /// parameter list, not the flattened wasm one, so the encoded kinds carry
    /// the real signature.
    fn lpfn_decl(
        func: &str,
        glsl_params: &str,
        params: &[IrType],
        returns: &[IrType],
    ) -> ImportDecl {
        let mut d = decl("lpfn", func, params, returns);
        d.lpfn_glsl_params = Some(glsl_params.to_string());
        d
    }
}
