//! Firmware manifest core: compile-time embedding and host-side reading.
//!
//! The manifest core is the build's self-description — package, target,
//! [`crate::LpFeature`] list, wire proto, provenance — assembled **at compile
//! time** from `cfg!`/`env!` facts into a magic-delimited JSON blob that
//! lives in a `#[used]` static inside every firmware artifact. Tooling
//! (`lp-cli firmware show`, CI drift checks) *extracts* the blob by scanning
//! artifact bytes for the delimiters; nothing downstream re-states what the
//! build enabled.
//!
//! Design notes (ADR `docs/adr/2026-08-01-firmware-manifest-architecture.md`):
//!
//! - The blob sits in ordinary `.rodata` — a dedicated link section would need
//!   per-target linker-script guarantees; a byte scan works on an ELF, an
//!   espflash merged image, and a wasm module alike.
//! - Assembly is `const`: no serde, no formatting machinery, no runtime cost
//!   beyond the blob's own bytes.
//! - The delimiters are split at the source level (`concat!` at use sites) so
//!   the only contiguous occurrences in an artifact are real blobs.

use serde::{Deserialize, Serialize};

use crate::feature::{LpFeature, ManifestLimits};

/// Blob prefix. Includes the layout version: bump only with a layout change
/// (the JSON inside carries its own `lpManifestCore` version for shape).
pub const MANIFEST_BLOB_BEGIN: &str = "\u{1}LP-FW-MANIFEST-BEGIN-v1\u{2}";
/// Blob suffix; guards against truncated artifacts.
pub const MANIFEST_BLOB_END: &str = "\u{3}LP-FW-MANIFEST-END-v1\u{4}";

/// JSON shape version of the manifest core payload.
pub const MANIFEST_CORE_VERSION: u32 = 1;

// --- Const assembly ---------------------------------------------------------------------------

/// Total byte length of the concatenation of `parts`.
pub const fn concat_len(parts: &[&str]) -> usize {
    let mut total = 0;
    let mut i = 0;
    while i < parts.len() {
        total += parts[i].len();
        i += 1;
    }
    total
}

/// Concatenate `parts` into a fixed-size byte array. `N` must equal
/// [`concat_len`]`(parts)`; the const evaluator rejects a mismatch.
pub const fn concat_bytes<const N: usize>(parts: &[&str]) -> [u8; N] {
    let mut out = [0u8; N];
    let mut at = 0;
    let mut i = 0;
    while i < parts.len() {
        let bytes = parts[i].as_bytes();
        let mut j = 0;
        while j < bytes.len() {
            out[at] = bytes[j];
            at += 1;
            j += 1;
        }
        i += 1;
    }
    assert!(at == N, "concat_bytes: N must equal concat_len(parts)");
    out
}

/// A `u32` as a fixed-width JSON number: the decimal digits followed by
/// spaces (JSON whitespace) up to 10 bytes, so the result is `const`-sized.
pub const fn u32_json(value: u32) -> [u8; 10] {
    let mut out = [b' '; 10];
    let mut n = value;
    let mut digits = 1;
    while n >= 10 {
        n /= 10;
        digits += 1;
    }
    let mut n = value;
    let mut i = digits;
    loop {
        i -= 1;
        out[i] = b'0' + (n % 10) as u8;
        n /= 10;
        if i == 0 {
            break;
        }
    }
    out
}

/// `"true"` / `"false"` for JSON booleans in const assembly.
pub const fn bool_json(value: bool) -> &'static str {
    if value { "true" } else { "false" }
}

/// Const string equality, for parsing `env!` facts (e.g. `LP_BUILD_DIRTY`,
/// which build scripts emit as `"true"`/`"false"`) at compile time.
pub const fn str_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

/// A feature-list fragment: the quoted wire name followed by a comma when
/// `enabled`, empty otherwise. Fragments concatenate into a JSON array body;
/// [`blank_trailing_comma`] handles the trailing comma.
pub const fn feature_fragment(enabled: bool, feature: LpFeature) -> &'static str {
    if enabled {
        // Fragments are pre-quoted strings; the match keeps this total over
        // the registry so a new variant cannot silently lack a fragment.
        match feature {
            LpFeature::NodeButton => "\"node.button\",",
            LpFeature::NodeClock => "\"node.clock\",",
            LpFeature::NodeFluid => "\"node.fluid\",",
            LpFeature::NodeFixture => "\"node.fixture\",",
            LpFeature::NodePlaylist => "\"node.playlist\",",
            LpFeature::NodeRadio => "\"node.radio\",",
            LpFeature::NodeShader => "\"node.shader\",",
            LpFeature::NodeTexture => "\"node.texture\",",
            LpFeature::SvcButton => "\"svc.button\",",
            LpFeature::SvcRadioEspnow => "\"svc.radio-espnow\",",
            LpFeature::GfxLpvm => "\"gfx.lpvm\",",
            LpFeature::GfxNull => "\"gfx.null\",",
            LpFeature::GfxWgpu => "\"gfx.wgpu\",",
            LpFeature::DiagUnwind => "\"diag.unwind\",",
            LpFeature::ShaderF32 => "\"shader.f32\",",
        }
    } else {
        ""
    }
}

