// A serial port over WebUSB, for the one browser where Web Serial cannot see
// a wired board: Chrome on Android.
//
// Chrome's Android Web Serial reaches Bluetooth RFCOMM ports only; its intent
// to ship left wired USB until Android supports it at system level, so a C6 on
// the phone's USB-C port gets "No compatible devices found". `navigator.usb`
// reaches the same board: the C6's USB-Serial-JTAG is a CDC-ACM function (a
// communications interface, class 2, beside a data interface, class 10, with
// one bulk IN and one bulk OUT endpoint), and CDC-ACM is small enough to speak
// from a page. See `docs/adr/2026-10-18-android-usb-over-webusb.md`.
//
// `UsbCdcSerialPort` presents the part of `SerialPort` that the device
// controller (`browser_esp32_device_controller.js`) and esptool-js's
// `Transport` use: `open`/`close`, `readable`/`writable` byte streams,
// `setSignals` for DTR/RTS, `getInfo` and `forget`. `UsbCdcSerial` presents
// the part of `navigator.serial` that `browser_serial.js` uses: `requestPort`,
// `getPorts` and the `connect`/`disconnect` events. Nothing above them knows
// which one it holds, the way nothing above `virtual_serial.js` does.
//
// Written from the USB CDC 1.2 and PSTN 1.2 class specs (SET_LINE_CODING,
// SET_CONTROL_LINE_STATE) and the WebUSB spec. Google's Apache-2.0
// web-serial-polyfill was read as a behavioural reference only; nothing here
// is copied from it.
//
// Only CDC-ACM devices can be reached this way. A CH340, CP210x or FTDI
// bridge speaks its own vendor protocol, so a classic ESP32 behind one still
// needs a computer; the chooser lists CDC devices only (`classCode: 2`), and
// opening anything else says so in plain words.

// CDC class codes (USB CDC 1.2, tables 3 and 6).
const CDC_COMMUNICATIONS_CLASS = 0x02;
const CDC_DATA_CLASS = 0x0a;
// PSTN 1.2, table 13: class-specific requests to the communications interface.
const SET_LINE_CODING = 0x20;
const SET_CONTROL_LINE_STATE = 0x22;
// SET_CONTROL_LINE_STATE's wValue bits (PSTN 1.2, table 18).
const LINE_DTR = 0x01;
const LINE_RTS = 0x02;

// IN transfers kept in flight, each one packet long — what a desktop CDC
// driver does (Linux's cdc-acm queues 16), so the endpoint is asked for its
// next packet without waiting on the page. WebUSB completes transfers on one
// endpoint in the order they were submitted, so the bytes stay in order. One
// packet each, so a full packet completes its transfer instead of waiting
// for a short one. It does NOT make esptool's flasher safe on a phone: the
// desk's Pixel still lost about 40 % of a continuous 4 KB burst with 1 or 16
// in flight alike (2026-10-18), which is why the flash is refused over this
// port (`browser_esp32_flash.js`'s `isSupported`).
const IN_TRANSFERS_IN_FLIGHT = 16;
// What the port holds for a reader that is not reading, like a serial
// driver's buffer. Beyond it the oldest bytes go: the link resends what it
// lost, and a port nobody reads must not grow without bound.
const HELD_BYTES_LIMIT = 4 * 1024 * 1024;

export const NOT_CDC_MESSAGE =
  "This board's USB chip isn't a USB serial (CDC) device, so Chrome on Android can't reach it. " +
  "Connect it to a computer instead.";

export class UsbCdcSerialPort {
  constructor(device) {
    this.device = device;
    this.opened = false;
    this.lost = false;
    this.control = null; // { interfaceNumber }
    this.data = null; // { interfaceNumber, inEndpoint, outEndpoint }
    this.lines = { dataTerminalReady: false, requestToSend: false };
    this.held = [];
    this.heldBytes = 0;
    this.sink = null; // the live readable's controller
    this.readableStream = null;
    this.writableStream = null;
    this.pump = null;
  }

  getInfo() {
    return { usbVendorId: this.device.vendorId, usbProductId: this.device.productId };
  }

