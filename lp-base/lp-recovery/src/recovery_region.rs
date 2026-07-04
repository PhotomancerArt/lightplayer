//! The persistent breadcrumb region: layout, validation, reinit.

use crate::crash_record::CrashRecord;
use crate::frame_path::MAX_FRAME_DEPTH;
use crate::frame_record::FrameRecord;
use crate::ledger::Ledger;
use crate::reset_cause::ResetCause;

/// "LPRC" — identifies an initialized region.
pub const REGION_MAGIC: u32 = 0x4C50_5243;

/// Bump on any layout change; old regions are discarded, never migrated.
pub const REGION_VERSION: u16 = 1;

/// Hard budget: the region must stay within 1 KB of RTC fast RAM.
pub const REGION_MAX_SIZE: usize = 1024;

/// The persistent recovery region.
///
/// Lives in RTC fast RAM on ESP32 (survives software/watchdog resets, NOT
/// power-off), in an exported static on the emulator, and in ordinary
/// buffers on host targets. All fields are plain integers/arrays so the
/// bytes are meaningful across reboots and reinterpretable by the emulator
/// test harness.
///
/// # Integrity
///
/// - `header_crc` covers identity fields (magic, version, generation) and
///   `body_crc` covers boot bookkeeping. Both change only at boot time or
///   at the boot-complete milestone — never on hot paths.
/// - The frame stack (`depth` + `frames`) and the crash record are
///   deliberately **not** CRC-covered: they change on hot paths and a
///   watchdog reset may interrupt a write by design. They use single-word
///   visibility flips (torn-write discipline) instead.
#[repr(C)]
pub struct RecoveryRegion {
    // --- identity (header_crc) ---
    magic: u32,
    version: u16,
    _pad: u16,
    generation: u32,
    header_crc: u32,
    // --- boot bookkeeping (body_crc) ---
    boot_count: u32,
    boot_complete: u32,
    body_crc: u32,
    _reserved_bookkeeping: u32,
    // --- hot area: torn-write discipline, no CRC ---
    depth: u32,
    frames: [FrameRecord; MAX_FRAME_DEPTH],
    crash: CrashRecord,
    // --- blame ledger (crash-time updates; torn-tolerant, no CRC) ---
    ledger: Ledger,
}

impl RecoveryRegion {
    /// An all-zeros region: invalid (magic mismatch) until `reinit`.
    pub const ZEROED: Self = Self {
        magic: 0,
        version: 0,
        _pad: 0,
        generation: 0,
        header_crc: 0,
        boot_count: 0,
        boot_complete: 0,
        body_crc: 0,
        _reserved_bookkeeping: 0,
        depth: 0,
        frames: [FrameRecord::EMPTY; MAX_FRAME_DEPTH],
        crash: CrashRecord::EMPTY,
        ledger: Ledger::EMPTY,
    };

    /// Whether the region carries valid state from a previous run.
    ///
    /// Power-on renders the region invalid by definition — RTC RAM contents
    /// are undefined after power loss, and a lucky CRC match must not
    /// resurrect stale blame.
    pub fn is_valid(&self, cause: ResetCause) -> bool {
        cause != ResetCause::PowerOn
            && self.magic == REGION_MAGIC
            && self.version == REGION_VERSION
            && self.header_crc == self.compute_header_crc()
            && self.body_crc == self.compute_body_crc()
    }

    /// Reset the whole region to a fresh, valid, empty state.
    pub fn reinit(&mut self) {
        *self = Self::ZEROED;
        self.magic = REGION_MAGIC;
        self.version = REGION_VERSION;
        self.update_crcs();
    }

    pub fn generation(&self) -> u32 {
        self.generation
    }

    pub fn boot_count(&self) -> u32 {
        self.boot_count
    }

    pub fn boot_complete(&self) -> bool {
        self.boot_complete != 0
    }

    /// Begin a new boot generation. Clears the per-run milestone flag.
    pub(crate) fn begin_generation(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.boot_count = self.boot_count.saturating_add(1);
        self.boot_complete = 0;
        self.update_crcs();
    }

    pub(crate) fn set_boot_complete(&mut self) {
        self.boot_complete = 1;
        self.update_crcs();
    }

    pub(crate) fn crash(&self) -> &CrashRecord {
        &self.crash
    }

    pub(crate) fn crash_mut(&mut self) -> &mut CrashRecord {
        &mut self.crash
    }

    pub(crate) fn depth(&self) -> u32 {
        self.depth
    }

    pub(crate) fn set_depth(&mut self, depth: u32) {
        self.depth = depth;
    }

    pub(crate) fn frames(&self) -> &[FrameRecord; MAX_FRAME_DEPTH] {
        &self.frames
    }

    pub(crate) fn frames_mut(&mut self) -> &mut [FrameRecord; MAX_FRAME_DEPTH] {
        &mut self.frames
    }

    pub(crate) fn ledger(&self) -> &Ledger {
        &self.ledger
    }

    pub(crate) fn ledger_mut(&mut self) -> &mut Ledger {
        &mut self.ledger
    }