/// Replace a trailing comma (if any) in `buf` with a space, in place. Called
/// on the assembled feature-array body so `["a","b",` becomes `["a","b" ` —
/// the closing `]` then yields valid JSON for empty and non-empty lists.
pub const fn blank_trailing_comma<const N: usize>(mut buf: [u8; N]) -> [u8; N] {
    if N > 0 && buf[N - 1] == b',' {
        buf[N - 1] = b' ';
    }
    buf
}

/// Const-concatenate string literals/consts into a `&'static str`.
#[macro_export]
macro_rules! lp_const_concat {
    ($($part:expr),* $(,)?) => {{
        const PARTS: &[&str] = &[$($part),*];
        const LEN: usize = $crate::manifest::concat_len(PARTS);
        const BUF: [u8; LEN] = $crate::manifest::concat_bytes::<LEN>(PARTS);
        // SAFETY: a concatenation of `&str`s is valid UTF-8.
        const OUT: &str = unsafe { ::core::str::from_utf8_unchecked(&BUF) };
        OUT
    }};
}

/// Copy a `&str`'s bytes into a fixed-size array. `N` must equal `s.len()`.
pub const fn str_bytes<const N: usize>(s: &str) -> [u8; N] {
    let bytes = s.as_bytes();
    assert!(bytes.len() == N, "str_bytes: N must equal s.len()");
    let mut out = [0u8; N];
    let mut i = 0;
    while i < N {
        out[i] = bytes[i];
        i += 1;
    }
    out
}

/// Embed the firmware manifest core into the current crate.
///
/// Expands to a hidden module holding the delimited blob in a `#[used]`
/// static, plus `pub fn manifest_core_json() -> &'static str` returning the
/// JSON payload (the `ServerHello` runtime projection reads this in M4).
/// Every field is required — a new embedder cannot forget one and still
/// compile.
///
/// `features` takes pre-quoted, comma-terminated fragments (see
/// [`feature_fragment`] and `lpc-engine`'s `ENGINE_FEATURE_FRAGMENT`);
/// name fragments by full path — the expansion lives in a nested module.
/// The trailing comma is blanked during assembly.
#[macro_export]
macro_rules! lp_embed_manifest_core {
    (
        package: $package:expr,
        chip_family: $family:expr,
        chip: $chip:expr,
        cargo_target: $cargo_target:expr,
        profile: $profile:expr,
        commit: $commit:expr,
        dirty: $dirty:expr,
        wire_proto: $wire_proto:expr,
        features: [$($feature_fragment:expr),* $(,)?],
        limits_json: $limits_json:expr,
    ) => {
        #[doc(hidden)]
        mod __lp_manifest_core {
            // Feature array body, trailing comma blanked to a space.
            const FEATURE_PARTS: &[&str] = &[$($feature_fragment),*];
            const FEATURES_LEN: usize =
                $crate::manifest::concat_len(FEATURE_PARTS);
            const FEATURES_BUF: [u8; FEATURES_LEN] =
                $crate::manifest::blank_trailing_comma(
                    $crate::manifest::concat_bytes::<FEATURES_LEN>(FEATURE_PARTS),
                );
            // SAFETY: concatenation of `&str`s with an ASCII byte substituted.
            const FEATURES: &str =
                unsafe { ::core::str::from_utf8_unchecked(&FEATURES_BUF) };

            const WIRE_PROTO_BUF: [u8; 10] =
                $crate::manifest::u32_json($wire_proto);
            // SAFETY: digits and spaces only.
            const WIRE_PROTO: &str =
                unsafe { ::core::str::from_utf8_unchecked(&WIRE_PROTO_BUF) };
            const CORE_VERSION_BUF: [u8; 10] =
                $crate::manifest::u32_json($crate::manifest::MANIFEST_CORE_VERSION);
            // SAFETY: digits and spaces only.
            const CORE_VERSION: &str =
                unsafe { ::core::str::from_utf8_unchecked(&CORE_VERSION_BUF) };

            const JSON: &str = $crate::lp_const_concat!(
                "{\"lpManifestCore\":", CORE_VERSION,
                ",\"package\":\"", $package,
                "\",\"profile\":\"", $profile,
                "\",\"commit\":\"", $commit,
                "\",\"dirty\":", $crate::manifest::bool_json($dirty),
                ",\"target\":{\"family\":\"", $family,
                "\",\"chip\":\"", $chip,
                "\",\"cargoTarget\":\"", $cargo_target,
                "\"},\"features\":[", FEATURES,
                "],\"limits\":", $limits_json,
                ",\"wireProto\":", WIRE_PROTO,
                "}",
            );

            const BLOB: &str = $crate::lp_const_concat!(
                // Delimiters are concat!-split so the macro's own expansion
                // never contains a contiguous delimiter besides the blob.
                concat!("\u{1}LP-FW-MANIFEST-", "BEGIN-v1\u{2}"),
                JSON,
                concat!("\u{3}LP-FW-MANIFEST-", "END-v1\u{4}"),
            );

            const BLOB_LEN: usize = BLOB.len();

            /// The one copy of the blob bytes in the artifact; `#[used]` pins
            /// it through dead-code stripping.
            #[used]
            pub(super) static BLOB_BYTES: [u8; BLOB_LEN] =
                $crate::manifest::str_bytes::<BLOB_LEN>(BLOB);
        }

        /// The embedded manifest core JSON (delimiters stripped). Slices the
        /// embedded static so the JSON bytes exist once in the artifact.
        #[allow(
            dead_code,
            reason = "the ServerHello projection (M4) reads this; until then \
                      the #[used] blob static is the consumer"
        )]
        pub fn manifest_core_json() -> &'static str {
            let begin = $crate::manifest::MANIFEST_BLOB_BEGIN.len();
            let end = __lp_manifest_core::BLOB_BYTES.len()
                - $crate::manifest::MANIFEST_BLOB_END.len();
            ::core::str::from_utf8(&__lp_manifest_core::BLOB_BYTES[begin..end])
                .expect("manifest blob is compile-time UTF-8")
        }
    };
}

