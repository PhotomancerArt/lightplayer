//! The `emu_*` exports — the wasm door itself.
//!
//! `wasm32` only. See [`super`] for the contract, the version rule and the
//! soundness argument behind the one static machine; this file is that
//! argument carried out, and holds nothing a host could get wrong by
//! reading it in another order.

use std::alloc::{Layout, alloc, dealloc};
use std::cell::UnsafeCell;
use std::collections::VecDeque;

use lp_emu_esp_common::QueueHandle;

use super::{ABI_VERSION, AbiError, Config, REPLY_MAX, code_for, outcome_code};
use crate::flash::FlashBacking;
use crate::machine::{AppSource, BootMode, Esp32C6Machine, StopCondition};

impl AbiError {
    /// Record `message` and answer with this code — the two halves of a
    /// refusal, so no caller can return one without the other.
    fn with(self, message: impl Into<String>) -> i32 {
        set_last_error(message.into());
        self.code()
    }
}

// ---- the one slot -------------------------------------------------------

/// A `static` cell for a single-threaded module. See the module docs for why
/// this is sound here and would not be anywhere else.
struct Slot<T>(UnsafeCell<T>);

// SAFETY: the wasip1 build has one thread. Nothing in this module is reached
// from anywhere but an exported function, and an exported function runs to
// completion before the host regains control.
unsafe impl<T> Sync for Slot<T> {}

impl<T> Slot<T> {
    const fn new(value: T) -> Self {
        Self(UnsafeCell::new(value))
    }

    /// The unique live reference to the slot's contents.
    ///
    /// # Safety
    ///
    /// The caller must not hold two of these at once. Every caller here is
    /// an exported function that takes one and drops it before returning,
    /// and no export calls another.
    #[allow(clippy::mut_from_ref)]
    unsafe fn get(&self) -> &mut T {
        unsafe { &mut *self.0.get() }
    }
}

/// Everything one emulated board is, on this side of the wall.
struct Host {
    machine: Esp32C6Machine,
    /// The live source's host end: host → guest bytes go in here.
    usb_in: QueueHandle,
    /// Guest → host bytes taken off the machine but not yet read by JS. The
    /// machine's log is drained whole; the host reads it in whatever chunks
    /// its buffer allows.
    usb_out: VecDeque<u8>,
    /// The same for the console.
    uart_out: VecDeque<u8>,
    /// The outcome that stopped this machine for good, if one did. A run
    /// that faulted answers the same code every time it is asked rather
    /// than stepping a hart that cannot continue.
    stopped: Option<i32>,
}

static HOST: Slot<Option<Host>> = Slot::new(None);
static LAST_ERROR: Slot<String> = Slot::new(String::new());

fn set_last_error(message: String) {
    // SAFETY: see `Slot::get` — one thread, no re-entrancy.
    *unsafe { LAST_ERROR.get() } = message;
}

/// The board, or `None` when nothing has been created.
///
/// # Safety
///
/// As [`Slot::get`]: one live reference at a time, which every caller here
/// satisfies by taking it and dropping it inside one export.
unsafe fn host() -> &'static mut Option<Host> {
    unsafe { HOST.get() }
}

// ---- buffers ------------------------------------------------------------

/// # Safety
///
/// `ptr`/`len` must name a range the host got from [`emu_alloc`] (or another
/// range it owns inside this module's memory) and must not alias anything
/// this call also writes.
unsafe fn bytes_in<'a>(ptr: i32, len: i32) -> Option<&'a [u8]> {
    if len < 0 || (len > 0 && ptr <= 0) {
        return None;
    }
    if len == 0 {
        return Some(&[]);
    }
    Some(unsafe { std::slice::from_raw_parts(ptr as *const u8, len as usize) })
}

/// # Safety
///
/// As [`bytes_in`], and the range must be writable.
unsafe fn bytes_out<'a>(ptr: i32, len: i32) -> Option<&'a mut [u8]> {
    if len < 0 || (len > 0 && ptr <= 0) {
        return None;
    }
    if len == 0 {
        return Some(&mut []);
    }
    Some(unsafe { std::slice::from_raw_parts_mut(ptr as *mut u8, len as usize) })
}

/// Copy as much of `from` as fits into `out`, answering the count — and
/// refusing rather than truncating when the caller asked for all of it.
fn copy_out(from: &[u8], out: &mut [u8]) -> i32 {
    let n = from.len().min(out.len());
    out[..n].copy_from_slice(&from[..n]);
    n as i32
}

