// `EmulatorPort` — one emulated board, as an in-page object.
//
// This is the object the `navigator.serial` polyfill (`virtual_serial.js`)
// sits on, and it is deliberately NOT a Web Serial thing: it is the board's
// two channels — bytes and control — plus the flash/snapshot/probe getters
// the sibling effort's eight-point contract asks for. The polyfill turns it
// into a `SerialPort`; a `ByteStreamLink` could turn the same object into a
// device card without either side learning about the other.
//
// TWO BACKINGS, ONE SHAPE. This file ships the **native** backing only:
// `lp-cli emu serve`'s WebSocket door (`GET /boards`, `ws /board/<id>/bytes`,
// `ws /board/<id>/control`). A wasm backing — the sibling's mode A — drops in
// behind the same methods, which is why every method here takes and returns
// plain data and why nothing outside this file touches a WebSocket.
//
// WHERE THE NATIVE BACKING CANNOT ANSWER IT SAYS SO. `getFlash`, `putFlash`,
// `snapshot` and `probes` reject with "not available on the native backing"
// rather than returning zeros: the door has no route for them, and a lie
// inside the contract is worse than a gap.
//
// THE SHIM TRANSLATES NOTHING. `signals()` writes the DTR/RTS lines onto the
// control channel exactly as it was given them. The reset dances are DECODED
// by the emulator (an RTS falling edge, and whether DTR was ever high — see
// `lp-emu/esp/README.md`, "What plan two's shim maps onto this"), so nothing
// here pattern-matches a dance, and nothing here sends a verb Studio did not
// ask for.

// `WebSocket.OPEN` / `WebSocket.CLOSED`, spelled out: the readyState codes
// are the protocol's, not one implementation's, and a scripted door reports
// the same numbers without having to subclass `WebSocket`.
const SOCKET_OPEN = 1;
const SOCKET_CLOSED = 3;

/// `GET /boards` — the registry, in the order the server was given them.
export async function listBoards(baseUrl, { fetchImpl = null } = {}) {
  const url = new URL("boards", baseWithSlash(baseUrl));
  const get = fetchImpl ?? globalThis.fetch.bind(globalThis);
  const response = await get(url, { cache: "no-store" });
  if (!response.ok) {
    throw new Error(`emu serve: GET ${url} answered ${response.status}`);
  }
  const body = await response.json();
  return Array.isArray(body?.boards) ? body.boards : [];
}

// A backing is `{ describe(), listBoards(), connect(boardId) }`. The polyfill
// takes one of these; this is the native one, and a wasm worker would be the
// same three methods.
//
// `transport` names the two globals this backing reaches for — `fetch` and
// `WebSocket`. The CI half of the conformance suite passes a scripted door
// there instead of standing a server up, so everything in this file and in
// the polyfill above it is the code CI actually runs; only the socket is
// scripted. Production passes nothing.
export function nativeBacking(baseUrl, transport = {}) {
  return {
    describe: () => `emu serve ${baseUrl}`,
    listBoards: () => listBoards(baseUrl, transport),
    connect: (boardId, board) => EmulatorPort.connect(baseUrl, boardId, board, transport),
  };
}

export class EmulatorPort {
  // The control channel is the cable and it is held for the port's whole
  // life; the byte socket is the *application's* open port and comes and goes
  // with `open()`/`close()`. That split is the door's coupling rule, not a
  // convention of ours: a byte client's connect IS the machine's `open` and
  // its disconnect IS the machine's `close`, so `open()` below sends no verb.
  static async connect(baseUrl, boardId, board = null, transport = {}) {
    const port = new EmulatorPort(baseUrl, boardId, board, transport);
    await port._openControl();
    return port;
  }

  constructor(baseUrl, boardId, board, transport = {}) {
    this.baseUrl = baseUrl;
    this.boardId = boardId;
    this.board = board;
    this._Socket = transport.WebSocketImpl ?? globalThis.WebSocket;
    this._control = null;
    this._bytes = null;
    this._pending = [];
    this._byteListeners = new Set();
    // Bytes that arrived before anyone was reading. A real port buffers them
    // between `open()` and `getReader()` and so does this one — dropping them
    // would lose the board's greeting on every single open.
    this._pendingBytes = [];
    this._eventListeners = new Map();
    // The last guest cycle any control reply named. A reply whose cycle is
    // BELOW it is a reboot: the machine went back to its power-on state, so
    // the counter did too. Reading a field the reply already carries is not
    // decoding the dance — it is the one honest way this backing learns that
    // a board the emulator reset underneath us has re-enumerated.
    this._lastCycle = -1;
    this._disposed = false;
  }

  // --- point 3: bytes both ways -------------------------------------------

  onBytes(callback) {
    this._byteListeners.add(callback);
    for (const bytes of this._pendingBytes.splice(0, this._pendingBytes.length)) {
      callback(bytes);
    }
    return () => this._byteListeners.delete(callback);
  }

