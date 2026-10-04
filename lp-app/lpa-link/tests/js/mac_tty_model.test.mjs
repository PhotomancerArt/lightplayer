// The Mac serial-path model, and the flash read that survives it (G1-F2).
//
// `node --test lp-app/lpa-link/tests/js/` — `just lpa-link-js-test`. No
// browser, no board: a fake esptool stub speaks the READ_FLASH protocol into
// `MacTtyModel` (the model `virtual_serial.js` puts between an emulated board
// and the page on a Mac), and the page side reads through it as late as it
// can — a whole packet lands before the page reads any of it, which is how an
// emulated board delivers and the worst a Mac's reader can fall behind.
//
// The pair that matters: esptool-js's own read parameters (4 KB packets,
// 1024 in flight) lose bytes of erased flash through the model — the stall
// Yona's walk hit on a real Mac and the emulator walk never did — and
// `readFlashSafely`'s do not.
import assert from "node:assert/strict";
import { test } from "node:test";

import { MacTtyModel, TTY_SLOTS } from "../../../lpa-studio-web/public/lpa-link/mac_tty_model.js";
import { readFlashSafely } from "../../src/providers/browser_serial_esp32/browser_esp32_flash.js";

test("bytes that are not 0xFF are held back, never dropped, however late the page reads", () => {
  const tty = new MacTtyModel();
  const sent = Uint8Array.from({ length: 16 * 1024 }, (_, i) => i % 0xff); // 0x00..0xFE
  tty.arrive(sent);
  const got = drain(tty);
  assert.equal(tty.dropped, 0);
  assert.deepEqual(got, sent);
});

test("a 4 KB burst of 0xFF read late loses bytes, as on a Mac", () => {
  const tty = new MacTtyModel();
  tty.arrive(new Uint8Array(4096).fill(0xff));
  const got = drain(tty);
  assert.ok(tty.dropped > 0, "nothing dropped");
  assert.equal(got.length + tty.dropped, 4096);
});

test("a 0xFF burst that fits the tty doubled is delivered whole", () => {
  const tty = new MacTtyModel();
  // 384 data bytes + 2 SLIP delimiters, all costing their worst.
  const burst = new Uint8Array(386).fill(0xff);
  assert.ok(burst.length * 2 <= TTY_SLOTS);
  tty.arrive(burst);
  assert.equal(drain(tty).length, 386);
  assert.equal(tty.dropped, 0);
});

test("esptool-js's read parameters lose erased flash through the model", async () => {
  const flash = erasedFlashWithData(64 * 1024);
  const { loader } = fakeLoader(flash);
  await assert.rejects(naiveReadFlash(loader, 0, flash.length), /No serial data|lost/);
});

test("readFlashSafely reads erased flash through the model byte for byte", async () => {
  const flash = erasedFlashWithData(64 * 1024);
  const { loader, tty, stub } = fakeLoader(flash);
  let lastProgress = 0;
  const bytes = await readFlashSafely(loader, 0, flash.length, (done) => {
    lastProgress = done;
  });
  assert.deepEqual(bytes, flash);
  assert.equal(tty.dropped, 0);
  assert.equal(lastProgress, flash.length);
  assert.equal(stub.maxInFlight, 1, "more than one packet unacknowledged");
});

test("readFlashSafely fails loudly when a packet comes up short", async () => {
  const flash = erasedFlashWithData(8 * 1024);
  const { loader, stub } = fakeLoader(flash);
  stub.corrupt = (packet, index) => (index === 3 ? packet.slice(0, packet.length - 10) : packet);
  await assert.rejects(readFlashSafely(loader, 0, flash.length), /lost bytes/);
});

test("readFlashSafely refuses a read whose digest does not match", async () => {
  const flash = erasedFlashWithData(8 * 1024);
  const { loader, stub } = fakeLoader(flash);
  stub.corrupt = (packet, index) => {
    if (index !== 2) return packet;
    const changed = packet.slice();
    changed[0] ^= 0x01;
    return changed;
  };
  await assert.rejects(readFlashSafely(loader, 0, flash.length), /digest/);
});

// --- helpers ---------------------------------------------------------------

function drain(tty) {
  const out = [];
  for (;;) {
    const chunk = tty.read();
    if (chunk.length === 0 && tty.pending() === 0) break;
    if (chunk.length === 0) throw new Error("model stuck with bytes pending");
    out.push(...chunk);
  }
  return Uint8Array.from(out);
}

