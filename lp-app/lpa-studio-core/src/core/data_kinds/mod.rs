//! Data kinds: one way to say each kind of data Studio shows (P17,
//! `docs/ux/README.md`). Every surface that names an age, a byte size, a
//! percentage, a date or a firmware version says it through the one rule
//! here, so the same fact never reads two ways on one screen.
//!
//! - [`age`]: "12 s ago", the duration "2 h", and in a sentence "2 hours
//!   ago".
//! - [`byte_size`]: "512 B", "1.5 KB", "38 KB", "4.2 MB".
//! - [`percent`]: "42%".
//! - [`calendar_date`]: "Oct 7" (and "Oct 7, 2026" where the year is the
//!   fact).
//! - [`firmware_version`]: "v2026.10.16-1", "dev c688c99", "unknown".
//!
//! The words are the rule today. A structured value with one web component
//! per kind (each kind drawn by its own component, the story beside it) is
//! the next step, held in `lp2025/2026-10-16-1844-card-words-round-2`'s
//! notes; the web's "data kinds" story shows every kind and its cases.

pub mod age;
pub mod byte_size;
pub mod calendar_date;
pub mod firmware_version;
pub mod percent;

pub use age::{age_spoken, age_words, duration_words};
pub use byte_size::byte_size;
pub use calendar_date::{CalendarDate, date_words};
pub use firmware_version::{
    FIRMWARE_NONE, FIRMWARE_UNKNOWN, PRE_HELLO_FIRMWARE, firmware_label_version, firmware_version,
    pre_ota_version,
};
pub use percent::{percent, percent_of};
