//! General utilities for test generation.

/// Generate file header with regeneration command.
pub fn generate_header(specifier: &str) -> String {
    format!(
        "// This file is GENERATED. Do not edit manually.\n\
         // To regenerate, run:\n\
         //   lp-glsl-filetests-gen-app {specifier} --write\n\
         //\n"
    )
}
