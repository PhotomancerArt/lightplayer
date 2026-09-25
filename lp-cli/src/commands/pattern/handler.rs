//! `lp-cli pattern preview`: render every (pattern, swatch) pair and write
//! one JSON per pair.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde_json::Value;

use super::args::{PatternCli, PatternSubcommand, PreviewArgs};
use super::knob_override::KnobOverride;
use super::pattern_source::PatternSource;
use super::preview_record;
use super::preview_render::{self, RecordedFrames};
use super::swatch::Swatch;
use super::swatch_rig::build_rig;

pub fn handle_pattern(cli: PatternCli) -> Result<()> {
    match cli.subcommand {
        PatternSubcommand::Preview(args) => handle_preview(args),
    }
}

fn handle_preview(args: PreviewArgs) -> Result<()> {
    let overrides = args
        .set
        .iter()
        .map(|text| KnobOverride::parse(text))
        .collect::<Result<Vec<_>>>()?;
    let swatches = args
        .swatches
        .iter()
        .map(|path| Swatch::read(path))
        .collect::<Result<Vec<_>>>()?;
    fs::create_dir_all(&args.out).with_context(|| format!("create {}", args.out.display()))?;

    let mut failures = 0usize;
    for pattern_dir in &args.patterns {
        let pattern = match PatternSource::read(pattern_dir) {
            Ok(pattern) => pattern,
            Err(e) => {
                eprintln!("{}: {e:#}", pattern_dir.display());
                failures += swatches.len();
                continue;
            }
        };
        for swatch in &swatches {
            let path = args
                .out
                .join(preview_record::file_name(&pattern.slug, &swatch.name));
            let record = match preview_one(&pattern, swatch, &overrides, args.seconds, args.fps) {
                Ok((record, frames)) => {
                    eprintln!(
                        "{:<16} {:<10} {:>4} lamps  {:>3} frames  peak {:>3}  {:>3} changing",
                        pattern.slug,
                        swatch.name,
                        swatch.lamp_count(),
                        frames.frame_count,
                        frames.peak(),
                        frames.changing_frames()
                    );
                    record
                }
                Err(e) => {
                    failures += 1;
                    let message = format!("{e:#}");
                    eprintln!("{:<16} {:<10} FAILED: {message}", pattern.slug, swatch.name);
                    preview_record::failure(
                        pattern.review_metadata(),
                        &pattern.float_mode(),
                        swatch,
                        &overrides,
                        args.seconds,
                        args.fps,
                        &message,
                    )
                }
            };
            write_json(&path, &record)?;
        }
    }

    eprintln!(
        "wrote {} preview(s) to {}",
        args.patterns.len() * swatches.len(),
        args.out.display()
    );
    if failures > 0 {
        anyhow::bail!("{failures} preview(s) failed; their JSONs carry the error");
    }
    Ok(())
}

/// Render one pattern on one swatch: the record to write, and the frames.
pub fn preview_one(
    pattern: &PatternSource,
    swatch: &Swatch,
    overrides: &[KnobOverride],
    seconds: f32,
    fps: u32,
) -> Result<(Value, RecordedFrames)> {
    let (fs, rig) = build_rig(pattern, swatch, overrides)?;
    let frames = preview_render::record(&fs, &pattern.slug, swatch.lamp_count(), seconds, fps)?;
    let record = preview_record::success(
        pattern.review_metadata(),
        &pattern.float_mode(),
        swatch,
        &rig,
        overrides,
        seconds,
        &frames,
    );
    Ok((record, frames))
}

fn write_json(path: &Path, value: &Value) -> Result<()> {
    let text = serde_json::to_string(value).context("serialize preview record")?;
    fs::write(path, text).with_context(|| format!("write {}", path.display()))
}
