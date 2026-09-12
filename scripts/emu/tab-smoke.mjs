#!/usr/bin/env node
// The tab host's hermetic smoke: the emulator module, the page's own WASI
// shim, and the mask ROM. No browser, no firmware build, no network.
//
//   node scripts/emu/tab-smoke.mjs              # after `just emu-c6-wasm`
//   node scripts/emu/tab-smoke.mjs <module.wasm>
//
// This is the CI proof that the `emu_*` ABI, the shim over it and the slice
// loop all work (plan decision D18). It runs the SAME
// `public/lpa-link/emulator_wasi.js` the Worker runs — importing it, not a
// copy — so a shim bug cannot pass here and fail in a tab.
//
// What it asserts, and why each one is worth a line:
//
//   1. the module speaks `emu_abi=1`, and `_start` is never called
//   2. a blank rom-up board on the download strap boots the REAL mask ROM:
//      `ESP-ROM:esp32c6` and `waiting for download` on UART0. That single
//      transcript covers the ABI, the shim's `clock_time_get`, the flash
//      model, the UART model and the run loop at once — a stub would have to
//      reproduce Espressif's boot banner to fake it
//   3. the chip is `blank`, a written image header makes it `loaded`, and an
//      erase takes it back — the `flash` word's whole sequence
//   4. a control line answers the protocol's own `ok state cyc=…`
//   5. host bytes pushed between slices reach the guest and its reply comes
//      back: esptool's SYNC in, the ROM's SLIP reply out
//   6. the run is DETERMINISTIC in guest time — the same slices twice give
//      the same cycle count and the same console. No wall duration is
//      asserted anywhere (`lp-emu/esp/README.md` §Determinism)
//
// Exits non-zero on the first miss, with what it wanted and what it got.

import { readFileSync, existsSync } from "node:fs";
import { fileURLToPath, pathToFileURL } from "node:url";
import { dirname, resolve } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, "..", "..");

const DEFAULT_MODULE = resolve(repo, "target/wasm32-wasip1/release/lp-emu-esp32c6.wasm");
const SHIM = resolve(repo, "lp-app/lpa-studio-web/public/lpa-link/emulator_wasi.js");

/** esptool's SYNC, SLIP-framed — `lp-emu-esp32c6/scripts/rom-download-sync.usb`. */
const SYNC = Uint8Array.from([
  0xc0, 0x00, 0x08, 0x24, 0x00, 0x00, 0x00, 0x00, 0x00, 0x07, 0x07, 0x12, 0x20,
  ...new Array(32).fill(0x55),
  0xc0,
]);
/** One SLIP SYNC response; `UartConnCheck` sends eight. */
const SYNC_REPLY = Uint8Array.from([
  0xc0, 0x01, 0x08, 0x04, 0x00, 0x07, 0x07, 0x12, 0x20, 0x00, 0x00, 0x00, 0x00, 0xc0,
]);
const SYNC_REPLIES = 8;

/** The first four bytes of an ESP image header — magic, segments, spi mode. */
const IMAGE_HEAD = Uint8Array.from([0xe9, 0x06, 0x02, 0x2f]);

/** 20 MHz × 1 s of guest time per slice, and enough slices for the banner. */
const CYCLES_PER_US = 160;
const SLICE_CYCLES = 5_000_000;
const MAX_CYCLES = 200_000_000;
const READ_CAP = 1 << 16;

let failures = 0;

function check(claim, ok, detail = "") {
  if (ok) {
    console.log(`  ok   ${claim}`);
  } else {
    failures += 1;
    console.error(`  FAIL ${claim}${detail ? `\n       ${detail}` : ""}`);
  }
  return ok;
}

function hex(bytes) {
  return Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join(" ");
}

function same(a, b) {
  return a.length === b.length && a.every((byte, i) => byte === b[i]);
}

const decoder = new TextDecoder();

/**
 * Run slices until `stop(state)` says so or the cycle ceiling is reached,
 * collecting both channels. Guest time only: nothing here reads a clock.
 */
function runUntil(emu, stop) {
  const console0 = [];
  const usb = [];
  let ran = 0;
  while (ran < MAX_CYCLES) {
    const outcome = emu.run(SLICE_CYCLES);
    ran += SLICE_CYCLES;
    const out = emu.uart0Read(READ_CAP);
    if (out.length) console0.push(out);
    const bytes = emu.usbRead(READ_CAP);
    if (bytes.length) usb.push(bytes);
    const state = {
      outcome,
      console: decoder.decode(concat(console0)),
      usb: concat(usb),
      cycles: emu.cycles(),
    };
    if (outcome !== 0 || stop(state)) return state;
  }
  return {
    outcome: 0,
    console: decoder.decode(concat(console0)),
    usb: concat(usb),
    cycles: emu.cycles(),
  };
}

function concat(chunks) {
  const total = chunks.reduce((n, c) => n + c.length, 0);
  const all = new Uint8Array(total);
  let at = 0;
  for (const c of chunks) {
    all.set(c, at);
    at += c.length;
  }
  return all;
}

