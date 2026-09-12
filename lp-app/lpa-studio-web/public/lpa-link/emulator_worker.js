// One emulated ESP32-C6, on its own thread, paced against wall time.
//
// A dedicated module Worker holding one instance of the `lp-emu-esp32c6`
// wasip1 module. It is the wasm backing of `EmulatorPort`: everything the
// `lp-cli emu serve` door does over two WebSockets, this does over
// `postMessage`, and the port object above it cannot tell which it has.
//
// ONE THREAD, ONE BOARD. The module holds a single machine in a static slot
// (`lp_emu_esp32c6::tab_abi`), so a second board is a second Worker. That is
// also why the machine can be driven with no locking at all: this file has
// the only reference, and the guest only runs inside `emu_run`.
//
// ONE HOST PER BOARD, TOO (P5b). The module imports `emu_host` — since M7 P7
// the wasm build installs a translated core by default — so the Worker builds
// `makeJitHost()` from M7's `jit-host.js` and attaches it before `emu_create`.
// `jit-host.js` requires a dedicated Worker (its compiles are synchronous, at
// up to 64 MB), which this is; the host's table slots belong to this Worker's
// one module instance, which is why it is built here and not shared. The URL
// arrives on the `create` message, content-hashed, the way the module's does.
//
// ============================ THE PACING RULE ============================
//
// Guest time is a budget the host hands out; it is never a clock the guest
// reads. Every slice is `emu_run(cycles)` with no wall timeout, and the
// conversion from wall time to cycles happens HERE, on this side of the
// wall — which is the whole of PD5 as it applies to a browser.
//
//     origin = now(), guestOrigin = 0
//     loop:
//       wallUs   = (now() - origin) * 1000
//       guestUs  = micros() - guestOrigin
//       deficit  = wallUs - guestUs
//       if deficit <= 0:  await tick(); continue      // ahead: wait
//       budget   = min(deficit, SLICE_US) as cycles   // never more than one slice
//       outcome  = emu_run(budget)
//       drain: USB bytes to the page NOW; console text into a buffer
//       if micros() went backwards: the chip rebooted — re-anchor
//       if deficit still > SLICE_US: DROP it — re-anchor to now — and let
//                                   the dilation window report the loss
//       await tick()                                  // let the inbox land
//
// **The deficit is dropped, never repaid** (plan decision D13). A tab that
// was hidden for thirty seconds comes back thirty seconds behind; repaying
// that would mean a thirty-second guest sprint, during which the board would
// answer nothing and then answer everything at once. Dropping it makes the
// board honest instead: it was slow, it says so as a number, and it carries
// on from now. `dilation` — guest microseconds advanced per wall microsecond
// elapsed, over a one-second sliding window — IS that number.
//
// `tick()` is a `MessageChannel` post rather than `setTimeout(0)`, which
// browsers clamp to 4 ms after a few nested calls and throttle hard in a
// hidden tab. The await is not politeness: inbound messages can only be
// applied BETWEEN slices (the machine's own rule — a control line never
// lands between two instructions), so a loop that never yielded would never
// read its inbox.
//
// ========================== THE CONSOLE CADENCE ==========================
//
// The two channels leave this thread on different terms. USB bytes are the
// wire: esptool's SLIP frames and the firmware's hello go out the moment a
// slice produces them, untouched, because a flasher's timeouts are counting.
// UART0 is the console, and nobody's timeout is counting on it — but a blank
// ESP32-C6 prints `invalid header: 0xffffffff` in a tight loop, and posting
// that line on every pacing slice woke the main thread ten times a second
// for text nobody was reading (measured 2026-09-11 on an unflashed
// `?emu=tab` board). So console text is COALESCED here and posted when it
// has waited `CONSOLE_EVERY_MS` of wall time or grown past
// `CONSOLE_FLUSH_BYTES`, whichever comes first. Bytes are concatenated in
// the order the guest wrote them and decoded once per flush with a streaming
// decoder, so a multi-byte character split across two slices — or across two
// flushes — comes out whole. `destroy` flushes what is left before it
// answers. Nothing is dropped and nothing is reordered; only the wake-ups
// are fewer.
//
// =========================================================================
//
// WHAT IS NOT HERE. No `navigator.serial`, no port object, no knowledge of
// Studio: this file speaks messages. `emulator_tab.js` is what turns it into
// a backing. No wall duration is ever asserted or compared — the numbers
// this file reports are observations, and `lp-emu/esp/README.md`
// §Determinism says why that distinction is load-bearing.

