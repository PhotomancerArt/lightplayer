#!/usr/bin/env node
// A headless Chrome driver for Studio's device walk (emulator plan two, M6).
//
// This is the `manual:` half of a device scenario, automated. On hardware a
// person reads the spec's steps and clicks; under the shim there is no board,
// no chooser and no person, so the steps become calls on this object.
//
// THREE RULES ARE BAKED IN, and each of them is somebody's scar:
//
//  1. **ALWAYS `--headless=new`.** A visible window steals the desk's focus
//     and there are other agents on this box (standing rule, plan two). The
//     existing silicon lane's `spawnSync("open", [url])` is exactly what not
//     to do here. Screenshots come from `Page.captureScreenshot`, which needs
//     no window.
//
//  2. **NOTHING POLLS, AND NOTHING TIMES.** An agent-driven hidden tab is
//     throttled to about 1 Hz (`sim-latency-throttling-artifact`), so a wait
//     written as "check every 100 ms for 5 s" is measuring the throttle. Every
//     wait here is a promise the PAGE resolves — `waitFor` installs a
//     MutationObserver and `Runtime.evaluate({awaitPromise:true})` blocks on
//     it — and no assertion anywhere in this milestone is about a duration.
//     The deadlines below exist to fail a wedged run, never to pass one.
//
//  3. **The cable is the bus's, not ours.** `detach`/`attach` go through
//     `window.__lpEmuSerial.bus`, the page-internal contract M3 built, because
//     the door admits ONE control client per board and answers a second with
//     409 — and that client is this page. A second WebSocket from node would
//     fail in a way that reads like a shim bug.
//
// Studio's own affordances are addressed BY THEIR VISIBLE TEXT ("It's
// connected", "Put it on the board") where they have no hook. That is
// deliberate: it is what the spec's `manual:` steps already say, so the
// emulated lane and the silicon lane are describing the same click.
//
// A BOARD is different: its card carries hooks (`app/board_card/mod.rs`,
// "Walk hooks"), and the card helpers below read it by them — the card by
// `data-board-card`, a bar by `data-bar`, its work by `data-bar-work`, the
// status corner by `data-board-corner`, and every verb by the offer path its
// `AgentMark` carries. A verb is pressed by its path, a state is waited on
// as the offer core publishes for it, and what the board said is read off
// its own terminal in the corner's details. None of them matches the card's
// face text for a verb or a state: a page string Studio can satisfy by
// itself is a weak predicate, and two of them shipped.

import { spawn } from "node:child_process";
import { once } from "node:events";
import { existsSync, writeFileSync } from "node:fs";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import process from "node:process";

const CDP_CALL_TIMEOUT_MS = 30_000;
/// A wedged-run deadline, never a measurement. See rule 2.
export const DEFAULT_WAIT_MS = 120_000;

const CHROME_CANDIDATES = [
  "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
  "/Applications/Brave Browser.app/Contents/MacOS/Brave Browser",
  "/Applications/Chromium.app/Contents/MacOS/Chromium",
  "/usr/bin/google-chrome",
  "/usr/bin/chromium",
];

export function findChrome() {
  if (process.env.CHROME_BIN) return process.env.CHROME_BIN;
  return CHROME_CANDIDATES.find((candidate) => existsSync(candidate)) ?? null;
}

class Cdp {
  static async open(wsUrl) {
    const socket = new WebSocket(wsUrl);
    await new Promise((resolve, reject) => {
      socket.addEventListener("open", resolve, { once: true });
      socket.addEventListener("error", reject, { once: true });
    });
    return new Cdp(socket);
  }

  constructor(socket) {
    this.socket = socket;
    this.nextId = 1;
    this.pending = new Map();
    this.listeners = new Map();
    socket.addEventListener("message", (event) => this.onMessage(event));
    socket.addEventListener("close", () => this.rejectAll(new Error("DevTools closed")));
  }

  on(method, handler) {
    const handlers = this.listeners.get(method) ?? [];
    handlers.push(handler);
    this.listeners.set(method, handlers);
  }

  send(method, params = {}, sessionId = undefined, timeoutMs = CDP_CALL_TIMEOUT_MS) {
    if (this.socket.readyState !== WebSocket.OPEN) {
      return Promise.reject(new Error(`DevTools connection is closed (${method})`));
    }
    const id = this.nextId;
    this.nextId += 1;
    const message = { id, method, params };
    if (sessionId) message.sessionId = sessionId;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        if (this.pending.delete(id)) reject(new Error(`CDP ${method} timed out after ${timeoutMs}ms`));
      }, timeoutMs);
      timer.unref?.();
      this.pending.set(id, {
        resolve: (value) => { clearTimeout(timer); resolve(value); },
        reject: (error) => { clearTimeout(timer); reject(error); },
      });
      this.socket.send(JSON.stringify(message));
    });
  }

  onMessage(event) {
    const message = JSON.parse(event.data.toString());
    if (!message.id) {
      for (const handler of this.listeners.get(message.method) ?? []) handler(message.params, message.sessionId);
      return;
    }
    const pending = this.pending.get(message.id);
    if (!pending) return;
    this.pending.delete(message.id);
    if (message.error) pending.reject(new Error(`${message.error.message}: ${message.error.data ?? ""}`));
    else pending.resolve(message.result ?? {});
  }

  rejectAll(error) {
    for (const pending of this.pending.values()) pending.reject(error);
    this.pending.clear();
  }

  close() { this.socket.close(); }
}

