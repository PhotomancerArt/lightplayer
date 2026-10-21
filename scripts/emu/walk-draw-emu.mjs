#!/usr/bin/env node
// Draw on an emulated board's live picture in headless Chrome, and say what
// came of it (plan `drawing-on-the-picture`, P6/P7).
//
// Against a running `just studio-dev-emu` whose board `c6-a` already plays a
// pattern that reads `touches` (e.g. `fluid-draw`): connect over the in-page
// USB chooser, press Connect so the card holds its panel, check the picture
// is drawable, then drag a finger across it with trusted mouse events (the
// path a real pointer takes: Pointer Events, capture, one Stroke per move)
// and take screenshots before, mid-stroke and after. A picture not marked
// drawable fails the walk.
//
//   node scripts/emu/walk-draw-emu.mjs <studio url with ?emu=…> <out dir> [board]
//
// Headless, so its timings are not latency evidence; what it proves is the
// path: drawable fact → pointer surface → core → momentary writes → board.

import { mkdirSync } from "node:fs";
import path from "node:path";

import { StudioDriver } from "./studio-driver.mjs";

const [url, outDir, board = "c6-a"] = process.argv.slice(2);
if (!url || !outDir) {
  console.error("usage: node scripts/emu/walk-draw-emu.mjs <studio url with ?emu=…> <out dir> [board]");
  process.exit(2);
}
mkdirSync(outDir, { recursive: true });

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const driver = await StudioDriver.launch({ width: 900, height: 900 });
await driver.cdp.send(
  "Emulation.setDeviceMetricsOverride",
  { width: 900, height: 900, deviceScaleFactor: 2, mobile: false },
  driver.sessionId,
);
const shot = async (name) => {
  const file = path.join(outDir, `${name}.png`);
  await driver.screenshot(file);
  console.log(`shot → ${file}`);
};

try {
  await driver.navigate(url);
  await driver.awaitShim();
  await driver.pressConnect("USB");
  await driver.pickBoard(board);
  await driver.waitFor(`Boolean(document.querySelector('[data-picture-frame="true"]'))`, {
    timeoutMs: 120_000,
    what: "the board's live picture",
  });
  await driver.clickWhenReady("Connect", { exact: true, timeoutMs: 120_000 });
  await driver.waitFor(`Boolean(document.querySelector('.ux-lamp-draw'))`, {
    timeoutMs: 120_000,
    what: "a drawable picture (the card's live picture, connected)",
  });
  await sleep(3_000);
  await shot("1-drawable-before");

  const box = await driver.evaluate(`(() => {
    const r = document.querySelector('.ux-lamp-draw').getBoundingClientRect();
    return { x: r.x, y: r.y, w: r.width, h: r.height };
  })()`);
  const cx = box.x + box.w / 2;
  const cy = box.y + box.h / 2;
  const mouse = (type, x, y, buttons = 1) =>
    driver.cdp.send(
      "Input.dispatchMouseEvent",
      { type, x, y, button: "left", buttons, clickCount: type === "mouseMoved" ? 0 : 1 },
      driver.sessionId,
    );
  // A loop across the picture, 4 s at ~60 Hz.
  const at = (t) => [cx + 0.3 * box.h * Math.cos(t * 2 * Math.PI + Math.PI), cy + 0.25 * box.h * Math.sin(t * 4 * Math.PI)];
  const [x0, y0] = at(0);
  await mouse("mousePressed", x0, y0);
  const start = Date.now();
  let shotMid = false;
  while (Date.now() - start < 4_000) {
    const t = (Date.now() - start) / 4_000;
    const [x, y] = at(t);
    await mouse("mouseMoved", x, y);
    if (!shotMid && t > 0.6) {
      shotMid = true;
      await shot("2-drawing");
    }
    await sleep(16);
  }
  const [x1, y1] = at(1);
  await mouse("mouseReleased", x1, y1, 0);
  await sleep(600);
  await shot("3-after-lift");
  await sleep(3_000);
  await shot("4-friends-back");
  const strokes = driver.consoleLines("touches").length;
  console.log(`done; console lines naming touches: ${strokes}`);
} catch (error) {
  await shot("failure").catch(() => {});
  console.error(error.message);
  for (const line of driver.consoleLines().slice(-30)) console.error(line);
  process.exitCode = 1;
} finally {
  await driver.close();
}
