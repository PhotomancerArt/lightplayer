#!/usr/bin/env node
// What the tab's pacing loop DELIVERS, as a number, in node.
//
//   node scripts/emu/tab-dilation.mjs                 # after `just emu-c6-wasm`
//   node scripts/emu/tab-dilation.mjs --seconds 6
//
// `dilation` — guest microseconds advanced per wall microsecond elapsed — is
// the number the band shows and the number G1 quoted (0.45× for a boot, 0.34×
// for a blank chip, both interpreted). It is a property of the HOST and the
// WORKLOAD together, never a constant, and it moves when either the machine's
// core or the engine running it changes. So it is measured here rather than
// asserted anywhere: **nothing in this file or in any test compares a wall
// duration** (`lp-emu/esp/README.md` §Determinism).
//
// THIS IS A RIG, NOT THE PRODUCT. It mirrors `emulator_worker.js`'s pacing
// loop — the same four constants, the same drop-the-deficit rule, the same
// one-second sliding window — because that loop lives in a Worker and cannot
// be imported into node (it wants `navigator.storage` and a `postMessage`
// inbox). If the worker's rule changes, this file is what has to follow it;
// the worker is the definition.
//
// Four rows, and the third and fourth are why this exists after M7 P7:
//
//   rom-up boot      the mask ROM to `waiting for download` — INTERPRETED,
//                    because a `boot=rom-up` machine never installs a
//                    translated core (machine.rs, and M7's `jit_default.rs`
//                    rule 4: the ROM and the second-stage bootloader publish
//                    code without ever emitting a `fence.i`)
//   blank chip       the same ROM spinning on `invalid header: 0xffffffff`,
//                    which is what an unflashed `?emu=tab` board does all day
//   direct, host     the SAME ROM code direct-booted, so the machine does
//                    install a translated core: two translation events at
//                    create, and every block the guest runs through is
//                    wasm the JS host compiled
//   direct, no host  refused, and the refusal is the point — see below
//
// The fourth row cannot be measured: a machine that asks the host to compile
// and has no host attached gets `COMPILE_ERROR.NO_HOST` and `emu_create`
// fails. That is the loud failure M7 P7 wanted, and it is why the tab attaches
// the host unconditionally rather than only when it thinks it will be used.
import { readFileSync, existsSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { fileURLToPath, pathToFileURL } from "node:url";
import { dirname, resolve } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, "..", "..");
const MODULE = resolve(repo, "target/wasm32-wasip1/release/lp-emu-esp32c6.wasm");
const SHIM = resolve(repo, "lp-app/lpa-studio-web/public/lpa-link/emulator_wasi.js");
const ROM_ELF = resolve(repo, "lp-emu/esp/roms/esp32c6_rev0_rom.elf");

// `emulator_worker.js`'s own four, spelled the same way.
const SLICE_US = 40_000;
const CYCLES_PER_US = 160;
const DILATION_WINDOW_MS = 1_000;
const READ_CAP = 1 << 16;

const decoder = new TextDecoder();
const now = () => performance.now();

/**
 * `--stub-host`: the imports satisfied and no host behind them, which is the
 * only way to run the pre-attach path at all now that the module declares
 * `emu_host` (without these two functions it does not instantiate). Every
 * interpreted row is untouched by it — nothing calls `jit_compile` on a
 * `boot=rom-up` machine — and the translated row is refused with
 * `COMPILE_ERROR.NO_HOST`, in the machine's own words. A data URL rather than
 * a file, so a host that is deliberately wrong is not a file anyone can
 * mistake for one that is right.
 */
const STUB_HOST =
  "data:text/javascript," +
  encodeURIComponent(
    "export function makeJitHost() { return { imports: { emu_host: { " +
      "jit_compile: () => -5, jit_release: () => {} } }, " +
      "attach: () => ({ ok: false, stub: true }), events: [], slots: 0 }; }",
  );

/**
 * Pace `emu` against the wall for `seconds`, exactly as the Worker does, and
 * answer what it delivered.
 *
 * The loop has no `await`: in a Worker the tick exists so the inbox can land
 * between slices, and there is no inbox here. Everything else — the deficit,
 * the ceiling of one slice, the drop-and-re-anchor, the window — is the rule
 * as written in `emulator_worker.js` under THE PACING RULE.
 */
