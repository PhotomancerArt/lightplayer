// Studio's LAN link: lp-link over a browser WebSocket to a board on Wi-Fi,
// owned here the way `browser_ble.js` owns a GATT connection (Wi-Fi M6 P07).
//
// A board on the LAN serves `ws://<board>/link`; each BINARY message is one
// whole lp-link frame (the DATAGRAM framing: a 4-byte header, up to one
// payload, a 4-byte CRC-32C, sealed — the link is a secure one). This file
// does not read frames, but it keeps their boundaries: every message is
// handed to Rust as it came (`takeFrames`), and every `write` is one frame
// from Rust, sent as one message — never cut, never joined. The link itself
// (the Noise handshake inside the SYN, the checksum, the session) is the Rust
// side's (`ws_link_port.rs`, one secure `LinkPortService` per connection).
//
// THE WEBSOCKET SUBSET, in full — the conformance suite's fake
// (`lpa-link/tests/js/websocket_support.js`) implements exactly this:
//
//   new globalThis.WebSocket(url); ws.binaryType = "arraybuffer"
//   ws.{readyState, send(bytes), close()}
//   ws.{onopen, onmessage, onerror, onclose}   (event.data, event.code, event.reason)
//
// THREE RULES:
//
//  1. **Every connect is bounded (10 s).** A socket to an address nothing
//     answers can sit in CONNECTING for a TCP timeout (minutes); the page
//     gives up sooner, says so, and retries on its own schedule.
//  2. **A drop is a departure, then a reconnect with no gesture.** An
//     unrequested close (the board rebooted, left the network, or closed an
//     unauthenticated link after its 10 s) is said once, as `wi-fi link
//     lost: …` (the Rust side reads that phrase as the link being gone), the
//     session stops being present, and a backed-off reconnect starts. Each
//     connection is a new lp-link session: the board makes its end per
//     socket, and `generation` moves with every connect, drop and close.
//  3. **Each frame is its own buffer.** A frame from Rust is a view onto wasm
//     memory; `send` gets a copy exactly one frame long (the Bluetooth path's
//     2026-10-02 lesson, kept here so no browser can send a view's whole
//     buffer).
//
// A BUSY BOARD IS NOT A CONNECTION. A C6 has one LAN slot; a second client
// (Studio in another tab, lp-cli) gets its socket upgraded and then closed at
// once with 1013, "try again later". That is said once, in words — `wi-fi
// link lost: busy with another Wi‑Fi connection …` — and redialled on its
// own slow schedule (2 s, 5 s, 15 s, then every 30 s), never the drop
// loop's 250 ms. Until the board answers again, a redial that opens is not
// announced as present (its link is serviced through `onActivity` all the
// same), so a busy board does not flap in and out of Studio's roster.
//
// A BOARD WHOSE ADDRESS MOVED IS FOUND BY NAME. A session keeps its URL (its
// identity), but once the Rust side knows the board's `.local` socket
// (`setFallback`, from its hello's MAC) and the URL has not answered for
// `FALLBACK_AFTER_MS`, every other redial goes to that name instead; the
// socket that answers is the one redialled first from then on.
//
// Presence is announced the way Web Serial's hotplug is — a `connect` /
// `disconnect` edge with no payload — and Studio's device layer re-derives
// from `presentSessions()`.
//
// TWO KINDS OF SESSION, ONE SOCKET CODE (the network transport's ND1). A
// `wi-fi` session dials a board's own `ws://<board>/link`; a `relay` session
// dials lightplayer.app's browser leg (`wss://<host>/relay/board/<mac>`),
// which carries the same frames. What differs is said here and nowhere else:
// a relay session's words start `relay …` instead of `wi-fi …` (so its drop
// is `relay link lost: …`), and a close with one of its `finalCodes` (the
// relay's refusals: not signed in, board offline, too many tries, busy) ends
// the session instead of redialling — a redial would only be refused again,
// and each one spends the page's tries at the relay.
//
// A HELD SESSION RIDES THROUGH A REBOOT. While an update runs through the
// relay the board resets three times, and each time the relay says "board
// offline" until it is back. The Rust side holds such a session (`hold`) for
// a while after each update message it sends; while it is held, a close with
// one of its `holdCodes` is a drop redialled after that code's own delay
// (slow enough to spare the page's tries at the relay), not the end.

