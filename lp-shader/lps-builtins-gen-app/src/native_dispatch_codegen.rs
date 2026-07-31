//! Generates `lpvm-wasm` wasmtime host dispatch for `lps-builtins` (`native_builtin_dispatch.rs`).

use std::fs;
use std::path::Path;

use crate::{BuiltinInfo, parse_rust_extern_sig};

/// For each `extern "C"` builtin that takes `*mut` guest pointers, how many scalars are written.
fn guest_pointer_out_plan(fn_name: &str) -> Option<Vec<(usize, usize, &'static str)>> {
    match fn_name {
        "__lp_lpfn_hsv2rgb_f32" => Some(vec![(0, 3, "f32")]),
        "__lp_lpfn_hsv2rgb_q32" => Some(vec![(0, 3, "i32")]),
        "__lp_lpfn_hsv2rgb_vec4_f32" => Some(vec![(0, 4, "f32")]),
        "__lp_lpfn_hsv2rgb_vec4_q32" => Some(vec![(0, 4, "i32")]),
        "__lp_lpfn_hue2rgb_f32" => Some(vec![(0, 3, "f32")]),
        "__lp_lpfn_hue2rgb_q32" => Some(vec![(0, 3, "i32")]),
        "__lp_lpfn_rgb2hsv_f32" => Some(vec![(0, 3, "f32")]),
        "__lp_lpfn_rgb2hsv_q32" => Some(vec![(0, 3, "i32")]),
        "__lp_lpfn_rgb2hsv_vec4_f32" => Some(vec![(0, 4, "f32")]),
        "__lp_lpfn_rgb2hsv_vec4_q32" => Some(vec![(0, 4, "i32")]),
        "__lp_lpfn_saturate_vec3_f32" => Some(vec![(0, 3, "f32")]),
        "__lp_lpfn_saturate_vec3_q32" => Some(vec![(0, 3, "i32")]),
        "__lp_lpfn_saturate_vec4_f32" => Some(vec![(0, 4, "f32")]),
        "__lp_lpfn_saturate_vec4_q32" => Some(vec![(0, 4, "i32")]),
        "__lp_lpfn_srandom3_vec_f32" => Some(vec![(0, 3, "f32")]),
        "__lp_lpfn_srandom3_vec_q32" => Some(vec![(0, 3, "i32")]),
        "__lp_lpfn_srandom3_tile_f32" => Some(vec![(0, 3, "f32")]),
        "__lp_lpfn_srandom3_tile_q32" => Some(vec![(0, 3, "i32")]),
        "__lp_lpfn_psrdnoise2_f32" => Some(vec![(5, 2, "f32")]),
        "__lp_lpfn_psrdnoise2_q32" => Some(vec![(5, 2, "i32")]),
        "__lp_lpfn_psrdnoise3_f32" => Some(vec![(7, 3, "f32")]),
        "__lp_lpfn_psrdnoise3_q32" => Some(vec![(7, 3, "i32")]),
        "__lp_texture1d_r16_unorm_q32" => Some(vec![(0, 4, "i32")]),
        "__lp_texture1d_rgba16_unorm_q32" => Some(vec![(0, 4, "i32")]),
        "__lp_texture2d_r16_unorm_q32" => Some(vec![(0, 4, "i32")]),
        "__lp_texture2d_rgba16_unorm_q32" => Some(vec![(0, 4, "i32")]),
        "__lps_sincos_f32" => Some(vec![(1, 1, "f32"), (2, 1, "f32")]),
        "__lps_sincos_q32" => Some(vec![(1, 1, "i32"), (2, 1, "i32")]),
        _ => None,
    }
}

fn emit_param_load_line(i: usize, t: &str) -> String {
    let t = t.trim();
    match t {
        "i32" | "i8" | "i16" | "i64" | "isize" => {
            format!("let p{i} = params[{i}].unwrap_i32();")
        }
        "u32" | "u8" | "u16" | "u64" | "usize" => {
            format!("let p{i} = params[{i}].unwrap_i32() as u32;")
        }
        "bool" => format!("let p{i} = params[{i}].unwrap_i32() != 0;"),
        "f32" => format!("let p{i} = params[{i}].unwrap_f32();"),
        _ if t.contains('*') => panic!("emit_param_load_line: unexpected pointer type {t}"),
        _ => panic!("emit_param_load_line: unsupported param type `{t}`"),
    }
}

