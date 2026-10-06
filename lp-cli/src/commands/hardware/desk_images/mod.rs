//! `lp-cli hardware desk-images`: pictures for the desk's board bench.
//!
//! The bench (`board`, github.com/PhotomancerArt/lp-board-bench) is MIT and
//! draws nothing; LightPlayer's drawing code is AGPL. So the pictures cross
//! as files: this writes `<home>/images/<MAC>.board.svg` (the board, drawn as
//! Studio draws it) for every registered board with an `lp_board`, and
//! `<MAC>.art.svg` (a frame of the piece) for every board with an
//! `lp_project`, and the bench's page shows them.

#[cfg(feature = "desk-images")]
mod art_svg;
#[cfg(feature = "desk-images")]
mod board_svg;

use anyhow::Result;

use super::args::DeskImagesArgs;

#[cfg(not(feature = "desk-images"))]
pub fn handle_desk_images(_args: DeskImagesArgs) -> Result<()> {
    anyhow::bail!(
        "this lp-cli was built without the `desk-images` feature; run `just desk-images` \
         (or `cargo run -p lp-cli --features desk-images -- hardware desk-images`)"
    )
}

#[cfg(feature = "desk-images")]
pub fn handle_desk_images(args: DeskImagesArgs) -> Result<()> {
    use std::path::PathBuf;

    use anyhow::{Context, bail};

    use crate::client::board_bench;

    let home = args
        .home
        .or_else(|| {
            std::env::var_os("BOARD_HOME")
                .filter(|home| !home.is_empty())
                .map(PathBuf::from)
        })
        .or_else(|| dirs_home().map(|home| home.join(".photomancer").join("desk")))
        .context("no home directory; pass --home")?;
    let images = home.join("images");
    std::fs::create_dir_all(&images).with_context(|| format!("create {}", images.display()))?;

    let Some(boards) = board_bench::list() else {
        bail!(
            "`board list --json` gave nothing to draw (is `board` installed? github.com/PhotomancerArt/lp-board-bench)"
        );
    };
    let mut failures = 0;
    for board in &boards {
        let (Some(slug), Some(mark), Some(mac)) = (&board.slug, &board.mark, &board.mac) else {
            continue; // not registered
        };
        if args.only.as_deref().is_some_and(|only| only != slug) {
            continue;
        }
        let mut done = Vec::new();
        if let Some(lp_board) = &board.lp_board {
            match board_svg::board_svg(lp_board) {
                Ok(svg) => {
                    write_atomic(&images.join(format!("{mac}.board.svg")), &svg)?;
                    done.push("board.svg ok".to_owned());
                }
                Err(err) => {
                    failures += 1;
                    done.push(format!("board.svg FAILED: {err:#}"));
                }
            }
        }
        if let Some(lp_project) = &board.lp_project {
            match art_svg::art_svg(&project_path(lp_project), args.time) {
                Ok(svg) => {
                    write_atomic(&images.join(format!("{mac}.art.svg")), &svg)?;
                    done.push("art.svg ok".to_owned());
                }
                Err(err) => {
                    failures += 1;
                    done.push(format!("art.svg FAILED: {err:#}"));
                }
            }
        }
        if !done.is_empty() {
            println!("{mark} {slug}: {}", done.join(", "));
        }
    }
    if failures > 0 {
        bail!("{failures} picture(s) could not be drawn");
    }
    Ok(())
}

/// An `lp_project` as written in the registry: absolute, else relative to
/// the current directory, else to the checkout this lp-cli was built from
/// (so `catalog/projects/playful-choker` works from anywhere).
#[cfg(feature = "desk-images")]
fn project_path(lp_project: &str) -> std::path::PathBuf {
    let path = std::path::PathBuf::from(lp_project);
    if path.is_absolute() || path.exists() {
        return path;
    }
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(path)
}

#[cfg(feature = "desk-images")]
fn write_atomic(path: &std::path::Path, text: &str) -> Result<()> {
    use anyhow::Context;
    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    std::fs::write(&tmp, text).with_context(|| format!("write {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("rename onto {}", path.display()))?;
    Ok(())
}

#[cfg(feature = "desk-images")]
fn dirs_home() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .map(std::path::PathBuf::from)
}