async function main() {
  const modulePath = process.argv[2] ? resolve(process.argv[2]) : DEFAULT_MODULE;
  if (!existsSync(modulePath)) {
    console.error(`tab-smoke: ${modulePath} is missing — run \`just emu-c6-wasm\` first`);
    process.exit(2);
  }
  const { instantiateEmu, EMU_ABI } = await import(pathToFileURL(SHIM).href);

  console.log(`tab-smoke: ${modulePath}`);
  const bytes = readFileSync(modulePath);
  const module = await WebAssembly.compile(bytes);

  // --- 1. the ABI, and no _start ------------------------------------------
  const exportNames = new Set(WebAssembly.Module.exports(module).map((x) => x.name));
  check("the module exports _start (the CLI is intact)", exportNames.has("_start"));
  check(
    "it exports no _initialize (a command binary, not a reactor)",
    !exportNames.has("_initialize"),
  );

  const emu = await instantiateEmu(module);
  check(`it speaks emu_abi=${EMU_ABI}`, emu.abiVersion === EMU_ABI, `got ${emu.abiVersion}`);

  // --- 2. the real mask ROM boots, with _start never called ---------------
  //
  // The DOWNLOAD strap, because that is what reaches the console: a blank
  // chip on the app strap prints `invalid header: 0xffffffff` forever, as it
  // does on the part (see the README's `kind=rom-up`).
  const config = [
    "boot=rom-up",
    "strap=download",
    "reset_cause=usb-uart-hpsys",
    "usb_host=attached",
    "mac=02:c6:7a:b0:00:01",
    "",
  ].join("\n");
  emu.create(config);
  check("emu_create built a blank rom-up board", true);

  const boot = runUntil(emu, (s) => s.console.includes("waiting for download"));
  check("the run reached its deadline rather than faulting", boot.outcome === 0, `outcome ${boot.outcome}`);
  check(
    "UART0 carries the mask ROM's banner",
    boot.console.includes("ESP-ROM:esp32c6"),
    JSON.stringify(boot.console.slice(0, 200)),
  );
  check(
    "…and its `waiting for download` line",
    boot.console.includes("waiting for download"),
    JSON.stringify(boot.console.slice(0, 400)),
  );
  check(
    "the download strap is named in the boot line",
    boot.console.includes("boot:0x16"),
    JSON.stringify(boot.console.slice(0, 400)),
  );

  // --- 4. a control line, answered by the protocol -------------------------
  const state = emu.control("state");
  check("`state` is answered `ok state cyc=…`", /^ok state cyc=\d+ us=\d+ /.test(state), state);
  check("`wait` is refused — it is the script's verb", emu.control("wait 5").startsWith("err "), emu.control("wait 5"));
  check(
    "an unknown verb is refused by name",
    emu.control("teleport").startsWith("err unknown command"),
    emu.control("teleport"),
  );

  // --- 5. host bytes in, guest reply out ----------------------------------
  emu.usbWrite(SYNC);
  const answered = runUntil(emu, (s) => s.usb.length >= SYNC_REPLY.length * SYNC_REPLIES);
  const wanted = concat(new Array(SYNC_REPLIES).fill(SYNC_REPLY));
  check(
    "the ROM answered esptool's SYNC over the in-process link",
    same(answered.usb, wanted),
    `wanted ${hex(wanted.slice(0, 28))}…\n       got    ${hex(answered.usb.slice(0, 28))}…`,
  );

  // --- 3. the flash word's sequence ---------------------------------------
  check("a 4 MiB chip", emu.flashLen() === 4 * 1024 * 1024, String(emu.flashLen()));
  check("…is `blank` to start with", !emu.flashHasImage());
  emu.flashWrite(0, IMAGE_HEAD);
  check("…`loaded` once an image header is at the reset vector", emu.flashHasImage());
  check(
    "…and the bytes read back as written",
    same(emu.flashRead(0, IMAGE_HEAD.length), IMAGE_HEAD),
    hex(emu.flashRead(0, IMAGE_HEAD.length)),
  );
  check("the host's write made the chip dirty", emu.flashDirty());
  emu.flashMarkSaved();
  check("…and saying so cleared the flag", !emu.flashDirty());
  emu.flashEraseChip();
  check("an erase takes it back to `blank`", !emu.flashHasImage());

  emu.destroy();

  // --- 6. determinism, in guest time --------------------------------------
  //
  // A second machine, the same config, the same slices: the same cycle count
  // and the same console. Nothing here times anything — a wall duration is
  // never a claim (README §Determinism).
  const second = await instantiateEmu(module);
  second.create(config);
  const again = runUntil(second, (s) => s.console.includes("waiting for download"));
  check(
    "a second run reaches the same guest cycle",
    again.cycles === boot.cycles,
    `${boot.cycles} vs ${again.cycles}`,
  );
  check("…with a byte-identical console", again.console === boot.console);
  second.destroy();

  if (failures > 0) {
    console.error(`\ntab-smoke: ${failures} claim(s) failed`);
    process.exit(1);
  }
  console.log("\ntab-smoke: every claim held");
}

main().catch((error) => {
  console.error(`tab-smoke: ${error?.stack ?? error}`);
  process.exit(1);
});
