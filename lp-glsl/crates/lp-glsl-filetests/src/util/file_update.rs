//! File update helper for bless mode.
//!
//! This module provides a helper struct to update test files in-place when
//! expectations don't match, matching Cranelift's FileUpdate semantics.

use crate::parse::test_type::ComparisonOp;
use anyhow::{Result, bail};
use lp_glsl_compiler::GlslValue;
use std::cell::Cell;
use std::fs;
use std::path::{Path, PathBuf};

/// A helper struct to update a file in-place as test expectations are
/// automatically updated.
///
/// This structure automatically handles multiple edits to one file. Our edits
/// are line-based but if editing a previous portion of the file adds lines then
/// all future edits need to know to skip over those previous lines. Note that
/// this assumes that edits are done front-to-back.
pub struct FileUpdate {
    path: PathBuf,
    line_diff: Cell<isize>,
    last_update: Cell<usize>,
}

impl FileUpdate {
    /// Create a new FileUpdate for the given path.
    pub fn new(path: &Path) -> Self {
        FileUpdate {
            path: path.to_path_buf(),
            line_diff: Cell::new(0),
            last_update: Cell::new(0),
        }
    }

    /// Update a `// run:` line with a new expected value.
    pub fn update_run_expectation(
        &self,
        line_number: usize,
        new_value: &GlslValue,
        comparison: ComparisonOp,
    ) -> Result<()> {
        // This is required for correctness of this update.
        assert!(line_number > self.last_update.get());
        self.last_update.set(line_number);

        // Read the old test file
        let old_test = fs::read_to_string(&self.path)?;
        let mut new_test = String::new();
        let mut lines = old_test.lines();
        let lines_to_preserve = (((line_number - 1) as isize) + self.line_diff.get()) as usize;

        // Push everything leading up to the run directive
        for _ in 0..lines_to_preserve {
            if let Some(line) = lines.next() {
                new_test.push_str(line);
                new_test.push('\n');
            }
        }

        // Find and update the run directive line
        if let Some(line) = lines.next() {
            if line.trim().starts_with("// run:") {
                // Parse the line to extract the expression part
                let trimmed = line.trim();
                if let Some(run_content) = trimmed.strip_prefix("// run:") {
                    let run_content = run_content.trim();

                    // Extract the expression (everything before == or ~=)
                    let expression = if let Some(pos) = run_content.rfind(" == ") {
                        run_content[..pos].trim()
                    } else if let Some(pos) = run_content.rfind(" ~= ") {
                        run_content[..pos].trim()
                    } else {
                        bail!("invalid run directive format at line {line_number}");
                    };

                    // Format the new expected value
                    let expected_str = format_glsl_value(new_value);
                    let op_str = match comparison {
                        ComparisonOp::Exact => "==",
                        ComparisonOp::Approx => "~=",
                    };

                    // Reconstruct the line with proper indentation
                    let indent = line
                        .chars()
                        .take_while(|c| c.is_whitespace())
                        .collect::<String>();
                    new_test.push_str(&format!(
                        "{indent}// run: {expression} {op_str} {expected_str}\n"
                    ));
                } else {
                    // Malformed run directive, keep original
                    new_test.push_str(line);
                    new_test.push('\n');
                }
            } else {
                // Not a run directive at expected line, keep original line
                new_test.push_str(line);
                new_test.push('\n');
            }
        }

        // Push the rest of the file
        for line in lines {
            new_test.push_str(line);
            new_test.push('\n');
        }

        // Record the difference in line count so future updates can be adjusted
        // accordingly, and then write the file back out to the filesystem.
        let old_line_count = old_test.lines().count();
        let new_line_count = new_test.lines().count();
        self.line_diff
            .set(self.line_diff.get() + (new_line_count as isize - old_line_count as isize));

        fs::write(&self.path, new_test)?;
        Ok(())
    }