function pace(emu, seconds, { untilConsole = null } = {}) {
  let origin = now();
  let guestOrigin = 0;
  let lastMicros = 0;
  let window = [[origin, 0]];
  let drops = 0;
  let text = "";
  let reboots = 0;

  const sample = (at) => {
    window.push([at, Number(emu.micros())]);
    while (window.length > 2 && at - window[0][0] > DILATION_WINDOW_MS) window.shift();
  };
  const dilation = () => {
    if (window.length < 2) return null;
    const [firstWall, firstGuest] = window[0];
    const [lastWall, lastGuest] = window[window.length - 1];
    const wallUs = (lastWall - firstWall) * 1000;
    if (wallUs < 100_000) return null;
    const guest = lastGuest - firstGuest;
    return guest <= 0 ? 0 : guest / wallUs;
  };

  const reported = [];
  const started = origin;
  while (now() - started < seconds * 1000) {
    const wallUs = (now() - origin) * 1000;
    const guestUs = Number(emu.micros()) - guestOrigin;
    const deficit = wallUs - guestUs;
    if (deficit > 0) {
      const budgetUs = Math.min(deficit, SLICE_US);
      emu.run(Math.max(1, Math.round(budgetUs * CYCLES_PER_US)));
      text += decoder.decode(emu.uart0Read(READ_CAP), { stream: true });
      emu.usbRead(READ_CAP);
      const micros = Number(emu.micros());
      if (micros < lastMicros) {
        origin = now();
        guestOrigin = micros;
        window = [[origin, micros]];
        reboots += 1;
      }
      lastMicros = micros;
      if ((now() - origin) * 1000 - (Number(emu.micros()) - guestOrigin) > SLICE_US) {
        origin = now();
        guestOrigin = Number(emu.micros());
        drops += 1;
      }
    }
    sample(now());
    const d = dilation();
    if (d !== null) reported.push(d);
    if (untilConsole && text.includes(untilConsole)) break;
  }

  const sorted = [...reported].sort((a, b) => a - b);
  return {
    dilation: sorted.length ? sorted[Math.floor(sorted.length / 2)] : null,
    low: sorted[0] ?? null,
    high: sorted[sorted.length - 1] ?? null,
    samples: sorted.length,
    drops,
    reboots,
    guestUs: Number(emu.micros()),
    cycles: Number(emu.cycles()),
    translationEvents: emu.host.events.length,
    console: text,
  };
}

function cfg(lines) {
  return [...lines, ""].join("\n");
}

async function main() {
  const args = process.argv.slice(2);
  const seconds = Number(args[args.indexOf("--seconds") + 1]) || 4;
  const stub = args.includes("--stub-host");
  if (!existsSync(MODULE)) {
    console.error(`tab-dilation: ${MODULE} is missing — run \`just emu-c6-wasm\` first`);
    process.exit(2);
  }
  const { instantiateEmu } = await import(pathToFileURL(SHIM).href);
  const jitHostUrl = stub
    ? STUB_HOST
    : pathToFileURL(execFileSync(resolve(repo, "scripts/emu/jit-host-source.sh"), {
        encoding: "utf8",
      }).trim()).href;
  const module = await WebAssembly.compile(readFileSync(MODULE));

  const rows = [
    {
      name: "rom-up boot (interpreted)",
      cfg: cfg(["boot=rom-up", "strap=download", "reset_cause=usb-uart-hpsys", "usb_host=attached"]),
      app: null,
    },
    {
      name: "blank chip (interpreted)",
      cfg: cfg(["boot=rom-up", "strap=app", "usb_host=attached"]),
      app: null,
    },
    {
      name: "direct boot (translated)",
      cfg: cfg(["boot=direct", "strap=download", "reset_cause=usb-uart-hpsys", "usb_host=attached"]),
      app: new Uint8Array(readFileSync(ROM_ELF)),
    },
  ];

  console.log(`tab-dilation: ${MODULE}`);
  console.log(`tab-dilation: host ${stub ? "STUBBED, not attached" : jitHostUrl}`);
  console.log(`tab-dilation: ${seconds}s per row, slice ${SLICE_US} guest µs\n`);
  console.log("workload                      dilation   range          drops  events  guest ms");
  for (const row of rows) {
    let emu;
    try {
      emu = await instantiateEmu(module, { jitHostUrl });
      emu.create(row.cfg, new Uint8Array(0), row.app);
    } catch (error) {
      console.log(`${row.name.padEnd(29)} REFUSED — ${error.message.split("\n")[0]}`);
      continue;
    }
    const r = pace(emu, seconds);
    const fmt = (x) => (x === null ? "n/a" : x.toFixed(3) + "x");
    console.log(
      `${row.name.padEnd(29)} ${fmt(r.dilation).padEnd(10)} ` +
        `${(fmt(r.low) + "–" + fmt(r.high)).padEnd(15)}${String(r.drops).padEnd(7)}` +
        `${String(r.translationEvents).padEnd(8)}${(r.guestUs / 1000).toFixed(0)}`,
    );
    emu.destroy();
  }
}

main().catch((error) => {
  console.error(`tab-dilation: ${error?.stack ?? error}`);
  process.exit(1);
});