/// Rule 1.
export const CONNECT_TIMEOUT_MS = 10_000;
/// Bytes of received frames a session holds for a Rust side that is not
/// draining. Its link drains on every message (`onActivity`) and on a timer,
/// so this only bounds a page that stopped running Rust at all.
const MAX_BUFFERED_BYTES = 256 * 1024;
/// The reconnect loop's delays, in order; the last repeats for as long as the
/// session is wanted (a board on the LAN that reboots, or comes back to the
/// network, is found again with no gesture).
const RECONNECT_DELAYS_MS = [250, 1_000, 2_000, 4_000, 8_000, 15_000];
/// How long a dropped session's own address must go unanswered before its
/// board's `.local` socket is tried beside it.
export const FALLBACK_AFTER_MS = 10_000;
/// The close code a board with its one LAN slot taken answers with (RFC
/// 6455 "try again later").
export const BUSY_CLOSE_CODE = 1013;
/// How long a redial to a board that was busy must stay open, unanswered,
/// before it is announced anyway (a busy board closes at once).
const BUSY_SETTLE_MS = 1_000;
/// A busy board's redials, in order; the last repeats.
export const BUSY_DELAYS_MS = [2_000, 5_000, 15_000, 30_000];
/// What a busy board's drop says (after `wi-fi link lost: `). Ends with the
/// close code, which Studio's connect words read.
export const BUSY_WORDS = `busy with another Wi\u2011Fi connection (Studio in another tab, or lp-cli; code ${BUSY_CLOSE_CODE})`;
/// `WebSocket.OPEN`, spelled out so a fake constructor need not carry it.
const OPEN = 1;

const sessions = new Map();
let nextSessionId = 1;
const presence = { onConnect: null, onDisconnect: null };

export function isSupported() {
  return typeof globalThis.WebSocket === "function";
}

// --- presence: the hotplug edges ------------------------------------------

/// Install the `connect`/`disconnect` edge callbacks, once. Each is called
/// with no argument and means "re-derive": a session became present, or
/// stopped being present.
export function installWebsocketEvents(onConnect, onDisconnect) {
  if (presence.onConnect) {
    return false;
  }
  presence.onConnect = onConnect;
  presence.onDisconnect = onDisconnect;
  // A session opened at page load (`?lan=`, `?relay=`) can come up before
  // the page installs these — a socket to a local server opens in
  // milliseconds — and its edge was said to nobody. Say it again now, so
  // the sweep that links it is not left waiting for the next edge.
  if ([...sessions.values()].some((session) => session.present)) {
    setTimeout(() => announce("connect"), 0);
  }
  return true;
}

function announce(edge) {
  const callback = edge === "connect" ? presence.onConnect : presence.onDisconnect;
  try {
    callback?.();
  } catch (error) {
    console.warn(`[lan] ${edge} edge handler threw:`, error);
  }
}

// --- sessions ----------------------------------------------------------------

class LanSession {
  constructor(id, url, kind, finalCodes) {
    this.id = id;
    this.url = url;
    // "wi-fi" | "relay": the first word of everything the session says.
    this.kind = kind === "relay" ? "relay" : "wi-fi";
    // Close codes after which the session is not redialled.
    this.finalCodes = new Set(finalCodes ?? []);
    // code → redial delay (ms) for a final code while the session is held.
    this.holdCodes = new Map();
    // Until when (Date.now() ms) the session is held (`hold`).
    this.holdUntil = 0;
    // idle | connecting | connected | lost | closed
    this.state = "idle";
    this.generation = 0;
    this.socket = null;
    this.connecting = null;
    this.frames = [];
    this.bufferedBytes = 0;
    this.overflowNoted = false;
    this.errors = [];
    this.listeners = new Set();
    this.wanted = false;
    this.present = false;
    this.reconnectTimer = null;
    this.attempt = 0;
    // Messages received over the session's life, and the words of its last
    // drop: what `settle` reads to tell an answering board from one that
    // turned the connection away.
    this.received = 0;
    this.lastDrop = null;
    // The link above the socket said it is up on this connection (`markUp`).
    this.up = false;
    // Busy closes (1013) since the board last sent a frame: picks the busy
    // redial delay, and keeps a redial that opens from being announced.
    this.busyStrikes = 0;
    // The board's `.local` socket (`setFallback`), the socket that answered
    // last, and when the session lost its connection.
    this.fallbackUrl = null;
    this.dialUrl = url;
    this.lostAt = null;
  }

