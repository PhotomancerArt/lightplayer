// A minimal WASI preview1 shim: exactly the imports the wasip1 build of
// `lp-emu-esp32c6` declares, and nothing else.
//
// Lifted out of `worker.js` unchanged in behaviour when P6 needed a second
// caller: the worker runs it in a browser and `bench-cli.mjs` runs it in
// `bun`/`node`, and a desk engine row is only comparable to a phone row if
// both ran the same shim. It does no I/O of its own — the caller hands it the
// ELF bytes — so the same module works where there is a `fetch` and where
// there is a filesystem.
'use strict';

const EBADF = 8, ENOENT = 44, ENOTSUP = 58;
const enc = new TextEncoder(), dec = new TextDecoder();

export function makeWasi(args, elfFilename, elfBytes) {
  let memory = null;
  const out = { 1: [], 2: [] };            // stdout, stderr chunks (Uint8Array)
  const names = {};                         // fd -> the path it was created at
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
      for (let off = 0; off < len; off += 65536) globalThis.crypto.getRandomValues(m.subarray(ptr + off, ptr + Math.min(len, off + 65536)));
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
      else if (oflags & 1) { out[fd] = []; names[fd] = path; }  // O_CREAT: a write sink
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
    // No fd carries real O_NONBLOCK/append semantics in this shim (every fd
    // is either an in-memory file, an output-chunk sink, or a std stream),
    // so there is nothing a flag change could affect — accept it as a no-op
    // rather than fail a call the guest is allowed to make on any open fd.
    fd_fdstat_set_flags(_fd, _flags) { return 0; },
    fd_filestat_get(fd, buf) {
      const f = files[fd]; if (!f && !out[fd]) return EBADF;
      const dv = mem(); u8().fill(0, buf, buf + 64);
      dv.setUint8(buf + 16, 4);
      dv.setBigUint64(buf + 24, 1n, true);
      dv.setBigUint64(buf + 32, BigInt(f ? f.data.length : 0), true);
      return 0;
    },
    // Stat-by-path (no fd yet). The only path this shim actually holds
    // bytes for is the staged ELF; anything else (e.g. a pre-open existence
    // check on the uart0 output path) correctly reads as not-yet-created.
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
        // `slice`, not `subarray`: these bytes outlive the call and the
        // memory under them can grow (which detaches every view of it), and
        // the identity hash is taken off them long after the run.
        sink.push(m.slice(p, p + l)); n += l;
      }
      dv.setUint32(nwPtr, n, true);
      return 0;
    },
    fd_close() { return 0; },
    // Declared only by the `jit` build, through `--jit-record`'s
    // `create_dir_all`. Nothing the rig runs asks for a directory, and a shim
    // whose only filesystem is one preopen with flat names has nowhere to put
    // one — so it succeeds and creates nothing, which is what a caller that
    // then writes flat names into it observes anyway.
    path_create_directory(dirfd) { return dirfd === 3 ? 0 : EBADF; },
    // The bench never accepts a socket connection (there is no listener),
    // so a clean not-supported refusal is the correct behavior, not a
    // missing feature.
    sock_accept(_fd, _flags, _resultFdPtr) { return ENOTSUP; },
    proc_exit(code) { throw { wasiExit: code }; },
  };

  const join = (parts) => {
    const len = parts.reduce((a, b) => a + b.length, 0);
    const all = new Uint8Array(len);
    let o = 0;
    for (const p of parts) { all.set(p, o); o += p.length; }
    return all;
  };

  return {
    imports: { wasi_snapshot_preview1: wasi },
    setMemory(m) { memory = m; },
    text(fd) { return dec.decode(join(out[fd] || [])); },
    /// The bytes the guest wrote to a path it created — the UART0 capture and
    /// the frame dump, which are what identity against a native run is read
    /// off.
    bytesAt(path) {
      for (const fd of Object.keys(names)) if (names[fd] === path) return join(out[fd]);
      return new Uint8Array(0);
    },
  };
}
