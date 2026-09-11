// The emulator's dedicated Worker.
//
// **A module worker** (`new Worker(url, {type: 'module'})`), which is a change
// from the classic worker this rig had: it reaches `jit-host.js` (through
// `bench-run.js`), and that module has to stay importable *unchanged* by
// Studio's own worker (JD25), so inlining it here was not an option. Module
// workers are the only shape that can `import`; `importScripts` does not exist
// in one, and this file does not use it. The import itself is dynamic rather
// than static — see DD33 below — but it is still an ES module import, and
// `import.meta` inside the graph is what carries the stamp down.
//
// **Dedicated is a correctness requirement, not a convenience** (JD13).
// Compiling a translated module is `new WebAssembly.Module` called
// synchronously, at 64 MB, and that is legal only because guest time is the
// scheduler's and the wall clock never enters the machine (PD5 / ADR
// 2026-09-06) — so it cannot reach a transcript. It can still freeze whatever
// thread it is on, and that thread must not be the one drawing the page.
'use strict';

let compiled = null;
const elfCache = {}; // slug -> Uint8Array

// The worker is loaded as worker.js?v=<short sha> (set by index.html from
// manifest.json's build.short) so a phone with a stale cache picks up the
// matching emu.wasm on refresh instead of silently running an old build.
const version = new URL(self.location.href).searchParams.get('v');
const v = (name) => name + (version ? '?v=' + version : '');

// DD33 — the stamp has to reach the SIBLINGS too.
//
// `import { runOnce } from './bench-run.js'` is a static specifier, and a
// static specifier cannot carry the stamp this file was loaded with. That is
// exactly how Safari paired a fresh `worker.js?v=…` with a *cached*
// `bench-run.js` and died at a missing export with a bare stack: the entry was
// versioned, everything behind it was not. A dynamic import can carry it, so
// this edge is dynamic and stamped, and `bench-run.js` passes its own stamp on
// to `wasi-shim.js` and `jit-host.js` the same way.
//
// Started here at module scope so the fetch is in flight while the page is
// still wiring up, and awaited inside `onmessage` rather than at the top level:
// a module worker's message queue and top-level await are a bad pair, and this
// costs nothing to avoid.
const benchRun = import(v('./bench-run.js'));

async function loadModule() {
  if (compiled) return;
  const bytes = await (await fetch(v('emu.wasm'))).arrayBuffer();
  const t0 = performance.now();
  // Synchronous, in a Worker, at whatever size the emulator is — the same
  // call `jit-host.js` makes on the translated module, and the same argument
  // makes it legal.
  compiled = new WebAssembly.Module(bytes);
  postMessage({ type: 'loaded', compileMs: performance.now() - t0, wasmBytes: bytes.byteLength });
}

// The ELF carries the manifest's own `elfStamp` (the first 12 hex of the
// image's sha256), NOT the build stamp: these are pinned images (DD25) that
// change when the pin changes and at no other time, and stamping 36 MB of ELF
// with the emulator's git sha would make a phone re-download all four every
// time the emulator is rebuilt. A content stamp busts exactly when the bytes
// move — which, for a firmware image, is the only thing a stale copy could
// ever get wrong, and it would get it wrong as a silently wrong NUMBER rather
// than as a missing export.
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
    // The one-click preset asks for rows by name rather than sending a plan,
    // so the sequence itself lives in `bench-run.js` beside the code that runs
    // it — one definition for the page, this Worker and the node harness.
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
        // that drops the wifi between two rows of a four-row preset must lose
        // THAT row, not the three around it and the upload with them.
        const image = images[step.slug];
        if (!image) throw new Error('no image ' + step.slug + ' in the manifest');
        const elfBytes = await loadElf(image);
        r = await runOnce({ compiled, image, elfBytes, ...step });
      } catch (e) {
        // A row that could not be taken is a row that says so. The rig must
        // report a crash rather than hang on it — on a phone at 64 MB of wasm
        // that is the failure mode the director named by name.
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
