//! The lender: one holder at a time, by priority, at safe points.

use crate::{Ask, Block, BlockClose, LenderStats, Loan, LoanId, LoanKind, Refusal, Tenant};

/// The block is kept for a refused taker that outranks whoever asks next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reservation {
    /// Who it is kept for.
    pub kind: LoanKind,
    /// The last tick it holds through; [`Lender::begin_tick`] drops it after.
    pub until_tick: u64,
}

/// Holds the right to the big block and lends it.
///
/// One holder at a time. A refused taker that outranks the holder reserves
/// the next grant for [`Lender::reservation_ticks`] ticks. A grant that
/// does not fit beside the block's tenants purges them, in the order the
/// caller passes them, until it fits. All calls are synchronous, on the
/// caller's thread.
#[derive(Debug)]
pub struct Lender {
    capacity: u32,
    overflow_allowance: u32,
    reservation_ticks: u64,
    held: Option<Held>,
    reservation: Option<Reservation>,
    tick: u64,
    next_id: u32,
    stats: LenderStats,
}

#[derive(Clone, Copy, Debug)]
struct Held {
    id: LoanId,
    kind: LoanKind,
    tick: u64,
    leak_counted: bool,
}

impl Lender {
    /// A lender for a block of `capacity` bytes. A job may count on
    /// `overflow_allowance` bytes of the general heap beyond the block for
    /// its working set (0: the whole working set must fit in the block).
    pub fn new(capacity: u32, overflow_allowance: u32) -> Self {
        Self {
            capacity,
            overflow_allowance,
            reservation_ticks: 4,
            held: None,
            reservation: None,
            tick: 0,
            next_id: 1,
            stats: LenderStats::default(),
        }
    }

    /// How many ticks a reservation holds (default 4).
    pub fn with_reservation_ticks(mut self, ticks: u64) -> Self {
        self.reservation_ticks = ticks;
        self
    }

    /// The block's size.
    pub fn capacity(&self) -> u32 {
        self.capacity
    }

    /// Who holds the block, if anyone.
    pub fn holder(&self) -> Option<LoanKind> {
        self.held.map(|held| held.kind)
    }

    /// The standing reservation, if any.
    pub fn reservation(&self) -> Option<Reservation> {
        self.reservation
    }

    /// What the lender has done.
    pub fn stats(&self) -> &LenderStats {
        &self.stats
    }

    /// How many ticks a reservation holds.
    pub fn reservation_ticks(&self) -> u64 {
        self.reservation_ticks
    }

    /// The safe point at the top of tick `tick`: an expired reservation is
    /// dropped, and a loan still held from an earlier tick is counted as a
    /// leak (once), unless its kind spans ticks.
    pub fn begin_tick(&mut self, tick: u64) {
        self.tick = tick;
        if let Some(reservation) = self.reservation
            && tick > reservation.until_tick
        {
            self.reservation = None;
            self.stats.reservations_expired += 1;
        }
        if let Some(held) = self.held.as_mut()
            && held.tick < tick
            && !held.kind.spans_ticks()
            && !held.leak_counted
        {
            held.leak_counted = true;
            self.stats.leaked += 1;
        }
    }

    /// Lend the block for `ask`, purging tenants (in the order given,
    /// cheapest to rebuild first) only as far as the grant needs.
    pub fn try_lend(
        &mut self,
        ask: Ask,
        block: &mut dyn Block,
        tenants: &mut [&mut dyn Tenant],
    ) -> Result<Loan, Refusal> {
        let result = self.decide(ask, block, tenants);
        let stats = self.stats.kind_mut(ask.kind);
        match &result {
            Ok(_) => stats.granted += 1,
            Err(Refusal::Busy { .. }) => stats.busy += 1,
            Err(Refusal::Reserved { .. }) => stats.reserved += 1,
            Err(Refusal::TooBig { .. }) => stats.too_big += 1,
            Err(Refusal::NoRoom { .. }) => stats.no_room += 1,
        }
        result
    }

    /// Return the block. The edge's [`Block::close`] says what the loan did.
    ///
    /// # Panics
    ///
    /// If `loan` is not the loan the lender holds (a loan from another
    /// lender): a programming error.
    pub fn release(&mut self, loan: Loan, block: &mut dyn Block) -> BlockClose {
        let held = self.held.take().expect("release: no loan is held");
        assert_eq!(held.id, loan.id, "release: not the held loan");
        let close = block.close(loan.id);
        let stats = self.stats.kind_mut(loan.kind);
        stats.peak_in_block = stats.peak_in_block.max(close.peak_in_block);
        stats.overflow = stats.overflow.saturating_add(close.overflow);
        if close.overflow > 0 {
            stats.overflowed_loans += 1;
        }
        stats.survivors = stats.survivors.saturating_add(close.survivors);
        close
    }