/// Page-side expression for the controls under `scope` (an expression for
/// the element to look under) that `click(text, ...)` may press: enabled,
/// text containing `text` (case-insensitive), and, with `exact`, a leaf
/// element whose whole text is `text`. `click` and the wait in
/// `clickWhenReady` both read it, so the one is never satisfied by a control
/// the other would not press. An absent `scope` element matches nothing.
function matchingControls(text, { scope = "document", exact = false } = {}) {
  return `((() => {
    const wanted = ${JSON.stringify(text.toLowerCase())};
    const norm = (el) => (el.textContent || '').replace(/\\s+/g, ' ').trim().toLowerCase();
    const root = ${scope};
    if (!root) return [];
    return [...root.querySelectorAll('button, [role="button"], a')]
      .filter((el) => !el.disabled)
      .filter((el) => norm(el).includes(wanted))
      .filter((el) => !${exact} || [el, ...el.querySelectorAll('*')]
        .some((n) => n.childElementCount === 0 && norm(n) === wanted));
  })())`;
}

/// The home page's Connect a board section: where the squares live.
const CONNECT_SCOPE = "document.querySelector('#home-connect-board')";

/// The open popover's panel: a picker the card opened (the board pick, the
/// project pick), or any other panel. One popover is open at a time.
export const PANEL = `document.querySelector('[id^="ux-popover-panel"]')`;

/// A bar's layer, as `data-bar` names it, to the name its details trigger
/// carries ("Project details").
const BAR_NAMES = {
  project: "Project",
  connection: "Connection",
  access: "Access",
  firmware: "Firmware",
  hardware: "Hardware",
};

/// `devices/<board ref>` for `board`: a MAC (any case, with or without
/// colons) is `mac-<12 lowercase hex>` (`BoardRef`); a ref (`mac-…`,
/// `new-3`, `sim-…`, `emu-…`) or a whole `devices/…` path is taken as it is.
export function boardPath(board) {
  const text = String(board).trim();
  if (text.startsWith("devices/")) return text;
  const hex = text.replace(/:/g, "").toLowerCase();
  if (/^[0-9a-f]{12}$/.test(hex)) return `devices/mac-${hex}`;
  return `devices/${text}`;
}

/// The MAC (`aa:bb:cc:dd:ee:ff`) a `devices/mac-<hex>` path names, or null.
export function macOfPath(boardPathText) {
  const match = /mac-([0-9a-f]{12})$/.exec(boardPathText ?? "");
  return match ? match[1].match(/../g).join(":") : null;
}

/// The page-side expression for the card at `path`.
function cardSelector(path) {
  return `document.querySelector(${JSON.stringify(`[data-board-card="${path}"]`)})`;
}

/// The page-side expression for the page's only card: null while there are
/// none, or several.
const ONLY_CARD = `((() => { const all = document.querySelectorAll('[data-board-card]'); return all.length === 1 ? all[0] : null; })())`;

/// The `AgentMark` around a verb's control: `data-offer-path` ends with
/// `/<verb>`.
function offerSelector(verb) {
  return `[data-offer-path$="/${verb}"]`;
}

/// Page-side: the button that presses an `AgentMark`'s offer. The mark is
/// `display: contents`; its control is the last button inside it (an
/// `ActionButton`'s one button, a picker's trigger, or a choice's press
/// after its params).
const PRESSABLE = `((mark) => { if (!mark) return null; const all = mark.querySelectorAll('button'); return all.length ? all[all.length - 1] : null; })`;

/// Page-side: a bar's line as it reads — its pieces (summary, aside, or the
/// work's words) each trimmed and joined by one space, so "USB · connected"
/// and its aside "also cloud" read "USB · connected also cloud".
const LINE_TEXT = `((el) => [...el.children].map((c) => (c.textContent || '').replace(/\\s+/g, ' ').trim()).filter(Boolean).join(' '))`;

/// Page-side test for a text: includes a string, or matches a RegExp.
function textTest(words) {
  return words instanceof RegExp
    ? `((t) => new RegExp(${JSON.stringify(words.source)}, ${JSON.stringify(words.flags)}).test(t))`
    : `((t) => t.includes(${JSON.stringify(words)}))`;
}

/// The page-side half of every wait: a promise that a MutationObserver
/// settles. Injected once per document; see rule 2.
const WAIT_HELPER = `
window.__lpWait = window.__lpWait || function (source, timeoutMs) {
  const test = new Function("return (" + source + ")");
  return new Promise((resolve, reject) => {
    let done = false;
    const settle = (ok, value) => {
      if (done) return;
      done = true;
      observer.disconnect();
      clearTimeout(timer);
      ok ? resolve(value) : reject(new Error(value));
    };
    const check = () => {
      let value;
      try { value = test(); } catch (error) { return; }
      if (value) settle(true, JSON.stringify(value === true ? true : value));
    };
    // Every DOM change re-tests. No interval, no rAF: a throttled hidden tab
    // would turn either into a measurement of the throttle.
    const observer = new MutationObserver(check);
    observer.observe(document.documentElement, {
      subtree: true, childList: true, characterData: true, attributes: true,
    });
    const timer = setTimeout(() => settle(false, "wait deadline"), timeoutMs);
    check();
  });
};
true`;

