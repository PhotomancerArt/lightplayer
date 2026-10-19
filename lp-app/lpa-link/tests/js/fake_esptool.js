// esptool-js 0.6.0's `Transport` and `ESPLoader`, as far as
// `browser_esp32_flash.js` uses them, over the fake stub in
// `fake_esp_stub.js`. A flow under test imports this module by path where it
// would import esptool-js from its CDN.
//
// `board` is the one board on the other end: its flash, the chip its ROM
// names, and every loader a flow opened (`loaders`), each with the
// `after(...)` calls it was asked for.
import { fakeBoard } from "./fake_esp_stub.js";

export const board = {
  flash: new Uint8Array(0),
  chipName: "ESP32-C6 (revision 0)",
  mac: "10:bd:a3:b0:bd:a8",
  loaders: [],
};

export class Transport {
  constructor(device) {
    this.device = device;
    this.disconnected = false;
  }

  async disconnect() {
    this.disconnected = true;
    this.device.events?.push({ disconnect: true });
  }
}

export class ESPLoader {
  constructor({ transport, terminal }) {
    const { loader, stub } = fakeBoard(board.flash);
    Object.assign(this, loader);
    this.transport = Object.assign(transport, loader.transport);
    this.terminal = terminal;
    this.stub = stub;
    this.afterCalls = [];
    this.chip = { readMac: async () => board.mac };
    board.loaders.push(this);
  }

  async main() {
    return board.chipName;
  }

  async after(mode) {
    this.afterCalls.push(mode);
  }

  async eraseFlash() {
    board.flash.fill(0xff);
    this.terminal.writeLine("Chip erase completed successfully in 0.1s");
  }

  // The C6's LP analog I2C clock is on and its master idle: nothing to undo.
  async readReg(_address) {
    return 0x20000000;
  }

  async writeReg() {}

  info(message) {
    this.terminal.writeLine(message);
  }
}
