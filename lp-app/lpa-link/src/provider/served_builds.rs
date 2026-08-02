//! Which firmware builds this Studio deployment actually serves.
//!
//! Deployment fact, not board data: the site's assets package
//! `firmware/<build id>/` directories (justfile `studio_firmware_build`),
//! and the browser flash provider's default manifest path points into the
//! one build shipped today (see
//! `browser_serial_esp32_options::DEFAULT_ESP32C6_FIRMWARE_MANIFEST_PATH` —
//! keep the two in lockstep). Target-unconditional so host-built UI code
//! (the provisioning picker, its stories and view tests) can read it.

/// The build ids the Studio site serves — the provisioning picker's
/// eligibility filter (board-selection M5): a board is only pickable when
/// one of its compatible builds (`lpa_boards::compatible_builds_for`) is in
/// this list. Per-request build selection lands when this grows past one.
pub const SERVED_FIRMWARE_BUILDS: &[&str] = &["esp32c6-4mb"];
