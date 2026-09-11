// The page's tab backing, behind a handle a synchronous Rust byte stream can
// hold (mode A, plan decision D4).
//
// `emulator_tab.js` lives in the SITE (`lpa-studio-web/public/lpa-link/`),
// not in this crate: it is the page's own module, shared with the
// `navigator.serial` polyfill (mode B) and with anything else in the tab that
// wants an emulated board. So it is reached by a dynamic `import()` of
// `/lpa-link/emulator_tab.js` rather than by a bundler edge — the same shape
// `browser_serial.js` uses for the device controller it loads.
//
// `EmulatorTabStream` / `EmulatorTabControl` (`mod.rs` in this directory) are
// the Rust side of this handle; `docs/adr/2026-09-10-the-c6-emulator-runs-in-the-tab.md`
// is the decision record for the shape (rule 1 and its Consequences on why a
// handle rather than the port object).
//
// # Why a numeric handle and not the port object
//
// `DeviceByteStream` (lpa-client) is `Send` and synchronous. A Rust struct
// holding a `JsValue` is neither, so the port lives HERE, in a registry keyed
// by a small integer, and Rust holds the integer — exactly what
// `browser_serial.js` does with its port ids, and for the same two reasons.
//
// # Why writes and control lines go through one chain
//
// The stream's verbs (`reopen`, `write_all`, `set_signals`) must return
// without awaiting: `Link::poll_event` may not block. Every one of them is
// therefore enqueued on the entry's single promise chain and applied in the
// order it was submitted, so a reset dance reaches the machine pin-write for
// pin-write and a frame written before the byte channel finishes opening is
// sent after it, not lost. A failure anywhere on the chain is recorded and
// handed to Rust by `takeError`, which is how an IO failure becomes a
// `LinkEvent::Error` instead of an unhandled rejection.
//
// # The bytes buffer has exactly one drainer at a time
//
// Two consumers read it: the model's pump (`takeBytes`, raw) and an
// `lpa-client` conversation (`takeLines`, whole lines, the remainder left
// behind for the pump). They never run together — the effects layer pauses
// the pump for the duration of a coarse effect, which is the same
// exclusive-borrow discipline the serial provider's `PortLineIo` documents.

/** Ports this page has opened, by the id Rust holds. */
const ports = new Map();

let nextId = 1;

/** The page module, imported once. */
let tabModule = null;

function loadTabModule() {
  if (!tabModule) {
    tabModule = import(tabModulePath());
  }
  return tabModule;
}

/**
 * Where the page serves its tab-emulator module.
 *
 * **A function, not an inline literal, and that is load-bearing.** The
 * specifier is a SITE path, resolved by the browser at run time; a literal
 * here is constant-foldable, so the bundler tries to resolve it against this
 * crate's source tree, finds nothing, and fails the whole JS bundling step —
 * after which `dx` falls back to copying the snippets somewhere the emitted
 * bundle does not look for them, and Studio stops booting at all (measured
 * 2026-09-11, the first build with `emulator-tab` enabled on
 * `lpa-studio-web`). Behind a call the specifier is opaque and the import
 * survives to run time untouched.
 *
 * `browser_serial.js` wraps its controller path for exactly this reason;
 * this is the same shape, deliberately.
 */
function tabModulePath() {
  return "/lpa-link/emulator_tab.js";
}

/**
 * Where this page serves the emulator module.
 *
 * Resolved at POWER-ON, not when the source was built, and for the reason
 * the sim's `discovered()` records at length: a served Studio build carries
 * the sidecar under a content-hashed name only, `window.__lpEngineAssets`
 * is a promise of the manifest that names it, and a link is built at
 * power-on — which can be the page's first action. A snapshot taken any
 * earlier is the unhashed fallback, which 404s.
 *
 * Absent key = this build serves no module (D21), and saying so by name
 * beats a fetch failure on a URL nobody chose.
 */
async function resolveModuleUrl(pinned) {
  if (pinned) return pinned;
  const assets = await globalThis.__lpEngineAssets;
  const url = assets?.emu_esp32c6_wasm;
  if (!url) {
    throw new Error(
      "this build ships no emulator module " +
        "(pkg/engine-manifest.json has no emu_esp32c6_wasm)",
    );
  }
  return url;
}

function entry(id) {
  const found = ports.get(id);
  if (!found) throw new Error(`no emulated board with handle ${id}`);
  return found;
}

/** Record a failure for `takeError`; the first one is the one that matters. */
function note(e, error) {
  const message = error?.message ?? String(error);
  if (!e.error) e.error = message;
  console.warn(`[emu-link] board ${e.uid}: ${message}`);
}

/**
 * Queue `step` behind everything already submitted for this board.
 *
 * The chain never rejects: a failure is noted and swallowed so the NEXT
 * step still runs. A wedged chain would strand the link with no events at
 * all, which reads as a board that went quiet rather than one that failed.
 */
