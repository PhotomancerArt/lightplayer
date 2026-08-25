//! I/O task for the classic ESP32's UART0 host link.
//!
//! Responsibilities are the S3's:
//! - Drain the outgoing log/message queue and write it (prefixed lines).
//! - Drain accountable server write requests, serialize to JSON, write, and
//!   report the outcome back to the transport.
//! - Read available bytes, split on newlines, push `M!` lines to the incoming
//!   queue.
//!
//! ## Two divergences from `fw-esp32s3/src/serial/io_task.rs`
//!
//! **1. No connection monitor.** The S3 polls the USB-Serial-JTAG SOF bit to
//! tell "cable plugged" from "host application draining", because on that chip
//! a host that stops reading makes every write time out, which stalls the task
//! and starves the watchdog. A UART has neither signal and neither problem:
//! the CH340K clocks bytes onto the wire at line rate whether or not anything
//! is listening, so a write always completes and there is nothing to latch.
//! `UsbConnectionMonitor` and the probe-write self-healing path have no
//! counterpart here; the write timeout below survives only as a backstop
//! against a wedged peripheral.
//!
//! **2. RX is drained *between* TX chunks.** On USB-Serial-JTAG a 16 KiB
//! `ProjectRead` frame leaves in a few milliseconds; at UART line rate it
//! takes real wall time, and UART0's RX FIFO is 128 bytes — ~1.4 ms of line
//! time at 921600 baud. Draining RX only at the top of the loop would
//! overflow the FIFO and silently lose whatever the host sent during a long
//! write. Hence every write goes through a [`ChunkedWriter`] whose `on_chunk`
//! hook drains RX ([`poll_rx_into`]) and whose chunk size
//! ([`WritePolicy::UART_921600`]) is sized in *line time*, not syscall
//! overhead — exactly the seam `chunked_write`'s docs promise this crate.
//!
//! The JSON-serialization half (stack buffer, `\nM!` framing) is the shared
//! `fw_esp32_common::serial::server_msg` implementation, the same one
//! fw-esp32c6 and fw-esp32s3 use.

extern crate alloc;

use alloc::{string::String, vec::Vec};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Instant};
use esp_hal::interrupt::{InterruptHandler, Priority};
use esp_hal::timer::{AnyTimer, PeriodicTimer};
use esp_hal::uart::{Uart, UartRx, UartTx};
use esp_hal::{Async, Blocking};
use fw_core::message_router::MessageRouter;
use fw_esp32_common::serial::chunked_write::{ChunkedWriter, WritePolicy};

/// Static message channels for MessageRouter
static INCOMING_MSG: Channel<CriticalSectionRawMutex, String, 32> = Channel::new();
static OUTGOING_MSG: Channel<CriticalSectionRawMutex, String, 32> = Channel::new();

/// Accountable server write requests.
///
/// The server transport submits one message here, then waits on
/// `SERVER_WRITE_RESULT`. This keeps `ServerTransport::send().await` aligned
/// with actual write completion instead of a best-effort task handoff.
///
/// Each request carries a wrapping `u32` generation token that `io_task` echoes
/// back on the result channel, so `transport.send()` can discard a result
/// orphaned by a cancelled send instead of trusting arrival order.
static SERVER_WRITE_REQUEST: Channel<
    CriticalSectionRawMutex,
    (u32, lpc_wire::WireServerMessage),
    1,
> = Channel::new();

static SERVER_WRITE_RESULT: Channel<
    CriticalSectionRawMutex,
    (u32, Result<(), lpc_wire::TransportError>),
    1,
> = Channel::new();