  describe() {
    return { id: this.id, url: this.url, kind: this.kind, connected: this.state === "connected" };
  }

  clearFrames() {
    this.frames = [];
    this.bufferedBytes = 0;
  }

  activity() {
    for (const listener of [...this.listeners]) {
      try {
        listener();
      } catch (error) {
        console.warn("[lan] activity listener threw:", error);
      }
    }
  }
}

/// Start (or keep) a session for the board at `url`: one per URL per page.
/// The session wants its link from now on, so it connects at once and keeps
/// reconnecting; it is present (listed) once it is connected. Answers its
/// descriptor `{ id, url, kind, connected }`. `options` (optional, read when
/// the session is made): `{ kind: "wi-fi" | "relay", finalCodes: [code…],
/// holdCodes: [[code, delayMs]…] }`.
export function openSession(url, options) {
  for (const session of sessions.values()) {
    if (session.url === url) {
      if (!session.wanted) {
        session.wanted = true;
        startReconnect(session, 0);
      }
      return session.describe();
    }
  }
  const session = new LanSession(nextSessionId++, url, options?.kind, options?.finalCodes);
  for (const [code, delayMs] of options?.holdCodes ?? []) {
    session.holdCodes.set(code, delayMs);
  }
  sessions.set(session.id, session);
  session.wanted = true;
  startReconnect(session, 0);
  return session.describe();
}

/// The sessions Studio should hold a link for right now: connected, or
/// closed by request. A dropped session is NOT listed while it reconnects —
/// that is how the departure sweep learns the board went away.
export function presentSessions() {
  return [...sessions.values()].filter((session) => session.present).map((s) => s.describe());
}

export function isConnected(id) {
  return sessions.get(id)?.state === "connected";
}

function requireSession(id) {
  const session = sessions.get(id);
  if (!session) {
    throw new Error(`Unknown wi-fi session: ${id}`);
  }
  return session;
}

/// Connect (or confirm) a session's link. Bounded (rule 1); concurrent
/// callers share the attempt in flight.
export async function connect(id) {
  const session = requireSession(id);
  session.wanted = true;
  cancelReconnect(session);
  try {
    await connectSession(session);
  } catch (error) {
    // The caller hears the failure; the session keeps trying on its own.
    if (session.wanted && sessions.has(session.id)) {
      startReconnect(session, null);
    }
    throw error;
  }
  return true;
}

/// Close the link by request: no reconnect follows, and the session stays
/// listed so the link can be opened again with no flag and no gesture.
export async function disconnect(id) {
  const session = sessions.get(id);
  if (!session) {
    return;
  }
  session.wanted = false;
  cancelReconnect(session);
  session.generation += 1;
  session.connecting = null;
  session.busyStrikes = 0;
  session.state = "closed";
  session.up = false;
  closeSocket(session);
  session.clearFrames();
  session.activity();
}

/// Forget a session: closed, and no longer listed or reconnected.
export async function forget(id) {
  const session = sessions.get(id);
  if (!session) {
    return false;
  }
  await disconnect(id);
  sessions.delete(id);
  if (session.present) {
    session.present = false;
    announce("disconnect");
  }
  return true;
}

// --- frames ----------------------------------------------------------------

/// Send ONE lp-link frame as one binary message (rule 3). `false` when the
/// link is not connected (the frame never left, which the link on the Rust
/// side treats like any lost frame).
export function write(id, frame) {
  const session = requireSession(id);
  const socket = session.socket;
  if (session.state !== "connected" || !socket || socket.readyState !== OPEN) {
    session.errors.push(`write on a ${session.kind} link that is not connected`);
    return false;
  }
  const data = frame instanceof Uint8Array ? frame.slice() : new Uint8Array(frame);
  try {
    socket.send(data);
    return true;
  } catch (error) {
    handleDrop(session, `a send failed: ${messageOf(error)}`);
    return false;
  }
}