/// What a C6's filesystem region mostly is: erased sectors, with a little
/// data at the front.
function erasedFlashWithData(length) {
  const flash = new Uint8Array(length).fill(0xff);
  for (let i = 0; i < 3000; i += 1) flash[i] = (i * 7 + 3) & 0xff;
  return flash;
}

/// esptool-js 0.6.0's `readFlash`, parameters and all, over the same fake.
async function naiveReadFlash(loader, offset, length) {
  const int32 = (v) => loader._intToByteArray(v);
  let pkt = loader._appendArray(int32(offset), int32(length));
  pkt = loader._appendArray(pkt, int32(0x1000));
  pkt = loader._appendArray(pkt, int32(1024));
  await loader.checkCommand("read flash", loader.ESP_READ_FLASH, pkt);
  let received = 0;
  while (received < length) {
    const packet = await loader.transport.read(100);
    received += packet.length;
    await loader.transport.write(int32(received));
  }
  return received;
}

/// A loader double: the esptool-js surface `readFlashSafely` uses, over a
/// fake stub whose bytes reach the page only through `MacTtyModel`.
function fakeLoader(flash) {
  const tty = new MacTtyModel();
  const stub = fakeStub(flash, (bytes) => tty.arrive(slipEncode(bytes)));
  let pending = [];
  const transport = {
    // One SLIP packet. The page reads only here, i.e. as late as it can.
    async read(_timeoutMs) {
      for (;;) {
        const packet = slipTake(pending);
        if (packet) {
          pending = packet.rest;
          return packet.body;
        }
        const chunk = tty.read();
        if (chunk.length === 0) {
          throw new Error(pending.length ? "No serial data received." : "Serial data stream stopped.");
        }
        pending = pending.concat(Array.from(chunk));
      }
    },
    async write(data) {
      stub.ack(data);
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
  };
  return { loader, tty, stub };
}

/// The esptool stub's READ_FLASH loop: send while fewer than `inFlight`
/// packets are unacknowledged, MD5 digest once everything is acked. (Whether
/// the real stub counts packets or bytes, `inFlight = 1` is one packet, and
/// 1024 is at least one 4 KB packet: both lose through the model.)
function fakeStub(flash, send) {
  const stub = {
    corrupt: (packet) => packet,
    maxInFlight: 0,
    start(offset, length, block, inFlight) {
      Object.assign(stub, { offset, length, block, inFlight, sent: 0, acked: 0, packets: 0 });
      stub.pump();
    },
    pump() {
      while (stub.sent < stub.length && (stub.sent - stub.acked) / stub.block < stub.inFlight) {
        const n = Math.min(stub.block, stub.length - stub.sent);
        const packet = flash.slice(stub.offset + stub.sent, stub.offset + stub.sent + n);
        send(stub.corrupt(packet, stub.packets));
        stub.packets += 1;
        stub.sent += n;
        stub.maxInFlight = Math.max(stub.maxInFlight, Math.ceil((stub.sent - stub.acked) / stub.block));
      }
    },
    ack(data) {
      stub.acked = new DataView(data.buffer, data.byteOffset, data.byteLength).getUint32(0, true);
      if (stub.acked >= stub.length) {
        send(md5(flash.slice(stub.offset, stub.offset + stub.length)));
        return;
      }
      stub.pump();
    },
  };
  return stub;
}

function slipEncode(bytes) {
  const out = [0xc0];
  for (const b of bytes) {
    if (b === 0xc0) out.push(0xdb, 0xdc);
    else if (b === 0xdb) out.push(0xdb, 0xdd);
    else out.push(b);
  }
  out.push(0xc0);
  return Uint8Array.from(out);
}

function slipTake(bytes) {
  const start = bytes.indexOf(0xc0);
  if (start < 0) return null;
  const end = bytes.indexOf(0xc0, start + 1);
  if (end < 0) return null;
  const body = [];
  for (let i = start + 1; i < end; i += 1) {
    if (bytes[i] === 0xdb) {
      body.push(bytes[i + 1] === 0xdc ? 0xc0 : 0xdb);
      i += 1;
    } else {
      body.push(bytes[i]);
    }
  }
  return { body: Uint8Array.from(body), rest: bytes.slice(end + 1) };
}

/// node's own MD5, for the stub's digest (the code under test has its own).
function md5(bytes) {
  return new Uint8Array(globalThis.process.getBuiltinModule("node:crypto").createHash("md5").update(bytes).digest());
}