// --- Host-side reading ------------------------------------------------------------------------

/// Parsed manifest core — the host-side (lp-cli, studio tooling, M4 hello)
/// projection of the embedded JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestCore {
    /// JSON shape version ([`MANIFEST_CORE_VERSION`]).
    pub lp_manifest_core: u32,
    /// Cargo package that produced the build (e.g. `fw-esp32c6`).
    pub package: alloc::string::String,
    /// Cargo profile (e.g. `release-esp32`).
    pub profile: alloc::string::String,
    /// Source commit; `unknown` where the embedder has no VCS facts.
    pub commit: alloc::string::String,
    /// Whether the source tree was dirty at build time.
    pub dirty: bool,
    /// Target identity.
    pub target: ManifestTarget,
    /// Enabled product features.
    pub features: alloc::vec::Vec<LpFeature>,
    /// By-construction numeric facts; empty object when unknown.
    pub limits: ManifestLimits,
    /// `lpc_wire::WIRE_PROTO_VERSION` the build speaks.
    pub wire_proto: u32,
}

/// Target identity block of the manifest core.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestTarget {
    /// Chip family / platform (e.g. `esp32`, `browser`, `host`).
    pub family: alloc::string::String,
    /// Concrete chip or platform detail (e.g. `esp32c6`, `wasm32`).
    pub chip: alloc::string::String,
    /// Rust target triple the build was compiled for.
    pub cargo_target: alloc::string::String,
}

