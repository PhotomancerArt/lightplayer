//! Simple example of compiling and executing GLSL code

use lp_glsl_compiler::{DecimalFormat, GlslOptions, RunMode, glsl_jit};

fn main() {
    let options = GlslOptions {
        run_mode: RunMode::HostJit,
        decimal_format: DecimalFormat::Float,
        q32_opts: lp_glsl_compiler::Q32Options::default(),
    };

    // Example 1: Integer arithmetic
    let shader1 = r#"
        int main() {
            int a = 10;
            int b = 32;
            return a + b;
        }
    "#;

    let mut executable1 = glsl_jit(shader1, options.clone()).unwrap();
    let result1 = executable1.call_i32("main", &[]).unwrap();
    println!("Example 1 - Integer arithmetic: {result1} (expected: 42)");
    assert_eq!(result1, 42);

    // Example 2: Boolean comparison
    let shader2 = r#"
        bool main() {
            int x = 5;
            int y = 10;
            return x < y;
        }
    "#;

    let mut executable2 = glsl_jit(shader2, options.clone()).unwrap();
    let result2 = executable2.call_bool("main", &[]).unwrap();
    println!("Example 2 - Boolean comparison: {result2} (expected: true)");
    assert_eq!(result2, true);

    // Example 3: Complex expression
    let shader3 = r#"
        int main() {
            int a = 5;
            int b = 3;
            int c = 2;
            return (a + b) * c - 4;
        }
    "#;

    let mut executable3 = glsl_jit(shader3, options).unwrap();
    let result3 = executable3.call_i32("main", &[]).unwrap();
    println!("Example 3 - Complex expression: {result3} (expected: 12)");
    assert_eq!(result3, 12); // (5 + 3) * 2 - 4 = 12

    println!("\nAll examples passed!");
}
