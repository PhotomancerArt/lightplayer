import { getPort, releasePort } from "./browser_serial.js";

const ESPTOOL_TRANSPORT_TRACING = false;

// The only `manifest.json` schemaVersion this build reads. Alpha posture is
// version + refuse: a v1 manifest fails loudly rather than half-decoding.
const FIRMWARE_MANIFEST_SCHEMA_VERSION = 2;

export function isSupported() {
  return Boolean(globalThis.navigator?.serial && globalThis.fetch);
}

export async function loadManifest(manifestPath) {
  const manifest = await loadFullManifest(manifestPath);
  return summarizeManifest(manifest, manifestPath);
}

export async function probeTarget(portId, esptoolModulePath) {
  if (!isSupported()) {
    throw new Error("Web Serial ESP32 probing is not supported in this browser.");
  }

  try {
    const port = await getPort(portId);
    await releasePort(portId);

    const { ESPLoader, Transport } = await loadEsptoolModule(esptoolModulePath);
    const logs = [];
    const terminal = terminalFor(logs, "esp32-probe");
    const transport = new Transport(port, ESPTOOL_TRANSPORT_TRACING);
    const loader = new ESPLoader({
      transport,
      baudrate: 115200,
      terminal,
      debugLogging: false,
    });

    try {
      const chipName = await loader.main();
      await loader.after("hard_reset");
      return {
        chipName: chipName ? String(chipName) : null,
        logs,
      };
    } finally {
      try {
        await transport.disconnect();
      } catch (error) {
        console.warn("[esp32-probe] transport disconnect failed", error);
      }
    }
  } catch (error) {
    reportFailure("esp32-probe", error);
    throw error;
  }
}

/**
 * Flash the merged image described by `manifestPath`.
 *
 * `knownChipIds` is `lpa_link::KNOWN_CHIP_IDS`, passed in rather than
 * restated here: it is an ordered table (most specific id first, bare
 * `esp32` last) whose ordering is the correctness argument for the chip
 * guard, and a second copy of it in another language is precisely the kind
 * of drift that lets a wrong image through.
 */
export async function flashFirmware(
  portId,
  manifestPath,
  esptoolModulePath,
  knownChipIds,
  onEvent,
) {
  if (!isSupported()) {
    throw new Error("Web Serial firmware flashing is not supported in this browser.");
  }

  const logs = [];
  const progress = [];
  const terminal = terminalFor(logs, "esp32-flash", onEvent);
  try {
    const manifest = await loadFullManifest(manifestPath);
    const imageFiles = await loadImageFiles(manifest, manifestPath);
    const { ESPLoader, Transport } = await loadEsptoolModule(esptoolModulePath);
    const port = await getPort(portId);
    await releasePort(portId);

    const transport = new Transport(port, ESPTOOL_TRANSPORT_TRACING);
    const loader = new ESPLoader({
      transport,
      baudrate: manifest.flash?.baudRate ?? 115200,
      terminal,
      debugLogging: false,
    });

    try {
      const chipName = await loader.main();
      // The image is chosen before the port is opened; the chip is only
      // known once the SYNC handshake answers. Between those two facts is
      // the last moment anything can stop a C6 image from being written
      // onto an S3 — after `writeFlash` the board is already bricked-ish
      // and the user has no idea why. Refuse loudly instead.
      assertChipMatchesManifest(chipName, manifest, manifestPath, knownChipIds);
      // Identity evidence, taken while the loader session is already open —
      // see `readBaseMac`. Before the write, deliberately: a flash that
      // fails halfway still learned which board it was talking to.
      const baseMac = await readBaseMac(loader);
      pushProgress(progress, onEvent, {
        label: "Connected to ESP32 bootloader",
        completedSteps: 1,
        totalSteps: 3,
        percent: 10,
      });
      await loader.writeFlash({
        fileArray: imageFiles.map((image) => ({
          data: image.data,
          address: image.address,
        })),
        flashSize: "keep",
        flashMode: "keep",
        flashFreq: "keep",
        eraseAll: false,
        compress: true,
        reportProgress: (fileIndex, written, total) => {
          const percent = total > 0 ? Math.round((written / total) * 100) : 0;
          pushProgress(progress, onEvent, {
            label: `Writing firmware image ${fileIndex + 1}/${imageFiles.length}`,
            completedSteps: 2,
            totalSteps: 3,
            percent,
          });
        },
      });
      pushProgress(progress, onEvent, {
        label: "Resetting flashed device",
        completedSteps: 3,
        totalSteps: 3,
        percent: 100,
      });
      // Between the write and the reset, over the stub that is still up:
      // the bootloader we just wrote cannot boot from the LP state a fresh
      // board's factory firmware leaves behind.
      await restoreLpAnalogI2cClock(loader, chipName, knownChipIds, terminal);
      await loader.after("hard_reset");
      return {
        manifest: summarizeManifest(manifest, manifestPath),
        chipName: chipName ? String(chipName) : null,
        baseMac,
        logs,
        progress: compactProgress(progress),
      };
    } finally {
      try {
        await transport.disconnect();
      } catch (error) {
        console.warn("[esp32-flash] transport disconnect failed", error);
      }
    }
  } catch (error) {
    reportFailure("esp32-flash", error, onEvent);
    throw error;
  }
}