export class StudioDriver {
  /// `profileDir`: a Chrome profile that OUTLIVES this browser (created if
  /// missing, never deleted on close) — what a walk needs when the page's
  /// OPFS must survive a closed tab (the migration walk's backup). Omitted,
  /// the profile is a fresh temporary one, removed on close.
  static async launch({ chrome = findChrome(), width = 1440, height = 1100, profileDir = null } = {}) {
    if (!chrome) throw new Error("no Chrome found; set CHROME_BIN");
    const userDataDir = profileDir ?? (await mkdtemp(path.join(tmpdir(), "lp-emu-walk-chrome-")));
    const child = spawn(
      chrome,
      [
        // Rule 1. Not negotiable.
        "--headless=new",
        "--disable-gpu",
        "--disable-dev-shm-usage",
        "--no-first-run",
        "--no-default-browser-check",
        // A headless page that Chrome thinks is hidden gets its timers
        // throttled; the walk's waits are MutationObserver-driven, but
        // Studio's own wasm loop is not, and a throttled Studio is a Studio
        // that never settles.
        "--disable-backgrounding-occluded-windows",
        "--disable-renderer-backgrounding",
        "--disable-background-timer-throttling",
        `--window-size=${width},${height}`,
        "--remote-debugging-port=0",
        `--user-data-dir=${userDataDir}`,
        "about:blank",
      ],
      { stdio: ["ignore", "ignore", "pipe"] },
    );
    const exited = once(child, "exit").catch(() => {});
    const cdp = await Cdp.open(await devToolsUrl(child));
    const { targetId } = await cdp.send("Target.createTarget", { url: "about:blank" });
    const { sessionId } = await cdp.send("Target.attachToTarget", { targetId, flatten: true });
    await cdp.send("Page.enable", {}, sessionId);
    await cdp.send("Runtime.enable", {}, sessionId);
    await cdp.send("Log.enable", {}, sessionId).catch(() => {});
    await cdp.send("Page.addScriptToEvaluateOnNewDocument", { source: WAIT_HELPER }, sessionId);
    // A headless target Chrome considers unfocused is throttled the way a
    // background tab is, and the command-line flags above do not reach it —
    // they are about backgrounded WINDOWS. This is the one that reaches a
    // CDP-created target, and it matters far more now that the page may be
    // hosting an emulator: a throttled Worker runs the guest at a fraction
    // of a per cent of real time, which reads as a board that never answered.
    await cdp.send("Emulation.setFocusEmulationEnabled", { enabled: true }, sessionId).catch(() => {});
    return new StudioDriver({ cdp, sessionId, child, exited, userDataDir, keepProfile: profileDir !== null });
  }

  constructor({ cdp, sessionId, child, exited, userDataDir, keepProfile = false }) {
    this.keepProfile = keepProfile;
    this.cdp = cdp;
    this.sessionId = sessionId;
    this.child = child;
    this.exited = exited;
    this.userDataDir = userDataDir;
    this.console = [];
    cdp.on("Runtime.consoleAPICalled", (params, sid) => {
      if (sid !== sessionId) return;
      const text = (params.args ?? []).map((arg) => arg.value ?? arg.description ?? arg.type).join(" ");
      this.console.push({ level: params.type, text });
    });
    cdp.on("Runtime.exceptionThrown", (params, sid) => {
      if (sid !== sessionId) return;
      this.console.push({ level: "exception", text: params.exceptionDetails?.text ?? "exception" });
    });
  }

  /// `loadTimeoutMs` bounds the wait for the page's `load` event, and it is a
  /// wedged-run guard rather than a measurement. It is NOT the CDP default of
  /// 30 s: Studio's `<script type="module">` glue holds `load` back until the
  /// ~105 MB debug wasm has been fetched and compiled, and on a desk where
  /// sibling worktrees are building that alone has taken longer than 30 s —
  /// which then read as "CDP Runtime.evaluate timed out" before the walk had
  /// touched the board at all (measured 2026-09-11, tab lane). The walk gives
  /// Studio's arrival its own deadline for the same reason; this is the same
  /// deadline applied one step earlier.
  async navigate(url, { loadTimeoutMs = 420_000 } = {}) {
    await this.cdp.send("Page.navigate", { url }, this.sessionId);
    // The load event, not a sleep: `Page.navigate` resolves on commit.
    await this.evaluate(
      `new Promise((r) => document.readyState === "complete"
         ? r(true) : window.addEventListener("load", () => r(true), { once: true }))`,
      { awaitPromise: true, timeoutMs: loadTimeoutMs },
    );
    await this.evaluate(WAIT_HELPER);
  }

  async evaluate(expression, { awaitPromise = false, timeoutMs = CDP_CALL_TIMEOUT_MS } = {}) {
    const { result, exceptionDetails } = await this.cdp.send(
      "Runtime.evaluate",
      { expression, returnByValue: true, awaitPromise },
      this.sessionId,
      timeoutMs,
    );
    if (exceptionDetails) {
      throw new Error(`page evaluation failed: ${exceptionDetails.exception?.description ?? exceptionDetails.text}`);
    }
    return result.value;
  }

