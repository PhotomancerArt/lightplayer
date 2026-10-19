// The flash that survives a lossy link: what a C6 does to its own output over
// a phone's WebUSB, where it drops what the phone did not read in time
// (`docs/defects/2026-10-18-a-flash-over-webusb-on-a-phone-loses-the-boards-replies.md`).
//
// `node --test lp-app/lpa-link/tests/js/` — `just lpa-link-js-test`. A fake
// esptool stub speaks READ_FLASH, FLASH_MD5 and the deflate write the way the
// real one does — a READ_FLASH acked with its whole length ends the send loop
// with a digest — and a `lose(frame)` hook decides which of its frames the
// page never sees.
import assert from "node:assert/strict";
import { test } from "node:test";

import {
  READ_PACE,
  readFlashSafely,
  rebootFromFlash,
  writeFlashChecked,
} from "../../src/providers/browser_serial_esp32/browser_esp32_flash.js";
import { UsbCdcSerialPort } from "../../../lpa-studio-web/public/lpa-link/usb_cdc_serial.js";
import { FakeUsbDevice } from "./fake_usb_device.js";

const PHONE = READ_PACE["usb-cdc"];

test("a lost packet costs its segment, not the read", async () => {
  const flash = patterned(64 * 1024);
  const { loader, stub } = fakeBoard(flash);
  let lost = 0;
  stub.lose = (frame) => frame.kind === "data" && frame.index === 5 && lost < 1 && ++lost > 0;
  const bytes = await readFlashSafely(loader, 0, flash.length, null, PHONE);
  assert.deepEqual(bytes, flash);
  assert.equal(lost, 1);
  assert.ok(stub.aborts >= 1, "the stub's send loop was never ended");
  assert.ok(stub.packetSizes.includes(PHONE.packet / 2), `no segment shrank: ${stub.packetSizes}`);
});

test("a packet that comes up short is read again", async () => {
  const flash = patterned(16 * 1024);
  const { loader, stub } = fakeBoard(flash);
  let cut = 0;
  stub.corrupt = (packet, index) => (index === 3 && cut < 2 && ++cut ? packet.slice(0, packet.length - 10) : packet);
  assert.deepEqual(await readFlashSafely(loader, 0, flash.length, null, PHONE), flash);
  assert.equal(cut, 2);
});

test("a lost digest reads the segment again, with no ack sent to a stub that is done", async () => {
  const flash = patterned(8 * 1024);
  const { loader, stub } = fakeBoard(flash);
  let lost = 0;
  stub.lose = (frame) => frame.kind === "digest" && lost < 1 && ++lost > 0;
  assert.deepEqual(await readFlashSafely(loader, 0, flash.length, null, PHONE), flash);
  assert.equal(stub.strayAcks, 0, "an ack reached the stub's command loop");
});

test("a changed byte is caught by the segment's digest and read again", async () => {
  const flash = patterned(8 * 1024);
  const { loader, stub } = fakeBoard(flash);
  let changed = 0;
  stub.corrupt = (packet, index) => {
    if (index !== 2 || changed > 0) return packet;
    changed += 1;
    const out = packet.slice();
    out[0] ^= 1;
    return out;
  };
  assert.deepEqual(await readFlashSafely(loader, 0, flash.length, null, PHONE), flash);
});

test("clean segments grow the packets back to the largest", async () => {
  const flash = patterned(256 * 1024);
  const { loader, stub } = fakeBoard(flash);
  await readFlashSafely(loader, 0, flash.length, null, PHONE);
  assert.equal(Math.max(...stub.packetSizes), PHONE.max);
  assert.equal(stub.maxInFlight, 1, "more than one packet unacknowledged");
});

test("a read that keeps losing gives up in plain words", async () => {
  const flash = patterned(8 * 1024);
  const { loader, stub } = fakeBoard(flash);
  stub.lose = (frame) => frame.kind === "data" && frame.index === 0;
  await assert.rejects(readFlashSafely(loader, 0, flash.length, null, PHONE), /lost bytes 12 times in a row/);
});

test("progress reaches the whole length", async () => {
  const flash = patterned(20 * 1024 + 100);
  const { loader } = fakeBoard(flash);
  let last = 0;
  await readFlashSafely(loader, 0, flash.length, (done) => (last = done), PHONE);
  assert.equal(last, flash.length);
});

