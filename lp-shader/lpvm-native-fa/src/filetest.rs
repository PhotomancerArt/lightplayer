//! Allocator filetest logic (`no_std` + alloc).
//!
//! Parsing and snapshot comparison run in the library. The integration test binary
//! (`tests/filetests.rs`) handles filesystem discovery and BLESS updates.

use crate::fa_alloc::pool::RegPool;
use crate::fa_alloc::render::render_interleaved;
use crate::fa_alloc::verify::verify_alloc;
use crate::lower::lower_ops;
use crate::region::Region;
use crate::rv32::abi;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use lpir::FloatMode;
use lpir::parse_module;

/// Snapshot delimiter between input LPIR and expected output (must match bless in `tests/filetests.rs`).
pub const FILETEST_SEPARATOR: &str = "; ==================================================================================================";

/// A parsed filetest.
pub struct FileTest {
    pub path: String,
    pub name: String,
    pub pool_size: Option<usize>,
    pub abi_params: usize,
    /// Return type for ABI: "void", "i32", "f32", "vec3", "vec4", "mat4" (default "void").
    pub abi_return: String,
    /// `import:` directive bodies, e.g. `helper(i32, i32) -> i32` → prepended as
    /// `import @filetest::helper(i32, i32) -> i32`.
    pub import_directives: Vec<String>,
    /// Raw LPIR input as string (for parsing)
    pub lpir_input: String,
    pub expected: String,
}

/// Parse a `.lpir` file.
pub fn parse_filetest(path: &str, content: &str) -> FileTest {
    let mut lines = content.lines().peekable();

    let mut pool_size: Option<usize> = None;
    let mut abi_params: usize = 0;
    let mut abi_return = String::from("void");
    let mut name: Option<String> = None;
    let mut import_directives: Vec<String> = Vec::new();

    while let Some(line) = lines.peek() {
        let t = line.trim_start();
        if t == FILETEST_SEPARATOR {
            break;
        }
        if t.starts_with("; ") {
            let directive = &t[2..]; // Strip "; "
            if let Some((key, value)) = directive.split_once(':') {
                let key = key.trim();
                let value = value.trim();
                match key {
                    "pool_size" => {
                        pool_size = value.parse().ok();
                    }
                    "abi_params" => {
                        abi_params = value.parse().unwrap_or(0);
                    }
                    "abi_return" => {
                        abi_return = value.to_string();
                    }
                    "name" => {
                        name = Some(value.to_string());
                    }
                    "import" => {
                        import_directives.push(value.to_string());
                    }
                    _ => {}
                }
            }
            lines.next();
        } else if t.is_empty() {
            lines.next();
        } else {
            break;
        }
    }

    let mut lpir_lines_for_input: Vec<&str> = Vec::new();

    for line in lines {
        if line.starts_with(FILETEST_SEPARATOR) {
            break;
        }
        if !line.trim().is_empty() {
            lpir_lines_for_input.push(line);
        }
    }

    while let Some(last) = lpir_lines_for_input.last() {
        if last.trim().is_empty() {
            lpir_lines_for_input.pop();
        } else {
            break;
        }
    }

    let lpir_input = lpir_lines_for_input.join("\n");

    let after_sep = content.splitn(2, FILETEST_SEPARATOR).nth(1).unwrap_or("");
    let after_sep = after_sep.trim_start_matches('\n');
    let expected = if after_sep.is_empty() {
        String::new()
    } else {
        format!("{FILETEST_SEPARATOR}\n{after_sep}")
    };
    let expected = expected.trim_end().to_string();

    FileTest {
        path: path.to_string(),
        name: name.unwrap_or_else(|| String::from("unnamed")),
        pool_size,
        abi_params,
        abi_return,
        import_directives,
        lpir_input,
        expected,
    }
}

/// Turn `helper(i32, i32) -> i32` into a full LPIR import line (`import @filetest::…`).
pub fn filetest_import_to_lpir_line(directive_body: &str) -> Result<String, String> {
    let s = directive_body.trim();
    let open = s
        .find('(')
        .ok_or_else(|| String::from("import: expected '(' after name"))?;
    let name = s[..open].trim();
    if name.is_empty() {
        return Err(String::from("import: empty callee name"));
    }
    let mut depth = 0i32;
    let mut close_rel: Option<usize> = None;
    for (i, c) in s[open..].char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    close_rel = Some(i);
                    break;
                }
            }
            _ => {}
        }
    }
    let close_rel = close_rel.ok_or_else(|| String::from("import: unclosed '('"))?;
    let close = open + close_rel;
    let params = &s[open..=close];
    let tail = s[close + 1..].trim();
    let ret = if let Some(r) = tail.strip_prefix("->") {
        r.trim()
    } else if tail.is_empty() {
        ""
    } else {
        return Err(alloc::format!(
            "import: expected '->' or end after ')', got {:?}",
            tail
        ));
    };
    if ret.is_empty() {
        Ok(alloc::format!("import @filetest::{}{}\n", name, params))
    } else {
        Ok(alloc::format!(
            "import @filetest::{}{} -> {}\n",
            name,
            params,
            ret
        ))
    }
}

