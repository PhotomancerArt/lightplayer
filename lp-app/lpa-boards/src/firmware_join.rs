//! The computed board↔firmware join.
//!
//! Which firmware a board runs is **never hand-authored**. It is computed
//! from two checked-in stores (ADR
//! `docs/adr/2026-08-01-firmware-manifest-architecture.md`, "the three-store
//! join"):
//!
//! - **build defs** (`lp-fw/builds/<id>.json`) — how an image is produced:
//!   chip identity and the flash size its partition table targets;
//!   distributed as `firmware/<id>/` by `lp-cli firmware package`.
//! - **board manifests** — here, the display sidecar's chip
//!   ([`BoardDisplayFile::family`], espflash spelling) and
//!   [`BoardDisplayFile::flash_mb`].
//!
//! Compatibility is `chip matches ∧ board flash ≥ build flash`. A board may
//! pin an exception ([`BoardDisplayFile::firmware_allow`] /
//! [`BoardDisplayFile::firmware_deny`]), always with a stated reason; an
//! allow-pin relaxes the flash rule only — **chip identity is never
//! overridable**, because a different ISA cannot execute the image at all.
//!
//! What a build *contains* is a third fact, and this module does not restate
//! it: [`build_features`] reads the checked-in `manifest-core.expected.json`
//! fixture for the build's package, which CI diffs against the manifest
//! extracted from the image it just built. The catalog therefore shows
//! features that were compiled in, not features someone typed.
//!
//! The third store of the join — measured soft limits, a property of a
//! (build × board) pair — is roadmap M6 and deliberately absent here.

use std::sync::OnceLock;

use lpc_model::{LpFeature, NodeKind};
use serde::Deserialize;

use crate::display_manifest::BoardDisplayFile;

// --- build defs -------------------------------------------------------------------------------

/// `(build_id, json_source)` for every checked-in build def. Adding a build
/// def = adding the file under `lp-fw/builds/` and listing it here; the drift
/// test fails if this list and the directory disagree.
pub const BUILD_DEF_SOURCES: &[(&str, &str)] = &[
    (
        "esp32c6-4mb",
        include_str!("../../../lp-fw/builds/esp32c6-4mb.json"),
    ),
    (
        "esp32s3-8mb",
        include_str!("../../../lp-fw/builds/esp32s3-8mb.json"),
    ),
    (
        "esp32v3-4mb",
        include_str!("../../../lp-fw/builds/esp32v3-4mb.json"),
    ),
];

/// The deployment's served-build list (`lp-fw/builds/served.json`).
///
/// Which images the Studio site actually ships is a **deployment** fact, not
/// board data — but it is a statement about build defs, so it lives beside
/// them and is drift-tested against them here. It has one file rather than
/// one constant because three languages read it: this crate, the justfile
/// recipes that package and copy `firmware/<id>/`, and the Pages smoke
/// check. Two hand-maintained copies of it in two languages is exactly the
/// class of bug that let the site offer a board it could not flash.
const SERVED_BUILDS_SOURCE: &str = include_str!("../../../lp-fw/builds/served.json");

/// `(package, manifest_core_fixture)` for every firmware package a build def
/// names. The fixture is the CI-verified projection of the manifest core
/// embedded in that package's image.
const MANIFEST_CORE_SOURCES: &[(&str, &str)] = &[
    (
        "fw-esp32c6",
        include_str!("../../../lp-fw/fw-esp32c6/manifest-core.expected.json"),
    ),
    (
        "fw-esp32s3",
        include_str!("../../../lp-fw/fw-esp32s3/manifest-core.expected.json"),
    ),
    (
        "fw-esp32v3",
        include_str!("../../../lp-fw/fw-esp32v3/manifest-core.expected.json"),
    ),
];

/// The build-def fields the join needs. Deliberately a subset: this crate
/// reads build defs, it does not own them (`lp-fw/builds/README.md` does),
/// and it must never grow a dependency on `lp-cli`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FirmwareBuild {
    /// Build-def schema version. Only `1` is accepted (version + refuse).
    pub format: u32,
    /// Variant id; also the packaged directory name `firmware/<id>/`.
    pub id: String,
    /// Human label carried into the distribution manifest.
    pub display_name: String,
    /// Cargo package that produces the image — the key into the manifest
    /// core fixtures.
    pub package: String,
    /// Flash the image header declares, in megabytes.
    pub flash_size_mb: u32,
    /// espflash chip identity.
    pub chip: BuildChip,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct BuildChip {
    pub family: String,
    /// espflash chip name, e.g. `esp32c6` — matched against a board's
    /// [`BoardDisplayFile::family`].
    pub name: String,
}