function enqueue(e, step) {
  e.queue = e.queue.then(step).catch((error) => note(e, error));
  return e.queue;
}

/** Everything buffered, as one array, leaving the buffer empty. */
function drain(e) {
  let total = 0;
  for (const chunk of e.chunks) total += chunk.length;
  const out = new Uint8Array(total);
  let at = 0;
  for (const chunk of e.chunks) {
    out.set(chunk, at);
    at += chunk.length;
  }
  e.chunks.length = 0;
  return out;
}

/**
 * Open one emulated board in this tab and hand back its handle.
 *
 * Synchronous on purpose: a link is built at power-on, and the model's
 * transport hands back a CLOSED link rather than a promise. Resolving the
 * module URL, starting the worker, fetching the package and the board's
 * cold ROM boot all happen behind `isStarting`, which is what the studio
 * asks before it decides a board has given up; a failure among them is
 * reported by `takeError`, which the link turns into an error event.
 *
 * `moduleUrl` is optional — absent means "ask the page at power-on", which
 * is the only honest moment (see `resolveModuleUrl`).
 */
export function openEmuPort({ uid, mac, moduleUrl, manifestUrl, persistKey }) {
  const id = nextId++;
  const board = {
    id: uid,
    mac,
    chip: "esp32c6",
    boot: "rom-up",
    link: "usb-serial-jtag",
    persistKey: persistKey ?? uid,
    manifestUrl: manifestUrl ?? null,
    // The board's own config text, the CLI's words. `usb_host=attached` is
    // the cable in with the port open from power-on — a board whose port
    // started closed comes back from every reset with nothing draining
    // (`emulator_tab.js`'s TAB_BOARD says why at length).
    cfg: ["boot=rom-up", "strap=app", "usb_host=attached", `mac=${mac}`, ""].join("\n"),
  };
  const e = {
    uid,
    board,
    backing: null,
    port: null,
    chunks: [],
    error: null,
    starting: true,
    attached: true,
    open: false,
    dilation: null,
    queue: Promise.resolve(),
  };
  ports.set(id, e);
  e.ready = (async () => {
    const resolved = await resolveModuleUrl(moduleUrl);
    const { tabBacking } = await loadTabModule();
    e.backing = tabBacking({ moduleUrl: resolved, boards: [board] });
    const port = await e.backing.connect(board.id);
    port.onBytes((bytes) => e.chunks.push(bytes));
    port.onStats((stats) => {
      e.dilation = stats?.dilation ?? e.dilation;
    });
    e.port = port;
    return port;
  })();
  e.ready.then(
    () => {
      e.starting = false;
    },
    (error) => {
      e.starting = false;
      note(e, error);
    },
  );
  // Nothing else may run before the board exists.
  e.queue = e.ready.catch(() => {});
  return id;
}

/** Attach the cable if it is out, then open the byte channel. */
export function reopenEmuPort(id) {
  const e = entry(id);
  enqueue(e, async () => {
    const port = await e.ready;
    if (!e.attached) {
      await port.attach();
      e.attached = true;
    }
    if (!e.open) {
      await port.open();
      e.open = true;
    }
  });
}

/** Close the byte channel. The board keeps running; the cable stays in. */
export function closeEmuPort(id) {
  const e = entry(id);
  enqueue(e, async () => {
    const port = await e.ready;
    if (!e.open) return;
    await port.close();
    e.open = false;
  });
}

/** Write bytes to the board, after everything already queued. */
export function writeEmuPort(id, bytes) {
  const e = entry(id);
  // Copied now: the caller's view of wasm linear memory does not survive an
  // await, and this write is applied later by construction.
  const owned = bytes.slice();
  enqueue(e, async () => {
    const port = await e.ready;
    port.write(owned);
  });
}

/**
 * Drive DTR/RTS. `null` leaves a line untouched, so the reset dances reach
 * the machine pin-write for pin-write and it decodes the edges itself.
 */
export function signalsEmuPort(id, dtr, rts) {
  const e = entry(id);
  enqueue(e, async () => {
    const port = await e.ready;
    await port.signals({ dtr, rts });
  });
}

/** Everything the board has said since the last drain. */
export function takeEmuBytes(id) {
  return drain(entry(id));
}

/**
 * Whole lines the board has said, with the trailing partial left in the
 * buffer for whoever drains next.
 */
