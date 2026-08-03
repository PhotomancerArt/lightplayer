#!/usr/bin/env node
// Headless pass/fail run of the fw-browser smoke page.
//
// `just fw-browser-smoke` serves the same page for a human to look at, and it
// reports failure only in the page itself — a rotted page (a renamed protocol
// variant, a moved response shape) just renders "error" while the recipe sits
// there serving happily. This script drives the page in headless Chrome and
// exits non-zero unless the page reaches `dataset.smoke === "ok"`, so the gate
// fails loudly in a terminal or in CI.
//
// Chrome is driven over CDP with node's built-in WebSocket, the same
// dependency-free approach as studio-story-pngs.mjs.

import { spawn } from "node:child_process";
import { once } from "node:events";
import { createReadStream, existsSync } from "node:fs";
import { mkdtemp, rm } from "node:fs/promises";
import { createServer as createHttpServer } from "node:http";
import { createServer as createTcpServer } from "node:net";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

class CdpConnection {
  static async open(wsUrl) {
    const ws = new WebSocket(wsUrl);
    await new Promise((resolve, reject) => {
      ws.addEventListener("open", resolve, { once: true });
      ws.addEventListener("error", reject, { once: true });
    });
    return new CdpConnection(ws);
  }

  constructor(ws) {
    this.nextId = 1;
    this.pending = new Map();
    this.ws = ws;
    this.ws.addEventListener("message", (event) => this.onMessage(event));
    this.ws.addEventListener("close", () => this.rejectAll(new Error("Chrome DevTools closed")));
    this.ws.addEventListener("error", () => this.rejectAll(new Error("Chrome DevTools failed")));
  }

  send(method, params = {}, sessionId = undefined) {
    if (this.ws.readyState !== WebSocket.OPEN) {
      return Promise.reject(new Error(`Chrome DevTools connection is closed (${method})`));
    }
    const id = this.nextId;
    this.nextId += 1;
    const message = { id, method, params };
    if (sessionId) {
      message.sessionId = sessionId;
    }
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        if (this.pending.delete(id)) {
          reject(new Error(`CDP ${method} timed out after ${cdpCallTimeoutMs}ms`));
        }
      }, cdpCallTimeoutMs);
      timer.unref?.();
      this.pending.set(id, {
        resolve: (value) => {
          clearTimeout(timer);
          resolve(value);
        },
        reject: (error) => {
          clearTimeout(timer);
          reject(error);
        },
      });
      this.ws.send(JSON.stringify(message));
    });
  }

  close() {
    this.ws.close();
  }

  onMessage(event) {
    const message = JSON.parse(event.data.toString());
    if (!message.id) {
      return;
    }
    const pending = this.pending.get(message.id);
    if (!pending) {
      return;
    }
    this.pending.delete(message.id);
    if (message.error) {
      pending.reject(new Error(`${message.error.message}: ${message.error.data ?? ""}`));
    } else {
      pending.resolve(message.result ?? {});
    }
  }

  rejectAll(error) {
    for (const pending of this.pending.values()) {
      pending.reject(error);
    }
    this.pending.clear();
  }
}

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const wwwDir = path.resolve(scriptDir, "../www");
const smokeTimeoutMs = Number(process.env.FW_BROWSER_SMOKE_TIMEOUT_MS ?? "90000");
const cdpCallTimeoutMs = 30_000;

if (!existsSync(path.join(wwwDir, "pkg", "fw_browser.js"))) {
  fail(`fw-browser wasm bundle is missing under ${wwwDir}/pkg — run 'just fw-browser-build' first.`);
}

const chrome = process.env.CHROME_BIN ?? findChrome();
if (!chrome) {
  fail("Could not find Google Chrome. Set CHROME_BIN=/path/to/chrome.");
}

// Port 0: nothing reconnects to this server, and a fixed port would collide
// when several worktrees run the check at once.
const port = await findFreePort();
const server = createStaticServer(wwwDir);
await new Promise((resolve, reject) => {
  server.once("error", reject);
  server.listen(port, "127.0.0.1", resolve);
});