export async function eraseDeviceFlash(portId, esptoolModulePath, onEvent) {
  if (!isSupported()) {
    throw new Error("Web Serial device erase is not supported in this browser.");
  }

  const logs = [];
  const progress = [];
  const terminal = terminalFor(logs, "esp32-erase", onEvent);
  try {
    const port = await getPort(portId);
    await releasePort(portId);

    const { ESPLoader, Transport } = await loadEsptoolModule(esptoolModulePath);
    const transport = new Transport(port, ESPTOOL_TRANSPORT_TRACING);
    const loader = new ESPLoader({
      transport,
      baudrate: 115200,
      terminal,
      debugLogging: false,
    });

    try {
      const chipName = await loader.main();
      pushProgress(progress, onEvent, {
        label: "Connected to ESP32 bootloader",
        completedSteps: 1,
        totalSteps: 3,
        percent: 10,
      });
      pushProgress(progress, onEvent, {
        label: "Erasing device flash",
        completedSteps: 2,
        totalSteps: 3,
        percent: 50,
      });
      await loader.eraseFlash();
      assertEraseCompleted(logs, "Device erase");
      pushProgress(progress, onEvent, {
        label: "Device flash erased",
        completedSteps: 3,
        totalSteps: 3,
        percent: 100,
      });
      return {
        chipName: chipName ? String(chipName) : null,
        logs,
        progress: compactProgress(progress),
      };
    } finally {
      try {
        await transport.disconnect();
      } catch (error) {
        console.warn("[esp32-erase] transport disconnect failed", error);
      }
    }
  } catch (error) {
    reportFailure("esp32-erase", error, onEvent);
    throw error;
  }
}

/**
 * Write the boot-control record — an instruction to the device's next boot,
 * delivered through flash because a device that cannot boot has no other
 * channel.
 *
 * `record` arrives already encoded (magic, version, flags, CRC) from
 * `lp-bootctl` on the Rust side. Do NOT reconstruct it here: the firmware
 * that reads these bytes cannot renegotiate the format at runtime, so one
 * implementation of it is the point.
 *
 * One `writeFlash` call, deliberately. Its FLASH_BEGIN erases the sectors it
 * is about to write, so splitting the record across two writes would have the
 * second erase the first.
 */
export async function writeBootControl(portId, esptoolModulePath, address, record, onEvent) {
  if (!isSupported()) {
    throw new Error("Web Serial boot-control write is not supported in this browser.");
  }

  const logs = [];
  const progress = [];
  const terminal = terminalFor(logs, "esp32-bootctl", onEvent);
  try {
    const port = await getPort(portId);
    await releasePort(portId);

    const { ESPLoader, Transport } = await loadEsptoolModule(esptoolModulePath);
    const transport = new Transport(port, ESPTOOL_TRANSPORT_TRACING);
    const loader = new ESPLoader({
      transport,
      baudrate: 115200,
      terminal,
      debugLogging: false,
    });

    try {
      const chipName = await loader.main();
      pushProgress(progress, onEvent, {
        label: "Connected to ESP32 bootloader",
        completedSteps: 1,
        totalSteps: 2,
        percent: 25,
      });
      await loader.writeFlash({
        fileArray: [{ data: new Uint8Array(record), address }],
        flashSize: "keep",
        flashMode: "keep",
        flashFreq: "keep",
        eraseAll: false,
        // MUST be true: esptool-js 0.6.0 implements ONLY deflate writes and
        // throws "Yet to handle Non Compressed Writes" otherwise — found on
        // the bench (2026-07-31) as "Arming safe mode failed". The ROM/stub
        // accepts FLASH_DEFL_BEGIN in download mode; flashFirmware above has
        // always used it.
        compress: true,
      });
      // Verify by READBACK, not by esptool's flash-ID warning. On the bench
      // (2026-07-31, ESP32-C6 rev 2 over USB-Serial-JTAG) the ID probe reads
      // 0 and esptool prints "Failed to communicate with the flash chip" —
      // while actual stub reads AND writes work fine (the 2.9 MB firmware
      // write and the filesystem backup both succeeded on the same plug
      // session). The ID probe drives per-chip SPI registers; the stub's
      // FLASH_DEFL_*/READ_FLASH commands are a different path. Gating on the
      // warning blocked every boot-control write on that board; comparing
      // the record byte-for-byte in flash is the guarantee we actually want.
      const readBack = await readFlashSafely(loader, address, record.byteLength ?? record.length);
      const written = new Uint8Array(record);
      const matches =
        readBack &&
        readBack.length === written.length &&
        written.every((byte, i) => readBack[i] === byte);
      if (!matches) {
        throw new Error(
          `Boot-control record readback mismatch at 0x${address.toString(16)}: ` +
            `wrote [${Array.from(written, (b) => b.toString(16).padStart(2, "0")).join(" ")}], ` +
            `read ${readBack ? `[${Array.from(readBack, (b) => b.toString(16).padStart(2, "0")).join(" ")}]` : "nothing"}`,
        );
      }
      pushProgress(progress, onEvent, {
        label: "Boot-control record written",
        completedSteps: 2,
        totalSteps: 2,
        percent: 100,
      });
      await loader.after("hard_reset");
      return {
        chipName: chipName ? String(chipName) : null,
        logs,
        progress: compactProgress(progress),
      };
    } finally {
      try {
        await transport.disconnect();
      } catch (error) {
        console.warn("[esp32-bootctl] transport disconnect failed", error);
      }
    }
  } catch (error) {
    reportFailure("esp32-bootctl", error, onEvent);
    throw error;
  }
}

/**
 * Read the device's filesystem partition back to the host, verbatim.
 *
 * The partition is per board — a device that cannot boot cannot be asked
 * where its files are, but its partition table can. So the table at 0x8000
 * is read first, in this session, and handed to `resolveRegion(tableBytes)`
 * on the Rust side, which parses it and answers the `lpfs` row (or null for
 * a table that has none). No layout lives in this file.
 *
 * **Default baud, deliberately.** These parts speak USB-Serial-JTAG, where
 * the baud parameter is meaningless and negotiating a higher one is measurably
 * SLOWER (3.2 s vs 4.2 s for 960 KB on the bench). Do not raise it.
 */
