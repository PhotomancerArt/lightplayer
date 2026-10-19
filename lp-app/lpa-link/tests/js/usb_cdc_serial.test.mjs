// The serial port over WebUSB (`usb_cdc_serial.js`), the port Studio's USB
// square uses in Chrome on Android, where Web Serial lists no wired board.
//
// `node --test lp-app/lpa-link/tests/js/` — `just lpa-link-js-test`. No
// browser, no board: a `USBDevice` double shaped like the C6's
// USB-Serial-JTAG (`fake_usb_device.js`), under the shipped port, under the
// shipped device controller where a test says so — the controller is what
// opens, resets and reads a board in Studio, and it must not be able to tell
// this port from a Web Serial one.
import assert from "node:assert/strict";
import { test } from "node:test";

import {
  NOT_CDC_MESSAGE,
  UsbCdcSerial,
  UsbCdcSerialPort,
} from "../../../lpa-studio-web/public/lpa-link/usb_cdc_serial.js";
import { BrowserEsp32DeviceController } from "../../../lpa-studio-web/public/lpa-link/browser_esp32_device_controller.js";
import { serialBackendKind } from "../../src/providers/browser_serial_esp32/browser_serial.js";
import { CH340_INTERFACES, FakeUsb, FakeUsbDevice } from "./fake_usb_device.js";

test("open claims both CDC interfaces, sets 115200 8N1, and asserts DTR and RTS together", async () => {
  const device = new FakeUsbDevice();
  const port = new UsbCdcSerialPort(device);
  await port.open({ baudRate: 115200 });

  assert.deepEqual([...device.claimed].sort(), [0, 1]);
  const [coding, lines] = device.controlLog;
  assert.equal(coding.request, 0x20);
  assert.equal(coding.requestType, "class");
  assert.equal(coding.recipient, "interface");
  assert.equal(coding.index, 0);
  assert.deepEqual(coding.data, [0x00, 0xc2, 0x01, 0x00, 0, 0, 8]); // 115200 LE, 1 stop, no parity, 8 bits
  assert.equal(lines.request, 0x22);
  assert.equal(lines.value, 0x03);
  // Both at once: RTS asserted while DTR is not would reset a C6.
  assert.deepEqual(device.lineLog, [{ dtr: true, rts: true }]);
  assert.ok(port.readable && port.writable);
  await port.close();
});

test("a closed port has no streams, and close drops both lines", async () => {
  const device = new FakeUsbDevice();
  const port = new UsbCdcSerialPort(device);
  assert.equal(port.readable, null);
  assert.equal(port.writable, null);
  await port.open({ baudRate: 115200 });
  await port.close();
  assert.equal(port.readable, null);
  assert.equal(port.writable, null);
  assert.equal(device.opened, false);
  assert.deepEqual(device.lines, { dtr: false, rts: false });
});

test("the board's bytes come out of readable and the page's go to the OUT endpoint", async () => {
  const device = new FakeUsbDevice();
  const port = new UsbCdcSerialPort(device);
  await port.open({ baudRate: 115200 });
  const reader = port.readable.getReader();
  device.deliver([1, 2, 3]);
  device.deliver([4]);
  assert.deepEqual(Array.from((await reader.read()).value), [1, 2, 3]);
  assert.deepEqual(Array.from((await reader.read()).value), [4]);

  const writer = port.writable.getWriter();
  await writer.write(new Uint8Array([9, 8, 7]));
  assert.equal(device.written[0].endpointNumber, 1);
  assert.deepEqual(Array.from(device.writtenBytes()), [9, 8, 7]);
  reader.releaseLock();
  writer.releaseLock();
  await reader.cancel?.().catch(() => {});
  await port.close();
});

