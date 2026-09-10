// M7 P2 browser-seam probe -- the actual Worker entry point.
//
// Runs identically as a browser module Worker (`self`/`postMessage`) and as
// a Node/Bun `node:worker_threads` worker (`parentPort`). JD13 makes this
// distinction matter: the product's emulator is a dedicated Worker, so
// every timed number in this probe is taken from inside one, not from a
// runtime's main thread.
import { runAll } from './probe-tests.mjs';

const result = await runAll();

if (typeof self !== 'undefined' && typeof postMessage === 'function') {
  postMessage(result);
} else {
  const { parentPort } = await import('node:worker_threads');
  parentPort.postMessage(result);
}