import { instantiateEmu, EMU_ABI, OUTCOMES } from "./emulator_wasi.js";

/** The `emu serve` door's slice (`lp-cli/.../serve/board.rs`), in guest µs. */
const SLICE_US = 40_000;
/** The C6 runs at 160 MHz; `memmap::CYCLES_PER_US`. */
const CYCLES_PER_US = 160;
/** How often `stats` goes to the page, in wall ms. */
const STATS_EVERY_MS = 500;
/** The dilation window, in wall ms. */
const DILATION_WINDOW_MS = 1_000;
/** The door's `FLUSH_EVERY`: how often a dirty chip is written back. */
const PERSIST_EVERY_MS = 2_000;
/** Read buffer for each drain. One USB packet is 64 bytes; this is generous. */
const READ_CAP = 1 << 16;
/** How long console text may wait in the worker before it is posted, in wall ms. */
const CONSOLE_EVERY_MS = 100;
/** …or how much of it may wait, whichever comes first. See THE CONSOLE CADENCE. */
const CONSOLE_FLUSH_BYTES = 4096;
/** The chip this board models. */
const FLASH_LEN = 4 * 1024 * 1024;
/**
 * Where persisted chips live, relative to the origin's OPFS root.
 *
 * Exported because Forget deletes a chip from the PAGE — see
 * `removeFlashImage` at the bottom. That is also why
 * this file guards its `self.onmessage` install: it is a Worker entry point
 * and a module the page imports, and importing it must not take the page's
 * message events.
 */
export const FLASH_DIR = "emu-flash";

const decoder = new TextDecoder();

/**
 * A yield that a hidden tab throttles but does not clamp to 4 ms.
 *
 * Built on first use rather than at module scope: the page imports this file
 * for `removeFlashImage`, and an open `MessageChannel` in a context that will
 * never pace anything is a live handle for no reason.
 */
let channel = null;
let tickResolvers = [];
function tick() {
  if (!channel) {
    channel = new MessageChannel();
    channel.port1.onmessage = () => {
      const waiting = tickResolvers;
      tickResolvers = [];
      for (const resolve of waiting) resolve();
    };
  }
  return new Promise((resolve) => {
    tickResolvers.push(resolve);
    channel.port2.postMessage(0);
  });
}

const now = () => performance.now();

// ---- the one machine ----------------------------------------------------

/** @type {ReturnType<typeof instantiateEmu> extends Promise<infer T> ? T : never} */
let emu = null;
let running = false;
let stopped = null;
/** Wall/guest anchors for the pacing loop. */
let origin = 0;
let guestOrigin = 0;
let lastMicros = 0;
/** `[wallMs, guestUs]` samples inside the dilation window. */
let dilationWindow = [];
let lastStatsAt = 0;
let lastPersistAt = 0;
/** The OPFS sync access handle for this board's chip, when it persists. */
let flashHandle = null;
let persistKey = null;
/** What the last `created`/`stats` said, so `listBoards` can be answered. */
let boardMac = null;
/** Console bytes waiting to be posted, in guest order, and when the first arrived. */
let consoleChunks = [];
let consoleBytes = 0;
let consoleSince = 0;

function post(message, transfer) {
  self.postMessage(message, transfer ?? []);
}

