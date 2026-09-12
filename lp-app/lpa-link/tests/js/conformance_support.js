// The scripted door the CI half of the conformance suite runs against, and
// the one JS entry point the Rust suite reaches through.
//
// PD9 asks for "the JS-conformance suite against a scripted `EmulatorPort`
// double". This scripts one layer LOWER than that — it replaces `fetch` and
// `WebSocket` under the real `EmulatorPort` rather than replacing
// `EmulatorPort` itself — so the code CI exercises is the whole shipped
// stack: `virtual_serial.js`, `emulator_port.js`, `browser_serial.js` and
// `browser_esp32_device_controller.js`. Only the socket is a script. It is
// still hermetic: no server, no firmware, no sockets, no timers.
//
// EVERY module here is loaded over HTTP, from the roots
// `scripts/wasm-serial-test-runner.sh` serves — `/provider/*` is
// `lp-app/lpa-link/src/providers/browser_serial_esp32/` and `/lpa-link/*` is
// `lp-app/lpa-studio-web/public/lpa-link/`. That is deliberate. Letting
// `wasm-bindgen` copy `browser_serial.js` in as a snippet would give the
// suite a SECOND instance of the module-scoped session map that is the thing
// under test, and `browser_esp32_flash.js`'s `import "./browser_serial.js"`
// would reach a third.
//
// The board model here is a faithful subset of
// `lp-emu/esp/lp-emu-esp32c6/src/control.rs`: one command per frame, one
// reply per command, `ok <verb> cyc=… us=…`, and the reset dances DECODED
// from the RTS falling edge and whether DTR was ever high — never
// pattern-matched against a known sequence, exactly as the emulator does it.

const SOCKET_CONNECTING = 0;
const SOCKET_OPEN = 1;
const SOCKET_CLOSED = 3;

// 160 MHz: the cycle→microsecond ratio the control replies carry.
const CYCLES_PER_US = 160;
const CYCLES_PER_COMMAND = 16_000;

let door = null;
/// The tab backing, when the suite is running over Workers. Held so
/// `uninstallShim` can end them: a Worker is not garbage-collected by
/// dropping the port that was talking to it.
let tab = null;
let modules = null;

/// Load the shipped JS once, over HTTP, and hand every wrapper below the same
/// instances. `browser_serial.js` pulls the device controller in itself, from
/// the absolute path it hard-codes.
///
/// ONE AT A TIME, and that is load-bearing. These five used to be a
/// `Promise.all`, and with the whole set in flight at once the runner's static
/// server (`tiny-http`, inside `wasm-bindgen-test-runner`) sometimes never
/// answers one of them: the import promise never settles, the first test hangs
/// with nothing logged, and the harness reports "Failed to detect test as
/// having been run" 20 seconds later. MEASURED 2026-09-09 in headless Firefox
/// 148 on the desk, 10 runs per variant: 0/10 hangs with M2's smaller
/// `virtual_serial.js`, 3/10 with M3's, and 1/8 with M2's file PADDED WITH
/// COMMENTS to M3's byte length — so the trigger is response size and timing,
/// not anything either file says. A traced failure showed three of the five
/// imports started and never finished. Sequential imports remove the
/// concurrency the race needs; the whole set is under 80 KB, so the cost is
/// nothing a test suite can measure.
///
/// The `tiny-http` behaviour is unfixed and lives upstream in
/// `wasm-bindgen-test-runner`; it is size-sensitive and it will bite again as
/// the served JS grows. So: **adding a sixth module here means keeping this
/// loader serial** — do not restore the `Promise.all`, and do not parallelise
/// "just the new one". (Emulator plan two, DD24; the residue is recorded in
/// `docs/debt/web-serial-js-untestable.md`'s incident log, not a new debt
/// entry — a third-party harness behaviour with a working mitigation is below
/// the filing bar.)
async function load() {
  modules ??= (async () => {
    const serial = await import("/provider/browser_serial.js");
    const flash = await import("/provider/browser_esp32_flash.js");
    const shim = await import("/lpa-link/virtual_serial.js");
    const port = await import("/lpa-link/emulator_port.js");
    const controller = await import("/lpa-link/browser_esp32_device_controller.js");
    return { serial, flash, shim, port, controller };
  })();
  return modules;
}

async function polyfill() {
  return (await load()).shim;
}

