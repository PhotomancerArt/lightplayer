//! Display and formatting logic for shader-debug.

use super::comparison_table;
use super::types::{DebugReport, SectionFilter};

fn should_color() -> bool {
    std::env::var("NO_COLOR").is_err()
}

/// Print comparison table across all backends.
pub fn print_comparison_table(report: &DebugReport) {
    if let Some(text) = comparison_table::render_summary_table(report, should_color()) {
        print!("{}", text);
    }
}

/// Print detailed output for each function.
pub fn print_detailed_view(report: &DebugReport, sections: &SectionFilter) {
    let func_names = report.function_names();
    let mut first_func = true;

    for func_name in &func_names {
        if !first_func {
            println!();
        }
        first_func = false;

        println!("=== Function: {} ===", func_name);
        println!();

        // Show each backend's output
        for (idx, backend) in report.backends.iter().enumerate() {
            if let Some(func_data) = backend.get_function(func_name) {
                if idx > 0 {
                    println!();
                }
                println!("--- {} ---", backend.backend);

                // Show spill slots for FA
                if let Some(slots) = func_data.spill_slots {
                    println!("; spill_slots: {}", slots);
                }

                // Interleaved section
                if sections.vinst && func_data.has_vinst {
                    if let Some(ref interleaved) = func_data.interleaved {
                        println!();
                        let vinst_count = interleaved.lines().filter(|l| l.contains(" = ")).count();
                        println!("--- interleaved ({} VInsts) ---", vinst_count);
                        println!("{}", interleaved);
                    }
                }

                // LPIR section (raw LPIR ops)
                if sections.lpir {
                    println!();
                    println!("--- LPIR ({} ops) ---", func_data.lpir_count);
                    // Note: We don't store raw LPIR in the data structure currently
                    println!("; (LPIR source not stored in debug data)");
                }

                // Assembly section
                if sections.asm {
                    println!();
                    println!("--- disasm ({} instructions) ---", func_data.disasm_count);
                    print!("{}", func_data.disasm);
                }
            }
        }
    }
}

/// Print help text with copy-pasteable commands.
pub fn print_help_text(file_path: &str, report: &DebugReport) {
    let func_names = report.function_names();
    if func_names.len() <= 1 {
        return;
    }

    println!("────────────────────────────────────────");
    println!("To show a specific function:");

    let targets = report
        .backends
        .iter()
        .map(|b| b.backend.as_str())
        .collect::<Vec<_>>()
        .join(",");
    for func_name in &func_names {
        println!(
            "  scripts/shader-debug.sh -t {} {} --fn {}",
            targets, file_path, func_name
        );
    }

    println!();
    print!("Available functions: ");
    println!("{}", func_names.join(", "));
}