  get readable() {
    if (!this.opened) return null;
    this.readableStream ??= this.makeReadable();
    return this.readableStream;
  }

  get writable() {
    if (!this.opened) return null;
    this.writableStream ??= this.makeWritable();
    return this.writableStream;
  }

  async open({ baudRate, dataBits = 8, stopBits = 1, parity = "none" } = {}) {
    if (this.opened) {
      throw domError("InvalidStateError", "The port is already open.");
    }
    if (!Number.isInteger(baudRate) || baudRate <= 0) {
      throw new TypeError("open() needs a positive integer baudRate.");
    }
    const device = this.device;
    try {
      await device.open();
      if (device.configuration === null) {
        await device.selectConfiguration(1);
      }
      const found = cdcInterfaces(device);
      if (!found) {
        throw domError("NotSupportedError", NOT_CDC_MESSAGE);
      }
      this.control = found.control;
      this.data = found.data;
      if (this.control) {
        await device.claimInterface(this.control.interfaceNumber);
      }
      if (this.data.interfaceNumber !== this.control?.interfaceNumber) {
        await device.claimInterface(this.data.interfaceNumber);
      }
      if (this.control) {
        await this.classRequest(SET_LINE_CODING, 0, lineCoding({ baudRate, dataBits, stopBits, parity }));
      }
      // Asserted on open, as a tty's open asserts them (HUPCL): what a C6
      // sees when a Mac opens it, and not a reset — that is RTS asserted
      // while DTR is not, which the controller's reset dances do on purpose.
      this.lines = { dataTerminalReady: true, requestToSend: true };
      await this.writeLines();
    } catch (error) {
      await this.closeDevice();
      throw openError(error);
    }
    this.opened = true;
    this.lost = false;
    this.pump = this.runPump();
  }

  async setSignals(signals = {}) {
    if (!this.opened) {
      throw domError("InvalidStateError", "The port is not open.");
    }
    // One request sets both lines, so a call naming one keeps the other.
    if (signals.dataTerminalReady !== undefined) {
      this.lines.dataTerminalReady = Boolean(signals.dataTerminalReady);
    }
    if (signals.requestToSend !== undefined) {
      this.lines.requestToSend = Boolean(signals.requestToSend);
    }
    await this.writeLines();
  }

  async close() {
    if (!this.opened) {
      return;
    }
    // Refused while a holder has a stream locked, as Web Serial refuses it:
    // the device controller and esptool-js share one port in turn, and a
    // close from the one that does not hold it must not end the other's reads.
    if (this.readableStream?.locked || this.writableStream?.locked) {
      throw new TypeError("Cannot close a port while its streams are locked.");
    }
    this.opened = false;
    if (this.control && !this.lost) {
      // Dropped on close, as a tty's close drops them.
      this.lines = { dataTerminalReady: false, requestToSend: false };
      await this.writeLines().catch(() => {});
    }
    this.endStreams(null);
    await this.closeDevice();
    await this.pump?.catch(() => {});
    this.pump = null;
    this.held = [];
    this.heldBytes = 0;
  }

  async forget() {
    await this.close();
    if (typeof this.device.forget === "function") {
      await this.device.forget();
    }
  }

  // --- below the SerialPort surface ---------------------------------------

  // The port reads for as long as it is open, whoever is reading: a stream a
  // reader cancelled must not take an IN transfer's bytes with it, so the
  // transfers belong to the port and the streams only carry what they read.
  async runPump() {
    const { inEndpoint, inPacketSize } = this.data;
    const inFlight = [];
    const submit = () => {
      const transfer = this.device.transferIn(inEndpoint, inPacketSize);
      // Awaited below in order; the ones a close cancels are never awaited.
      transfer.catch(() => {});
      inFlight.push(transfer);
    };
    for (let i = 0; i < IN_TRANSFERS_IN_FLIGHT; i++) submit();
    while (this.opened) {
      let result;
      try {
        result = await inFlight.shift();
      } catch (error) {
        if (this.opened) {
          this.deviceLost(error);
        }
        return;
      }
      if (!this.opened) {
        return;
      }
      if (result.status === "stall") {
        // A halt fails every transfer queued behind it: clear it, and start
        // the queue again.
        await Promise.allSettled(inFlight.splice(0));
        try {
          await this.device.clearHalt("in", inEndpoint);
        } catch (error) {
          this.deviceLost(error);
          return;
        }
        for (let i = 0; i < IN_TRANSFERS_IN_FLIGHT; i++) submit();
        continue;
      }
      submit();
      const view = result.data;
      if (!view || view.byteLength === 0) {
        continue;
      }
      this.deliver(new Uint8Array(view.buffer, view.byteOffset, view.byteLength).slice());
    }
  }