/**
 * Report a failure to the page, and — when it belongs to a request — say
 * WHICH request, so the caller's promise can be rejected rather than left
 * hanging.
 *
 * An error posted without an id is how a `signals` line that threw in here
 * became a `setSignals()` that never resolved: esptool-js waited out its own
 * connect timeout instead of seeing the refusal, retried the whole reset
 * dance, and eventually gave up with "Failed to connect with the device"
 * while this worker was healthy and running the guest at 0.47× real time
 * (measured 2026-09-10).
 */
function fail(phase, error, id = null) {
  post({
    type: "error",
    id,
    phase,
    message: error?.message ?? String(error),
    code: error?.code ?? null,
  });
}

// ---- flash persistence (D15) -------------------------------------------
//
// The chip lives in OPFS, in the Worker, behind a **sync access handle** —
// which only a Worker can have, and which is why the image is here rather
// than in the library's `LpFs` mirror: four megabytes per board through an
// in-memory mirror would be four megabytes of page RAM per board and a
// library snapshot that carried flash chips.

async function openFlashFile(key) {
  const root = await navigator.storage.getDirectory();
  const dir = await root.getDirectoryHandle(FLASH_DIR, { create: true });
  const file = await dir.getFileHandle(`${key}.bin`, { create: true });
  return file.createSyncAccessHandle();
}

/** Everything the persisted chip holds, or `null` when there is nothing. */
function readPersisted() {
  if (!flashHandle) return null;
  const size = flashHandle.getSize();
  if (size === 0) return null;
  const bytes = new Uint8Array(size);
  flashHandle.read(bytes, { at: 0 });
  return bytes;
}

/** Write the chip back if the guest (or the host) changed it. */
function persistIfDirty() {
  if (!flashHandle || !emu || !emu.flashDirty()) return false;
  const bytes = emu.flashRead(0, emu.flashLen());
  flashHandle.write(bytes, { at: 0 });
  flashHandle.truncate(bytes.length);
  flashHandle.flush();
  // Only now: a Worker killed between the read and the write must come back
  // to a chip that still knows it was not saved.
  emu.flashMarkSaved();
  return true;
}

// ---- born flashed (D22) -------------------------------------------------
//
// A record is created and the card asks for a board, so the board has to
// arrive with firmware on it. The page hands over the same manifest URL
// esptool's path uses, and the fetch happens HERE so its progress is the
// card's `Downloading` stage rather than a hang.

async function fetchPackage(manifestUrl) {
  post({ type: "download", received: 0, total: 0, what: "manifest" });
  const manifestResponse = await fetch(manifestUrl, { cache: "no-store" });
  if (!manifestResponse.ok) {
    throw new Error(`${manifestUrl} answered ${manifestResponse.status}`);
  }
  const manifest = await manifestResponse.json();
  if (manifest?.schemaVersion !== 2) {
    // Version and refuse, never migrate: the packaged format's own rule.
    throw new Error(
      `${manifestUrl} is schemaVersion ${manifest?.schemaVersion}; this build reads 2`,
    );
  }
  if (manifest?.flash?.format !== "espflash-merged-image") {
    throw new Error(`${manifestUrl}: flash.format is ${manifest?.flash?.format}`);
  }
  const image = manifest.images?.[0];
  if (!image?.path) throw new Error(`${manifestUrl} names no image`);
  const address = Number(image.address ?? manifest.flash.address ?? 0);
  const imageUrl = new URL(image.path, manifestUrl).toString();

  const response = await fetch(imageUrl, { cache: "no-store" });
  if (!response.ok) throw new Error(`${imageUrl} answered ${response.status}`);
  const total = Number(image.sizeBytes ?? response.headers.get("content-length") ?? 0);

  // Streamed so the card's progress is the download's, not a spinner.
  const chunks = [];
  let received = 0;
  const reader = response.body?.getReader();
  if (reader) {
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      chunks.push(value);
      received += value.length;
      post({ type: "download", received, total, what: "image" });
    }
  } else {
    const buffer = new Uint8Array(await response.arrayBuffer());
    chunks.push(buffer);
    received = buffer.length;
    post({ type: "download", received, total: received, what: "image" });
  }
  const bytes = new Uint8Array(received);
  let at = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, at);
    at += chunk.length;
  }
  return { bytes, address };
}

