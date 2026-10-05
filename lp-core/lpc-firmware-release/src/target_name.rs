//! A target: a line of builds (`esp32c6-4mb`).

use alloc::string::{String, ToString};
use core::fmt;

/// Longest target name, in bytes (`[a-z0-9][a-z0-9-]{0,63}`).
pub const TARGET_NAME_MAX_LEN: usize = 64;

/// A target name: today's `lp-fw/builds/` id, e.g. `esp32c6-4mb`.
///
/// **Opaque** (doors #1, #2): it is never parsed for a chip, a flash size or
/// a variant, and a target is never renamed. Facts about a target (its chip,
/// its layout) are fields beside it, never pieces of it — `esp32v3-4mb`'s
/// chip is `esp32`, and a board with 8 MB of flash may run `esp32c6-4mb`.
///
/// Grammar: `[a-z0-9][a-z0-9-]{0,63}`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TargetName(String);

impl TargetName {
    /// Parse a target name, or `None` when it is outside the grammar.
    pub fn parse(s: &str) -> Option<Self> {
        is_target_name(s).then(|| Self(s.to_string()))
    }

    /// The name as written.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TargetName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// True when `s` is a target name (`[a-z0-9][a-z0-9-]{0,63}`).
pub fn is_target_name(s: &str) -> bool {
    let bytes = s.as_bytes();
    let Some(first) = bytes.first() else {
        return false;
    };
    bytes.len() <= TARGET_NAME_MAX_LEN
        && (first.is_ascii_lowercase() || first.is_ascii_digit())
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_todays_targets() {
        for name in ["esp32c6-4mb", "esp32s3-8mb", "esp32v3-4mb", "x", "9"] {
            assert_eq!(TargetName::parse(name).unwrap().as_str(), name);
        }
    }

    #[test]
    fn refuses_outside_the_grammar() {
        let long = "a".repeat(TARGET_NAME_MAX_LEN + 1);
        for name in [
            "",
            "-esp32",
            "ESP32C6",
            "esp32c6_4mb",
            "esp32c6.4mb",
            "a/b",
            &long,
        ] {
            assert!(TargetName::parse(name).is_none(), "{name:?}");
        }
        assert!(TargetName::parse(&"a".repeat(TARGET_NAME_MAX_LEN)).is_some());
    }
}
