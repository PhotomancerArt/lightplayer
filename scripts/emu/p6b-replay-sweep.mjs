#!/usr/bin/env node
// M7 P6b scratch: the blocks-per-sub-dispatcher sweep, on the module alone.
//
//   node scripts/emu/p6b-replay-sweep.mjs <recording-dir> [--sizes 8,16,...]
//                                         [--engines node,bun] [--seconds N]
//
// P6's `jit-image-bench.mjs` replays ONE module against a recording and times
// the per-entry loop — the module's `run` call AND the between-entries memory
// delta the replay stands the interpreter in with, which cannot be hoisted out
// of it. This drives it once per (engine, size) from a single
// parent invocation, so every row is taken back to back under the same desk
// load — the comparability rule G-M7P states — and prints them as one table
// with the two numbers P6b is after beside the two the bench reports:
//
//   ns/entry    = steady ns/instr x (retired / entries)
//   crosses/entry = cross-function hops the module made per entry
//
// The whole point of the size axis here is that the recording is the SAME for
// every row — same block set, same entries, same import answers, same guest
// work — so the only thing that changes between two rows is how the emitter
// split the blocks into wasm functions. A difference is therefore the split
// and can be nothing else.
//
// `--jit-record-sizes` writes `size-<n>.wasm` beside the recording and does
// NOT go through `install`, so sizes under `MIN_BLOCKS` exist here even
// though a real run cannot ask for them. That is what makes the "does the
// curve turn below 64" question answerable without touching the floor.
import { spawnSync } from 'node:child_process';
import { readFileSync, existsSync } from 'node:fs';
import { join } from 'node:path';
import { loadavg } from 'node:os';

const args = process.argv.slice(2);
const dir = args.shift();
if (!dir) {
  console.error('usage: p6b-replay-sweep.mjs <recording-dir> [--sizes a,b] [--engines node,bun] [--seconds N]');
  process.exit(2);
}
let sizes = [8, 16, 32, 64, 128, 256];
let engines = ['node', 'bun'];
let seconds = 4;
let split = false;
for (let i = 0; i < args.length; i++) {
  if (args[i] === '--sizes') sizes = args[++i].split(',').map(Number);
  else if (args[i] === '--engines') engines = args[++i].split(',');
  else if (args[i] === '--seconds') seconds = Number(args[++i]);
  else if (args[i] === '--split') split = true;
  else throw new Error('unknown option ' + args[i]);
}

const meta = JSON.parse(readFileSync(join(dir, 'meta.json'), 'utf8'));
const perEntry = meta.retired / meta.entries;
console.log(`recording  ${dir}`);
console.log(`           ${meta.entries} entries, ${meta.retired} instructions retired, ` +
            `${perEntry.toFixed(2)} per entry, ${meta.blocks} blocks`);
console.log('');

// `ns/instr` is gross — it includes the harness's own per-entry work, of
// which the between-entries memory delta is by far the largest part (M7b P5).
// `harness` is that floor, measured by the same loop with the module call
// removed, and `net` is what the module cost. **Read a residual off `net`.**
const head = split
  ? ['engine', 'blocks/fn', 'fns', 'module MB', 'compile ms', 'E ns/entry', 'T ns/instr', 'flat ns/e', 'flat ns/i', 'cross/entry', 'load']
  : ['engine', 'blocks/fn', 'fns', 'module MB', 'compile ms', 'ns/instr 1st', 'ns/instr', 'harness', 'net', 'ns/entry', 'cross/entry', 'load'];
const w = split ? [7, 9, 7, 9, 10, 11, 11, 10, 10, 11, 6] : [7, 9, 7, 9, 10, 12, 9, 8, 8, 9, 11, 6];
const line = (cells) => cells.map((c, i) => String(c).padStart(w[i])).join('  ');
console.log(line(head));
console.log(w.map((n) => '-'.repeat(n)).join('  '));

const rows = [];
for (const engine of engines) {
  for (const size of sizes) {
    const wasm = join(dir, `size-${size}.wasm`);
    if (!existsSync(wasm)) {
      console.log(line([engine, size, '-', '-', '-', '-', '-', '-', '-', '-', '-', loadavg()[0].toFixed(1)]) + '   (no module)');
      continue;
    }
    const r = spawnSync(engine, [split ? 'scripts/emu/p6b-entry-split.mjs' : 'scripts/emu/jit-image-bench.mjs', dir, wasm, String(seconds)], {
      encoding: 'utf8', maxBuffer: 64 << 20,
    });
    const last = (r.stdout || '').trim().split('\n').pop();
    let j;
    try { j = JSON.parse(last); } catch {
      console.log(line([engine, size, '-', '-', '-', '-', 'FAILED', '-', '-', '-', '-', loadavg()[0].toFixed(1)]));
      console.log('  !! ' + ((r.stderr || last || '').trim().split('\n')[0] || 'no output'));
      continue;
    }
    const nsPerEntry = split
      ? j.nsPerEntryFlat
      : (j.steadyNsPerInstrNet ?? j.steadyNsPerInstr) * perEntry;
    const crossPerEntry = split ? j.crossPerEntry : j.crossesPerIteration / meta.entries;
    j.nsPerEntry = nsPerEntry;
    j.crossPerEntry = crossPerEntry;
    j.loadavg = loadavg()[0];
    j.size = size;
    rows.push(j);
    console.log(line(split ? [
      engine, size, Math.ceil(meta.blocks / size), (j.moduleBytes / 1e6).toFixed(1), j.compileMs.toFixed(0),
      j.fit2.nsPerEntry.toFixed(1), j.fit2.nsPerInstr.toFixed(3),
      j.nsPerEntryFlat.toFixed(1), j.nsPerInstrFlat.toFixed(3),
      crossPerEntry.toFixed(2), j.loadavg.toFixed(1),
    ] : [
      engine, size, Math.ceil(meta.blocks / size),
      (j.moduleBytes / 1e6).toFixed(1), j.compileMs.toFixed(0),
      j.firstSecondNsPerInstr.toFixed(3), j.steadyNsPerInstr.toFixed(3),
      (j.harnessNsPerInstr ?? 0).toFixed(3), (j.steadyNsPerInstrNet ?? j.steadyNsPerInstr).toFixed(3),
      nsPerEntry.toFixed(1), crossPerEntry.toFixed(2), j.loadavg.toFixed(1),
    ]));
  }
}

// The hop, priced. Two rows of the same recording differ in exactly one thing
// — how many cross-function hops the module makes — so the slope of time
// against hops is the cost of one hop, measured rather than modelled. Reported
// per engine and only where the sign is the same across the pairs, because a
// slope fitted through rows that also differ in engine codegen is not a hop
// cost.
console.log('');
console.log('== the hop, from the slope between adjacent sizes ==');
console.log(['engine', 'pair', 'd cross/entry', 'd ns/entry', 'ns per hop'].join('\t'));
for (const engine of engines) {
  const mine = rows.filter((r) => r.engine.startsWith(engine === 'node' ? 'node' : 'bun'));
  for (let i = 1; i < mine.length; i++) {
    const a = mine[i - 1], b = mine[i];
    const dc = b.crossPerEntry - a.crossPerEntry;
    const dt = b.nsPerEntry - a.nsPerEntry;
    console.log([engine, `${a.size}->${b.size}`, dc.toFixed(3), dt.toFixed(2),
                 Math.abs(dc) < 1e-9 ? '-' : (dt / dc).toFixed(1)].join('\t'));
  }
}