/// A byte buffer the host writes into before a call, and frees after.
///
/// One-byte alignment: every buffer this ABI takes is bytes.
///
/// # Safety
///
/// Freeing is [`emu_free`]'s, with the same length. A zero or negative
/// length allocates nothing and answers 0, which the host treats as "no
/// buffer needed" rather than as an error.
#[unsafe(no_mangle)]
pub extern "C" fn emu_alloc(len: i32) -> i32 {
    if len <= 0 {
        return 0;
    }
    let Ok(layout) = Layout::from_size_align(len as usize, 1) else {
        return AbiError::BadBuffer.with(format!("cannot lay out {len} bytes"));
    };
    // SAFETY: a non-zero size with a valid layout.
    let ptr = unsafe { alloc(layout) };
    if ptr.is_null() {
        return AbiError::BadBuffer.with(format!("out of memory allocating {len} bytes"));
    }
    ptr as i32
}

/// Free a buffer from [`emu_alloc`]. The length must be the one it was
/// allocated with.
///
/// # Safety
///
/// Calling this with a pointer this module did not allocate, or with a
/// different length, is undefined behaviour — the ordinary allocator
/// contract, stated because the caller is on the other side of a wall.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn emu_free(ptr: i32, len: i32) {
    if ptr <= 0 || len <= 0 {
        return;
    }
    if let Ok(layout) = Layout::from_size_align(len as usize, 1) {
        // SAFETY: the caller's contract, above.
        unsafe { dealloc(ptr as *mut u8, layout) };
    }
}

// ---- version and errors -------------------------------------------------

/// [`ABI_VERSION`]. The first call the host makes.
#[unsafe(no_mangle)]
pub extern "C" fn emu_abi_version() -> i32 {
    ABI_VERSION
}

/// The largest control reply, so the host can size one scratch buffer.
#[unsafe(no_mangle)]
pub extern "C" fn emu_reply_max() -> i32 {
    REPLY_MAX
}

/// The sentence behind the last negative return, as UTF-8. Empty when
/// nothing has failed.
///
/// # Safety
///
/// `out_ptr`/`out_cap` must name a writable range in this module's memory.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn emu_last_error(out_ptr: i32, out_cap: i32) -> i32 {
    // SAFETY: the caller's contract.
    let Some(out) = (unsafe { bytes_out(out_ptr, out_cap) }) else {
        return AbiError::BadBuffer.code();
    };
    // SAFETY: see `Slot::get`.
    let message = unsafe { LAST_ERROR.get() };
    if message.len() > out.len() {
        return AbiError::BufferTooSmall.code();
    }
    copy_out(message.as_bytes(), out)
}

// ---- creation -----------------------------------------------------------

/// Build the machine from a text config and the flash chip's starting bytes.
///
/// An empty flash slice is a blank chip — which is what a board that has
/// never been flashed is, not an error. Refuses a second machine: one board
/// per module, one module per Worker.
///
/// # Safety
///
/// The two ranges must be readable in this module's memory.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn emu_create(
    cfg_ptr: i32,
    cfg_len: i32,
    flash_ptr: i32,
    flash_len: i32,
) -> i32 {
    // SAFETY: the caller's contract.
    unsafe { create(cfg_ptr, cfg_len, flash_ptr, flash_len, 0, 0) }
}

/// [`emu_create`] with an application ELF placed by the host loader — the
/// `boot=direct` form, whose app cannot ride the flash chip.
///
/// # Safety
///
/// As [`emu_create`], plus the app range.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn emu_create_direct(
    cfg_ptr: i32,
    cfg_len: i32,
    flash_ptr: i32,
    flash_len: i32,
    app_ptr: i32,
    app_len: i32,
) -> i32 {
    // SAFETY: the caller's contract.
    unsafe { create(cfg_ptr, cfg_len, flash_ptr, flash_len, app_ptr, app_len) }
}