  write(bytes) {
    const socket = this._bytes;
    if (!socket || socket.readyState !== SOCKET_OPEN) {
      throw new Error(`emulated board ${this.boardId}: byte channel is not open`);
    }
    socket.send(bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes));
  }

  // --- point 4: the control vocabulary, one method per verb ----------------

  attach() {
    return this.command("attach");
  }

  detach() {
    return this.command("detach");
  }

  // The coupling rule: connecting the byte socket IS `open` on the machine,
  // so this opens a socket and sends no verb. Sending `open` here as well
  // would answer `err` — the port is already open, by our own connect.
  async open() {
    this._pendingBytes.length = 0;
    if (this._bytes) {
      throw portBusy(`emulated board ${this.boardId} is already open in this page`);
    }
    await this._openBytes();
  }

  async close() {
    const socket = this._bytes;
    if (!socket) {
      return;
    }
    this._bytes = null;
    await closeSocket(socket);
  }

  // One write when both lines are named, `dtr`/`rts` when one is — the same
  // three spellings a UartBridge-style host uses, and the emulator's reply
  // echoes which it got.
  signals({ dtr, rts } = {}) {
    const hasDtr = dtr !== undefined && dtr !== null;
    const hasRts = rts !== undefined && rts !== null;
    if (hasDtr && hasRts) {
      return this.command(`signals dtr=${level(dtr)} rts=${level(rts)}`);
    }
    if (hasDtr) {
      return this.command(`dtr ${level(dtr)}`);
    }
    if (hasRts) {
      return this.command(`rts ${level(rts)}`);
    }
    return Promise.resolve(null);
  }

  async reset() {
    const reply = await this.command("reset");
    this._reenumerated();
    return reply;
  }

  async downloadMode() {
    const reply = await this.command("download-mode");
    this._reenumerated();
    return reply;
  }

  async state() {
    return parseFields(await this.command("state"));
  }

  async pins() {
    return parsePins(await this.command("pins"));
  }

  // --- point 5: flash and snapshots ---------------------------------------
  //
  // The door persists flash server-side (`--state-dir`, one `<id>.flash.bin`
  // per board) and reports a state word in `GET /boards`; it exposes no route
  // for the bytes and none for a snapshot. So these say so.

  getFlash() {
    return Promise.reject(unavailable("getFlash"));
  }

  putFlash() {
    return Promise.reject(unavailable("putFlash"));
  }

  snapshot() {
    return Promise.reject(unavailable("snapshot"));
  }

  /// What `GET /boards` said about this board's flash when the port was made
  /// (`blank` / `loaded` / …). Not the bytes — see `getFlash`.
  flashState() {
    return this.board?.flash ?? null;
  }

  // --- point 6: probes -----------------------------------------------------
  //
  // Heap ledger, stack high-water, decoded frames per pin: the wasm backing's
  // getters. The door answers `state` and `pins`, which is guest time and the
  // pads, and nothing else — so `probes()` does not pretend.

  probes() {
    return Promise.reject(unavailable("probes"));
  }

  // --- the raw control channel --------------------------------------------

  /// Send one line, get its one reply. Rejects on `err …`.
  command(line) {
    if (this._disposed) {
      return Promise.reject(new Error(`emulated board ${this.boardId}: port disposed`));
    }
    const socket = this._control;
    if (!socket || socket.readyState !== SOCKET_OPEN) {
      return Promise.reject(
        new Error(`emulated board ${this.boardId}: control channel is not open`),
      );
    }
    return new Promise((resolve, reject) => {
      this._pending.push({ resolve, reject, line });
      socket.send(line);
    });
  }

  // --- events --------------------------------------------------------------
  //
  // `reenumerate` — the board went back to power-on, so the port object above
  // this one is a dead generation and its replacement must be a NEW object.
  // `byteserror` — the byte channel failed under an open port; the polyfill
  // errors the `readable` stream, which is what a device going away does.

  on(type, callback) {
    let set = this._eventListeners.get(type);
    if (!set) {
      set = new Set();
      this._eventListeners.set(type, set);
    }
    set.add(callback);
    return () => set.delete(callback);
  }

  emit(type, detail) {
    for (const callback of this._eventListeners.get(type) ?? []) {
      try {
        callback(detail);
      } catch (error) {
        console.warn(`[emulator-port] ${type} listener failed`, error);
      }
    }
  }

  /// Tear down both sockets. Named apart from `close()` on purpose: `close()`
  /// is the control vocabulary's verb (the application closing the port) and
  /// this is the cable, the page and the object going away together.
  async dispose() {
    this._disposed = true;
    const bytes = this._bytes;
    const control = this._control;
    this._bytes = null;
    this._control = null;
    for (const pending of this._pending.splice(0, this._pending.length)) {
      pending.reject(new Error(`emulated board ${this.boardId}: port disposed`));
    }
    if (bytes) {
      await closeSocket(bytes);
    }
    if (control) {
      await closeSocket(control);
    }
  }

  // --- internals -----------------------------------------------------------

  _openControl() {
    const socket = new this._Socket(this._url("control"));
    return new Promise((resolve, reject) => {
      socket.addEventListener("open", () => {
        this._control = socket;
        resolve();
      });
      socket.addEventListener("error", () => {
        reject(
          portBusy(
            `emulated board ${this.boardId}: control channel refused ` +
              `(a second client is refused with 409 — one application per port)`,
          ),
        );
      });
      socket.addEventListener("message", (event) => this._onControlMessage(event));
      socket.addEventListener("close", () => {
        if (this._control === socket) {
          this._control = null;
        }
      });
    });
  }

  _onControlMessage(event) {
    const line = String(event.data ?? "").trim();
    const pending = this._pending.shift();
    const cycle = cycleOf(line);
    if (cycle !== null) {
      if (this._lastCycle >= 0 && cycle < this._lastCycle) {
        // The counter went backwards: the machine is back at power-on.
        this._reenumerated();
      }
      this._lastCycle = cycle;
    }
    if (!pending) {
      return;
    }
    if (line.startsWith("err")) {
      pending.reject(new Error(`emulated board ${this.boardId}: \`${pending.line}\` → ${line}`));
    } else {
      pending.resolve(line);
    }
  }

  _reenumerated() {
    // A fresh generation starts its cycles from zero; forget the old high
    // water so the next reply is not read as a second reboot.
    this._lastCycle = -1;
    this.emit("reenumerate", { boardId: this.boardId });
  }

  _openBytes() {
    const socket = new this._Socket(this._url("bytes"));
    socket.binaryType = "arraybuffer";
    return new Promise((resolve, reject) => {
      let opened = false;
      socket.addEventListener("open", () => {
        opened = true;
        this._bytes = socket;
        resolve();
      });
      socket.addEventListener("message", (event) => {
        const data = event.data;
        const bytes =
          data instanceof ArrayBuffer
            ? new Uint8Array(data)
            : new TextEncoder().encode(String(data));
        if (this._byteListeners.size === 0) {
          this._pendingBytes.push(bytes);
          return;
        }
        for (const listener of this._byteListeners) {
          listener(bytes);
        }
      });
      socket.addEventListener("error", () => {
        if (!opened) {
          reject(
            portBusy(
              `emulated board ${this.boardId}: byte channel refused ` +
                `(409 — the port is held by another client)`,
            ),
          );
        }
      });
      socket.addEventListener("close", () => {
        if (this._bytes === socket) {
          this._bytes = null;
          this.emit("byteserror", {
            boardId: this.boardId,
            reason: "the emulated board's byte channel closed",
          });
        }
      });
    });
  }

  _url(channel) {
    const base = new URL(baseWithSlash(this.baseUrl));
    base.protocol = base.protocol === "https:" ? "wss:" : "ws:";
    return new URL(`board/${encodeURIComponent(this.boardId)}/${channel}`, base).toString();
  }
}

