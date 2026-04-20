//! Arguments for `shader-debug`.

use std::path::PathBuf;

use clap::Parser;
use lps_shared::TextureStorageFormat;

use super::types::{BackendTarget, SectionFilter};

#[derive(Debug, Parser)]
#[command(about = "Unified shader debug output for all backends")]
pub struct Args {
    /// Path to GLSL source
    pub input: PathBuf,

    /// Comma-separated list of targets (rv32n, rv32c, emu)
    #[arg(short, long, default_value = "rv32n")]
    pub target: String,

    /// Function name to filter (shows all by default)
    #[arg(id = "fn", long = "fn", value_name = "NAME")]
    pub func: Option<String>,

    /// Floating point mode
    #[arg(long, default_value = "q32", value_name = "MODE")]
    pub float_mode: String,

    /// Show LPIR section
    #[arg(long)]
    pub lpir: bool,

    /// Show VInst/interleaved section
    #[arg(long)]
    pub vinst: bool,

    /// Show assembly/disasm section
    #[arg(long)]
    pub asm: bool,

    /// Summary only - don't show detailed function output
    #[arg(long)]
    pub summary: bool,

    /// Synthesise `__render_texture_<format>` and include it in output.
    ///
    /// The synth function is the loop wrapper that calls the user's
    /// `render(vec2 pos)` once per pixel and stores results into a texture
    /// buffer. Inspecting its LPIR / asm is essential for debugging
    /// per-pixel iteration bugs.
    ///
    /// Values: `none`, `r16`, `rgb16`, `rgba16`, `all`. Default: `rgba16`.
    /// If the input has no `render` function the flag is silently ignored.
    #[arg(long, default_value = "rgba16", value_name = "FORMAT")]
    pub render_texture: String,

    /// Override compiler options. Format: `key=value`. Repeatable.
    /// Use `--opt` alone (no value) to print valid keys and values.
    /// Example: `-o q32.mul=wrapping -o inline.mode=never`.
    #[arg(
        short = 'o',
        long = "opt",
        value_name = "KEY=VALUE",
        action = clap::ArgAction::Append,
        num_args = 0..=1,
        default_missing_value = "",
    )]
    pub opt: Vec<String>,
}

impl Args {
    /// Parse the targets string into a list of BackendTarget.
    ///
    /// Supports comma-separated targets like "rv32c,rv32n"
    pub fn targets(&self) -> Vec<BackendTarget> {
        self.target
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.parse::<BackendTarget>())
            .filter_map(Result::ok)
            .collect()
    }

    /// Parse `--render-texture` into the set of formats to synthesise.
    ///
    /// Returns `Err` for unknown values. An empty Vec means "no synth".
    pub fn render_texture_formats(&self) -> Result<Vec<TextureStorageFormat>, String> {
        match self.render_texture.as_str() {
            "none" => Ok(Vec::new()),
            "r16" => Ok(vec![TextureStorageFormat::R16Unorm]),
            "rgb16" => Ok(vec![TextureStorageFormat::Rgb16Unorm]),
            "rgba16" => Ok(vec![TextureStorageFormat::Rgba16Unorm]),
            "all" => Ok(vec![
                TextureStorageFormat::R16Unorm,
                TextureStorageFormat::Rgb16Unorm,
                TextureStorageFormat::Rgba16Unorm,
            ]),
            other => Err(format!(
                "invalid --render-texture value {other:?}; expected one of: none, r16, rgb16, rgba16, all"
            )),
        }
    }

    /// Determine which sections to show.
    ///
    /// Default behavior (no flags): show all sections
    /// With explicit flags: show only selected sections
    pub fn sections(&self) -> SectionFilter {
        let any_explicit = self.lpir || self.vinst || self.asm;

        if any_explicit {
            SectionFilter {
                lpir: self.lpir,
                vinst: self.vinst,
                asm: self.asm,
            }
        } else {
            SectionFilter::all()
        }
    }
}