export async function readRawFilesystem(portId, esptoolModulePath, resolveRegion, onEvent) {
  if (!isSupported()) {
    throw new Error("Web Serial filesystem backup is not supported in this browser.");
  }

  const logs = [];
  const progress = [];
  const terminal = terminalFor(logs, "esp32-fsread", onEvent);
  try {
    const port = await getPort(portId);
    await releasePort(portId);

    const { ESPLoader, Transport } = await loadEsptoolModule(esptoolModulePath);
    const transport = new Transport(port, ESPTOOL_TRANSPORT_TRACING);
    const loader = new ESPLoader({
      transport,
      baudrate: 115200,
      terminal,
      debugLogging: false,
    });

    try {
      const chipName = await loader.main();
      pushProgress(progress, onEvent, {
        label: "Connected to ESP32 bootloader",
        completedSteps: 0,
        totalSteps: 1,
        percent: 0,
      });
      // The region is the device's OWN table's `lpfs` row (Rust parses it):
      // a C6 flashed before the 2026-10 repartition and one flashed after
      // keep their files in different places.
      const partitionTable = await readFlashSafely(loader, PARTITION_TABLE_OFFSET, PARTITION_TABLE_LEN);
      const region = resolveRegion(new Uint8Array(partitionTable));
      if (!region) {
        throw new Error(
          `The ${chipName ?? "device"} holds no LightPlayer filesystem partition ` +
            "(its partition table has no lpfs row).",
        );
      }
      const image = await readFlashSafely(
        loader,
        region.offset,
        region.length,
        (bytesRead, totalBytes) => {
          const percent = totalBytes > 0 ? Math.round((bytesRead / totalBytes) * 100) : 0;
          pushProgress(progress, onEvent, {
            label: "Reading filesystem",
            completedSteps: bytesRead,
            totalSteps: totalBytes,
            percent,
          });
        },
      );
      if (!image || image.length !== region.length) {
        // A short image would mount as a damaged filesystem and look like
        // data loss on the device rather than a truncated transfer.
        throw new Error(
          `Filesystem read returned ${image ? image.length : 0} bytes, expected ${region.length}.`,
        );
      }
      pushProgress(progress, onEvent, {
        label: "Filesystem read",
        completedSteps: region.length,
        totalSteps: region.length,
        percent: 100,
      });
      return {
        chipName: chipName ? String(chipName) : null,
        offset: region.offset,
        length: region.length,
        image,
        partitionTable,
        logs,
        progress: compactProgress(progress),
      };
    } finally {
      try {
        await transport.disconnect();
      } catch (error) {
        console.warn("[esp32-fsread] transport disconnect failed", error);
      }
    }
  } catch (error) {
    reportFailure("esp32-fsread", error, onEvent);
    throw error;
  }
}

// Where every ESP32 flasher writes the partition table, and its length.
const PARTITION_TABLE_OFFSET = 0x8000;
const PARTITION_TABLE_LEN = 0xc00;

/**
 * Read what the board holds relative to the package at `manifestPath`
 * (plan lp2025/2026-10-01-1843-c6-repartition, the layout migration).
 *
 * A dumb executor (`docs/debt/web-serial-js-untestable.md`): WHAT to read
 * is decided in Rust. `nextRead(chipName, targetTable, lastReadBytes)` is
 * called first with `lastReadBytes = null`, then once after every read with
 * that read's bytes, and answers `{offset, length}` or null when it has seen
 * enough. Rust keeps the bytes; this returns only what it alone knows.
 *
 * Ends WITHOUT a reset: the chip stays in ROM download, so the board does
 * not boot its old firmware (which writes to its filesystem) between this
 * read and the write that follows it. A caller that ends up not writing
 * resets the board itself.
 */
export async function inspectLayout(
  portId,
  manifestPath,
  esptoolModulePath,
  knownChipIds,
  nextRead,
  onEvent,
) {
  if (!isSupported()) {
    throw new Error("Web Serial firmware updates are not supported in this browser.");
  }
  const logs = [];
  const progress = [];
  const terminal = terminalFor(logs, "esp32-inspect", onEvent);
  try {
    const manifest = await loadFullManifest(manifestPath);
    const imageFiles = await loadImageFiles(manifest, manifestPath);
    const merged = imageFiles.find((image) => image.address === 0);
    if (!merged || merged.data.length < PARTITION_TABLE_OFFSET + PARTITION_TABLE_LEN) {
      throw new Error("The firmware package has no merged image at 0x0.");
    }
    // Copies, never views (the Safari OPFS lesson applies to every async
    // sink): these bytes cross into Rust and into esptool-js.
    const targetTable = merged.data.slice(
      PARTITION_TABLE_OFFSET,
      PARTITION_TABLE_OFFSET + PARTITION_TABLE_LEN,
    );
    const targetImageLen = Math.max(
      ...imageFiles.map((image) => image.address + image.data.length),
    );
    const { ESPLoader, Transport } = await loadEsptoolModule(esptoolModulePath);
    const port = await getPort(portId);
    await releasePort(portId);
    const transport = new Transport(port, ESPTOOL_TRANSPORT_TRACING);
    const loader = new ESPLoader({ transport, baudrate: 115200, terminal, debugLogging: false });
    try {
      const chipName = await loader.main();
      assertChipMatchesManifest(chipName, manifest, manifestPath, knownChipIds);
      const baseMac = await readBaseMac(loader);
      let read = nextRead(chipName ? String(chipName) : "", new Uint8Array(targetTable), null);
      while (read) {
        const label = read.length > 0x10000 ? "Reading the board's files" : "Reading the board's layout";
        const bytes = await readFlashSafely(loader, read.offset, read.length, (done, total) => {
          pushProgress(progress, onEvent, {
            label,
            completedSteps: done,
            totalSteps: total,
            percent: total > 0 ? Math.round((done / total) * 100) : 0,
          });
        });
        if (!bytes || bytes.length !== read.length) {
          throw new Error(
            `Flash read at 0x${read.offset.toString(16)} returned ${bytes ? bytes.length : 0} ` +
              `bytes, expected ${read.length}.`,
          );
        }
        read = nextRead(chipName ? String(chipName) : "", new Uint8Array(targetTable), new Uint8Array(bytes));
      }
      return {
        chipName: chipName ? String(chipName) : null,
        baseMac,
        targetImageLen,
        logs,
        progress: compactProgress(progress),
      };
    } finally {
      try {
        await transport.disconnect();
      } catch (error) {
        console.warn("[esp32-inspect] transport disconnect failed", error);
      }
    }
  } catch (error) {
    reportFailure("esp32-inspect", error, onEvent);
    throw error;
  }
}