function unavailable(what) {
  const error = new Error(`${what}: not available on the native backing`);
  error.name = "NotSupportedError";
  return error;
}

// The name the Studio controller classifies as a HELD port
// (`browser_esp32_device_controller.js`'s open retry ladder leads with the
// DOMException name so Rust can tell "in use elsewhere" from "unresponsive").
function portBusy(message) {
  const error = new Error(message);
  error.name = "NetworkError";
  return error;
}

function level(value) {
  return value ? "1" : "0";
}

function baseWithSlash(baseUrl) {
  const text = String(baseUrl);
  return text.endsWith("/") ? text : `${text}/`;
}

function cycleOf(line) {
  const match = /(?:^|\s)cyc=(\d+)/.exec(line);
  return match ? Number(match[1]) : null;
}

/// `ok state cyc=… us=… host=… draining=… …` → `{ cyc, us, host, … }`, with
/// the numbers as numbers and `true`/`false` as booleans. The verb and the
/// `ok` are dropped; everything else is whatever the emulator said.
function parseFields(line) {
  const fields = {};
  for (const word of String(line).trim().split(/\s+/).slice(2)) {
    const split = word.indexOf("=");
    if (split < 0) {
      continue;
    }
    const key = word.slice(0, split);
    const value = word.slice(split + 1);
    if (/^\d+$/.test(value)) {
      fields[key] = Number(value);
    } else if (value === "true" || value === "false") {
      fields[key] = value === "true";
    } else {
      fields[key] = value;
    }
  }
  return fields;
}

/// `ok pins cyc=… us=… pads=<n> gpio18[route=… ie=… …] …` → one row per pad.
function parsePins(line) {
  const text = String(line);
  const rows = [];
  const pattern = /(\w+)\[([^\]]*)\]/g;
  let match;
  while ((match = pattern.exec(text)) !== null) {
    const row = { pad: match[1] };
    for (const word of match[2].trim().split(/\s+/)) {
      const split = word.indexOf("=");
      if (split > 0) {
        row[word.slice(0, split)] = word.slice(split + 1);
      }
    }
    rows.push(row);
  }
  return { ...parseFields(text), pins: rows };
}

function closeSocket(socket) {
  if (socket.readyState === SOCKET_CLOSED) {
    return Promise.resolve();
  }
  return new Promise((resolve) => {
    socket.addEventListener("close", () => resolve(), { once: true });
    try {
      socket.close();
    } catch {
      resolve();
    }
  });
}
