//! I/O task for the classic ESP32's UART0 host link: a byte shuttle.
//!
//! Since wire proto 32 the host link is lp-link (plan
//! `classic-uart-on-lp-link`), and its [`Link`](lp_link::Link) runs on the
//! thread executor, in `fw_esp32_common::uart_link`'s link task. This task
//! owns UART0 and does only the part that must happen every millisecond
//! whatever the engine is doing (ruling DD20):
//!
//! - take what the RX FIFO holds and hand it to the link task
//!   (`uart_link_pipes::io_received`);
//! - write the frames the link task queued (`uart_link_pipes::io_take_tx`)
//!   through a [`ChunkedWriter`], **draining RX between chunks**.
//!
//! It never sees a frame, a message or a log record: the link task queues
//! whole frames, and the I/O task writes whatever bytes it is given in
//! order, so nothing else can land inside a frame. Serialization, parsing,
//! the link's timers and every log line happen thread-side; this task polls
//! in interrupt context on a borrowed stack, so its per-poll footprint must
//! stay at "chunked byte shuttling" scale (ADR
//! `2026-08-25-classic-uart-io-task-executor-isolation`, mechanism 3).
//!
//! ## Two divergences from the C6/S3's USB link task, both kept from the `M!` era
//!
//! **1. No connection monitor.** USB-Serial-JTAG's SOF bit tells "cable
//! plugged" from "host draining". A UART has neither signal nor the problem
//! it solves: the CH340K clocks bytes onto the wire at line rate whether or
//! not anything is listening, so a write always completes and there is
//! nothing to latch. The link's own session (`Up` / `Reset` / stalled) is the
//! only liveness signal, as it is on USB since its cut-over; the write
//! timeout below survives only as a backstop against a wedged peripheral.
//!
//! **2. RX is drained *between* TX chunks.** At UART line rate a window of
//! frames takes real wall time (~12 ms for 1 KB at 921600 baud), and UART0's
//! RX FIFO is 128 bytes — ~1.4 ms of line time. Draining RX only at the top
//! of the loop would overflow the FIFO while the host is talking. Hence every
//! write goes through a [`ChunkedWriter`] whose `on_chunk` hook drains RX
//! ([`poll_rx`]) and whose chunk size ([`WritePolicy::UART_921600`]) is sized
//! in *line time*, not syscall overhead. A byte the FIFO drops anyway (an
//! overflow while the task is held off: a flash write, a long critical
//! section) is no longer silent or fatal: its frame fails its CRC and the
//! link resends it, counted.
//!
//! What went with the `M!` lines: the line splitter, the "stale partial line"
//! flush (lp-link's deframer resyncs on the next `0x00` and abandons a quiet
//! partial frame itself, and a new host's SYN — not a guess about silence —
//! is what ends a dead session), the hello-drains-the-backlog rule (a session
//! reset does that), the accountable write request/result pair, and the
//! log queue (logs ride the link's log channel from the log ring).

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use esp_hal::interrupt::{InterruptHandler, Priority};
use esp_hal::timer::{AnyTimer, PeriodicTimer};
use esp_hal::uart::{Uart, UartRx, UartTx};
use esp_hal::{Async, Blocking};
use fw_esp32_common::serial::chunked_write::{ChunkedWriter, WritePolicy};
use fw_esp32_common::uart_link::{uart_link_counters, uart_link_pipes};

/// RX drain buffer. One FIFO's worth, so a single `read_buffered` empties a
/// full FIFO.
const READ_CHUNK_SIZE: usize = 128;

/// Bytes taken from the TX pipe per chunked write: about one frame. Held
/// across the write's awaits, so it lives in the task's future, not on the
/// interrupted stack.
const WRITE_CHUNK_SIZE: usize = 256;

/// The io pacer tick period. Sized like the old `Timer::after(1 ms)` loop
/// pacing was: the 128 B RX FIFO holds ~1.4 ms of line at 921600 baud, so a
/// 1 ms poll cadence drains it with margin.
const IO_TICK_PERIOD: esp_hal::time::Duration = esp_hal::time::Duration::from_millis(1);

/// ⚠️ io_task must NEVER await embassy-time (`Timer::after`, `Delay`).
///
/// It runs on an esp-rtos interrupt executor, and esp-rtos 0.3.0 never
/// delivers embassy-time wakes to tasks on interrupt executors: the task
/// parks at its first timed await forever, and processing its entry in the
/// shared timer queue while the engine runs has crashed the chip on the bench
/// (`InstrError`, PC in DRAM — 2026-08-25 dig2go). Every wake io_task relies
/// on must be a *direct* waker wake: channel/signal sends, and this pacer —
/// a hardware timer (TIMG0's second timer, which esp-rtos does not use)
/// whose ISR signals [`IO_TICK`] every millisecond. See
/// `docs/adr/2026-08-25-classic-uart-io-task-executor-isolation.md`.
static IO_TICK: Signal<CriticalSectionRawMutex, ()> = Signal::new();

