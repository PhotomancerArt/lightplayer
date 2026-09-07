//! What hardware a project declares it is for — read from the container
//! manifest's advisory `target`, with a read-time default.
//!
//! `target` has been in `project.json` since 2026-08-05 (ADR
//! `2026-08-05-project-target-metadata`) and has been advisory ever since:
//! nothing read it. This is where it starts to mean something — the sim
//! boots wearing the target's board manifest — and the read-time default is
//! what makes that possible without touching a single persisted byte:
//!
//! **An absent `target` reads as [`ProjectTarget::Desktop`].** It is not a
//! format bump and not a migration; every checked-in project still says
//! nothing about hardware, and every one of them opens on the desktop
//! firmware, which is what they have always effectively done.

/// The board id of the Desktop board — a computer running the desktop
/// firmware. Not silicon; see `boards/lightplayer/desktop.json`.
pub const DESKTOP_BOARD_ID: &str = "lightplayer/desktop";

/// The hardware a project is for.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectTarget {
    /// A computer: `fw-browser` in this tab, `fw-host` on a machine. The
    /// default for a project that names no target.
    Desktop,
    /// A catalog board, by its id (`seeed/xiao-esp32-c6`).
    Board(String),
}

impl ProjectTarget {
    /// Read a container manifest's `target` field. Absent (or blank) reads
    /// as [`Self::Desktop`] — the whole point of this type.
    pub fn from_manifest(target: Option<&str>) -> Self {
        match target.map(str::trim) {
            None | Some("") => Self::Desktop,
            Some(DESKTOP_BOARD_ID) => Self::Desktop,
            Some(board_id) => Self::Board(board_id.to_string()),
        }
    }

    /// The catalog board id this target names. Desktop has one too — it is
    /// a board file like any other, which is what lets every existing path
    /// (the catalog, the runtime manifest lookup, the display name) work
    /// unchanged.
    pub fn board_id(&self) -> &str {
        match self {
            Self::Desktop => DESKTOP_BOARD_ID,
            Self::Board(board_id) => board_id,
        }
    }

    /// Read a container manifest's bytes for its `target`, leniently.
    ///
    /// Deliberately not the strict manifest reader: this answers "what
    /// hardware is this for?" ahead of an open, and a manifest this cannot
    /// read is the open's problem to report, not this function's. Anything
    /// unreadable reads as [`Self::Desktop`], the same as an absent target.
    pub fn from_manifest_bytes(bytes: &[u8]) -> Self {
        let target = serde_json::from_slice::<serde_json::Value>(bytes)
            .ok()
            .and_then(|value| {
                value
                    .get("target")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
            });
        Self::from_manifest(target.as_deref())
    }

    /// The target a sim can actually wear, plus the notice when that is not
    /// the one asked for.
    ///
    /// A board with no checked-in runtime manifest (a display-only catalog
    /// entry, or an id nobody recognizes) cannot be simulated honestly, so
    /// the sim runs as Desktop and SAYS so. Guessing a board's table would
    /// refuse endpoints for reasons no one could explain.
    pub fn resolve_for_sim(&self) -> (Self, Option<String>) {
        if self.runtime_manifest_json().is_some() {
            return (self.clone(), None);
        }
        (
            Self::Desktop,
            Some(format!(
                "no hardware profile is checked in for {} — the simulator runs as Desktop",
                self.board_id()
            )),
        )
    }

    /// The checked-in runtime manifest for this target, verbatim.
    ///
    /// `None` when the target names a board with no runtime manifest — a
    /// display-only catalog entry, or a board id nobody recognizes. The
    /// caller falls back to Desktop WITH A NOTICE rather than guessing a
    /// board: running a project on a table nobody authored would refuse
    /// endpoints for reasons no one could explain.
    pub fn runtime_manifest_json(&self) -> Option<&'static str> {
        lpa_boards::runtime_manifest_json(self.board_id())
    }

    /// Boot options for a runtime wearing this target on `tier` — the board
    /// manifest as text, and no identity (P1 mints none).
    ///
    /// `None` for a target with no runtime manifest; [`Self::resolve_for_sim`]
    /// is what a caller does about that. Present only where there is a
    /// browser worker to boot.
    #[cfg(all(feature = "browser-worker", target_arch = "wasm32"))]
    pub fn runtime_options(
        &self,
        tier: lpa_link::providers::browser_worker::BrowserRuntimeTier,
    ) -> Option<lpa_link::providers::browser_worker::BrowserRuntimeOptions> {
        self.runtime_manifest_json().map(|manifest| {
            lpa_link::providers::browser_worker::BrowserRuntimeOptions::new(tier, manifest)
        })
    }
}

