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
//! **Reading is by hand, writing is serde.** The device reads these files
//! at project load, so the reader is what firmware links: a small JSON
//! reader that is not generic (`json_text.rs`) and one short loop per
//! document struct (`doc_read.rs`), straight into the model's types where a
//! value has no rules to check. Derived deserializers cost the C6 engine
//! about 30 KB more (one visitor, field enum and error path per struct and
//! enum, monomorphized over serde_json's reader); the hand reader reads and
//! refuses what they did, apart from serde's positional form of a struct (a
//! JSON array of its fields), which no format-6 document uses and this
//! reader refuses. The writer stays on serde, on document structs of its
//! own (`map2d6_doc.rs` and its parts), which only hosts link: the model's
//! shapes are not the file's (a path's commands are one `d` string; a port
//! Set carries a flow's `plus`; parts nest their kind), and the model stays
//! free of serde attributes.

mod doc_read;
mod json_text;
mod map2d6_doc;
mod map2d6_error;
mod numbered_id;
mod op_doc;
mod override_doc;
mod part_doc;
mod path_data;
mod read;
mod write;

pub use map2d6_error::{DEVICE_REFUSAL, Map2d6Error};
pub use read::{MAP2D6_FORMAT, peek_format, read};
pub use write::write;
