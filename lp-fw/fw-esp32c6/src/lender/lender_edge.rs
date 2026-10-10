//! The one lender, and the hooks that make the server's reads and the
//! engine's compiles its takers.
//!
//! - Project reads and whole-file reads: `lpa_server::BigBlockHook`, on the
//!   server's thread; the read's ask is E7's per-request estimate.
//! - Shader compiles: lp-perf's `shader-compile` markers. The begin marker
//!   asks for the whole block; the end marker returns it. A marker cannot
//!   defer a compile, so a refused compile runs unlent and is counted.
//! - The safe point: lp-perf's `frame` begin marker is the tick
//!   ([`lp_lender::Lender::begin_tick`]).
//! - The tenant: after each project read, the card stand-in is checked out
//!   and, if purged, rebuilt into the block under a `Rebuild` loan.

use core::cell::RefCell;

use critical_section::Mutex;
use lp_lender::{Ask, BlockClose, Lender, Loan, LoanKind, Refusal};

use super::c6_block::C6Block;
use super::card_tenant::CardTenant;
use super::{LEND_BYTES, borrower, lend_region};

/// How much of a job's working set may overflow to the general heap.
const OVERFLOW_ALLOWANCE: u32 = 8 * 1024;
/// A compile's ask, until compiles estimate their own: E7's largest single
/// compile ask in the catalog (fire2012, 15,924 B) and the choker's compile
/// working set (20.6–29.5 KB above its start). Not the whole block: once
/// anything lives in it (a read's 172 B kept, a spilled allocation), the
/// whole block is never free again, and the first run asked for it and was
/// refused every time.
const COMPILE_LARGEST: u32 = 16 * 1024;
const COMPILE_TOTAL: u32 = 30 * 1024;
/// The read after which the link stand-in allocates.
#[cfg(feature = "e11_link_standin")]
const LINK_AT_READ: u32 = 30;

struct Edge {
    lender: Lender,
    block: C6Block,
    tenant: CardTenant,
    loan: Option<Loan>,
    frame: u64,
    reads: u32,
    compiles_unlent: u32,
    compile_lent: bool,
    seq: [u32; LoanKind::COUNT],
}

static EDGE: Mutex<RefCell<Option<Edge>>> = Mutex::new(RefCell::new(None));

/// Make reads, whole-file reads and compiles borrow the block. Call once,
/// with the server built, before its loop starts.
pub fn install(server: &mut lpa_server::LpServer) {
    critical_section::with(|cs| {
        EDGE.borrow_ref_mut(cs).replace(Edge {
            lender: Lender::new(LEND_BYTES as u32, OVERFLOW_ALLOWANCE),
            block: C6Block::default(),
            tenant: CardTenant::default(),
            loan: None,
            frame: 0,
            reads: 0,
            compiles_unlent: 0,
            compile_lent: false,
            seq: [0; LoanKind::COUNT],
        });
    });
    server.set_big_block(Some(lpa_server::big_block::BigBlockHook { lend, release }));
    lp_perf::set_hook(on_marker);
    let (start, size) = lend_region::region();
    log::info!("[e11] lender installed: block {size} B at {start:#x}, overflow allowance {OVERFLOW_ALLOWANCE} B");
}

/// One loan's line, logged outside the critical section.
struct LoanLine {
    kind: LoanKind,
    seq: u32,
    ask: Ask,
    outcome: Result<(), Refusal>,
    purges_before: u32,
    purges_after: u32,
    block_used: u32,
}

fn lend(ask: Ask) -> Result<(), Refusal> {
    let line = critical_section::with(|cs| {
        let mut edge = EDGE.borrow_ref_mut(cs);
        let edge = edge.as_mut().expect("e11: lender installed");
        lend_in(edge, ask)
    });
    log_lend(&line);
    line.outcome
}

fn lend_in(edge: &mut Edge, ask: Ask) -> LoanLine {
    let seq = edge.seq[ask.kind.index()];
    edge.seq[ask.kind.index()] += 1;
    let purges_before = edge.lender.stats().purges;
    let block_used = edge.block.used();
    let outcome = if edge.loan.is_some() {
        // A nested ask (a compile inside a read's render probe): the
        // lender says busy; the nested job runs unlent.
        let holder = edge.lender.holder().unwrap_or(LoanKind::Read);
        Err(Refusal::Busy { holder })
    } else {
        let Edge {
            lender,
            block,
            tenant,
            ..
        } = edge;
        let mut tenants: [&mut dyn lp_lender::Tenant; 1] = [tenant];
        match lender.try_lend(ask, block, &mut tenants) {
            Ok(loan) => {
                edge.loan = Some(loan);
                Ok(())
            }
            Err(refusal) => Err(refusal),
        }
    };
    LoanLine {
        kind: ask.kind,
        seq,
        ask,
        outcome,
        purges_before,
        purges_after: edge.lender.stats().purges,
        block_used,
    }
}

fn log_lend(line: &LoanLine) {
    match &line.outcome {
        Ok(()) => log::info!(
            "[e11] lend {}#{} ask {}/{} B: granted{} (block used {} B before)",
            line.kind.name(),
            line.seq,
            line.ask.largest,
            line.ask.total,
            if line.purges_after > line.purges_before {
                " after purge"
            } else {
                ""
            },
            line.block_used
        ),
        Err(refusal) => log::warn!(
            "[e11] lend {}#{} ask {}/{} B: REFUSED {:?} (block used {} B)",
            line.kind.name(),
            line.seq,
            line.ask.largest,
            line.ask.total,
            refusal,
            line.block_used
        ),
    }
}