fn emit_direct_arm(b: &BuiltinInfo) -> String {
    let (pts, ret) = parse_rust_extern_sig(&b.rust_signature);
    let call = format!(
        "lps_builtins::builtins::{}::{}",
        b.module_path, b.function_name
    );
    let mut s = String::new();
    for (i, t) in pts.iter().enumerate() {
        if t.contains('*') {
            panic!(
                "direct dispatch arm: unexpected pointer in {} ({})",
                b.function_name, t
            );
        }
        s.push_str("            ");
        s.push_str(&emit_param_load_line(i, t));
        s.push('\n');
    }
    let args: Vec<String> = (0..pts.len()).map(|i| format!("p{i}")).collect();
    let args_j = args.join(", ");
    let ret = ret.trim();
    match ret {
        "()" => {
            s.push_str(&format!("            {call}({args_j});\n"));
        }
        "i32" | "i8" | "i16" | "i64" | "isize" => {
            s.push_str(&format!("            let r = {call}({args_j});\n"));
            s.push_str("            results[0] = wasmtime::Val::I32(r);\n");
        }
        "u32" | "u8" | "u16" | "u64" | "usize" | "bool" => {
            s.push_str(&format!("            let r = {call}({args_j});\n"));
            s.push_str("            results[0] = wasmtime::Val::I32(r as i32);\n");
        }
        "f32" => {
            s.push_str(&format!("            let r = {call}({args_j});\n"));
            s.push_str("            results[0] = wasmtime::Val::F32(r.to_bits());\n");
        }
        o => panic!(
            "direct dispatch: unsupported return `{o}` for {}",
            b.function_name
        ),
    }
    s.push_str("            Ok(())\n");
    s
}

