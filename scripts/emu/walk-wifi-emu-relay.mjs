#!/usr/bin/env node
// THE CLOUD RELAY WALK WITH NO BOARD (Wi‑Fi relay plan P9;
// `just walk-wifi-emu relay`), and its protocol 1 lane
// (`just walk-wifi-emu relay-p1`, pictures-through-the-cloud plan P6).
//
// lp-cli-driven, not Studio (Studio's relay walk is `studio-relay`): the
// shipped C6 image under `lp-cli emu run --lan`, on a virtual LAN whose
// fixture names an uplink to an in-process relay (`lp-cloud-server`, mem
// store, a dev account), driven by `lp-cli/tests/emu_relay_link.rs` — the
// same cell CI runs in `test-emu-serve`. This lane runs it (filtered to its
// main test), keeps everything it printed
// (`target/walk-wifi-emu/relay/walk.log`), and then reads the BOARD's own
// words for each step, never only the test's verdict: its console (its
// `[relay]` lines, which reach the host over the USB link the cell holds),
// or its own status answer over USB (`NetworkStatus.relay`, which the cell
// prints as `the board answers (…): relay …`), or the hub's answers the
// cell prints:
//
//     R1 joined, no account key   → answers `relay noAccount`
//     R2 registered by itself     → `[relay] leg open to lightplayer.app`,
//                                   answers `relay connected`
//     P1 a picture right after    → `[relay] state=connected … pictures N idle`,
//        registering                and the hub holds it (relay protocol 2)
//     R3 a session through it     → `[relay] route N: … secure session opening`
//     P2 the project and its      → the hub lists `Basic`; Bob and a guest
//        colours                    read nothing; a lit picture of its lamps
//     P3 watched, then idle by    → `pictures N watched`, then a later
//        itself                     `pictures N idle`
//     P4 the heap while watched   → a heap row read through a relay session
//     R4 the same key on the LAN  → `closed (taken over by the same key)`
//     R5 someone else             → `the network link is in use — busy`
//     P5 lost at a deploy, back   → the new hub holds no picture, then one
//        with the board             once the board is back
//     R6 Cloud relay off / on     → answers `relay off`, then `relay connected`
//     P6 kept while offline       → the picture kept (`online false`), then
//                                   online again and newer
//     R7 the account key reset    → answers `relay refused: unknownAccount`
//
// The board's console has no line of its own for a relay state change (its
// heartbeat's `[relay] state=…` comes every five emulated seconds, and the
// last steps end before it prints); one was tried and cost a 32 KB flash
// page (docs/reports/2026-10-07-wifi-relay-emulator-walk.md), so the walk
// reads the status answers instead.
//
// and writes `summary.json` beside the log: the steps, the board's heap
// readings, the picture lines and the round-trip lines, with the
// configuration (`lp-emu:esp32c6:t1+net=lan@<commit>`). Times are WALL time
// on this host and never a gate (AGENTS.md: emulated runs never gate on wall
// clock).
//
// `--protocol-1` (`relay-p1`) walks a core built at LAST_PROTOCOL_1 (below)
// at this hub — the never-break property's walk: it registers, is routed and
// listed at `relayProto` 1, is never sent a protocol 2 frame (one `[relay]
// leg open` line across a minute of a member watching it and a relay session
// coming and going: a protocol 1 client drops its leg on any frame it does
// not expect), and comes back after a deploy, still at protocol 1. The
// image, cheapest first: `LP_RELAY_P1_ELF`; CI's own image of that commit
// (`scripts/ci/ci-images.py fetch <sha> esp32c6 --force`, the shipped
// `tree/ESP32C6_SERVER_RADIO/fw-esp32c6`; CI keeps it 7 days); else a build
// in a throwaway worktree at that commit (minutes; removed after). Output in
// `target/walk-wifi-emu/relay-p1/`.
//
// NOT CI (the cell is). Needs the rv32 target and a firmware build
// (`LP_EMU_BUILD_FW=1` builds it; `LP_CI_IMAGES` takes CI's). One
// foreground command, ~3–5 minutes on an M2 Max after the build (the
// protocol 1 lane: ~2 minutes after its image).

import { spawnSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import path from "node:path";
import process from "node:process";

const ROOT = path.resolve(path.dirname(new URL(import.meta.url).pathname), "../..");
const PROTOCOL_1 = process.argv.includes("--protocol-1");
const OUT = path.join(ROOT, "target/walk-wifi-emu", PROTOCOL_1 ? "relay-p1" : "relay");

/// The last relay protocol 1 commit: `main` at the merge base of the branch
/// that added relay protocol 2 (`claude/relay-pictures`, PR #1066) when its
/// walk ran (2026-10-09). Every `main` commit before protocol 2 landed
/// speaks protocol 1 and was released; this one is pinned so the lane always
/// walks the same core.
const LAST_PROTOCOL_1 = "f5039fb93c4696bf952453e2c71ae8ac66abd8e9";

const MAIN_TEST = "an_emulated_c6_reaches_lightplayer_app_through_the_lans_uplink";
const PROTOCOL_1_TEST = "a_protocol_1_core_at_the_new_hub_is_answered_as_before";

const ANSWERS = (what, relay) => new RegExp(`emu_relay_link: the board answers \\(${what}\\): relay ${relay}$`, "m");
const SAYS = (line) => new RegExp(`^emu_relay_link: ${line}`, "m");
const STEPS = [
  ["R1", "joined, no account key", [ANSWERS("no account", "noAccount")]],
  ["R2", "registered by itself", [/\[relay\] leg open to lightplayer\.app/, ANSWERS("relay connected after boot", "connected")]],
  ["P1", "a picture right after registering", [/\[relay\] state=connected .*pictures [1-9]\d* idle/, SAYS("listed at relayProto 2, firmware "), SAYS("the hub holds the board's picture")]],
  ["R3", "a session through the relay", [/\[relay\] route \d+: link \S+, secure session opening/]],
  ["P2", "the project's name and colours, members only", [SAYS('the hub lists the project "Basic"; Bob and a guest read no picture'), SAYS("the picture carries the project's colours")]],
  ["P3", "watched, then idle by itself", [{ first: /pictures \d+ watched/, then: /pictures \d+ idle/ }, SAYS("watched: seq "), SAYS("idle again by itself")]],
  ["P4", "the heap while watched, a relay session open", [SAYS("heap — projects/test/basic loaded, relay registered, pictures watched, a relay session open \\(read through it\\): free")]],
  ["R4", "the same key takes the session on the LAN", [/closed \(taken over by the same key\)/]],
  ["R5", "another key is told busy", [/the network link is in use — busy/]],
  ["P5", "lost at a deploy, back with the board", [SAYS("right after the deploy the new hub holds no picture"), SAYS("a picture is back with the board after the deploy")]],
  ["R6", "Cloud relay off, then on", [ANSWERS("relay off", "off"), ANSWERS("relay connected again", "connected")]],
  ["P6", "kept while offline", [SAYS("offline, the hub keeps the board's picture"), SAYS("back online, the picture is online again and newer")]],
  ["R7", "the account key reset: refused", [ANSWERS("refused", "refused: unknownAccount")]],
];
const SAYS_P1 = (line) => new RegExp(`^emu_relay_link \\(protocol 1\\): ${line}`, "m");
const PROTOCOL_1_STEPS = [
  ["Q1", "registered by itself", [/\[relay\] leg open to lightplayer\.app/, SAYS_P1("registered by itself")]],
  ["Q2", "listed at relayProto 1, no firmware, no project", [SAYS_P1("listed at relayProto 1, no firmware, no project")]],
  ["Q3", "routed: a relay session at the edit tier", [SAYS_P1("a relay session opened at the edit tier")]],
  ["Q4", "watched for a minute: never a picture", [SAYS_P1("watched for \\d+ s wall \\(\\d+ reads\\): the hub never held a picture for it")]],
  ["Q5", "never sent a protocol 2 frame: one leg, connected throughout", [SAYS_P1("one leg the whole window"), /\[relay\] state=connected routes=/]],
  ["Q6", "back after a deploy, still protocol 1", [SAYS_P1("back after a deploy, still relayProto 1, on one new leg")]],
];

/// Whether `log` holds `word`: a regex, or `{ first, then }` — `then`
/// somewhere after the first `first`.
function said(log, word) {
  if (word instanceof RegExp) return word.test(log);
  const first = log.search(word.first);
  return first >= 0 && word.then.test(log.slice(first));
}

function describe(word) {
  return word instanceof RegExp ? String(word) : `${word.first} then ${word.then}`;
}

/// The protocol 1 image: `LP_RELAY_P1_ELF`, CI's artifact, or a build.
function protocol1Elf() {
  if (process.env.LP_RELAY_P1_ELF) {
    return { elf: process.env.LP_RELAY_P1_ELF, how: "LP_RELAY_P1_ELF" };
  }
  const ci = path.join(ROOT, "target/ci-images", LAST_PROTOCOL_1.slice(0, 12), "esp32c6/tree/ESP32C6_SERVER_RADIO/fw-esp32c6");
  if (!existsSync(ci)) {
    // --force: its sources differ from this checkout's, which is the point.
    spawnSync("scripts/ci/ci-images.py", ["fetch", LAST_PROTOCOL_1, "esp32c6", "--force"], { cwd: ROOT, stdio: "inherit" });
  }
  if (existsSync(ci)) return { elf: ci, how: `CI's image of ${LAST_PROTOCOL_1.slice(0, 9)} (target/ci-images)` };
  // CI keeps its images 7 days: past that, build it where nothing else is.
  const tree = path.join(OUT, "worktree");
  const elf = path.join(OUT, "fw-esp32c6");
  if (existsSync(elf)) return { elf, how: `built here at ${LAST_PROTOCOL_1.slice(0, 9)} (kept)` };
  console.log(`walk-wifi-emu relay-p1: no CI image of ${LAST_PROTOCOL_1.slice(0, 9)}; building it in a throwaway worktree (minutes)`);
  rmSync(tree, { recursive: true, force: true });
  const run = (cmd, args, cwd) => {
    const result = spawnSync(cmd, args, { cwd, stdio: "inherit" });
    if (result.status !== 0) throw new Error(`${cmd} ${args.join(" ")} failed (${result.status})`);
  };
  run("git", ["worktree", "add", "--detach", tree, LAST_PROTOCOL_1], ROOT);
  try {
    run(
      "cargo",
      ["build", "--target", "riscv32imac-unknown-none-elf", "--profile", "release-esp32", "--features", "esp32c6,server,radio"],
      path.join(tree, "lp-fw/fw-esp32c6"),
    );
    copyFileSync(path.join(tree, "target/riscv32imac-unknown-none-elf/release-esp32/fw-esp32c6"), elf);
  } finally {
    spawnSync("git", ["worktree", "remove", "--force", tree], { cwd: ROOT, stdio: "inherit" });
  }
  return { elf, how: `built here at ${LAST_PROTOCOL_1.slice(0, 9)}` };
}

function main() {
  if (process.argv.includes("--help")) {
    console.log("usage: node scripts/emu/walk-wifi-emu.mjs relay | relay-p1");
    return;
  }
  mkdirSync(OUT, { recursive: true });
  const commit = spawnSync("git", ["rev-parse", "--short=9", "HEAD"], { cwd: ROOT, encoding: "utf8" }).stdout.trim();
  const configuration = `lp-emu:esp32c6:t1+net=lan@${commit}`;
  const image = PROTOCOL_1 ? protocol1Elf() : null;
  console.log(
    `walk-wifi-emu ${PROTOCOL_1 ? "relay-p1" : "relay"}: ${configuration} → ${path.relative(ROOT, OUT)}/` +
      (image ? `\n  the protocol 1 core: ${LAST_PROTOCOL_1.slice(0, 9)}, ${image.how}: ${image.elf}` : ""),
  );
  const test = PROTOCOL_1 ? PROTOCOL_1_TEST : MAIN_TEST;
  const cargo = ["cargo", "test", "-p", "lp-cli", "--test", "emu_relay_link", "--", "--include-ignored", "--nocapture", "--exact", test];
  const run = spawnSync(PROTOCOL_1 ? cargo[0] : "scripts/ci/ci-images.py", PROTOCOL_1 ? cargo.slice(1) : ["with", "esp32c6", "--", ...cargo], {
    cwd: ROOT,
    encoding: "utf8",
    env: PROTOCOL_1
      ? { ...process.env, LP_RELAY_P1_ELF: image.elf }
      : { ...process.env, LP_EMU_BUILD_FW: process.env.LP_EMU_BUILD_FW ?? "1" },
    maxBuffer: 256 * 1024 * 1024,
  });
  const log = `${run.stdout ?? ""}${run.stderr ?? ""}`;
  writeFileSync(path.join(OUT, "walk.log"), log);
  if (/emu_relay_link: skipped|emu_relay_link \(protocol 1\): not asked/.test(log)) {
    console.error("✗ the cell skipped (no firmware image) — see walk.log");
    process.exit(1);
  }

  const steps = (PROTOCOL_1 ? PROTOCOL_1_STEPS : STEPS).map(([id, what, words]) => {
    const missing = words.filter((word) => !said(log, word)).map(describe);
    return { id, what, ok: missing.length === 0, missing };
  });
  const prefix = PROTOCOL_1 ? "emu_relay_link (protocol 1): " : "emu_relay_link: ";
  const measured = log
    .split("\n")
    .filter((line) => line.startsWith(prefix) && !line.includes("the board answers"))
    .map((line) => line.slice(prefix.length));
  const heap = measured.filter((line) => line.startsWith("heap — "));
  const pictures = measured.filter((line) => /picture|watched|idle again|the board says/.test(line));
  const passed = run.status === 0 && /test result: ok\. 1 passed/.test(log);
  writeFileSync(
    path.join(OUT, "summary.json"),
    JSON.stringify(
      {
        configuration,
        ...(PROTOCOL_1 ? { protocol1: { commit: LAST_PROTOCOL_1, image: image.how, elf: image.elf } } : {}),
        passed,
        steps,
        heap,
        pictures,
        measured,
      },
      null,
      2,
    ) + "\n",
  );

  for (const step of steps) {
    console.log(`${step.ok ? "✓" : "✗"} ${step.id} ${step.what}${step.ok ? "" : ` — missing: ${step.missing.join(", ")}`}`);
  }
  for (const line of measured) console.log(`  ${line}`);
  if (!passed || steps.some((s) => !s.ok)) {
    console.error(`\n✗ the relay walk failed (cell exit ${run.status}) — ${path.relative(ROOT, path.join(OUT, "walk.log"))}`);
    process.exit(1);
  }
  console.log(
    PROTOCOL_1
      ? `\n✓ the protocol 1 relay walk (${configuration}, core ${LAST_PROTOCOL_1.slice(0, 9)}) finished: registered, listed at protocol 1, routed, watched with no picture, one leg throughout, back after a deploy — with no board.`
      : `\n✓ the relay walk (${configuration}) finished: registered with a picture, edited through the relay, watched then idle, taken over on the LAN, busy, the picture lost at a deploy and back, kept while off, refused — with no board.`,
  );
}

main();
