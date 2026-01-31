//! Extract GLSL source and CLIF expectations.

use crate::parse::test_type::{ClifExpectations, TestType};
use anyhow::Result;

/// Section boundaries in a test file.
#[derive(Debug, Clone, Copy)]
pub struct SectionBoundaries {
    /// Line index where GLSL source ends (exclusive).
    pub glsl_end: usize,
    /// Line index where run expectations start.
    pub run_start: usize,
}

/// CLIF section boundaries within the expectations section.
#[derive(Debug, Clone, Copy)]
pub struct ClifSectionBoundaries {
    /// Start of compile section (inclusive).
    pub compile_start: Option<usize>,
    /// End of compile section (exclusive).
    pub compile_end: Option<usize>,
    /// Start of transform section (inclusive).
    pub transform_start: Option<usize>,
    /// End of transform section (exclusive).
    pub transform_end: Option<usize>,
}

/// Find CLIF section boundaries within the expectations section.
/// `expectations_start` and `expectations_end` define the range to search within.
/// TODO: Implement when CLIF tests are implemented.
pub fn find_clif_section_boundaries(
    _lines: &[String],
    _expectations_start: usize,
    _expectations_end: usize,
) -> ClifSectionBoundaries {
    // TODO: Implement CLIF section boundary detection when CLIF tests are implemented
    ClifSectionBoundaries {
        compile_start: None,
        compile_end: None,
        transform_start: None,
        transform_end: None,
    }
}

/// Find section boundaries in a test file.
/// Returns the end of GLSL code and the start of run expectations.
pub fn find_section_boundaries(lines: &[String]) -> SectionBoundaries {
    // Find the end of GLSL code (last non-comment, non-directive line)
    let mut glsl_end = 0;
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if !trimmed.is_empty()
            && !trimmed.starts_with("//")
            && !trimmed.starts_with("// test")
            && !trimmed.starts_with("// target")
            && !trimmed.starts_with("// run:")
        {
            glsl_end = i + 1;
        }
    }

    // Find the start of run expectations (first `// #run:` or `// run:` after GLSL)
    let mut run_start = lines.len();
    for (i, line) in lines.iter().enumerate().skip(glsl_end) {
        let trimmed = line.trim();
        if trimmed.starts_with("// #run:") || trimmed.starts_with("// run:") {
            run_start = i;
            break;
        }
    }

    SectionBoundaries {
        glsl_end,
        run_start,
    }
}

/// Extract GLSL source code and CLIF expectations from file lines.
pub fn extract_source_and_expectations(
    lines: &[String],
    test_types: &[TestType],
) -> Result<(String, ClifExpectations)> {
    let mut glsl_source = String::new();
    let mut clif_expectations = ClifExpectations::default();

    // Find section boundaries
    let boundaries = find_section_boundaries(lines);
    let glsl_end = boundaries.glsl_end;
    let run_start = boundaries.run_start;

    // Extract GLSL source (everything before expectations)
    for line in lines.iter().take(glsl_end) {
        let trimmed = line.trim();
        // Skip directives
        if trimmed.starts_with("// test") || trimmed.starts_with("// target") {
            continue;
        }
        glsl_source.push_str(line);
        glsl_source.push('\n');
    }

    // Extract CLIF expectations from the section between GLSL and run expectations
    if run_start > glsl_end {
        let clif_section: Vec<&String> = lines[glsl_end..run_start].iter().collect();
        clif_expectations = parse_clif_expectations(&clif_section, test_types)?;
    }

    Ok((glsl_source, clif_expectations))
}

/// Local CLIF section boundaries (relative to the expectations section).
#[derive(Debug, Clone, Copy)]
struct LocalClifBoundaries {
    compile_start: Option<usize>,
    compile_end: Option<usize>,
    transform_start: Option<usize>,
    transform_end: Option<usize>,
}

/// Parse CLIF expectations from comment lines.
fn parse_clif_expectations(lines: &[&String], test_types: &[TestType]) -> Result<ClifExpectations> {
    let mut expectations = ClifExpectations::default();

    // Find CLIF section boundaries (these are local indices within the expectations section)
    let clif_boundaries = find_local_clif_boundaries(lines, test_types);

    // Extract sections based on boundaries
    if let Some(start) = clif_boundaries.compile_start {
        let end = clif_boundaries.compile_end.unwrap_or(lines.len());
        if test_types.contains(&TestType::Compile) {
            expectations.pre_transform = Some(extract_clif_from_lines(&lines[start..end]));
        }
    }

    if let Some(start) = clif_boundaries.transform_start {
        let end = clif_boundaries.transform_end.unwrap_or(lines.len());
        if test_types.contains(&TestType::TransformQ32) {
            expectations.post_transform_q32 = Some(extract_clif_from_lines(&lines[start..end]));
        }
    }

    Ok(expectations)
}

/// Find CLIF section boundaries within the expectations section (local indices).
fn find_local_clif_boundaries(lines: &[&String], test_types: &[TestType]) -> LocalClifBoundaries {
    let mut compile_start = None;
    let mut compile_end = None;
    let mut transform_start = None;
    let mut transform_end = None;

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("// #compile:") {
            compile_start = Some(i + 1);
        } else if trimmed.starts_with("// #transform:") {
            if compile_start.is_some() && compile_end.is_none() {
                compile_end = Some(i);
            }
            transform_start = Some(i + 1);
        } else if trimmed == "//" {
            if compile_start.is_some() && compile_end.is_none() {
                compile_end = Some(i);
            } else if transform_start.is_some() && transform_end.is_none() {
                transform_end = Some(i);
            }
        }
    }

    // If no markers found, try to split by blank comment lines
    if compile_start.is_none() && transform_start.is_none() {
        let has_compile = test_types.contains(&TestType::Compile);
        let has_transform = test_types.contains(&TestType::TransformQ32);

        if has_compile && has_transform {
            // Split by blank comment line
            if let Some(blank) = lines.iter().position(|l| l.trim() == "//") {
                compile_end = Some(blank);
                transform_start = Some(blank + 1);
            } else if has_compile {
                compile_start = Some(0);
            } else if has_transform {
                transform_start = Some(0);
            }
        } else if has_compile {
            compile_start = Some(0);
        } else if has_transform {
            transform_start = Some(0);
        }
    }

    LocalClifBoundaries {
        compile_start,
        compile_end,
        transform_start,
        transform_end,
    }
}

/// Extract CLIF text from comment lines (removing `//` prefix).
fn extract_clif_from_lines(lines: &[&String]) -> String {
    let mut result = String::new();
    for line in lines {
        let trimmed = line.trim();
        if trimmed == "//" {
            result.push('\n');
        } else if let Some(clif_line) = trimmed.strip_prefix("// ") {
            result.push_str(clif_line);
            result.push('\n');
        } else if trimmed.starts_with("//") {
            // Handle lines that are just "//" or have content after "//"
            let clif_line = trimmed.strip_prefix("//").unwrap_or(trimmed).trim();
            if !clif_line.is_empty() {
                result.push_str(clif_line);
                result.push('\n');
            } else {
                result.push('\n');
            }
        }
    }
    result.trim_end().to_string()
}