/// Every checked-in build def, parsed once, in `BUILD_DEF_SOURCES` order.
/// Panics on malformed embedded data — the drift tests keep that impossible
/// at HEAD.
pub fn all_builds() -> &'static [FirmwareBuild] {
    static BUILDS: OnceLock<Vec<FirmwareBuild>> = OnceLock::new();
    BUILDS.get_or_init(|| {
        BUILD_DEF_SOURCES
            .iter()
            .map(|(id, source)| {
                let build: FirmwareBuild = serde_json::from_str(source)
                    .unwrap_or_else(|error| panic!("embedded build def {id}: {error}"));
                assert_eq!(build.format, 1, "build def {id}: unsupported format");
                assert_eq!(&build.id, id, "build def listed under the wrong id");
                build
            })
            .collect()
    })
}

pub fn build_by_id(build_id: &str) -> Option<&'static FirmwareBuild> {
    all_builds().iter().find(|build| build.id == build_id)
}

/// The build ids this Studio deployment serves, in `served.json` order.
///
/// The provisioning picker's eligibility filter: a board is only offered
/// when one of its compatible builds ([`compatible_builds_for`]) is in this
/// list, because flashing has to be real rather than aspirational. Also the
/// candidate set [`provisioning_build_id`] chooses from.
pub fn served_build_ids() -> &'static [String] {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct ServedFile {
        format: u32,
        builds: Vec<String>,
    }

    static SERVED: OnceLock<Vec<String>> = OnceLock::new();
    SERVED.get_or_init(|| {
        let file: ServedFile = serde_json::from_str(SERVED_BUILDS_SOURCE)
            .unwrap_or_else(|error| panic!("embedded served.json: {error}"));
        assert_eq!(file.format, 1, "served.json: unsupported format");
        file.builds
    })
}

/// Whether the deployment serves an image for `build_id`.
pub fn is_served(build_id: &str) -> bool {
    served_build_ids().iter().any(|id| id == build_id)
}

/// The features compiled into `package`'s firmware, from its checked-in
/// manifest-core fixture. `None` for a package with no fixture — the catalog
/// then says nothing about that build's features rather than guessing.
pub fn build_features(package: &str) -> Option<&'static [LpFeature]> {
    #[derive(Deserialize)]
    struct CoreFixture {
        features: Vec<LpFeature>,
    }

    static FEATURES: OnceLock<Vec<(&'static str, Vec<LpFeature>)>> = OnceLock::new();
    let table = FEATURES.get_or_init(|| {
        MANIFEST_CORE_SOURCES
            .iter()
            .map(|(package, source)| {
                let fixture: CoreFixture = serde_json::from_str(source)
                    .unwrap_or_else(|error| panic!("manifest core fixture {package}: {error}"));
                (*package, fixture.features)
            })
            .collect()
    });
    table
        .iter()
        .find(|(name, _)| *name == package)
        .map(|(_, features)| features.as_slice())
}

// --- the join ---------------------------------------------------------------------------------

/// Why a build is offered for a board.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompatibilityBasis {
    /// The computed rule: chip matches and the board's flash fits the image.
    Computed,
    /// The board pinned this build despite the flash rule, for this reason.
    Pinned { reason: String },
}

/// One compatible build, with the fit facts a UI needs to explain itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompatibleBuild {
    pub build: &'static FirmwareBuild,
    pub basis: CompatibilityBasis,
    /// Board flash minus the image's declared flash, in MB. `None` when the
    /// board's flash size is unverified (pinned matches only).
    pub headroom_mb: Option<u32>,
}

/// Why no build matched — the honest empty state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoBuildReason {
    /// Nothing checked in targets this chip yet.
    NoBuildForChip,
    /// Builds exist for the chip, but every one wants more flash than the
    /// board has. Carries the smallest requirement.
    NeedsMoreFlash { needs_mb: u32, board_mb: u32 },
    /// The board's flash size is not verified, so nothing can be computed.
    FlashUnknown,
    /// Every matching build is denied by the board's pins.
    AllDenied,
}