class ScriptedBoard {
  constructor(id, index) {
    this.id = id;
    this.mac = `70:04:1d:ad:35:${(0x10 + index).toString(16).padStart(2, "0")}`;
    this.cycle = 160_000;
    this.reboots = 0;
    this.host = "attached";
    this.sof = "on";
    this.draining = false;
    this.dtr = false;
    this.rts = false;
    this.dtrEverHigh = false;
    this.controlLog = [];
    // Host → device bytes, as they arrived on the byte channel.
    this.received = [];
    this.control = null;
    this.bytes = null;
    // What the board says the moment an application opens the port — the
    // door's replay of a boot console to its first byte client.
    this.greeting = `M! {"hello":"${id}"}\n`;
  }

  record() {
    return {
      id: this.id,
      mac: this.mac,
      chip: "esp32c6",
      bytes: `/board/${this.id}/bytes`,
      control: `/board/${this.id}/control`,
      flash: "loaded",
      link: "usb-serial-jtag",
      state: "running",
      reboots: this.reboots,
    };
  }

  micros() {
    return Math.floor(this.cycle / CYCLES_PER_US);
  }

  // A reboot puts the machine back at power-on: the cycle counter restarts,
  // which is the only thing about it a control client can see. `emu serve`
  // holds `--reboot-on-reset` on, and the loopback byte client survives it —
  // so the byte socket stays up here too.
  reboot() {
    this.cycle = 0;
    this.reboots += 1;
    this.dtrEverHigh = false;
  }

  command(line) {
    this.controlLog.push(line);
    this.cycle += CYCLES_PER_COMMAND;
    const words = line.trim().split(/\s+/);
    const verb = words[0];
    const applied = (name) => `ok ${name} cyc=${this.cycle} us=${this.micros()}`;
    switch (verb) {
      case "attach":
        this.host = "attached";
        this.sof = "on";
        return applied("attach");
      case "detach":
        this.host = "absent";
        this.sof = "off";
        return applied("detach");
      case "dtr":
        this.setLines({ dtr: words[1] === "1" });
        return applied("dtr");
      case "rts":
        this.setLines({ rts: words[1] === "1" });
        return applied("rts");
      case "signals": {
        const lines = {};
        for (const word of words.slice(1)) {
          const [name, value] = word.split("=");
          lines[name] = value === "1";
        }
        this.setLines(lines);
        return applied("signals");
      }
      case "reset": {
        const reply = applied("reset");
        this.reboot();
        return reply;
      }
      case "download-mode": {
        const reply = applied("download-mode");
        this.reboot();
        return reply;
      }
      case "state":
        return (
          `ok state cyc=${this.cycle} us=${this.micros()} host=${this.host} ` +
          `draining=${this.draining} sof=${this.sof} in_pending=0 out_queued=0`
        );
      case "pins":
        return `ok pins cyc=${this.cycle} us=${this.micros()} pads=1 gpio18[route=- ie=1 drv=- lvl=0 wire=-]`;
      default:
        return `err unknown command \`${verb}\``;
    }
  }

  // The dances are decoded, never matched: an RTS falling edge resets the
  // chip, and it lands in the download console instead when DTR was seen
  // high since the last edge.
  setLines({ dtr, rts }) {
    const before = this.rts;
    if (dtr !== undefined) {
      this.dtr = dtr;
      this.dtrEverHigh = this.dtrEverHigh || dtr;
    }
    if (rts !== undefined) {
      this.rts = rts;
    }
    if (before && !this.rts) {
      this.reboot();
    }
  }
}

class ScriptedDoor {
  constructor(boardIds) {
    this.boards = boardIds.map((id, index) => new ScriptedBoard(id, index));
  }

  board(id) {
    return this.boards.find((board) => board.id === id) ?? null;
  }

  fetch(url) {
    const path = new URL(String(url)).pathname;
    if (path !== "/boards") {
      return Promise.resolve({ ok: false, status: 404, json: async () => ({}) });
    }
    return Promise.resolve({
      ok: true,
      status: 200,
      json: async () => ({ boards: this.boards.map((board) => board.record()) }),
    });
  }
}

// A `WebSocket` with the same six members `emulator_port.js` uses. It is not
// a subclass: the point is that `EmulatorPort` reaches for nothing beyond the
// documented surface, and a double that had to subclass would prove less.
class ScriptedSocket {
  constructor(url) {
    this.url = String(url);
    this.readyState = SOCKET_CONNECTING;
    this.binaryType = "blob";
    this.listeners = new Map();
    const match = /\/board\/([^/]+)\/(bytes|control)$/.exec(new URL(this.url).pathname);
    this.board = match ? door?.board(decodeURIComponent(match[1])) : null;
    this.channel = match ? match[2] : null;
    queueMicrotask(() => this.settle());
  }