/**
 * Execute a flash plan decided in Rust, in one bootloader session.
 *
 * `steps` is data: `{kind: "firmware"}` (the package's images),
 * `{kind: "erase", offset, length}`, `{kind: "write", offset, data}`,
 * `{kind: "verify", offset, length}`. Before anything is written,
 * `approveBoard(chipName, baseMac)` (Rust) answers null to proceed or a
 * refusal message — a different board plugged in since the inspection must
 * not receive this one's files. A verify step reads the region back and asks
 * `afterVerify(index, bytes)` (Rust, which compares) for the index to run
 * next: the following step, the start of the filesystem steps (the one
 * retry), or -1 to fail. An erase is written as 0xff sectors: a flash write
 * erases the sectors it touches first, so the result is an erased region by
 * the same path every write already takes.
 */
export async function executePlan(
  portId,
  manifestPath,
  esptoolModulePath,
  knownChipIds,
  steps,
  approveBoard,
  afterVerify,
  onEvent,
) {
  if (!isSupported()) {
    throw new Error("Web Serial firmware updates are not supported in this browser.");
  }
  const logs = [];
  const progress = [];
  const terminal = terminalFor(logs, "esp32-plan", onEvent);
  try {
    const manifest = await loadFullManifest(manifestPath);
    const imageFiles = await loadImageFiles(manifest, manifestPath);
    const { ESPLoader, Transport } = await loadEsptoolModule(esptoolModulePath);
    const port = await getPort(portId);
    await releasePort(portId);
    const transport = new Transport(port, ESPTOOL_TRANSPORT_TRACING);
    const loader = new ESPLoader({
      transport,
      baudrate: manifest.flash?.baudRate ?? 115200,
      terminal,
      debugLogging: false,
    });
    try {
      const chipName = await loader.main();
      assertChipMatchesManifest(chipName, manifest, manifestPath, knownChipIds);
      const baseMac = await readBaseMac(loader);
      const refusal = approveBoard(chipName ? String(chipName) : "", baseMac);
      if (refusal) {
        throw new Error(String(refusal));
      }
      const write = async (fileArray, label) => {
        await loader.writeFlash({
          fileArray,
          flashSize: "keep",
          flashMode: "keep",
          flashFreq: "keep",
          eraseAll: false,
          // esptool-js 0.6.0 implements only deflate writes.
          compress: true,
          reportProgress: (_index, written, total) => {
            pushProgress(progress, onEvent, {
              label,
              completedSteps: written,
              totalSteps: total,
              percent: total > 0 ? Math.round((written / total) * 100) : 0,
            });
          },
        });
      };
      let index = 0;
      while (index < steps.length) {
        const step = steps[index];
        if (step.kind === "firmware") {
          await write(
            imageFiles.map((image) => ({ data: image.data, address: image.address })),
            "Writing firmware",
          );
        } else if (step.kind === "erase") {
          await write(
            [{ data: new Uint8Array(step.length).fill(0xff), address: step.offset }],
            "Moving files",
          );
        } else if (step.kind === "write") {
          await write([{ data: new Uint8Array(step.data), address: step.offset }], "Moving files");
        } else if (step.kind === "verify") {
          const bytes = await readFlashSafely(loader, step.offset, step.length, (done, total) => {
            pushProgress(progress, onEvent, {
              label: "Verifying files",
              completedSteps: done,
              totalSteps: total,
              percent: total > 0 ? Math.round((done / total) * 100) : 0,
            });
          });
          const next = afterVerify(index, new Uint8Array(bytes ?? []));
          if (next < 0) {
            throw new Error(
              `The files written at 0x${step.offset.toString(16)} did not read back the same, twice.`,
            );
          }
          index = next;
          continue;
        } else {
          throw new Error(`Unknown flash step kind ${step.kind}.`);
        }
        index += 1;
      }
      pushProgress(progress, onEvent, { label: "Resetting the board", percent: 100 });
      await restoreLpAnalogI2cClock(loader, chipName, knownChipIds, terminal);
      await loader.after("hard_reset");
      return {
        manifest: summarizeManifest(manifest, manifestPath),
        chipName: chipName ? String(chipName) : null,
        baseMac,
        logs,
        progress: compactProgress(progress),
      };
    } finally {
      try {
        await transport.disconnect();
      } catch (error) {
        console.warn("[esp32-plan] transport disconnect failed", error);
      }
    }
  } catch (error) {
    reportFailure("esp32-plan", error, onEvent);
    throw error;
  }
}

