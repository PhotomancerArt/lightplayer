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
// connected", "Flash firmware", "Put it on the board"). That is deliberate:
// it is what the spec's `manual:` steps already say, so the emulated lane and
// the silicon lane are describing the same click.

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
  static async launch({ chrome = findChrome(), width = 1440, height = 1100 } = {}) {
    if (!chrome) throw new Error("no Chrome found; set CHROME_BIN");
    const userDataDir = await mkdtemp(path.join(tmpdir(), "lp-emu-walk-chrome-"));
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
    return new StudioDriver({ cdp, sessionId, child, exited, userDataDir });
  }

  constructor({ cdp, sessionId, child, exited, userDataDir }) {
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

  async navigate(url) {
    await this.cdp.send("Page.navigate", { url }, this.sessionId);
    // The load event, not a sleep: `Page.navigate` resolves on commit.
    await this.evaluate(
      `new Promise((r) => document.readyState === "complete"
         ? r(true) : window.addEventListener("load", () => r(true), { once: true }))`,
      { awaitPromise: true },
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
  /// an agent runs when Studio's wording moves.
  async controls() {
    return this.evaluate(`
      [...document.querySelectorAll('button, [role="button"], a')]
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
  async click(text, { scope = "document", nth = 0 } = {}) {
    const clicked = await this.evaluate(`
      (() => {
        const wanted = ${JSON.stringify(text.toLowerCase())};
        const all = [...${scope}.querySelectorAll('button, [role="button"], a')]
          .filter((el) => !el.disabled)
          .filter((el) => (el.textContent || '').replace(/\\s+/g, ' ').trim().toLowerCase().includes(wanted));
        const el = all[${nth}];
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

  /// Wait for the control, then click it. The wait is the page's, not ours.
  async clickWhenReady(text, options = {}) {
    await this.waitFor(
      `[...document.querySelectorAll('button, [role="button"], a')]
         .some((el) => !el.disabled && (el.textContent||'').replace(/\\s+/g,' ').trim().toLowerCase()
           .includes(${JSON.stringify(text.toLowerCase())}))`,
      { what: `the control ${JSON.stringify(text)}`, ...options },
    );
    return this.click(text, options);
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
        await rm(this.userDataDir, { recursive: true, force: true, maxRetries: 10, retryDelay: 100 });
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
