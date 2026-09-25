use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "pattern",
    about = "Work with catalog patterns (kind: pattern projects) on the host."
)]
pub struct PatternCli {
    #[command(subcommand)]
    pub subcommand: PatternSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum PatternSubcommand {
    /// Render patterns on standard lamp shapes ("swatches") and record the
    /// per-lamp colours as JSON, one file per (pattern, swatch).
    ///
    /// Each run builds an in-memory project from the pattern's exported
    /// module(s) plus a swatch rig — the pattern's own clock, a fixture that
    /// keeps the pattern's render size and sampling but maps the swatch's
    /// lamps (brightness 1, no gamma), and an output — and ticks it on the
    /// host engine. What is recorded is the output's control samples, which
    /// is the fixture's colour before any board-side white point or LUT.
    ///
    /// The JSONs feed `scripts/pattern-review/build-page.mjs`
    /// (`just pattern-review` runs both).
    ///
    /// Exit code 1 when any (pattern, swatch) failed; the failure is still
    /// written as a JSON carrying `error`, so a review page shows it.
    #[command(verbatim_doc_comment)]
    Preview(PreviewArgs),
}

#[derive(Debug, Args)]
pub struct PreviewArgs {
    /// Pattern project directories (e.g. catalog/patterns/plasma).
    #[arg(required = true)]
    pub patterns: Vec<PathBuf>,

    /// Swatch mapping files (`*.map2d.json`); the file stem names the
    /// swatch. Repeat the flag for several.
    #[arg(long = "swatch", required = true)]
    pub swatches: Vec<PathBuf>,

    /// Directory the JSONs are written to (created if missing).
    #[arg(long, default_value = "target/pattern-review/frames")]
    pub out: PathBuf,

    /// Seconds of animation to record.
    #[arg(long, default_value_t = 5.0)]
    pub seconds: f32,

    /// Frames per second to record.
    #[arg(long, default_value_t = 24)]
    pub fps: u32,

    /// Override a knob's default: `<slot or bus channel>=<number>`, e.g.
    /// `--set tail=0.3` or `--set bus:tail=0.3`. Applies to every `value`
    /// slot with that name or bound to that bus channel. Repeatable.
    #[arg(long = "set", value_name = "KNOB=VALUE")]
    pub set: Vec<String>,
}