  /// Wait for a page-side predicate. `source` is a JS expression evaluated in
  /// the page on every DOM mutation; the returned value is its result.
  async waitFor(source, { timeoutMs = DEFAULT_WAIT_MS, what = source } = {}) {
    try {
      const json = await this.evaluate(
        `window.__lpWait(${JSON.stringify(source)}, ${timeoutMs})`,
        { awaitPromise: true, timeoutMs: timeoutMs + 5_000 },
      );
      return JSON.parse(json);
    } catch (error) {
      throw new Error(`waiting for ${what}: ${error.message}`);
    }
  }

  // --- Studio's affordances, by their visible text -------------------------

  /// Every clickable control the page is showing, with its text — the probe
  /// an agent runs when Studio's wording moves. `scope` is a page-side
  /// expression for the element to look under (`document` by default); one
  /// that evaluates to nothing lists no controls.
  async controls({ scope = "document" } = {}) {
    return this.evaluate(`
      [...(${scope} ?? document.createElement('div')).querySelectorAll('button, [role="button"], a')]
        .filter((el) => el.offsetParent !== null || el.closest('#lp-emu-picker'))
        .map((el) => ({
          tag: el.tagName.toLowerCase(),
          text: (el.textContent || '').replace(/\\s+/g, ' ').trim().slice(0, 60),
          id: el.id || null,
          cls: (el.className && el.className.baseVal !== undefined ? el.className.baseVal : el.className || '').toString().slice(0, 60),
          disabled: el.disabled === true,
        }))
        .filter((el) => el.text.length > 0)
    `);
  }

  /// Click the first enabled, visible control whose text contains `text`.
  /// Throws with the full control list when there is none — a rename should
  /// read as a rename, not as a timeout.
  /// `exact` keeps only controls with a leaf element whose whole text is
  /// `text`, so "PLAYFUL Choker" does not also pick "PLAYFUL Choker Tryout".
  async click(text, { scope = "document", nth = 0, exact = false } = {}) {
    const clicked = await this.evaluate(`
      (() => {
        const el = ${matchingControls(text, { scope, exact })}[${nth}];
        if (!el) return null;
        el.scrollIntoView({ block: 'center' });
        el.click();
        return (el.textContent || '').replace(/\\s+/g, ' ').trim();
      })()
    `);
    if (clicked === null) {
      const controls = await this.controls();
      throw new Error(
        `no enabled control matching ${JSON.stringify(text)}. Visible controls:\n` +
          controls.map((control) => `  [${control.disabled ? "x" : " "}] ${control.text}`).join("\n"),
      );
    }
    return clicked;
  }

  /// Type `text` into the first visible text field under `scope` whose
  /// placeholder contains `placeholder`, as keystrokes would: the field is
  /// focused and cleared, then the text goes in through CDP's
  /// `Input.insertText`, so the page sees real `input` events.
  async type(placeholder, text, { scope = "document" } = {}) {
    const found = await this.evaluate(`
      (() => {
        const wanted = ${JSON.stringify(placeholder.toLowerCase())};
        const el = [...${scope}.querySelectorAll('input[type="text"], input:not([type])')]
          .find((el) => (el.placeholder || '').toLowerCase().includes(wanted));
        if (!el) return false;
        el.scrollIntoView({ block: 'center' });
        el.focus();
        el.select();
        return true;
      })()
    `);
    if (!found) throw new Error(`no text field with a placeholder like ${JSON.stringify(placeholder)}`);
    await this.cdp.send("Input.insertText", { text }, this.sessionId);
  }

  /// Put `paths` (absolute, on this machine) into the file input `selector`
  /// matches, as a person picking them in the file dialog would: CDP's
  /// `DOM.setFileInputFiles`, which fires the input's change event.
  async setFiles(selector, paths) {
    const { root } = await this.cdp.send("DOM.getDocument", { depth: -1, pierce: true }, this.sessionId);
    const { nodeId } = await this.cdp.send("DOM.querySelector", { nodeId: root.nodeId, selector }, this.sessionId);
    if (!nodeId) throw new Error(`no file input matches ${selector}`);
    await this.cdp.send("DOM.setFileInputFiles", { nodeId, files: paths }, this.sessionId);
  }

  /// Wait for the control, then click it. The wait is the page's, not ours,
  /// and it asks the question `click` will ask: the same `scope`, `exact` and
  /// `nth`, so it cannot be satisfied by one control while the click finds
  /// another (or none). Anything else in `options` (`timeoutMs`) is the wait's.
  async clickWhenReady(text, options = {}) {
    const { scope = "document", nth = 0, exact = false, ...wait } = options;
    await this.waitFor(
      `${matchingControls(text, { scope, exact })}.length > ${nth}`,
      { what: `the control ${JSON.stringify(text)}${exact ? " (exact)" : ""}`, ...wait },
    );
    return this.click(text, { scope, nth, exact });
  }