  deliver(bytes) {
    if (this.sink) {
      this.sink.enqueue(bytes);
      return;
    }
    this.held.push(bytes);
    this.heldBytes += bytes.length;
    while (this.heldBytes > HELD_BYTES_LIMIT && this.held.length > 1) {
      this.heldBytes -= this.held.shift().length;
    }
  }

  makeReadable() {
    return new ReadableStream(
      {
        start: (controller) => {
          this.sink = controller;
          for (const bytes of this.held.splice(0, this.held.length)) {
            controller.enqueue(bytes);
          }
          this.heldBytes = 0;
        },
        cancel: () => {
          this.sink = null;
          this.readableStream = null;
        },
      },
      { highWaterMark: 0 },
    );
  }

  makeWritable() {
    return new WritableStream({
      write: async (chunk) => {
        const bytes = chunk instanceof Uint8Array ? chunk : new Uint8Array(chunk);
        const { outEndpoint } = this.data;
        let result = await this.device.transferOut(outEndpoint, bytes);
        if (result.status === "stall") {
          await this.device.clearHalt("out", outEndpoint);
          result = await this.device.transferOut(outEndpoint, bytes);
        }
        if (result.status !== "ok") {
          throw domError("NetworkError", `USB write ${result.status}`);
        }
      },
      close: () => {
        this.writableStream = null;
      },
      abort: () => {
        this.writableStream = null;
      },
    });
  }

  // The board went away under an open port (unplugged, or the phone took
  // the device back): the streams fail the way Web Serial's do, with a
  // NetworkError, and the port is closed.
  deviceLost(error) {
    this.lost = true;
    this.opened = false;
    this.endStreams(domError("NetworkError", `The device has been lost. (${errorText(error)})`));
    this.closeDevice();
  }

  endStreams(error) {
    const sink = this.sink;
    this.sink = null;
    this.readableStream = null;
    this.writableStream = null;
    if (!sink) return;
    try {
      if (error) {
        sink.error(error);
      } else {
        sink.close();
      }
    } catch {}
  }

  async writeLines() {
    if (!this.control) return;
    const value =
      (this.lines.dataTerminalReady ? LINE_DTR : 0) | (this.lines.requestToSend ? LINE_RTS : 0);
    await this.classRequest(SET_CONTROL_LINE_STATE, value);
  }

  async classRequest(request, value, data) {
    const setup = {
      requestType: "class",
      recipient: "interface",
      request,
      value,
      index: this.control.interfaceNumber,
    };
    const result = await this.device.controlTransferOut(setup, data);
    if (result.status !== "ok") {
      throw domError("NetworkError", `USB control request 0x${request.toString(16)} ${result.status}`);
    }
  }

  async closeDevice() {
    const device = this.device;
    if (!device.opened) return;
    try {
      await device.close();
    } catch {}
  }
}

// The `navigator.serial` the page uses when Web Serial cannot reach USB.
// Ports are one object per USBDevice, so `getPorts()` answers with the same
// objects `requestPort()` gave out — `browser_serial.js` keys its sessions on
// port identity.
export class UsbCdcSerial {
  constructor(usb = globalThis.navigator?.usb) {
    this.usb = usb;
    this.ports = new WeakMap();
  }

  portFor(device) {
    let port = this.ports.get(device);
    if (!port) {
      port = new UsbCdcSerialPort(device);
      this.ports.set(device, port);
    }
    return port;
  }

