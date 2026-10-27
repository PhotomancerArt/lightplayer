//! Map2d format 6: one fixture's mapping, its layout and its patching, as
//! one `*.map2d.json` file (G1, 2026-10-27: one file; patch files retire at
//! M3).
//!
//! ```json
//! {
//!   "format": 6,
//!   "sample_diameter": 2.0,
//!   "next": { "circle": 2 },
//!   "start": { "output": { "set": "main" }, "port": { "set": 0 },
//!              "channel": { "set": { "lamps": 0 } } },
//!   "parts": [ { "id": "circle1", "name": "display", "kind": { "circle": {
//!     "center": [50.0, 50.0], "start_angle_deg": -90.0, "dir": "cw",
//!     "drawn_from": "outside", "next_ring": 3,
//!     "rings": [ { "key": "r1", "radius": 42.0, "count": 24 },
//!                { "key": "r2", "radius": 0.0, "count": 1 } ] } } } ],
//!   "overrides": { "circle1/r2": { "port": { "set": 1 }, "direction": "reverse" } }
//! }
//! ```
//!
//! [`read`] reads it into a [`crate::part::Mapping`], refusing by name what
//! breaks its rules; [`write`] writes one canonical form. Today's reader,
//! `Map2dDoc::from_json`, still reads formats 1 to 5 and refuses 6, so the
//! engine is unchanged until M2; [`peek_format`] lets a caller choose.
//!
//! **Where the serde lives.** In this module, on document structs of its
//! own, converted to and from the model: the model's shapes are not the
//! file's (a path's commands are one `d` string; a port Set carries a
//! flow's `plus`; parts nest their kind), and the model stays free of serde
//! attributes. The polygon, grid and filled polygon reuse today's shape
//! structs, whose deserializers every firmware already links, and the small
//! unit enums whose words are the file's ([`crate::part::PathSplit`],
//! [`crate::part::CircleSplit`], [`crate::patching::Direction`]) derive
//! serde themselves. Nothing here is linked into firmware until M2.

mod map2d6_doc;
mod map2d6_error;
mod numbered_id;
mod op_doc;
mod override_doc;
mod part_doc;
mod path_data;
mod read;
mod write;

pub use map2d6_error::Map2d6Error;
pub use read::{MAP2D6_FORMAT, peek_format, read};
pub use write::write;
