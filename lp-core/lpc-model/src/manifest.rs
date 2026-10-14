//! Firmware manifest core: compile-time embedding and host-side reading.
//!
//! The manifest core is the build's self-description — package, target,
//! platform,
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
///
/// - 3: `target`, the opaque name of the line of builds this build belongs
///   to (a `lp-fw/builds/` id such as `esp32c6-4mb`, or `unknown` when no
///   build def built it), after `version`; the object that used to be called
///   `target` (`family`, `chip`, `cargoTarget`) is now `platform`.
/// - 2: `version`, the build's app version (`LP_APP_VERSION`, what
///   `scripts/print-app-version.sh` prints), after `package`.
/// - 1: the first shape.
pub const MANIFEST_CORE_VERSION: u32 = 3;

/// The bytes an embedded `target` value may take: a target is
/// `[a-z0-9][a-z0-9-]{0,63}`. Stored in a fixed-width slot for the reason
/// [`VERSION_SLOT_BYTES`] gives.
pub const TARGET_SLOT_BYTES: usize = 64;

/// The bytes an embedded `version` value may take. The value is stored in a
/// slot of exactly this size (space-padded) wherever a firmware image holds
/// it, so a release, a dev or a dirty build differ in the slot's CONTENT and
/// never in any size or address after it — the pinned firmware figures (heap,
/// stack, boot text, cycle counts) cannot start depending on how a build was
/// stamped. The longest real form, `<12-hex sha>-dirty-<HHMMSS>PT`, is 27.
pub const VERSION_SLOT_BYTES: usize = 40;

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

/// A version in its fixed-size slot: the version's bytes, then spaces up to
/// [`VERSION_SLOT_BYTES`]. Refuses (at compile time, in const use) a version
/// that is too long or that holds a byte a JSON string or the padding could
/// not carry — a space, a quote, a backslash or a control byte.
pub const fn version_slot(version: &str) -> [u8; VERSION_SLOT_BYTES] {
    text_slot::<VERSION_SLOT_BYTES>(version)
}

/// A version slot as a fixed-width JSON string: `"`, the version, `"`, then
/// spaces (JSON whitespace) — the same size whatever the version.
pub const fn version_json(version: &str) -> [u8; VERSION_SLOT_BYTES + 2] {
    text_json::<{ VERSION_SLOT_BYTES + 2 }>(version)
}

/// A target name as a fixed-width JSON string, like [`version_json`].
pub const fn target_json(target: &str) -> [u8; TARGET_SLOT_BYTES + 2] {
    text_json::<{ TARGET_SLOT_BYTES + 2 }>(target)
}

/// `text`'s bytes, then spaces up to `N`. Refuses (at compile time, in
/// const use) text that is empty or too long, or that holds a byte a JSON
/// string or the padding could not carry — a space, a quote, a backslash or
/// a control byte.
pub const fn text_slot<const N: usize>(text: &str) -> [u8; N] {
    let bytes = text.as_bytes();
    assert!(
        !bytes.is_empty() && bytes.len() <= N,
        "version_slot: a slot holds 1..=N bytes"
    );
    let mut out = [b' '; N];
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        assert!(
            b > b' ' && b != b'"' && b != b'\\' && b < 0x7f,
            "version_slot: a slot holds printable ASCII with no space, quote or backslash"
        );
        out[i] = b;
        i += 1;
    }
    out
}

/// `text` as a JSON string padded with JSON whitespace to `M` bytes (`M` is
/// the slot's size plus the two quotes).
pub const fn text_json<const M: usize>(text: &str) -> [u8; M] {
    let bytes = text.as_bytes();
    assert!(
        !bytes.is_empty() && bytes.len() + 2 <= M,
        "version_slot: a slot holds 1..=N bytes"
    );
    let mut out = [b' '; M];
    out[0] = b'"';
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        assert!(
            b > b' ' && b != b'"' && b != b'\\' && b < 0x7f,
            "version_slot: a slot holds printable ASCII with no space, quote or backslash"
        );
        out[i + 1] = b;
        i += 1;
    }
    out[i + 1] = b'"';
    out
}

