// A fake esptool stub, and the esptool-js loader surface over it that the
// flash code in `browser_esp32_flash.js` uses: READ_FLASH, FLASH_MD5 and the
// deflate write, the way the real stub speaks them. A READ_FLASH acked with
// its whole length ends the send loop with a digest, and a `lose(frame)`
// hook decides which of its frames the page never sees.
import assert from "node:assert/strict";

export function patterned(length) {
  // JSON-ish, with the bytes SLIP escapes and the stub flushes on.
  return Uint8Array.from({ length }, (_, i) => [0x7b, 0x0a, 0xc0, 0xdb, 0x22, 0xff][i % 6] ^ (i >> 8 & 0x10));
}

/// A loader double over a fake stub: the esptool-js surface the code under
/// test uses. Frames the stub sends reach the page unless `lose` says not.
export function fakeBoard(flash) {
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

export function md5(bytes) {
  return new Uint8Array(globalThis.process.getBuiltinModule("node:crypto").createHash("md5").update(bytes).digest());
}

export function hex(bytes) {
  return Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
}