// ---- create -------------------------------------------------------------

async function create(message) {
  if (emu) throw new Error("this worker already holds a board");
  if (!message.jitHostUrl) {
    // Not optional, and not defaulted to a path in this directory. Since M7
    // P7 the module imports `emu_host` and cannot be instantiated without a
    // host; the URL is the content-hashed sidecar copy of M7's `jit-host.js`
    // that `engine-manifest.json` names, and a board created without it is a
    // board that would fail three lines later with the engine's own
    // unreadable link error.
    throw new Error(
      "create: no jitHostUrl — the emulator module imports `emu_host` and needs " +
        "the JS host (engine-manifest.json's `emu_jit_host_js`)",
    );
  }

  // `force-cache`: the URL is content-hashed, so a second Worker on the same
  // page pays nothing and a reload pays nothing.
  const moduleResponse = await fetch(message.moduleUrl, { cache: "force-cache" });
  if (!moduleResponse.ok) {
    throw new Error(`${message.moduleUrl} answered ${moduleResponse.status}`);
  }
  // `compileStreaming` refuses a response that is not `application/wasm`, and
  // a static server that has never heard of the type is a configuration
  // problem rather than a reason not to boot. Falling back costs one extra
  // copy of the bytes and only on servers that need it.
  let module;
  try {
    module = await WebAssembly.compileStreaming(moduleResponse.clone());
  } catch {
    module = await WebAssembly.compile(await moduleResponse.arrayBuffer());
  }
  // The host is attached inside `instantiateEmu`, before `emu_create` — and
  // ONE HOST PER WORKER, because one Worker holds one machine (D10) and the
  // host's table slots belong to that machine's module instance.
  emu = await instantiateEmu(module, { jitHostUrl: message.jitHostUrl });

  // The chip, in priority order: what this board already has, then the
  // package it was born with, then nothing (which IS a blank chip).
  persistKey = message.persistKey ?? null;
  let flash = new Uint8Array(0);
  let source = "blank";
  if (persistKey) {
    flashHandle = await openFlashFile(persistKey);
    const persisted = readPersisted();
    if (persisted) {
      // A board keeps what it has: a record that was flashed once is not
      // re-flashed on every power-on.
      flash = persisted;
      source = "persisted";
    }
  }
  if (flash.length === 0 && message.manifestUrl) {
    try {
      const { bytes, address } = await fetchPackage(message.manifestUrl);
      const chip = new Uint8Array(FLASH_LEN).fill(0xff);
      if (address + bytes.length > chip.length) {
        throw new Error(
          `the image is ${bytes.length} bytes at 0x${address.toString(16)}, past a ${chip.length}-byte chip`,
        );
      }
      chip.set(bytes, address);
      flash = chip;
      source = "package";
    } catch (error) {
      // A failed fetch leaves a blank chip and says so. The card then shows
      // its needs-firmware face and its Flash verb, honestly — which is
      // better than a board that refuses to exist.
      post({ type: "package-failed", message: error?.message ?? String(error) });
    }
  }
  if (flash.length === 0 && message.flashBytes) {
    flash = new Uint8Array(message.flashBytes);
    source = "given";
  }

  emu.create(message.cfg ?? "", flash);
  if (source !== "persisted" && persistKey) {
    // A chip that arrived from somewhere else is not yet saved; the first
    // persist tick writes it.
    persistIfDirty();
  }

  boardMac = readMac(message.cfg ?? "");
  origin = now();
  guestOrigin = 0;
  lastMicros = 0;
  lastStatsAt = origin;
  lastPersistAt = origin;
  resetDilationWindow(origin, 0);
  post({
    type: "created",
    mac: boardMac,
    flash: emu.flashHasImage() ? "loaded" : "blank",
    source,
    abi: EMU_ABI,
    // What `host.attach` proved in THIS engine, before the board existed: the
    // module's function table grew and a funcref in a slot answered a
    // `call_indirect` by index. A board reports it once; `stats` reports what
    // the seam has since carried.
    jitSeam: emu.jitSelftest?.ok === true,
  });
  running = true;
  void pace();
}