test("setSignals naming one line keeps the other, as the reset dances need", async () => {
  const device = new FakeUsbDevice();
  const port = new UsbCdcSerialPort(device);
  await port.open({ baudRate: 115200 });
  await port.setSignals({ dataTerminalReady: false });
  await port.setSignals({ requestToSend: false });
  await port.setSignals({ dataTerminalReady: true });
  assert.deepEqual(device.lineLog.slice(1), [
    { dtr: false, rts: true },
    { dtr: false, rts: false },
    { dtr: true, rts: false },
  ]);
  await port.close();
});

test("a reader that cancels does not take the next bytes with it", async () => {
  const device = new FakeUsbDevice();
  const port = new UsbCdcSerialPort(device);
  await port.open({ baudRate: 115200 });
  const first = port.readable.getReader();
  const pending = first.read();
  await first.cancel();
  assert.equal((await pending).done, true);
  first.releaseLock();
  // The IN transfer the cancelled read was waiting on is the port's, and so
  // are the bytes it brings.
  device.deliver([0x42, 0x43]);
  await settle();
  const second = port.readable.getReader();
  assert.deepEqual(Array.from((await second.read()).value), [0x42, 0x43]);
  second.releaseLock();
  await port.close();
});

test("an unplugged board fails the read with a NetworkError and closes the port", async () => {
  const device = new FakeUsbDevice();
  const port = new UsbCdcSerialPort(device);
  await port.open({ baudRate: 115200 });
  const reader = port.readable.getReader();
  const read = reader.read();
  device.unplug();
  await assert.rejects(read, (error) => error.name === "NetworkError");
  assert.equal(port.readable, null);
  assert.equal(port.writable, null);
  // and it opens again once it is back
  device.unplugged = false;
  await port.open({ baudRate: 115200 });
  assert.ok(port.readable);
  await port.close();
});

test("a CH340 is refused in plain words, and the device is let go", async () => {
  const device = new FakeUsbDevice({ vendorId: 0x1a86, productId: 0x7522, interfaces: CH340_INTERFACES });
  const port = new UsbCdcSerialPort(device);
  await assert.rejects(port.open({ baudRate: 115200 }), (error) => {
    assert.equal(error.name, "NotSupportedError");
    assert.equal(error.message, NOT_CDC_MESSAGE);
    return true;
  });
  assert.equal(device.opened, false);
  assert.equal(port.readable, null);
});

test("an interface another app holds is a NetworkError, which Studio reads as in use", async () => {
  const device = new FakeUsbDevice();
  device.refuseClaim = true;
  const port = new UsbCdcSerialPort(device);
  await assert.rejects(port.open({ baudRate: 115200 }), (error) => error.name === "NetworkError");
  assert.equal(device.opened, false);
});

test("getInfo reports the USB ids the link picks its preset from", () => {
  const port = new UsbCdcSerialPort(new FakeUsbDevice());
  assert.deepEqual(port.getInfo(), { usbVendorId: 0x303a, usbProductId: 0x1001 });
});

test("the chooser asks for CDC devices of the vendors Web Serial's filter named", async () => {
  const c6 = new FakeUsbDevice();
  const usb = new FakeUsb([new FakeUsbDevice({ vendorId: 0x1a86, productId: 0x7522, interfaces: CH340_INTERFACES }), c6]);
  const serial = new UsbCdcSerial(usb);
  const port = await serial.requestPort({ filters: [{ usbVendorId: 0x303a }, { usbVendorId: 0x1a86 }] });
  assert.deepEqual(usb.lastFilters, [
    { classCode: 2, vendorId: 0x303a },
    { classCode: 2, vendorId: 0x1a86 },
  ]);
  assert.equal(port.device, c6);
  // getPorts answers with the same object: sessions are keyed on it.
  await c6.selectConfiguration(1);
  assert.deepEqual(await serial.getPorts(), [port]);
});

test("forget revokes the device's grant", async () => {
  const device = new FakeUsbDevice();
  const port = new UsbCdcSerialPort(device);
  await port.open({ baudRate: 115200 });
  await port.forget();
  assert.equal(device.forgotten, true);
  assert.equal(device.opened, false);
});