    fn decide(
        &mut self,
        ask: Ask,
        block: &mut dyn Block,
        tenants: &mut [&mut dyn Tenant],
    ) -> Result<Loan, Refusal> {
        if ask.largest > self.capacity
            || ask.total > self.capacity.saturating_add(self.overflow_allowance)
        {
            return Err(Refusal::TooBig {
                largest: ask.largest,
                total: ask.total,
                capacity: self.capacity,
            });
        }
        if let Some(held) = self.held {
            if ask.kind.outranks(held.kind) {
                self.reserve(ask.kind);
            }
            return Err(Refusal::Busy { holder: held.kind });
        }
        if let Some(reservation) = self.reservation
            && reservation.kind != ask.kind
            && reservation.kind.outranks(ask.kind)
        {
            return Err(Refusal::Reserved {
                for_kind: reservation.kind,
            });
        }
        let mut purged_any = false;
        if !self.fits(ask, block) && ask.kind.may_purge() {
            for tenant in tenants.iter_mut() {
                if tenant.pinned() || tenant.resident() == 0 {
                    continue;
                }
                let freed = tenant.purge();
                self.stats.purges += 1;
                self.stats.purged_bytes = self.stats.purged_bytes.saturating_add(freed);
                purged_any = true;
                if self.fits(ask, block) {
                    break;
                }
            }
        }
        if !self.fits(ask, block) {
            return Err(Refusal::NoRoom {
                largest_free: block.largest_free(),
                free: block.free(),
            });
        }
        if purged_any {
            self.stats.grants_after_purge += 1;
        }
        if self
            .reservation
            .is_some_and(|reservation| reservation.kind == ask.kind)
        {
            self.reservation = None;
        }
        let id = LoanId(self.next_id);
        self.next_id = self.next_id.wrapping_add(1);
        self.held = Some(Held {
            id,
            kind: ask.kind,
            tick: self.tick,
            leak_counted: false,
        });
        block.open(id, ask.kind);
        Ok(Loan {
            id,
            kind: ask.kind,
            tick: self.tick,
        })
    }

    fn fits(&self, ask: Ask, block: &dyn Block) -> bool {
        block.largest_free() >= ask.largest
            && block.free().saturating_add(self.overflow_allowance) >= ask.total
    }

