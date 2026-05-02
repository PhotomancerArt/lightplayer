//! M4.1 compatibility helpers (`legacy`/`compatibility` naming for M4.5 discovery).

mod legacy_compat_bytes;

pub use legacy_compat_bytes::{
    LEGACY_COMPAT_RESOURCE_STR_PREFIX, LegacyCompatBytesBody, LegacyCompatBytesField,
    encode_legacy_compat_resource_str,
};