/// # Safety
///
/// Every range must be readable in this module's memory.
unsafe fn create(
    cfg_ptr: i32,
    cfg_len: i32,
    flash_ptr: i32,
    flash_len: i32,
    app_ptr: i32,
    app_len: i32,
) -> i32 {
    // SAFETY: the caller's contract.
    let slot = unsafe { host() };
    if slot.is_some() {
        return AbiError::AlreadyCreated
            .with("this module already holds a machine; make another Worker");
    }
    // SAFETY: the caller's contract.
    let (Some(cfg_bytes), Some(flash_bytes), Some(app_bytes)) = (unsafe {
        (
            bytes_in(cfg_ptr, cfg_len),
            bytes_in(flash_ptr, flash_len),
            bytes_in(app_ptr, app_len),
        )
    }) else {
        return AbiError::BadBuffer.with("emu_create was given a range outside module memory");
    };
    let Ok(cfg_text) = std::str::from_utf8(cfg_bytes) else {
        return AbiError::BadConfig.with("the config is not UTF-8");
    };
    let cfg = match Config::parse(cfg_text) {
        Ok(cfg) => cfg,
        Err(reason) => return AbiError::BadConfig.with(reason),
    };
    let app = if app_bytes.is_empty() {
        AppSource::None
    } else {
        AppSource::Bytes(app_bytes.to_vec())
    };
    if cfg.boot == BootMode::Direct && matches!(app, AppSource::None) {
        return AbiError::BadConfig.with("boot=direct needs an app: use emu_create_direct");
    }
    let machine = match cfg
        .builder(FlashBacking::Bytes(flash_bytes.to_vec()), app)
        .build()
    {
        Ok(machine) => machine,
        Err(e) => return AbiError::BuildFailed.with(e.to_string()),
    };
    let usb_in = machine
        .usb_sj_host_handle()
        .expect("the builder installed a queue source");
    *slot = Some(Host {
        machine,
        usb_in,
        usb_out: VecDeque::new(),
        uart_out: VecDeque::new(),
        stopped: None,
    });
    set_last_error(String::new());
    0
}

/// Drop the machine. Idempotent: destroying nothing is not an error.
#[unsafe(no_mangle)]
pub extern "C" fn emu_destroy() {
    // SAFETY: see `Slot::get`.
    *unsafe { host() } = None;
}

// ---- running ------------------------------------------------------------

/// Run at most `budget_cycles` of **guest** time and answer an outcome code.
///
/// The slice carries no wall timeout and no exit condition: the host decides
/// when to stop by choosing the budget, and it is the only party that knows
/// what wall time is. A machine that has already returned a stopping outcome
/// answers that same code again without running — a hart that cannot
/// continue is not stepped twice.
#[unsafe(no_mangle)]
pub extern "C" fn emu_run(budget_cycles: i64) -> i32 {
    // SAFETY: see `Slot::get`.
    let Some(host) = (unsafe { host() }).as_mut() else {
        return AbiError::NoMachine.code();
    };
    if let Some(code) = host.stopped {
        return code;
    }
    if budget_cycles <= 0 {
        return AbiError::BadBuffer.with("emu_run needs a positive cycle budget");
    }
    let stop = StopCondition {
        stop_cycle: Some(host.machine.cycles().saturating_add(budget_cycles as u64)),
        ..Default::default()
    };
    let outcome = host.machine.run_until(&stop);
    let code = code_for(&outcome);
    if code != outcome_code::DEADLINE {
        host.stopped = Some(code);
        set_last_error(format!("{outcome:?}"));
    }
    code
}

/// Guest cycles since power-on — or since the last reboot, because a reboot
/// restarts the chip's clock exactly as it does on the part.
#[unsafe(no_mangle)]
pub extern "C" fn emu_cycles() -> i64 {
    with_host(|host| host.machine.cycles() as i64)
}

/// The same as microseconds of emulated time.
#[unsafe(no_mangle)]
pub extern "C" fn emu_micros() -> i64 {
    with_host(|host| host.machine.micros() as i64)
}

/// How many times the chip has rebooted itself under `reboot_on_reset`.
#[unsafe(no_mangle)]
pub extern "C" fn emu_reboots() -> i64 {
    with_host(|host| host.machine.reboots() as i64)
}

fn with_host(f: impl FnOnce(&mut Host) -> i64) -> i64 {
    // SAFETY: see `Slot::get`.
    match (unsafe { host() }).as_mut() {
        Some(host) => f(host),
        None => AbiError::NoMachine.code() as i64,
    }
}

// ---- the two channels ---------------------------------------------------

/// Apply one control line and write its reply text into `out`.
///
/// The line grammar and the replies are the control protocol's, identical to
/// the `--control` socket's — `attach`, `open`, `signals dtr=… rts=…`,
/// `reset`, `state`, `pins`. A refusal is an `err …` reply with a
/// non-negative length, not a negative return: the protocol answering "no"
/// is not the ABI failing.
///
/// # Safety
///
/// Both ranges must be in this module's memory.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn emu_control(
    line_ptr: i32,
    line_len: i32,
    out_ptr: i32,
    out_cap: i32,
) -> i32 {
    // SAFETY: see `Slot::get`.
    let Some(host) = (unsafe { host() }).as_mut() else {
        return AbiError::NoMachine.code();
    };
    // SAFETY: the caller's contract.
    let (Some(line_bytes), Some(out)) =
        (unsafe { (bytes_in(line_ptr, line_len), bytes_out(out_ptr, out_cap)) })
    else {
        return AbiError::BadBuffer.with("emu_control was given a range outside module memory");
    };
    let Ok(line) = std::str::from_utf8(line_bytes) else {
        return AbiError::BadConfig.with("the control line is not UTF-8");
    };
    let reply = host.machine.control_line(line).to_string();
    if reply.len() > out.len() {
        return AbiError::BufferTooSmall.with(format!(
            "a {}-byte reply does not fit a {}-byte buffer",
            reply.len(),
            out.len()
        ));
    }
    copy_out(reply.as_bytes(), out)
}