/// The builds `board` can run, best first (largest flash image that fits,
/// then by id). Pure over its inputs; [`compatible_builds_for`] applies it to
/// the checked-in set.
pub fn compatible_builds(
    board: &BoardDisplayFile,
    builds: &'static [FirmwareBuild],
) -> Vec<CompatibleBuild> {
    let denied = |build: &FirmwareBuild| {
        board
            .firmware_deny
            .iter()
            .any(|pin| pin.build_id == build.id)
    };
    let pinned = |build: &FirmwareBuild| {
        board
            .firmware_allow
            .iter()
            .find(|pin| pin.build_id == build.id)
    };

    let mut matches: Vec<CompatibleBuild> = builds
        .iter()
        .filter(|build| build.chip.name == board.family && !denied(build))
        .filter_map(|build| {
            let fits = board
                .flash_mb
                .is_some_and(|board_mb| board_mb >= build.flash_size_mb);
            let basis = if fits {
                CompatibilityBasis::Computed
            } else {
                // An allow-pin relaxes flash fit only; the chip check above
                // already ran and is not overridable.
                CompatibilityBasis::Pinned {
                    reason: pinned(build)?.reason.clone(),
                }
            };
            Some(CompatibleBuild {
                build,
                basis,
                headroom_mb: board
                    .flash_mb
                    .map(|board_mb| board_mb.saturating_sub(build.flash_size_mb)),
            })
        })
        .collect();
    matches.sort_by(|a, b| {
        b.build
            .flash_size_mb
            .cmp(&a.build.flash_size_mb)
            .then_with(|| a.build.id.cmp(&b.build.id))
    });
    matches
}

/// [`compatible_builds`] over the checked-in build defs.
pub fn compatible_builds_for(board: &BoardDisplayFile) -> Vec<CompatibleBuild> {
    compatible_builds(board, all_builds())
}

// --- provisioning selection ---------------------------------------------------------------------

/// The build to flash when provisioning, or `None` when nothing served fits.
///
/// **Chip is the necessary condition, board is the refinement.** A different
/// ISA cannot execute the image at all, so the chip decides which builds are
/// even candidates; the board only chooses among several that all run on it
/// (today that is never more than one, but flash-size variants are exactly
/// what build defs exist to express). A probed chip with no picked board —
/// the generic-install path, and the common one — still resolves.
///
/// A pick that contradicts the detected chip is **honoured**, not
/// second-guessed: provisioning also writes the picked board's runtime
/// manifest to `/hardware.json`, so silently flashing the detected chip's
/// image would produce a booting device with another board's pin map. The
/// flash-time chip guard refuses the mismatch by name instead, which is the
/// answer that tells the user what to change.
///
/// `normalized_chip` must already be lowercase alphanumeric
/// (`lpa_link::normalize_chip_name`): probe answers say `ESP32-C6` and boot
/// banners say `esp32c6`, and this crate cannot depend on the crate that
/// owns that normalizer. `chip_names_are_already_normalized` in the drift
/// tests pins the other side — every checked-in `chip.name` and board
/// `family` is in that form.
///
/// `None` is a real answer, not a failure: the caller falls back to the
/// deployment default and the flash-time chip guard catches a mismatch.
/// Guessing an image here would be the bug this whole path exists to fix.
pub fn provisioning_build(
    board: Option<&BoardDisplayFile>,
    normalized_chip: Option<&str>,
    builds: &'static [FirmwareBuild],
    served: &[String],
) -> Option<&'static FirmwareBuild> {
    let is_served = |build: &FirmwareBuild| served.iter().any(|id| *id == build.id);

    // A picked board is the most specific evidence there is: it carries a
    // verified flash size, so `compatible_builds` can rank by fit.
    if let Some(board) = board
        && let Some(matched) = compatible_builds(board, builds)
            .into_iter()
            .find(|matched| is_served(matched.build))
    {
        return Some(matched.build);
    }

    // Generic install, or a board whose builds this deployment does not
    // serve. Fall back to the chip — detected first, because it is ground
    // truth and a picked board can simply be the wrong one.
    let chip = normalized_chip.or(board.map(|board| board.family.as_str()))?;
    builds
        .iter()
        .filter(|build| build.chip.name == chip && is_served(build))
        // Smallest image first: without a board there is no verified flash
        // size, and the smallest declared flash is the one most likely to
        // fit whatever is actually on the module.
        .min_by(|a, b| {
            a.flash_size_mb
                .cmp(&b.flash_size_mb)
                .then_with(|| a.id.cmp(&b.id))
        })
}

