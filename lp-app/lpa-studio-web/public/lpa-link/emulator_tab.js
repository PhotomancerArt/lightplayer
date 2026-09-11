// The tab backing: one Worker, presented as the thing `EmulatorPort` already
// knows how to hold.
//
// `emulator_port.js` reaches its two channels through exactly one seam — the
// `transport.WebSocketImpl` / `fetchImpl` pair it is constructed with — and
// the conformance suite already proves a WebSocket-SHAPED double works there
// (its `ScriptedSocket` is one). So the wasm backing needs no fork of that
// file and no socket code in it: this file supplies two channel objects with
// `readyState`, `send`, `close` and `addEventListener`, and everything above
// them runs unchanged, including the flasher, the device controller and the
// Rust provider over them.
//
// THE COUPLING RULE, REPRODUCED. On the native door a byte client's connect
// IS the machine's `open` and its disconnect IS its `close`, which is why
// `EmulatorPort.open()` sends no verb. A Worker has no connect, so the bytes
// channel applies `open` before it fires its `open` event and `close` before
// its `close` — the rule expressed rather than a second rule invented. Get
// this wrong and `open()` answers `err open: the port is already open`, or
// worse, a board drains while nobody is reading.
//
// ONE WORKER PER PAGE (D10). `SESSION_CAPACITY = 1` upstream means one live
// board anyway; the hub below holds it and `dispose()` on the last port ends
// it.
//
// WHAT STAYS REFUSED. `snapshot()` is still refused, with the same
// `NotSupportedError` name and a message that names this backing: there is
// no bytes format for a `Snapshot` (its own module says in-memory only, on
// purpose), and a lie inside the contract is worse than a gap.

import { EmulatorPort } from "./emulator_port.js";
import { deleteFlash } from "./emulator_worker.js";

export { deleteFlash };

/** The synthetic origin the port builds its channel URLs from. */
const TAB_ORIGIN = "http://tab.emu.invalid/";

/** `WebSocket.OPEN` / `CLOSED`, spelled out as `emulator_port.js` does. */
const SOCKET_OPEN = 1;
const SOCKET_CLOSED = 3;

/** D20's one board for mode B: blank, rom-up, one fixed identity. */
export const TAB_BOARD = {
  id: "tab-c6",
  mac: "02:c6:7a:b0:00:01",
  chip: "esp32c6",
  boot: "rom-up",
  link: "usb-serial-jtag",
  // `usb_host=attached` is the CLI's own word: the cable in with the port
  // OPEN from power-on. It has to be, and not the closed `attached-idle`: a
  // reset returns the USB block to its power-on state, so a board whose port
  // started closed comes back from every reset with nothing draining — and
  // the host's reset dance is the FIRST thing a flasher does. Measured
  // 2026-09-10: with the port closed at power-on, esptool-js's `connect()`
  // failed on every attempt because the ROM's download console was writing
  // to a port nobody was reading.
  cfg: [
    "boot=rom-up",
    "strap=app",
    "usb_host=attached",
    "mac=02:c6:7a:b0:00:01",
    "",
  ].join("\n"),
};

/**
 * One Worker, and the bookkeeping the two channels share.
 *
 * Every ABI call is a message with an id, because a reply has to find the
 * caller that asked for it; bytes and console lines are unsolicited and go
 * to whoever is listening.
 */
class TabHub {
  constructor(moduleUrl, board) {
    this.moduleUrl = moduleUrl;
    this.board = board;
    this.worker = null;
    this.nextId = 1;
    this.pending = new Map();
    this.byteListeners = new Set();
    this.consoleListeners = new Set();
    this.statsListeners = new Set();
    /** The live registry row, kept current by `created` and `stats`. */
    this.row = { ...board, flash: "blank", state: "starting", reboots: 0, dilation: null };
    this.ready = null;
    this.lastError = null;
  }

  start() {
    if (this.ready) return this.ready;
    this.ready = new Promise((resolve, reject) => {
      this.worker = new Worker(new URL("./emulator_worker.js", import.meta.url), {
        type: "module",
        name: `emu-${this.board.id}`,
      });
      this.worker.onmessage = (event) => this._onMessage(event.data ?? {}, resolve, reject);
      this.worker.onerror = (event) => {
        const error = new Error(`the emulator worker failed: ${event.message ?? event.type}`);
        this.lastError = error;
        reject(error);
      };
      this.worker.postMessage({
        type: "create",
        moduleUrl: this.moduleUrl,
        cfg: this.board.cfg ?? "",
        persistKey: this.board.persistKey ?? null,
        manifestUrl: this.board.manifestUrl ?? null,
      });
    });
    return this.ready;
  }