/// Host → guest bytes on the USB link. They are delivered at whichever slice
/// boundary the USB block next polls at, exactly as a socket client's are.
///
/// # Safety
///
/// The range must be readable in this module's memory.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn emu_usb_write(ptr: i32, len: i32) -> i32 {
    // SAFETY: see `Slot::get`.
    let Some(host) = (unsafe { host() }).as_mut() else {
        return AbiError::NoMachine.code();
    };
    // SAFETY: the caller's contract.
    let Some(bytes) = (unsafe { bytes_in(ptr, len) }) else {
        return AbiError::BadBuffer.with("emu_usb_write was given a range outside module memory");
    };
    host.usb_in.push(bytes);
    bytes.len() as i32
}

/// Guest → host bytes on the USB link, as many as fit. What does not fit is
/// kept for the next call, so a small buffer costs calls and never bytes.
///
/// # Safety
///
/// The range must be writable in this module's memory.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn emu_usb_read(out_ptr: i32, out_cap: i32) -> i32 {
    // SAFETY: the caller's contract.
    unsafe { drain_into(Channel::Usb, out_ptr, out_cap) }
}

/// The console, on the same terms.
///
/// # Safety
///
/// The range must be writable in this module's memory.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn emu_uart0_read(out_ptr: i32, out_cap: i32) -> i32 {
    // SAFETY: the caller's contract.
    unsafe { drain_into(Channel::Uart0, out_ptr, out_cap) }
}

enum Channel {
    Usb,
    Uart0,
}

/// # Safety
///
/// The range must be writable in this module's memory.
unsafe fn drain_into(channel: Channel, out_ptr: i32, out_cap: i32) -> i32 {
    // SAFETY: see `Slot::get`.
    let Some(host) = (unsafe { host() }).as_mut() else {
        return AbiError::NoMachine.code();
    };
    // SAFETY: the caller's contract.
    let Some(out) = (unsafe { bytes_out(out_ptr, out_cap) }) else {
        return AbiError::BadBuffer.with("a read was given a range outside module memory");
    };
    // Take everything the machine has, then serve from the leftover: the
    // machine's log is drained whole so a small host buffer cannot leave
    // bytes sitting where `--exit-on` and a reboot would see them.
    let (fresh, pending) = match channel {
        Channel::Usb => (host.machine.take_usb_sj_output(), &mut host.usb_out),
        Channel::Uart0 => (host.machine.take_uart0_output(), &mut host.uart_out),
    };
    pending.extend(fresh);
    let n = pending.len().min(out.len());
    for (slot, byte) in out[..n].iter_mut().zip(pending.drain(..n)) {
        *slot = byte;
    }
    n as i32
}

// ---- the chip -----------------------------------------------------------

/// The modelled chip's size in bytes.
#[unsafe(no_mangle)]
pub extern "C" fn emu_flash_len() -> i32 {
    with_host(|host| {
        host.machine
            .flash()
            .lock()
            .map(|flash| flash.len() as i64)
            .unwrap_or(AbiError::NoMachine.code() as i64)
    }) as i32
}

/// Copy `len` bytes of the chip at `off` out into `out_ptr`. Chunked reads
/// are supported so a 4 MiB image can be streamed rather than held twice.
///
/// # Safety
///
/// The range must be writable in this module's memory.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn emu_flash_read(off: i32, out_ptr: i32, len: i32) -> i32 {
    // SAFETY: see `Slot::get`.
    let Some(host) = (unsafe { host() }).as_mut() else {
        return AbiError::NoMachine.code();
    };
    // SAFETY: the caller's contract.
    let Some(out) = (unsafe { bytes_out(out_ptr, len) }) else {
        return AbiError::BadBuffer.with("emu_flash_read was given a range outside module memory");
    };
    if off < 0 {
        return AbiError::OutOfRange.with(format!("a flash offset cannot be {off}"));
    }
    let Ok(flash) = host.machine.flash().lock() else {
        return AbiError::NoMachine.with("the flash chip is poisoned");
    };
    // `peek`, not `read`: copying the chip out is the host's business, and
    // it must not appear in the census of what the GUEST asked the
    // controller to do.
    match flash.peek(off as u32, out.len() as u32) {
        Some(bytes) => copy_out(bytes, out),
        None => AbiError::OutOfRange.with(format!(
            "{}..{} leaves a {}-byte chip",
            off,
            off as i64 + out.len() as i64,
            flash.len()
        )),
    }
}