fn release() {
    let (kind, close, tenant_line) = critical_section::with(|cs| {
        let mut edge = EDGE.borrow_ref_mut(cs);
        let edge = edge.as_mut().expect("e11: lender installed");
        let loan = edge.loan.take().expect("e11: release without a loan");
        let kind = loan.kind();
        let close = edge.lender.release(loan, &mut edge.block);
        let mut tenant_line = None;
        if kind == LoanKind::Read {
            edge.reads += 1;
            #[cfg(feature = "e11_link_standin")]
            if edge.reads == LINK_AT_READ {
                super::link_standin::trigger();
            }
            tenant_line = Some(checkout_tenant(edge));
        }
        (kind, close, tenant_line)
    });
    log_release(kind, &close);
    if let Some(Some(line)) = tenant_line {
        log_lend(&line);
    }
}

/// Check the tenant out after a read; rebuild it into the block if it was
/// purged. Returns the rebuild's loan line, if one was asked for.
fn checkout_tenant(edge: &mut Edge) -> Option<LoanLine> {
    if cfg!(feature = "e11_no_tenant") || edge.tenant.checkout() {
        return None;
    }
    let ask = Ask::new(
        LoanKind::Rebuild,
        (super::card_tenant::TENANT_BYTES / 3) as u32 + 64,
        super::card_tenant::TENANT_BYTES as u32 + 256,
    );
    let line = lend_in(edge, ask);
    if line.outcome.is_ok() {
        edge.tenant.rebuild();
        if !edge.tenant.inside(lend_region::region()) {
            edge.tenant.rebuilds_outside += 1;
        }
        let loan = edge.loan.take().expect("e11: the rebuild's loan");
        edge.lender.release(loan, &mut edge.block);
    } else {
        edge.tenant.rebuilds_refused += 1;
    }
    Some(line)
}

fn log_release(kind: LoanKind, close: &BlockClose) {
    log::info!(
        "[e11] return {}: in-block peak {} B, overflow {} B, net left {} B, largest free after {} B",
        kind.name(),
        close.peak_in_block,
        close.overflow,
        close.survivors,
        close.largest_free_after
    );
}

fn on_marker(name: &'static str, kind: lp_perf::PerfEventKind) {
    match (name, kind) {
        (lp_perf::EVENT_FRAME, lp_perf::PerfEventKind::Begin) => {
            critical_section::with(|cs| {
                if let Some(edge) = EDGE.borrow_ref_mut(cs).as_mut() {
                    edge.frame += 1;
                    let frame = edge.frame;
                    edge.lender.begin_tick(frame);
                }
            });
        }
        (lp_perf::EVENT_SHADER_COMPILE, lp_perf::PerfEventKind::Begin) => {
            let ask = Ask::new(LoanKind::Compile, COMPILE_LARGEST, COMPILE_TOTAL);
            let lent = lend(ask).is_ok();
            critical_section::with(|cs| {
                if let Some(edge) = EDGE.borrow_ref_mut(cs).as_mut() {
                    edge.compile_lent = lent;
                    if !lent {
                        edge.compiles_unlent += 1;
                    }
                }
            });
        }
        (lp_perf::EVENT_SHADER_COMPILE, lp_perf::PerfEventKind::End) => {
            let lent = critical_section::with(|cs| {
                EDGE.borrow_ref_mut(cs)
                    .as_mut()
                    .is_some_and(|edge| core::mem::take(&mut edge.compile_lent))
            });
            if lent {
                release();
            }
        }
        _ => {}
    }
}

/// The heartbeat's `[e11]` lines: per-kind counters, the tenant, the block.
pub fn log_heartbeat() {
    let snapshot = critical_section::with(|cs| {
        EDGE.borrow_ref(cs).as_ref().map(|edge| {
            (
                *edge.lender.stats(),
                edge.frame,
                edge.compiles_unlent,
                (
                    edge.tenant.hits,
                    edge.tenant.misses,
                    edge.tenant.rebuilds,
                    edge.tenant.rebuilds_refused,
                    edge.tenant.rebuilds_outside,
                    edge.tenant.corrupt,
                    edge.tenant.purged,
                ),
            )
        })
    });
    let Some((stats, frame, compiles_unlent, tenant)) = snapshot else {
        return;
    };
    for kind in LoanKind::ALL {
        let k = stats.kind(kind);
        log::info!(
            "[e11] kind {}: granted {} busy {} reserved {} too-big {} no-room {} | peak-in-block {} overflow {} B in {} loans | net-left {} B",
            kind.name(),
            k.granted,
            k.busy,
            k.reserved,
            k.too_big,
            k.no_room,
            k.peak_in_block,
            k.overflow,
            k.overflowed_loans,
            k.survivors
        );
    }
    let lend = esp_alloc::HEAP.lend_stats();
    let (used, free) = esp_alloc::HEAP.lend_used_free();
    let largest = esp_alloc::HEAP.lend_largest_free();
    log::info!(
        "[e11] frame {frame} | purges {} ({} B) grants-after-purge {} leaked {} reservations-expired {} | compiles unlent {compiles_unlent} | thread-only opens {}",
        stats.purges,
        stats.purged_bytes,
        stats.grants_after_purge,
        stats.leaked,
        stats.reservations_expired,
        borrower::thread_only_opens()
    );
    log::info!(
        "[e11] tenant hits {} misses {} rebuilds {} refused {} outside {} corrupt {} purged {}",
        tenant.0,
        tenant.1,
        tenant.2,
        tenant.3,
        tenant.4,
        tenant.5,
        tenant.6
    );
    log::info!(
        "[e11] block used {used} free {free} largest {largest} | borrower allocs {} | overflow {} allocs {} B | spill {} allocs {} B",
        lend.borrower_count,
        lend.overflow_count,
        lend.overflow_bytes,
        lend.spill_count,
        lend.spill_bytes
    );
}
