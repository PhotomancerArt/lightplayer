//! **A board reports exactly its release's identity** (one-way-doors §2,
//! AC13): what a board running release R says in its board manifest (`M`,
//! or the hello's `firmware` block) equals what R's `ota-manifest.json`
//! publishes, field by field:
//!
//! | Board manifest | `ota-manifest.json` | Rule |
//! |---|---|---|
//! | `target`, `chip`, `version`, `wireProto` | same keys | equal |
//! | `buildId` | [`OtaManifest::build_id`] | equal |
//! | `coreSha256`, `coreLen` | `core.sha256`, `core.length` | equal |
//! | `engineSha256` | `engine.sha256` | equal |
//! | `engineLen` | `engine.length` | equal when reported (`null` only on an engine-less board) |
//! | `layout` | `requires.layout` | equal |
//! | `loader` | `requires.loader` | the board's **≥** the manifest's |
//!
//! `proto`, `state`, `refusedBuild`, `transfer` and `regionLen` are the
//! board's alone. The emulator scenario U17 runs this against a packaged
//! image; a host may use it to say "this board runs exactly release R".

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use lpc_firmware_release::OtaManifest;
use lpc_update::BoardManifest;

/// One field where the board and the release disagree: the board
/// manifest's key, and both values as text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IdentityMismatch {
    pub field: &'static str,
    pub board: String,
    pub release: String,
}

/// Whether `board` reports exactly `release`'s identity; every field that
/// does not, otherwise. See the module docs for the rules.
pub fn board_matches_release(
    board: &BoardManifest,
    release: &OtaManifest,
) -> Result<(), Vec<IdentityMismatch>> {
    let mut out = Vec::new();
    let mut equal = |field: &'static str, b: String, r: String| {
        if b != r {
            out.push(IdentityMismatch {
                field,
                board: b,
                release: r,
            });
        }
    };
    equal("target", board.target.clone(), release.target.clone());
    equal("chip", board.chip.clone(), release.chip.clone());
    equal("version", board.version.clone(), release.version.clone());
    equal("buildId", board.build_id.clone(), release.build_id());
    equal(
        "wireProto",
        board.wire_proto.to_string(),
        release.wire_proto.to_string(),
    );
    equal(
        "coreSha256",
        board.core_sha256.clone(),
        release.core.sha256.clone(),
    );
    equal(
        "coreLen",
        board.core_len.to_string(),
        release.core.length.to_string(),
    );
    equal(
        "engineSha256",
        board.engine_sha256.clone(),
        release.engine.sha256.clone(),
    );
    if let Some(len) = board.engine_len {
        equal(
            "engineLen",
            len.to_string(),
            release.engine.length.to_string(),
        );
    }
    equal(
        "layout",
        board.layout.to_string(),
        release.requires.layout.to_string(),
    );
    if board.loader < release.requires.loader {
        out.push(IdentityMismatch {
            field: "loader",
            board: board.loader.to_string(),
            release: alloc::format!("≥ {}", release.requires.loader),
        });
    }
    if out.is_empty() { Ok(()) } else { Err(out) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host_build_from_ota_manifest::tests::release;
    use lpc_update::BoardState;

    /// The board manifest the release's own board would send.
    fn board_of(release: &OtaManifest) -> BoardManifest {
        BoardManifest {
            proto: 1,
            target: release.target.clone(),
            chip: release.chip.clone(),
            version: release.version.clone(),
            build_id: release.build_id(),
            wire_proto: release.wire_proto,
            core_sha256: release.core.sha256.clone(),
            core_len: release.core.length as u32,
            engine_sha256: release.engine.sha256.clone(),
            engine_len: Some(release.engine.length as u32),
            layout: release.requires.layout,
            loader: release.requires.loader,
            region_len: 3_375_104,
            state: BoardState::Running,
            refused_build: None,
            transfer: None,
        }
    }

    fn mismatched(b: &BoardManifest, r: &OtaManifest) -> Vec<&'static str> {
        match board_matches_release(b, r) {
            Ok(()) => Vec::new(),
            Err(m) => m.iter().map(|m| m.field).collect(),
        }
    }

    #[test]
    fn the_releases_own_board_matches() {
        let (r, _) = release(true);
        assert_eq!(board_matches_release(&board_of(&r), &r), Ok(()));
    }

    #[test]
    fn an_engine_less_board_still_matches_without_its_length() {
        let (r, _) = release(true);
        let mut b = board_of(&r);
        b.engine_len = None;
        b.state = BoardState::NeedsEngine;
        assert_eq!(board_matches_release(&b, &r), Ok(()));
    }

    #[test]
    fn a_newer_loader_matches_and_an_older_one_does_not() {
        let (r, _) = release(false);
        let mut b = board_of(&r);
        b.loader = 2;
        assert_eq!(board_matches_release(&b, &r), Ok(()));
        b.loader = 0;
        assert_eq!(mismatched(&b, &r), ["loader"]);
    }

    #[test]
    fn each_field_is_named_when_it_differs() {
        let (r, _) = release(false);
        type Edit = fn(&mut BoardManifest);
        let cases: [(&str, Edit); 10] = [
            ("target", |b| b.target = "esp32c6-8mb".into()),
            ("chip", |b| b.chip = "esp32s3".into()),
            ("version", |b| b.version = "2026.10.05-4".into()),
            ("buildId", |b| b.build_id.push('x')),
            ("wireProto", |b| b.wire_proto += 1),
            ("coreSha256", |b| b.core_sha256 = "0".repeat(64)),
            ("coreLen", |b| b.core_len += 1),
            ("engineSha256", |b| b.engine_sha256 = "1".repeat(64)),
            ("engineLen", |b| b.engine_len = Some(1)),
            ("layout", |b| b.layout = 2),
        ];
        for (field, edit) in cases {
            let mut b = board_of(&r);
            edit(&mut b);
            assert_eq!(mismatched(&b, &r), [field], "{field}");
        }
    }
}