/// The version held in a slot (its bytes up to the first space). Scanned at
/// run time on purpose: a length baked in as an immediate could change an
/// instruction's encoding, and with it every address after it.
pub fn version_from_slot(slot: &[u8; VERSION_SLOT_BYTES]) -> &str {
    let len = slot
        .iter()
        .position(|b| *b == b' ')
        .unwrap_or(VERSION_SLOT_BYTES);
    core::str::from_utf8(&slot[..len]).expect("a version slot is ASCII")
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
            LpFeature::NodePowerButton => "\"node.power-button\",",
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
            LpFeature::FsTree => "\"fs.tree\",",
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

/// A manifest core's `ota_layout` (an update layout code, `0` for none) as
/// the number the JSON carries.
pub const fn ota_layout(layout: u16) -> u32 {
    layout as u32
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
        target: $target:expr,
        chip_family: $family:expr,
        chip: $chip:expr,
        cargo_target: $cargo_target:expr,
        profile: $profile:expr,
        version: $version:expr,
        commit: $commit:expr,
        dirty: $dirty:expr,
        wire_proto: $wire_proto:expr,
        features: [$($feature_fragment:expr),* $(,)?],
        limits_json: $limits_json:expr,
        ota_layout: $ota_layout:expr,
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
            // The version in its fixed-width slot (see
            // `VERSION_SLOT_BYTES`): the blob is the same size whatever the
            // build was stamped with.
            const VERSION_JSON_BUF: [u8; $crate::manifest::VERSION_SLOT_BYTES + 2] =
                $crate::manifest::version_json($version);
            // SAFETY: `version_json` writes printable ASCII only.
            const VERSION_JSON: &str =
                unsafe { ::core::str::from_utf8_unchecked(&VERSION_JSON_BUF) };
            // The target name, in its fixed-width slot for the same reason.
            const TARGET_JSON_BUF: [u8; $crate::manifest::TARGET_SLOT_BYTES + 2] =
                $crate::manifest::target_json($target);
            // SAFETY: `target_json` writes printable ASCII only.
            const TARGET_JSON: &str =
                unsafe { ::core::str::from_utf8_unchecked(&TARGET_JSON_BUF) };

            /// The version alone, in the same fixed-width slot, for the
            /// runtime (the hello reads it through `manifest_version`).
            pub(super) static VERSION_SLOT: [u8; $crate::manifest::VERSION_SLOT_BYTES] =
                $crate::manifest::version_slot($version);

            // The over-the-air update layout this image supports (`0`: a
            // single image, which takes no update over a link — the key is
            // left out).
            const OTA_LAYOUT: u32 = $crate::manifest::ota_layout($ota_layout);
            const OTA_LAYOUT_BUF: [u8; 10] = $crate::manifest::u32_json(OTA_LAYOUT);
            // SAFETY: digits and spaces only.
            const OTA_LAYOUT_JSON: &str =
                unsafe { ::core::str::from_utf8_unchecked(&OTA_LAYOUT_BUF) };
            const OTA_SOME: &str =
                $crate::lp_const_concat!(",\"ota\":{\"layout\":", OTA_LAYOUT_JSON, "}");
            const OTA_JSON: &str = if OTA_LAYOUT == 0 { "" } else { OTA_SOME };

            const CORE_VERSION_BUF: [u8; 10] =
                $crate::manifest::u32_json($crate::manifest::MANIFEST_CORE_VERSION);
            // SAFETY: digits and spaces only.
            const CORE_VERSION: &str =
                unsafe { ::core::str::from_utf8_unchecked(&CORE_VERSION_BUF) };

            const JSON: &str = $crate::lp_const_concat!(
                "{\"lpManifestCore\":", CORE_VERSION,
                ",\"package\":\"", $package,
                "\",\"version\":", VERSION_JSON,
                ",\"target\":", TARGET_JSON,
                ",\"profile\":\"", $profile,
                "\",\"commit\":\"", $commit,
                "\",\"dirty\":", $crate::manifest::bool_json($dirty),
                ",\"platform\":{\"family\":\"", $family,
                "\",\"chip\":\"", $chip,
                "\",\"cargoTarget\":\"", $cargo_target,
                "\"},\"features\":[", FEATURES,
                "],\"limits\":", $limits_json,
                ",\"wireProto\":", WIRE_PROTO,
                OTA_JSON,
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

        /// This build's app version, as the manifest core states it — what
        /// the wire hello reports. Read from a fixed-width slot, so the
        /// image's layout does not depend on the version's length.
        #[allow(
            dead_code,
            reason = "an embedder that sets no hello identity has no reader"
        )]
        pub fn manifest_version() -> &'static str {
            $crate::manifest::version_from_slot(&__lp_manifest_core::VERSION_SLOT)
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
    /// The build's app version (`2026.10.03-1`, or the dev form
    /// `<short-sha>[-dirty-<HHMMSS>PT]`); `unknown` where the embedder has
    /// no VCS facts.
    pub version: alloc::string::String,
    /// The line of builds this build belongs to: a `lp-fw/builds/` id such
    /// as `esp32c6-4mb`, or `unknown` when no build def built it. An opaque
    /// name — never parsed for a chip or a flash size (those are
    /// [`ManifestPlatform`]'s and the build def's).
    pub target: alloc::string::String,
    /// Cargo profile (e.g. `release-esp32`).
    pub profile: alloc::string::String,
    /// Source commit; `unknown` where the embedder has no VCS facts.
    pub commit: alloc::string::String,
    /// Whether the source tree was dirty at build time.
    pub dirty: bool,
    /// Platform identity: what the build runs on.
    pub platform: ManifestPlatform,
    /// Enabled product features.
    pub features: alloc::vec::Vec<LpFeature>,
    /// By-construction numeric facts; empty object when unknown.
    pub limits: ManifestLimits,
    /// `lpc_wire::WIRE_PROTO_VERSION` the build speaks.
    pub wire_proto: u32,
    /// Whether this build can update over a link, and how: the update
    /// layout it supports (`lpc-update`'s code table: `1` = the split
    /// image's layout 1). Absent on a single-image build — a plain local
    /// build, or any chip but the C6 — which only USB can update. Additive:
    /// a reader that does not know it ignores it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ota: Option<ManifestOta>,
}

