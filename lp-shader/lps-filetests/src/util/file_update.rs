//! File update helper for bless mode and baseline annotations.

use crate::parse::parse_annotation;
use crate::parse::test_type::ComparisonOp;
use anyhow::{Result, bail};
use lpvm::LpsValueF32;
use std::cell::Cell;
use std::fs;
use std::path::{Path, PathBuf};

use crate::targets::{AnnotationKind, Target};

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
        new_value: &LpsValueF32,
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

    /// `true` if any consecutive `// @` line immediately above this `// run:` has
    /// `@unimplemented(<target>)` (uses `line_diff` like [`add_annotation`]).
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
        let mut j = run_line_idx;
        while j > 0 {
            j -= 1;
            let line = all_lines[j];
            let prev = line.trim();
            if !prev.starts_with("// @") {
                break;
            }
            if let Ok(Some(ann)) = parse_annotation::parse_annotation_line(line, j + 1) {
                if ann.kind == AnnotationKind::Unimplemented && ann.applies_to(target) {
                    return Ok(true);
                }
            }
        }
        Ok(false)
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
    /// Deprecated: use [`add_annotation`](Self::add_annotation) with `// @unimplemented(target)`.
    pub fn add_expect_fail_marker(&self, line_number: usize) -> Result<()> {
        for t in crate::targets::ALL_TARGETS {
            self.add_annotation(line_number, &format!("// @unimplemented({})", t.name()))?;
        }
        Ok(())
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

    /// Like [`Self::remove_annotation`], but removes only `// @` lines that apply to `target`.
    /// Other annotations immediately above the same `// run:` are preserved.
    /// Returns `true` if any annotation was actually removed.
    pub fn remove_annotation_matching_target(
        &self,
        line_number: usize,
        target: &Target,
    ) -> Result<bool> {
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

        let want = target.name();
        let mut indices_to_remove: Vec<usize> = Vec::new();
        let mut j = run_line_idx;
        while j > 0 {
            j -= 1;
            let line = all_lines[j];
            let prev = line.trim();
            if prev.starts_with("// @") {
                if let Ok(Some(ann)) = parse_annotation::parse_annotation_line(line, j + 1) {
                    if ann.target == want {
                        indices_to_remove.push(j);
                    }
                }
            } else {
                break;
            }
        }

        if indices_to_remove.is_empty() {
            return Ok(false);
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
        Ok(true)
    }

    /// Like [`Self::remove_annotation_matching_target`], but removes annotations for multiple targets
    /// in a single pass. This is needed when the same `// run:` line has multiple `@unimplemented`
    /// annotations (e.g., for wasm.q32, rv32c.q32, rv32n.q32) that all need to be removed.
    /// Returns the number of annotations removed.
    pub fn remove_annotations_matching_targets(
        &self,
        line_number: usize,
        targets: &[&Target],
    ) -> Result<usize> {
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

        let target_names: std::collections::HashSet<String> =
            targets.iter().map(|t| t.name().to_string()).collect();
        let mut indices_to_remove: Vec<usize> = Vec::new();
        let mut j = run_line_idx;
        while j > 0 {
            j -= 1;
            let line = all_lines[j];
            let prev = line.trim();
            if prev.starts_with("// @") {
                if let Ok(Some(ann)) = parse_annotation::parse_annotation_line(line, j + 1) {
                    if target_names.contains(ann.target.as_str()) {
                        indices_to_remove.push(j);
                    }
                }
            } else {
                break;
            }
        }

        if indices_to_remove.is_empty() {
            return Ok(0);
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
        Ok(indices_to_remove.len())
    }

    /// Remove `[expect-fail]` marker from a `// run:` line.
    /// Deprecated: use remove_annotation instead.
    pub fn remove_expect_fail_marker(&self, line_number: usize) -> Result<()> {
        self.remove_annotation(line_number)
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
pub fn format_glsl_value(value: &LpsValueF32) -> String {
    match value {
        LpsValueF32::I32(i) => i.to_string(),
        LpsValueF32::U32(u) => format!("{u}u"),
        LpsValueF32::F32(f) => {
            // Format float with enough precision but avoid unnecessary decimals
            if f.fract() == 0.0 {
                format!("{f:.1}")
            } else {
                format!("{f}")
            }
        }
        LpsValueF32::Bool(b) => b.to_string(),
        LpsValueF32::Vec2(v) => format!("vec2({}, {})", format_float(v[0]), format_float(v[1])),
        LpsValueF32::Vec3(v) => format!(
            "vec3({}, {}, {})",
            format_float(v[0]),
            format_float(v[1]),
            format_float(v[2])
        ),
        LpsValueF32::Vec4(v) => format!(
            "vec4({}, {}, {}, {})",
            format_float(v[0]),
            format_float(v[1]),
            format_float(v[2]),
            format_float(v[3])
        ),
        LpsValueF32::IVec2(v) => format!("ivec2({}, {})", v[0], v[1]),
        LpsValueF32::IVec3(v) => format!("ivec3({}, {}, {})", v[0], v[1], v[2]),
        LpsValueF32::IVec4(v) => format!("ivec4({}, {}, {}, {})", v[0], v[1], v[2], v[3]),
        LpsValueF32::UVec2(v) => format!("uvec2({}u, {}u)", v[0], v[1]),
        LpsValueF32::UVec3(v) => format!("uvec3({}u, {}u, {}u)", v[0], v[1], v[2]),
        LpsValueF32::UVec4(v) => format!("uvec4({}u, {}u, {}u, {}u)", v[0], v[1], v[2], v[3]),
        LpsValueF32::BVec2(v) => format!("bvec2({}, {})", v[0], v[1]),
        LpsValueF32::BVec3(v) => format!("bvec3({}, {}, {})", v[0], v[1], v[2]),
        LpsValueF32::BVec4(v) => format!("bvec4({}, {}, {}, {})", v[0], v[1], v[2], v[3]),
        LpsValueF32::Mat2x2(m) => {
            format!(
                "mat2(vec2({}, {}), vec2({}, {}))",
                format_float(m[0][0]),
                format_float(m[0][1]),
                format_float(m[1][0]),
                format_float(m[1][1])
            )
        }
        LpsValueF32::Mat3x3(m) => {
            format!(
                "mat3(vec3({}, {}, {}), vec3({}, {}, {}), vec3({}, {}, {}))",
                format_float(m[0][0]),
                format_float(m[0][1]),
                format_float(m[0][2]),
                format_float(m[1][0]),
                format_float(m[1][1]),
                format_float(m[1][2]),
                format_float(m[2][0]),
                format_float(m[2][1]),
                format_float(m[2][2])
            )
        }
        LpsValueF32::Mat4x4(m) => {
            format!(
                "mat4(vec4({}, {}, {}, {}), vec4({}, {}, {}, {}), vec4({}, {}, {}, {}), vec4({}, {}, {}, {}))",
                format_float(m[0][0]),
                format_float(m[0][1]),
                format_float(m[0][2]),
                format_float(m[0][3]),
                format_float(m[1][0]),
                format_float(m[1][1]),
                format_float(m[1][2]),
                format_float(m[1][3]),
                format_float(m[2][0]),
                format_float(m[2][1]),
                format_float(m[2][2]),
                format_float(m[2][3]),
                format_float(m[3][0]),
                format_float(m[3][1]),
                format_float(m[3][2]),
                format_float(m[3][3])
            )
        }
        LpsValueF32::Array(items) => {
            if items.is_empty() {
                return "float[0]()".to_string();
            }
            let inner = items
                .iter()
                .map(format_glsl_value)
                .collect::<Vec<_>>()
                .join(", ");
            match glsl_array_type_prefix(items.first()) {
                Some(base) => format!("{}[{}]({})", base, items.len(), inner),
                None => format!("[{inner}]"),
            }
        }
        LpsValueF32::Struct { name, fields } => {
            let inner = fields
                .iter()
                .map(|(_, v)| format_glsl_value(v))
                .collect::<Vec<_>>()
                .join(", ");
            match name {
                Some(n) => format!("{n}({inner})"),
                None => format!("struct({inner})"),
            }
        }
        LpsValueF32::Texture2D(v) => format!(
            "/* Texture2D ptr={} {}x{} stride={} fmt={:?} bytes={} */",
            v.descriptor.ptr,
            v.descriptor.width,
            v.descriptor.height,
            v.descriptor.row_stride,
            v.format,
            v.byte_len
        ),
    }
}

fn glsl_array_type_prefix(elem: Option<&LpsValueF32>) -> Option<&'static str> {
    Some(match elem? {
        LpsValueF32::F32(_) => "float",
        LpsValueF32::I32(_) => "int",
        LpsValueF32::U32(_) => "uint",
        LpsValueF32::Bool(_) => "bool",
        LpsValueF32::Vec2(_) => "vec2",
        LpsValueF32::Vec3(_) => "vec3",
        LpsValueF32::Vec4(_) => "vec4",
        LpsValueF32::IVec2(_) => "ivec2",
        LpsValueF32::IVec3(_) => "ivec3",
        LpsValueF32::IVec4(_) => "ivec4",
        LpsValueF32::UVec2(_) => "uvec2",
        LpsValueF32::UVec3(_) => "uvec3",
        LpsValueF32::UVec4(_) => "uvec4",
        LpsValueF32::BVec2(_) => "bvec2",
        LpsValueF32::BVec3(_) => "bvec3",
        LpsValueF32::BVec4(_) => "bvec4",
        LpsValueF32::Mat2x2(_) => "mat2",
        LpsValueF32::Mat3x3(_) => "mat3",
        LpsValueF32::Mat4x4(_) => "mat4",
        LpsValueF32::Array(_) => return None,
        LpsValueF32::Struct { .. } => return None,
        LpsValueF32::Texture2D(_) => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::targets::Target;

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
    fn per_directive_unimplemented_present_scans_stacked_annotations() {
        let path = temp_glsl("stacked_ann");
        let content = r"// test run

float one() { return 1.0; }
// @unimplemented(rv32c.q32)
// @unimplemented(wasm.q32)
// run: one() ~= 1.0
";
        fs::write(&path, content).expect("write");
        let u = FileUpdate::new(&path);
        let wasm = Target::from_name("wasm.q32").expect("wasm");
        let rv32c = Target::from_name("rv32c.q32").expect("rv32c");
        assert!(
            u.per_directive_unimplemented_present(6, wasm)
                .expect("wasm")
        );
        assert!(
            u.per_directive_unimplemented_present(6, rv32c)
                .expect("rv32c")
        );
        let jit = Target::from_name("jit.q32").expect("jit");
        assert!(!u.per_directive_unimplemented_present(6, jit).expect("jit"));
    }

    #[test]
    fn remove_annotation_matching_target_strips_one_backend() {
        let path = temp_glsl("per_target");
        let content = r"// test run

float one() { return 1.0; }
// @unimplemented(wasm.q32)
// @unimplemented(rv32c.q32)
// run: one() ~= 1.0
";
        fs::write(&path, content).expect("write");
        let u = FileUpdate::new(&path);
        let wasm = Target::from_name("wasm.q32").expect("wasm");
        u.remove_annotation_matching_target(6, wasm)
            .expect("strip wasm");
        let out = fs::read_to_string(&path).expect("read");
        assert!(!out.contains("wasm.q32"), "wasm ann removed: {out}");
        assert!(out.contains("rv32c.q32"), "rv32c kept: {out}");
    }
}