  /// Press one of the home page's Connect a board squares — `"USB"`,
  /// `"Bluetooth"` or `"Network"` — by its whole word, inside
  /// `#home-connect-board`. A bare substring would also match a board card's
  /// "USB · live" Connection bar or its Bluetooth switch; this is the one
  /// place a walk names a square. Waits for the square to be enabled.
  async pressConnect(word, { timeoutMs = DEFAULT_WAIT_MS } = {}) {
    const scope = CONNECT_SCOPE;
    try {
      return await this.clickWhenReady(word, { scope, exact: true, timeoutMs });
    } catch (error) {
      const controls = await this.controls({ scope });
      throw new Error(
        `no enabled ${JSON.stringify(word)} square in #home-connect-board (${error.message.split("\n")[0]}). ` +
          (controls.length === 0
            ? "The section is not on the page, or holds no controls."
            : "Controls in the section:\n" +
              controls.map((control) => `  [${control.disabled ? "x" : " "}] ${control.text}`).join("\n")),
      );
    }
  }

  // --- the board card, by its hooks ----------------------------------------
  //
  // Every helper here takes `{ board }`: a MAC (any case, colons or not), a
  // board ref (`mac-…`, `new-3`, `sim-…`, `emu-…`) or a whole
  // `devices/<ref>` path ([`boardPath`]). Omitted, it means the page's only
  // card, and the helper throws when there are several — a walk with more
  // than one board names the one it means.

  /// The page-side expression for one board's card (an element or null),
  /// for use as a `scope`. Throws when `board` is omitted and the page holds
  /// several cards.
  async card({ board = null } = {}) {
    if (board != null) return cardSelector(boardPath(board));
    const paths = await this.cardPaths();
    if (paths.length > 1) {
      throw new Error(`there are ${paths.length} board cards (${paths.join(", ")}); name the board`);
    }
    return ONLY_CARD;
  }

  /// Every board card's `devices/<ref>`, in page order.
  async cardPaths() {
    return this.evaluate(
      `[...document.querySelectorAll('[data-board-card]')].map((el) => el.getAttribute('data-board-card'))`,
    );
  }

  /// Wait for the board's card to be on the page.
  async waitCard({ board = null, timeoutMs = DEFAULT_WAIT_MS } = {}) {
    const scope = await this.card({ board });
    await this.waitFor(`Boolean(${scope})`, { timeoutMs, what: `the card of ${board ?? "the board"}` });
    return this.evaluate(`${scope}.getAttribute('data-board-card')`);
  }

  /// Wait until core publishes `verb` on the board's card — its offer at
  /// `devices/<ref>/<verb>` is drawn (`data-offer-path`), and with
  /// `enabled` its button can be pressed. A published offer is core's
  /// reading of the board's state, so this stands where a wait on "Ready"
  /// stood. A verb that lives in a bar's details is drawn only while they
  /// are open: name the `bar` and they are opened first.
  async waitOffer(verb, { board = null, bar = null, enabled = true, timeoutMs = DEFAULT_WAIT_MS } = {}) {
    if (bar) await this.openBar(bar, { board, timeoutMs });
    const scope = await this.card({ board });
    await this.waitFor(
      `(() => { const card = ${scope}; if (!card) return false;
                const button = ${PRESSABLE}(card.querySelector(${JSON.stringify(offerSelector(verb))}));
                return Boolean(button) && (${!enabled} || !button.disabled); })()`,
      { timeoutMs, what: `the offer \`${verb}\`${enabled ? " (enabled)" : ""} on ${board ?? "the card"}` },
    );
  }

  /// Whether `verb` is drawn on the card right now (no wait).
  async offered(verb, { board = null, enabled = false } = {}) {
    const scope = await this.card({ board });
    return this.evaluate(
      `(() => { const card = ${scope}; if (!card) return false;
                const button = ${PRESSABLE}(card.querySelector(${JSON.stringify(offerSelector(verb))}));
                return Boolean(button) && (${!enabled} || !button.disabled); })()`,
    );
  }

  /// Open a bar's details ("project", "connection", "access", "firmware",
  /// "hardware"): any other details on the card are closed first — one
  /// popover at a time. Waits for the details card to be drawn. The details
  /// render inside the card's DOM, so reads stay scoped to the card.
  async openBar(layer, { board = null, timeoutMs = DEFAULT_WAIT_MS } = {}) {
    const name = BAR_NAMES[layer];
    if (!name) throw new Error(`no bar ${JSON.stringify(layer)}; bars are ${Object.keys(BAR_NAMES).join(", ")}`);
    return this.openDetails(`[data-bar="${layer}"] button[aria-label="${name} details"]`, { board, timeoutMs, what: `the ${layer} bar` });
  }

  /// Open the status corner's details: notices, how the board is running,
  /// the picture's words, and the board's terminal.
  async openCorner({ board = null, timeoutMs = DEFAULT_WAIT_MS } = {}) {
    return this.openDetails(`[data-board-corner] button[aria-label="Status details"]`, { board, timeoutMs, what: "the status corner" });
  }

