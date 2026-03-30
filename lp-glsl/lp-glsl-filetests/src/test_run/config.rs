//! Filetest runner configuration toggles.

/// When true, summary mode compiles once per `// run:` directive (same GLSL slicing as detail mode)
/// instead of compiling the full translation unit once. Set to `false` when the compiler reliably
/// handles mixed-feature files so suite runs can use a single compile per file again.
///
/// For a runtime toggle without rebuilding, an environment variable could be added here later.
pub const SUMMARY_COMPILE_PER_DIRECTIVE: bool = true;
