//! Where an update's time goes on the board: one `[OTA] timing` line per
//! piece (at its commit) and per read-back (at the reset that ends it).
//!
//! The flash side is counted in [`super::update_target_impl`] (every erase,
//! program and read the session asks for); the edge counts the messages,
//! the time spent inside the session for each, and the gaps between them —
//! the time the board waited on the link. Clocks are this edge's, never
//! the session's (sans-IO).
//!
//! `[OTA] timing <what>: <n> msgs in <wall> ms, session <s> ms, waiting <w>
//! ms, longest <l> ms`, then `[OTA] timing <what> ms/ops: erase <ms>/<n>
//! block <ms>/<n> program <ms>/<n> read <ms>/<n> hash <ms> other <ms>` (other:
//! inside the session but not in flash or the hash — inflate, mostly).

/// Microseconds since boot.
pub fn now_us() -> u64 {
    embassy_time::Instant::now().as_micros()
}

/// The flash operations the session asked for.
#[derive(Clone, Copy, Debug, Default)]
pub struct FlashTiming {
    pub erase_us: u64,
    pub erases: u32,
    pub block_us: u64,
    pub blocks: u32,
    pub program_us: u64,
    pub programs: u32,
    pub read_us: u64,
    pub reads: u32,
    /// The piece's SHA-256 on the accelerator, its reads included.
    pub hash_us: u64,
}

impl FlashTiming {
    fn total_us(&self) -> u64 {
        self.erase_us + self.block_us + self.program_us + self.read_us + self.hash_us
    }
}

/// The messages of one piece (or one read-back).
#[derive(Clone, Copy, Debug, Default)]
pub struct MessageTiming {
    pub messages: u32,
    first_at: Option<u64>,
    last_end: Option<u64>,
    pub session_us: u64,
    pub waiting_us: u64,
    pub longest_us: u64,
}

impl MessageTiming {
    /// One message handled from `start` to `end`.
    pub fn note(&mut self, start: u64, end: u64) {
        self.messages += 1;
        self.first_at.get_or_insert(start);
        if let Some(prev) = self.last_end {
            self.waiting_us += start.saturating_sub(prev);
        }
        self.last_end = Some(end);
        let took = end.saturating_sub(start);
        self.session_us += took;
        self.longest_us = self.longest_us.max(took);
    }

    /// Log the line for `what`, with the flash work done meanwhile.
    pub fn log(&self, what: &str, flash: &FlashTiming) {
        if self.messages == 0 {
            return;
        }
        let wall = match (self.first_at, self.last_end) {
            (Some(a), Some(b)) => b.saturating_sub(a),
            _ => 0,
        };
        let ms = |us: u64| us / 1000;
        // Two short lines: the link's log channel cuts a long one.
        log::info!(
            "[OTA] timing {what}: {} msgs in {} ms, session {} ms, waiting {} ms, longest {} ms",
            self.messages,
            ms(wall),
            ms(self.session_us),
            ms(self.waiting_us),
            ms(self.longest_us),
        );
        log::info!(
            "[OTA] timing {what} ms/ops: erase {}/{} block {}/{} program {}/{} read {}/{} hash {} other {}",
            ms(flash.erase_us),
            flash.erases,
            ms(flash.block_us),
            flash.blocks,
            ms(flash.program_us),
            flash.programs,
            ms(flash.read_us),
            flash.reads,
            ms(flash.hash_us),
            ms(self.session_us.saturating_sub(flash.total_us())),
        );
    }
}