  _onMessage(message, resolveReady, rejectReady) {
    switch (message.type) {
      case "created":
        this.row.mac = message.mac ?? this.row.mac;
        this.row.flash = message.flash;
        this.row.state = "running";
        resolveReady(this.row);
        return;

      case "stats":
        this.row.flash = message.flash;
        this.row.state = message.state;
        this.row.reboots = message.reboots;
        this.row.dilation = message.dilation;
        this.row.cycles = message.cycles;
        this.row.micros = message.micros;
        for (const listener of this.statsListeners) listener(message);
        return;

      case "usb": {
        const bytes = new Uint8Array(message.bytes);
        for (const listener of this.byteListeners) listener(bytes);
        return;
      }

      case "uart0":
        for (const listener of this.consoleListeners) listener(message.text);
        return;

      case "package-failed":
        // Not fatal: the board exists with a blank chip, and the card's
        // needs-firmware face and its Flash verb are the honest next step.
        console.warn(`[emu-tab] the firmware package did not download: ${message.message}`);
        this.row.packageError = message.message;
        return;

      case "rebooted":
        this.row.reboots = message.reboots;
        return;

      case "stopped":
        this.row.state = message.name ?? "stopped";
        return;

      case "error": {
        const error = new Error(`${message.phase}: ${message.message}`);
        this.lastError = error;
        // A failure with an id belongs to its caller. Handing it back is what
        // turns a refusal into a rejected promise instead of a promise that
        // never settles — and a control line that never settles is a
        // `setSignals()` that never returns, which is how a flasher hangs.
        if (message.id != null && this.pending.has(message.id)) {
          this.pending.get(message.id).reject(error);
          this.pending.delete(message.id);
          return;
        }
        console.error(`[emu-tab] ${error.message}`);
        // Only a failure before the board exists can still be the `create`
        // that everyone is waiting on; afterwards this is the pacing loop's,
        // and there is nobody to hand it to but the console.
        if (this.row.state === "starting") rejectReady(error);
        return;
      }

      default: {
        const waiting = message.id != null ? this.pending.get(message.id) : null;
        if (waiting) {
          this.pending.delete(message.id);
          waiting.resolve(message);
        }
      }
    }
  }

  /** Send a message that expects one reply, and wait for it. */
  request(message, transfer) {
    const id = this.nextId++;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.worker.postMessage({ ...message, id }, transfer ?? []);
    });
  }

  /** Fire and forget — bytes on the wire have no reply. */
  send(message, transfer) {
    this.worker.postMessage(message, transfer ?? []);
  }

  async control(line) {
    const reply = await this.request({ type: "control", line });
    return reply.line;
  }

  onBytes(listener) {
    this.byteListeners.add(listener);
    return () => this.byteListeners.delete(listener);
  }

  onConsole(listener) {
    this.consoleListeners.add(listener);
    return () => this.consoleListeners.delete(listener);
  }

  onStats(listener) {
    this.statsListeners.add(listener);
    return () => this.statsListeners.delete(listener);
  }

  async destroy() {
    if (!this.worker) return;
    try {
      await this.request({ type: "destroy" });
    } catch {
      // A worker that cannot answer is a worker that is going away anyway.
    }
    this.worker.terminate();
    this.worker = null;
    this.ready = null;
  }
}

/**
 * A WebSocket-shaped view of one worker channel.
 *
 * Built per hub so the class the port constructs closes over the right
 * Worker. The two channels differ in what `send` means and in whether
 * opening applies a control verb — see the coupling rule in this file's
 * header.
 */
function channelClassFor(hub) {
  return class TabChannelSocket {
    constructor(url) {
      this.url = String(url);
      this.channel = this.url.endsWith("/control") ? "control" : "bytes";
      this.readyState = 0;
      this.binaryType = "arraybuffer";
      this._listeners = new Map();
      this._unsubscribe = null;
      // Asynchronous, like a real socket's connect: the port registers its
      // listeners after construction returns, so firing `open` inline would
      // fire it at nobody.
      queueMicrotask(() => void this._open());
    }

    async _open() {
      try {
        await hub.start();
        if (this.channel === "bytes") {
          // The coupling rule: a byte client arriving IS the machine's
          // `open`. The port sends no verb, so this does.
          //
          // An `err` here is NOT fatal, and the door proves it: its own
          // coupling logs the refusal and carries on, because a board whose
          // port is open from power-on (`usb_host=attached`, the CLI's
          // default and this backing's) answers `err open: the port is
          // already open` on the very first client. Throwing here would make
          // every board that keeps its boot log unusable.
          const reply = await hub.control("open");
          if (reply.startsWith("err")) {
            console.debug(`[emu-tab] open: ${reply}`);
          }
          this._unsubscribe = hub.onBytes((bytes) => {
            this._fire("message", { data: bytes.buffer });
          });
        }
        this.readyState = SOCKET_OPEN;
        this._fire("open", {});
      } catch (error) {
        this.readyState = SOCKET_CLOSED;
        this._fire("error", { error });
      }
    }

    send(data) {
      if (this.readyState !== SOCKET_OPEN) {
        throw new Error(`the ${this.channel} channel is not open`);
      }
      if (this.channel === "control") {
        hub
          .control(String(data))
          .then((line) => this._fire("message", { data: line }))
          .catch((error) => this._fire("error", { error }));
        return;
      }
      const bytes = data instanceof Uint8Array ? data : new Uint8Array(data);
      // Copied before transfer: the caller (esptool-js, the device
      // controller) may still hold its own view of that buffer.
      const owned = bytes.slice();
      hub.send({ type: "usb", bytes: owned.buffer }, [owned.buffer]);
    }

    close() {
      if (this.readyState === SOCKET_CLOSED) return;
      this.readyState = SOCKET_CLOSED;
      this._unsubscribe?.();
      this._unsubscribe = null;
      if (this.channel === "bytes") {
        // …and a byte client leaving IS the machine's `close`.
        hub.control("close").catch(() => {});
      }
      this._fire("close", {});
    }

    addEventListener(type, callback) {
      let set = this._listeners.get(type);
      if (!set) {
        set = new Set();
        this._listeners.set(type, set);
      }
      set.add(callback);
    }

    removeEventListener(type, callback) {
      this._listeners.get(type)?.delete(callback);
    }

    _fire(type, detail) {
      for (const callback of this._listeners.get(type) ?? []) {
        try {
          callback({ type, ...detail });
        } catch (error) {
          console.warn(`[emu-tab] ${type} listener failed`, error);
        }
      }
    }
  };
}

