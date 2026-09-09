// M5 P1b: run the wasip1 build of lp-emu-esp32c6 under the SAME minimal WASI
// preview1 shim the phone rig uses (scripts/emu/bench-web/worker.js), but in
// node, so the wasm A/B can be taken on the desk in the same window as the
// native one.  Node is V8; the phone is JSC.  That difference is the point of
// the measurement, not a flaw in it.
import { readFileSync } from 'node:fs';

const EBADF = 8, ENOENT = 44, ENOTSUP = 58;
const enc = new TextEncoder(), dec = new TextDecoder();

function makeWasi(args, elfFilename, elfBytes) {
  let memory = null;
  const out = { 1: [], 2: [] };
  const files = {};
  let nextFd = 5;
  const mem = () => new DataView(memory.buffer);
  const u8 = () => new Uint8Array(memory.buffer);
  const wasi = {
    args_sizes_get(argcPtr, bufSizePtr) {
      const dv = mem();
      dv.setUint32(argcPtr, args.length, true);
      dv.setUint32(bufSizePtr, args.reduce((n, a) => n + enc.encode(a).length + 1, 0), true);
      return 0;
    },
    args_get(argvPtr, bufPtr) {
      const dv = mem(), m = u8();
      let p = bufPtr;
      for (let i = 0; i < args.length; i++) {
        dv.setUint32(argvPtr + 4 * i, p, true);
        const b = enc.encode(args[i]);
        m.set(b, p); m[p + b.length] = 0; p += b.length + 1;
      }
      return 0;
    },
    environ_sizes_get(cPtr, sPtr) { const dv = mem(); dv.setUint32(cPtr, 0, true); dv.setUint32(sPtr, 0, true); return 0; },
    environ_get() { return 0; },
    clock_time_get(id, _prec, ptr) {
      const ns = id === 0 ? BigInt(Math.round(Date.now() * 1e6)) : BigInt(Math.round(performance.now() * 1e6));
      mem().setBigUint64(ptr, ns, true);
      return 0;
    },
    random_get(ptr, len) {
      const m = u8();
      for (let off = 0; off < len; off += 65536) crypto.getRandomValues(m.subarray(ptr + off, ptr + Math.min(len, off + 65536)));
      return 0;
    },
    fd_prestat_get(fd, buf) {
      if (fd !== 3) return EBADF;
      const dv = mem(); dv.setUint8(buf, 0); dv.setUint32(buf + 4, 2, true);
      return 0;
    },
    fd_prestat_dir_name(fd, ptr, len) {
      if (fd !== 3) return EBADF;
      u8().set(enc.encode('/w').subarray(0, len), ptr);
      return 0;
    },
    path_open(dirfd, _dirflags, pathPtr, pathLen, oflags, _rb, _ri, _fdflags, fdOut) {
      if (dirfd !== 3) return EBADF;
      const path = dec.decode(u8().subarray(pathPtr, pathPtr + pathLen));
      const fd = nextFd++;
      if (path === elfFilename) files[fd] = { data: elfBytes, pos: 0 };
      else if (oflags & 1) out[fd] = [];
      else return ENOENT;
      mem().setUint32(fdOut, fd, true);
      return 0;
    },
    fd_fdstat_get(fd, buf) {
      const dv = mem();
      const type = fd <= 2 ? 2 : fd === 3 ? 3 : 4;
      dv.setUint8(buf, type); dv.setUint16(buf + 2, 0, true);
      dv.setBigUint64(buf + 8, 0xffffffffffffffffn, true); dv.setBigUint64(buf + 16, 0xffffffffffffffffn, true);
      return 0;
    },
    fd_fdstat_set_flags() { return 0; },
    fd_filestat_get(fd, buf) {
      const f = files[fd]; if (!f && !out[fd]) return EBADF;
      const dv = mem(); u8().fill(0, buf, buf + 64);
      dv.setUint8(buf + 16, 4);
      dv.setBigUint64(buf + 24, 1n, true);
      dv.setBigUint64(buf + 32, BigInt(f ? f.data.length : 0), true);
      return 0;
    },
    path_filestat_get(dirfd, _flags, pathPtr, pathLen, buf) {
      if (dirfd !== 3) return EBADF;
      const path = dec.decode(u8().subarray(pathPtr, pathPtr + pathLen));
      if (path !== elfFilename) return ENOENT;
      const dv = mem(); u8().fill(0, buf, buf + 64);
      dv.setUint8(buf + 16, 4);
      dv.setBigUint64(buf + 24, 1n, true);
      dv.setBigUint64(buf + 32, BigInt(elfBytes.length), true);
      return 0;
    },
    fd_read(fd, iovs, iovsLen, nreadPtr) {
      const dv = mem(), m = u8(); const f = files[fd];
      let n = 0;
      if (f) for (let i = 0; i < iovsLen; i++) {
        const p = dv.getUint32(iovs + 8 * i, true), l = dv.getUint32(iovs + 8 * i + 4, true);
        const take = Math.min(l, f.data.length - f.pos);
        if (take <= 0) break;
        m.set(f.data.subarray(f.pos, f.pos + take), p); f.pos += take; n += take;
      }
      dv.setUint32(nreadPtr, n, true);
      return 0;
    },
    fd_write(fd, iovs, iovsLen, nwPtr) {
      const dv = mem(), m = u8(); let n = 0;
      const sink = out[fd]; if (!sink) return EBADF;
      for (let i = 0; i < iovsLen; i++) {
        const p = dv.getUint32(iovs + 8 * i, true), l = dv.getUint32(iovs + 8 * i + 4, true);
        sink.push(m.slice(p, p + l)); n += l;
      }
      dv.setUint32(nwPtr, n, true);
      return 0;
    },
    fd_close() { return 0; },
    fd_seek() { return ENOTSUP; },
    sock_accept() { return ENOTSUP; },
    proc_exit(code) { throw { wasiExit: code }; },
  };
  return {
    imports: { wasi_snapshot_preview1: wasi },
    setMemory(m) { memory = m; },
    text(fd) {
      const parts = out[fd] || [];
      const len = parts.reduce((a, b) => a + b.length, 0);
      const all = new Uint8Array(len); let o = 0;
      for (const p of parts) { all.set(p, o); o += p.length; }
      return dec.decode(all);
    },
  };
}