/// The packet size of every flash read this file makes, and why it is small.
///
/// esptool-js's own `readFlash` asks the stub for 4 KB packets with 1024 of
/// them in flight. On a Mac, Chromium's Web Serial opens the port with
/// `PARMRK` set, so the kernel's tty queue stores each `0xFF` byte TWICE in
/// its 1,024 slots, and once doubling pushes the queue past 1,020 it drops
/// bytes silently (`docs/defects/2026-09-26-web-serial-on-macos-drops-bytes-of-packed-frames.md`).
/// Flash is mostly `0xFF` — an erased sector is nothing else — so a 4 KB
/// packet of it is 8 KB of tty slots, and the read loses a run of bytes the
/// first time the page reads late. esptool-js then waits for the rest of a
/// packet the stub believes it sent, the stub waits for the ack, and the read
/// stalls for esptool-js's 100 s packet timeout (G1-F2, 2026-10-02:
/// `docs/defects/2026-10-02-studio-reading-a-boards-files-stalls-on-a-mac.md`).
///
/// One packet in flight of at most 384 bytes is at most 770 slots — every
/// data byte costs at most two (a doubled `0xFF`, or a SLIP-escaped `0xC0` /
/// `0xDB`), plus the two SLIP delimiters — under the 1,020 the tty can hold
/// with no reader at all, so nothing can be dropped however late the page
/// reads; the stub sends the next packet only after the ack, which the page
/// writes only once it has read the whole packet out of the tty. The cost is
/// a round trip per packet: 960 KB took 14.2 s at 384 bytes (21.7 s at 256)
/// over a desk C6's Web Serial in headless Brave on macOS (2026-10-02), where
/// esptool-js's 4 KB packets took 4.8 s on a lossless path and, on the Mac's
/// real one, stalled at 310 KB.
const SAFE_READ_PACKET = 384;
/// One packet unacknowledged. Whether the stub counts this in packets or in
/// bytes, 1 means it sends a packet and then waits for the ack.
const SAFE_READ_IN_FLIGHT = 1;
/// A packet that has not arrived in this long is lost, not slow: fail with a
/// message instead of esptool-js's 100 s silence.
const SAFE_READ_PACKET_TIMEOUT_MS = 3000;

/// Read `length` bytes of flash at `offset` over the stub, in packets a
/// Mac's serial path cannot drop (see [`SAFE_READ_PACKET`]). Every packet's
/// length is checked as it arrives and the stub's closing MD5 digest is
/// compared with the bytes, so a read that lost or changed anything fails
/// loudly rather than returning a short or wrong image.
///
/// `onProgress(bytesRead, totalBytes)` is called every 4 KB and at the end.
export async function readFlashSafely(loader, offset, length, onProgress = null) {
  const int32 = (value) => loader._intToByteArray(value);
  let request = loader._appendArray(int32(offset), int32(length));
  request = loader._appendArray(request, int32(SAFE_READ_PACKET));
  request = loader._appendArray(request, int32(SAFE_READ_IN_FLIGHT));
  const status = await loader.checkCommand("read flash", loader.ESP_READ_FLASH, request);
  if (status != 0) {
    throw new Error(`The board refused to read flash at 0x${offset.toString(16)}: ${status}`);
  }
  const bytes = new Uint8Array(length);
  let received = 0;
  while (received < length) {
    const packet = await loader.transport.read(SAFE_READ_PACKET_TIMEOUT_MS);
    const expected = Math.min(SAFE_READ_PACKET, length - received);
    if (!(packet instanceof Uint8Array) || packet.length !== expected) {
      throw new Error(
        `Flash read at 0x${offset.toString(16)} lost bytes: a packet of ` +
          `${packet?.length ?? 0} bytes where ${expected} were sent ` +
          `(${received} of ${length} read).`,
      );
    }
    bytes.set(packet, received);
    received += packet.length;
    await loader.transport.write(int32(received));
    // Every 4 KB, not every packet: the card does not need 16 times the
    // events esptool-js's own reads sent.
    if (onProgress && (received % 4096 === 0 || received === length)) {
      onProgress(received, length);
    }
  }
  const digest = await loader.transport.read(SAFE_READ_PACKET_TIMEOUT_MS);
  const expectedDigest = md5Hex(bytes);
  const gotDigest = digest instanceof Uint8Array && digest.length === 16 ? toHex(digest) : null;
  if (gotDigest !== expectedDigest) {
    throw new Error(
      `Flash read at 0x${offset.toString(16)} does not match the board's digest ` +
        `(board ${gotDigest ?? "sent no digest"}, read ${expectedDigest}).`,
    );
  }
  return bytes;
}

function toHex(bytes) {
  return Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
}

