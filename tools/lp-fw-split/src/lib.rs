//! The ESP32-C6 split image, built from one link.
//!
//! `fw-esp32c6` is linked twice. Pass 1 links it whole, with relocations
//! and a map; the link's input sections and relocations become a graph, and
//! everything the core's roots reach is the **core** — the rest is the
//! **engine**, the shader compiler included. A generated linker script then
//! places every engine section at the engine's own address, and pass 2
//! links the same program again under it. The verifier checks pass 2: no
//! core section may sit in the engine region.
//!
//! The pass-2 ELF is then cut into `core.bin` (an ESP application image the
//! loader boots) and `engine.bin` (raw bytes the core maps), the loader is
//! built, and everything is laid out for the app partition (`app.bin`) and
//! for the whole chip (`merged.bin`) with `lp-bootctl`'s own format and
//! constants. See `README.md`.

pub mod app_image;
pub mod engine_script;
pub mod merged_image;
pub mod pass_link;
pub mod reachability;
pub mod section_graph;
pub mod split_artifacts;
pub mod split_build;
pub mod tree_guard;
pub mod verify_report;

pub use split_build::{BuildOptions, SplitReport, build, verify};
