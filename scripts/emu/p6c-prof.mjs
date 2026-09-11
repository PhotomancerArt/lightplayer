#!/usr/bin/env node
// M7 P6c (Q1): what is inside the emulator's OWN wasm during a translated run.
//
//   node --cpu-prof --cpu-prof-dir=target/p6c/prof \
//     target/emu-bench-web/bench-cli.mjs --stage target/emu-bench-web \
//     --image render-basic --grade t2 --mode jit --fn-blocks 32 --timeout 5500ms
//   node scripts/emu/p6c-prof.mjs target/p6c/prof/CPU.*.cpuprofile
//
// `p6b-prof.mjs` answered "how much" — 55.4 % of the run at 64 blocks a
// function — by bucketing on the frame's `url`. This answers "of what", and it
// needs two things `p6b-prof.mjs` does not do.
//
// **Names.** The `wasm32-wasip1` build keeps its name section (325,234 B of
// it), so every frame in the emulator's own module carries a v0-mangled Rust
// symbol. `rustfilt` turns them into paths, and a path is what a bucket rule
// can be written against. Without this the ten hottest functions are ten
// copies of `wasm-function[N]`.
//
// **Boot against steady state.** A translated run emits its module INSIDE the
// same wasm, three times on `render-basic` t2 (boot plus two `fence.i`
// events), and `wasm_encoder` is the second-hottest function in the whole
// process. Attributing that to "the emulator" and then reasoning about the
// bus is how a phase spends a day on the wrong half. So every sample is
// classified by its ANCESTRY as well as its leaf: a sample with
// `lp_emu_esp32c6::jit::install` anywhere on its stack is translation, and is
// reported separately from the steady state underneath it.
//
// The buckets are the ones P6c's brief asks for — MMIO dispatch, each
// peripheral, the scheduler, the hart's slice loop, the interpreter, the block
// cache, the entry index — plus the two the measurement itself turned up
// (translation, and the pin fabric).
import { readFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';

const paths = process.argv.slice(2).filter((a) => !a.startsWith('--'));
const opt = new Set(process.argv.slice(2).filter((a) => a.startsWith('--')));
if (!paths.length) {
  console.error('usage: p6c-prof.mjs [--json] <file.cpuprofile> [...]');
  process.exit(2);
}

/// The bucket rules, in order — first match wins. Written against the
/// DEMANGLED path, so a rule reads like the module it is about.
///
/// `re` is tested against the demangled name; `name` is the row it lands in.
const RULES = [
  // --- the entry path: the host side of a stay, and the index it looks the
  // entry up in. P6b priced this at 82.6 ns an entry natively, of which 24.5
  // was two ordered-map lookups; this is the same thing in wasm.
  [/BTreeMap<u32, u32>>::get/, 'entry index (BTreeMap lookups)'],
  [/TranslatedCore<.*>>::run|JitCore as |mach::translated/, 'entry path (JitCore::run)'],

  // --- MMIO: the bus's own dispatch, and the two host callbacks the module
  // calls through. Everything from the guest address to the peripheral.
  [/HostOps>::mmio_(load|store)|^jit_mmio_/, 'MMIO dispatch (host callback)'],
  [/bus::SocBus>?(::| as ).*(read_mmio|write_mmio|mmio_index|require_mmio)/, 'MMIO dispatch (bus routing)'],
  [/bus::SocBus>?(::| as ).*(region_index|check_grade|check_watchpoints|arena_offset)/, 'MMIO dispatch (bus routing)'],
  [/bus::SocBus>::(read|write|load|store)\b/, 'MMIO dispatch (bus routing)'],

  // --- the peripherals, each its own row.
  [/periph::rmt|engine::rmt/, 'peripheral: RMT'],
  [/periph::uart|engine::uart/, 'peripheral: UART'],
  [/periph::systimer/, 'peripheral: SYSTIMER'],
  [/periph::timg|engine::timg/, 'peripheral: TIMG'],
  [/periph::gpio|periph::io_mux/, 'peripheral: GPIO/IO_MUX'],
  [/intmatrix|periph::intpri/, 'peripheral: INTMTX/INTPRI'],
  [/periph::pcr/, 'peripheral: PCR'],
  [/periph::spi|engine::spi/, 'peripheral: SPI'],
  [/periph::usb_sj/, 'peripheral: USB_SERIAL_JTAG'],
  [/periph::sha|engine::sha/, 'peripheral: SHA'],
  [/periph::(efuse|rng|lp_wdt|i2c_ana_mst|wifi_stub|accept)/, 'peripheral: other'],
  [/esp_common::periph|Peripheral>::(read|write|tick|poll)/, 'peripheral: other'],

  // --- the pin fabric: what a peripheral's output does after it is written.
  [/pins::Fabric|drain_pins|::strip::|ws281x/, 'pin fabric + LED strip'],

  // --- the scheduler.
  [/lp_emu_core::sched|Scheduler>/, 'scheduler'],

  // --- the hart's own loop, and the machine step around it.
  [/Esp32C6Machine>::run_until|Esp32C6Machine>::step|MachineHart<.*>>::step|machine::Esp32C6Machine>::(drain|service|pump)/, "hart slice loop"],

  // --- the interpreter: the ~6.5 % of instructions translated code does not
  // cover, plus everything between an exit and the next entry.
  [/emu::executor|emu::decoder|riscv_emu::mach::(csr|trap|trigger|block)|fetch_instruction/, 'interpreter'],
  [/lp_riscv_emu::/, 'interpreter'],

  // --- the block cache the interpreter serves those instructions from.
  [/lp_emu_core::block|esp32c6::cache/, 'block cache'],

  // --- translation. Only reached when the ancestry test did not already
  // catch it (an emit that happens off `jit::install`'s stack).
  [/wasm_encoder|leb128fmt|lp_emu_jit::(translate|dispatch|decode|discover|blocks)|jit::install|jit::arena_word|emit_sizes/, 'translation (emit)'],

  // --- the wasm runtime under all of it.
  [/^(dlmalloc|dlfree|dlcalloc|calloc|malloc|free|realloc|sbrk|memcpy|memset|memmove|__multi3|__udivti3|abort)$/, 'allocator + runtime'],
  [/alloc::(vec|collections|raw_vec|alloc)|core::ptr::drop_in_place/, 'allocator + runtime'],
  [/^(read|write|__wasi_|fd_)/, 'WASI I/O'],
  [/std::(fs|io|time)/, 'WASI I/O'],
];

function bucketOf(name) {
  for (const [re, b] of RULES) if (re.test(name)) return b;
  return 'other (emulator wasm)';
}

for (const path of paths) {
  const p = JSON.parse(readFileSync(path, 'utf8'));
  const byId = new Map(p.nodes.map((n) => [n.id, n]));
  const parent = new Map();
  for (const n of p.nodes) for (const c of n.children ?? []) parent.set(c, n.id);

  // Self time from the sample stream and the delta stream: `hitCount` is
  // samples and the deltas are what those samples actually cost.
  const self = new Map();
  let total = 0;
  for (let i = 0; i < p.samples.length; i++) {
    const dt = p.timeDeltas[i] ?? 0;
    total += dt;
    self.set(p.samples[i], (self.get(p.samples[i]) ?? 0) + dt);
  }

  const isEmu = (f) => (f.url || '').includes('lp_emu_esp32c6');
  const isTranslated = (f) => (f.url || '').startsWith('wasm://') && !isEmu(f);

  // Demangle every emulator-module name the profile carries, in one call.
  const raw = [...new Set(p.nodes.filter((n) => isEmu(n.callFrame)).map((n) => n.callFrame.functionName))];
  const dem = raw.length
    ? execFileSync('rustfilt', { input: raw.join('\n'), encoding: 'utf8', maxBuffer: 1 << 26 }).split('\n')
    : [];
  const nameOf = new Map(raw.map((r, i) => [r, dem[i] ?? r]));

  // Ancestry: is `jit::install` (or the sizing emit beside it) on this stack?
  const installing = new Map();
  const isInstalling = (id) => {
    if (installing.has(id)) return installing.get(id);
    const n = byId.get(id);
    let v = false;
    if (n) {
      const nm = isEmu(n.callFrame) ? nameOf.get(n.callFrame.functionName) ?? '' : '';
      v = /lp_emu_esp32c6::jit::install|jit::emit_sizes|install_translated_core/.test(nm)
        || (parent.has(id) ? isInstalling(parent.get(id)) : false);
    }
    installing.set(id, v);
    return v;
  };

  const top = new Map();       // the whole process, one level up
  const emuBuckets = new Map(); // steady state only
  const emuFns = new Map();
  let emuTotal = 0, emuBoot = 0, translatedTotal = 0;
  const translatedByUrl = new Map();

  for (const [id, us] of self) {
    const n = byId.get(id);
    if (!n) continue;
    const f = n.callFrame;
    if (isEmu(f)) {
      emuTotal += us;
      const nm = nameOf.get(f.functionName) ?? f.functionName;
      const boot = isInstalling(id);
      const b = boot ? 'translation (emit/compile/install)' : bucketOf(nm);
      if (boot) emuBoot += us;
      emuBuckets.set(b, (emuBuckets.get(b) ?? 0) + us);
      const key = (boot ? '[boot] ' : '') + nm;
      emuFns.set(key, (emuFns.get(key) ?? 0) + us);
      top.set("the emulator's own wasm", (top.get("the emulator's own wasm") ?? 0) + us);
    } else if (isTranslated(f)) {
      translatedTotal += us;
      translatedByUrl.set(f.url, (translatedByUrl.get(f.url) ?? 0) + us);
      top.set('the translated modules', (top.get('the translated modules') ?? 0) + us);
    } else if (['(garbage collector)', '(program)', '(idle)'].includes(f.functionName)) {
      top.set(f.functionName, (top.get(f.functionName) ?? 0) + us);
    } else {
      top.set('the JavaScript rig (host, WASI shim, node)', (top.get('the JavaScript rig (host, WASI shim, node)') ?? 0) + us);
    }
  }

  const ms = (us) => (us / 1000).toFixed(1);
  const pct = (us, of = total) => ((100 * us) / of).toFixed(2);

  console.log(`profile   ${path}`);
  console.log(`samples   ${p.samples.length}, ${ms(total)} ms of wall clock attributed`);
  console.log('');
  console.log('the run, one level up'.padEnd(46) + 'ms'.padStart(10) + '% run'.padStart(9));
  console.log('-'.repeat(65));
  for (const [b, us] of [...top].sort((a, b2) => b2[1] - a[1])) {
    console.log(b.padEnd(46) + ms(us).padStart(10) + pct(us).padStart(9));
  }
  console.log('');
  for (const [u, us] of [...translatedByUrl].sort((a, b2) => b2[1] - a[1])) {
    console.log(`  translated module ${u.padEnd(26)} ${ms(us).padStart(9)} ${pct(us).padStart(8)}`);
  }

  console.log('');
  console.log(`inside the emulator's own wasm — ${ms(emuTotal)} ms, ${pct(emuTotal)} % of the run`);
  console.log(`  of which translation (ancestry: jit::install): ${ms(emuBoot)} ms, ${pct(emuBoot)} % of the run`);
  console.log('');
  console.log('bucket'.padEnd(40) + 'ms'.padStart(10) + '% run'.padStart(9) + '% emu'.padStart(9));
  console.log('-'.repeat(68));
  for (const [b, us] of [...emuBuckets].sort((a, b2) => b2[1] - a[1])) {
    console.log(b.padEnd(40) + ms(us).padStart(10) + pct(us).padStart(9) + pct(us, emuTotal).padStart(9));
  }
  console.log('-'.repeat(68));
  console.log('TOTAL'.padEnd(40) + ms(emuTotal).padStart(10) + pct(emuTotal).padStart(9) + '100.00'.padStart(9));

  console.log('');
  console.log('the hottest functions in the emulator’s own wasm');
  console.log('-'.repeat(96));
  const n = opt.has('--all') ? 1e9 : 30;
  for (const [k, us] of [...emuFns].sort((a, b2) => b2[1] - a[1]).slice(0, n)) {
    console.log(ms(us).padStart(9) + pct(us).padStart(8) + '  ' + bucketOf(k.replace('[boot] ', '')).padEnd(34) + k.slice(0, 120));
  }
  console.log('');
}