/// RX drain buffer. One FIFO's worth, so a single `read_buffered` empties a
/// full FIFO.
const READ_CHUNK_SIZE: usize = 128;

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
/// is already claimed by the swi1 executor, and a P1 tick still wakes the P2
/// executor (the wake path is signal → pender → swi1 raise, and a pending
/// swi fires as soon as the level allows).
pub fn start_io_pacer(timer: AnyTimer<'static>) {
    // SAFETY: the one and only mutable reference taken outside the ISR, and
    // the ISR cannot run yet — `listen` below is what enables it.
    let slot = unsafe { &mut *core::ptr::addr_of_mut!(IO_PACER) };
    let pacer = slot.insert(PeriodicTimer::new(timer));
    pacer.set_interrupt_handler(InterruptHandler::new(io_pacer_isr, Priority::Priority1));
    if let Err(error) = pacer.start(IO_TICK_PERIOD) {
        // A dead pacer means a mute io_task; say so loudly while esp_println
        // still reaches the wire directly.
        esp_println::println!("[ERROR] io pacer failed to start ({error:?}); host link will be mute");
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

/// How long a non-empty partial line may sit with no further RX bytes before
/// it is declared a dead session's remnant and discarded. Real frames stream
/// at line rate (a full 16 KiB frame takes ~175 ms at 921600), so a one-second
/// intra-frame silence is not a live client — it is a host that died or
/// disconnected mid-frame, whose half-frame would otherwise prefix-corrupt
/// the next session's first line (the hello, typically; observed on the
/// 2026-08-21 dig2go bench, see
/// `docs/defects/2026-08-21-hello-gate-assumes-fresh-boot.md`).
const STALE_PARTIAL_TIMEOUT: Duration = Duration::from_secs(1);

/// The split UART plus the partial-line buffer, held together because writing
/// and reading interleave (see the module docs).
struct UartLink {
    rx: UartRx<'static, Async>,
    tx: UartTx<'static, Async>,
    read_buffer: Vec<u8>,
    /// When RX bytes last arrived, for [`Self::flush_stale_partial`].
    last_rx: Instant,
}

impl UartLink {
    fn new(uart: Uart<'static, Async>) -> Self {
        let (rx, tx) = uart.split();
        Self {
            rx,
            tx,
            read_buffer: Vec::new(),
            last_rx: Instant::now(),
        }
    }

    /// [`poll_rx_into`] over this link's RX half and line buffer.
    fn poll_rx(&mut self, router: &MessageRouter) {
        poll_rx_into(
            &mut self.rx,
            &mut self.read_buffer,
            &mut self.last_rx,
            router,
        );
    }

    /// Discard a partial line that stopped growing: a dead session's remnant.
    ///
    /// Without this, a host that vanished mid-frame leaves its half-line in
    /// `read_buffer`, and the next session's first frame is glued onto it —
    /// one corrupt line whose parse failure eats the new session's hello.
    /// See [`STALE_PARTIAL_TIMEOUT`] for the threshold rationale.
    fn flush_stale_partial(&mut self) {
        if !self.read_buffer.is_empty() && self.last_rx.elapsed() >= STALE_PARTIAL_TIMEOUT {
            log::warn!(
                "[io_task] discarding {} B partial line (no RX for {} ms) — dead session remnant",
                self.read_buffer.len(),
                self.last_rx.elapsed().as_millis()
            );
            self.read_buffer.clear();
        }
    }

    /// A [`ChunkedWriter`] over this link's TX half whose per-chunk hook
    /// drains RX — the "drain RX at least twice per FIFO fill time" invariant
    /// the UART write policy sizes its chunks for.
    ///
    /// Borrows the link field-by-field: the hook needs the RX half and the
    /// line buffer while the writer holds TX, which `&mut self` methods can't
    /// express.
    fn writer<'a>(
        &'a mut self,
        router: &'a MessageRouter,
    ) -> ChunkedWriter<'a, UartTx<'static, Async>, impl FnMut() + 'a, TickDelay> {
        let Self {
            rx,
            tx,
            read_buffer,
            last_rx,
        } = self;
        ChunkedWriter::new(
            tx,
            WritePolicy::UART_921600,
            || poll_rx_into(rx, read_buffer, last_rx, router),
            TickDelay,
        )
    }

    /// Write everything in `data`, draining RX between chunks.
    ///
    /// Returns false on a per-chunk timeout or error, with the failure's
    /// chunk/elapsed detail logged: a wedged peripheral crawls through every
    /// chunk (elapsed ≈ chunk × timeout), a starved task sails through its
    /// chunks and then stalls once (elapsed ≈ one timeout) — see
    /// `docs/debt/shared-uart-io-task-starvation.md`.
    async fn write_chunked(&mut self, data: &[u8], router: &MessageRouter) -> bool {
        match self.writer(router).try_write_all(data).await {
            Ok(()) => true,
            Err(failure) => {
                log::warn!("[io_task] UART TX write {failure}");
                false
            }
        }
    }
}