const [, , wasmPath, elfPath, grade, exitOn, extraArg] = process.argv;
const elfBytes = new Uint8Array(readFileSync(elfPath));
const elfName = 'fw.elf';
const compiled = await WebAssembly.compile(readFileSync(wasmPath));

const args = ['lp-emu-esp32c6', '--elf', '/w/' + elfName, '--timeout', '8s',
  '--wall-timeout', '900', '--uart0', 'file:/w/uart.txt', '--time-grade', grade];
if (exitOn && exitOn !== '-') args.push('--exit-on', exitOn);
if (extraArg) args.push(extraArg);

const wasi = makeWasi(args, elfName, elfBytes);
const inst = await WebAssembly.instantiate(compiled, wasi.imports);
wasi.setMemory(inst.exports.memory);
const cpu0 = process.cpuUsage();
const t0 = performance.now();
try { inst.exports._start(); } catch (e) { if (!(e && typeof e.wasiExit === 'number')) throw e; }
const t1 = performance.now();
const cpu = process.cpuUsage(cpu0);
const text = wasi.text(1) + '\n' + wasi.text(2);
const m = /stopped after (\d+) cycles \((\d+) us emulated, (\d+) instructions/.exec(text);
console.log(JSON.stringify({
  wasm: wasmPath.split('/').pop(), grade,
  wallS: +((t1 - t0) / 1000).toFixed(3),
  userS: +(cpu.user / 1e6).toFixed(3),
  cycles: m ? +m[1] : null, us: m ? +m[2] : null, instr: m ? +m[3] : null,
  uartLen: wasi.text(5).length,
}));
