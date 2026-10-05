// A model of the bytes a Mac's serial path drops before Web Serial sees them.
//
// Chromium on macOS opens a serial port with `PARMRK` set and `IGNBRK` clear,
// so xnu's tty line discipline queues every data `0xFF` TWICE in its 1,024
// slots, and `IOSerialBSDClient::getData` feeds the tty `1020 - queued` bytes
// a pass in a `UInt32`: once doubling has pushed the queue past 1,020 the
// subtraction wraps, every pass hands the full tty another kilobyte, and the
// tty drops what does not fit. Bytes that are not `0xFF` never pass 1,020,
// and for them the count is exact backpressure: the board's bytes wait in the
// driver, nothing is lost. The whole mechanism, measured on silicon, is
// `docs/defects/2026-09-26-web-serial-on-macos-drops-bytes-of-packed-frames.md`.
//
// The emulator's `navigator.serial` shim (`virtual_serial.js`) is a lossless
// pipe without this, and that is how a flash read that stalls on every Mac
// passed the emulator walk (G1-F2, 2026-10-02:
// `docs/defects/2026-10-02-the-emulated-serial-path-never-drops-a-byte.md`).
// With it, a burst of `0xFF`-heavy bytes that arrives while the page is not
// reading loses bytes exactly where a Mac would.
//
// Pure and synchronous on purpose: the shim drives it from its stream, and
// `lp-app/lpa-link/tests/js/mac_tty_model.test.mjs` drives it from node.

/// The tty's raw queue (`MAX_INPUT`).
export const TTY_SLOTS = 1024;
/// `TTY_HIGHWATER`: what IOSerialBSDClient tries to keep the queue under.
export const TTY_HIGHWATER = 1020;
/// Chromium's read pipe between the tty and the page (`bufferSize`, default).
export const CHROMIUM_PIPE = 255;

export class MacTtyModel {
  constructor({ pipe = CHROMIUM_PIPE } = {}) {
    // The driver's side: bytes the board sent that the tty has not taken.
    // Unbounded, because USB backpressure holds them on the board instead.
    this.driver = [];
    // The tty's queue, one entry per data byte, and the slots it costs.
    this.tty = [];
    this.slots = 0;
    // Chromium's pipe, already folded back to one byte per data byte.
    this.pipe = [];
    this.pipeCapacity = pipe;
    this.dropped = 0;
  }

  /// Bytes from the board.
  arrive(bytes) {
    for (const byte of bytes) this.driver.push(byte);
    this.pump();
  }

  /// What the page's `read()` would get now (at most the pipe's worth), or
  /// an empty array when nothing is waiting.
  read() {
    const out = Uint8Array.from(this.pipe.splice(0, this.pipe.length));
    this.pump();
    return out;
  }

  /// Bytes anywhere on the way (driver, tty or pipe).
  pending() {
    return this.driver.length + this.tty.length + this.pipe.length;
  }

  /// IOSerialBSDClient's passes, until the driver is empty or the tty is at
  /// its high-water mark (backpressure: the rest waits for a read). Every
  /// pass takes at least one byte or returns, so this ends.
  pump() {
    for (;;) {
      this.drainTtyIntoPipe();
      if (this.driver.length === 0) return;
      // The UInt32: at or under the high-water mark it is the room left;
      // over it, it wraps and `MIN(…, 1024)` makes it a full kilobyte.
      const room = this.slots <= TTY_HIGHWATER ? TTY_HIGHWATER - this.slots : TTY_SLOTS;
      if (room === 0) return;
      for (const byte of this.driver.splice(0, room)) {
        const cost = byte === 0xff ? 2 : 1;
        if (this.slots + cost > TTY_SLOTS) {
          this.dropped += 1;
          continue;
        }
        this.tty.push(byte);
        this.slots += cost;
      }
    }
  }

  drainTtyIntoPipe() {
    while (this.tty.length > 0 && this.pipe.length < this.pipeCapacity) {
      const byte = this.tty.shift();
      this.slots -= byte === 0xff ? 2 : 1;
      this.pipe.push(byte);
    }
  }
}