/// Move whatever the RX FIFO already holds into the line buffer and push
/// any complete `M!` lines to the incoming queue.
///
/// Non-blocking by construction: `read_buffered` returns what is there and
/// never waits, which is the read semantic the C6 and S3 adapters also
/// present. It is deliberately not the async `read()` — that one is
/// cancellation-safe but parks on an RX-timeout interrupt that the classic
/// ESP32 cannot clear while the FIFO is non-empty (esp-hal notes the
/// erratum in `read_exact_async`), and polling sidesteps the question
/// entirely at 1 ms granularity.
///
/// A free function over the link's parts rather than a method so the
/// [`ChunkedWriter`] `on_chunk` hook can borrow the RX half and line buffer
/// while the writer holds TX.
fn poll_rx_into(
    rx: &mut UartRx<'static, Async>,
    read_buffer: &mut Vec<u8>,
    last_rx: &mut Instant,
    router: &MessageRouter,
) {
    let mut temp = [0u8; READ_CHUNK_SIZE];
    loop {
        match rx.read_buffered(&mut temp) {
            Ok(0) => break,
            Ok(n) => {
                *last_rx = Instant::now();
                read_buffer.extend_from_slice(&temp[..n]);
                // A short read means the FIFO is empty; anything else and
                // there may be more waiting.
                if n < temp.len() {
                    break;
                }
            }
            Err(error) => {
                // Overflow/parity/framing. The FIFO is reset by esp-hal on
                // overflow; drop the partial line rather than splice two
                // halves of different messages together.
                log::warn!("[io_task] UART RX error: {error:?}; dropping partial line");
                read_buffer.clear();
                break;
            }
        }
    }
    process_read_buffer(read_buffer, router);
}

/// I/O task for the UART0 host link.
///
/// Runs independently of the server loop and converts between UART bytes and
/// `M!`-prefixed JSON lines.
///
/// `main.rs` spawns this on an `esp_rtos` interrupt executor (swi1,
/// Priority2), NOT the thread executor the server loop runs on: the 1 ms
/// poll cadence below must hold while a ~41 ms engine tick monopolizes the
/// thread executor, or the 128 B RX FIFO (~1.4 ms at 921600) overflows and
/// TX chunks time out unpolled (`docs/debt/shared-uart-io-task-starvation.md`).
/// Two consequences to preserve: the future must stay `Send` (it is spawned
/// through a `SendSpawner`), and everything it shares with the rest of the
/// firmware must remain these static channels — cross-executor communication
/// is the design, not an accident.
///
/// # Arguments
///
/// * `uart` - UART0, already configured at 921600 8N1 by `init_board` (which
///   is also where the baud divisor `esp-println` piggybacks on gets set).
#[embassy_executor::task]
pub async fn io_task(uart: Uart<'static, Blocking>) {
    // Direct-FIFO breadcrumbs (NOT the log router, which this task itself
    // services): the first proves the interrupt executor polled the task at
    // all, the second proves pacer-tick wakes reach it. If the second never
    // prints, the pacer is dead and the host link will be mute — see
    // `start_io_pacer` and the `IO_TICK` docs.
    esp_println::println!("[INIT] io_task polled (interrupt executor entry)");
    let router = MessageRouter::new(&INCOMING_MSG, &OUTGOING_MSG);
    let mut link = UartLink::new(uart.into_async());

    // The old 100 ms boot settle, in ticks.
    wait_ticks(100).await;
    esp_println::println!("[INIT] io_task first pacer ticks (io alive)");

    loop {
        drain_server_write_request(&mut link, &router).await;
        drain_outgoing_messages(&router, &mut link).await;
        link.poll_rx(&router);
        link.flush_stale_partial();

        // Pace on the hardware tick, never on embassy-time (see `IO_TICK`).
        IO_TICK.wait().await;
    }
}

/// Drain the outgoing log/message queue. Always consumes, so the server loop
/// never blocks on a full channel.
async fn drain_outgoing_messages(router: &MessageRouter, link: &mut UartLink) {
    let receiver = router.outgoing().receiver();
    while let Ok(msg) = receiver.try_receive() {
        if !link.write_chunked(b"\n", router).await {
            break;
        }
        if !link.write_chunked(msg.as_bytes(), router).await {
            break;
        }
    }
}

