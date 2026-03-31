//! File update helper for bless mode.
//!
//! This module provides a helper struct to update test files in-place when
//! expectations don't match, matching Cranelift's FileUpdate semantics.

use crate::parse::parse_annotation;
use crate::parse::test_type::ComparisonOp;
use anyhow::{Result, bail};
use lp_glsl_values::GlslValue;
use std::cell::Cell;
use std::fs;
use std::path::{Path, PathBuf};

use crate::target::{Annotation, AnnotationKind, Target};

/// Line indices of `// @…` annotations in the file-level prefix (see [`crate::parse::parse_test_file`]):
/// before the first `// run:` and before the first non-empty non-comment line.
fn file_level_annotation_indices(
    all_lines: &[&str],
    mut keep_annotation: impl FnMut(&Annotation) -> bool,
) -> Vec<usize> {
    let first_run = all_lines
        .iter()
        .position(|line| line.trim().starts_with("// run:"))
        .unwrap_or(all_lines.len());
    let mut seen_glsl = false;
    let mut indices = Vec::new();
    for i in 0..first_run {
        let line = all_lines[i];
        let trimmed = line.trim();
        if !trimmed.is_empty() && !trimmed.starts_with("//") {
            seen_glsl = true;
        }
        if seen_glsl {
            continue;
        }
        if let Ok(Some(ann)) = parse_annotation::parse_annotation_line(line, i + 1) {
            if keep_annotation(&ann) {
                indices.push(i);
            }
        }
    }
    indices
}

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

    /// `true` if the line immediately before this `// run:` already has `@unimplemented` matching
    /// `target` (uses `line_diff` like [`add_annotation`]).
    pub fn per_directive_unimplemented_present(
        &self,
        run_line_1based: usize,
        target: &Target,
    ) -> Result<bool> {
        let old_test = fs::read_to_string(&self.path)?;
        let all_lines: Vec<&str> = old_test.lines().collect();
        let run_line_idx =
            (((run_line_1based - 1) as isize) + self.line_diff.get()).max(0) as usize;
        if run_line_idx == 0 {
            return Ok(false);
        }
        let prev = all_lines[run_line_idx - 1];
        if let Ok(Some(ann)) =
            parse_annotation::parse_annotation_line(prev, run_line_1based.saturating_sub(1))
        {
            return Ok(ann.kind == AnnotationKind::Unimplemented && ann.filter.matches(target));
        }
        Ok(false)
    }

    /// Insert `// @unimplemented(backend=…)` before the first `// run:` if none exists at file
    /// level for `target`. Returns `Ok(true)` if a line was added, `Ok(false)` if already present.
    pub fn ensure_file_level_unimplemented(&self, target: &Target) -> Result<bool> {
        let annotation = format!(
            "// @unimplemented(backend={})",
            match target.backend {
                crate::target::Backend::Jit => "jit",
                crate::target::Backend::Rv32 => "rv32",
                crate::target::Backend::Wasm => "wasm",
            }
        );

        let old_test = fs::read_to_string(&self.path)?;
        let all_lines: Vec<&str> = old_test.lines().collect();
        let mut first_run_idx: Option<usize> = None;
        for (i, line) in all_lines.iter().enumerate() {
            if line.trim().starts_with("// run:") {
                first_run_idx = Some(i);
                break;
            }
        }
        let Some(first_run_idx) = first_run_idx else {
            bail!("no // run: directive in {}", self.path.display());
        };

        for line in &all_lines[..first_run_idx] {
            if let Ok(Some(ann)) = parse_annotation::parse_annotation_line(line, 1) {
                if ann.kind == AnnotationKind::Unimplemented && ann.filter.matches(target) {
                    return Ok(false);
                }
            }
        }

        let indent = all_lines[first_run_idx]
            .chars()
            .take_while(|c| c.is_whitespace())
            .collect::<String>();
        let new_line = format!("{indent}{annotation}");

        let mut new_test = String::new();
        for i in 0..first_run_idx {
            new_test.push_str(all_lines[i]);
            new_test.push('\n');
        }
        new_test.push_str(&new_line);
        new_test.push('\n');
        for i in first_run_idx..all_lines.len() {
            new_test.push_str(all_lines[i]);
            new_test.push('\n');
        }

        fs::write(&self.path, new_test)?;
        Ok(true)
    }

    /// Add an annotation line before the run directive at the given line number.
    pub fn add_annotation(&self, line_number: usize, annotation: &str) -> Result<()> {
        assert!(line_number > self.last_update.get());
        self.last_update.set(line_number);

        let old_test = fs::read_to_string(&self.path)?;
        let all_lines: Vec<&str> = old_test.lines().collect();
        let run_line_idx = (((line_number - 1) as isize) + self.line_diff.get()).max(0) as usize;

        if run_line_idx >= all_lines.len() {
            bail!("line {line_number} out of range");
        }

        if !all_lines[run_line_idx].trim().starts_with("// run:") {
            bail!("line {line_number} is not a run directive");
        }

        let indent = all_lines[run_line_idx]
            .chars()
            .take_while(|c| c.is_whitespace())
            .collect::<String>();
        let new_line = format!("{indent}{annotation}");

        let mut new_test = String::new();
        for i in 0..run_line_idx {
            new_test.push_str(all_lines[i]);
            new_test.push('\n');
        }
        new_test.push_str(&new_line);
        new_test.push('\n');
        for i in run_line_idx..all_lines.len() {
            new_test.push_str(all_lines[i]);
            new_test.push('\n');
        }

        let old_line_count = all_lines.len();
        let new_line_count = new_test.lines().count();
        self.line_diff
            .set(self.line_diff.get() + (new_line_count as isize - old_line_count as isize));

        fs::write(&self.path, new_test)?;
        Ok(())
    }

    /// Add `[expect-fail]` marker to a `// run:` line.
    /// Deprecated: use add_annotation with "// @unimplemented()" instead.
    pub fn add_expect_fail_marker(&self, line_number: usize) -> Result<()> {
        self.add_annotation(line_number, "// @unimplemented()")
    }

    /// Remove annotation line(s) immediately before the run directive at the given line number.
    /// Also strips legacy [expect-fail] from the run line if present.
    pub fn remove_annotation(&self, line_number: usize) -> Result<()> {
        assert!(line_number > self.last_update.get());
        self.last_update.set(line_number);

        let old_test = fs::read_to_string(&self.path)?;
        let all_lines: Vec<&str> = old_test.lines().collect();
        let run_line_idx = (((line_number - 1) as isize) + self.line_diff.get()).max(0) as usize;

        if run_line_idx >= all_lines.len() {
            bail!("line {line_number} out of range");
        }

        let run_line = all_lines[run_line_idx];
        if !run_line.trim().starts_with("// run:") {
            bail!("line {line_number} is not a run directive");
        }

        let mut first_annotation_idx = run_line_idx;
        if run_line_idx > 0 {
            let mut j = run_line_idx - 1;
            while let Some(line) = all_lines.get(j) {
                let prev = line.trim();
                if prev.starts_with("// @") {
                    first_annotation_idx = j;
                    if j == 0 {
                        break;
                    }
                    j -= 1;
                } else {
                    break;
                }
            }
        }

        let mut new_test = String::new();
        for i in 0..first_annotation_idx {
            new_test.push_str(all_lines[i]);
            new_test.push_str("\n");
        }

        let trimmed = run_line.trim();
        let updated_run = if trimmed.ends_with("[expect-fail]") {
            let without = trimmed.strip_suffix("[expect-fail]").unwrap().trim_end();
            let indent = run_line
                .chars()
                .take_while(|c| c.is_whitespace())
                .collect::<String>();
            format!("{indent}{without}")
        } else {
            run_line.to_string()
        };
        new_test.push_str(&updated_run);
        new_test.push('\n');

        for i in run_line_idx + 1..all_lines.len() {
            new_test.push_str(all_lines[i]);
            new_test.push('\n');
        }

        let old_line_count = all_lines.len();
        let new_line_count = new_test.lines().count();
        self.line_diff
            .set(self.line_diff.get() + (new_line_count as isize - old_line_count as isize));

        fs::write(&self.path, new_test)?;
        Ok(())
    }

    /// Like [`Self::remove_annotation`], but removes only `// @` lines whose parsed filter matches
    /// `target`. Other annotations immediately above the same `// run:` are preserved.
    pub fn remove_annotation_matching_target(
        &self,
        line_number: usize,
        target: &Target,
    ) -> Result<()> {
        assert!(line_number > self.last_update.get());
        self.last_update.set(line_number);

        let old_test = fs::read_to_string(&self.path)?;
        let all_lines: Vec<&str> = old_test.lines().collect();
        let run_line_idx = (((line_number - 1) as isize) + self.line_diff.get()).max(0) as usize;

        if run_line_idx >= all_lines.len() {
            bail!("line {line_number} out of range");
        }

        let run_line = all_lines[run_line_idx];
        if !run_line.trim().starts_with("// run:") {
            bail!("line {line_number} is not a run directive");
        }

        let mut indices_to_remove: Vec<usize> = Vec::new();
        let mut j = run_line_idx;
        while j > 0 {
            j -= 1;
            let line = all_lines[j];
            let prev = line.trim();
            if prev.starts_with("// @") {
                if let Ok(Some(ann)) = parse_annotation::parse_annotation_line(line, j + 1) {
                    if ann.filter.matches(target) {
                        indices_to_remove.push(j);
                    }
                }
            } else {
                break;
            }
        }

        if indices_to_remove.is_empty() {
            return Ok(());
        }

        let skip: std::collections::HashSet<usize> = indices_to_remove.iter().copied().collect();

        let mut new_test = String::new();
        for (i, line) in all_lines.iter().enumerate() {
            if skip.contains(&i) {
                continue;
            }
            if i == run_line_idx {
                let trimmed = line.trim();
                let updated = if trimmed.ends_with("[expect-fail]") {
                    let without = trimmed.strip_suffix("[expect-fail]").unwrap().trim_end();
                    let indent = line
                        .chars()
                        .take_while(|c| c.is_whitespace())
                        .collect::<String>();
                    format!("{indent}{without}")
                } else {
                    (*line).to_string()
                };
                new_test.push_str(&updated);
                new_test.push('\n');
            } else {
                new_test.push_str(line);
                new_test.push('\n');
            }
        }

        let old_line_count = all_lines.len();
        let new_line_count = new_test.lines().count();
        self.line_diff
            .set(self.line_diff.get() + (new_line_count as isize - old_line_count as isize));

        fs::write(&self.path, new_test)?;
        Ok(())
    }

    /// Remove `[expect-fail]` marker from a `// run:` line.
    /// Deprecated: use remove_annotation instead.
    pub fn remove_expect_fail_marker(&self, line_number: usize) -> Result<()> {
        self.remove_annotation(line_number)
    }

    /// Remove every file-level `// @unimplemented(...)` line, regardless of backend or other
    /// filter fields. File-level matches [`crate::parse::parse_test_file`]: annotations that appear
    /// before any `// run:` and before the first non-empty line that is not a `//` comment.
    ///
    /// Does not remove `@broken` or `@unsupported`. Updates [`Self::line_diff`] like
    /// [`Self::remove_file_level_annotations_matching`].
    pub fn remove_all_file_level_unimplemented_annotations(&self) -> Result<()> {
        let old_test = fs::read_to_string(&self.path)?;
        let all_lines: Vec<&str> = old_test.lines().collect();
        let indices_to_remove = file_level_annotation_indices(&all_lines, |ann| {
            matches!(ann.kind, AnnotationKind::Unimplemented)
        });

        if indices_to_remove.is_empty() {
            return Ok(());
        }

        let skip: std::collections::HashSet<usize> = indices_to_remove.iter().copied().collect();
        let mut new_test = String::new();
        for (i, line) in all_lines.iter().enumerate() {
            if skip.contains(&i) {
                continue;
            }
            new_test.push_str(line);
            new_test.push('\n');
        }

        let removed = indices_to_remove.len() as isize;
        self.line_diff.set(self.line_diff.get() - removed);
        fs::write(&self.path, new_test)?;
        Ok(())
    }

    /// Remove file-level annotations (at top of file, before first run directive)
    /// that match the target. Used when tests with file-level @unimplemented(backend=wasm)
    /// now pass. Updates line_diff so subsequent remove_annotation calls use correct indices.
    pub fn remove_file_level_annotations_matching(&self, target: &Target) -> Result<()> {
        let old_test = fs::read_to_string(&self.path)?;
        let all_lines: Vec<&str> = old_test.lines().collect();
        let indices_to_remove =
            file_level_annotation_indices(&all_lines, |ann| ann.filter.matches(target));
        if indices_to_remove.is_empty() {
            return Ok(());
        }
        let mut new_test = String::new();
        for (i, line) in all_lines.iter().enumerate() {
            if indices_to_remove.contains(&i) {
                continue;
            }
            new_test.push_str(line);
            new_test.push('\n');
        }
        let removed = indices_to_remove.len() as isize;
        self.line_diff.set(self.line_diff.get() - removed);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::target::Target;

    fn temp_glsl(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "lp_file_update_test_{}_{}",
            std::process::id(),
            name
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir.join("sample.glsl")
    }

    #[test]
    fn remove_all_file_level_unimplemented_strips_backend_wasm_header() {
        let path = temp_glsl("wasm_header");
        let content = r"// test run
// @unimplemented(backend=wasm)

float one() { return 1.0; }
// run: one() ~= 1.0
";
        fs::write(&path, content).expect("write");
        let u = FileUpdate::new(&path);
        u.remove_all_file_level_unimplemented_annotations()
            .expect("remove header");
        let out = fs::read_to_string(&path).expect("read");
        assert!(
            !out.contains("@unimplemented"),
            "expected wasm-only header removed: {out}"
        );
        assert!(
            out.contains("// run:"),
            "run directive should remain: {out}"
        );
    }

    #[test]
    fn remove_all_file_level_unimplemented_keeps_broken_header() {
        let path = temp_glsl("broken_header");
        let content = r"// test run
// @broken()

float one() { return 1.0; }
// run: one() ~= 1.0
";
        fs::write(&path, content).expect("write");
        let u = FileUpdate::new(&path);
        u.remove_all_file_level_unimplemented_annotations()
            .expect("noop ok");
        let out = fs::read_to_string(&path).expect("read");
        assert!(out.contains("// @broken()"));
    }

    #[test]
    fn remove_all_file_level_unimplemented_does_not_touch_per_run_annotations() {
        let path = temp_glsl("per_run");
        let content = r"// test run

float one() { return 1.0; }
// @unimplemented()
// run: one() ~= 1.0
";
        fs::write(&path, content).expect("write");
        let u = FileUpdate::new(&path);
        u.remove_all_file_level_unimplemented_annotations()
            .expect("remove");
        let out = fs::read_to_string(&path).expect("read");
        assert!(
            out.contains("// @unimplemented()"),
            "per-run marker must remain: {out}"
        );
    }

    #[test]
    fn remove_file_level_then_per_run_matching_rv32_updates_line_diff() {
        let path = temp_glsl("fix_order");
        let content = r"// test run
// @unimplemented(backend=wasm)

float one() { return 1.0; }
// @unimplemented()
// run: one() ~= 1.0
";
        fs::write(&path, content).expect("write");
        let u = FileUpdate::new(&path);
        u.remove_all_file_level_unimplemented_annotations()
            .expect("strip wasm header");
        let rv32 = Target::from_name("rv32.q32").expect("rv32 target");
        u.remove_annotation_matching_target(6, rv32)
            .expect("strip per-run");
        let out = fs::read_to_string(&path).expect("read");
        assert!(!out.contains("@unimplemented"), "all lifted: {out}");
        assert!(out.contains("// run: one()"));
    }
}