test("an image whose write reply was lost is written again and proven by its MD5", async () => {
  const flash = new Uint8Array(64 * 1024).fill(0xff);
  const { loader, stub } = fakeBoard(flash);
  stub.failWrites = 1;
  const image = patterned(10_001); // not a whole word: padded with 0xFF, as esptool-js writes it
  const lines = [];
  await writeFlashChecked(loader, [{ data: image, address: 0x1000 }], null, { writeLine: (l) => lines.push(l) });
  assert.deepEqual(flash.slice(0x1000, 0x1000 + image.length), image);
  assert.equal(stub.writes, 2);
  assert.match(lines.join("\n"), /attempt 1 of 3/);
});

test("an image that never reads back the same fails the flash", async () => {
  const flash = new Uint8Array(16 * 1024).fill(0xff);
  const { loader, stub } = fakeBoard(flash);
  stub.scribble = true;
  await assert.rejects(
    writeFlashChecked(loader, [{ data: patterned(4096), address: 0 }], null, null),
    /does not read back the same/,
  );
  assert.equal(stub.writes, 3);
});

test("a lost MD5 reply is asked again, not written again", async () => {
  const flash = new Uint8Array(16 * 1024).fill(0xff);
  const { loader, stub } = fakeBoard(flash);
  stub.loseMd5 = 1;
  await writeFlashChecked(loader, [{ data: patterned(4096), address: 0 }], null, null);
  assert.equal(stub.writes, 1);
  assert.equal(stub.md5Asked, 2);
});

test("an erase reboots the board out of the stub, over WebUSB", async () => {
  // A Factory reset from a phone (2026-10-19) erased the board and left it in
  // the stub, which says nothing, so the card read "not responding" until a
  // reload. esptool-js's own hard reset only releases RTS, and the connect's
  // reset sequence had already left both lines low.
  const device = new FakeUsbDevice();
  const port = new UsbCdcSerialPort(device);
  await port.open({ baudRate: 115200 });
  // Where esptool-js's USB-JTAG connect sequence leaves the lines.
  await port.setSignals({ dataTerminalReady: false, requestToSend: false });
  const before = device.lineLog.length;
  const lines = [];
  await rebootFromFlash({ device: port }, { writeLine: (l) => lines.push(l) });
  const pulse = device.lineLog.slice(before);
  assert.deepEqual(pulse, [
    { dtr: false, rts: true },
    { dtr: false, rts: false },
  ]);
  assert.equal(c6Resets(device.lineLog.slice(before - 1)), 1, "the C6 saw no reset");
  assert.ok(
    !pulse.some((l) => l.dtr && !l.rts),
    "the pulse passed through a CH34x's download-mode selection",
  );
  assert.match(lines.join("\n"), /Rebooting the board/);
  await port.close();
});

test("a board that re-enumerates on the reset does not fail the erase", async () => {
  const signals = [];
  const port = {
    async setSignals(next) {
      signals.push(next);
      if (signals.length === 2) throw new Error("The device has been lost.");
    },
  };
  const lines = [];
  await rebootFromFlash({ device: port }, { writeLine: (l) => lines.push(l) });
  assert.equal(signals.length, 2);
  assert.match(lines.join("\n"), /release went unanswered: The device has been lost/);
});

// --- helpers ---------------------------------------------------------------

/// How many times the lines entered the state a C6's USB-Serial-JTAG resets
/// the chip on: RTS asserted, DTR not (the ESP32-C6 TRM's USB Serial/JTAG
/// controller; esptool's hard reset drives the same).
function c6Resets(lineLog) {
  let resets = 0;
  for (let i = 1; i < lineLog.length; i += 1) {
    const was = lineLog[i - 1];
    const now = lineLog[i];
    if (now.rts && !now.dtr && !(was.rts && !was.dtr)) resets += 1;
  }
  return resets;
}

function patterned(length) {
  // JSON-ish, with the bytes SLIP escapes and the stub flushes on.
  return Uint8Array.from({ length }, (_, i) => [0x7b, 0x0a, 0xc0, 0xdb, 0x22, 0xff][i % 6] ^ (i >> 8 & 0x10));
}