  // Web Serial's `{ usbVendorId, usbProductId }` filters, as WebUSB filters
  // that also ask for a CDC interface: the chooser lists only what this
  // port can open. A device's interfaces match `classCode` as well as the
  // device itself (WebUSB, "matches a device filter").
  async requestPort({ filters = [] } = {}) {
    const usbFilters = filters.length
      ? filters.map((filter) => {
          const out = { classCode: CDC_COMMUNICATIONS_CLASS };
          if (filter.usbVendorId !== undefined) out.vendorId = filter.usbVendorId;
          if (filter.usbProductId !== undefined) out.productId = filter.usbProductId;
          return out;
        })
      : [{ classCode: CDC_COMMUNICATIONS_CLASS }];
    const device = await this.usb.requestDevice({ filters: usbFilters });
    return this.portFor(device);
  }

  async getPorts() {
    const devices = await this.usb.getDevices();
    return devices.filter(hasCdcInterface).map((device) => this.portFor(device));
  }

  addEventListener(type, listener) {
    if (type !== "connect" && type !== "disconnect") return;
    this.usb.addEventListener(type, (event) => {
      if (event?.device && !hasCdcInterface(event.device)) return;
      listener(event);
    });
  }
}

// The communications and data interfaces of a CDC-ACM function, from the
// device's active configuration. A device with a data interface and no
// communications interface still reads and writes; it just has no lines.
export function cdcInterfaces(device) {
  const interfaces = device.configuration?.interfaces ?? [];
  let control = null;
  let data = null;
  for (const iface of interfaces) {
    for (const alternate of iface.alternates ?? []) {
      if (!control && alternate.interfaceClass === CDC_COMMUNICATIONS_CLASS) {
        control = { interfaceNumber: iface.interfaceNumber };
      }
      if (!data && alternate.interfaceClass === CDC_DATA_CLASS) {
        const inEndpoint = bulkEndpoint(alternate, "in");
        const outEndpoint = bulkEndpoint(alternate, "out");
        if (inEndpoint && outEndpoint) {
          data = {
            interfaceNumber: iface.interfaceNumber,
            inEndpoint: inEndpoint.endpointNumber,
            inPacketSize: inEndpoint.packetSize || 64,
            outEndpoint: outEndpoint.endpointNumber,
          };
        }
      }
    }
  }
  return data ? { control, data } : null;
}

function hasCdcInterface(device) {
  // A device not yet opened may report no configuration; let it through and
  // let `open()` say what it is.
  if (!device.configuration) return true;
  return cdcInterfaces(device) !== null;
}

function bulkEndpoint(alternate, direction) {
  return (
    (alternate.endpoints ?? []).find(
      (candidate) => candidate.type === "bulk" && candidate.direction === direction,
    ) ?? null
  );
}

// PSTN 1.2, table 17: dwDTERate, bCharFormat, bParityType, bDataBits.
export function lineCoding({ baudRate, dataBits = 8, stopBits = 1, parity = "none" }) {
  const bytes = new Uint8Array(7);
  new DataView(bytes.buffer).setUint32(0, baudRate, true);
  bytes[4] = stopBits === 2 ? 2 : 0;
  bytes[5] = { none: 0, odd: 1, even: 2 }[parity] ?? 0;
  bytes[6] = dataBits;
  return bytes;
}

// An open that failed, named the way the controller and Studio read a
// Web Serial one: the DOMException name leads, and a NetworkError means
// "held elsewhere" (another app, or Android's own USB service, has the
// interface). Android's own "Allow Chrome to access" dialog, refused, is a
// SecurityError.
function openError(error) {
  if (error?.name === "NotSupportedError") return error;
  if (error?.name === "SecurityError") {
    return domError("SecurityError", "Android didn't let Chrome open this board. Plug it in again and allow access.");
  }
  if (error?.name === "NetworkError" || /claim/i.test(errorText(error))) {
    return domError("NetworkError", `The board is in use by another app. (${errorText(error)})`);
  }
  return error;
}

function domError(name, message) {
  if (typeof DOMException === "function") {
    return new DOMException(message, name);
  }
  const error = new Error(message);
  error.name = name;
  return error;
}

function errorText(error) {
  return error instanceof Error || error?.message ? error.message : String(error);
}