/// MD5 (RFC 1321) of `bytes` as lowercase hex — the stub's read digest.
/// Here only to check a transfer, never for anything a secret depends on.
function md5Hex(bytes) {
  const shifts = [7, 12, 17, 22, 5, 9, 14, 20, 4, 11, 16, 23, 6, 10, 15, 21];
  const k = new Uint32Array(64);
  for (let i = 0; i < 64; i += 1) {
    k[i] = Math.floor(Math.abs(Math.sin(i + 1)) * 0x100000000) >>> 0;
  }
  const paddedLength = (((bytes.length + 8) >>> 6) + 1) << 6;
  const padded = new Uint8Array(paddedLength);
  padded.set(bytes);
  padded[bytes.length] = 0x80;
  const view = new DataView(padded.buffer);
  view.setUint32(paddedLength - 8, (bytes.length * 8) >>> 0, true);
  view.setUint32(paddedLength - 4, Math.floor(bytes.length / 0x20000000), true);
  let a0 = 0x67452301;
  let b0 = 0xefcdab89;
  let c0 = 0x98badcfe;
  let d0 = 0x10325476;
  const m = new Uint32Array(16);
  for (let chunk = 0; chunk < paddedLength; chunk += 64) {
    for (let i = 0; i < 16; i += 1) {
      m[i] = view.getUint32(chunk + i * 4, true);
    }
    let a = a0;
    let b = b0;
    let c = c0;
    let d = d0;
    for (let i = 0; i < 64; i += 1) {
      let f;
      let g;
      if (i < 16) {
        f = (b & c) | (~b & d);
        g = i;
      } else if (i < 32) {
        f = (d & b) | (~d & c);
        g = (5 * i + 1) % 16;
      } else if (i < 48) {
        f = b ^ c ^ d;
        g = (3 * i + 5) % 16;
      } else {
        f = c ^ (b | ~d);
        g = (7 * i) % 16;
      }
      const s = shifts[(i >>> 4) * 4 + (i % 4)];
      const sum = (a + f + k[i] + m[g]) >>> 0;
      a = d;
      d = c;
      c = b;
      b = (b + ((sum << s) | (sum >>> (32 - s)))) >>> 0;
    }
    a0 = (a0 + a) >>> 0;
    b0 = (b0 + b) >>> 0;
    c0 = (c0 + c) >>> 0;
    d0 = (d0 + d) >>> 0;
  }
  const out = new Uint8Array(16);
  const outView = new DataView(out.buffer);
  [a0, b0, c0, d0].forEach((word, i) => outView.setUint32(i * 4, word, true));
  return toHex(out);
}

/// Judge an erase by its OWN outcome, not by the flash-ID probe.
///
/// The ID probe reads 0 and prints "Failed to communicate with the flash
/// chip" on ESP32-C6 rev 2 over USB-Serial-JTAG while real stub traffic
/// works — established on the bench 2026-07-31 (see f3586b9c8, which moved
/// the boot-control write to readback verification for this reason). That
/// commit left the erase path gated on the warning because "there is
/// nothing to read back after an erase"; it turns out there IS something
/// to check — esptool announces the chip erase it actually performed.
///
/// Yona's walk 2026-08-02: erase logged the benign warning, then "Chip
/// erase completed successfully in 2.241s", and this gate failed the
/// operation anyway. So: a completion line is proof and outranks the
/// warning; without one, the warning is the best explanation we have and
/// is surfaced; with neither, `eraseFlash()` returned without throwing and
/// there is no evidence of failure to report.
function assertEraseCompleted(logs, context) {
  const completed = logs.some((line) =>
    line.includes("Chip erase completed successfully")
  );
  if (completed) {
    return;
  }
  const warning = logs.find((line) =>
    line.includes("Failed to communicate with the flash chip") ||
    line.includes("Flash ID: 0")
  );
  if (warning) {
    throw new Error(`${context} failed: ${warning}`);
  }
}

function terminalFor(logs, target, onEvent) {
  return {
    clean() {},
    writeLine(line) {
      const message = String(line ?? "");
      logs.push(message);
      emitEvent(onEvent, { kind: "log", message });
      console.info(`[${target}] ${message}`);
    },
    write(text) {
      const message = String(text ?? "").trimEnd();
      if (message.length > 0) {
        logs.push(message);
        emitEvent(onEvent, { kind: "log", message });
        console.info(`[${target}] ${message}`);
      }
    },
  };
}

function pushProgress(progress, onEvent, entry) {
  const normalized = {
    label: String(entry.label ?? ""),
    completedSteps: Number(entry.completedSteps ?? 0),
    totalSteps: entry.totalSteps == null ? null : Number(entry.totalSteps),
    percent: entry.percent == null ? null : Number(entry.percent),
  };
  const previous = progress.at(-1);
  if (
    previous &&
    previous.label === normalized.label &&
    previous.completedSteps === normalized.completedSteps &&
    previous.totalSteps === normalized.totalSteps &&
    previous.percent === normalized.percent
  ) {
    return;
  }
  progress.push(normalized);
  emitEvent(onEvent, { kind: "progress", ...normalized });
}

function emitEvent(onEvent, event) {
  if (typeof onEvent === "function") {
    onEvent(event);
  }
}

async function loadFullManifest(manifestPath) {
  const url = new URL(manifestPath, globalThis.location?.href ?? "http://localhost/");
  const response = await fetch(url, { cache: "no-store" });
  const contentType = response.headers.get("content-type") ?? "";
  const text = await response.text();
  if (!response.ok) {
    throw new Error(
      `Firmware manifest is unavailable at ${url.href} (${response.status} ${response.statusText}, content-type: ${contentType || "unknown"}): ${snippet(text)}`,
    );
  }
  if (looksLikeHtml(contentType, text)) {
    throw new Error(
      `Firmware manifest URL returned HTML instead of JSON: ${url.href} (content-type: ${contentType || "unknown"}): ${snippet(text)}`,
    );
  }
  let manifest;
  try {
    manifest = JSON.parse(text);
  } catch (error) {
    throw new Error(
      `Firmware manifest is not valid JSON at ${url.href} (content-type: ${contentType || "unknown"}): ${errorMessage(error)}; body: ${snippet(text)}`,
    );
  }
  validateManifest(manifest);
  return manifest;
}