/// `{ url, generation, connected, frames }`: every frame the board sent
/// since the last call, each as the one message it came in; which connection
/// they belong to (a new `generation` is a new lp-link session); and whether
/// the link can be written now.
export function takeFrames(id) {
  const session = requireSession(id);
  const frames = session.frames;
  session.clearFrames();
  return {
    url: session.url,
    generation: session.generation,
    connected: session.state === "connected",
    frames,
  };
}

/// Call `callback` (no argument) whenever the Rust side should look at the
/// session now: a message came, the link came up or went away. A hidden page
/// throttles timers, not these events. Returns the unsubscribe function.
export function onActivity(id, callback) {
  const session = requireSession(id);
  session.listeners.add(callback);
  return () => session.listeners.delete(callback);
}

/// Wait until the connection that is up now proves itself — because a
/// person asked for it and is waiting for the answer ("Connect over Wi‑Fi",
/// an address typed into the add slot). A board that answers sends a frame
/// (its half of the handshake); a board that turns the connection away
/// closes it right after the upgrade (a busy board's 1013, "try again
/// later"). Resolves `true` on a frame, or when `ms` pass with the link
/// still up; rejects with the drop's words when it drops. Never touches the
/// frames themselves (the link reads those).
///
/// `untilUp`: wait for more than a frame — for the link above the socket to
/// say it is up (`markUp`). Through the relay the first frame proves only
/// that the relay passed the socket on; the board's answer to the key is
/// what counts, and a session that runs out of keys gives up (`giveUp`),
/// which rejects here with its words.
export function settle(id, ms, untilUp) {
  const session = requireSession(id);
  const generation = session.generation;
  const heard = session.received;
  return new Promise((resolve, reject) => {
    let done = false;
    let timer = null;
    const check = () => {
      if (done) {
        return;
      }
      const dropped = session.generation !== generation || session.state !== "connected";
      const proved = untilUp ? session.up : session.received !== heard;
      if (!dropped && !proved) {
        return;
      }
      done = true;
      clearTimeout(timer);
      session.listeners.delete(check);
      if (dropped) {
        const why = session.lastDrop ?? "the connection closed";
        reject(new Error(`${session.kind} link lost: ${why}`));
      } else {
        resolve(true);
      }
    };
    timer = setTimeout(() => {
      if (!done) {
        done = true;
        session.listeners.delete(check);
        resolve(true);
      }
    }, ms);
    session.listeners.add(check);
    check();
  });
}

/// The link above the socket is up on the connection open now (its secure
/// handshake is done): what `settle(…, untilUp)` waits for.
export function markUp(id) {
  const session = sessions.get(id);
  if (!session || session.up || session.state !== "connected") {
    return;
  }
  session.up = true;
  session.activity();
}

/// Where else this session's board answers: its `.local` socket, tried
/// beside the session's URL once that has gone unanswered for
/// `FALLBACK_AFTER_MS`.
export function setFallback(id, url) {
  const session = sessions.get(id);
  if (session && url && url !== session.url) {
    session.fallbackUrl = url;
  }
}

/// Hold the session for `ms` from now: until then a close with one of its
/// `holdCodes` is redialled after that code's delay instead of ending the
/// session (an update through the relay, across the board's resets).
export function hold(id, ms) {
  const session = sessions.get(id);
  if (session) {
    session.holdUntil = Math.max(session.holdUntil, Date.now() + ms);
  }
}

/// End the session for good, saying why: the link above the socket has
/// nothing left to try (a relay session out of keys). Heard as a drop —
/// `<kind> link lost: <why>` — but never redialled, and no longer listed.
export function giveUp(id, why) {
  const session = sessions.get(id);
  if (!session) {
    return;
  }
  session.wanted = false;
  cancelReconnect(session);
  if (session.state === "connected" || session.state === "connecting") {
    handleDrop(session, why);
  }
}

export function takeErrors(id) {
  const session = requireSession(id);
  const errors = session.errors;
  session.errors = [];
  return errors;
}

// --- the connection itself -------------------------------------------------