export function takeEmuLines(id) {
  const e = entry(id);
  const bytes = drain(e);
  let end = -1;
  for (let at = bytes.length - 1; at >= 0; at -= 1) {
    if (bytes[at] === 0x0a) {
      end = at;
      break;
    }
  }
  if (end < 0) {
    if (bytes.length > 0) e.chunks.push(bytes);
    return [];
  }
  const remainder = bytes.subarray(end + 1);
  if (remainder.length > 0) e.chunks.push(remainder.slice());
  return new TextDecoder()
    .decode(bytes.subarray(0, end))
    .split("\n")
    .map((line) => line.replace(/\r$/, ""))
    .filter((line) => line.length > 0);
}

/** The first failure since the last ask, or `null`. */
export function takeEmuError(id) {
  const e = entry(id);
  const error = e.error;
  e.error = null;
  return error;
}

/** Whether the board is still coming up (the worker, the module, the boot). */
export function isEmuStarting(id) {
  return entry(id).starting;
}

/**
 * How fast the board runs against wall time, as the worker last reported it
 * — `null` until the first stats tick. An honest fact, never a target.
 */
export function emuDilation(id) {
  return entry(id).dilation;
}

/** Reset the chip. Not a replug: the cable and the port stay as they are. */
export async function resetEmuPort(id) {
  const e = entry(id);
  await e.queue;
  const port = await e.ready;
  await port.reset();
}

/** The whole chip, as bytes. */
export async function getEmuFlash(id) {
  const e = entry(id);
  await e.queue;
  const port = await e.ready;
  return await port.getFlash();
}

/**
 * Erase the chip.
 *
 * `putFlash` is erase-then-write-at-zero, so an empty image is the erase on
 * its own — and the ABI answers a zero-length write with zero rather than
 * touching the chip (`tab_abi`'s `emu_flash_write`). Going through the port
 * keeps this file off `emulator_tab.js`'s private hub.
 */
export async function eraseEmuFlash(id) {
  const e = entry(id);
  await e.queue;
  const port = await e.ready;
  await port.putFlash(new Uint8Array(0));
}

/**
 * Fetch a packaged firmware build and write it into the emulated chip.
 *
 * The same manifest the esptool path fetches, read with the same rules —
 * `schemaVersion` 2 or refuse, `espflash-merged-image` or refuse — and then
 * written as the whole chip, because the packager emits ONE merged image at
 * address 0 (`lp-cli`'s `firmware package`) and the tab port's write verb is
 * the whole chip. An image that claims any other address is refused by name
 * rather than written at the wrong offset.
 *
 * The board is reset afterwards, so the card sees the boot the new image
 * produces rather than the one the old image is still running.
 */
export async function flashEmuPackage(id, manifestUrl) {
  const e = entry(id);
  await e.queue;
  const port = await e.ready;

  const manifestResponse = await fetch(manifestUrl, { cache: "no-store" });
  if (!manifestResponse.ok) {
    throw new Error(`${manifestUrl} answered ${manifestResponse.status}`);
  }
  const manifest = await manifestResponse.json();
  if (manifest?.schemaVersion !== 2) {
    // Version and refuse, never migrate: the packaged format's own rule.
    throw new Error(
      `${manifestUrl} is schemaVersion ${manifest?.schemaVersion}; this build reads 2`,
    );
  }
  if (manifest?.flash?.format !== "espflash-merged-image") {
    throw new Error(`${manifestUrl}: flash.format is ${manifest?.flash?.format}`);
  }
  const image = manifest.images?.[0];
  if (!image?.path) throw new Error(`${manifestUrl} names no image`);
  const address = Number(image.address ?? manifest.flash.address ?? 0);
  if (address !== 0) {
    throw new Error(
      `${manifestUrl} puts its image at 0x${address.toString(16)}; ` +
        "the tab backing writes the whole chip from zero",
    );
  }
  const imageUrl = new URL(image.path, manifestUrl).toString();
  const response = await fetch(imageUrl, { cache: "no-store" });
  if (!response.ok) throw new Error(`${imageUrl} answered ${response.status}`);
  const bytes = new Uint8Array(await response.arrayBuffer());
  if (bytes.length === 0) throw new Error(`${imageUrl} is empty`);

  await port.putFlash(bytes);
  await port.reset();
  return manifest.displayName ?? manifest.firmwareId ?? "the firmware";
}

/** `state` + `pins` + the tab's own counters, as one object. */
export async function emuProbes(id) {
  const e = entry(id);
  await e.queue;
  const port = await e.ready;
  return await port.probes();
}

/** End the board and its worker. The persisted image is untouched. */
export async function disposeEmuPort(id) {
  const e = ports.get(id);
  if (!e) return;
  ports.delete(id);
  try {
    await e.ready;
    await e.port?.dispose();
  } catch {
    // A board that never came up has nothing to dispose.
  }
  await e.backing?.dispose();
}

/** Forget a board's persisted 4 MiB image (the record's Forget). */
export async function deleteEmuFlash(persistKey) {
  const { deleteFlash } = await loadTabModule();
  await deleteFlash(persistKey);
}