/// The pacer timer, parked in a static so [`io_pacer_isr`] can clear its
/// interrupt. Written once by [`start_io_pacer`] before the interrupt is
/// enabled; only the ISR touches it afterwards.
static mut IO_PACER: Option<PeriodicTimer<'static, Blocking>> = None;

/// Start the 1 ms io pacer on `timer` (TIMG0's timer1 — see `start_runtime`).
///
/// Must be called before [`io_task`] is spawned would be ideal, but any order
/// works: ticks signalled before io_task waits simply collapse into one.
/// Priority1 is deliberate — the classic's single level-2 CPU interrupt slot
/// is already claimed by the swi2 executor, and a P1 tick still wakes the P2
/// executor (the wake path is signal → pender → swi2 raise, and a pending
/// swi fires as soon as the level allows).
pub fn start_io_pacer(timer: AnyTimer<'static>) {
    // SAFETY: the one and only mutable reference taken outside the ISR, and
    // the ISR cannot run yet — `listen` below is what enables it.
    let slot = unsafe { &mut *core::ptr::addr_of_mut!(IO_PACER) };
    let pacer = slot.insert(PeriodicTimer::new(timer));
    pacer.set_interrupt_handler(InterruptHandler::new(io_pacer_isr, Priority::Priority1));
    if let Err(error) = pacer.start(IO_TICK_PERIOD) {
        // A dead pacer means a mute io_task; say so loudly while esp_println
        // still reaches the wire directly (boot, thread context).
        esp_println::println!(
            "[ERROR] io pacer failed to start ({error:?}); host link will be mute"
        );
        return;
    }
    pacer.listen();
}

extern "C" fn io_pacer_isr() {
    // SAFETY: sole accessor once the interrupt is live (see `start_io_pacer`).
    if let Some(pacer) = unsafe { (*core::ptr::addr_of_mut!(IO_PACER)).as_mut() } {
        pacer.clear_interrupt();
    }
    IO_TICK.signal(());
}

/// Wait for `n` pacer ticks (≈ `n` milliseconds).
///
/// Ticks that arrive while io_task is busy collapse (a `Signal` latches one),
/// so this is a lower bound in wall time — exactly what loop pacing and
/// timeouts want, and never a scheduling dependency on embassy-time.
async fn wait_ticks(n: u32) {
    for _ in 0..n {
        IO_TICK.wait().await;
    }
}

/// The io task's delay source for [`ChunkedWriter`]: pacer ticks, not
/// embassy-time (see [`IO_TICK`] for why that distinction is load-bearing).
struct TickDelay;

impl embedded_hal_async::delay::DelayNs for TickDelay {
    async fn delay_ns(&mut self, ns: u32) {
        // Round up to whole ticks; a delay is a minimum, and 1 ms resolution
        // is the pacer's grain.
        wait_ticks(ns.div_ceil(1_000_000).max(1)).await;
    }
}

/// The split UART, held together because writing and reading interleave
/// (see the module docs).
struct UartLink {
    rx: UartRx<'static, Async>,
    tx: UartTx<'static, Async>,
}

impl UartLink {
    fn new(uart: Uart<'static, Async>) -> Self {
        let (rx, tx) = uart.split();
        Self { rx, tx }
    }

    /// Write every byte the link task has queued, draining RX between
    /// chunks — the "drain RX at least twice per FIFO fill time" invariant
    /// the UART write policy sizes its chunks for.
    ///
    /// A write that fails or times out (a wedged peripheral) is counted and
    /// the rest of this batch waits for the next tick: the bytes that did
    /// not go out cost a frame the link resends, so there is nothing to retry
    /// here and no resync marker to send.
    async fn write_queued(&mut self, chunk: &mut [u8; WRITE_CHUNK_SIZE]) {
        let Self { rx, tx } = self;
        let mut writer =
            ChunkedWriter::new(tx, WritePolicy::UART_921600, || poll_rx(rx), TickDelay);
        loop {
            let n = uart_link_pipes::io_take_tx(chunk);
            if n == 0 {
                return;
            }
            if writer.try_write_link_bytes(&chunk[..n]).await.is_err() {
                uart_link_counters::note_write_failure();
                return;
            }
        }
    }
}

