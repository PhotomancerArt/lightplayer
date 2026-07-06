//! Prefixed base-62 identifiers for packages and devices.

pub mod prefixed_uid;
pub mod uid_prefix;

pub use prefixed_uid::{PrefixedUid, UidParseError};
pub use uid_prefix::UidPrefix;