/// [`provisioning_build`] over the checked-in build defs and served list,
/// returning just the id the flash request carries.
pub fn provisioning_build_id(
    board: Option<&BoardDisplayFile>,
    normalized_chip: Option<&str>,
) -> Option<&'static str> {
    provisioning_build(board, normalized_chip, all_builds(), served_build_ids())
        .map(|build| build.id.as_str())
}

/// Why `board` has no compatible build, for the catalog's empty state.
/// Returns `None` when it does have one.
pub fn no_build_reason(
    board: &BoardDisplayFile,
    builds: &'static [FirmwareBuild],
) -> Option<NoBuildReason> {
    if !compatible_builds(board, builds).is_empty() {
        return None;
    }
    let for_chip: Vec<&FirmwareBuild> = builds
        .iter()
        .filter(|build| build.chip.name == board.family)
        .collect();
    if for_chip.is_empty() {
        return Some(NoBuildReason::NoBuildForChip);
    }
    let all_denied = for_chip.iter().all(|build| {
        board
            .firmware_deny
            .iter()
            .any(|pin| pin.build_id == build.id)
    });
    if all_denied {
        return Some(NoBuildReason::AllDenied);
    }
    let Some(board_mb) = board.flash_mb else {
        return Some(NoBuildReason::FlashUnknown);
    };
    let needs_mb = for_chip
        .iter()
        .map(|build| build.flash_size_mb)
        .min()
        .expect("non-empty");
    Some(NoBuildReason::NeedsMoreFlash { needs_mb, board_mb })
}

/// [`no_build_reason`] over the checked-in build defs.
pub fn no_build_reason_for(board: &BoardDisplayFile) -> Option<NoBuildReason> {
    no_build_reason(board, all_builds())
}

// --- feature presentation ---------------------------------------------------------------------