async function loadImageFiles(manifest, manifestPath) {
  const basePath = new URL(manifestPath, globalThis.location?.href ?? "http://localhost/");
  return Promise.all(
    manifest.images.map(async (image) => {
      const url = new URL(image.path, basePath);
      const response = await fetch(url, { cache: "no-store" });
      const contentType = response.headers.get("content-type") ?? "";
      if (!response.ok) {
        throw new Error(
          `Firmware image is unavailable at ${url.href} (${response.status} ${response.statusText}, content-type: ${contentType || "unknown"}).`,
        );
      }
      if (contentType.includes("text/html")) {
        const text = await response.text();
        throw new Error(
          `Firmware image URL returned HTML instead of binary data: ${url.href}: ${snippet(text)}`,
        );
      }
      return {
        address: parseAddress(image.address),
        data: new Uint8Array(await response.arrayBuffer()),
      };
    }),
  );
}

async function loadEsptoolModule(esptoolModulePath) {
  if (!esptoolModulePath) {
    throw new Error("Missing esptool_module_path.");
  }
  try {
    return await import(esptoolModulePath);
  } catch (error) {
    throw new Error(`Failed to import esptool module ${esptoolModulePath}: ${errorMessage(error)}`);
  }
}

/// The chip's factory base MAC, read from the loader session the flash
/// preflight ALREADY opened.
///
/// esptool-js 0.6.0 puts the read on the per-target ROM class, reached as
/// `loader.chip.readMac(loader)` (`lib/targets/esp32.js`, `esp32c6.js`,
/// `esp32s3.js` — each returns `%02x`-joined colon hex). `loader.main()`
/// calls it itself to log the `MAC: …` line, so this costs two register
/// reads and NO reset: no new bootloader-entry path exists here, which is
/// the whole reason the read lives in this flow rather than in a probe of
/// its own.
///
/// Nothing is validated here — the string goes to Rust as-is, where
/// `lpa_link::normalize_base_mac` decides whether it is an address at all.
/// A failed read returns null and is NOT a failed flash: identity is
/// evidence, and a board that will not name itself still deserves its
/// firmware.
async function readBaseMac(loader) {
  try {
    if (typeof loader.chip?.readMac !== "function") {
      return null;
    }
    const mac = await loader.chip.readMac(loader);
    return mac ? String(mac) : null;
  } catch (error) {
    console.warn(`[esp32-flash] base MAC read failed: ${errorMessage(error)}`);
    return null;
  }
}

/// ESP32-C6 LP analog I2C clock registers. The twin of
/// `host_serial_esp32/lp_analog_i2c.rs` — same addresses, same sequence,
/// same log line; this file has no tests, so keep them in step by hand.
const LPPERI_CLK_EN = 0x600b2800;
const LPPERI_RESET_EN = 0x600b2804;
const LP_ANA_I2C_BIT = 0x20000000; // bit 29 in both registers above
const LP_I2C_ANA_MST_I2C0_CTRL = 0x600b2400;
const LP_I2C_ANA_MST_BUSY = 0x02000000; // bit 25

/// Undo the LP-domain state a fresh board's factory firmware leaves behind,
/// so the bootloader we just wrote can boot on the reset that follows.
///
/// ESP-IDF apps gate the LP peripheral clocks they do not use — `LPPERI_CLK_EN`
/// bit 29, the LP analog I2C clock, among them. Our merged image's
/// second-stage bootloader (ESP-IDF v5.1-beta1, bundled by espflash 3.3.0)
/// drives the analog bus through the LP aperture, and every reset a flasher
/// can send is HP-only, so on a factory-fresh board it hangs on its first
/// regi2c write (`Saved PC:0x4086ed7a`) until someone replugs the board.
/// Bench-proven fix (2026-09-06, XIAO ESP32C6): set the bit AND pulse
/// `LPPERI_RESET_EN` bit 29 — the clock alone does not clear the latched
/// busy — then the ordinary hard reset boots LightPlayer.
///
/// C6 only; silent when the clock is on and the master idle. Best-effort: a
/// failed register call is logged and the flash still ends in a reset — the
/// board may then need the replug it always needed. See
/// `docs/defects/2026-09-06-c6-first-flash-bootloader-hang-lp-analog-i2c-clock.md`.
async function restoreLpAnalogI2cClock(loader, chipName, knownChipIds, terminal) {
  if (chipIdFrom(chipName, knownChipIds) !== "esp32c6") {
    return;
  }
  try {
    const clkBefore = (await loader.readReg(LPPERI_CLK_EN)) >>> 0;
    const ctrl = (await loader.readReg(LP_I2C_ANA_MST_I2C0_CTRL)) >>> 0;
    const clockWasGated = (clkBefore & LP_ANA_I2C_BIT) === 0;
    const masterWasBusy = (ctrl & LP_I2C_ANA_MST_BUSY) !== 0;
    if (!clockWasGated && !masterWasBusy) {
      return;
    }
    await loader.writeReg(LPPERI_CLK_EN, (clkBefore | LP_ANA_I2C_BIT) >>> 0, 0xffffffff);
    const resetBefore = (await loader.readReg(LPPERI_RESET_EN)) >>> 0;
    await loader.writeReg(LPPERI_RESET_EN, (resetBefore | LP_ANA_I2C_BIT) >>> 0, 0xffffffff);
    await loader.writeReg(LPPERI_RESET_EN, (resetBefore & ~LP_ANA_I2C_BIT) >>> 0, 0xffffffff);
    const clkAfter = (await loader.readReg(LPPERI_CLK_EN)) >>> 0;
    const busyAfter = ((await loader.readReg(LP_I2C_ANA_MST_I2C0_CTRL)) & LP_I2C_ANA_MST_BUSY) !== 0;
    const what = clockWasGated
      ? "restored the LP analog I2C clock the previous firmware left gated"
      : "reset the LP analog I2C master the previous firmware left busy";
    const busy = busyAfter ? "busy still set; the board may need a replug" : "busy cleared";
    terminal.writeLine(
      `${what} (LPPERI_CLK_EN 0x${hex32(clkBefore)} -> 0x${hex32(clkAfter)}, ${busy})`,
    );
  } catch (error) {
    terminal.writeLine(`warning: LP analog I2C clock check failed: ${errorMessage(error)}`);
  }
}