async function connectSession(session) {
  if (session.state === "connected" && session.socket?.readyState === OPEN) {
    return;
  }
  if (session.connecting) {
    return session.connecting;
  }
  session.state = "connecting";
  const generation = ++session.generation;
  const attempt = openSocket(session, generation, dialTarget(session));
  session.connecting = attempt;
  try {
    await attempt;
  } finally {
    if (session.connecting === attempt) {
      session.connecting = null;
    }
  }
}

/// The socket a connect dials: the one that answered last, and — once the
/// session has been lost for `FALLBACK_AFTER_MS` with a fallback known —
/// every other attempt the other of the session's URL and its fallback.
function dialTarget(session) {
  const last = session.dialUrl ?? session.url;
  if (!session.fallbackUrl || session.lostAt === null) {
    return last;
  }
  if (Date.now() - session.lostAt < FALLBACK_AFTER_MS || session.attempt % 2 === 0) {
    return last;
  }
  return last === session.url ? session.fallbackUrl : session.url;
}

function openSocket(session, generation, target) {
  return new Promise((resolve, reject) => {
    let settled = false;
    let socket;
    const fail = (why) => {
      if (settled) {
        return;
      }
      settled = true;
      clearTimeout(timer);
      if (session.generation === generation) {
        session.state = session.wanted ? "lost" : "idle";
        session.socket = null;
      }
      try {
        socket?.close();
      } catch {
        // nothing to abandon
      }
      reject(new Error(why));
    };
    const timer = setTimeout(
      () => fail(`${session.kind} connect timed out after ${CONNECT_TIMEOUT_MS / 1000} s`),
      CONNECT_TIMEOUT_MS,
    );
    try {
      socket = new globalThis.WebSocket(target);
    } catch (error) {
      fail(`${session.kind} connect failed: ${messageOf(error)}`);
      return;
    }
    socket.binaryType = "arraybuffer";
    socket.onopen = () => {
      if (settled) {
        return;
      }
      settled = true;
      clearTimeout(timer);
      if (session.generation !== generation) {
        // Superseded while it was in flight (a timeout, a close): not ours.
        try {
          socket.close();
        } catch {
          // already gone
        }
        resolve();
        return;
      }
      session.socket = socket;
      session.clearFrames();
      session.overflowNoted = false;
      session.state = "connected";
      session.lostAt = null;
      if (target !== session.dialUrl) {
        console.info(`[lan] ${session.url}: the board answered at ${target}`);
      }
      session.dialUrl = target;
      session.up = false;
      session.attempt = 0;
      session.activity();
      // A board that turned the last connections away (busy) is present
      // again once it answers (`onFrame`) — or, if nothing is servicing this
      // session's link yet (it was turned away before Studio attached it),
      // once the connection has stayed open for `BUSY_SETTLE_MS`.
      if (session.busyStrikes === 0) {
        becomePresent(session);
      } else {
        setTimeout(() => {
          if (session.generation === generation && session.state === "connected") {
            becomePresent(session);
          }
        }, BUSY_SETTLE_MS);
      }
      resolve();
    };
    socket.onmessage = (event) => {
      if (session.socket !== socket || session.generation !== generation) {
        return;
      }
      onFrame(session, event.data);
    };
    socket.onerror = () => {
      // A connect that fails fires error, then close; the close says why.
      if (!settled) {
        fail(`${session.kind} connect to ${target} failed`);
      }
    };
    socket.onclose = (event) => {
      if (!settled) {
        fail(`${session.kind} connect to ${target} was closed (${closeWords(event)})`);
        return;
      }
      if (session.socket === socket && session.generation === generation) {
        // A refusal that would only be repeated ends the session here —
        // unless the session is held and the code is one it rides through.
        if (session.finalCodes.has(event?.code)) {
          const held = heldDelay(session, event?.code);
          if (held !== null && session.wanted) {
            handleDrop(session, `the board closed the link (${closeWords(event)})`, held);
            return;
          }
          session.wanted = false;
        }
        if (event?.code === BUSY_CLOSE_CODE && session.wanted) {
          handleBusy(session);
          return;
        }
        handleDrop(session, `the board closed the link (${closeWords(event)})`);
      }
    };
  });
}