/// Write bytes into the chip at `off` — a **direct** write: every sector the
/// range touches is erased first, then the bytes are placed.
///
/// This is a flasher's write, not a program: a page-program can only clear
/// bits, so writing an image over an unerased chip would AND it into
/// nonsense. The host that calls this has the whole image and means it (D5).
///
/// # Safety
///
/// The range must be readable in this module's memory.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn emu_flash_write(off: i32, ptr: i32, len: i32) -> i32 {
    // SAFETY: see `Slot::get`.
    let Some(host) = (unsafe { host() }).as_mut() else {
        return AbiError::NoMachine.code();
    };
    // SAFETY: the caller's contract.
    let Some(bytes) = (unsafe { bytes_in(ptr, len) }) else {
        return AbiError::BadBuffer.with("emu_flash_write was given a range outside module memory");
    };
    if off < 0 {
        return AbiError::OutOfRange.with(format!("a flash offset cannot be {off}"));
    }
    if bytes.is_empty() {
        return 0;
    }
    let sector = crate::flash::SECTOR_LEN;
    let Ok(mut flash) = host.machine.flash().lock() else {
        return AbiError::NoMachine.with("the flash chip is poisoned");
    };
    let off = off as u32;
    let end = off as u64 + bytes.len() as u64;
    if end > u64::from(flash.len()) {
        return AbiError::OutOfRange
            .with(format!("{off}..{end} leaves a {}-byte chip", flash.len()));
    }
    let first = (off / sector) * sector;
    let last = ((end as u32).div_ceil(sector)) * sector;
    for base in (first..last).step_by(sector as usize) {
        if !flash.erase(base, sector) {
            return AbiError::OutOfRange.with(format!("the sector at {base} is not on the chip"));
        }
    }
    if !flash.stage(off, bytes) {
        return AbiError::OutOfRange.with(format!("{off}..{end} leaves the chip"));
    }
    bytes.len() as i32
}

/// Erase the whole chip — the `Erase` verb, and the first half of a flash.
#[unsafe(no_mangle)]
pub extern "C" fn emu_flash_erase_chip() -> i32 {
    // SAFETY: see `Slot::get`.
    let Some(host) = (unsafe { host() }).as_mut() else {
        return AbiError::NoMachine.code();
    };
    let Ok(mut flash) = host.machine.flash().lock() else {
        return AbiError::NoMachine.with("the flash chip is poisoned");
    };
    flash.erase_chip();
    0
}

/// `1` when the chip has changed since the host last said it had saved it.
/// The persistence cadence's question.
#[unsafe(no_mangle)]
pub extern "C" fn emu_flash_dirty() -> i32 {
    flash_flag(|flash| flash.dirty())
}

/// The host has persisted the current bytes: clear the flag.
///
/// Nothing clears it on the host's behalf, so a Worker that was killed
/// between the read and the write comes back with a chip that still knows it
/// was not saved.
#[unsafe(no_mangle)]
pub extern "C" fn emu_flash_mark_saved() -> i32 {
    // SAFETY: see `Slot::get`.
    let Some(host) = (unsafe { host() }).as_mut() else {
        return AbiError::NoMachine.code();
    };
    let Ok(mut flash) = host.machine.flash().lock() else {
        return AbiError::NoMachine.with("the flash chip is poisoned");
    };
    flash.mark_saved();
    0
}

/// `1` when the reset vector holds something the ROM would boot — the
/// `flash` word's `blank` / `loaded` answer.
#[unsafe(no_mangle)]
pub extern "C" fn emu_flash_has_image() -> i32 {
    // SAFETY: see `Slot::get`.
    match (unsafe { host() }).as_ref() {
        Some(host) => i32::from(host.machine.has_image_at_reset_vector()),
        None => AbiError::NoMachine.code(),
    }
}

fn flash_flag(f: impl FnOnce(&crate::flash::FlashImage) -> bool) -> i32 {
    // SAFETY: see `Slot::get`.
    let Some(host) = (unsafe { host() }).as_ref() else {
        return AbiError::NoMachine.code();
    };
    match host.machine.flash().lock() {
        Ok(flash) => i32::from(f(&flash)),
        Err(_) => AbiError::NoMachine.with("the flash chip is poisoned"),
    }
}
