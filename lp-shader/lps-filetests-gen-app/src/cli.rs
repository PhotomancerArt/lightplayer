//! CLI argument parsing.

use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "lps-filetests-gen-app")]
#[command(about = "Generate lps vector and matrix test files")]
pub struct Args {
    /// Test file specifier(s) (e.g., "vec/vec4/fn-equal", "vec/vec3", or "vec/vec4/fn-equal.gen.glsl")
    /// Supports multiple specifiers, directory patterns, and .gen.glsl file paths
    pub specifiers: Vec<String>,

    /// Write files to disk (default: dry-run, print to stdout)
    #[arg(long)]
    pub write: bool,

    /// Verify the checked-in files match what this generator produces, and exit
    /// nonzero on any drift. With no specifiers this checks the whole `vec`
    /// corpus, which is what the `lint-vec-corpus` gate wants.
    #[arg(long, conflicts_with = "write")]
    pub check: bool,
}

pub fn parse_args() -> Args {
    Args::parse()
}