    /// Add `[expect-fail]` marker to a `// run:` line.
    pub fn add_expect_fail_marker(&self, line_number: usize) -> Result<()> {
        // This is required for correctness of this update.
        assert!(line_number > self.last_update.get());
        self.last_update.set(line_number);

        // Read the old test file
        let old_test = fs::read_to_string(&self.path)?;
        let mut new_test = String::new();
        let mut lines = old_test.lines();
        let lines_to_preserve = (((line_number - 1) as isize) + self.line_diff.get()) as usize;

        // Push everything leading up to the run directive
        for _ in 0..lines_to_preserve {
            if let Some(line) = lines.next() {
                new_test.push_str(line);
                new_test.push('\n');
            }
        }

        // Find and add the marker to the run directive line
        if let Some(line) = lines.next() {
            if line.trim().starts_with("// run:") {
                // Add [expect-fail] marker if not already present
                let trimmed = line.trim();
                let updated_line = if trimmed.ends_with("[expect-fail]") {
                    // Already has marker, keep original
                    line.to_string()
                } else {
                    // Add marker at the end
                    let indent = line
                        .chars()
                        .take_while(|c| c.is_whitespace())
                        .collect::<String>();
                    format!("{indent}{trimmed} [expect-fail]")
                };
                new_test.push_str(&updated_line);
                new_test.push('\n');
            } else {
                // Not a run directive at expected line, keep original line
                new_test.push_str(line);
                new_test.push('\n');
            }
        }

        // Push the rest of the file
        for line in lines {
            new_test.push_str(line);
            new_test.push('\n');
        }

        // Record the difference in line count so future updates can be adjusted
        // accordingly, and then write the file back out to the filesystem.
        let old_line_count = old_test.lines().count();
        let new_line_count = new_test.lines().count();
        self.line_diff
            .set(self.line_diff.get() + (new_line_count as isize - old_line_count as isize));

        fs::write(&self.path, new_test)?;
        Ok(())
    }

    /// Remove `[expect-fail]` marker from a `// run:` line.
    pub fn remove_expect_fail_marker(&self, line_number: usize) -> Result<()> {
        // This is required for correctness of this update.
        assert!(line_number > self.last_update.get());
        self.last_update.set(line_number);

        // Read the old test file
        let old_test = fs::read_to_string(&self.path)?;
        let mut new_test = String::new();
        let mut lines = old_test.lines();
        let lines_to_preserve = (((line_number - 1) as isize) + self.line_diff.get()) as usize;

        // Push everything leading up to the run directive
        for _ in 0..lines_to_preserve {
            if let Some(line) = lines.next() {
                new_test.push_str(line);
                new_test.push('\n');
            }
        }

        // Find and remove the marker from the run directive line
        if let Some(line) = lines.next() {
            if line.trim().starts_with("// run:") {
                // Remove [expect-fail] marker if present
                let trimmed = line.trim();
                let updated_line = if trimmed.ends_with("[expect-fail]") {
                    // Strip the marker and any trailing whitespace
                    let without_marker = trimmed.strip_suffix("[expect-fail]").unwrap().trim_end();
                    // Preserve indentation
                    let indent = line
                        .chars()
                        .take_while(|c| c.is_whitespace())
                        .collect::<String>();
                    format!("{indent}{without_marker}")
                } else {
                    // No marker, keep original
                    line.to_string()
                };
                new_test.push_str(&updated_line);
                new_test.push('\n');
            } else {
                // Not a run directive at expected line, keep original line
                new_test.push_str(line);
                new_test.push('\n');
            }
        }

        // Push the rest of the file
        for line in lines {
            new_test.push_str(line);
            new_test.push('\n');
        }

        // Record the difference in line count so future updates can be adjusted
        // accordingly, and then write the file back out to the filesystem.
        let old_line_count = old_test.lines().count();
        let new_line_count = new_test.lines().count();
        self.line_diff
            .set(self.line_diff.get() + (new_line_count as isize - old_line_count as isize));

        fs::write(&self.path, new_test)?;
        Ok(())
    }

    /// Update CLIF expectations for a test type (compile or transform.q32).
    /// TODO: Implement when CLIF tests are implemented.
    pub fn update_clif_expectations(&self, _test_type: &str, _new_clif: &str) -> Result<()> {
        // TODO: Implement CLIF expectation updates when CLIF tests are implemented
        todo!("CLIF expectation updates not yet implemented")
    }
}

/// Format a float value with .0 suffix for whole numbers (matching GLSL literal format)
fn format_float(f: f32) -> String {
    if f.fract() == 0.0 {
        format!("{f:.1}")
    } else {
        format!("{f}")
    }
}

