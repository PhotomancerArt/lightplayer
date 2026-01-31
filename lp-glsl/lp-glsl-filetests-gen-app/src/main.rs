//! Generator for lp-glsl-compiler vector and matrix test files.

use anyhow::Result;

mod cli;
mod expand;
mod generator;
mod types;
mod util;
mod vec;

fn main() -> Result<()> {
    let args = cli::parse_args();
    generator::generate(&args)
}
