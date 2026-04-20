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

/// Generate `lpvm-wasm/src/rt_wasmtime/native_builtin_dispatch.rs`.
pub(crate) fn generate_native_wasmtime_dispatch(path: &Path, builtins: &[BuiltinInfo]) {
    for b in builtins {
        if b.rust_signature.contains('*') && guest_pointer_out_plan(&b.function_name).is_none() {
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
        if b.symbol_name == "__lp_vm_get_fuel_q32" {
            body.push_str(emit_get_fuel_arm());
            body.push('\n');
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