  settle() {
    const held = this.board && this.board[this.channel] !== null;
    if (!this.board || held) {
      // Unknown board (404) or a second client on an endpoint already held
      // (409): refused, never multiplexed — one application per port.
      this.readyState = SOCKET_CLOSED;
      this.dispatch("error", {});
      this.dispatch("close", {});
      return;
    }
    this.board[this.channel] = this;
    this.readyState = SOCKET_OPEN;
    this.dispatch("open", {});
    if (this.channel === "bytes" && this.board.greeting) {
      this.deliver(new TextEncoder().encode(this.board.greeting));
    }
  }

  addEventListener(type, callback, options = {}) {
    let set = this.listeners.get(type);
    if (!set) {
      set = new Set();
      this.listeners.set(type, set);
    }
    const entry = options.once ? { callback, once: true } : { callback, once: false };
    set.add(entry);
  }

  dispatch(type, event) {
    for (const entry of [...(this.listeners.get(type) ?? [])]) {
      if (entry.once) {
        this.listeners.get(type).delete(entry);
      }
      entry.callback(event);
    }
  }

  send(data) {
    if (this.readyState !== SOCKET_OPEN) {
      throw new Error("scripted socket: send on a closed socket");
    }
    if (this.channel === "control") {
      const reply = this.board.command(String(data));
      queueMicrotask(() => this.dispatch("message", { data: reply }));
      return;
    }
    this.board.draining = true;
    this.board.received.push(
      typeof data === "string" ? data : new TextDecoder().decode(new Uint8Array(data)),
    );
  }

  deliver(bytes) {
    queueMicrotask(() => this.dispatch("message", { data: bytes.buffer }));
  }

  close() {
    if (this.readyState === SOCKET_CLOSED) {
      return;
    }
    this.readyState = SOCKET_CLOSED;
    if (this.board && this.board[this.channel] === this) {
      this.board[this.channel] = null;
      if (this.channel === "bytes") {
        this.board.draining = false;
      }
    }
    queueMicrotask(() => this.dispatch("close", {}));
  }
}

// --- what the Rust suite drives -------------------------------------------

/// How many reboots the SHIM noticed per board — `EmulatorPort`'s `reboot`
/// event, which is fired when a control reply's guest cycle goes backwards
/// and by the explicit `reset`/`download-mode` verbs.
///
/// M5 rules that a chip reset does not re-enumerate, and a test that only
/// asserted "the port object survived" would also pass on a shim that read no
/// control reply at all. This is the other half of that claim.
const rebootsNoticedByBoard = new Map();

export function rebootsNoticed(boardId) {
  return rebootsNoticedByBoard.get(boardId) ?? 0;
}

async function watchReboots() {
  const { bus } = await polyfill();
  rebootsNoticedByBoard.clear();
  for (const port of bus()?.generations ?? []) {
    const id = port.boardId;
    if (rebootsNoticedByBoard.has(id)) {
      continue;
    }
    rebootsNoticedByBoard.set(id, 0);
    port.emulator.on("reboot", () =>
      rebootsNoticedByBoard.set(id, rebootsNoticedByBoard.get(id) + 1),
    );
  }
}

/// Install the polyfill over a scripted door holding `boardIds`.
export async function installScripted(boardIds) {
  door = new ScriptedDoor(boardIds);
  const { install } = await polyfill();
  await install("http://scripted.emu.invalid/", {
    backing: nativeBackingOverScript(),
  });
  await watchReboots();
}

function nativeBackingOverScript() {
  // Built here rather than imported so the transport injection is visible at
  // the one place a test could otherwise be fooled about what it is running.
  return {
    describe: () => "scripted door",
    listBoards: async () => (await (await door.fetch("http://scripted.emu.invalid/boards")).json()).boards,
    connect: async (boardId, board) => {
      const { EmulatorPort } = (await load()).port;
      return EmulatorPort.connect("http://scripted.emu.invalid/", boardId, board, {
        WebSocketImpl: ScriptedSocket,
        fetchImpl: (url) => door.fetch(url),
      });
    },
  };
}