/// How a feature reads on the catalog. The classification is wildcard-free,
/// so a new [`LpFeature`] cannot ship without someone deciding whether the
/// boards page names it.
enum CatalogNote {
    /// Expected on every product build — its **absence** is the news.
    Norm { absent: &'static str },
    /// Beyond the norm — its **presence** is the news.
    Extra { present: &'static str },
    /// Rolled up by the node-kind summary; never named on its own.
    NodeRuntime,
    /// Deliberately unnamed: an implementation detail, or the shadow of
    /// another feature that already carries the message.
    Silent,
}

const fn catalog_note(feature: LpFeature) -> CatalogNote {
    match feature {
        LpFeature::NodeButton
        | LpFeature::NodeClock
        | LpFeature::NodeFluid
        | LpFeature::NodeFixture
        | LpFeature::NodePlaylist
        | LpFeature::NodeRadio
        | LpFeature::NodeShader
        | LpFeature::NodeTexture => CatalogNote::NodeRuntime,
        LpFeature::SvcButton => CatalogNote::Norm {
            absent: "no button input",
        },
        LpFeature::SvcRadioEspnow => CatalogNote::Norm {
            absent: "no ESP-NOW radio",
        },
        LpFeature::GfxLpvm => CatalogNote::Norm {
            absent: "no shader rendering",
        },
        // `gfx.null` present is exactly `gfx.lpvm` absent, already reported.
        LpFeature::GfxNull => CatalogNote::Silent,
        LpFeature::GfxWgpu => CatalogNote::Extra {
            present: "GPU rendering",
        },
        // Was `Norm { absent: "shader faults reboot the device" }` — accurate
        // when the C6 could catch a shader fault in-process and other chips
        // could not. No build reports `diag.unwind` since ADR
        // 2026-08-02-rv32-firmwares-are-abort-tier, so as a Norm it would print
        // that gap on *every* board: not news, and framed as a deficiency
        // against a baseline that no longer exists. The behaviour it described
        // is now universal and belongs in the docs, not on each board card.
        LpFeature::DiagUnwind => CatalogNote::Silent,
        LpFeature::ShaderF32 => CatalogNote::Extra {
            present: "f32 shader math",
        },
    }
}

/// One build's features as the catalog says them: gaps first, extras second,
/// node runtimes rolled up. Mirrors the device card's gaps-only convention —
/// listing what every board has would bury the line that matters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildFeatureSummary {
    /// Gated node kinds whose runtime this build does not carry.
    pub missing_node_kinds: Vec<NodeKind>,
    /// Absent norms, in registry order ("no ESP-NOW radio").
    pub gaps: Vec<&'static str>,
    /// Present extras, in registry order ("f32 shader math").
    pub extras: Vec<&'static str>,
    /// The manifest's feature list, verbatim.
    pub features: &'static [LpFeature],
}

impl BuildFeatureSummary {
    /// Every gated node kind's runtime is present.
    pub fn runs_every_node_kind(&self) -> bool {
        self.missing_node_kinds.is_empty()
    }
}

/// Summarize the features of `build`'s package, or `None` when no manifest
/// fixture is checked in for it (say nothing rather than guess).
pub fn feature_summary(build: &FirmwareBuild) -> Option<BuildFeatureSummary> {
    let features = build_features(&build.package)?;
    let missing_node_kinds = NodeKind::ALL
        .iter()
        .copied()
        .filter(|kind| {
            LpFeature::for_node_kind(*kind).is_some_and(|feature| !features.contains(&feature))
        })
        .collect();
    let mut gaps = Vec::new();
    let mut extras = Vec::new();
    for feature in LpFeature::ALL {
        let present = features.contains(&feature);
        match catalog_note(feature) {
            CatalogNote::Norm { absent } if !present => gaps.push(absent),
            CatalogNote::Extra { present: text } if present => extras.push(text),
            CatalogNote::Norm { .. }
            | CatalogNote::Extra { .. }
            | CatalogNote::NodeRuntime
            | CatalogNote::Silent => {}
        }
    }
    Some(BuildFeatureSummary {
        missing_node_kinds,
        gaps,
        extras,
        features,
    })
}

/// Display label for a node kind. Same vocabulary as the studio's node
/// naming — this crate cannot depend on `lpa-studio-core`, and the labels are
/// user-facing product names, not identifiers.
pub fn node_kind_label(kind: NodeKind) -> &'static str {
    match kind {
        NodeKind::Module => "Module",
        NodeKind::Button => "Button",
        NodeKind::Clock => "Clock",
        NodeKind::Texture => "Texture",
        NodeKind::Shader => "Shader",
        NodeKind::ComputeShader => "Compute shader",
        NodeKind::Fluid => "Fluid",
        NodeKind::Playlist => "Playlist",
        NodeKind::ControlRadio => "Radio",
        NodeKind::Output => "Output",
        NodeKind::Fixture => "Fixture",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board_by_id;
    use crate::display_manifest::FirmwarePin;

    fn board(id: &str) -> BoardDisplayFile {
        board_by_id(id).expect("checked-in board").clone()
    }

    fn ids(matches: &[CompatibleBuild]) -> Vec<&str> {
        matches.iter().map(|m| m.build.id.as_str()).collect()
    }

    #[test]
    fn build_defs_parse() {
        let builds = all_builds();
        assert_eq!(builds.len(), BUILD_DEF_SOURCES.len());
        let c6 = build_by_id("esp32c6-4mb").expect("c6 build def");
        assert_eq!(c6.chip.name, "esp32c6");
        assert_eq!(c6.flash_size_mb, 4);
    }

    #[test]
    fn chip_mismatch_never_matches() {
        // A chip no checked-in build targets: synthesized, so this test
        // survives new firmware families landing (esp32v3 obsoleted the
        // original classic-ESP32 form of this test the day it shipped).
        let mut board = board("domraem/dom-z-102");
        board.family = "esp32c99".to_string();
        assert!(compatible_builds_for(&board).is_empty());
        assert_eq!(
            no_build_reason_for(&board),
            Some(NoBuildReason::NoBuildForChip)
        );
    }

    #[test]
    fn flash_too_small_is_rejected_with_a_reason() {
        let mut board = board("espressif/esp32-s3-devkitc-1");
        board.flash_mb = Some(4);
        assert!(compatible_builds_for(&board).is_empty());
        assert_eq!(
            no_build_reason_for(&board),
            Some(NoBuildReason::NeedsMoreFlash {
                needs_mb: 8,
                board_mb: 4,
            })
        );
    }

    #[test]
    fn exact_and_roomy_fits_both_match() {
        let xiao_c6 = board("seeed/xiao-esp32-c6");
        let matches = compatible_builds_for(&xiao_c6);
        assert_eq!(ids(&matches), ["esp32c6-4mb"]);
        assert_eq!(matches[0].headroom_mb, Some(0));
        assert_eq!(matches[0].basis, CompatibilityBasis::Computed);

        let xiao_s3 = board("seeed/xiao-esp32-s3-plus");
        let matches = compatible_builds_for(&xiao_s3);
        assert_eq!(ids(&matches), ["esp32s3-8mb"]);
        assert_eq!(matches[0].headroom_mb, Some(8));
    }

    #[test]
    fn unverified_flash_matches_nothing() {
        let mut board = board("seeed/xiao-esp32-c6");
        board.flash_mb = None;
        assert!(compatible_builds_for(&board).is_empty());
        assert_eq!(
            no_build_reason_for(&board),
            Some(NoBuildReason::FlashUnknown)
        );
    }

    #[test]
    fn an_allow_pin_relaxes_flash_but_not_chip() {
        let mut board = board("seeed/xiao-esp32-c6");
        board.flash_mb = Some(2);
        board.firmware_allow = vec![
            FirmwarePin {
                build_id: "esp32c6-4mb".into(),
                reason: "bench-verified with the shrunk partition table".into(),
            },
            // Wrong chip: a pin can never make an S3 image run on a C6.
            FirmwarePin {
                build_id: "esp32s3-8mb".into(),
                reason: "wishful thinking".into(),
            },
        ];
        let matches = compatible_builds_for(&board);
        assert_eq!(ids(&matches), ["esp32c6-4mb"]);
        assert!(matches!(
            matches[0].basis,
            CompatibilityBasis::Pinned { .. }
        ));
    }

    #[test]
    fn a_deny_pin_removes_a_computed_match() {
        let mut board = board("seeed/xiao-esp32-c6");
        board.firmware_deny = vec![FirmwarePin {
            build_id: "esp32c6-4mb".into(),
            reason: "this revision ships a 2 MB flash chip".into(),
        }];
        assert!(compatible_builds_for(&board).is_empty());
        assert_eq!(no_build_reason_for(&board), Some(NoBuildReason::AllDenied));
    }

    #[test]
    fn no_defs_means_no_matches() {
        let board = board("seeed/xiao-esp32-c6");
        assert!(compatible_builds(&board, &[]).is_empty());
        assert_eq!(
            no_build_reason(&board, &[]),
            Some(NoBuildReason::NoBuildForChip)
        );
    }

    fn served(ids: &[&str]) -> Vec<String> {
        ids.iter().map(|id| (*id).to_string()).collect()
    }

    fn picked(
        board: Option<&BoardDisplayFile>,
        chip: Option<&str>,
        served_ids: &[&str],
    ) -> Option<&'static str> {
        provisioning_build(board, chip, all_builds(), &served(served_ids))
            .map(|build| build.id.as_str())
    }