function readMac(cfg) {
  const match = /^\s*mac\s*=\s*(\S+)\s*$/m.exec(cfg);
  return match ? match[1] : null;
}

// ---- the loop -----------------------------------------------------------

async function pace() {
  while (running && emu) {
    try {
      await step();
    } catch (error) {
      running = false;
      // Whatever the guest said before it died is still worth reading.
      try {
        flushConsole(true);
      } catch {
        // the failure below is the one to report
      }
      fail("run", error);
      return;
    }
    await tick();
  }
}

async function step() {
  const wallNow = now();
  const wallUs = (wallNow - origin) * 1000;
  const guestUs = Number(emu.micros()) - guestOrigin;
  let deficit = wallUs - guestUs;

  if (deficit > 0 && stopped === null) {
    const budgetUs = Math.min(deficit, SLICE_US);
    const outcome = emu.run(Math.max(1, Math.round(budgetUs * CYCLES_PER_US)));
    if (outcome !== 0) {
      stopped = outcome;
      post({ type: "stopped", outcome, name: OUTCOMES[outcome] ?? String(outcome) });
    }
    drain();

    const micros = Number(emu.micros());
    if (micros < lastMicros) {
      // The chip rebooted inside the slice (`reboot_on_reset`): guest time
      // restarted, so the anchor has to as well or the loop would believe it
      // was hours behind and drop for ever — and the dilation window's
      // samples are now about a machine that no longer exists.
      const at = now();
      reanchor(at, micros);
      resetDilationWindow(at, micros);
      post({ type: "rebooted", reboots: Number(emu.reboots()) });
    }
    lastMicros = micros;

    // Recompute against the wall we are at NOW: the slice itself took time.
    deficit = (now() - origin) * 1000 - (Number(emu.micros()) - guestOrigin);
    if (deficit > SLICE_US) {
      // Behind by more than one slice: drop it. See THE PACING RULE.
      reanchor(now(), Number(emu.micros()));
    }
  } else {
    // Ahead of the wall, or stopped. Either way the guest gets no cycles;
    // the drain still runs so a last line reaches the page.
    drain();
  }

  sample(now());
  maybeReport(now());
  maybePersist(now());
}

function reanchor(wallAt, micros) {
  origin = wallAt;
  guestOrigin = micros;
  // The dilation window is deliberately NOT touched here. A re-anchor is the
  // pacing rule dropping a deficit, and dilation is what reports that it
  // happened: guest microseconds advanced per wall microsecond elapsed, over
  // a window that spans re-anchors. Clearing it here left the window with one
  // sample — and since a guest slower than real time re-anchors on EVERY
  // slice, the number was `null` for ever, which is precisely when it was
  // wanted (measured 2026-09-10: 18 s of guest time and no dilation at all).
}

/// A reboot restarts guest time, so the samples before it describe a machine
/// that no longer exists. This is the one thing that empties the window.
function resetDilationWindow(wallAt, micros) {
  dilationWindow = [[wallAt, micros]];
}

function drain() {
  const bytes = emu.usbRead(READ_CAP);
  if (bytes.length) {
    // Transferred, not copied: these can be a whole upload's worth. And
    // posted NOW — see THE CONSOLE CADENCE for why this channel is not
    // coalesced with the other one.
    post({ type: "usb", bytes: bytes.buffer }, [bytes.buffer]);
  }
  const console0 = emu.uart0Read(READ_CAP);
  if (console0.length) {
    // `uart0Read` hands back a copy (`emulator_wasi.js`'s `withOut`), so
    // holding it past the next slice is safe.
    if (consoleBytes === 0) consoleSince = now();
    consoleChunks.push(console0);
    consoleBytes += console0.length;
  }
  if (
    consoleBytes > 0 &&
    (consoleBytes >= CONSOLE_FLUSH_BYTES || now() - consoleSince >= CONSOLE_EVERY_MS)
  ) {
    flushConsole();
  }
}

