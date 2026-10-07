#!/usr/bin/env node
// THE CLOUD RELAY WALK WITH NO BOARD (Wi‑Fi relay plan P9;
// `just walk-wifi-emu relay`).
//
// lp-cli-driven, not Studio (Studio's relay walk is M8's): the shipped C6
// image under `lp-cli emu run --lan`, on a virtual LAN whose fixture names
// an uplink to an in-process relay (`lp-cloud-server`, mem store, a dev
// account), driven by `lp-cli/tests/emu_relay_link.rs` — the same cell CI
// runs in `test-emu-serve`. This lane runs it, keeps everything it printed
// (`target/walk-wifi-emu/relay/walk.log`), and then reads the BOARD's own
// console words for each step, never only the test's verdict:
//
//     R1 joined, no account key   → `[relay] now no account`
//     R2 registered by itself     → `[relay] leg open to lightplayer.app`,
//                                   `[relay] now connected`
//     R3 a session through it     → `[relay] route N: … secure session opening`
//     R4 the same key on the LAN  → `closed (taken over by the same key)`
//     R5 someone else             → `the network link is in use — busy`
//     R6 Cloud relay off / on     → `[relay] now off`, then `now connected` again
//     R7 the account key reset    → `[relay] now refused: unknown account`
//
// and writes `summary.json` beside the log: the steps, the board's heap
// readings and the round-trip lines, with the configuration
// (`lp-emu:esp32c6:t1+net=lan@<commit>`). Times are WALL time on this host
// and never a gate (AGENTS.md: emulated runs never gate on wall clock).
//
// NOT CI (the cell is). Needs the rv32 target and a firmware build
// (`LP_EMU_BUILD_FW=1` builds it; `LP_CI_IMAGES` takes CI's). One
// foreground command, ~2 minutes on an M2 Max after the build.

import { spawnSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import process from "node:process";

const ROOT = path.resolve(path.dirname(new URL(import.meta.url).pathname), "../..");
const OUT = path.join(ROOT, "target/walk-wifi-emu/relay");

const STEPS = [
  ["R1", "joined, no account key", [/\[relay\] now no account/]],
  ["R2", "registered by itself", [/\[relay\] leg open to lightplayer\.app/, /\[relay\] now connected/]],
  ["R3", "a session through the relay", [/\[relay\] route \d+: link \S+, secure session opening/]],
  ["R4", "the same key takes the session on the LAN", [/closed \(taken over by the same key\)/]],
  ["R5", "another key is told busy", [/the network link is in use — busy/]],
  ["R6", "Cloud relay off, then on", [/\[relay\] now off/]],
  ["R7", "the account key reset: refused", [/\[relay\] now refused: unknown account/]],
];

function main() {
  if (process.argv.includes("--help")) {
    console.log("usage: node scripts/emu/walk-wifi-emu.mjs relay");
    return;
  }
  mkdirSync(OUT, { recursive: true });
  const commit = spawnSync("git", ["rev-parse", "--short=9", "HEAD"], { cwd: ROOT, encoding: "utf8" }).stdout.trim();
  const configuration = `lp-emu:esp32c6:t1+net=lan@${commit}`;
  console.log(`walk-wifi-emu relay: ${configuration} → ${path.relative(ROOT, OUT)}/`);
  const run = spawnSync(
    "scripts/ci/ci-images.py",
    [
      "with",
      "esp32c6",
      "--",
      "cargo",
      "test",
      "-p",
      "lp-cli",
      "--test",
      "emu_relay_link",
      "--",
      "--include-ignored",
      "--nocapture",
    ],
    {
      cwd: ROOT,
      encoding: "utf8",
      env: { ...process.env, LP_EMU_BUILD_FW: process.env.LP_EMU_BUILD_FW ?? "1" },
      maxBuffer: 256 * 1024 * 1024,
    },
  );
  const log = `${run.stdout ?? ""}${run.stderr ?? ""}`;
  writeFileSync(path.join(OUT, "walk.log"), log);
  if (/emu_relay_link: skipped/.test(log)) {
    console.error("✗ the cell skipped (no firmware image) — see walk.log");
    process.exit(1);
  }

  const steps = STEPS.map(([id, what, words]) => {
    const missing = words.filter((re) => !re.test(log)).map(String);
    return { id, what, ok: missing.length === 0, missing };
  });
  // R6's second half: connected again after the off (the board's words, in order).
  const off = log.search(/\[relay\] now off/);
  const r6 = steps.find((s) => s.id === "R6");
  if (off >= 0 && !/\[relay\] now connected/.test(log.slice(off))) {
    r6.ok = false;
    r6.missing.push("[relay] now connected after now off");
  }
  const measured = log
    .split("\n")
    .filter((line) => line.startsWith("emu_relay_link:"))
    .map((line) => line.slice("emu_relay_link: ".length));
  const passed = run.status === 0 && /test result: ok\. 1 passed/.test(log);
  writeFileSync(
    path.join(OUT, "summary.json"),
    JSON.stringify({ configuration, passed, steps, measured }, null, 2) + "\n",
  );

  for (const step of steps) {
    console.log(`${step.ok ? "✓" : "✗"} ${step.id} ${step.what}${step.ok ? "" : ` — missing: ${step.missing.join(", ")}`}`);
  }
  for (const line of measured) console.log(`  ${line}`);
  if (!passed || steps.some((s) => !s.ok)) {
    console.error(`\n✗ the relay walk failed (cell exit ${run.status}) — ${path.relative(ROOT, path.join(OUT, "walk.log"))}`);
    process.exit(1);
  }
  console.log(`\n✓ the relay walk (${configuration}) finished: registered, edited through the relay, taken over on the LAN, busy, back after a deploy, off and on, refused — with no board.`);
}

main();
