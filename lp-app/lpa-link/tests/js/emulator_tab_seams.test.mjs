// A Devices-page emulated board asks the emulator for its LED seam, softly;
// the dev path's board asks for nothing (docs/adr/2026-10-05-emulator-seams.md).
// `emulator_tab_seams.rs` decides WHAT a board asks for; this proves the
// bridge writes it into the board's config text as the emulator reads it.

import assert from "node:assert/strict";
import { test } from "node:test";

import { TAB_BOARD } from "../../../lpa-studio-web/public/lpa-link/emulator_tab.js";
import { emuBoardCfg } from "../../src/providers/emulator_tab/emulator_tab_bridge.js";

const MAC = "02:00:00:00:00:01";

test("an end-user board asks for led=fast, softly", () => {
  const cfg = emuBoardCfg({ mac: MAC, seams: "led=fast" });
  assert.deepEqual(cfg.split("\n"), [
    "boot=rom-up",
    "strap=app",
    "usb_host=attached",
    `mac=${MAC}`,
    "seams_prefer=led=fast",
    "",
  ]);
  assert.ok(!/^seams=/m.test(cfg), "never a strict request: an old image must still boot");
});

test("?seams= passes its atoms through, and none asks for nothing", () => {
  assert.match(
    emuBoardCfg({ mac: MAC, seams: "led=fast+test=echo" }),
    /^seams_prefer=led=fast\+test=echo$/m,
  );
  for (const seams of ["none", "", undefined]) {
    assert.ok(!/seams/.test(emuBoardCfg({ mac: MAC, seams })), `seams=${seams}`);
  }
});

test("the dev path's tab board asks for no seams", () => {
  assert.match(TAB_BOARD.cfg, /^boot=rom-up$/m);
  assert.ok(!/seams/.test(TAB_BOARD.cfg), TAB_BOARD.cfg);
});