    /// All three served builds, as P4 ships them.
    const ALL_SERVED: &[&str] = &["esp32c6-4mb", "esp32s3-8mb", "esp32v3-4mb"];

    #[test]
    fn a_picked_board_selects_its_own_build() {
        let s3 = board("seeed/xiao-esp32-s3-plus");
        assert_eq!(
            picked(Some(&s3), Some("esp32s3"), ALL_SERVED),
            Some("esp32s3-8mb")
        );
        let classic = board("domraem/dom-z-102");
        assert_eq!(
            picked(Some(&classic), Some("esp32"), ALL_SERVED),
            Some("esp32v3-4mb")
        );
    }

    /// The generic-install path, and the common one: no board picked, but
    /// the probe named the chip. This is the case that made the old
    /// single-image flow write a C6 image onto everything.
    #[test]
    fn a_detected_chip_alone_resolves_a_build() {
        assert_eq!(
            picked(None, Some("esp32s3"), ALL_SERVED),
            Some("esp32s3-8mb")
        );
        assert_eq!(picked(None, Some("esp32"), ALL_SERVED), Some("esp32v3-4mb"));
        assert_eq!(
            picked(None, Some("esp32c6"), ALL_SERVED),
            Some("esp32c6-4mb")
        );
    }

    /// A pick that contradicts the chip is HONOURED, and the flash-time
    /// guard refuses it by name. Quietly resolving the detected chip instead
    /// would flash an S3 image while `/hardware.json` stamps the C6 board the
    /// user chose — a booting device with the wrong pin map, which is worse
    /// than a refusal that says "pick the board you actually have".
    #[test]
    fn a_board_that_contradicts_the_chip_is_honoured_and_refused_downstream() {
        let c6 = board("seeed/xiao-esp32-c6");
        assert_eq!(
            picked(Some(&c6), Some("esp32s3"), ALL_SERVED),
            Some("esp32c6-4mb")
        );
    }