/// Install the polyfill over boards hosted **in this tab** — one Worker per
/// board, each holding the emulator's own wasm (`?emu=tab`'s backing).
///
/// The third backing, and the third way to run the SAME assertions: scripted
/// (CI), live over a socket, and now in-page over `postMessage`. A claim that
/// fails only here is a real divergence between the door's coupling and the
/// tab's — a finding, not something to patch around.
///
/// Boards are synthesized from the ids the suite asked for so the assertions
/// keep their own names. Each gets its own MAC and its own Worker, because
/// the module holds one machine.
export async function installTab(moduleUrl, boardIds) {
  door = null;
  const { install } = await polyfill();
  const { tabBacking } = await import("/lpa-link/emulator_tab.js");
  const boards = boardIds.map((id, index) => {
    const mac = `02:c6:7a:b0:00:${(index + 1).toString(16).padStart(2, "0")}`;
    return {
      id,
      mac,
      chip: "esp32c6",
      boot: "rom-up",
      link: "usb-serial-jtag",
      cfg: ["boot=rom-up", "strap=app", "usb_host=attached", `mac=${mac}`, ""].join("\n"),
    };
  });
  // The JS host, beside the module. `wasm-serial-test-runner.sh` stages the
  // pair into the runner's served root under fixed names — unlike Studio,
  // which takes both from `engine-manifest.json` under content-hashed ones —
  // so here the host is the module's sibling and is resolved as one rather
  // than passed in through the Rust harness.
  const jitHostUrl = new URL("jit-host.js", new URL(moduleUrl, location.href)).toString();
  tab = tabBacking({ moduleUrl, jitHostUrl, boards });
  await install("http://tab.emu.invalid/", { backing: tab });
  await watchReboots();
}

/// Install the polyfill over a REAL `lp-cli emu serve` at `baseUrl` — the
/// live half of the suite, run by a `just` recipe and never in CI.
export async function installLive(baseUrl, boardIds) {
  door = null;
  const { install } = await polyfill();
  // The boards are named rather than listed: `GET /boards` is cross-origin
  // from this page and the door sends no `Access-Control-Allow-Origin`, so
  // the registry is unreadable from a browser today (see `virtual_serial.js`
  // `list()`). Naming them exercises everything else.
  await install(baseUrl, { boards: boardIds });
  await watchReboots();
}

export async function uninstallShim() {
  const { uninstall } = await polyfill();
  await uninstall();
  door = null;
  // A Worker outlives the bus that was holding its port, and a suite that
  // left one per install would end with a thread per test.
  if (tab) {
    await tab.dispose().catch(() => {});
    tab = null;
  }
}

/// Board ids the installed bus is holding, in enumeration order.
export async function busBoardIds() {
  const { bus } = await polyfill();
  return (bus()?.boardIds ?? []).join(",");
}

/// Every control line the door received for a board, in order, newline-joined.
export function controlLog(boardId) {
  return (door?.board(boardId)?.controlLog ?? []).join("\n");
}

/// Everything the host wrote at a board, newline-joined.
export function receivedBytes(boardId) {
  return (door?.board(boardId)?.received ?? []).join("");
}

export function rebootCount(boardId) {
  return door?.board(boardId)?.reboots ?? -1;
}

/// Push bytes at an open port, as a board writing to its console does.
export function deliverBytes(boardId, text) {
  const board = door?.board(boardId);
  if (!board?.bytes) {
    throw new Error(`scripted door: board ${boardId} has no open byte channel`);
  }
  board.bytes.deliver(new TextEncoder().encode(text));
}

/// The device goes away under an open port: the byte channel drops from the
/// far side. This is the read-pump error path, and nothing about it is a
/// timeout.
export function dropByteChannel(boardId) {
  const board = door?.board(boardId);
  if (!board?.bytes) {
    throw new Error(`scripted door: board ${boardId} has no open byte channel`);
  }
  board.bytes.close();
}

/// Reset a board over the CONTROL CHANNEL, the way M3's dev controls and the
/// emulator's own `reset` verb do. Returns the port object that was live
/// before, so a test can assert whether identity moved.
export async function resetOverControlChannel(boardId) {
  const { bus } = await polyfill();
  const port = bus().livePortFor(boardId);
  await port.emulator.reset();
  return port;
}

/// Pull the cable out and put it back — the one thing on this bus that makes
/// a board enumerate again (M5's ruling: a chip reset does not). Returns the
/// port object that was live before the replug.
export async function replugOverTheCable(boardId) {
  const { bus } = await polyfill();
  const port = bus().livePortFor(boardId);
  await bus().detach(boardId);
  await bus().attach(boardId);
  return port;
}

/// Ask the door to reboot a board with no verb from us at all — a reboot the
/// emulator decided on, which this backing learns about only from the guest
/// cycle in the next control reply going backwards.
export function rebootBehindOurBack(boardId) {
  door.board(boardId).reboot();
}

