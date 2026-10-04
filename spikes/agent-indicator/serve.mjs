#!/usr/bin/env node
// Serve the spike's story build LIVE, with a round's override CSS and cell
// map applied in the page, plus a floating Replay / Loop control (a story
// mounts its light once; it fades after 4 s).
//
//   node spikes/agent-indicator/serve.mjs [round-css] [round-cells.json]
//
// Port: scripts/dev-port.sh (stable per worktree, never pinned).

import { execFileSync } from "node:child_process";
import { createReadStream } from "node:fs";
import { readFile, stat } from "node:fs/promises";
import { createServer } from "node:http";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const repo = path.resolve(here, "../..");
const site = path.join(repo, "target/dx/lpa-studio-web/release/web/public");
const css = process.argv[2] ? await readFile(process.argv[2], "utf8") : "";
const cells = process.argv[3] ? JSON.parse(await readFile(process.argv[3], "utf8")) : {};
const port = Number(execFileSync(path.join(repo, "scripts/dev-port.sh"), ["agent-indicator-spike"], { cwd: repo, encoding: "utf8" }).trim());

const STORY_KEYS = {
  "agent-spike-save": "save",
  "agent-spike-remove": "node-verb",
  "devices-card-agent-spike": "device-button",
  "agent-spike-edited": "edited-node",
};

const inject = `<script>
(() => {
  const CSS = ${JSON.stringify(css)};
  const CELLS = ${JSON.stringify(cells)};
  const KEYS = ${JSON.stringify(STORY_KEYS)};
  const style = document.createElement("style");
  style.textContent = CSS;
  const keyNow = () => KEYS[location.pathname.split("/").filter(Boolean).pop()];
  const apply = () => {
    if (style.parentNode !== document.body || document.body.lastElementChild !== panel) {
      document.body.appendChild(style);
      document.body.appendChild(panel);
    }
    const map = CELLS[keyNow()];
    if (!map) return;
    document.querySelectorAll("[data-spike-cell]:not([data-mapped])").forEach((cell, i) => {
      const all = [...document.querySelectorAll("[data-spike-cell]")];
      const m = map[all.indexOf(cell)];
      cell.setAttribute("data-mapped", "1");
      if (!m) { cell.style.display = "none"; return; }
      cell.setAttribute("data-spike-cell", m.label);
      cell.className = "tw:grid tw:min-w-0 tw:content-start tw:gap-2 tw:p-4 " + m.class;
      const span = cell.querySelector(":scope > span");
      if (span) span.textContent = m.label;
    });
  };
  const replay = () => document.getAnimations().forEach((a) => { a.cancel(); a.play(); });
  const panel = document.createElement("div");
  panel.style.cssText = "position:fixed;right:16px;bottom:16px;z-index:99999;display:flex;gap:10px;align-items:center;padding:10px 14px;border-radius:12px;background:#1d1d28;border:1px solid #4a4a5e;color:#f2f0e8;font:600 13px Inter,system-ui,sans-serif;box-shadow:0 8px 30px rgba(0,0,0,.5)";
  panel.innerHTML = '<span style="color:#9c9cab">agent light</span><button id="spike-replay" style="font:inherit;padding:6px 12px;border-radius:8px;border:1px solid #4a4a5e;background:#252532;color:inherit;cursor:pointer">Replay</button><label style="display:flex;gap:6px;align-items:center;cursor:pointer"><input id="spike-loop" type="checkbox" checked> loop every 5 s</label>';
  panel.querySelector("#spike-replay").onclick = replay;
  let loop = setInterval(replay, 5000);
  panel.querySelector("#spike-loop").onchange = (e) => { clearInterval(loop); if (e.target.checked) loop = setInterval(replay, 5000); };
  const start = () => { apply(); new MutationObserver(apply).observe(document.body, { childList: true, subtree: true }); };
  document.readyState === "loading" ? document.addEventListener("DOMContentLoaded", start) : start();
})();
</script>`;

const types = { ".html": "text/html", ".js": "text/javascript", ".wasm": "application/wasm", ".css": "text/css", ".png": "image/png", ".woff2": "font/woff2", ".svg": "image/svg+xml", ".json": "application/json" };
createServer(async (req, res) => {
  const url = new URL(req.url, "http://x");
  let file = path.join(site, decodeURIComponent(url.pathname));
  try {
    const s = await stat(file);
    if (s.isDirectory()) file = path.join(file, "index.html");
    await stat(file);
  } catch {
    file = path.join(site, "index.html");
  }
  const type = types[path.extname(file)] ?? "application/octet-stream";
  if (type === "text/html") {
    const html = (await readFile(file, "utf8")).replace("</body>", `${inject}</body>`);
    res.writeHead(200, { "content-type": type, "cache-control": "no-store" });
    res.end(html);
    return;
  }
  res.writeHead(200, { "content-type": type });
  createReadStream(file).pipe(res);
}).listen(port, "127.0.0.1", () => {
  const base = `http://127.0.0.1:${port}/stories`;
  console.log(`agent indicator spike, live:`);
  for (const p of ["studio/layout/site-chrome/agent-spike-save", "studio/node/node/agent-spike-remove", "studio/home/home-gallery/devices-card-agent-spike", "studio/node/shader-face/agent-spike-edited"]) {
    console.log(`  ${base}/${p}`);
  }
});
