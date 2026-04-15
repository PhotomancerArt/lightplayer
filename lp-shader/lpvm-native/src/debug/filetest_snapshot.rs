//! Filetest snapshot rendering gated by `feature = "debug"`.

use alloc::string::{String, ToString};

use lpir::{IrFunction, LpirModule};

use crate::abi::FuncAbi;
#[cfg(feature = "debug")]
use crate::alloc::render::render_interleaved;
use crate::lower::LoweredFunction;
use crate::vinst::{VInst, VReg};

/// Build comment-prefixed snapshot lines for allocator filetests.
pub fn build_allocator_snapshot_lines(
    module: &LpirModule,
    func: &IrFunction,
    vinsts: &[VInst],
    vreg_pool: &[VReg],
    output: &crate::alloc::AllocOutput,
    func_abi: &FuncAbi,
    lowered: &LoweredFunction,
    filetest_separator: &str,
) -> Result<String, String> {
    #[cfg(feature = "debug")]
    {
        let rendered = render_interleaved(
            func,
            module,
            vinsts,
            vreg_pool,
            output,
            func_abi,
            &lowered.symbols,
        );

        let mut actual_lines = vec![filetest_separator.to_string()];
        actual_lines.push(";".to_string());
        for line in rendered.lines() {
            if line.is_empty() {
                actual_lines.push(";".to_string());
            } else {
                actual_lines.push(alloc::format!("; {}", line));
            }
        }
        Ok(actual_lines.join("\n"))
    }
    #[cfg(not(feature = "debug"))]
    {
        let mut actual_lines = vec![filetest_separator.to_string()];
        actual_lines.push(";".to_string());
        actual_lines
            .push("; Debug output disabled (build with --features debug to enable)".to_string());
        actual_lines.push(";".to_string());
        let _ = (module, func, vinsts, vreg_pool, output, func_abi, lowered);
        Ok(actual_lines.join("\n"))
    }
}
