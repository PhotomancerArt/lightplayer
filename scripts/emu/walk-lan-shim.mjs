// A page-side WebSocket shim for the emulated walks: test scaffolding, not a
// product flag (best-link plan, D6).
//
// An emulated board's LAN address is on the door's VIRTUAL LAN (`10.x`),
// which the host cannot dial; the door reaches each board through a loopback
// forward (`forward lan:127.0.0.1:<port>` in `GET /boards`). Studio dials the
// address lightplayer.app's list gave it, so the walk puts this shim in front
// of `globalThis.WebSocket` before Studio's first script runs
// (`Page.addScriptToEvaluateOnNewDocument`): a socket to a `ws://…/link` that
// is not on loopback is answered in one of four ways the walk switches:
//
//   refuse   fails at once (closed before it opened, code 1006), as Chrome
//            does to a page without the Local Network grant
//   forward  opens `ws://<forward>/link` instead: the board's own LAN link
//   hold     keeps the dial pending until the mode changes (the provider's
//            own 10 s bound still applies)
//   drop     `drop(code)` closes every forwarded socket now (the board sees
//            its link close) and tells the page `code`
//
// Every other socket (the relay's browser leg, Studio's own) is the real one.
// The mode lives in this origin's `localStorage`, so it survives a reload.
//
// ⚠️ A loopback http page never sees Chrome's Local Network prompt: this
// shim stands in for the LAN's reachability, and says nothing about Mac or
// Android Chrome's prompt.

const KEY = "lp.walk.lan-shim";

/// The shim's source, run in the page before anything else.
export const LAN_SHIM_SOURCE = `(() => {
  if (globalThis.__lpLanShim) return;
  const Real = globalThis.WebSocket;
  const KEY = ${JSON.stringify(KEY)};
  const saved = (() => { try { return JSON.parse(localStorage.getItem(KEY) || "{}"); } catch { return {}; } })();
  const state = { mode: saved.mode || "refuse", forward: saved.forward || null, held: [], live: [], dials: [] };
  globalThis.__lpLanShim = state;
  const isLan = (url) => /^ws:\\/\\/[^/]+\\/link$/.test(url) && !/^ws:\\/\\/(127\\.0\\.0\\.1|localhost)[:/]/.test(url);
  class ShimSocket {
    constructor(url) {
      this.url = url; this.readyState = 0; this.binaryType = "blob";
      this.onopen = null; this.onmessage = null; this.onerror = null; this.onclose = null;
      this._inner = null; this._closed = false; this._dropped = false;
      state.dials.push({ url, mode: state.mode, at: Date.now() });
      const go = () => this._dial();
      if (state.mode === "hold") state.held.push(go); else setTimeout(go, 0);
    }
    _dial() {
      if (this._closed) return;
      if (state.mode !== "forward" || !state.forward) {
        this.readyState = 3;
        this.onerror?.({});
        this.onclose?.({ code: 1006, reason: "" });
        return;
      }
      const inner = new Real("ws://" + state.forward + "/link");
      inner.binaryType = this.binaryType;
      this._inner = inner;
      state.live.push(this);
      inner.onopen = (event) => { this.readyState = 1; this.onopen?.(event); };
      inner.onmessage = (event) => this.onmessage?.(event);
      inner.onerror = (event) => { if (!this._dropped) this.onerror?.(event); };
      inner.onclose = (event) => {
        this.readyState = 3;
        state.live = state.live.filter((socket) => socket !== this);
        if (!this._dropped) this.onclose?.({ code: event.code, reason: event.reason });
      };
    }
    send(data) {
      if (!this._inner || this.readyState !== 1) throw new Error("send on a socket that is not open");
      this._inner.send(data);
    }
    close() {
      this._closed = true;
      this.readyState = 3;
      try { this._inner?.close(); } catch {}
    }
    _drop(code) {
      this._dropped = true;
      this.readyState = 3;
      state.live = state.live.filter((socket) => socket !== this);
      try { this._inner?.close(); } catch {}
      setTimeout(() => this.onclose?.({ code, reason: "" }), 0);
    }
  }
  const Shim = function (url, protocols) {
    return isLan(String(url)) ? new ShimSocket(String(url)) : new Real(url, protocols);
  };
  Shim.prototype = Real.prototype;
  for (const name of ["CONNECTING", "OPEN", "CLOSING", "CLOSED"]) Shim[name] = Real[name];
  globalThis.WebSocket = Shim;
  state.set = (mode, forward) => {
    state.mode = mode;
    if (forward !== undefined) state.forward = forward;
    try { localStorage.setItem(KEY, JSON.stringify({ mode: state.mode, forward: state.forward })); } catch {}
    if (mode !== "hold") { const held = state.held; state.held = []; for (const go of held) setTimeout(go, 0); }
    return state.mode;
  };
  state.drop = (code) => { const live = [...state.live]; for (const socket of live) socket._drop(code); return live.length; };
})();`;

/// Put the shim in front of every page `driver` loads from now on.
export async function installLanShim(driver) {
  await driver.cdp.send("Page.addScriptToEvaluateOnNewDocument", { source: LAN_SHIM_SOURCE }, driver.sessionId);
}

/// Switch the shim on the page open now (and on its next loads): `mode` is
/// `refuse`, `forward` or `hold`; `forward` is `127.0.0.1:<port>`.
export async function setLanShim(driver, mode, forward) {
  return driver.evaluate(`globalThis.__lpLanShim.set(${JSON.stringify(mode)}${forward === undefined ? "" : `, ${JSON.stringify(forward)}`})`);
}

/// Close every forwarded socket now and tell the page `code`.
export async function dropLanShim(driver, code) {
  return driver.evaluate(`globalThis.__lpLanShim.drop(${code})`);
}

/// The dials the shim answered on this page: `{ url, mode, at }`.
export async function lanShimDials(driver) {
  return driver.evaluate(`globalThis.__lpLanShim?.dials ?? []`);
}
