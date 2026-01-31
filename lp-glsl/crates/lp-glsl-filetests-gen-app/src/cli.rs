//! CLI argument parsing.

use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "lp-glsl-filetests-gen-app")]
#[command(about = "Generate lp-glsl-compiler vector and matrix test files")]
pub struct Args {
    /// Test file specifier(s) (e.g., "vec/vec4/fn-equal", "vec/vec3", or "vec/vec4/fn-equal.gen.glsl")
    /// Supports multiple specifiers, directory patterns, and .gen.glsl file paths
    pub specifiers: Vec<String>,

    /// Write files to disk (default: dry-run, print to stdout)
    #[arg(long)]
    pub write: bool,
}

pub fn parse_args() -> Args {
    Args::parse()
}