/**
 * Post the console text that has accumulated, as ONE message.
 *
 * `final` is the last flush this worker will make (`destroy`, or the loop
 * dying): it drains the streaming decoder too, so a trailing partial
 * character is emitted rather than held for a flush that will never come.
 */
function flushConsole(final = false) {
  if (consoleBytes === 0 && !final) return;
  const all = new Uint8Array(consoleBytes);
  let at = 0;
  for (const chunk of consoleChunks) {
    all.set(chunk, at);
    at += chunk.length;
  }
  consoleChunks = [];
  consoleBytes = 0;
  const text = decoder.decode(all, { stream: !final });
  if (text.length) {
    post({ type: "uart0", text });
  }
}

function sample(wallAt) {
  dilationWindow.push([wallAt, Number(emu.micros())]);
  while (dilationWindow.length > 2 && wallAt - dilationWindow[0][0] > DILATION_WINDOW_MS) {
    dilationWindow.shift();
  }
}

/**
 * Guest microseconds advanced per wall microsecond elapsed, over the window.
 *
 * `null` until there is a window worth reporting: an absent number is
 * dropped rather than guessed (the band's own rule), and a dilation computed
 * over three milliseconds would be noise wearing a decimal point.
 */
function dilation() {
  if (dilationWindow.length < 2) return null;
  const [firstWall, firstGuest] = dilationWindow[0];
  const [lastWall, lastGuest] = dilationWindow[dilationWindow.length - 1];
  const wallUs = (lastWall - firstWall) * 1000;
  if (wallUs < 100_000) return null;
  const guest = lastGuest - firstGuest;
  return guest <= 0 ? 0 : guest / wallUs;
}

/**
 * The translation events this board's host has served, for the `stats` line.
 *
 * **Dilation's witness.** A board's dilation is the host's and the workload's
 * together, and since M7 P7 it is also the CORE's: the wasm build installs a
 * translated core by default, so a number that moved could be the tab being
 * throttled or the machine having stopped interpreting, and those want
 * different answers. `events` says which — one entry per translation event,
 * with the engine's own compile and instantiate milliseconds in it.
 *
 * On every board the tab creates today it stays at zero, because they are all
 * `boot=rom-up` and a rom-up machine installs no translated core (M7's
 * `jit_default.rs` rule 4). That is worth REPORTING rather than assuming: the
 * day a tab board direct-boots, this is the line that says so.
 *
 * Milliseconds here are observations, like dilation. Nothing compares them to
 * anything.
 */
function translation() {
  const events = emu?.host?.events ?? [];
  const last = events[events.length - 1];
  return {
    translationEvents: events.length,
    translationLastMs: last ? last.compileMs + last.instantiateMs : null,
    translationLastBytes: last ? last.bytes : null,
  };
}

function maybeReport(wallAt) {
  if (wallAt - lastStatsAt < STATS_EVERY_MS) return;
  lastStatsAt = wallAt;
  post({
    type: "stats",
    cycles: Number(emu.cycles()),
    micros: Number(emu.micros()),
    reboots: Number(emu.reboots()),
    dilation: dilation(),
    flash: emu.flashHasImage() ? "loaded" : "blank",
    state: stopped === null ? "running" : (OUTCOMES[stopped] ?? "stopped"),
    ...translation(),
  });
}

function maybePersist(wallAt) {
  if (!flashHandle || wallAt - lastPersistAt < PERSIST_EVERY_MS) return;
  lastPersistAt = wallAt;
  try {
    persistIfDirty();
  } catch (error) {
    fail("persist", error);
  }
}

// ---- the inbox ----------------------------------------------------------
//
// Applied between slices and nowhere else, which is the machine's own rule:
// `control_line` and `emu_usb_write` are documented as slice-boundary calls,
// and a Worker satisfies that by construction — a message handler can only
// run when the loop is awaiting its tick.

