#!/usr/bin/env node
// Build the pattern review page: ONE self-contained index.html, every
// preview inline, no external requests.
//
//   node scripts/pattern-review/build-page.mjs <frames-dir> <out.html>
//
// <frames-dir> holds the `<pattern>__<swatch>.json` files that
// `lp-cli pattern preview` writes (schema `lp-pattern-preview/1`). One row
// per pattern, one cell per swatch; each cell is a <canvas> drawing the
// recorded per-lamp colours as LED dots on black, looping. Under each row:
// the pattern's name, family, description, knobs with defaults and idea
// source, and a keep / tune / cut choice with notes that persist in
// localStorage and copy out as text.
//
// `just pattern-review` runs the preview and this in one go.

import { readdirSync, readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";

const SCHEMA = "lp-pattern-preview/1";
// Display order; a swatch not listed here sorts after these, by name.
const SWATCH_ORDER = ["choker", "matrix16", "ring24", "disc241", "strip60"];

function main() {
  const [framesDir, outPath] = process.argv.slice(2);
  if (!framesDir || !outPath) {
    console.error("usage: build-page.mjs <frames-dir> <out.html>");
    process.exit(2);
  }
  const files = readdirSync(framesDir).filter((f) => f.endsWith(".json")).sort();
  if (files.length === 0) {
    console.error(`${framesDir}: no preview JSONs`);
    process.exit(1);
  }

  const patterns = new Map();
  const swatches = new Map();
  let backend = null;
  for (const file of files) {
    const record = JSON.parse(readFileSync(join(framesDir, file), "utf8"));
    if (record.schema !== SCHEMA) {
      console.error(`${file}: schema ${record.schema}, expected ${SCHEMA} — skipped`);
      continue;
    }
    const slug = record.pattern.slug;
    const swatch = record.swatch;
    if (!swatches.has(swatch.name)) {
      swatches.set(swatch.name, {
        name: swatch.name,
        lamps: swatch.lamp_count,
        size: swatch.size,
        pitch: swatch.pitch,
        // Flat [x0, y0, x1, y1, …], drawing units (long side 0 → 1, y down).
        xy: swatch.positions.flat(),
      });
    }
    if (!patterns.has(slug)) {
      patterns.set(slug, { meta: record.pattern, render: record.render, cells: {} });
    }
    backend ??= `${record.render.backend}; ${record.render.float_mode}`;
    patterns.get(slug).cells[swatch.name] = {
      frames: record.frames,
      fps: record.render.fps,
      count: record.render.frame_count ?? 0,
      renderSize: record.render.render_size,
      sampling: record.render.sampling,
      stats: record.stats,
      error: record.error,
    };
  }

  const swatchNames = [...swatches.keys()].sort((a, b) => {
    const ia = SWATCH_ORDER.indexOf(a);
    const ib = SWATCH_ORDER.indexOf(b);
    return (ia < 0 ? 99 : ia) - (ib < 0 ? 99 : ib) || a.localeCompare(b);
  });
  const rows = [...patterns.values()].sort(
    (a, b) =>
      (a.meta.family ?? "~").localeCompare(b.meta.family ?? "~") ||
      a.meta.name.localeCompare(b.meta.name),
  );
  const data = {
    generated: new Date().toISOString(),
    backend,
    swatchOrder: swatchNames,
    swatches: Object.fromEntries(swatches),
    patterns: rows,
  };

  // `</` inside the inline JSON would close the script element.
  const json = JSON.stringify(data).replace(/<\//g, "<\\/");
  const html = PAGE.replace("/*DATA*/", () => json);
  mkdirSync(dirname(outPath), { recursive: true });
  writeFileSync(outPath, html);
  const errors = rows.flatMap((row) =>
    Object.entries(row.cells)
      .filter(([, cell]) => cell.error)
      .map(([swatch]) => `${row.meta.slug}/${swatch}`),
  );
  console.error(
    `${outPath}: ${rows.length} pattern(s) × ${swatchNames.length} swatch(es), ` +
      `${(html.length / 1e6).toFixed(1)} MB` +
      (errors.length ? `; ${errors.length} cell(s) carry an error: ${errors.join(", ")}` : ""),
  );
}

const PAGE = String.raw`<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Pattern Review</title>
<style>
  :root {
    --bg: #07070a;
    --panel: #101016;
    --line: #22222c;
    --text: #e8e8ee;
    --muted: #8a8a99;
    --keep: #3ecf8e;
    --tune: #f5b942;
    --cut: #ef5b5b;
    --accent: #8f7cff;
    color-scheme: dark;
  }
  * { box-sizing: border-box; }
  body {
    margin: 0;
    background: var(--bg);
    color: var(--text);
    font: 15px/1.45 system-ui, -apple-system, "Segoe UI", sans-serif;
  }
  header, main, footer { max-width: 1180px; margin: 0 auto; padding: 0 16px; }
  header { padding-top: 22px; padding-bottom: 8px; }
  h1 { font-size: 22px; margin: 0 0 4px; letter-spacing: 0.01em; }
  .sub { color: var(--muted); font-size: 13px; margin: 0; }
  .sub code { color: var(--text); }
  section.pattern {
    border-top: 1px solid var(--line);
    padding: 18px 0 22px;
  }
  .cells { display: flex; flex-wrap: wrap; gap: 10px; }
  .cell {
    flex: 1 1 calc(33.333% - 10px);
    min-width: 0;
    background: #000;
    border: 1px solid #16161c;
    border-radius: 10px;
    overflow: hidden;
    position: relative;
    padding-top: 20px;
  }
  .cell.wide { flex-basis: 100%; }
  .cell canvas { display: block; width: 100%; }
  .cell .tag {
    position: absolute; left: 8px; top: 6px;
    font-size: 11px; color: #6c6c7a; letter-spacing: 0.04em;
    pointer-events: none;
  }
  .cell .flag {
    position: absolute; right: 8px; top: 6px;
    font-size: 11px; font-weight: 600; color: var(--cut);
  }
  .cell .err {
    padding: 28px 12px 12px; color: var(--cut); font-size: 12px;
    font-family: ui-monospace, Menlo, monospace; white-space: pre-wrap;
    word-break: break-word; min-height: 90px;
  }
  .about { display: grid; grid-template-columns: 1fr 300px; gap: 18px; margin-top: 12px; }
  .about h2 { margin: 0; font-size: 18px; }
  .family {
    display: inline-block; margin-left: 8px; padding: 1px 8px; border-radius: 99px;
    border: 1px solid var(--line); color: var(--muted); font-size: 12px; font-weight: 500;
    vertical-align: 2px;
  }
  .desc { margin: 4px 0 8px; }
  .facts { color: var(--muted); font-size: 13px; margin: 2px 0; }
  .facts b { color: var(--text); font-weight: 500; }
  ul.knobs { list-style: none; padding: 0; margin: 6px 0 0; font-size: 13px; }
  ul.knobs li { padding: 2px 0; }
  ul.knobs .k { color: var(--text); font-weight: 600; }
  ul.knobs .v { font-family: ui-monospace, Menlo, monospace; }
  ul.knobs .b { color: var(--muted); font-size: 12px; }
  .ramp {
    display: inline-block; width: 90px; height: 10px; border-radius: 3px;
    vertical-align: -1px; border: 1px solid #333;
  }
  .verdict { background: var(--panel); border: 1px solid var(--line); border-radius: 10px; padding: 10px; }
  .choices { display: flex; gap: 6px; }
  .choices label {
    flex: 1; text-align: center; cursor: pointer; padding: 7px 0; border-radius: 7px;
    border: 1px solid var(--line); color: var(--muted); font-weight: 600; font-size: 14px;
    user-select: none;
  }
  .choices input { position: absolute; opacity: 0; pointer-events: none; }
  .choices input:focus-visible + span { outline: 2px solid var(--accent); outline-offset: 3px; }
  .choices label.on-keep { background: color-mix(in srgb, var(--keep) 22%, transparent); color: var(--keep); border-color: var(--keep); }
  .choices label.on-tune { background: color-mix(in srgb, var(--tune) 22%, transparent); color: var(--tune); border-color: var(--tune); }
  .choices label.on-cut { background: color-mix(in srgb, var(--cut) 22%, transparent); color: var(--cut); border-color: var(--cut); }
  textarea {
    width: 100%; margin-top: 8px; min-height: 64px; resize: vertical;
    background: #0b0b10; color: var(--text); border: 1px solid var(--line);
    border-radius: 7px; padding: 7px; font: inherit; font-size: 14px;
  }
  footer { border-top: 1px solid var(--line); padding-top: 18px; padding-bottom: 40px; }
  footer h2 { font-size: 18px; margin: 0 0 8px; }
  button {
    background: var(--accent); color: #fff; border: 0; border-radius: 8px;
    padding: 9px 16px; font: inherit; font-weight: 600; cursor: pointer;
  }
  #copied { color: var(--muted); margin-left: 10px; font-size: 13px; }
  #verdictText { min-height: 160px; font-family: ui-monospace, Menlo, monospace; font-size: 13px; }
  @media (max-width: 760px) {
    .about { grid-template-columns: 1fr; }
    .cell { flex-basis: calc(50% - 10px); }
    .cell.wide { flex-basis: 100%; }
  }
</style>
</head>
<body>
<header>
  <h1>Pattern review</h1>
  <p class="sub" id="subtitle"></p>
</header>
<main id="rows"></main>
<footer>
  <h2>Verdicts</h2>
  <p class="sub">Choices and notes stay in this browser. Copy them out and paste them back into the session.</p>
  <p><button id="copy" type="button">Copy verdicts as text</button><span id="copied"></span></p>
  <textarea id="verdictText" readonly aria-label="Verdicts as text"></textarea>
</footer>
<script id="data" type="application/json">/*DATA*/</script>
<script>
"use strict";
const DATA = JSON.parse(document.getElementById("data").textContent);
const STORE = "lp-pattern-review:";

// ---- storage (every access guarded: private windows and previews throw) ----
function load(slug) {
  try { return JSON.parse(localStorage.getItem(STORE + slug)) || {}; } catch (e) { return {}; }
}
function save(slug, value) {
  try { localStorage.setItem(STORE + slug, JSON.stringify(value)); } catch (e) { /* not kept */ }
}

// ---- LED look -------------------------------------------------------------
// The recorded bytes are PWM duty (the fixture runs with gamma off), which an
// LED emits linearly; a screen pixel does not, so encode to sRGB before
// drawing. Each lamp is three additive sprites (R, G, B), each a bright core
// and a dim bloom, on black.
const TO_SRGB = new Float32Array(256);
for (let i = 0; i < 256; i++) {
  const v = i / 255;
  TO_SRGB[i] = v <= 0.0031308 ? 12.92 * v : 1.055 * Math.pow(v, 1 / 2.4) - 0.055;
}
const spriteCache = new Map();
function sprites(pitchPx) {
  const key = Math.max(2, Math.round(pitchPx * 4)) / 4;
  if (spriteCache.has(key)) return spriteCache.get(key);
  const bloom = Math.max(2.5, key * 1.15);
  const core = Math.max(1, key * 0.26);
  const size = Math.ceil(bloom * 2) + 2;
  const make = (rgb) => {
    const c = document.createElement("canvas");
    c.width = c.height = size;
    const g = c.getContext("2d");
    const mid = size / 2;
    const grad = g.createRadialGradient(mid, mid, 0, mid, mid, bloom);
    const k = core / bloom;
    grad.addColorStop(0, "rgba(" + rgb + ",1)");
    grad.addColorStop(k * 0.7, "rgba(" + rgb + ",0.95)");
    grad.addColorStop(k, "rgba(" + rgb + ",0.42)");
    grad.addColorStop(Math.min(0.99, k + (1 - k) * 0.35), "rgba(" + rgb + ",0.12)");
    grad.addColorStop(1, "rgba(" + rgb + ",0)");
    g.fillStyle = grad;
    g.fillRect(0, 0, size, size);
    return c;
  };
  const set = { size, r: make("255,0,0"), g: make("0,255,0"), b: make("0,0,255") };
  spriteCache.set(key, set);
  return set;
}

function b64bytes(text) {
  const bin = atob(text);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

// ---- cells ----------------------------------------------------------------
const cells = [];
function isWide(sw) {
  const pad = 2 * sw.pitch;
  return (sw.size[0] + pad) / (sw.size[1] + pad) >= 2.5;
}

function makeCell(swatchName, cell) {
  const sw = DATA.swatches[swatchName];
  const box = document.createElement("div");
  box.className = "cell" + (isWide(sw) ? " wide" : "");
  const tag = document.createElement("div");
  tag.className = "tag";
  tag.textContent = swatchName + " · " + sw.lamps;
  if (!cell || cell.error) {
    const err = document.createElement("div");
    err.className = "err";
    err.textContent = cell ? cell.error : "not rendered";
    box.append(tag, err);
    return box;
  }
  const canvas = document.createElement("canvas");
  box.append(canvas, tag);
  const flags = [];
  if (cell.stats && cell.stats.peak === 0) flags.push("BLACK");
  if (cell.stats && cell.stats.changing_frames === 0) flags.push("STATIC");
  if (flags.length) {
    const f = document.createElement("div");
    f.className = "flag";
    f.textContent = flags.join(" ");
    box.append(f);
  }
  cells.push({ box, canvas, sw, cell, rgb: null, frame: -1, visible: false, w: 0 });
  return box;
}

function layout(c) {
  const dpr = Math.min(window.devicePixelRatio || 1, 2);
  const cssW = c.box.clientWidth;
  if (!cssW) return;
  const sw = c.sw;
  const pad = sw.pitch;
  const boxW = sw.size[0] + 2 * pad;
  const boxH = sw.size[1] + 2 * pad;
  let cssH = cssW * (boxH / boxW);
  // A wide swatch keeps its real aspect; a square-ish one is capped so a
  // row stays scannable (the drawing is centred and aspect-kept either way).
  cssH = Math.max(Math.min(cssH, 280), 48);
  c.canvas.style.height = cssH + "px";
  c.canvas.width = Math.round(cssW * dpr);
  c.canvas.height = Math.round(cssH * dpr);
  const scale = Math.min(c.canvas.width / boxW, c.canvas.height / boxH);
  c.scale = scale;
  c.ox = (c.canvas.width - sw.size[0] * scale) / 2;
  c.oy = (c.canvas.height - sw.size[1] * scale) / 2;
  c.sprites = sprites(sw.pitch * scale);
  c.w = cssW;
  c.frame = -1;
}

function draw(c, frame) {
  const g = c.canvas.getContext("2d");
  g.globalCompositeOperation = "source-over";
  g.globalAlpha = 1;
  g.fillStyle = "#000";
  g.fillRect(0, 0, c.canvas.width, c.canvas.height);
  const n = c.sw.lamps;
  const xy = c.sw.xy;
  const s = c.sprites;
  const half = s.size / 2;
  // Unlit lamps: a faint mark so the shape reads when the pattern is dark.
  g.fillStyle = "#151518";
  const mark = Math.max(0.8, c.sw.pitch * c.scale * 0.12);
  for (let i = 0; i < n; i++) {
    g.beginPath();
    g.arc(c.ox + xy[2 * i] * c.scale, c.oy + xy[2 * i + 1] * c.scale, mark, 0, 6.2832);
    g.fill();
  }
  g.globalCompositeOperation = "lighter";
  const base = frame * n * 3;
  const rgb = c.rgb;
  for (let i = 0; i < n; i++) {
    const x = c.ox + xy[2 * i] * c.scale - half;
    const y = c.oy + xy[2 * i + 1] * c.scale - half;
    const r = rgb[base + 3 * i], gr = rgb[base + 3 * i + 1], b = rgb[base + 3 * i + 2];
    if (r) { g.globalAlpha = TO_SRGB[r]; g.drawImage(s.r, x, y); }
    if (gr) { g.globalAlpha = TO_SRGB[gr]; g.drawImage(s.g, x, y); }
    if (b) { g.globalAlpha = TO_SRGB[b]; g.drawImage(s.b, x, y); }
  }
}

const seen = new IntersectionObserver((entries) => {
  for (const e of entries) {
    const c = cells.find((cell) => cell.box === e.target);
    if (c) c.visible = e.isIntersecting;
  }
}, { rootMargin: "200px" });

const start = performance.now();
function tick(now) {
  const t = (now - start) / 1000;
  for (const c of cells) {
    if (!c.visible) continue;
    if (c.w !== c.box.clientWidth) layout(c);
    if (!c.rgb) c.rgb = b64bytes(c.cell.frames);
    if (!c.cell.count) continue;
    const frame = Math.floor(t * c.cell.fps) % c.cell.count;
    if (frame !== c.frame) {
      draw(c, frame);
      c.frame = frame;
    }
  }
  requestAnimationFrame(tick);
}

// ---- rows -----------------------------------------------------------------
function el(tag, cls, text) {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text != null) e.textContent = text;
  return e;
}

function fmt(v) {
  if (v == null) return "–";
  if (typeof v === "number") return String(Math.round(v * 1000) / 1000);
  return JSON.stringify(v);
}

function knobItem(k) {
  const li = el("li");
  li.append(el("span", "k", k.label || k.slot), " ");
  if (k.kind === "value") {
    li.append(el("span", "v", fmt(k.default)));
    if (k.min != null || k.max != null) li.append(" ", el("span", "b", "(" + fmt(k.min) + "–" + fmt(k.max) + ")"));
  } else if (k.kind === "phasor") {
    li.append(el("span", "v", "phasor " + fmt(k.period_seconds) + " s " + (k.waveform || "")));
  } else if (k.kind === "palette") {
    const colors = String(k.stops || "").split(/\s+/).filter((t) => t.startsWith("#"));
    const ramp = el("span", "ramp");
    if (colors.length) ramp.style.background = "linear-gradient(90deg," + colors.join(",") + ")";
    li.append(ramp);
  } else {
    li.append(el("span", "v", k.kind + (k.default != null ? " " + fmt(k.default) : "")));
  }
  if (k.binding) li.append(" ", el("span", "b", k.binding));
  if (k.description) li.title = k.description;
  return li;
}

function renderRow(row) {
  const m = row.meta;
  const sec = el("section", "pattern");
  sec.id = "p-" + m.slug;
  const grid = el("div", "cells");
  for (const name of DATA.swatchOrder) grid.append(makeCell(name, row.cells[name]));
  sec.append(grid);

  const about = el("div", "about");
  const left = el("div");
  const h = el("h2", null, m.name);
  h.append(el("span", "family", m.family || "family: –"));
  left.append(h, el("p", "desc", m.description || "(no description)"));
  const prov = m.provenance || {};
  const facts = [
    ["idea", m.idea || "–"],
    ["license", [prov.license, prov.author].filter(Boolean).join(", ") || "–"],
    ["render", (row.render.render_size ? row.render.render_size.join("×") : "?") + " " + (row.render.sampling || "")],
    ["slug", m.slug],
  ];
  for (const [k, v] of facts) {
    const p = el("p", "facts");
    p.append(el("span", null, k + ": "), el("b", null, v));
    left.append(p);
  }
  const knobs = (m.knobs || []).filter((k) => k.label || k.binding);
  if (knobs.length) {
    const ul = el("ul", "knobs");
    for (const k of knobs) ul.append(knobItem(k));
    left.append(ul);
  }

  const right = el("div", "verdict");
  const state = load(m.slug);
  const choices = el("div", "choices");
  const labels = [];
  for (const choice of ["keep", "tune", "cut"]) {
    const label = el("label");
    const input = el("input");
    input.type = "radio";
    input.name = "v-" + m.slug;
    input.value = choice;
    input.checked = state.choice === choice;
    label.append(input, el("span", null, choice));
    input.addEventListener("change", () => {
      state.choice = choice;
      save(m.slug, state);
      paint();
      refreshText();
    });
    labels.push([label, choice]);
    choices.append(label);
  }
  const paint = () => {
    for (const [label, choice] of labels) label.className = state.choice === choice ? "on-" + choice : "";
  };
  paint();
  const notes = el("textarea");
  notes.placeholder = "Notes (what to tune, what it reminds you of…)";
  notes.value = state.notes || "";
  notes.addEventListener("input", () => {
    state.notes = notes.value;
    save(m.slug, state);
    refreshText();
  });
  right.append(choices, notes);
  about.append(left, right);
  sec.append(about);
  return sec;
}

function verdictText() {
  const lines = ["Pattern review verdicts (" + new Date().toISOString().slice(0, 10) + ")", ""];
  for (const row of DATA.patterns) {
    const s = load(row.meta.slug);
    const notes = (s.notes || "").trim().replace(/\s*\n\s*/g, " / ");
    lines.push("- " + row.meta.slug + ": " + (s.choice || "(no choice)") + (notes ? " — " + notes : ""));
  }
  return lines.join("\n");
}
function refreshText() {
  document.getElementById("verdictText").value = verdictText();
}

document.getElementById("copy").addEventListener("click", async () => {
  const text = verdictText();
  const area = document.getElementById("verdictText");
  area.value = text;
  let ok = false;
  try { await navigator.clipboard.writeText(text); ok = true; } catch (e) {
    try { area.select(); ok = document.execCommand("copy"); } catch (e2) { ok = false; }
  }
  document.getElementById("copied").textContent = ok ? "Copied." : "Select the text below and copy it.";
});

const sub = document.getElementById("subtitle");
sub.textContent =
  DATA.patterns.length + " patterns × " + DATA.swatchOrder.length + " swatches (" +
  DATA.swatchOrder.join(", ") + "). Rendered on the host engine: " + DATA.backend +
  ". Built " + DATA.generated.slice(0, 16).replace("T", " ") + " UTC. " +
  "Dots show PWM duty as an LED emits it; brightness is the fixture's 1.0.";

const rowsEl = document.getElementById("rows");
for (const row of DATA.patterns) rowsEl.append(renderRow(row));
for (const c of cells) seen.observe(c.box);
refreshText();
requestAnimationFrame(tick);
</script>
</body>
</html>
`;

main();