/// Move whatever the RX FIFO already holds to the link task.
///
/// Non-blocking by construction: `read_buffered` returns what is there and
/// never waits, which is the read semantic the C6 and S3 adapters also
/// present. It is deliberately not the async `read()` — that one is
/// cancellation-safe but parks on an RX-timeout interrupt that the classic
/// ESP32 cannot clear while the FIFO is non-empty (esp-hal notes the
/// erratum in `read_exact_async`), and polling sidesteps the question
/// entirely at 1 ms granularity.
///
/// A free function over the RX half rather than a method so the
/// [`ChunkedWriter`] `on_chunk` hook can borrow the RX half while the writer
/// holds TX.
fn poll_rx(rx: &mut UartRx<'static, Async>) {
    let mut temp = [0u8; READ_CHUNK_SIZE];
    loop {
        match rx.read_buffered(&mut temp) {
            Ok(0) => break,
            Ok(n) => {
                uart_link_pipes::io_received(&temp[..n]);
                // A short read means the FIFO is empty; anything else and
                // there may be more waiting.
                if n < temp.len() {
                    break;
                }
            }
            Err(_) => {
                // Overflow/parity/framing. esp-hal resets the FIFO on
                // overflow; the frame those bytes belonged to fails its CRC
                // on the host's side of the link and is resent. Counted here,
                // said by the link task (no logging from this executor).
                uart_link_counters::note_rx_error();
                break;
            }
        }
    }
}

/// A `Uart<Async>` that may cross the `SendSpawner` boundary.
///
/// `Uart<Async>` is `!Send` (driver state is core-local). Moving it into
/// io_task is sound HERE: `into_async()` runs on the PRO core in thread
/// context, and io_task's executor (swi2) is bound to that same core — the
/// driver never changes cores and the value moves exactly once.
///
/// Pre-converting is REQUIRED, not a convenience: `into_async()` registers
/// and enables the UART interrupt, and doing that from inside the executor's
/// handler's own poll does not survive the handler's exit (the interrupt
/// epilogue restores the interrupt-enable state it entered with). The bench
/// signature was async TX writes pending forever unless the bytes happened
/// to fit the idle FIFO synchronously — one lucky boot flowed, the rest sat
/// mute with io_task wedged inside its first `write_all` (2026-08-25).
pub struct SendUart(pub Uart<'static, Async>);
unsafe impl Send for SendUart {}

/// I/O task for the UART0 host link: RX FIFO → link task, link task → TX.
///
/// `main.rs` spawns this on an `esp_rtos` interrupt executor (swi2,
/// Priority2), NOT the thread executor the server loop and the link task run
/// on: the 1 ms poll cadence below must hold while a ~41 ms engine tick
/// monopolizes the thread executor, or the 128 B RX FIFO (~1.4 ms at 921600)
/// overflows (`docs/debt/shared-uart-io-task-starvation.md`). Two
/// consequences to preserve: the future must stay `Send` (it is spawned
/// through a `SendSpawner`), and everything it shares with the rest of the
/// firmware must remain statics safe from interrupt context — the byte pipes
/// and counters of `fw_esp32_common::uart_link`, never the `Link` itself.
///
/// # Arguments
///
/// * `uart` - UART0, already configured at 921600 8N1 by `init_board` (which
///   is also where the baud divisor `esp-println` piggybacks on gets set),
///   and already converted to async in thread context (see [`SendUart`]).
#[embassy_executor::task]
pub async fn io_task(uart: SendUart) {
    // ⚠️ No `esp_println!` and no `log::*` anywhere in this task — it polls
    // in interrupt context (the swi2 executor), and printing from there
    // corrupted the system on the bench: a deterministic `InstrError` (PC in
    // DRAM) fired seconds later in thread context whenever a diagnostic print
    // ran in this task's entry poll (2026-08-25 dig2go; esp-sync's locks are
    // priority-limited, per esp-rtos's own timer-priority comment, so an
    // interrupt-context print can re-enter a lock the thread believes it
    // holds). A log record also formats on this borrowed stack. Liveness
    // evidence goes through an atomic instead (`uart_link_pipes::io_pass`),
    // and the link task says it on the log channel.
    let mut link = UartLink::new(uart.0);
    let mut chunk = [0u8; WRITE_CHUNK_SIZE];

    // The old 100 ms boot settle, in ticks.
    wait_ticks(100).await;

    loop {
        uart_link_pipes::io_pass();
        poll_rx(&mut link.rx);
        link.write_queued(&mut chunk).await;

        // Pace on the hardware tick, never on embassy-time (see `IO_TICK`).
        IO_TICK.wait().await;
    }
}