    /// A board whose build this deployment does not serve resolves nothing
    /// on the board path, so the detected chip is what is left. (The picker
    /// filters on the served list, so this is a stale pick or a shrinking
    /// deployment, not a normal flow.)
    #[test]
    fn an_unserved_board_falls_back_to_the_detected_chip() {
        let c6 = board("seeed/xiao-esp32-c6");
        assert_eq!(
            picked(Some(&c6), Some("esp32s3"), &["esp32s3-8mb"]),
            Some("esp32s3-8mb")
        );
        // With no chip detected either, the board's own family is the last
        // evidence — and it is unserved, so: nothing.
        assert_eq!(picked(Some(&c6), None, &["esp32s3-8mb"]), None);
    }

    #[test]
    fn nothing_known_and_nothing_served_resolve_to_none() {
        assert_eq!(picked(None, None, ALL_SERVED), None);
        assert_eq!(picked(None, Some("esp32c3"), ALL_SERVED), None);
        // Served list gates the answer: the S3 build exists but is not
        // shipped, so the picker could not have offered it either.
        assert_eq!(picked(None, Some("esp32s3"), &["esp32c6-4mb"]), None);
    }

    /// Without a board there is no verified flash size, so the smallest
    /// declared image — the one most likely to fit — wins. Synthesized,
    /// because no chip has two served builds today and this rule is exactly
    /// what will be untested the day one does.
    #[test]
    fn chip_only_selection_prefers_the_smallest_image() {
        static TWO_C6_BUILDS: OnceLock<Vec<FirmwareBuild>> = OnceLock::new();
        let builds = TWO_C6_BUILDS.get_or_init(|| {
            let base = build_by_id("esp32c6-4mb").expect("c6 build def").clone();
            vec![
                FirmwareBuild {
                    id: "esp32c6-8mb".into(),
                    flash_size_mb: 8,
                    ..base.clone()
                },
                base,
            ]
        });
        let served_both = served(&["esp32c6-4mb", "esp32c6-8mb"]);
        assert_eq!(
            provisioning_build(None, Some("esp32c6"), builds, &served_both).map(|b| b.id.as_str()),
            Some("esp32c6-4mb")
        );
        // A board, though, HAS a verified flash size — 8 MB of headroom is
        // the better fit, and `compatible_builds` already ranks it first.
        let mut roomy = board("seeed/xiao-esp32-c6");
        roomy.flash_mb = Some(8);
        assert_eq!(
            provisioning_build(Some(&roomy), Some("esp32c6"), builds, &served_both)
                .map(|b| b.id.as_str()),
            Some("esp32c6-8mb")
        );
    }

    #[test]
    fn features_come_from_the_manifest_fixtures() {
        let c6 = feature_summary(build_by_id("esp32c6-4mb").unwrap()).expect("c6 fixture");
        assert!(c6.runs_every_node_kind());
        assert!(c6.gaps.is_empty(), "unexpected C6 gaps: {:?}", c6.gaps);
        assert!(
            c6.extras.is_empty(),
            "unexpected C6 extras: {:?}",
            c6.extras
        );

        let s3 = feature_summary(build_by_id("esp32s3-8mb").unwrap()).expect("s3 fixture");
        assert!(s3.runs_every_node_kind());
        assert_eq!(s3.extras, ["f32 shader math"]);
        assert!(s3.gaps.contains(&"no ESP-NOW radio"), "{:?}", s3.gaps);
    }

    #[test]
    fn every_build_def_has_a_manifest_fixture() {
        for build in all_builds() {
            assert!(
                build_features(&build.package).is_some(),
                "no manifest-core fixture embedded for {}",
                build.package
            );
        }
    }
}