    fn reserve(&mut self, kind: LoanKind) {
        let keep = match self.reservation {
            Some(reservation) => !kind.outranks(reservation.kind),
            None => false,
        };
        if !keep {
            self.reservation = Some(Reservation {
                kind,
                until_tick: self.tick.saturating_add(self.reservation_ticks),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use core::cell::Cell;

    use super::*;

    #[test]
    fn a_read_that_fits_is_lent_and_returned() {
        let tenants = Cell::new(0);
        let mut block = FakeBlock::new(32_768, &tenants);
        let mut lender = Lender::new(32_768, 0);
        let loan = lender
            .try_lend(Ask::new(LoanKind::Read, 4_096, 9_000), &mut block, &mut [])
            .expect("granted");
        assert_eq!(block.open, Some(loan.id()));
        assert_eq!(lender.holder(), Some(LoanKind::Read));
        block.used_by_holder = 9_000;
        let close = lender.release(loan, &mut block);
        assert_eq!(close.peak_in_block, 9_000);
        assert_eq!(lender.holder(), None);
        assert_eq!(lender.stats().kind(LoanKind::Read).granted, 1);
    }

    #[test]
    fn a_job_longer_than_the_block_is_refused_in_words() {
        let tenants = Cell::new(0);
        let mut block = FakeBlock::new(32_768, &tenants);
        let mut lender = Lender::new(32_768, 8_192);
        let refusal = lender
            .try_lend(
                Ask::new(LoanKind::WholeFile, 40_000, 40_000),
                &mut block,
                &mut [],
            )
            .unwrap_err();
        assert!(matches!(refusal, Refusal::TooBig { .. }));
        assert!(!refusal.is_transient());
        // A working set over block + allowance is too big as well.
        let refusal = lender
            .try_lend(Ask::new(LoanKind::Read, 10_240, 50_168), &mut block, &mut [])
            .unwrap_err();
        assert!(matches!(refusal, Refusal::TooBig { .. }));
        // Within block + allowance it is lent: the rest overflows.
        assert!(
            lender
                .try_lend(Ask::new(LoanKind::Read, 10_240, 39_000), &mut block, &mut [])
                .is_ok()
        );
    }

    #[test]
    fn one_holder_at_a_time() {
        let tenants = Cell::new(0);
        let mut block = FakeBlock::new(32_768, &tenants);
        let mut lender = Lender::new(32_768, 0);
        let loan = lender
            .try_lend(Ask::new(LoanKind::Read, 1_024, 4_096), &mut block, &mut [])
            .unwrap();
        let refusal = lender
            .try_lend(Ask::new(LoanKind::Read, 1_024, 4_096), &mut block, &mut [])
            .unwrap_err();
        assert_eq!(
            refusal,
            Refusal::Busy {
                holder: LoanKind::Read
            }
        );
        lender.release(loan, &mut block);
        assert!(
            lender
                .try_lend(Ask::new(LoanKind::Read, 1_024, 4_096), &mut block, &mut [])
                .is_ok()
        );
    }

    #[test]
    fn a_refused_higher_priority_taker_gets_the_next_grant() {
        let tenants = Cell::new(0);
        let mut block = FakeBlock::new(32_768, &tenants);
        let mut lender = Lender::new(32_768, 0);
        lender.begin_tick(10);
        let read = lender
            .try_lend(Ask::new(LoanKind::Read, 1_024, 4_096), &mut block, &mut [])
            .unwrap();
        let compile = Ask::whole(LoanKind::Compile, 32_768);
        assert!(matches!(
            lender.try_lend(compile, &mut block, &mut []),
            Err(Refusal::Busy { .. })
        ));
        lender.release(read, &mut block);
        // A read asking before the compile comes back is kept waiting.
        assert_eq!(
            lender.try_lend(Ask::new(LoanKind::Read, 1_024, 4_096), &mut block, &mut []),
            Err(Refusal::Reserved {
                for_kind: LoanKind::Compile
            })
        );
        // The compile gets it, and the reservation is spent.
        let loan = lender.try_lend(compile, &mut block, &mut []).unwrap();
        lender.release(loan, &mut block);
        assert_eq!(lender.reservation(), None);
        assert!(
            lender
                .try_lend(Ask::new(LoanKind::Read, 1_024, 4_096), &mut block, &mut [])
                .is_ok()
        );
    }

    #[test]
    fn an_unclaimed_reservation_expires() {
        let tenants = Cell::new(0);
        let mut block = FakeBlock::new(32_768, &tenants);
        let mut lender = Lender::new(32_768, 0).with_reservation_ticks(2);
        lender.begin_tick(1);
        let read = lender
            .try_lend(Ask::new(LoanKind::Read, 1_024, 4_096), &mut block, &mut [])
            .unwrap();
        let _ = lender.try_lend(Ask::whole(LoanKind::Compile, 32_768), &mut block, &mut []);
        lender.release(read, &mut block);
        lender.begin_tick(3);
        assert!(lender.reservation().is_some());
        lender.begin_tick(4);
        assert_eq!(lender.reservation(), None);
        assert_eq!(lender.stats().reservations_expired, 1);
    }

    #[test]
    fn a_grant_purges_only_the_tenants_it_needs() {
        let tenants = Cell::new(0);
        let mut cheap = FakeTenant::new(8_192, &tenants);
        let mut dear = FakeTenant::new(8_192, &tenants);
        let mut block = FakeBlock::new(32_768, &tenants);
        let mut lender = Lender::new(32_768, 0);
        // 16 KiB free beside the tenants: a 20 KiB ask purges one of them.
        let loan = lender
            .try_lend(
                Ask::new(LoanKind::Read, 8_192, 20_000),
                &mut block,
                &mut [&mut cheap, &mut dear],
            )
            .unwrap();
        assert_eq!(cheap.resident(), 0, "the cheap tenant goes first");
        assert_eq!(dear.resident(), 8_192, "the dear one is kept");
        assert_eq!(lender.stats().purges, 1);
        assert_eq!(lender.stats().purged_bytes, 8_192);
        assert_eq!(lender.stats().grants_after_purge, 1);
        lender.release(loan, &mut block);
    }

    #[test]
    fn a_grant_that_fits_beside_the_tenants_purges_nothing() {
        let tenants = Cell::new(0);
        let mut tenant = FakeTenant::new(8_192, &tenants);
        let mut block = FakeBlock::new(32_768, &tenants);
        let mut lender = Lender::new(32_768, 0);
        let loan = lender
            .try_lend(
                Ask::new(LoanKind::Read, 4_096, 9_000),
                &mut block,
                &mut [&mut tenant],
            )
            .unwrap();
        assert_eq!(tenant.resident(), 8_192);
        assert_eq!(lender.stats().purges, 0);
        lender.release(loan, &mut block);
    }

    #[test]
    fn a_pinned_tenant_is_never_purged() {
        let tenants = Cell::new(0);
        let mut pinned = FakeTenant::new(24_576, &tenants);
        pinned.pinned = true;
        let mut block = FakeBlock::new(32_768, &tenants);
        let mut lender = Lender::new(32_768, 0);
        let refusal = lender
            .try_lend(
                Ask::whole(LoanKind::Compile, 32_768),
                &mut block,
                &mut [&mut pinned],
            )
            .unwrap_err();
        assert!(matches!(refusal, Refusal::NoRoom { .. }));
        assert!(refusal.is_transient());
        assert_eq!(lender.stats().purges, 0);
    }

    #[test]
    fn a_rebuild_never_purges_another_tenant() {
        let tenants = Cell::new(0);
        let mut other = FakeTenant::new(24_576, &tenants);
        let mut block = FakeBlock::new(32_768, &tenants);
        let mut lender = Lender::new(32_768, 0);
        let refusal = lender
            .try_lend(
                Ask::new(LoanKind::Rebuild, 12_288, 12_288),
                &mut block,
                &mut [&mut other],
            )
            .unwrap_err();
        assert!(matches!(refusal, Refusal::NoRoom { .. }));
        assert_eq!(other.resident(), 24_576);
    }

    #[test]
    fn survivors_and_overflow_are_counted_per_kind() {
        let tenants = Cell::new(0);
        let mut block = FakeBlock::new(32_768, &tenants);
        let mut lender = Lender::new(32_768, 16_384);
        let loan = lender
            .try_lend(Ask::whole(LoanKind::Compile, 32_768), &mut block, &mut [])
            .unwrap();
        block.next_close = BlockClose {
            peak_in_block: 30_000,
            overflow: 2_000,
            survivors: 4_096,
            largest_free_after: 20_000,
        };
        lender.release(loan, &mut block);
        let compile = lender.stats().kind(LoanKind::Compile);
        assert_eq!(compile.peak_in_block, 30_000);
        assert_eq!(compile.overflow, 2_000);
        assert_eq!(compile.overflowed_loans, 1);
        assert_eq!(compile.survivors, 4_096);
    }

    #[test]
    fn a_loan_held_past_its_tick_is_a_leak_unless_it_is_an_update() {
        let tenants = Cell::new(0);
        let mut block = FakeBlock::new(40_000, &tenants);
        let mut lender = Lender::new(40_000, 0);
        lender.begin_tick(1);
        let read = lender
            .try_lend(Ask::new(LoanKind::Read, 1_024, 4_096), &mut block, &mut [])
            .unwrap();
        lender.begin_tick(2);
        lender.begin_tick(3);
        assert_eq!(lender.stats().leaked, 1, "counted once");
        lender.release(read, &mut block);
        let ota = lender
            .try_lend(Ask::new(LoanKind::Ota, 36_864, 36_864), &mut block, &mut [])
            .unwrap();
        lender.begin_tick(4);
        assert_eq!(lender.stats().leaked, 1);
        lender.release(ota, &mut block);
    }

    /// A block with no fragmentation: free is its capacity less what the
    /// holder and the tenants hold, and the largest run is all of it.
    struct FakeBlock<'a> {
        capacity: u32,
        tenants: &'a Cell<u32>,
        used_by_holder: u32,
        open: Option<LoanId>,
        next_close: BlockClose,
    }

    impl<'a> FakeBlock<'a> {
        fn new(capacity: u32, tenants: &'a Cell<u32>) -> Self {
            Self {
                capacity,
                tenants,
                used_by_holder: 0,
                open: None,
                next_close: BlockClose::default(),
            }
        }
    }

    impl Block for FakeBlock<'_> {
        fn capacity(&self) -> u32 {
            self.capacity
        }
        fn free(&self) -> u32 {
            self.capacity - self.tenants.get() - self.used_by_holder
        }
        fn largest_free(&self) -> u32 {
            self.free()
        }
        fn open(&mut self, loan: LoanId, _kind: LoanKind) {
            self.open = Some(loan);
        }
        fn close(&mut self, _loan: LoanId) -> BlockClose {
            self.open = None;
            let mut close = core::mem::take(&mut self.next_close);
            close.peak_in_block = close.peak_in_block.max(self.used_by_holder);
            self.used_by_holder = 0;
            close
        }
    }

    /// A tenant whose bytes count against the block it shares a counter with.
    struct FakeTenant<'a> {
        bytes: u32,
        pinned: bool,
        block: &'a Cell<u32>,
    }

    impl<'a> FakeTenant<'a> {
        fn new(bytes: u32, block: &'a Cell<u32>) -> Self {
            block.set(block.get() + bytes);
            Self {
                bytes,
                pinned: false,
                block,
            }
        }
    }

    impl Tenant for FakeTenant<'_> {
        fn resident(&self) -> u32 {
            self.bytes
        }
        fn pinned(&self) -> bool {
            self.pinned
        }
        fn purge(&mut self) -> u32 {
            let freed = core::mem::take(&mut self.bytes);
            self.block.set(self.block.get() - freed);
            freed
        }
    }
}
