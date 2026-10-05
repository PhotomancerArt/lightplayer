//! Release asset names: `<target>.<file>` on the tag `v<version>` (doors #15).

use alloc::format;
use alloc::string::String;

use crate::firmware_lookup_path::is_lookup_file_name;
use crate::target_name::TargetName;

/// The GitHub release asset name of `file` for `target`:
/// `esp32c6-4mb.ota-manifest.json`, `esp32c6-4mb.engine.z`.
///
/// A target never holds a `.`, so the first `.` always splits it back
/// ([`split_asset_name`]).
pub fn asset_name(target: &TargetName, file: &str) -> String {
    format!("{target}.{file}")
}

/// Split an asset name back into its target and file, or `None` when it is
/// not `<target>.<file>`.
pub fn split_asset_name(name: &str) -> Option<(TargetName, &str)> {
    let (target, file) = name.split_once('.')?;
    let target = TargetName::parse(target)?;
    is_lookup_file_name(file).then_some((target, file))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip() {
        let target = TargetName::parse("esp32c6-4mb").unwrap();
        for file in [
            "ota-manifest.json",
            "engine.z",
            "fw-esp32c6-merged.bin",
            "package.json",
        ] {
            let name = asset_name(&target, file);
            assert_eq!(name, format!("esp32c6-4mb.{file}"));
            assert_eq!(split_asset_name(&name), Some((target.clone(), file)));
        }
    }

    #[test]
    fn refuses_names_that_are_not_target_dot_file() {
        for name in [
            "ota-manifest",
            "ESP32.engine.z",
            "esp32c6-4mb.",
            ".engine.z",
            "esp32c6-4mb...z",
        ] {
            assert!(split_asset_name(name).is_none(), "{name:?}");
        }
    }
}