const inWorker =
  typeof WorkerGlobalScope !== "undefined" && self instanceof WorkerGlobalScope;

/** The inbox handler. Installed only in a Worker — see `FLASH_DIR` above. */
const onMessage = async (event) => {
  const message = event.data ?? {};
  try {
    switch (message.type) {
      case "create":
        await create(message);
        break;

      case "control": {
        if (!emu) throw new Error("no board");
        const line = emu.control(message.line);
        post({ type: "reply", id: message.id, line });
        break;
      }

      case "usb": {
        if (!emu) throw new Error("no board");
        emu.usbWrite(new Uint8Array(message.bytes));
        break;
      }

      case "flash-read": {
        if (!emu) throw new Error("no board");
        const bytes = emu.flashRead(0, emu.flashLen());
        post({ type: "flash", id: message.id, bytes: bytes.buffer }, [bytes.buffer]);
        break;
      }

      case "flash-write": {
        if (!emu) throw new Error("no board");
        const written = emu.flashWrite(message.offset ?? 0, new Uint8Array(message.bytes));
        persistIfDirty();
        post({ type: "flash-written", id: message.id, written });
        break;
      }

      case "flash-erase": {
        if (!emu) throw new Error("no board");
        emu.flashEraseChip();
        persistIfDirty();
        post({ type: "flash-erased", id: message.id });
        break;
      }

      case "flash-state": {
        if (!emu) throw new Error("no board");
        post({
          type: "flash-state",
          id: message.id,
          flash: emu.flashHasImage() ? "loaded" : "blank",
          len: emu.flashLen(),
          dirty: emu.flashDirty(),
        });
        break;
      }

      case "destroy": {
        running = false;
        try {
          // The last of the console goes before the board does, and before
          // `destroyed` — a page that tears the worker down on that reply
          // must have already been handed every byte.
          if (emu) {
            const last = emu.uart0Read(READ_CAP);
            if (last.length) {
              consoleChunks.push(last);
              consoleBytes += last.length;
            }
          }
          flushConsole(true);
          persistIfDirty();
        } finally {
          flashHandle?.close();
          flashHandle = null;
          emu?.destroy();
          emu = null;
        }
        post({ type: "destroyed", id: message.id });
        break;
      }

      default:
        throw new Error(`unknown message \`${message.type}\``);
    }
  } catch (error) {
    // The worker never throws across `postMessage`: a rejected handler would
    // surface as an unhandled rejection in a thread nobody is watching.
    fail(message.type ?? "message", error, message.id ?? null);
  }
};

if (inWorker) {
  self.onmessage = onMessage;
}

/**
 * Remove one board's persisted chip from the store. Answers whether there
 * was a file to remove; **rejects** when there was one and it did not go.
 *
 * Called from the page, which is why it and [`FLASH_DIR`] live in this file
 * rather than beside the caller: one place knows the store's layout, and it
 * is the place that writes to it. The page's Forget verb is
 * `emulator_tab.js`'s `deleteFlash`, which sequences the live worker out of
 * the way first — this is only the removal.
 *
 * IT REPORTS ITS FAILURES. This used to be `removeEntry(...).catch(() => {})`,
 * and a swallowed rejection is how Forget came to delete nothing while
 * answering that it had: OPFS refuses `removeEntry` with
 * `NoModificationAllowedError` while a sync access handle is open on the
 * file, and that refusal went into the void (measured 2026-09-11). A missing
 * file is the one honest no-op — there is nothing left to delete — and it
 * answers `false` rather than throwing.
 */
export async function removeFlashImage(key) {
  const root = await navigator.storage.getDirectory();
  // No `create`: a delete must not bring the store into existence.
  let dir;
  try {
    dir = await root.getDirectoryHandle(FLASH_DIR);
  } catch (error) {
    if (error?.name === "NotFoundError") return false;
    throw error;
  }
  try {
    await dir.removeEntry(`${key}.bin`);
    return true;
  } catch (error) {
    if (error?.name === "NotFoundError") return false;
    throw error;
  }
}