  async openDetails(triggerSelector, { board, timeoutMs, what }) {
    const scope = await this.card({ board });
    const trigger = `${scope}?.querySelector(${JSON.stringify(triggerSelector)})`;
    await this.waitFor(`Boolean(${trigger})`, { timeoutMs, what });
    if ((await this.evaluate(`${trigger}.getAttribute('aria-expanded')`)) === "true") return;
    await this.closeDetails({ board });
    await this.evaluate(`(() => { const el = ${trigger}; el.scrollIntoView({ block: 'center' }); el.click(); return true; })()`);
    await this.waitFor(
      `(() => { const el = ${trigger}; return Boolean(el) && el.getAttribute('aria-expanded') === 'true'
                && Boolean(el.parentElement.querySelector('[id^="ux-popover-panel"]')); })()`,
      { timeoutMs, what: `${what}'s details to open` },
    );
  }

  /// Close every popover open on the card — a bar's details, the corner's,
  /// or a picker — by its own trigger, and wait until none is open.
  async closeDetails({ board = null, timeoutMs = 30_000 } = {}) {
    const scope = await this.card({ board });
    const open = `[...(${scope}?.querySelectorAll('button[aria-expanded="true"]') ?? [])]`;
    const closed = await this.evaluate(`(() => { const all = ${open}; all.forEach((el) => el.click()); return all.length; })()`);
    if (closed > 0) {
      await this.waitFor(`${open}.length === 0`, { timeoutMs, what: "the card's open details to close" });
    }
    return closed;
  }

  /// Press the offer at `devices/<ref>/<verb>` on the card: its own button,
  /// in the card's word. `bar` opens that bar's details first, when the verb
  /// lives there. A Lasting verb arms on its first click; `confirm` clicks
  /// it again, which is the press. A pick (the project or board pick) opens
  /// its picker; the caller picks inside it (`PANEL`). Returns the button's
  /// words.
  async pressOffer(verb, { board = null, bar = null, confirm = false, timeoutMs = DEFAULT_WAIT_MS } = {}) {
    // Without a bar the verb is on the card's face: whatever details are
    // open close first, so the press never opens a second popover.
    if (!bar) await this.closeDetails({ board });
    await this.waitOffer(verb, { board, bar, timeoutMs });
    const scope = await this.card({ board });
    const press = `(() => { const button = ${PRESSABLE}(${scope}.querySelector(${JSON.stringify(offerSelector(verb))}));
                            if (!button || button.disabled) return null;
                            button.scrollIntoView({ block: 'center' }); button.click();
                            return (button.textContent || '').replace(/\\s+/g, ' ').trim(); })()`;
    const pressed = await this.evaluate(press);
    if (pressed === null) throw new Error(`the offer \`${verb}\` went away before it could be pressed`);
    if (confirm) {
      const again = await this.evaluate(press);
      if (again === null) throw new Error(`the offer \`${verb}\` went away between its arm and its press`);
    }
    return pressed;
  }

  /// A bar's line as it reads: its summary and aside, or its work's words
  /// while it carries work. `null` when the card or the bar is not there.
  async barText(layer, { board = null } = {}) {
    const scope = await this.card({ board });
    return this.evaluate(`(() => { const el = ${scope}?.querySelector(${JSON.stringify(`[data-bar="${layer}"] button[aria-label="${BAR_NAMES[layer]} details"]`)});
      return el ? ${LINE_TEXT}(el) : null; })()`);
  }

  /// Wait until a bar's line includes `words` (a string) or matches it (a
  /// RegExp). Returns the line.
  async waitBar(layer, words, { board = null, timeoutMs = DEFAULT_WAIT_MS } = {}) {
    const scope = await this.card({ board });
    const test = textTest(words);
    return this.waitFor(
      `(() => { const el = ${scope}?.querySelector(${JSON.stringify(`[data-bar="${layer}"] button[aria-label="${BAR_NAMES[layer]} details"]`)});
                if (!el) return false; const t = ${LINE_TEXT}(el);
                return ${test}(t) ? t : false; })()`,
      { timeoutMs, what: `the ${layer} bar to read ${words}` },
    );
  }

  /// One fact in a bar's details ("Version" in the firmware details, "Id"
  /// in the hardware details): opens them, reads the value, closes them.
  /// `null` when the details carry no such fact.
  async detailsFact(layer, label, { board = null, timeoutMs = DEFAULT_WAIT_MS } = {}) {
    await this.openBar(layer, { board, timeoutMs });
    const scope = await this.card({ board });
    try {
      return await this.evaluate(`(() => { const card = ${scope}; if (!card) return null;
        const wanted = ${JSON.stringify(label.toLowerCase())};
        const dt = [...card.querySelectorAll('[id^="ux-popover-panel"] dt')]
          .find((el) => (el.textContent || '').trim().toLowerCase() === wanted);
        const dd = dt?.nextElementSibling;
        return dd ? (dd.textContent || '').replace(/\\s+/g, ' ').trim() : null; })()`);
    } finally {
      await this.closeDetails({ board });
    }
  }

  /// A bar's work: `"running"`, `"done"`, `"failed"`, or `"none"` (no work),
  /// off its `data-bar-work`; `null` when the card is not there.
  async workState(layer, { board = null } = {}) {
    const scope = await this.card({ board });
    return this.evaluate(`(() => { const card = ${scope}; if (!card) return null;
      const bar = card.querySelector('[data-bar="${layer}"]'); if (!bar) return null;
      return bar.getAttribute('data-bar-work') || 'none'; })()`);
  }