impl Default for ProjectTarget {
    fn default() -> Self {
        Self::Desktop
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_absent_target_reads_as_desktop() {
        assert_eq!(ProjectTarget::from_manifest(None), ProjectTarget::Desktop);
        assert_eq!(
            ProjectTarget::from_manifest(Some("  ")),
            ProjectTarget::Desktop
        );
        // Written out longhand, it is still the same target — a project
        // whose Hardware row says "Desktop" must not read as a board.
        assert_eq!(
            ProjectTarget::from_manifest(Some(DESKTOP_BOARD_ID)),
            ProjectTarget::Desktop
        );
    }

    #[test]
    fn a_named_board_reads_as_that_board() {
        let target = ProjectTarget::from_manifest(Some("seeed/xiao-esp32-c6"));

        assert_eq!(target, ProjectTarget::Board("seeed/xiao-esp32-c6".into()));
        assert_eq!(target.board_id(), "seeed/xiao-esp32-c6");
    }

    /// Desktop always resolves; so does every board with a checked-in
    /// runtime manifest.
    #[test]
    fn targets_with_a_manifest_resolve_to_its_bytes() {
        let desktop = ProjectTarget::Desktop
            .runtime_manifest_json()
            .expect("the Desktop board has a runtime manifest");
        assert!(desktop.contains("\"lightplayer/desktop\""));
        assert!(desktop.contains("\"target\": \"desktop\""));

        let c6 = ProjectTarget::from_manifest(Some("seeed/xiao-esp32-c6"))
            .runtime_manifest_json()
            .expect("the C6 has a runtime manifest");
        assert!(c6.contains("\"seeed/xiao-esp32-c6\""));
    }

    /// A display-only board and an unknown id both answer `None`: the
    /// caller's cue to fall back to Desktop and say so.
    #[test]
    fn a_target_without_a_manifest_answers_none() {
        assert_eq!(
            ProjectTarget::from_manifest(Some("quinled/dig-uno")).runtime_manifest_json(),
            None
        );
        assert_eq!(
            ProjectTarget::from_manifest(Some("nobody/nothing")).runtime_manifest_json(),
            None
        );
    }

    #[test]
    fn an_unsimulatable_board_falls_back_to_desktop_with_a_notice() {
        let (worn, notice) =
            ProjectTarget::from_manifest(Some("quinled/dig-uno")).resolve_for_sim();

        assert_eq!(worn, ProjectTarget::Desktop);
        let notice = notice.expect("the fallback must say so");
        assert!(notice.contains("quinled/dig-uno"), "{notice}");
        assert!(notice.contains("Desktop"), "{notice}");

        let (worn, notice) =
            ProjectTarget::from_manifest(Some("seeed/xiao-esp32-c6")).resolve_for_sim();
        assert_eq!(worn, ProjectTarget::Board("seeed/xiao-esp32-c6".into()));
        assert_eq!(notice, None, "a board with a profile is worn as asked");
    }

    #[test]
    fn manifest_bytes_read_leniently() {
        assert_eq!(
            ProjectTarget::from_manifest_bytes(br#"{"format":6,"name":"x"}"#),
            ProjectTarget::Desktop
        );
        assert_eq!(
            ProjectTarget::from_manifest_bytes(br#"{"format":6,"target":"seeed/xiao-esp32-c6"}"#),
            ProjectTarget::Board("seeed/xiao-esp32-c6".into())
        );
        // Unreadable bytes are the OPEN's problem to report, not this
        // function's: they read as the default.
        assert_eq!(
            ProjectTarget::from_manifest_bytes(b"not json at all"),
            ProjectTarget::Desktop
        );
    }

    /// The invariant behind the sim's `as <board>` line: the board id the
    /// project names is the id INSIDE the manifest the sim then wears — so
    /// the hello `fw-browser` sends (it reports `manifest.board_id()`) says
    /// the same thing the host recorded from the project's `target`.
    #[test]
    fn the_worn_manifest_names_the_same_board_the_target_does() {
        for target in [
            ProjectTarget::Desktop,
            ProjectTarget::from_manifest(Some("seeed/xiao-esp32-c6")),
            ProjectTarget::from_manifest(Some("domraem/dom-z-102")),
        ] {
            let manifest = target
                .runtime_manifest_json()
                .expect("a simulatable target has a manifest");
            let value: serde_json::Value =
                serde_json::from_str(manifest).expect("the checked-in manifest parses");
            assert_eq!(
                value["id"].as_str(),
                Some(target.board_id()),
                "{}: the manifest must name the board the target does",
                target.board_id()
            );
        }
    }
}