fn emit_pointer_arm(b: &BuiltinInfo, plan: &[(usize, usize, &'static str)]) -> String {
    let (pts, ret) = parse_rust_extern_sig(&b.rust_signature);
    let call = format!(
        "lps_builtins::builtins::{}::{}",
        b.module_path, b.function_name
    );
    let mut s = String::new();
    s.push_str("            let mem = linked_env_memory;\n");
    for (idx, count, elem_ty) in plan {
        s.push_str(&format!(
            "            let off_{idx} = params[{idx}].unwrap_i32() as u32 as usize;\n"
        ));
        let z = match *elem_ty {
            "f32" => "0f32",
            "i32" => "0i32",
            other => panic!("pointer arm: bad elem {other}"),
        };
        s.push_str(&format!(
            "            let mut buf_{idx} = [{z}; {count}];\n"
        ));
    }
    for (i, t) in pts.iter().enumerate() {
        if plan.iter().any(|(idx, _, _)| *idx == i) {
            continue;
        }
        if t.contains('*') {
            panic!("pointer arm: extra pointer param in {}", b.function_name);
        }
        s.push_str("            ");
        s.push_str(&emit_param_load_line(i, t));
        s.push('\n');
    }
    let mut args = Vec::new();
    for i in 0..pts.len() {
        if plan.iter().any(|(idx, _, _)| *idx == i) {
            args.push(format!("buf_{i}.as_mut_ptr()"));
        } else {
            args.push(format!("p{i}"));
        }
    }
    let args_j = args.join(", ");
    let ret = ret.trim();
    match ret {
        "()" => {
            s.push_str(&format!("            {call}({args_j});\n"));
        }
        "i32" | "i8" | "i16" | "i64" | "isize" => {
            s.push_str(&format!("            let r = {call}({args_j});\n"));
            s.push_str("            results[0] = wasmtime::Val::I32(r);\n");
        }
        "u32" | "u8" | "u16" | "u64" | "usize" | "bool" => {
            s.push_str(&format!("            let r = {call}({args_j});\n"));
            s.push_str("            results[0] = wasmtime::Val::I32(r as i32);\n");
        }
        "f32" => {
            s.push_str(&format!("            let r = {call}({args_j});\n"));
            s.push_str("            results[0] = wasmtime::Val::F32(r.to_bits());\n");
        }
        o => panic!(
            "pointer dispatch: unsupported return `{o}` for {}",
            b.function_name
        ),
    }
    for (idx, _, elem_ty) in plan {
        match *elem_ty {
            "f32" | "i32" => {
                s.push_str(&format!(
                    "            for (i, v) in buf_{idx}.iter().enumerate() {{\n\
                                    mem.write(&mut caller, off_{idx} + i * 4, &v.to_le_bytes())\n\
                                        .map_err(|e| wasmtime::Error::msg(format!(\"builtin write-back: {{e}}\")))?;\n\
                                }}\n"
                ));
            }
            other => panic!("bad elem {other}"),
        }
    }
    s.push_str("            Ok(())\n");
    s
}

fn emit_get_fuel_arm() -> &'static str {
    r#"            let vmctx_word = params[0].unwrap_i32();
            let mem = linked_env_memory;
            let base = vmctx_word as u32 as usize;
            let mut buf = [0u8; 8];
            mem.read(&caller, base, &mut buf)
                .map_err(|e| wasmtime::Error::msg(format!("vmctx fuel read: {e}")))?;
            let fuel = u64::from_le_bytes(buf);
            results[0] = wasmtime::Val::I32(fuel as u32 as i32);
            Ok(())"#
}

/// Texture builtins pass descriptor `ptr` as a **guest linear-memory byte offset**. Wasmtime dispatch
/// must translate that offset through [`wasmtime::Memory::data`] before sampling — unlike native JIT,
/// where `ptr` may be used as an address-sized token compatible with `ptr as *const u8`.
/// Texture samplers take the texture as a **guest offset**, not a host pointer,
/// so their host-side dispatch has to translate it through Wasmtime's linear
/// memory before calling the sampler. That is the one thing the generic
/// pointer-arm emitter cannot do, which is why these get their own body.
fn is_texture_guest_offset_builtin(symbol_name: &str) -> bool {
    parse_texture_symbol(symbol_name).is_some()
}

/// `__lp_texture{1,2}d_{r16,rgba16}_unorm_{q32,f32}` -> (dims, format, mode).
///
/// Parsed rather than enumerated: the family is eight symbols across two float
/// modes and every one of them wants the same twenty lines with three tokens
/// swapped. It was already four near-identical copies before f32 doubled it.
fn parse_texture_symbol(symbol_name: &str) -> Option<(u32, &'static str, &'static str)> {
    let rest = symbol_name.strip_prefix("__lp_texture")?;
    let (dims, rest) = match rest.strip_prefix("1d_") {
        Some(r) => (1u32, r),
        None => (2u32, rest.strip_prefix("2d_")?),
    };
    let (format, rest) = match rest.strip_prefix("rgba16_unorm") {
        Some(r) => ("rgba16", r),
        None => ("r16", rest.strip_prefix("r16_unorm")?),
    };
    let mode = match rest {
        "_q32" => "q32",
        "_f32" => "f32",
        _ => return None,
    };
    Some((dims, format, mode))
}

fn emit_texture_guest_dispatch(symbol_name: &str) -> String {
    let (dims, format, mode) = parse_texture_symbol(symbol_name).unwrap_or_else(|| {
        panic!("emit_texture_guest_dispatch: unknown texture builtin `{symbol_name}`")
    });

    // Coordinate lanes are Q16.16 words in q32 and real floats in f32; every
    // other parameter is an integer descriptor lane in both modes.
    let coord = |i: usize| -> String {
        match mode {
            "f32" => format!("            let p{i} = params[{i}].unwrap_f32();\n"),
            _ => format!("            let p{i} = params[{i}].unwrap_i32();\n"),
        }
    };
    let u32_lane = |i: usize| format!("            let p{i} = params[{i}].unwrap_i32() as u32;\n");

    let args_suffix = if mode == "f32" { "F32" } else { "" };
    let fn_suffix = if mode == "f32" { "_f32" } else { "" };
    let module = format!("{format}_unorm_{mode}");

    let mut s = String::new();
    s.push_str("            let mem = linked_env_memory;\n");
    s.push_str("            let off_0 = params[0].unwrap_i32() as u32 as usize;\n");
    s.push_str("            let tex_guest_off = params[1].unwrap_i32() as u32 as usize;\n");
    s.push_str(
        "            let tex_base_host = mem.data(&caller).as_ptr().wrapping_add(tex_guest_off);\n",
    );

    if dims == 2 {
        s.push_str(&u32_lane(2));
        s.push_str(&u32_lane(3));
        s.push_str(&u32_lane(4));
        s.push_str(&coord(5));
        s.push_str(&coord(6));
        s.push_str(&u32_lane(7));
        s.push_str(&u32_lane(8));
        s.push_str(&u32_lane(9));
        s.push_str(&format!(
            "            let args = lps_builtins::builtins::texture::Texture2dUnormSampleArgs{args_suffix} {{\n\
                            width: p2,\n\
                            height: p3,\n\
                            row_stride: p4,\n\
                            u: p5,\n\
                            v: p6,\n\
                            filter_abi: p7,\n\
                            wrap_x_abi: p8,\n\
                            wrap_y_abi: p9,\n\
                        }};\n"
        ));
    } else {
        s.push_str(&u32_lane(2));
        s.push_str(&u32_lane(3));
        s.push_str(&coord(4));
        s.push_str(&u32_lane(5));
        s.push_str(&u32_lane(6));
        s.push_str(&format!(
            "            let args = lps_builtins::builtins::texture::Texture1dUnormSampleArgs{args_suffix} {{\n\
                            width: p2,\n\
                            row_stride: p3,\n\
                            u: p4,\n\
                            filter_abi: p5,\n\
                            wrap_x_abi: p6,\n\
                        }};\n"
        ));
    }

    s.push_str(&format!(
        "            let lanes = unsafe {{\n\
                        // SAFETY: guest `ptr` translated through Wasmtime linear memory; bounds match descriptor lanes.\n\
                        lps_builtins::builtins::texture::{module}::texture{dims}d_{format}_unorm_sample{fn_suffix}(tex_base_host, args)\n\
                    }};\n"
    ));
    s.push_str(
        "            for (i, v) in lanes.iter().enumerate() {\n\
                        mem.write(&mut caller, off_0 + i * 4, &v.to_le_bytes())\n\
                            .map_err(|e| wasmtime::Error::msg(format!(\"builtin write-back: {e}\")))?;\n\
                    }\n\
                    Ok(())\n",
    )
    ;
    s
}

/// Generate `lpvm-wasm/src/rt_wasmtime/native_builtin_dispatch.rs`.
pub(crate) fn generate_native_wasmtime_dispatch(path: &Path, builtins: &[BuiltinInfo]) {
    for b in builtins {
        // Texture samplers have their own dispatch body (guest-offset
        // translation), so they legitimately have no generic pointer plan.
        if b.rust_signature.contains('*')
            && guest_pointer_out_plan(&b.function_name).is_none()
            && !is_texture_guest_offset_builtin(&b.function_name)
        {
            panic!(
                "native wasmtime dispatch: add guest_pointer_out_plan entry for {}",
                b.function_name
            );
        }
    }

    let mut sorted: Vec<&BuiltinInfo> = builtins.iter().collect();
    sorted.sort_by(|a, b| a.enum_variant.cmp(&b.enum_variant));

    let header = r#"//! wasmtime host dispatch into `lps-builtins` (guest linear memory for pointer ABI).
//!
//! AUTO-GENERATED by lps-builtins-gen-app. Do not edit manually.
//!
//! Regenerate: `cargo run -p lps-builtins-gen-app` or `scripts/build-builtins.sh`

use wasmtime::{Caller, Memory, Val};

use lps_builtin_ids::BuiltinId;

/// Linear memory handle supplied at link time (`env.memory` from [`super::link`]).
///
/// `Caller::get_export` only sees WASM **exports**; our shaders **import** `env.memory`, so there is
/// no `"env"` export to discover. Always use the memory handle wired into the linker.
pub(super) fn dispatch_native_builtin(
    mut caller: Caller<'_, ()>,
    linked_env_memory: Memory,
    id: BuiltinId,
    params: &[Val],
    results: &mut [Val],
) -> Result<(), wasmtime::Error> {
    match id {
"#;

    let mut body = String::from(header);
    for b in sorted {
        body.push_str(&format!("        BuiltinId::{} => {{\n", b.enum_variant));
        if b.symbol_name == "__lp_vm_get_fuel" {
            body.push_str(emit_get_fuel_arm());
            body.push('\n');
        } else if is_texture_guest_offset_builtin(&b.symbol_name) {
            body.push_str(&emit_texture_guest_dispatch(&b.symbol_name));
        } else if let Some(ref plan) = guest_pointer_out_plan(&b.function_name) {
            body.push_str(&emit_pointer_arm(b, plan));
        } else {
            body.push_str(&emit_direct_arm(b));
        }
        body.push_str("        }\n");
    }
    body.push_str("    }\n}\n");

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create native_builtin_dispatch parent dir");
    }
    fs::write(path, body).expect("write native_builtin_dispatch.rs");
}

#[cfg(test)]
mod tests {
    use super::emit_texture_guest_dispatch;

    #[test]
    fn wasmtime_texture_dispatch_uses_linear_memory_base_and_public_sampler() {
        let code = emit_texture_guest_dispatch("__lp_texture2d_rgba16_unorm_q32");
        assert!(
            code.contains("mem.data(&caller)"),
            "expected wasmtime Memory::data for guest offset translation:\n{code}"
        );
        assert!(
            code.contains("wrapping_add(tex_guest_off)"),
            "expected host base = linear_memory.as_ptr() + guest offset:\n{code}"
        );
        assert!(
            code.contains("texture2d_rgba16_unorm_sample"),
            "expected Rust sampler helper, not extern `__lp_texture*`:\n{code}"
        );
        assert!(
            !code.contains("__lp_texture2d_rgba16_unorm_q32"),
            "must not call extern that casts guest offset to host pointer:\n{code}"
        );
    }
}