/// One control round trip on a board, so a reboot behind our back has a reply
/// to be noticed in. The test sends this, never the polyfill.
export async function pokeControlChannel(boardId) {
  const { bus } = await polyfill();
  await bus().livePortFor(boardId).emulator.state();
}

export async function livePortFor(boardId) {
  const { bus } = await polyfill();
  return bus().livePortFor(boardId);
}

export async function portIsDead(port) {
  return Boolean(port?.dead);
}

export function portReadableIsNull(port) {
  return port.readable === null;
}

export function portWritableIsNull(port) {
  return port.writable === null;
}

export function portInfoJson(port) {
  const info = port.getInfo();
  return JSON.stringify({
    usbVendorId: info.usbVendorId,
    usbProductId: info.usbProductId,
    vendorHex: `0x${info.usbVendorId.toString(16)}`,
    productHex: `0x${info.usbProductId.toString(16)}`,
  });
}

/// `labelForPort` from the REAL controller module — the same function the
/// session descriptors are built with.
export async function labelForPortOf(port) {
  return (await load()).controller.labelForPort(port);
}

// --- the shipped `browser_serial.js`, wrapped one call per export ----------
//
// Thin on purpose: each of these is the shipped function and nothing else, so
// what the Rust suite asserts about is `browser_serial.js`'s behaviour rather
// than a helper's. The Rust side cannot declare these as `wasm_bindgen`
// externs directly — `browser_serial.rs` already declares the same
// (module, name) pairs for the library, and two identical declarations in one
// binary are a duplicate-symbol link error.

export async function serialIsSupported() {
  return (await load()).serial.isSupported();
}

export async function flashBridgeIsSupported() {
  return (await load()).flash.isSupported();
}

export async function installSerialEvents(onConnect, onDisconnect) {
  return (await load()).serial.installSerialEvents(onConnect, onDisconnect);
}

export async function getGrantedPorts() {
  return (await load()).serial.getGrantedPorts();
}

export async function requestPortSession() {
  return (await load()).serial.requestPort();
}

export async function openPort(id, baudRate, reset, resetKind) {
  return (await load()).serial.openPort(id, baudRate, reset, resetKind);
}

export async function closePort(id) {
  return (await load()).serial.closePort(id);
}

export async function forgetPort(id) {
  return (await load()).serial.forgetPort(id);
}

export async function releasePort(id) {
  return (await load()).serial.releasePort(id);
}

export async function getPortObject(id) {
  return (await load()).serial.getPort(id);
}

export async function takeLines(id) {
  return (await load()).serial.takeLines(id);
}

export async function takeErrors(id) {
  return (await load()).serial.takeErrors(id);
}

export async function writeLine(id, line) {
  return (await load()).serial.writeLine(id, line);
}

// --- DD9: does an own property shadow Chromium's prototype getter? ---------

/// What this browser says about `navigator.serial` right now. Called before
/// install, while installed, and after uninstall; M3's install seam rests on
/// the answer.
export async function serialShadowingReport() {
  const nav = globalThis.navigator;
  const prototypeDescriptor = Object.getOwnPropertyDescriptor(
    Object.getPrototypeOf(nav),
    "serial",
  );
  const own = Object.getOwnPropertyDescriptor(nav, "serial");
  const { bus } = await polyfill();
  return JSON.stringify({
    prototypeHasGetter: typeof prototypeDescriptor?.get === "function",
    hasOwnProperty: own !== undefined,
    ownIsConfigurable: own?.configurable ?? null,
    serialIsUndefined: nav.serial === undefined,
    serialIsTheShim: bus() !== null && nav.serial === bus(),
    serialConstructor: nav.serial?.constructor?.name ?? null,
  });
}

/// The one call that has to work in real Chrome for M3 to have an install
/// seam at all, exercised standalone against a sentinel and taken straight
/// back off again.
export function canShadowNavigatorSerial() {
  const nav = globalThis.navigator;
  const before = Object.getOwnPropertyDescriptor(nav, "serial");
  const sentinel = { sentinel: true };
  Object.defineProperty(nav, "serial", { value: sentinel, configurable: true });
  const shadowed = nav.serial === sentinel;
  if (before) {
    Object.defineProperty(nav, "serial", before);
  } else {
    delete nav.serial;
  }
  const removed = nav.serial !== sentinel;
  return JSON.stringify({ shadowed, removedAgain: removed });
}