/// The manifest core's `ota` block: what an over-the-air update needs of
/// this build's layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestOta {
    /// The update layout (`lpc-update`'s code table).
    pub layout: u16,
}

/// Platform identity block of the manifest core (`platform`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestPlatform {
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
            target: "fake-4mb",
            chip_family: "test",
            chip: "testchip",
            cargo_target: "riscv32imac-unknown-none-elf",
            profile: "release-test",
            version: "2026.10.03-1",
            commit: "abc1234",
            dirty: false,
            wire_proto: 4,
            features: [
                crate::manifest::feature_fragment(true, crate::LpFeature::NodeShader),
                crate::manifest::feature_fragment(false, crate::LpFeature::NodeRadio),
                crate::manifest::feature_fragment(true, crate::LpFeature::GfxLpvm),
            ],
            limits_json: "{}",
            ota_layout: 1,
        }
    }

    mod empty_features_firmware {
        lp_embed_manifest_core! {
            package: "fw-empty",
            target: "unknown",
            chip_family: "test",
            chip: "testchip",
            cargo_target: "riscv32imac-unknown-none-elf",
            profile: "debug",
            version: "0123abcde-dirty-120000PT",
            commit: "unknown",
            dirty: true,
            wire_proto: 4,
            features: [],
            limits_json: "{\"flashAppBytes\":3145728}",
            ota_layout: 0,
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
        assert_eq!(core.version, "2026.10.03-1");
        assert_eq!(fake_firmware::manifest_version(), "2026.10.03-1");
        assert_eq!(core.profile, "release-test");
        assert_eq!(core.commit, "abc1234");
        assert!(!core.dirty);
        assert_eq!(core.target, "fake-4mb");
        assert_eq!(core.platform.family, "test");
        assert_eq!(core.platform.chip, "testchip");
        assert_eq!(core.platform.cargo_target, "riscv32imac-unknown-none-elf");
        assert_eq!(
            core.features,
            vec![LpFeature::NodeShader, LpFeature::GfxLpvm]
        );
        assert_eq!(core.limits, ManifestLimits::default());
        assert_eq!(core.wire_proto, 4);
        assert_eq!(core.ota, Some(ManifestOta { layout: 1 }));
    }

    /// Empty feature list and populated limits both survive assembly.
    #[test]
    fn empty_feature_list_and_limits_assemble() {
        let json = empty_features_firmware::manifest_core_json();
        let core: ManifestCore = serde_json::from_str(json).unwrap();
        assert!(core.features.is_empty());
        assert!(core.dirty);
        assert_eq!(core.version, "0123abcde-dirty-120000PT");
        assert_eq!(core.target, "unknown");
        assert_eq!(
            empty_features_firmware::manifest_version(),
            "0123abcde-dirty-120000PT"
        );
        assert_eq!(core.limits.flash_app_bytes, Some(3 * 1024 * 1024));
        assert_eq!(core.limits.flash_total_bytes, None);
        assert_eq!(core.ota, None, "a single image says nothing of updates");
        assert!(!json.contains("\"ota\""));
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

    /// A version's slot is the same size whatever the version, and the JSON
    /// form of it is a string followed by whitespace.
    #[test]
    fn a_version_takes_the_same_bytes_however_long_it_is() {
        let release = version_json("2026.10.03-1");
        let dirty = version_json("0123456789ab-dirty-235959PT");
        assert_eq!(release.len(), dirty.len());
        assert!(release.starts_with(b"\"2026.10.03-1\" "));
        assert!(dirty.starts_with(b"\"0123456789ab-dirty-235959PT\" "));
        assert_eq!(
            version_from_slot(&version_slot("2026.10.03-1")),
            "2026.10.03-1"
        );
        let full = "v".repeat(VERSION_SLOT_BYTES);
        assert_eq!(version_from_slot(&version_slot(&full)), full);
        assert_eq!(
            serde_json::from_slice::<alloc::string::String>(&release).unwrap(),
            "2026.10.03-1"
        );
    }

    /// A target's JSON is the same size whatever the target.
    #[test]
    fn a_target_takes_the_same_bytes_however_long_it_is() {
        let short = target_json("esp32c6-4mb");
        let unknown = target_json("unknown");
        assert_eq!(short.len(), unknown.len());
        assert_eq!(short.len(), TARGET_SLOT_BYTES + 2);
        assert_eq!(
            serde_json::from_slice::<alloc::string::String>(&short).unwrap(),
            "esp32c6-4mb"
        );
    }

    #[test]
    #[should_panic(expected = "version_slot")]
    fn a_target_too_long_for_its_slot_is_refused() {
        target_json(&"t".repeat(TARGET_SLOT_BYTES + 1));
    }

    #[test]
    #[should_panic(expected = "version_slot")]
    fn a_version_too_long_for_its_slot_is_refused() {
        version_slot(&"v".repeat(VERSION_SLOT_BYTES + 1));
    }

    #[test]
    #[should_panic(expected = "version_slot")]
    fn a_version_with_a_quote_is_refused() {
        version_slot("2026\"x");
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
