//! The on-flash format, pinned: a fixed tree on a fixed geometry, and the
//! flash image's bytes compared with `format_golden.hex` (each sector's
//! bytes up to its last non-`0xFF` byte, 64 per line).
//!
//! A mismatch is a format change. Never re-record the golden to make a
//! change pass: a deliberate change bumps `FORMAT_VERSION` (FORMAT.md
//! "Versioning") and re-records with a commit message saying so
//! (`LP_TREE_STORE_RECORD_GOLDEN=1 cargo test -p lp-tree-store --test
//! format_golden`).

use std::fmt::Write as _;

use lp_nor_sim::{NorFlashSim, NorGeometry};
use lp_tree_store::{FORMAT_VERSION, SoftSha256, StoreConfig, TreeStore};

const GOLDEN: &str = include_str!("format_golden.hex");

/// 672 bytes of shader text, raw-deflated once (Python's zlib, level 9) and
/// pinned here so the golden never depends on an encoder's version.
const SHADER_DEFLATED: &str = "2bcdcb4ccb2fca5548cbc94f2c51282e484d4db1e62a4b4d365648cecfc92f52b0550071340cf48c4c75140cf48084a19e81a63557e9a8be517d74d00700";

fn shader() -> Vec<u8> {
    b"uniform float speed;\nvec3 color = vec3(0.25, 0.5, 1.0);\n".repeat(12)
}

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

/// The fixed tree: every record kind, both codecs, a multi-part file, a
/// multi-part directory, the hot directory, a transaction, an append and a
/// delete.
fn image() -> String {
    let cfg = StoreConfig {
        record_max: 256,
        ..StoreConfig::default()
    };
    let mut f = NorFlashSim::new(NorGeometry::c6(8));
    TreeStore::format(&mut f, &mut SoftSha256, &cfg).unwrap();
    let Ok(mut st) = TreeStore::mount(f, SoftSha256, cfg.clone()) else {
        panic!("mount")
    };
    st.put("/hardware.json", b"{\"board\": \"c6\"}").unwrap();
    st.begin().unwrap();
    st.put("/projects/a/project.json", b"{\"name\": \"a\"}").unwrap();
    let big: Vec<u8> = (0..1200u32).map(|i| (i * 7 % 251) as u8).collect();
    st.put("/projects/a/big.bin", &big).unwrap();
    st.put("/projects/a/.lp/panel.json", b"{\"speed\": 1}").unwrap();
    st.put_chunk_deflated("/projects/a/s.glsl", 0, 672, None, &unhex(SHADER_DEFLATED))
        .unwrap();
    for i in 0..12 {
        st.put(&format!("/projects/a/m/module-{i:02}.json"), b"{}").unwrap();
    }
    st.commit().unwrap();
    st.append("/projects/a/project.json", b"\n").unwrap();
    st.delete("/hardware.json").unwrap();
    assert_eq!(st.get("/projects/a/s.glsl").unwrap().unwrap(), shader());
    let st = match TreeStore::mount(st.into_flash(), SoftSha256, cfg) {
        Ok(s) => s,
        Err(_) => panic!("remount"),
    };
    let flash = st.into_flash();
    let mut out = format!("# lp-tree-store FORMAT_VERSION {FORMAT_VERSION}, 8 x 4096, record_max 256\n");
    for s in 0..8u32 {
        let mut cells = vec![0u8; 4096];
        flash.peek(s * 4096, &mut cells);
        let end = cells.iter().rposition(|&b| b != 0xFF).map_or(0, |p| p + 1);
        writeln!(out, "sector {s} {end}").unwrap();
        for line in cells[..end].chunks(64) {
            for b in line {
                write!(out, "{b:02x}").unwrap();
            }
            out.push('\n');
        }
    }
    out
}

#[test]
fn the_flash_image_is_byte_for_byte_the_golden() {
    let got = image();
    if std::env::var_os("LP_TREE_STORE_RECORD_GOLDEN").is_some() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/format_golden.hex");
        std::fs::write(path, &got).unwrap();
        return;
    }
    if got != GOLDEN {
        let line = got
            .lines()
            .zip(GOLDEN.lines())
            .position(|(a, b)| a != b)
            .map_or(String::from("(length)"), |i| format!("line {}", i + 1));
        panic!(
            "the on-flash format moved (first difference at {line}). This is a format change: \
             bump FORMAT_VERSION and re-record deliberately, never to make a change pass."
        );
    }
}