function onFrame(session, data) {
  if (typeof data === "string") {
    session.errors.push(`a text message on the ${session.kind} link (the link is binary frames only)`);
    session.activity();
    return;
  }
  const frame = data instanceof ArrayBuffer ? new Uint8Array(data) : new Uint8Array(data.buffer ?? data);
  session.received += 1;
  // The board answered: it is not busy any more, and it is here.
  session.busyStrikes = 0;
  if (!session.present && session.state === "connected") {
    becomePresent(session);
  }
  if (session.bufferedBytes + frame.length > MAX_BUFFERED_BYTES) {
    if (!session.overflowNoted) {
      session.overflowNoted = true;
      session.errors.push(`${session.kind} frames nobody drained were dropped`);
    }
  } else {
    session.frames.push(frame);
    session.bufferedBytes += frame.length;
  }
  session.activity();
}

/// Mark a session present, and say so if it was not.
function becomePresent(session) {
  const wasPresent = session.present;
  session.present = true;
  if (!wasPresent) {
    announce("connect");
  }
}

/// The board closed the connection with 1013: its one LAN slot is taken.
/// The first strike is a drop, in words; later ones are quiet (nothing was
/// announced for them). Either way the redial waits the busy schedule.
function handleBusy(session) {
  session.busyStrikes += 1;
  const delay = BUSY_DELAYS_MS[Math.min(session.busyStrikes - 1, BUSY_DELAYS_MS.length - 1)];
  if (session.busyStrikes === 1 || session.present) {
    handleDrop(session, BUSY_WORDS, delay);
    return;
  }
  session.lastDrop = BUSY_WORDS;
  session.generation += 1;
  session.connecting = null;
  session.state = "lost";
  session.up = false;
  closeSocket(session);
  session.clearFrames();
  session.activity();
  startReconnect(session, delay);
}

/// The link died underneath us: say so ONCE (the Rust side reads this exact
/// phrase as a departure), stop being present, and reconnect if wanted.
function handleDrop(session, why, reconnectDelayMs) {
  session.lastDrop = why;
  if (session.state === "connected") {
    session.lostAt = Date.now();
  }
  session.generation += 1;
  session.connecting = null;
  session.state = "lost";
  session.up = false;
  closeSocket(session);
  session.errors.push(`${session.kind} link lost: ${why}`);
  session.clearFrames();
  session.activity();
  if (session.present) {
    session.present = false;
    announce("disconnect");
  }
  if (session.wanted) {
    startReconnect(session, reconnectDelayMs ?? null);
  }
}

/// The redial delay for a close with `code` on a held session, or `null`
/// when the session is not held or the code is not one it rides through.
function heldDelay(session, code) {
  if (Date.now() >= session.holdUntil) {
    return null;
  }
  return session.holdCodes.get(code) ?? null;
}

function closeSocket(session) {
  const socket = session.socket;
  session.socket = null;
  if (!socket) {
    return;
  }
  socket.onopen = null;
  socket.onmessage = null;
  socket.onerror = null;
  socket.onclose = null;
  try {
    socket.close();
  } catch {
    // already gone
  }
}

function startReconnect(session, firstDelayMs) {
  cancelReconnect(session);
  session.attempt = 0;
  schedule(session, firstDelayMs ?? RECONNECT_DELAYS_MS[0]);
}

function schedule(session, delayMs) {
  session.reconnectTimer = setTimeout(async () => {
    session.reconnectTimer = null;
    if (!session.wanted || !sessions.has(session.id)) {
      return;
    }
    session.attempt += 1;
    try {
      await connectSession(session);
    } catch (error) {
      if (!session.wanted || !sessions.has(session.id)) {
        return;
      }
      const delays = session.busyStrikes > 0 ? BUSY_DELAYS_MS : RECONNECT_DELAYS_MS;
      const delay = delays[Math.min(session.attempt, delays.length - 1)];
      schedule(session, delay);
    }
  }, delayMs);
}

function cancelReconnect(session) {
  if (session.reconnectTimer !== null) {
    clearTimeout(session.reconnectTimer);
    session.reconnectTimer = null;
  }
}

function closeWords(event) {
  const code = event?.code ?? 0;
  const reason = event?.reason ? `: ${event.reason}` : "";
  return `code ${code}${reason}`;
}

function messageOf(error) {
  return error?.message ?? String(error);
}