test("connect events come from navigator.usb", async () => {
  const usb = new FakeUsb([]);
  const serial = new UsbCdcSerial(usb);
  let connects = 0;
  serial.addEventListener("connect", () => connects++);
  usb.plugIn(new FakeUsbDevice());
  assert.equal(connects, 1);
});

test("the backend: WebUSB on Android, Web Serial elsewhere, and a page polyfill always wins", () => {
  // Chromium's `serial` and `usb` are getters on Navigator.prototype, never
  // own properties, so every navigator here keeps them on its prototype.
  const nav = (members) => Object.create(members);
  const usb = {};
  const serial = {};
  const android = { platform: "Android" };
  assert.equal(serialBackendKind(nav({ userAgentData: android, usb, serial })), "usb-cdc");
  assert.equal(
    serialBackendKind(nav({ userAgent: "Mozilla/5.0 (Linux; Android 14; Pixel 8a)", usb, serial })),
    "usb-cdc",
  );
  assert.equal(serialBackendKind(nav({ userAgentData: { platform: "macOS" }, usb, serial })), "serial");
  assert.equal(serialBackendKind(nav({ userAgent: "Mozilla/5.0 (Macintosh)", usb })), "usb-cdc");
  assert.equal(serialBackendKind(nav({ userAgent: "Mozilla/5.0 (Macintosh)" })), null);
  assert.equal(serialBackendKind(undefined), null);
  // `?emu=` installs `navigator.serial` as an own property of navigator.
  const polyfilled = nav({ usb, userAgentData: android });
  Object.defineProperty(polyfilled, "serial", { value: serial, enumerable: true });
  assert.equal(serialBackendKind(polyfilled), "serial");
});

// --- the device controller over this port ---------------------------------

test("the controller opens, resets and reads a board over WebUSB as over Web Serial", async () => {
  const device = new FakeUsbDevice();
  const controller = new BrowserEsp32DeviceController({ port: new UsbCdcSerialPort(device) });
  assert.equal(controller.label, "ESP32 Serial (303a:1001)");
  const opened = await controller.openProtocol({ baudRate: 115200, reset: true, resetKind: "normal" });
  assert.equal(opened.resetOutcome.ok, true);
  // "normal" is D0 W100 R1 W100 R0, from the lines the open asserted.
  assert.deepEqual(device.lineLog, [
    { dtr: true, rts: true },
    { dtr: false, rts: true },
    { dtr: false, rts: true },
    { dtr: false, rts: false },
  ]);

  device.deliver(new TextEncoder().encode("ESP-ROM:esp32c6\n"));
  await settle();
  const taken = controller.takeBytes();
  assert.equal(taken.open, true);
  assert.equal(new TextDecoder().decode(taken.bytes), "ESP-ROM:esp32c6\n");

  await controller.writeBytes(new Uint8Array([0xc0, 0x01]));
  assert.deepEqual(Array.from(device.writtenBytes()), [0xc0, 0x01]);

  await controller.close();
  assert.deepEqual(controller.takeErrors(), []);
  assert.equal(device.opened, false);
});

test("the controller sees an unplug as Web Serial's lost device, and reopens", async () => {
  const device = new FakeUsbDevice();
  const controller = new BrowserEsp32DeviceController({ port: new UsbCdcSerialPort(device) });
  await controller.openProtocol({ baudRate: 115200, reset: false });
  device.unplug();
  await settle();
  const errors = controller.takeErrors();
  assert.ok(errors.some((error) => /disconnected/i.test(error)), errors.join(" | "));

  device.unplugged = false;
  await controller.openProtocol({ baudRate: 115200, reset: false });
  device.deliver([7]);
  await settle();
  assert.deepEqual(Array.from(controller.takeBytes().bytes), [7]);
  await controller.close();
});

