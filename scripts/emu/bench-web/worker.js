// Emulator speed bench worker: runs the wasip1 build of lp-emu-esp32c6 under a
// minimal WASI preview1 shim (only the 15 imports the module declares).
'use strict';

const EBADF = 8, ENOENT = 44;
const enc = new TextEncoder(), dec = new TextDecoder();

let compiled = null;
const elfCache = {}; // slug -> Uint8Array

function makeWasi(args, elfFilename, elfBytes) {
  let memory = null;
  const out = { 1: [], 2: [] };            // stdout, stderr chunks (Uint8Array)
  const files = {};                         // fd -> {data, pos} for readable files
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
      const dv = mem(); dv.setUint8(buf, 0); dv.setUint32(buf + 4, 2, true); // dir "/w"
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
      else if (oflags & 1) out[fd] = [];       // O_CREAT: a write sink
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
    fd_filestat_get(fd, buf) {
      const f = files[fd]; if (!f && !out[fd]) return EBADF;
      const dv = mem(); u8().fill(0, buf, buf + 64);
      dv.setUint8(buf + 16, 4);
      dv.setBigUint64(buf + 24, 1n, true);
      dv.setBigUint64(buf + 32, BigInt(f ? f.data.length : 0), true);
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
    proc_exit(code) { throw { wasiExit: code }; },
  };
  return {
    imports: { wasi_snapshot_preview1: wasi },
    setMemory(m) { memory = m; },
    text(fd) { const parts = out[fd] || []; const len = parts.reduce((a, b) => a + b.length, 0); const all = new Uint8Array(len); let o = 0; for (const p of parts) { all.set(p, o); o += p.length; } return dec.decode(all); },
  };
}

async function loadModule() {
  if (compiled) return;
  const t0 = performance.now();
  compiled = await WebAssembly.compile(await (await fetch('emu.wasm')).arrayBuffer());
  postMessage({ type: 'loaded', compileMs: performance.now() - t0 });
}

async function loadElf(image) {
  if (elfCache[image.slug]) return elfCache[image.slug];
  const bytes = new Uint8Array(await (await fetch(image.elf)).arrayBuffer());
  elfCache[image.slug] = bytes;
  return bytes;
}

async function runOnce(image, grade) {
  const elfBytes = await loadElf(image);
  const args = ['lp-emu-esp32c6', '--elf', '/w/' + image.elf, '--timeout', image.timeout,
    '--wall-timeout', '600', '--uart0', 'file:/w/' + image.slug + '.txt', '--time-grade', grade];
  if (image.exitOn) args.push('--exit-on', image.exitOn);
  const wasi = makeWasi(args, image.elf, elfBytes);
  const t0 = performance.now();
  const inst = await WebAssembly.instantiate(compiled, wasi.imports);
  wasi.setMemory(inst.exports.memory);
  const t1 = performance.now();
  let exit = 0;
  try { inst.exports._start(); } catch (e) { if (e && typeof e.wasiExit === 'number') exit = e.wasiExit; else throw e; }
  const t2 = performance.now();
  const text = wasi.text(1) + '\n' + wasi.text(2);
  const m = /stopped after (\d+) cycles \((\d+) us emulated, (\d+) instructions, grade ([^)]+)\)/.exec(text);
  const wallMs = t2 - t1;
  const r = { slug: image.slug, grade, exit, instantiateMs: t1 - t0, wallMs, tail: text.trim().split('\n').slice(-6).join('\n') };
  if (m) {
    r.cycles = +m[1]; r.us = +m[2]; r.instr = +m[3]; r.gradeName = m[4];
    r.ips = r.instr / (wallMs / 1000);
    r.realtime = (r.us / 1000) / wallMs;
  }
  return r;
}

onmessage = async (ev) => {
  try {
    await loadModule();
    const plan = ev.data.plan;
    const images = {};
    for (const image of ev.data.images) images[image.slug] = image;
    for (let i = 0; i < plan.length; i++) {
      const { slug, grade } = plan[i];
      postMessage({ type: 'progress', i, n: plan.length, slug, grade });
      const r = await runOnce(images[slug], grade);
      postMessage({ type: 'result', i, r });
    }
    postMessage({ type: 'done' });
  } catch (e) {
    postMessage({ type: 'error', message: String(e && e.stack || e) });
  }
};
