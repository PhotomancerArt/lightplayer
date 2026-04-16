//! Compilation debug information types.
//!
//! Provides structured debug output for compiled shader modules.
//! Each backend populates sections appropriate to its capabilities:
//! - rv32n: interleaved, disasm, vinst, liveness, region
//! - rv32c/rv32n: disasm only
//! - jit/wasm: (not available)

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

/// Per-function compilation debug info.
#[derive(Clone, Debug, Default)]
pub struct FunctionDebugInfo {
    /// Function name.
    pub name: String,
    /// Static instruction count (from disassembly).
    pub inst_count: usize,
    /// Named sections. Standard keys: "interleaved", "disasm", "vinst", "liveness", "region".
    pub sections: BTreeMap<String, String>,
}

impl FunctionDebugInfo {
    /// Create new FunctionDebugInfo with just a name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            inst_count: 0,
            sections: BTreeMap::new(),
        }
    }

    /// Add a section.
    pub fn with_section(mut self, name: impl Into<String>, content: impl Into<String>) -> Self {
        self.sections.insert(name.into(), content.into());
        self
    }

    /// Add multiple sections from a map.
    pub fn with_sections(mut self, sections: BTreeMap<String, String>) -> Self {
        self.sections = sections;
        self
    }

    /// Set instruction count.
    pub fn with_inst_count(mut self, count: usize) -> Self {
        self.inst_count = count;
        self
    }
}

/// Module-level compilation debug info.
#[derive(Clone, Debug, Default)]
pub struct ModuleDebugInfo {
    /// Function name → debug info.
    pub functions: BTreeMap<String, FunctionDebugInfo>,
}

impl ModuleDebugInfo {
    /// Create empty ModuleDebugInfo.
    pub fn new() -> Self {
        Self {
            functions: BTreeMap::new(),
        }
    }

    /// Add a function's debug info.
    pub fn add_function(&mut self, info: FunctionDebugInfo) {
        self.functions.insert(info.name.clone(), info);
    }

    /// Get list of function names.
    pub fn function_names(&self) -> Vec<&str> {
        self.functions.keys().map(|s| s.as_str()).collect()
    }

    /// Render all functions or a filtered function to a string.
    pub fn render(&self, fn_filter: Option<&str>) -> String {
        let mut out = String::new();

        let functions_to_render: Vec<_> = if let Some(name) = fn_filter {
            self.functions.get(name).into_iter().collect()
        } else {
            self.functions.values().collect()
        };

        for (i, func) in functions_to_render.iter().enumerate() {
            if i > 0 {
                out.push_str("\n\n");
            }
            out.push_str(&format!("=== Function: {} ===\n\n", func.name));

            // Standard section order
            let section_order = &["interleaved", "disasm", "vinst", "liveness", "region"];

            for section_name in section_order {
                if let Some(content) = func.sections.get(*section_name) {
                    let count_line = if *section_name == "disasm" {
                        format!(" ({} instructions)", func.inst_count)
                    } else if *section_name == "interleaved" {
                        // Count VInsts in content (lines containing " = ")
                        let vinst_count = content.lines().filter(|l| l.contains(" = ")).count();
                        format!(" ({vinst_count} VInsts)")
                    } else {
                        String::new()
                    };

                    out.push_str(&format!("--- {section_name}{count_line} ---\n"));
                    out.push_str(content);
                    if !content.ends_with('\n') {
                        out.push('\n');
                    }
                    out.push('\n');
                } else if *section_name == "interleaved" && func.sections.contains_key("disasm") {
                    // Special message for missing interleaved when disasm is present
                    out.push_str(&format!("--- {section_name} ---\n"));
                    out.push_str(
                        "(not available for this backend - only disassembly available)\n\n",
                    );
                }
            }
        }

        out
    }

    /// Generate help text with copy-pasteable commands.
    pub fn help_text(&self, file_path: &str, target: &str) -> String {
        let mut out = String::new();

        out.push_str("────────────────────────────────────────\n");
        out.push_str("To show a specific function:\n");

        for func_name in self.function_names() {
            out.push_str(&format!(
                "  lp-cli shader-debug -t {target} {file_path} --fn {func_name}\n"
            ));
        }

        out.push('\n');
        out.push_str("Available functions: ");
        out.push_str(&self.function_names().join(", "));
        out.push('\n');

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn function_debug_info_builder() {
        let info = FunctionDebugInfo::new("test")
            .with_inst_count(10)
            .with_section("disasm", "addi...");
        assert_eq!(info.name, "test");
        assert_eq!(info.inst_count, 10);
        assert!(info.sections.contains_key("disasm"));
    }

    #[test]
    fn module_debug_info_add_function() {
        let mut module = ModuleDebugInfo::new();
        let func = FunctionDebugInfo::new("foo").with_section("disasm", "...");
        module.add_function(func);
        assert!(module.functions.contains_key("foo"));
    }

    #[test]
    fn module_debug_info_render_empty() {
        let module = ModuleDebugInfo::new();
        let output = module.render(None);
        assert!(output.is_empty());
    }

    #[test]
    fn module_debug_info_render_single_function() {
        let mut module = ModuleDebugInfo::new();
        let func = FunctionDebugInfo::new("test")
            .with_inst_count(5)
            .with_section("disasm", "0000 addi\n0004 ret\n");
        module.add_function(func);

        let output = module.render(None);
        assert!(output.contains("=== Function: test ==="));
        assert!(output.contains("--- disasm (5 instructions) ---"));
        assert!(output.contains("(not available for this backend"));
    }

    #[test]
    fn module_debug_info_render_interleaved() {
        let mut module = ModuleDebugInfo::new();
        let func = FunctionDebugInfo::new("test")
            .with_inst_count(3)
            .with_section("interleaved", "v1 = iconst\n    i1 = IConst32\n")
            .with_section("disasm", "addi...\n");
        module.add_function(func);

        let output = module.render(None);
        assert!(output.contains("--- interleaved (2 VInsts) ---"));
        assert!(output.contains("--- disasm (3 instructions) ---"));
        // Should NOT show "not available" when interleaved is present
        assert!(!output.contains("not available for this backend"));
    }

    #[test]
    fn module_debug_info_help_text() {
        let mut module = ModuleDebugInfo::new();
        module.add_function(FunctionDebugInfo::new("foo"));
        module.add_function(FunctionDebugInfo::new("bar"));

        let help = module.help_text("test.glsl", "rv32n");
        assert!(help.contains("lp-cli shader-debug -t rv32n test.glsl --fn foo"));
        assert!(help.contains("lp-cli shader-debug -t rv32n test.glsl --fn bar"));
        // BTreeMap iterates in sorted order, so "bar" comes before "foo"
        assert!(help.contains("Available functions: bar, foo"));
    }

    #[test]
    fn module_debug_info_filter_single_function() {
        let mut module = ModuleDebugInfo::new();
        module.add_function(FunctionDebugInfo::new("foo").with_section("disasm", "foo\n"));
        module.add_function(FunctionDebugInfo::new("bar").with_section("disasm", "bar\n"));

        let output = module.render(Some("foo"));
        assert!(output.contains("=== Function: foo ==="));
        assert!(!output.contains("=== Function: bar ==="));
    }
}
