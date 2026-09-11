// How JavaScript talks to the ESP32-C6 emulator module: the WASI imports it
// declares, and the buffer plumbing over its `emu_*` ABI.
//
// The module is `lp-emu-esp32c6` built for `wasm32-wasip1` — an ordinary
// command binary with a `_start` we never call, plus twenty-three exports we
// do. See `lp-emu/esp/README.md` §"The tab host" for the ABI itself; this
// file is the other side of it. `emulator_worker.js` is the caller (it holds
// the module and the pacing loop); `emulator_tab.js` is the port object one
// layer up. `docs/adr/2026-09-10-the-c6-emulator-runs-in-the-tab.md` is the
// decision record for the whole shape.
//
// NO FILE FAÇADE. The bench rig's shim (`scripts/emu/bench-web/worker.js`,
// which is M7's file and is not imported here) stages a firmware ELF as a
// WASI file and reads UART0 back out of another, because it drives `_start`
// with a command line. The tab drives the exports instead, and every byte in
// or out of this machine crosses through memory. So `path_open` answers
// ENOENT for everything, and that is a complete implementation rather than a
// stub: there is no file for the guest to want.
//
// The imports are the eighteen the module actually declares
// (`wasm-tools print … | grep import`). On the export path exactly ONE of
// them is ever called — `clock_time_get`, which is `run_until`'s
// unconditional `Instant::now()` and cannot influence a slice that carries no
// wall timeout. The rest exist because the binary also has a `_start`.
//
// Runnable under node as well as in a Worker: no DOM, no `self`, no
// `postMessage`. `scripts/emu/tab-smoke.mjs` is the node caller.

/** WASI preview1 errno values this shim answers with. */
const EBADF = 8;
const ENOENT = 44;
const ENOTSUP = 58;

/** The ABI revision this file speaks. Must equal `emu_abi_version()`. */
export const EMU_ABI = 1;

/**
 * The negative returns, mirrored from `lp_emu_esp32c6::tab_abi::AbiError`.
 * Kept as a name-per-number rather than a bare lookup so a wrong code reads
 * as a wrong word in a message rather than as a number nobody recognises.
 */
export const ABI_ERRORS = {
  "-1": "no machine (emu_create was not called, or emu_destroy was)",
  "-2": "already created (one board per module)",
  "-3": "bad config",
  "-4": "build failed",
  "-5": "buffer too small",
  "-6": "bad buffer",
  "-7": "out of range",
  "-8": "unsupported",
};

/** `emu_run`'s outcome codes, mirrored from the same module. */
export const OUTCOMES = {
  0: "deadline",
  1: "exit-matched",
  2: "fault",
  3: "strict-bus",
  4: "reset",
  5: "breakpoint",
  6: "wall-timeout",
};

const encoder = new TextEncoder();
const decoder = new TextDecoder();

/**
 * The imports the module declares, and the two things a caller needs back:
 * somewhere to put the memory once instantiation hands it over, and the text
 * the guest's own `_start`-side writes produced (nothing writes there on the
 * export path, so it is a diagnostic and not a channel).
 */