/// A loader double over a fake stub: the esptool-js surface the code under
/// test uses. Frames the stub sends reach the page unless `lose` says not.
function fakeBoard(flash) {
  let inbox = [];
  const stub = fakeStub(flash, (frame, bytes) => {
    if (stub.lose(frame)) return;
    inbox.push(bytes);
  });
  const transport = {
    async read(_timeoutMs) {
      if (inbox.length === 0) throw new Error("Serial data stream stopped: Possible serial noise or corruption.");
      return inbox.shift();
    },
    async write(data) {
      stub.ack(data);
    },
    flushInput() {
      inbox = [];
    },
  };
  const loader = {
    ESP_READ_FLASH: 0xd2,
    transport,
    _intToByteArray: (v) => Uint8Array.of(v & 0xff, (v >> 8) & 0xff, (v >> 16) & 0xff, (v >>> 24) & 0xff),
    _appendArray: (a, b) => {
      const out = new Uint8Array(a.length + b.length);
      out.set(a);
      out.set(b, a.length);
      return out;
    },
    async checkCommand(_what, op, data) {
      assert.equal(op, 0xd2);
      const words = new DataView(data.buffer, data.byteOffset, data.byteLength);
      stub.start(words.getUint32(0, true), words.getUint32(4, true), words.getUint32(8, true), words.getUint32(12, true));
      return 0;
    },
    async writeFlash({ fileArray, compress }) {
      assert.equal(compress, true);
      stub.writes += 1;
      const [{ data, address }] = fileArray;
      const padded = new Uint8Array(Math.ceil(data.length / 4) * 4).fill(0xff);
      padded.set(data);
      if (stub.scribble) padded[0] ^= 0xff;
      flash.set(padded, address);
      if (stub.failWrites > 0) {
        stub.failWrites -= 1;
        throw new Error("No serial data received.");
      }
    },
    async flashMd5sum(address, length) {
      stub.md5Asked += 1;
      if (stub.loseMd5 > 0) {
        stub.loseMd5 -= 1;
        throw new Error("No serial data received.");
      }
      return hex(md5(flash.slice(address, address + length)));
    },
  };
  return { loader, stub };
}

/// The esptool stub's READ_FLASH loop: send while fewer than `inFlight`
/// packets are unacknowledged; once the acks reach the length — however few
/// packets were actually read — send the digest of what it sent and go back
/// to reading commands.
function fakeStub(flash, send) {
  const stub = {
    lose: () => false,
    corrupt: (packet) => packet,
    maxInFlight: 0,
    packetSizes: [],
    aborts: 0,
    strayAcks: 0,
    writes: 0,
    failWrites: 0,
    md5Asked: 0,
    loseMd5: 0,
    scribble: false,
    reading: false,
    start(offset, length, block, inFlight) {
      Object.assign(stub, { offset, length, block, inFlight, sent: 0, acked: 0, packets: 0, reading: true });
      stub.packetSizes.push(block);
      stub.pump();
    },
    pump() {
      while (stub.sent < stub.length && (stub.sent - stub.acked) / stub.block < stub.inFlight) {
        const n = Math.min(stub.block, stub.length - stub.sent);
        const packet = flash.slice(stub.offset + stub.sent, stub.offset + stub.sent + n);
        send({ kind: "data", index: stub.packets }, stub.corrupt(packet, stub.packets));
        stub.packets += 1;
        stub.sent += n;
        stub.maxInFlight = Math.max(stub.maxInFlight, Math.ceil((stub.sent - stub.acked) / stub.block));
      }
    },
    ack(data) {
      if (!stub.reading) {
        stub.strayAcks += 1;
        return;
      }
      stub.acked = new DataView(data.buffer, data.byteOffset, data.byteLength).getUint32(0, true);
      if (stub.acked >= stub.length) {
        if (stub.sent < stub.length) stub.aborts += 1;
        stub.reading = false;
        send({ kind: "digest" }, md5(flash.slice(stub.offset, stub.offset + stub.sent)));
        return;
      }
      stub.pump();
    },
  };
  return stub;
}

function md5(bytes) {
  return new Uint8Array(globalThis.process.getBuiltinModule("node:crypto").createHash("md5").update(bytes).digest());
}

function hex(bytes) {
  return Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
}