function hex32(value) {
  return (value >>> 0).toString(16).padStart(8, "0");
}

/// The chip id a reported name belongs to — the JS half of
/// `lpa_link::chip_id_from_reported`, over the table Rust hands in.
///
/// Whole-string equality does NOT work here, and this is not theoretical:
/// esptool-js 0.6.0's `main()` returns `getChipDescription()`, which builds
/// `"ESP32-C6 (revision 0)"`, `"ESP32-S3 (QFN56) (revision v0.2)"`, and for
/// the classic a DIE name — `"ESP32-D0WDQ6"`, `"ESP32-U4WDH"`,
/// `"ESP32-PICO-D4"` — never the bare id a manifest carries. Comparing
/// normalized strings for equality therefore refuses EVERY real device.
/// Nor does a substring test work: all of those contain "esp32", so a
/// classic image would sail onto a C6. Prefix-matching an ordered,
/// most-specific-first table is what distinguishes them.
function chipIdFrom(reported, knownChipIds) {
  const normalized = String(reported ?? "")
    .toLowerCase()
    .replace(/[^a-z0-9]/g, "");
  if (!normalized) {
    return null;
  }
  return Array.from(knownChipIds ?? []).find((id) => normalized.startsWith(id)) ?? null;
}

/// Refuse to write `manifest` onto `reportedChip`.
///
/// An unresolvable chip on either side is NOT a match: esptool-js always
/// names the chip it synced with, so an absent or unrecognized name means
/// the handshake did not go the way this code assumes, and guessing there
/// is the same bet the guard exists to refuse.
function assertChipMatchesManifest(reportedChip, manifest, manifestPath, knownChipIds) {
  const manifestChip = manifest.core?.target?.chip;
  const detected = chipIdFrom(reportedChip, knownChipIds);
  const expected = chipIdFrom(manifestChip, knownChipIds);
  if (detected && detected === expected) {
    return;
  }
  const detectedLabel = reportedChip ? String(reportedChip) : "an unidentified chip";
  throw new Error(
    `Refusing to flash: this device is ${detectedLabel}, but the firmware image ` +
      `${manifest.firmwareId} (${manifestPath}) is built for ${manifestChip}. ` +
      `Pick the board you actually have in the setup form, or install the generic ` +
      `image for this chip.`,
  );
}

function summarizeManifest(manifest, manifestPath) {
  return {
    firmwareId: String(manifest.firmwareId),
    displayName: String(manifest.displayName ?? manifest.firmwareId),
    targetChip: String(manifest.core?.target?.chip ?? "esp32c6"),
    imageCount: manifest.images.length,
    totalBytes: manifest.images.reduce((total, image) => total + Number(image.sizeBytes ?? 0), 0),
    manifestPath,
  };
}

function compactProgress(progress) {
  const compacted = [];
  let previousKey = null;
  for (const entry of progress) {
    const key = `${entry.label}:${entry.percent}`;
    if (key === previousKey) {
      continue;
    }
    previousKey = key;
    compacted.push(entry);
  }
  return compacted;
}

function validateManifest(manifest) {
  if (!manifest || typeof manifest !== "object") {
    throw new Error("Firmware manifest is not a JSON object.");
  }
  if (manifest.schemaVersion !== FIRMWARE_MANIFEST_SCHEMA_VERSION) {
    throw new Error(
      `Firmware manifest has schemaVersion ${manifest.schemaVersion} — this build understands only ${FIRMWARE_MANIFEST_SCHEMA_VERSION}; repackage with \`lp-cli firmware package\`.`,
    );
  }
  if (typeof manifest.firmwareId !== "string") {
    throw new Error("Firmware manifest is missing firmwareId.");
  }
  if (typeof manifest.core?.target?.chip !== "string") {
    throw new Error("Firmware manifest is missing the extracted core's target chip.");
  }
  if (!Array.isArray(manifest.images) || manifest.images.length === 0) {
    throw new Error("Firmware manifest does not list any flash images.");
  }
  for (const image of manifest.images) {
    if (typeof image.path !== "string" || typeof image.address !== "string") {
      throw new Error("Firmware manifest image entries must include path and address.");
    }
  }
}

function parseAddress(address) {
  const value = Number(address);
  if (!Number.isInteger(value)) {
    throw new Error(`Firmware image address is invalid: ${address}`);
  }
  return value;
}

function reportFailure(target, error, onEvent = null) {
  const message = `${errorMessage(error)}${error?.stack ? `\n${error.stack}` : ""}`;
  emitEvent(onEvent, { kind: "log", message });
  // String-only, deliberately: the console forwarder ships arguments to the
  // Rust log bridge, which expects strings — a raw Error object arrives as
  // `{}` and the whole entry dies with "invalid type: map, expected a
  // string" (bench, 2026-07-31). The message already carries the stack.
  console.error(`[${target}] ${message}`);
}

function errorMessage(error) {
  return error instanceof Error ? error.message : String(error ?? "unknown error");
}

function looksLikeHtml(contentType, text) {
  return contentType.includes("text/html") || text.trimStart().startsWith("<!DOCTYPE") || text.trimStart().startsWith("<html");
}

function snippet(text, limit = 240) {
  return String(text ?? "")
    .replace(/\s+/g, " ")
    .trim()
    .slice(0, limit);
}