export function makeWasi() {
  let memory = null;
  const std = { 1: [], 2: [] };

  const view = () => new DataView(memory.buffer);
  const bytes = () => new Uint8Array(memory.buffer);

  const wasi = {
    // No arguments and no environment: nothing on the export path reads
    // either, and answering "none" is cheaper and truer than inventing a
    // command line for a `_start` that is never called.
    args_sizes_get(argcPtr, bufSizePtr) {
      const dv = view();
      dv.setUint32(argcPtr, 0, true);
      dv.setUint32(bufSizePtr, 0, true);
      return 0;
    },
    args_get() {
      return 0;
    },
    environ_sizes_get(countPtr, sizePtr) {
      const dv = view();
      dv.setUint32(countPtr, 0, true);
      dv.setUint32(sizePtr, 0, true);
      return 0;
    },
    environ_get() {
      return 0;
    },

    // The one import the export path actually calls. `id === 0` is
    // CLOCK_REALTIME and everything else is treated as the monotonic clock,
    // which is what `Instant::now()` compiles to. `performance.now()` rather
    // than `Date.now()` for the monotonic one so it cannot go backwards
    // across a clock adjustment.
    clock_time_get(id, _precision, out) {
      const ns =
        id === 0
          ? BigInt(Math.round(Date.now() * 1e6))
          : BigInt(Math.round(performance.now() * 1e6));
      view().setBigUint64(out, ns, true);
      return 0;
    },

    random_get(ptr, len) {
      const m = bytes();
      // `crypto.getRandomValues` refuses more than 65536 bytes at once.
      for (let off = 0; off < len; off += 65536) {
        crypto.getRandomValues(m.subarray(ptr + off, ptr + Math.min(len, off + 65536)));
      }
      return 0;
    },

    // There is no pre-opened directory, so there is no path a relative open
    // could be resolved against, and `path_open` below never succeeds.
    fd_prestat_get() {
      return EBADF;
    },
    fd_prestat_dir_name() {
      return EBADF;
    },
    path_open() {
      return ENOENT;
    },
    path_filestat_get() {
      return ENOENT;
    },

    fd_fdstat_get(fd, buf) {
      if (fd > 2) return EBADF;
      const dv = view();
      // filetype 2 = character device: what a std stream is.
      dv.setUint8(buf, 2);
      dv.setUint16(buf + 2, 0, true);
      dv.setBigUint64(buf + 8, 0xffffffffffffffffn, true);
      dv.setBigUint64(buf + 16, 0xffffffffffffffffn, true);
      return 0;
    },
    fd_fdstat_set_flags() {
      // Nothing here has blocking or append semantics to change.
      return 0;
    },
    fd_filestat_get() {
      return EBADF;
    },
    fd_read() {
      return EBADF;
    },
    fd_write(fd, iovs, iovsLen, writtenPtr) {
      const sink = std[fd];
      if (!sink) return EBADF;
      const dv = view();
      const m = bytes();
      let total = 0;
      for (let i = 0; i < iovsLen; i++) {
        const ptr = dv.getUint32(iovs + 8 * i, true);
        const len = dv.getUint32(iovs + 8 * i + 4, true);
        sink.push(m.slice(ptr, ptr + len));
        total += len;
      }
      dv.setUint32(writtenPtr, total, true);
      return 0;
    },
    fd_close() {
      return 0;
    },
    // There is no listener in a tab and never will be: a clean refusal is
    // the right answer, not a missing feature.
    sock_accept() {
      return ENOTSUP;
    },
    // Nothing on the export path exits. If something does, it is a bug, and
    // a thrown object is how it reaches the worker's error message rather
    // than silently ending a slice.
    proc_exit(code) {
      throw new Error(`the emulator module called proc_exit(${code})`);
    },
  };

  return {
    imports: { wasi_snapshot_preview1: wasi },
    setMemory(m) {
      memory = m;
    },
    /** Whatever the module wrote to stdout (1) or stderr (2), as text. */
    stdText(fd = 2) {
      const parts = std[fd] ?? [];
      const total = parts.reduce((n, part) => n + part.length, 0);
      const all = new Uint8Array(total);
      let at = 0;
      for (const part of parts) {
        all.set(part, at);
        at += part.length;
      }
      return decoder.decode(all);
    },
  };
}

/** A negative ABI return, as an `Error` that names the code and the call. */
export class EmuAbiError extends Error {
  constructor(call, code, detail) {
    const name = ABI_ERRORS[String(code)] ?? `unknown code ${code}`;
    super(detail ? `${call}: ${name} — ${detail}` : `${call}: ${name}`);
    this.name = "EmuAbiError";
    this.call = call;
    this.code = code;
  }
}

/**
 * The `emu_*` exports as ordinary JavaScript, with the buffers handled.
 *
 * Every call that takes or returns bytes needs a range inside the module's
 * own memory, allocated by the module and freed by it. That bookkeeping is
 * here so the worker's calls read as one line each, and so there is exactly
 * one place that can leak.
 *
 * `memory.buffer` is re-read on every access: a `memory.grow` (which
 * `emu_alloc` can cause) detaches the old `ArrayBuffer`, and a `Uint8Array`
 * held across a call would be a zero-length view onto nothing.
 */
