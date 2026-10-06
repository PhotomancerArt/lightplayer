// Updating a tab-hosted emulated board keeps its files
// (docs/defects/2026-10-02-updating-a-tab-hosted-board-erases-its-files.md):
// the page's real `TabEmulatorPort` and the bridge's real package write, over
// a hub answering the worker's flash verbs as `emu_flash_*` does —
// `flash-erase` is the whole chip, `flash-write` erases only its own sectors.

import assert from "node:assert/strict";
import { test } from "node:test";

import { TabEmulatorPort } from "../../../lpa-studio-web/public/lpa-link/emulator_tab.js";
import { writeEmuPackage } from "../../src/providers/emulator_tab/emulator_tab_bridge.js";

test("an update writes the image and leaves the filesystem where it was", async () => {
  // The current table's filesystem, and the legacy one the C6 holds.
  for (const lpfs of [0x350000, 0x310000]) {
    const chip = new Uint8Array(4 << 20).fill(0xff);
    const files = Uint8Array.from({ length: 0x10000 }, (_, i) => (i * 31 + 7) & 0xff);
    chip.set(files, lpfs);
    // `--skip-padding`: the merged image ends where the app ends.
    const image = new Uint8Array(0x2c0000).fill(0x22);
    image.set([0xe9, 0x04, 0x02, 0x20]);
    let resets = 0;
    const port = Object.create(TabEmulatorPort.prototype);
    port._hub = { request: async (message) => hub(chip, message) };
    port.reset = async () => void (resets += 1);
    globalThis.fetch = async (url) =>
      String(url).endsWith("/manifest.json")
        ? { ok: true, json: async () => MANIFEST }
        : { ok: true, arrayBuffer: async () => image.slice().buffer };

    assert.equal(await writeEmuPackage(port, "./firmware/next/manifest.json"), "next");
    // Not `deepEqual`: its diff of a megabyte array is a hang, not a message.
    assert.equal(differs(chip, 0, image), -1, "the image is at 0x0");
    const at = differs(chip, lpfs, files);
    assert.equal(at, -1, `0x${lpfs.toString(16)}+${at} is 0x${chip[lpfs + at]?.toString(16)}`);
    assert.equal(resets, 1, "the board boots the new image");
  }
});

const MANIFEST = {
  schemaVersion: 2,
  displayName: "next",
  flash: { format: "espflash-merged-image", address: "0x0" },
  images: [{ path: "fw-esp32c6.bin", address: "0x0" }],
};

/** `emulator_worker.js`'s flash cases, over `chip`. */
function hub(chip, { type, offset = 0, bytes }) {
  if (type === "flash-erase") return void chip.fill(0xff);
  if (type !== "flash-write") throw new Error(`unexpected hub request ${type}`);
  const data = new Uint8Array(bytes);
  chip.fill(0xff, offset & ~0xfff, (offset + data.length + 0xfff) & ~0xfff);
  chip.set(data, offset);
  return { written: data.length };
}

/** The first index where `chip` at `offset` differs from `want`, or -1. */
function differs(chip, offset, want) {
  for (let i = 0; i < want.length; i++) if (chip[offset + i] !== want[i]) return i;
  return -1;
}
