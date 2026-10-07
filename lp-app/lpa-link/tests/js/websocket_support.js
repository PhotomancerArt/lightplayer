// A `WebSocket` double for the LAN provider's conformance suite
// (`tests/browser_websocket_conformance.rs`).
//
// The suite cannot serve a real WebSocket peer: `wasm-bindgen-test-runner`
// serves static files over HTTP and nothing else. So the boundary is faked
// at the one place the provider touches the browser — `globalThis.WebSocket`
// — and the BOARD behind it is real: the Rust suite runs a secure lp-link
// responder (`lp_link::Link::new_secure`, the board's own role) and moves
// frames through the functions below. Everything above the constructor is
// the shipped `browser_websocket.js`, loaded once, as the library embeds it.
//
// The double implements exactly the subset `browser_websocket.js`'s header
// names: the constructor, `binaryType`, `readyState`, `send`, `close`, and
// the four `on*` handlers. Events are dispatched on a later task, as a
// browser does.

const CONNECTING = 0;
const OPEN = 1;
const CLOSED = 3;

let original = null;
/// One entry per URL the suite has a board at.
const boards = new Map();

function boardAt(url) {
  let board = boards.get(url);
  if (!board) {
    board = { accept: true, sockets: [], sent: [], opened: 0 };
    boards.set(url, board);
  }
  return board;
}

class FakeWebSocket {
  constructor(url) {
    this.url = url;
    this.readyState = CONNECTING;
    this.binaryType = "blob";
    this.onopen = null;
    this.onmessage = null;
    this.onerror = null;
    this.onclose = null;
    const board = boardAt(url);
    board.sockets.push(this);
    setTimeout(() => {
      if (this.readyState !== CONNECTING) {
        return;
      }
      if (board.accept) {
        this.readyState = OPEN;
        board.opened += 1;
        this.onopen?.({});
      } else {
        this.readyState = CLOSED;
        this.onerror?.({});
        this.onclose?.({ code: 1006, reason: "" });
      }
    }, 0);
  }

  send(data) {
    if (this.readyState !== OPEN) {
      throw new Error("send on a socket that is not open");
    }
    if (typeof data === "string") {
      throw new Error("the double carries binary frames only");
    }
    const bytes = data instanceof Uint8Array ? data.slice() : new Uint8Array(data);
    // A frame that is a view of a larger buffer would be a bug in the
    // provider (rule 3): record it so the suite can say so.
    boardAt(this.url).sent.push({ bytes, ownBuffer: data.byteLength === data.buffer?.byteLength });
  }

  close() {
    if (this.readyState === CLOSED) {
      return;
    }
    this.readyState = CLOSED;
    setTimeout(() => this.onclose?.({ code: 1000, reason: "" }), 0);
  }
}

/// Put the double in place of the browser's `WebSocket` (once per page).
export function installFakeWebSocket() {
  if (!original) {
    original = globalThis.WebSocket ?? null;
    globalThis.WebSocket = FakeWebSocket;
  }
}

/// Every frame the page sent to the board at `url` since the last call, in
/// order. Throws if any was a view of a larger buffer.
export function takeSent(url) {
  const board = boardAt(url);
  const sent = board.sent;
  board.sent = [];
  for (const frame of sent) {
    if (!frame.ownBuffer) {
      throw new Error("a frame was sent as a view of a larger buffer");
    }
  }
  return sent.map((frame) => frame.bytes);
}

/// One frame from the board to the page at `url`, as one binary message.
export function deliver(url, frame) {
  const socket = openSocket(url);
  if (!socket) {
    return false;
  }
  const buffer = frame.slice().buffer;
  setTimeout(() => {
    if (socket.readyState === OPEN) {
      socket.onmessage?.({ data: buffer });
    }
  }, 0);
  return true;
}

/// The board closes the socket at `url` (a reboot, a link it gave up on).
export function dropSocket(url, code, reason) {
  const socket = openSocket(url);
  if (!socket) {
    return false;
  }
  socket.readyState = CLOSED;
  setTimeout(() => socket.onclose?.({ code, reason }), 0);
  return true;
}

/// Whether new connects to `url` are accepted.
export function acceptConnects(url, accept) {
  boardAt(url).accept = accept;
}

/// How many sockets to `url` have opened.
export function socketsOpened(url) {
  return boardAt(url).opened;
}

function openSocket(url) {
  const board = boardAt(url);
  for (let i = board.sockets.length - 1; i >= 0; i -= 1) {
    if (board.sockets[i].readyState === OPEN) {
      return board.sockets[i];
    }
  }
  return null;
}

export function tick(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