/// Format a GlslValue as a string for use in test files.
/// Matrices are displayed in GLSL constructor format (e.g., mat2(vec2(...), vec2(...)))
pub fn format_glsl_value(value: &GlslValue) -> String {
    match value {
        GlslValue::I32(i) => i.to_string(),
        GlslValue::U32(u) => format!("{u}u"),
        GlslValue::F32(f) => {
            // Format float with enough precision but avoid unnecessary decimals
            if f.fract() == 0.0 {
                format!("{f:.1}")
            } else {
                format!("{f}")
            }
        }
        GlslValue::Bool(b) => b.to_string(),
        GlslValue::Vec2(v) => format!("vec2({}, {})", format_float(v[0]), format_float(v[1])),
        GlslValue::Vec3(v) => format!(
            "vec3({}, {}, {})",
            format_float(v[0]),
            format_float(v[1]),
            format_float(v[2])
        ),
        GlslValue::Vec4(v) => format!(
            "vec4({}, {}, {}, {})",
            format_float(v[0]),
            format_float(v[1]),
            format_float(v[2]),
            format_float(v[3])
        ),
        GlslValue::IVec2(v) => format!("ivec2({}, {})", v[0], v[1]),
        GlslValue::IVec3(v) => format!("ivec3({}, {}, {})", v[0], v[1], v[2]),
        GlslValue::IVec4(v) => format!("ivec4({}, {}, {}, {})", v[0], v[1], v[2], v[3]),
        GlslValue::UVec2(v) => format!("uvec2({}u, {}u)", v[0], v[1]),
        GlslValue::UVec3(v) => format!("uvec3({}u, {}u, {}u)", v[0], v[1], v[2]),
        GlslValue::UVec4(v) => format!("uvec4({}u, {}u, {}u, {}u)", v[0], v[1], v[2], v[3]),
        GlslValue::BVec2(v) => format!("bvec2({}, {})", v[0], v[1]),
        GlslValue::BVec3(v) => format!("bvec3({}, {}, {})", v[0], v[1], v[2]),
        GlslValue::BVec4(v) => format!("bvec4({}, {}, {}, {})", v[0], v[1], v[2], v[3]),
        GlslValue::Mat2x2(m) => {
            // Display in GLSL constructor format: mat2(vec2(col0), vec2(col1))
            // m[col][row] format, so column 0 is [m[0][0], m[0][1]], column 1 is [m[1][0], m[1][1]]
            format!(
                "mat2(vec2({}, {}), vec2({}, {}))",
                format_float(m[0][0]),
                format_float(m[0][1]),
                format_float(m[1][0]),
                format_float(m[1][1])
            )
        }
        GlslValue::Mat3x3(m) => {
            // Display in GLSL constructor format: mat3(vec3(col0), vec3(col1), vec3(col2))
            // m[col][row] format, so column 0 is [m[0][0], m[0][1], m[0][2]]
            // Column 1: [m[1][0], m[1][1], m[1][2]]
            // Column 2: [m[2][0], m[2][1], m[2][2]]
            format!(
                "mat3(vec3({}, {}, {}), vec3({}, {}, {}), vec3({}, {}, {}))",
                format_float(m[0][0]),
                format_float(m[0][1]),
                format_float(m[0][2]), // column 0
                format_float(m[1][0]),
                format_float(m[1][1]),
                format_float(m[1][2]), // column 1
                format_float(m[2][0]),
                format_float(m[2][1]),
                format_float(m[2][2]) // column 2
            )
        }
        GlslValue::Mat4x4(m) => {
            // Display in GLSL constructor format: mat4(vec4(col0), vec4(col1), vec4(col2), vec4(col3))
            // m[col][row] format, so column 0 is [m[0][0], m[0][1], m[0][2], m[0][3]]
            // Column 1: [m[1][0], m[1][1], m[1][2], m[1][3]]
            // Column 2: [m[2][0], m[2][1], m[2][2], m[2][3]]
            // Column 3: [m[3][0], m[3][1], m[3][2], m[3][3]]
            format!(
                "mat4(vec4({}, {}, {}, {}), vec4({}, {}, {}, {}), vec4({}, {}, {}, {}), vec4({}, {}, {}, {}))",
                format_float(m[0][0]),
                format_float(m[0][1]),
                format_float(m[0][2]),
                format_float(m[0][3]), // column 0
                format_float(m[1][0]),
                format_float(m[1][1]),
                format_float(m[1][2]),
                format_float(m[1][3]), // column 1
                format_float(m[2][0]),
                format_float(m[2][1]),
                format_float(m[2][2]),
                format_float(m[2][3]), // column 2
                format_float(m[3][0]),
                format_float(m[3][1]),
                format_float(m[3][2]),
                format_float(m[3][3]) // column 3
            )
        }
    }
}