/// Find the manifest-core JSON payload in raw artifact bytes (ELF, espflash
/// merged image, or wasm module). Returns the JSON slice, or `None` when no
/// well-formed blob is present.
pub fn find_manifest_core(artifact: &[u8]) -> Option<&[u8]> {
    let begin = MANIFEST_BLOB_BEGIN.as_bytes();
    let end = MANIFEST_BLOB_END.as_bytes();
    let start = find(artifact, begin)? + begin.len();
    let len = find(&artifact[start..], end)?;
    Some(&artifact[start..start + len])
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;

    // Exercise the embedding macro exactly as a firmware crate would.
    mod fake_firmware {
        lp_embed_manifest_core! {
            package: "fw-fake",
            chip_family: "test",
            chip: "testchip",
            cargo_target: "riscv32imac-unknown-none-elf",
            profile: "release-test",
            commit: "abc1234",
            dirty: false,
            wire_proto: 4,
            features: [
                crate::manifest::feature_fragment(true, crate::LpFeature::NodeShader),
                crate::manifest::feature_fragment(false, crate::LpFeature::NodeRadio),
                crate::manifest::feature_fragment(true, crate::LpFeature::GfxLpvm),
            ],
            limits_json: "{}",
        }
    }

    mod empty_features_firmware {
        lp_embed_manifest_core! {
            package: "fw-empty",
            chip_family: "test",
            chip: "testchip",
            cargo_target: "riscv32imac-unknown-none-elf",
            profile: "debug",
            commit: "unknown",
            dirty: true,
            wire_proto: 4,
            features: [],
            limits_json: "{\"flashAppBytes\":3145728}",
        }
    }

    /// The embedded JSON parses into `ManifestCore` with exactly the facts
    /// the macro was given — the round trip the whole design rests on.
    #[test]
    fn embedded_blob_round_trips_through_manifest_core() {
        let json = fake_firmware::manifest_core_json();
        let core: ManifestCore = serde_json::from_str(json).unwrap();
        assert_eq!(core.lp_manifest_core, MANIFEST_CORE_VERSION);
        assert_eq!(core.package, "fw-fake");
        assert_eq!(core.profile, "release-test");
        assert_eq!(core.commit, "abc1234");
        assert!(!core.dirty);
        assert_eq!(core.target.family, "test");
        assert_eq!(core.target.chip, "testchip");
        assert_eq!(core.target.cargo_target, "riscv32imac-unknown-none-elf");
        assert_eq!(
            core.features,
            vec![LpFeature::NodeShader, LpFeature::GfxLpvm]
        );
        assert_eq!(core.limits, ManifestLimits::default());
        assert_eq!(core.wire_proto, 4);
    }

    /// Empty feature list and populated limits both survive assembly.
    #[test]
    fn empty_feature_list_and_limits_assemble() {
        let json = empty_features_firmware::manifest_core_json();
        let core: ManifestCore = serde_json::from_str(json).unwrap();
        assert!(core.features.is_empty());
        assert!(core.dirty);
        assert_eq!(core.limits.flash_app_bytes, Some(3 * 1024 * 1024));
        assert_eq!(core.limits.flash_total_bytes, None);
    }

    /// Extraction finds the delimited payload in surrounding artifact bytes,
    /// exactly as `lp-cli firmware show` scans a binary.
    #[test]
    fn find_manifest_core_scans_artifact_bytes() {
        let json = fake_firmware::manifest_core_json();
        let mut artifact = vec![0u8; 512];
        artifact.extend_from_slice(MANIFEST_BLOB_BEGIN.as_bytes());
        artifact.extend_from_slice(json.as_bytes());
        artifact.extend_from_slice(MANIFEST_BLOB_END.as_bytes());
        artifact.extend_from_slice(&[0xFF; 256]);

        let found = find_manifest_core(&artifact).expect("blob found");
        assert_eq!(found, json.as_bytes());

        assert_eq!(find_manifest_core(&[0u8; 64]), None);
        // Truncated artifact: BEGIN present, END missing.
        let cut = &artifact[..artifact.len() - 300];
        assert_eq!(find_manifest_core(cut), None);
    }

    /// The delimiter byte forms are pinned: `scripts/extract-fw-manifest.mjs`
    /// mirrors them (dependency-free CI extraction). Change these only with
    /// that script, in the same commit.
    #[test]
    fn delimiters_are_pinned() {
        assert_eq!(MANIFEST_BLOB_BEGIN, "\u{1}LP-FW-MANIFEST-BEGIN-v1\u{2}");
        assert_eq!(MANIFEST_BLOB_END, "\u{3}LP-FW-MANIFEST-END-v1\u{4}");
    }

    /// Const number formatting: digits then JSON whitespace.
    #[test]
    fn u32_json_is_digits_then_spaces() {
        assert_eq!(&u32_json(0), b"0         ");
        assert_eq!(&u32_json(42), b"42        ");
        assert_eq!(&u32_json(u32::MAX), b"4294967295");
    }

    /// The feature-fragment match stays total over the registry and agrees
    /// with the pinned wire names.
    #[test]
    fn feature_fragments_agree_with_wire_names() {
        for feature in LpFeature::ALL {
            let frag = feature_fragment(true, feature);
            let expected = alloc::format!("\"{}\",", feature.wire_name());
            assert_eq!(frag, expected.as_str());
            assert_eq!(feature_fragment(false, feature), "");
        }
    }
}
