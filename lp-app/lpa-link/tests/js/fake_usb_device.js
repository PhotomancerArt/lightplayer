// A `USBDevice` double for the WebUSB serial port (`usb_cdc_serial.js`): the
// members that port uses and nothing else, shaped like the ESP32-C6's
// USB-Serial-JTAG as Android's `dumpsys usb` listed it on the desk's Pixel
// (2026-10-18): interface 0 class 2 (CDC communications, an interrupt IN),
// interface 1 class 10 (CDC data, bulk IN 1 and bulk OUT 1), interface 2
// class 255 (the JTAG half).
//
// The board's side is a script: `deliver(bytes)` is the board sending,
// `written` is what the host sent, `controlLog` every class request, and
// `lines` the DTR/RTS the board would see, after each request.

export const C6_INTERFACES = [
  iface(0, 0x02, [{ endpointNumber: 2, direction: "in", type: "interrupt", packetSize: 64 }]),
  iface(1, 0x0a, [
    { endpointNumber: 1, direction: "in", type: "bulk", packetSize: 64 },
    { endpointNumber: 1, direction: "out", type: "bulk", packetSize: 64 },
  ]),
  iface(2, 0xff, [
    { endpointNumber: 2, direction: "out", type: "bulk", packetSize: 64 },
    { endpointNumber: 3, direction: "in", type: "bulk", packetSize: 64 },
  ]),
];

// A CH340: one vendor-class interface, which no CDC driver can speak.
export const CH340_INTERFACES = [
  iface(0, 0xff, [
    { endpointNumber: 2, direction: "in", type: "bulk", packetSize: 32 },
    { endpointNumber: 2, direction: "out", type: "bulk", packetSize: 32 },
    { endpointNumber: 1, direction: "in", type: "interrupt", packetSize: 8 },
  ]),
];

export class FakeUsbDevice {
  constructor({ vendorId = 0x303a, productId = 0x1001, interfaces = C6_INTERFACES, serialNumber = "10:BD:A3:B0:BD:A8" } = {}) {
    this.vendorId = vendorId;
    this.productId = productId;
    this.serialNumber = serialNumber;
    this.interfaces = interfaces;
    this.opened = false;
    this.configuration = null;
    this.claimed = new Set();
    this.controlLog = [];
    this.lineLog = [];
    this.written = [];
    this.inbox = [];
    this.pendingIn = [];
    this.forgotten = false;
    this.unplugged = false;
    this.refuseClaim = false;
  }

  get lines() {
    return this.lineLog.at(-1) ?? null;
  }

  async open() {
    if (this.unplugged) throw dom("NotFoundError", "The device was disconnected.");
    this.opened = true;
  }

  async selectConfiguration(value) {
    this.configuration = { configurationValue: value, interfaces: this.interfaces };
  }

  async claimInterface(number) {
    if (this.refuseClaim) throw dom("NetworkError", "Unable to claim interface.");
    this.claimed.add(number);
  }

  async close() {
    this.opened = false;
    this.claimed.clear();
    for (const pending of this.pendingIn.splice(0)) {
      pending.reject(dom("AbortError", "The transfer was cancelled."));
    }
  }

  async forget() {
    this.forgotten = true;
  }

  async controlTransferOut(setup, data) {
    this.requireOpen();
    this.controlLog.push({ ...setup, data: data ? Array.from(data) : null });
    if (setup.request === 0x22) {
      this.lineLog.push({ dtr: Boolean(setup.value & 1), rts: Boolean(setup.value & 2) });
    }
    return { status: "ok" };
  }

  async transferOut(endpointNumber, data) {
    this.requireOpen();
    this.written.push({ endpointNumber, bytes: Uint8Array.from(data) });
    return { status: "ok", bytesWritten: data.byteLength };
  }

  transferIn(endpointNumber, length) {
    try {
      this.requireOpen();
    } catch (error) {
      return Promise.reject(error);
    }
    if (this.inbox.length) {
      return Promise.resolve(this.inResult(this.inbox.shift(), length));
    }
    return new Promise((resolve, reject) => {
      this.pendingIn.push({ resolve: (bytes) => resolve(this.inResult(bytes, length)), reject, endpointNumber, length });
    });
  }

  async clearHalt() {}

  // --- the board's side ----------------------------------------------------

  // The board sending: the transfers already waiting take it a transfer's
  // length at a time, in the order they were submitted; what is left waits.
  deliver(bytes) {
    let rest = Uint8Array.from(bytes);
    while (rest.length && this.pendingIn.length) {
      const pending = this.pendingIn.shift();
      pending.resolve(rest.slice(0, pending.length));
      rest = rest.slice(pending.length);
    }
    if (rest.length) this.inbox.push(rest);
  }

  unplug() {
    this.unplugged = true;
    this.opened = false;
    for (const pending of this.pendingIn.splice(0)) {
      pending.reject(dom("NotFoundError", "The device was disconnected."));
    }
  }

  writtenBytes() {
    const total = this.written.reduce((sum, write) => sum + write.bytes.length, 0);
    const out = new Uint8Array(total);
    let at = 0;
    for (const write of this.written) {
      out.set(write.bytes, at);
      at += write.bytes.length;
    }
    return out;
  }

  inResult(bytes, length) {
    const data = bytes.slice(0, length);
    if (bytes.length > length) this.inbox.unshift(bytes.slice(length));
    return { status: "ok", data: new DataView(data.buffer) };
  }

  requireOpen() {
    if (this.unplugged) throw dom("NotFoundError", "The device was disconnected.");
    if (!this.opened) throw dom("InvalidStateError", "The device must be opened first.");
  }
}

// `navigator.usb` over a set of fake devices: the chooser picks the first
// device a filter matches, as a user with one board would.
export class FakeUsb extends EventTarget {
  constructor(devices = []) {
    super();
    this.devices = devices;
    this.granted = new Set();
    this.lastFilters = null;
  }

  async requestDevice({ filters }) {
    this.lastFilters = filters;
    const device = this.devices.find((candidate) => filters.some((filter) => matches(candidate, filter)));
    if (!device) throw dom("NotFoundError", "No device selected.");
    this.granted.add(device);
    return device;
  }

  async getDevices() {
    return this.devices.filter((device) => this.granted.has(device) && !device.unplugged);
  }

  plugIn(device) {
    this.devices.push(device);
    this.granted.add(device);
    const event = new Event("connect");
    event.device = device;
    this.dispatchEvent(event);
  }
}

function matches(device, filter) {
  if (filter.vendorId !== undefined && filter.vendorId !== device.vendorId) return false;
  if (filter.productId !== undefined && filter.productId !== device.productId) return false;
  if (filter.classCode !== undefined) {
    return device.interfaces.some((i) => i.alternates.some((a) => a.interfaceClass === filter.classCode));
  }
  return true;
}

function iface(interfaceNumber, interfaceClass, endpoints) {
  return {
    interfaceNumber,
    claimed: false,
    alternates: [{ alternateSetting: 0, interfaceClass, interfaceSubclass: 0, interfaceProtocol: 0, endpoints }],
  };
}

function dom(name, message) {
  return new DOMException(message, name);
}
