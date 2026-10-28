//! Generates the **radiance dome** test project
//! (`lab/mapping/projects/radiance-dome/`) in the product's own on-disk
//! formats: a project manifest, one root module, a clock, a shader, one
//! fixture whose `map2d` mapping (format 6) holds its layout and its patch,
//! and ten outputs.
//!
//! Why it exists: the mapping design lab needs a project at the radiance
//! dome's scale (275 panels, tens of thousands of lamps, ten control boxes)
//! to test journeys against, and no such project is on disk. lp2014's
//! `radiance.json` is the 2017 one-light-per-panel install, not the
//! wrapped-panel dome (fit-check §7).
//!
//! The geometry is lp2014's: a frequency-5 geodesic icosahedral dome, the
//! top 8 face rings lit (275 panels), drawn as a plan view in the
//! azimuthal equidistant projection (`GeodesicIcosahedralFixture.remap()`;
//! see [`dome_geometry`]). The plan is exactly 5-fold symmetric about the
//! zenith, so the map is 55 panel classes, each a 5-way repeat of one
//! polygon part.
//!
//! **The lamp counts, the port counts and the patch are PLACEHOLDERS**
//! (fit-check Q-E). The real unknowns: how many LEDs each panel carries
//! (one count, or separate pentagonal/hexagonal counts as lp2014 allowed),
//! whether 7 or 8 rings are lit, and which panel is plugged into which box
//! and port. Until an as-built patch turns up, every panel carries
//! [`PANEL_LAMPS`] lamps and panel `n` (sector-major, see [`patch_plan`])
//! goes to output `n / 32 + 1`, port `n % 32`, with two invented install
//! quirks so the patch is not a pure identity.
//!
//! Run from anywhere in the workspace; the optional argument is the output
//! directory (default: the checked-in project):
//!
//! ```bash
//! cargo run -p lab-mapping-radiance-gen [-- <out-dir>]
//! ```
//!
//! Deterministic: the same source writes the same bytes.

mod dome_geometry;
mod frozen_patching;
mod json_text;
mod patch_plan;
mod project_files;

use std::path::PathBuf;

/// Lamps per panel — a placeholder (fit-check Q-E).
pub const PANEL_LAMPS: u32 = 80;
/// Output nodes (control boxes) — a placeholder.
pub const OUTPUTS: usize = 10;
/// Ports per output — a placeholder.
pub const PORTS_PER_OUTPUT: usize = 32;

fn main() {
    let out_dir = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../projects/radiance-dome")
        });
    std::fs::create_dir_all(&out_dir).expect("create the project directory");
    let out_dir = out_dir
        .canonicalize()
        .expect("the project directory resolves");
    for (name, text) in project_files::project_files() {
        let path = out_dir.join(&name);
        std::fs::write(&path, &text).expect("write a project file");
        println!("wrote {} ({} bytes)", path.display(), text.len());
    }
}