/**
 * The port a tab-hosted board presents.
 *
 * Four of `EmulatorPort`'s refusals become answers here, because a Worker
 * holding the chip can answer them; `snapshot()` is not one of them.
 */
export class TabEmulatorPort extends EmulatorPort {
  static async connectTab(hub, boardId, board) {
    const port = new TabEmulatorPort(TAB_ORIGIN, boardId, board, {
      WebSocketImpl: channelClassFor(hub),
    });
    port._hub = hub;
    await port._openControl();
    return port;
  }

  /** The whole chip. */
  async getFlash() {
    const reply = await this._hub.request({ type: "flash-read" });
    return new Uint8Array(reply.bytes);
  }

  /** Replace the whole chip: erase, then write at zero. */
  async putFlash(bytes) {
    const data = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes);
    await this._hub.request({ type: "flash-erase" });
    const owned = data.slice();
    await this._hub.request({ type: "flash-write", offset: 0, bytes: owned.buffer }, [
      owned.buffer,
    ]);
    return data.length;
  }

  /** `state` and `pins` as the door answers them, plus what only a tab has. */
  async probes() {
    const [state, pins] = await Promise.all([this.state(), this.pins()]);
    return {
      ...state,
      pins: pins.pins,
      dilation: this._hub.row.dilation,
      reboots: this._hub.row.reboots,
    };
  }

  /** The live word, not the one the registry carried at connect time. */
  flashState() {
    return this._hub.row.flash ?? null;
  }

  /** The console, for a page that wants to show it. */
  onConsole(callback) {
    return this._hub.onConsole(callback);
  }

  /** Dilation and the counters, every half second. */
  onStats(callback) {
    return this._hub.onStats(callback);
  }

  snapshot() {
    const error = new Error(
      "snapshot: not available on the tab backing (a Snapshot is in-memory only — " +
        "there is no bytes format to hand over)",
    );
    error.name = "NotSupportedError";
    return Promise.reject(error);
  }
}

/**
 * A `{ describe, listBoards, connect }` backing over boards hosted in this
 * tab — the same triple `nativeBacking` answers, and what `createBus` takes.
 */
export function tabBacking({ moduleUrl, boards = [TAB_BOARD] }) {
  const hubs = new Map();
  const hubFor = (board) => {
    let hub = hubs.get(board.id);
    if (!hub) {
      hub = new TabHub(moduleUrl, board);
      hubs.set(board.id, hub);
    }
    return hub;
  };

  return {
    describe: () => "in this tab",

    async listBoards() {
      // Every board is reported before it is connected, the way the door's
      // registry is: a board that has never been powered on still has an id,
      // a MAC and a blank chip.
      return boards.map((board) => {
        const hub = hubs.get(board.id);
        const row = hub?.row ?? { ...board, flash: "blank", state: "idle", reboots: 0 };
        return {
          id: board.id,
          mac: row.mac ?? board.mac,
          chip: board.chip ?? "esp32c6",
          bytes: `/board/${board.id}/bytes`,
          control: `/board/${board.id}/control`,
          flash: row.flash ?? "blank",
          boot: board.boot ?? "rom-up",
          link: board.link ?? "usb-serial-jtag",
          state: row.state ?? "idle",
          reboots: row.reboots ?? 0,
          // Not in `GET /boards` — the door has nothing to say here. It is on
          // the row because it is the tab's own honest fact, and because the
          // walk and anyone at a console read this shape.
          dilation: row.dilation ?? null,
        };
      });
    },

    async connect(boardId, row) {
      const board = boards.find((candidate) => candidate.id === boardId);
      if (!board) throw new Error(`no board \`${boardId}\` in this tab`);
      const hub = hubFor(board);
      await hub.start();
      return TabEmulatorPort.connectTab(hub, boardId, row ?? hub.row);
    },

    /** Every worker this backing started. The page calls it on `pagehide`. */
    async dispose() {
      for (const hub of hubs.values()) {
        await hub.destroy();
      }
      hubs.clear();
    },
  };
}