  /// Wait until a bar's work is one of `states` (a state or a list of them,
  /// as `workState` names them). Returns the state.
  async waitWork(layer, states, { board = null, timeoutMs = DEFAULT_WAIT_MS } = {}) {
    const wanted = Array.isArray(states) ? states : [states];
    const scope = await this.card({ board });
    return this.waitFor(
      `(() => { const card = ${scope}; if (!card) return false;
                const bar = card.querySelector('[data-bar="${layer}"]'); if (!bar) return false;
                const state = bar.getAttribute('data-bar-work') || 'none';
                return ${JSON.stringify(wanted)}.includes(state) ? state : false; })()`,
      { timeoutMs, what: `the ${layer} bar's work to be ${wanted.join(" or ")}` },
    );
  }

  /// The board's own terminal lines (its corner's details, `data-board-terminal`),
  /// one string per row. Opens the corner, reads, and closes it again.
  async terminalLines({ board = null, timeoutMs = DEFAULT_WAIT_MS } = {}) {
    await this.openCorner({ board, timeoutMs });
    const scope = await this.card({ board });
    try {
      return await this.evaluate(`(() => { const term = ${scope}?.querySelector('[data-board-terminal]');
        return term ? [...term.children].map((row) => (row.textContent || '').replace(/\\s+/g, ' ').trim()) : []; })()`);
    } finally {
      await this.closeDetails({ board });
    }
  }

  /// THE BOARD'S OWN WORDS: open the status corner's details and wait for
  /// the board's terminal to hold `words` (a string, or a RegExp), then
  /// close them. Returns the line that said it.
  async boardSaid(words, { board = null, timeoutMs = DEFAULT_WAIT_MS } = {}) {
    await this.openCorner({ board, timeoutMs });
    const scope = await this.card({ board });
    const test = textTest(words);
    try {
      return await this.waitFor(
        `(() => { const term = ${scope}?.querySelector('[data-board-terminal]'); if (!term) return false;
                  const rows = [...term.children].map((row) => (row.textContent || '').replace(/\\s+/g, ' ').trim());
                  return rows.find((row) => ${test}(row)) || false; })()`,
        { timeoutMs, what: `the board to say ${words}` },
      );
    } finally {
      await this.closeDetails({ board }).catch(() => {});
    }
  }

  /// Studio's own flash flow on a blank board's card, the way a person does
  /// it: the firmware bar says "No firmware", the primary Install (`flash`)
  /// opens the board pick, picking `model` presses it, and the flash is the
  /// firmware bar's work — it must START and then FINISH (a wait on "No
  /// firmware went away" alone is satisfied the instant the work starts).
  /// Finished means the work is gone and the bar no longer says "No
  /// firmware"; the board's own state (its backing's registry) is the
  /// caller's to check.
  async flashBlank(model, { board = null, timeoutMs = DEFAULT_WAIT_MS, flashTimeoutMs = 900_000 } = {}) {
    await this.waitBar("firmware", "No firmware", { board, timeoutMs });
    await this.pressOffer("flash", { board, timeoutMs });
    await this.waitFor(`Boolean(${PANEL})`, { timeoutMs, what: "the board-model picker" });
    // Picking the model is the press: the board pick in verb mode.
    await this.click(model, { scope: PANEL });
    await this.waitWork("firmware", "running", { board, timeoutMs });
    const scope = await this.card({ board });
    await this.waitFor(
      `(() => { const bar = ${scope}?.querySelector('[data-bar="firmware"]');
                if (!bar || bar.getAttribute('data-bar-work') === 'running') return false;
                return !(bar.textContent || '').includes('No firmware'); })()`,
      { timeoutMs: flashTimeoutMs, what: "the flash to finish" },
    );
  }

  /// What the board says it runs, as core reads it: `"empty"` once its
  /// project bar offers `push` ("Add a project"), `"running"` once its
  /// primary Edit can be pressed. Waits for one of the two — the card is
  /// ready either way.
  async boardRuns({ board = null, timeoutMs = DEFAULT_WAIT_MS } = {}) {
    const scope = await this.card({ board });
    return this.waitFor(
      `(() => { const card = ${scope}; if (!card) return false;
                if (${PRESSABLE}(card.querySelector('[data-bar="project"] [data-offer-path$="/push"]'))) return 'empty';
                const edit = ${PRESSABLE}(card.querySelector('[data-offer-path$="/edit"]'));
                return edit && !edit.disabled ? 'running' : false; })()`,
      { timeoutMs, what: `the board to say what it runs (\`push\` or \`edit\` offered)` },
    );
  }

  // --- the shim's own page contract ---------------------------------------

  /// `window.__lpEmuSerial.bus.describeBoards()` — what the page's chrome
  /// knows. NOT evidence of flash state: the bus caches `GET /boards` at load
  /// (M5 finding), so the door's live registry is the truth about flash.
  async boards() {
    return this.evaluate(`window.__lpEmuSerial ? window.__lpEmuSerial.bus.describeBoards() : null`);
  }

  async awaitShim({ timeoutMs = DEFAULT_WAIT_MS } = {}) {
    await this.waitFor("Boolean(window.__lpEmuSerial)", { timeoutMs, what: "the shim to install" });
    return this.boards();
  }