    /// Size of the region in bytes (for harness-side raw access).
    pub const SIZE: usize = core::mem::size_of::<Self>();

    /// Copy a region out of raw bytes (e.g. emulator guest RAM read by a
    /// test harness). All fields are plain integers, so any bit pattern is
    /// a sound (if possibly invalid) region; validity is still governed by
    /// [`Self::is_valid`].
    pub fn read_from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < Self::SIZE {
            return None;
        }
        let mut region = Self::ZEROED;
        // SAFETY: repr(C) POD (integers and arrays thereof); sizes checked.
        unsafe {
            core::ptr::copy_nonoverlapping(
                bytes.as_ptr(),
                (&raw mut region).cast::<u8>(),
                Self::SIZE,
            );
        }
        Some(region)
    }

    /// Serialize the region into raw bytes (harness-side restore into guest
    /// RAM). Returns `false` if `out` is too small.
    pub fn write_to_bytes(&self, out: &mut [u8]) -> bool {
        if out.len() < Self::SIZE {
            return false;
        }
        // SAFETY: repr(C) POD; sizes checked.
        unsafe {
            core::ptr::copy_nonoverlapping(
                (self as *const Self).cast::<u8>(),
                out.as_mut_ptr(),
                Self::SIZE,
            );
        }
        true
    }

    /// Snapshot for out-of-band inspection (test harnesses reading guest
    /// memory). The reset cause is not stored in the region, so it reads
    /// as `Unknown` here.
    pub fn inspect(&self) -> crate::snapshot::RecoverySnapshot {
        crate::snapshot::RecoverySnapshot::capture(self, ResetCause::Unknown)
    }

    fn update_crcs(&mut self) {
        self.header_crc = self.compute_header_crc();
        self.body_crc = self.compute_body_crc();
    }

    fn compute_header_crc(&self) -> u32 {
        let mut crc = Crc32::new();
        crc.update(&self.magic.to_le_bytes());
        crc.update(&self.version.to_le_bytes());
        crc.update(&self.generation.to_le_bytes());
        crc.finish()
    }

    fn compute_body_crc(&self) -> u32 {
        let mut crc = Crc32::new();
        crc.update(&self.boot_count.to_le_bytes());
        crc.update(&self.boot_complete.to_le_bytes());
        crc.finish()
    }
}

/// Small bitwise CRC32 (IEEE, reflected). No table: this runs a handful of
/// times per boot on a few dozen bytes; 8 iterations/byte is nothing, and a
/// 1 KB table in flash/RAM would cost more than it saves.
struct Crc32(u32);

impl Crc32 {
    fn new() -> Self {
        Self(0xFFFF_FFFF)
    }

    fn update(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 ^= u32::from(b);
            for _ in 0..8 {
                self.0 = if self.0 & 1 != 0 {
                    (self.0 >> 1) ^ 0xEDB8_8320
                } else {
                    self.0 >> 1
                };
            }
        }
    }

    fn finish(self) -> u32 {
        self.0 ^ 0xFFFF_FFFF
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn region_fits_the_budget() {
        let size = core::mem::size_of::<RecoveryRegion>();
        assert!(
            size <= REGION_MAX_SIZE,
            "RecoveryRegion is {size} bytes, budget is {REGION_MAX_SIZE}"
        );
    }

    #[test]
    fn zeroed_region_is_invalid_reinit_makes_it_valid() {
        let mut region = RecoveryRegion::ZEROED;
        assert!(!region.is_valid(ResetCause::SoftwareReset));
        region.reinit();
        assert!(region.is_valid(ResetCause::SoftwareReset));
        assert!(region.is_valid(ResetCause::WatchdogReset));
    }

    #[test]
    fn power_on_invalidates_even_a_well_formed_region() {
        let mut region = RecoveryRegion::ZEROED;
        region.reinit();
        assert!(!region.is_valid(ResetCause::PowerOn));
    }

    #[test]
    fn corrupted_identity_fails_validation() {
        let mut region = RecoveryRegion::ZEROED;
        region.reinit();
        region.magic ^= 1;
        assert!(!region.is_valid(ResetCause::SoftwareReset));

        let mut region = RecoveryRegion::ZEROED;
        region.reinit();
        region.generation = 99; // not reflected in header_crc
        assert!(!region.is_valid(ResetCause::SoftwareReset));
    }

    #[test]
    fn generation_and_boot_flags_round_trip_through_crc() {
        let mut region = RecoveryRegion::ZEROED;
        region.reinit();
        region.begin_generation();
        region.set_boot_complete();
        assert!(region.is_valid(ResetCause::SoftwareReset));
        assert_eq!(region.generation(), 1);
        assert_eq!(region.boot_count(), 1);
        assert!(region.boot_complete());
    }

    #[test]
    fn crc32_matches_known_vector() {
        // CRC-32/ISO-HDLC of "123456789" is 0xCBF43926.
        let mut crc = Crc32::new();
        crc.update(b"123456789");
        assert_eq!(crc.finish(), 0xCBF4_3926);
    }
}
