//! Format 5 → 6: prefixed uids became one token — prefix + lowercase
//! Crockford base-32 body, no separator (PR #384, ADR
//! `2026-08-07-uid-format-single-token-base32`).
//!
//! The break renamed nothing structural; it re-rendered every uid STRING:
//! `prj_h7Kq9xY2mQ4tB8Wz` (a `_` then 16 base-62 chars) became
//! `prjhk7q9xy2mq4tb8wz` (16 lowercase base-32 chars, no separator). A v5
//! package whose manifest carries an old-style uid refuses to parse under
//! the new reader — "uid body must be exactly 16 characters" — which is
//! exactly the sad state this step repairs.
//!
//! The transcode is value-preserving, not a re-mint: decode the old
//! base-62 body, keep the low 80 bits, re-encode in the new alphabet. The
//! same old uid transcodes to the same new uid everywhere it appears, so
//! cross-references inside a package stay coherent — and an efuse-derived
//! device uid (which embeds a value < 2^56 in those bits) transcodes to
//! EXACTLY the uid live hardware now derives for the same MAC.
//!
//! The rule keys off the uid SHAPE — a whole string value of
//! `<known prefix>_<exactly 16 base-62 chars>` — never off a field name:
//! uids ride in manifest `uid` fields, provenance sidecars, and device
//! associations alike, and the shape match migrates every one the same
//! way. A string that merely resembles a uid (`prj_test`, wrong length,
//! an underscore in the body) passes through byte-identical.

use crate::json::JsonNode;
use crate::json_file_edit::edit_json_files;
use crate::project_files::{ProjectFiles, is_manifest_path};
use crate::upgrade_error::UpgradeError;
use crate::upgrade_report::UpgradeReport;

const FROM: u32 = 5;
const TO: u32 = 6;

/// The v5 uid body alphabet (base-62, in digit → upper → lower order).
const OLD_ALPHABET: &[u8; 62] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

/// The v6 uid body alphabet (lowercase Crockford base-32).
const NEW_ALPHABET: &[u8; 32] = b"0123456789abcdefghjkmnpqrstvwxyz";

/// Both formats use 16-character bodies; v6 keeps the low 16 × 5 = 80 bits.
const BODY_LEN: usize = 16;

/// The uid kind prefixes that existed at v5.
const PREFIXES: &[&str] = &["prj", "mod", "dev", "usr"];

pub(crate) fn apply(
    files: &mut ProjectFiles,
    report: &mut UpgradeReport,
) -> Result<(), UpgradeError> {
    edit_json_files(files, report, |path, document, report| {
        if is_manifest_path(path) {
            bump_manifest_format(path, document, report);
        }
        rewrite_uid_strings(path, document, report);
        Ok(())
    })
}

/// R1: the manifest's own version stamp.
fn bump_manifest_format(path: &str, document: &mut JsonNode, report: &mut UpgradeReport) {
    if document.get("format").and_then(JsonNode::as_u32) == Some(FROM) {
        document.set("format", JsonNode::u32(TO));
        report.note(format!("{path}: format {FROM} → {TO}"));
    }
}

/// R2: every string value that is exactly an old-format uid transcodes.
fn rewrite_uid_strings(path: &str, node: &mut JsonNode, report: &mut UpgradeReport) {
    match node {
        JsonNode::Object(members) => {
            for (_, child) in members.iter_mut() {
                rewrite_uid_strings(path, child, report);
            }
        }
        JsonNode::Array(items) => {
            for item in items.iter_mut() {
                rewrite_uid_strings(path, item, report);
            }
        }
        JsonNode::Scalar(_) => {
            let Some(text) = node.as_str() else { return };
            let Some(new) = transcode_uid(&text) else {
                return;
            };
            *node = JsonNode::string(&new);
            report.note(format!("{path}: uid {text} → {new}"));
        }
    }
}

/// `prj_h7Kq9xY2mQ4tB8Wz` → its v6 rendering, or `None` when `text` is
/// not exactly an old-format uid.
fn transcode_uid(text: &str) -> Option<String> {
    let prefix = PREFIXES.iter().find(|p| text.starts_with(**p))?;
    let body = text.strip_prefix(*prefix)?.strip_prefix('_')?;
    if body.len() != BODY_LEN {
        return None;
    }
    let mut value: u128 = 0;
    for ch in body.bytes() {
        let digit = OLD_ALPHABET.iter().position(|c| *c == ch)? as u128;
        value = value * 62 + digit;
    }
    // Keep the low 80 bits — the identical reduction fresh v6 mints apply.
    value &= (1u128 << 80) - 1;
    let mut new_body = [0u8; BODY_LEN];
    for slot in new_body.iter_mut().rev() {
        *slot = NEW_ALPHABET[(value & 31) as usize];
        value >>= 5;
    }
    let new_body = core::str::from_utf8(&new_body).expect("alphabet is ASCII");
    Some(format!("{prefix}{new_body}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transcodes_a_random_style_uid() {
        // Deterministic, prefix-preserving, lowercase base-32 out.
        let new = transcode_uid("prj_h7Kq9xY2mQ4tB8Wz").unwrap();
        assert!(new.starts_with("prj"));
        assert_eq!(new.len(), 3 + BODY_LEN);
        assert!(new[3..].bytes().all(|b| NEW_ALPHABET.contains(&b)), "{new}");
        // Same input, same output — cross-references stay coherent.
        assert_eq!(transcode_uid("prj_h7Kq9xY2mQ4tB8Wz").unwrap(), new);
    }

    /// The efuse embed invariant: the old derived uid for MAC
    /// aa:bb:cc:dd:ee:ff transcodes to exactly what `HardwareId` now
    /// derives for that MAC (the value sits below 2^56, untouched by the
    /// 80-bit mask on both sides).
    #[test]
    fn a_derived_device_uid_transcodes_to_the_new_derivation() {
        assert_eq!(
            transcode_uid("dev_000000029EVDlKLX").unwrap(),
            "dev000000daqf6dvvqz"
        );
    }

    #[test]
    fn near_misses_pass_through() {
        for text in [
            "prj_test",               // wrong body length
            "prj_h7Kq9xY2mQ4tB8W!",   // bad body char
            "prjh7kq9xy2mq4tb8wz",    // already new-format
            "xxx_h7Kq9xY2mQ4tB8Wz",   // unknown prefix
            "prj_h7Kq9xY2mQ4tB8WzZ",  // one too long
            "a prj_h7Kq9xY2mQ4tB8Wz", // embedded, not the whole value
        ] {
            assert_eq!(transcode_uid(text), None, "for {text:?}");
        }
    }
}