const url = `http://127.0.0.1:${port}/smoke.html`;
let browser;
try {
  browser = await launchBrowser();
  const result = await runSmoke(browser, url);
  report(result);
  if (result.smoke !== "ok") {
    process.exitCode = 1;
  }
} finally {
  // Teardown must never overturn the verdict: the smoke result is decided
  // and reported above, and `process.exitCode` is already set. Letting a
  // cleanup throw escape turned a PASSING CI run red (ENOTEMPTY unlinking
  // the Chrome profile). Warn instead — the temp dir is under the OS temp
  // root, which CI reclaims regardless.
  try {
    await browser?.close();
  } catch (error) {
    console.warn(`warning: browser teardown failed (smoke verdict stands): ${error}`);
  }
  server.close();
}

async function runSmoke(browser, pageUrl) {
  const { cdp, sessionId } = browser;
  await cdp.send("Page.navigate", { url: pageUrl }, sessionId);

  const deadline = Date.now() + smokeTimeoutMs;
  let last = { smoke: undefined };
  // An evaluation exception inside the polling window means "not readable
  // YET" — the first evaluate can land while navigation is still replacing
  // the execution context (CDP reports a bare `Uncaught`), and a loaded CI
  // runner widens that window. Only a deadline expiry raises it, as the
  // timeout's evidence.
  let lastError = null;
  while (Date.now() < deadline) {
    try {
      last = await readPageState(cdp, sessionId);
      lastError = null;
    } catch (error) {
      lastError = error;
      await delay(250);
      continue;
    }
    if (last.smoke === "ok" || last.smoke === "error") {
      return last;
    }
    await delay(250);
  }
  if (lastError) {
    throw new Error(`page never became readable before the deadline: ${lastError.message}`);
  }
  return { ...last, smoke: last.smoke ?? "timeout" };
}

// The page publishes its verdict on `documentElement.dataset.smoke` and keeps a
// human-readable transcript in `#log`; both come back so a failure prints the
// same evidence a person would read off the served page.
async function readPageState(cdp, sessionId) {
  const { result, exceptionDetails } = await cdp.send(
    "Runtime.evaluate",
    {
      expression: `JSON.stringify({
        smoke: document.documentElement.dataset.smoke ?? null,
        status: document.getElementById('status')?.textContent ?? null,
        checks: [...document.querySelectorAll('.check-item input')]
          .map((input) => ({ id: input.id.replace(/^check-/, ''), done: input.checked })),
        log: document.getElementById('log')?.textContent ?? '',
      })`,
      returnByValue: true,
    },
    sessionId,
  );
  if (exceptionDetails) {
    throw new Error(`page evaluation failed: ${exceptionDetails.text}`);
  }
  return JSON.parse(result.value);
}

function report(state) {
  const checks = state.checks ?? [];
  for (const check of checks) {
    console.log(`${check.done ? "PASS" : "FAIL"} ${check.id}`);
  }
  if (state.smoke === "ok") {
    console.log(`fw-browser smoke passed (${state.status ?? "no status"}).`);
    return;
  }
  console.error("");
  console.error(`fw-browser smoke ${state.smoke} (status: ${state.status ?? "none"}).`);
  const log = (state.log ?? "").split("\n").slice(-25).join("\n");
  if (log.trim()) {
    console.error("Last worker log lines:");
    console.error(log);
  }
  console.error("");
  console.error(`Reproduce interactively with: just fw-browser-smoke`);
}