/// Run allocation and return the snapshot text (including [`FILETEST_SEPARATOR`] as the first line).
pub fn compute_filetest_snapshot(test: &FileTest) -> Result<String, String> {
    use crate::abi::ModuleAbi;
    use lps_shared::LpsModuleSig;

    let mut lpir_full = String::new();
    for d in &test.import_directives {
        lpir_full.push_str(&filetest_import_to_lpir_line(d)?);
    }
    lpir_full.push_str(&test.lpir_input);

    let mut module =
        parse_module(&lpir_full).map_err(|e| format!("Failed to parse LPIR: {:?}", e))?;

    for imp in &mut module.imports {
        if imp.module_name == "filetest" {
            imp.needs_vmctx = true;
        }
    }

    let mut func = module
        .functions
        .first()
        .cloned()
        .ok_or_else(|| String::from("No functions in LPIR module"))?;

    // Ensure vreg_types covers ABI params (vmctx + user params) for rendering
    let total_abi_slots = 1 + test.abi_params; // vmctx + user params
    if func.vreg_types.len() < total_abi_slots {
        while func.vreg_types.len() < total_abi_slots {
            func.vreg_types.push(lpir::IrType::I32);
        }
    }
    let abi = ModuleAbi::from_ir_and_sig(
        &module,
        &LpsModuleSig {
            functions: Vec::new(),
        },
    );

    let lowered = lower_ops(&func, &module, &abi, FloatMode::Q32)
        .map_err(|e| format!("Failed to lower LPIR: {:?}", e))?;

    let vinsts = &lowered.vinsts;
    let vreg_pool = &lowered.vreg_pool;

    use lps_shared::{FnParam, LpsFnSig, LpsType, ParamQualifier};

    let return_type = match test.abi_return.as_str() {
        "i32" => LpsType::Int,
        "f32" => LpsType::Float,
        "vec3" => LpsType::Vec3,
        "vec4" => LpsType::Vec4,
        "mat4" => LpsType::Mat4,
        _ => LpsType::Void,
    };

    let params: Vec<FnParam> = (0..test.abi_params)
        .map(|i| FnParam {
            name: alloc::format!("arg{}", i),
            ty: LpsType::Int,
            qualifier: ParamQualifier::In,
        })
        .collect();

    let total_param_slots = 1 + test.abi_params; // vmctx + user params
    let func_abi = abi::func_abi_rv32(
        &LpsFnSig {
            name: String::from("test"),
            return_type,
            parameters: params,
        },
        total_param_slots,
    );

    use crate::fa_alloc::walk::walk_linear_with_pool;

    let pool = if let Some(n) = test.pool_size {
        RegPool::with_capacity(n)
    } else {
        RegPool::new()
    };

    let output = match &lowered.region_tree.nodes[lowered.region_tree.root as usize] {
        Region::Linear { .. } => walk_linear_with_pool(vinsts, vreg_pool, &func_abi, pool)
            .map_err(|e| format!("Allocation failed: {:?}", e))?,
        _ => {
            return Err(String::from(
                "Non-linear regions not yet supported in filetests",
            ));
        }
    };

    verify_alloc(vinsts, vreg_pool, &output, &func_abi);

    let rendered = render_interleaved(
        &func,
        &module,
        vinsts,
        vreg_pool,
        &output,
        &func_abi,
        &lowered.symbols,
    );

    let mut actual_lines = vec![FILETEST_SEPARATOR.to_string()];
    actual_lines.push(";".to_string());
    for line in rendered.lines() {
        if line.is_empty() {
            actual_lines.push(";".to_string());
        } else {
            actual_lines.push(format!("; {}", line));
        }
    }
    Ok(actual_lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::filetest_import_to_lpir_line;

    #[test]
    fn import_directive_to_lpir_scalar() {
        assert_eq!(
            filetest_import_to_lpir_line("helper(i32, i32) -> i32").unwrap(),
            "import @filetest::helper(i32, i32) -> i32\n"
        );
    }

    #[test]
    fn import_directive_void_return() {
        assert_eq!(
            filetest_import_to_lpir_line("noop()").unwrap(),
            "import @filetest::noop()\n"
        );
    }
}

/// Compare [`FileTest::expected`] with a fresh snapshot.
pub fn run_filetest(test: &FileTest) -> Result<(), String> {
    let actual = compute_filetest_snapshot(test)?;
    let expected_normalized = test.expected.trim_end().replace("\r\n", "\n");
    let actual_normalized = actual.trim_end().replace("\r\n", "\n");

    if expected_normalized != actual_normalized {
        return Err(format!(
            "Output mismatch in {}\n\nExpected (raw):\n{}\n\nActual (raw):\n{}",
            test.path, test.expected, actual
        ));
    }
    Ok(())
}