test("the controller's CH34x reset reaches the lines as whole-status writes", async () => {
  const device = new FakeUsbDevice();
  const controller = new BrowserEsp32DeviceController({ port: new UsbCdcSerialPort(device) });
  await controller.openProtocol({ baudRate: 115200, reset: true, resetKind: "both-then-drop" });
  assert.deepEqual(device.lineLog.slice(1), [
    { dtr: false, rts: false },
    { dtr: true, rts: true },
    { dtr: false, rts: true },
    { dtr: false, rts: false },
  ]);
  await controller.close();
});

test("close is refused while a stream is held, and the holder's reads go on", async () => {
  // Web Serial's rule: the controller and esptool-js share one port in turn,
  // and a close from the one that does not hold it must not end the other's.
  const device = new FakeUsbDevice();
  const port = new UsbCdcSerialPort(device);
  await port.open({ baudRate: 115200 });
  const flasher = port.readable.getReader();
  await assert.rejects(port.close(), TypeError);
  device.deliver(new TextEncoder().encode("OHAI"));
  assert.equal(new TextDecoder().decode((await flasher.read()).value), "OHAI");
  await flasher.cancel();
  flasher.releaseLock();
  await port.close();
  assert.equal(device.opened, false);
});

test("sixteen packet-long IN transfers stay in flight, so the endpoint is never left unread", async () => {
  // esptool's flasher stub on a C6 flushes a frame only if the host read the
  // previous packet within a short spin; with one transfer at a time its
  // "OHAI" never left the chip (the first flash from the phone).
  const device = new FakeUsbDevice();
  const port = new UsbCdcSerialPort(device);
  await port.open({ baudRate: 115200 });
  await settle();
  assert.equal(device.pendingIn.length, 16);
  const reader = port.readable.getReader();
  device.deliver([0xc0]);
  device.deliver([0x4f, 0x48, 0x41, 0x49, 0xc0]);
  assert.deepEqual(Array.from((await reader.read()).value), [0xc0]);
  assert.deepEqual(Array.from((await reader.read()).value), [0x4f, 0x48, 0x41, 0x49, 0xc0]);
  await settle();
  assert.equal(device.pendingIn.length, 16, "each completed transfer is replaced");
  // A packet each: 100 bytes come as a full packet and the rest.
  device.deliver(new Uint8Array(100).fill(7));
  assert.equal((await reader.read()).value.length, 64);
  assert.equal((await reader.read()).value.length, 36);
  reader.releaseLock();
  await port.close();
});

test("a session reopening while esptool holds the port fails and holds nothing", async () => {
  // A flash from the phone (2026-10-18) hung 1.4 MB into reading the board:
  // Studio's link reopened its session mid-flash, the reopen took the free
  // writer before failing on the held reader, and kept it — the link's own
  // frames then went into the flasher's stub until one carried a 0xC0.
  const device = new FakeUsbDevice();
  const port = new UsbCdcSerialPort(device);
  await port.open({ baudRate: 115200 });
  const flasher = port.readable.getReader();

  const session = new BrowserEsp32DeviceController({ port });
  await assert.rejects(session.openProtocol({ baudRate: 115200, reset: false }));
  assert.equal(port.writable.locked, false, "the failed reopen let the writer go");
  await assert.rejects(session.writeBytes(new Uint8Array([0x00, 0x02])), /not open/);

  const writer = port.writable.getWriter();
  await writer.write(new Uint8Array([0xc0, 0x00, 0xc0]));
  writer.releaseLock();
  assert.deepEqual(Array.from(device.writtenBytes()), [0xc0, 0x00, 0xc0], "only the flasher wrote");
  device.deliver([0x42]);
  assert.deepEqual(Array.from((await flasher.read()).value), [0x42]);
  await flasher.cancel();
  flasher.releaseLock();
  await port.close();
});

function settle() {
  return new Promise((resolve) => setTimeout(resolve, 0));
}