async function launchBrowser() {
  const userDataDir = await mkdtemp(path.join(tmpdir(), "fw-browser-smoke-chrome-"));
  const child = spawn(
    chrome,
    [
      "--headless=new",
      "--disable-gpu",
      // Shared-memory transport can exhaust /dev/shm on Linux CI runners.
      "--disable-dev-shm-usage",
      "--no-first-run",
      "--no-default-browser-check",
      "--remote-debugging-port=0",
      `--user-data-dir=${userDataDir}`,
      "about:blank",
    ],
    { stdio: ["ignore", "ignore", "pipe"] },
  );
  const childExited = once(child, "exit").catch(() => {});
  const cdp = await CdpConnection.open(await waitForDevTools(child));
  const { targetId } = await cdp.send("Target.createTarget", { url: "about:blank" });
  const { sessionId } = await cdp.send("Target.attachToTarget", { targetId, flatten: true });
  await cdp.send("Page.enable", {}, sessionId);
  await cdp.send("Runtime.enable", {}, sessionId);

  return {
    cdp,
    sessionId,
    async close() {
      try {
        try {
          await cdp.send("Browser.close");
        } catch {
          cdp.close();
        }
      } finally {
        // The child MUST be reaped even if the CDP shutdown above threw: a
        // live Chrome keeps node's event loop alive, so the script would hang
        // until the CI job timeout instead of failing in seconds — strictly
        // worse than a loud error. Hence `finally`, not straight-line code.
        if (child.exitCode === null) {
          child.kill("SIGTERM");
        }
        // Chrome must also be GONE before the profile directory is removed. A
        // process still flushing `Default/` races the recursive unlink and
        // surfaces as ENOTEMPTY (entries are scanned, then rmdir finds a
        // freshly written file). Escalate rather than trust one fixed grace
        // period — CI runners shut down slower than dev machines, which is
        // why this passed locally and failed on the first CI run.
        if (child.exitCode === null) {
          await Promise.race([childExited, delay(5_000)]);
        }
        if (child.exitCode === null) {
          child.kill("SIGKILL");
          await Promise.race([childExited, delay(2_000)]);
        }
        // Retries cover the residual race (`force` alone only swallows
        // ENOENT). A temp dir we cannot remove is not worth failing a passing
        // smoke run over — the OS temp root gets reclaimed regardless.
        try {
          await rm(userDataDir, {
            recursive: true,
            force: true,
            maxRetries: 10,
            retryDelay: 100,
          });
        } catch (error) {
          console.warn(`warning: could not remove ${userDataDir}: ${error}`);
        }
      }
    },
  };
}

function waitForDevTools(child) {
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

function createStaticServer(rootDir) {
  const mimeTypes = {
    ".css": "text/css",
    ".glsl": "text/plain; charset=utf-8",
    ".html": "text/html; charset=utf-8",
    ".js": "text/javascript",
    ".json": "application/json",
    ".mjs": "text/javascript",
    ".wasm": "application/wasm",
  };
  return createHttpServer((request, response) => {
    const fatal = (status) => {
      response.writeHead(status);
      response.end();
    };
    let pathname;
    try {
      pathname = decodeURIComponent(new URL(request.url, "http://localhost/").pathname);
    } catch {
      return fatal(400);
    }
    if (pathname.endsWith("/")) {
      pathname += "index.html";
    }
    const filePath = path.join(rootDir, pathname);
    // path.join normalizes ".." segments; anything escaping rootDir is refused.
    if (!filePath.startsWith(rootDir + path.sep)) {
      return fatal(403);
    }
    const stream = createReadStream(filePath);
    stream.once("error", () => fatal(404));
    stream.once("open", () => {
      response.writeHead(200, {
        "content-type": mimeTypes[path.extname(filePath)] ?? "application/octet-stream",
        "cache-control": "no-store",
      });
      stream.pipe(response);
    });
    response.once("close", () => stream.destroy());
  });
}

function findFreePort() {
  return new Promise((resolve, reject) => {
    const probe = createTcpServer();
    probe.once("error", reject);
    probe.listen(0, "127.0.0.1", () => {
      const { port: assigned } = probe.address();
      probe.close((closeError) => (closeError ? reject(closeError) : resolve(assigned)));
    });
  });
}

function findChrome() {
  const candidates = [
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
    "/Applications/Chromium.app/Contents/MacOS/Chromium",
    "/usr/bin/google-chrome",
    "/usr/bin/chromium",
    "/usr/bin/chromium-browser",
  ];
  return candidates.find((candidate) => existsSync(candidate)) ?? null;
}

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function fail(message) {
  console.error(message);
  process.exit(1);
}