/// Drain accountable server write requests.
async fn drain_server_write_request(link: &mut UartLink, router: &MessageRouter) {
    let receiver = SERVER_WRITE_REQUEST.receiver();
    let Ok((generation, msg)) = receiver.try_receive() else {
        return;
    };

    let result = timed_write_server_msg(link, msg, router).await;
    // Echo the request generation so `transport.send()` can discard any stale
    // result left over from a cancelled send.
    SERVER_WRITE_RESULT
        .sender()
        .send((generation, result))
        .await;
}

/// Serialize and write one server message through the shared
/// `fw_esp32_common::serial::server_msg` path — the same stack-buffer
/// serialization and `\nM!` framing (and failure `\n` resync) the C6 and S3
/// io tasks use. `connected` is unconditionally `true`: a UART has no
/// cable/drain signal (see the module docs), so there is no verdict to pass.
async fn timed_write_server_msg(
    link: &mut UartLink,
    msg: lpc_wire::WireServerMessage,
    router: &MessageRouter,
) -> Result<(), lpc_wire::TransportError> {
    link.writer(router).write_server_msg(msg, true).await
}

/// Process the read buffer and extract complete lines.
///
/// Looks for newlines, extracts lines starting with `M!`, and pushes them to
/// the incoming queue. Non-`M!` lines are ignored (they are the device's own
/// `esp_println!` output, which shares this UART).
fn process_read_buffer(read_buffer: &mut Vec<u8>, router: &MessageRouter) {
    while let Some(newline_pos) = read_buffer.iter().position(|&b| b == b'\n') {
        let line_bytes: Vec<u8> = read_buffer.drain(..=newline_pos).collect();

        if let Ok(line_str) = core::str::from_utf8(&line_bytes[..line_bytes.len() - 1])
            && line_str.starts_with("M!")
        {
            // An inbound hello is the firmware's only "new client attached"
            // signal (a UART has no cable event), so it doubles as the
            // session boundary: whatever is still queued outbound was
            // addressed to a session that no longer exists.
            if line_str.contains("\"msg\":\"hello\"") {
                drain_stale_outgoing(router);
            }
            use alloc::string::ToString;
            if router
                .incoming()
                .sender()
                .try_send(line_str.to_string())
                .is_err()
            {
                log::warn!("[io_task] incoming queue full, dropping M! message");
            }
        }
    }
}

/// Drop every queued fire-and-forget outbound line at a session boundary.
///
/// These are log/telemetry lines a previous session never drained (they only
/// accumulate when io_task itself was blocked, e.g. across a flash window) —
/// flushing them keeps a new session's first exchange from being preceded by
/// a dead session's backlog. Accountable server writes
/// (`SERVER_WRITE_REQUEST`) are deliberately NOT touched: dropping one
/// without posting its result would leave the server task awaiting forever.
///
/// The `"msg":"hello"` substring test is sound because both wire clients
/// (lp-app and serial-lab) emit compact JSON, and inside a JSON string value
/// those quotes would be escaped — the raw byte sequence only occurs as the
/// envelope's own field.
fn drain_stale_outgoing(router: &MessageRouter) {
    let receiver = router.outgoing().receiver();
    let mut dropped = 0usize;
    while receiver.try_receive().is_ok() {
        dropped += 1;
    }
    if dropped > 0 {
        log::info!(
            "[io_task] hello: dropped {dropped} outbound lines queued for a previous session"
        );
    }
}

/// Get references to the static message channels.
///
/// Used by main.rs to create the `StreamingMessageRouterTransport`.
pub fn get_message_channels() -> (
    &'static Channel<CriticalSectionRawMutex, String, 32>,
    &'static Channel<CriticalSectionRawMutex, String, 32>,
) {
    (&INCOMING_MSG, &OUTGOING_MSG)
}

/// Get the accountable server write channels for StreamingMessageRouterTransport.
pub fn get_server_write_channels() -> (
    &'static Channel<CriticalSectionRawMutex, (u32, lpc_wire::WireServerMessage), 1>,
    &'static Channel<CriticalSectionRawMutex, (u32, Result<(), lpc_wire::TransportError>), 1>,
) {
    (&SERVER_WRITE_REQUEST, &SERVER_WRITE_RESULT)
}

/// Write log output to the outgoing channel (serial to host).
///
/// Lines go out without the `M!` prefix so the client prints them. When the
/// channel is full, log lines are dropped — logging that would recurse.
pub fn log_write_to_outgoing(msg: &str) {
    use alloc::string::ToString;
    let _ = OUTGOING_MSG.sender().try_send(msg.to_string());
}
