// The classic ESP32 emulator's dedicated Worker.
//
// **A twin of `scripts/emu/bench-web/worker.js`**, staged as `worker.js` so
// the perf lab's page — which spawns `builds/<id>/worker.js?v=<id>` as a module
// worker and posts `{plan, preset, defaults, images}` — can drive a classic
// build with no change to the lab. The lab's page, its slug parser and its job
// queue are that lane's and are not edited by this plan; this file is the one
// end of the contract a build owns.
//
// What differs from the C6's worker: the sibling it imports
// (`xt-bench-run.js`, whose argv and report-line grammar are the classic's) and
// nothing else. The message protocol, the ELF cache, the per-row try/catch and
// the DD33 stamping are the same, deliberately.
//
// **A module worker** (`new Worker(url, {type: 'module'})`), because it reaches
// `jit-host.js` through `xt-bench-run.js`, and that module has to stay
// importable *unchanged* by Studio's own worker (JD25).
//
// **Dedicated is a correctness requirement, not a convenience** (JD13).
// Compiling a translated module is `new WebAssembly.Module` called
// synchronously, at tens of megabytes, and that is legal only because guest
// time is the scheduler's and the wall clock never enters the machine (PD5 /
// ADR 2026-09-06) — so it cannot reach a transcript. It can still freeze
// whatever thread it is on, and that thread must not be the one drawing the
// page.
'use strict';

let compiled = null;
const elfCache = {}; // slug -> Uint8Array

const version = new URL(self.location.href).searchParams.get('v');
const v = (name) => name + (version ? '?v=' + version : '');

// DD33 — the stamp has to reach the SIBLINGS too, so this edge is dynamic and
// stamped, and `xt-bench-run.js` passes its own stamp on to `wasi-shim.js` and
// `jit-host.js` the same way. Started at module scope so the fetch is in
// flight while the page is still wiring up, and awaited inside `onmessage`
// rather than at the top level: a module worker's message queue and top-level
// await are a bad pair.
const benchRun = import(v('./xt-bench-run.js'));

async function loadModule() {
  if (compiled) return;
  const bytes = await (await fetch(v('emu.wasm'))).arrayBuffer();
  const t0 = performance.now();
  compiled = new WebAssembly.Module(bytes);
  postMessage({ type: 'loaded', compileMs: performance.now() - t0, wasmBytes: bytes.byteLength });
}

// The ELF carries the manifest's own `elfStamp` (the first 12 hex of the
// image's sha256), NOT the build stamp: these are pinned images that change
// when the pin changes and at no other time, and a content stamp busts exactly
// when the bytes move — which, for a firmware image, is the only thing a stale
// copy could ever get wrong, and it would get it wrong as a silently wrong
// NUMBER rather than as a missing export.
async function loadElf(image) {
  if (elfCache[image.slug]) return elfCache[image.slug];
  const url = image.elf + (image.elfStamp ? '?v=' + image.elfStamp : '');
  const bytes = new Uint8Array(await (await fetch(url)).arrayBuffer());
  elfCache[image.slug] = bytes;
  return bytes;
}

onmessage = async (ev) => {
  try {
    const { runOnce, gateRowsPlan } = await benchRun;
    await loadModule();
    const plan = ev.data.preset === 'gate-rows'
      ? gateRowsPlan({ defaults: ev.data.defaults })
      : ev.data.plan;
    const images = {};
    for (const image of ev.data.images) images[image.slug] = image;
    for (let i = 0; i < plan.length; i++) {
      const step = plan[i];
      postMessage({ type: 'progress', i, n: plan.length, step });
      let r;
      try {
        // Fetching the image is inside the try with the run itself: a phone
        // that drops the wifi between two rows of a preset must lose THAT row,
        // not the ones around it and the upload with them.
        const image = images[step.slug];
        if (!image) throw new Error('no image ' + step.slug + ' in the manifest');
        const elfBytes = await loadElf(image);
        r = await runOnce({ compiled, image, elfBytes, ...step });
      } catch (e) {
        // A row that could not be taken is a row that says so. The rig must
        // report a crash rather than hang on it.
        r = { slug: step.slug, grade: step.grade, mode: step.mode, fnBlocks: step.fnBlocks,
              timeout: step.timeout,
              failed: String((e && e.stack) || e) };
      }
      postMessage({ type: 'result', i, r });
    }
    postMessage({ type: 'done' });
  } catch (e) {
    postMessage({ type: 'error', message: String((e && e.stack) || e) });
  }
};
