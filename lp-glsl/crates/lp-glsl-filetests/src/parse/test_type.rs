//! Test type enum and related types.

/// Test type directive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestType {
    /// `test compile` - verify CLIF IR before transformations
    Compile,
    /// `test transform.q32` - verify CLIF IR after q32 transformation
    TransformQ32,
    /// `test run` - execute and verify results
    Run,
}

/// CLIF expectations extracted from test file comments.
#[derive(Debug, Clone, Default)]
pub struct ClifExpectations {
    /// Pre-transform CLIF (for `test compile`).
    pub pre_transform: Option<String>,
    /// Post-transform q32 CLIF (for `test transform.q32`).
    pub post_transform_q32: Option<String>,
}

/// Comparison operator for run directives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComparisonOp {
    /// Exact equality (`==`).
    Exact,
    /// Approximate equality with tolerance (`~=`).
    Approx,
}

/// A run directive parsed from a `// run:` line.
#[derive(Debug, Clone)]
pub struct RunDirective {
    /// The original expression string (e.g., "add_float(0.0, 0.0)").
    pub expression_str: String,
    /// The comparison operator.
    pub comparison: ComparisonOp,
    /// The expected value string (e.g., "0.0").
    pub expected_str: String,
    /// Custom tolerance for approximate comparisons (None = use default).
    pub tolerance: Option<f32>,
    /// Line number for bless mode updates.
    pub line_number: usize,
    /// Whether this test is marked as expected to fail with `[expect-fail]`.
    pub expect_fail: bool,
}

/// A trap expectation parsed from a `// EXPECT_TRAP:` or `// EXPECT_TRAP_CODE:` line.
#[derive(Debug, Clone, PartialEq)]
pub struct TrapExpectation {
    /// Expected trap code (TrapCode::user(n) value), if specified.
    pub trap_code: Option<u8>,
    /// Expected trap message substring, if specified.
    pub trap_message: Option<String>,
    /// Line number for this expectation.
    pub line_number: usize,
}

/// A parsed test file.
pub struct TestFile {
    /// The original source code (with directives filtered out for compilation).
    pub glsl_source: String,
    /// All run directives found in the file.
    pub run_directives: Vec<RunDirective>,
    /// All trap expectations found in the file.
    pub trap_expectations: Vec<TrapExpectation>,
    /// Target specification (e.g., "riscv32.q32").
    pub target: Option<String>,
    /// Whether this is a "test run" file.
    pub is_test_run: bool,
    /// Test types requested in this file.
    pub test_types: Vec<TestType>,
    /// CLIF expectations extracted from comments.
    pub clif_expectations: ClifExpectations,
}