  /// The cable comes out / goes back in. Rule 3: through the bus.
  async detach(boardId) {
    return this.evaluate(
      `window.__lpEmuSerial.bus.detach(${JSON.stringify(boardId)}).then(() => "detached")`,
      { awaitPromise: true },
    );
  }

  async attach(boardId) {
    return this.evaluate(
      `window.__lpEmuSerial.bus.attach(${JSON.stringify(boardId)}).then(() => "attached")`,
      { awaitPromise: true },
    );
  }

  /// The in-page chooser standing where Chrome's would be. `requestPort()`
  /// is already in flight when this is called — Studio called it — so the
  /// wait is for the overlay, and the click is the answer.
  async pickBoard(boardId, { timeoutMs = DEFAULT_WAIT_MS } = {}) {
    // Built with `CSS.escape` in the page rather than by string-splicing a
    // selector here: a board id is data, and the first attempt at this
    // spliced a quote into the predicate and failed as a SyntaxError that
    // read like "the picker never opened".
    const finder =
      `document.querySelector('#lp-emu-picker')` +
      `?.querySelector('.lp-emu-picker-board[data-board-id=' + CSS.escape(${JSON.stringify(boardId)}) + ']')`;
    await this.waitFor(`Boolean(${finder})`, { timeoutMs, what: `the picker to offer ${boardId}` });
    const picked = await this.evaluate(`
      (() => {
        const el = ${finder};
        if (!el) return null;
        el.click();
        return el.dataset.boardId;
      })()
    `);
    if (!picked) throw new Error(`the picker had no row for ${boardId}`);
    return picked;
  }

  async pickerIsOpen() {
    return this.evaluate(`Boolean(document.querySelector('#lp-emu-picker'))`);
  }

  async screenshot(file) {
    const { data } = await this.cdp.send(
      "Page.captureScreenshot",
      { format: "png", captureBeyondViewport: false },
      this.sessionId,
      60_000,
    );
    writeFileSync(file, Buffer.from(data, "base64"));
    return file;
  }

  consoleLines(filter = null) {
    return this.console
      .filter((line) => !filter || line.text.includes(filter))
      .map((line) => `[${line.level}] ${line.text}`);
  }

  async close() {
    try {
      try { await this.cdp.send("Browser.close"); } catch { this.cdp.close(); }
    } finally {
      // Kill by pid, only what we started (standing rule). Never `pkill -f`.
      if (this.child.exitCode === null) this.child.kill("SIGTERM");
      if (this.child.exitCode === null) await Promise.race([this.exited, delay(5_000)]);
      if (this.child.exitCode === null) {
        this.child.kill("SIGKILL");
        await Promise.race([this.exited, delay(2_000)]);
      }
      try {
        if (!this.keepProfile) {
          await rm(this.userDataDir, { recursive: true, force: true, maxRetries: 10, retryDelay: 100 });
        }
      } catch (error) {
        console.warn(`warning: could not remove ${this.userDataDir}: ${error}`);
      }
    }
  }
}

function devToolsUrl(child) {
  return new Promise((resolve, reject) => {
    let buffered = "";
    const timer = setTimeout(() => reject(new Error("Chrome did not report a DevTools endpoint")), 30_000);
    child.stderr.setEncoding("utf8");
    child.stderr.on("data", (chunk) => {
      buffered += chunk;
      const match = buffered.match(/ws:\/\/[^\s]+/);
      if (match) {
        clearTimeout(timer);
        resolve(match[0]);
      }
    });
    child.once("exit", (code) => {
      clearTimeout(timer);
      reject(new Error(`Chrome exited with ${code} before reporting a DevTools endpoint`));
    });
  });
}

function delay(ms) {
  return new Promise((resolve) => {
    const timer = setTimeout(resolve, ms);
    timer.unref?.();
  });
}

// --- `node scripts/emu/studio-driver.mjs probe <url>` ----------------------
//
// The discovery command: open a URL headless, print the shim's boards and
// every visible control, and save a screenshot. This is how you find out what
// Studio calls a button today.
if (import.meta.url === `file://${process.argv[1]}`) {
  const [, , command, url, shot] = process.argv;
  if (command !== "probe" || !url) {
    console.error("usage: node scripts/emu/studio-driver.mjs probe <url> [screenshot.png]");
    process.exit(2);
  }
  const driver = await StudioDriver.launch();
  try {
    await driver.navigate(url);
    await driver.waitFor("Boolean(document.querySelector('#main')?.children.length)", {
      what: "Studio to render",
    }).catch((error) => console.error(`(${error.message})`));
    const boards = await driver.boards();
    console.log("shim boards:", JSON.stringify(boards, null, 2));
    console.log("controls:");
    for (const control of await driver.controls()) {
      console.log(`  [${control.disabled ? "x" : " "}] <${control.tag}> ${JSON.stringify(control.text)}  id=${control.id} cls=${control.cls}`);
    }
    if (shot) console.log(`screenshot → ${await driver.screenshot(shot)}`);
    const noise = driver.consoleLines();
    if (noise.length) {
      console.log("console:");
      for (const line of noise.slice(-40)) console.log(`  ${line}`);
    }
  } finally {
    await driver.close();
  }
}