export function bindEmu(instance) {
  const e = instance.exports;
  const version = e.emu_abi_version();
  if (version !== EMU_ABI) {
    throw new Error(
      `the emulator module speaks emu_abi=${version}; this page speaks ${EMU_ABI}. ` +
        `Rebuild it with \`just emu-c6-wasm\`.`,
    );
  }
  const replyCap = e.emu_reply_max();
  const u8 = () => new Uint8Array(e.memory.buffer);

  /** Run `fn(ptr, len)` with `data` copied into module memory. */
  const withIn = (data, fn) => {
    if (data.length === 0) return fn(0, 0);
    const ptr = e.emu_alloc(data.length);
    if (ptr <= 0) throw new EmuAbiError("emu_alloc", ptr);
    try {
      u8().set(data, ptr);
      return fn(ptr, data.length);
    } finally {
      e.emu_free(ptr, data.length);
    }
  };

  /** Run `fn(ptr, cap)` over a scratch buffer and hand back what it wrote. */
  const withOut = (cap, call, fn) => {
    const ptr = e.emu_alloc(cap);
    if (ptr <= 0) throw new EmuAbiError("emu_alloc", ptr);
    try {
      const n = fn(ptr, cap);
      if (n < 0) throw new EmuAbiError(call, n, lastError());
      // Copied, not viewed: the caller keeps this past the next call, and
      // the next call may grow (and detach) the memory.
      return u8().slice(ptr, ptr + n);
    } finally {
      e.emu_free(ptr, cap);
    }
  };

  /** The sentence behind the last negative return, or "" — never throws. */
  const lastError = () => {
    const ptr = e.emu_alloc(replyCap);
    if (ptr <= 0) return "";
    try {
      const n = e.emu_last_error(ptr, replyCap);
      return n > 0 ? decoder.decode(u8().slice(ptr, ptr + n)) : "";
    } catch {
      return "";
    } finally {
      e.emu_free(ptr, replyCap);
    }
  };

  const check = (call, code) => {
    if (code < 0) throw new EmuAbiError(call, code, lastError());
    return code;
  };

  return {
    exports: e,
    abiVersion: version,
    lastError,

    /** Build the board. `flash` may be empty — that IS a blank chip. */
    create(configText, flash = new Uint8Array(0), app = null) {
      const cfg = encoder.encode(configText);
      return withIn(cfg, (cfgPtr, cfgLen) =>
        withIn(flash, (flashPtr, flashLen) => {
          if (app && app.length > 0) {
            return withIn(app, (appPtr, appLen) =>
              check(
                "emu_create_direct",
                e.emu_create_direct(cfgPtr, cfgLen, flashPtr, flashLen, appPtr, appLen),
              ),
            );
          }
          return check("emu_create", e.emu_create(cfgPtr, cfgLen, flashPtr, flashLen));
        }),
      );
    },
    destroy: () => e.emu_destroy(),

    /** One slice. Answers an outcome CODE; negative would be an ABI error. */
    run(budgetCycles) {
      return check("emu_run", e.emu_run(BigInt(budgetCycles)));
    },
    cycles: () => e.emu_cycles(),
    micros: () => e.emu_micros(),
    reboots: () => e.emu_reboots(),

    /** One control line; the reply text, `ok …` or `err …`, never thrown. */
    control(line) {
      const text = encoder.encode(line);
      return decoder.decode(
        withIn(text, (ptr, len) =>
          withOut(replyCap, "emu_control", (outPtr, cap) =>
            e.emu_control(ptr, len, outPtr, cap),
          ),
        ),
      );
    },

    usbWrite: (data) =>
      withIn(data, (ptr, len) => check("emu_usb_write", e.emu_usb_write(ptr, len))),
    usbRead: (cap) => withOut(cap, "emu_usb_read", (ptr, n) => e.emu_usb_read(ptr, n)),
    uart0Read: (cap) => withOut(cap, "emu_uart0_read", (ptr, n) => e.emu_uart0_read(ptr, n)),

    flashLen: () => check("emu_flash_len", e.emu_flash_len()),
    flashRead: (offset, len) =>
      withOut(len, "emu_flash_read", (ptr, cap) => e.emu_flash_read(offset, ptr, cap)),
    flashWrite: (offset, data) =>
      withIn(data, (ptr, len) =>
        check("emu_flash_write", e.emu_flash_write(offset, ptr, len)),
      ),
    flashEraseChip: () => check("emu_flash_erase_chip", e.emu_flash_erase_chip()),
    flashDirty: () => check("emu_flash_dirty", e.emu_flash_dirty()) === 1,
    flashMarkSaved: () => check("emu_flash_mark_saved", e.emu_flash_mark_saved()),
    flashHasImage: () => check("emu_flash_has_image", e.emu_flash_has_image()) === 1,
  };
}

/**
 * Instantiate `module` (a compiled `WebAssembly.Module`) and hand back the
 * bound ABI. `_start` is never called; see this file's header.
 */
export async function instantiateEmu(module) {
  const wasi = makeWasi();
  const instance = await WebAssembly.instantiate(module, wasi.imports);
  wasi.setMemory(instance.exports.memory);
  const emu = bindEmu(instance);
  emu.stdText = wasi.stdText;
  return emu;
}
