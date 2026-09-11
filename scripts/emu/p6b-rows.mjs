#!/usr/bin/env node
// M7 P6b: bench rows that survive a shared desk — interleaved, repeated, and
// reported best-of.
//
//   node scripts/emu/p6b-rows.mjs --engine bun --sizes 8,16,32,64 --reps 3
//   node scripts/emu/p6b-rows.mjs --engine node --sizes 32,64,128 --reps 3
//
// G-M7P's rule is that rows from ONE invocation are comparable and rows from
// two are not. That rule assumes the load holds still for the length of an
// invocation, and on this desk it does not: a `bun` sweep whose four rows take
// ninety seconds saw its `--interpreter` leg come out at 14.87 s in one
// invocation and 9.95 s in the next, which is enough to reverse the sign of
// the answer.
//
// So the legs are **interleaved** — every size and the interpreter row, then
// all of them again — and the reported number is the **best** of the repeats
// rather than the mean. A shared desk can only make a run slower, so the
// fastest observation of a leg is the one least contaminated, and comparing
// two legs' bests compares two numbers contaminated as little as this desk
// allows. Every row's own load average and UART0 sha256 are printed, so a run
// that was contaminated anyway, or that stopped being the same run, says so.
import { spawnSync } from 'node:child_process';
import { loadavg } from 'node:os';

const args = process.argv.slice(2);
let engine = 'node', stage = 'target/emu-bench-web', reps = 3, image = 'render-basic',
    grade = 't2', timeout = '5500ms', sizes = [64], interp = true;
for (let i = 0; i < args.length; i++) {
  if (args[i] === '--engine') engine = args[++i];
  else if (args[i] === '--stage') stage = args[++i];
  else if (args[i] === '--reps') reps = Number(args[++i]);
  else if (args[i] === '--image') image = args[++i];
  else if (args[i] === '--grade') grade = args[++i];
  else if (args[i] === '--timeout') timeout = args[++i];
  else if (args[i] === '--sizes') sizes = args[++i].split(',').map(Number);
  else if (args[i] === '--no-interp') interp = false;
  else throw new Error('unknown option ' + args[i]);
}

const legs = [
  ...sizes.map((s) => ({ name: `jit ${s}`, args: ['--mode', 'jit', '--fn-blocks', String(s)] })),
  ...(interp ? [{ name: '--interpreter', args: ['--mode', 'interp'] }] : []),
];

console.log(`${engine}  ${image} ${grade}  ${timeout}  ${reps} rep(s), interleaved`);
console.log('');
console.log(['leg', 'rep', 'wall s', 'ns/instr', 'real time', 'cover %', 'stay', 'sys load', 'uart'].join('\t'));

const rows = [];
for (let r = 0; r < reps; r++) {
  for (const leg of legs) {
    const sysLoad = loadavg()[0];
    const out = spawnSync(engine, [`${stage}/bench-cli.mjs`, '--stage', stage, '--image', image,
      '--grade', grade, '--timeout', timeout, ...leg.args], { encoding: 'utf8', maxBuffer: 64 << 20 });
    const line = (out.stdout || '').trim().split('\n').filter((l) => l.startsWith(image)).pop();
    if (!line) {
      console.log([leg.name, r, 'FAILED', '', '', '', '', sysLoad.toFixed(1), ''].join('\t'));
      console.log('  !! ' + ((out.stderr || out.stdout || '').trim().split('\n').pop() || 'no output'));
      continue;
    }
    const f = line.trim().split(/\s+/);
    const row = { leg: leg.name, rep: r, wall: Number(f[4]), ns: Number(f[5]),
                  real: f[6], cover: f[7], stay: f[8], sysLoad, uart: f[11] };
    rows.push(row);
    console.log([row.leg, r, row.wall.toFixed(2), row.ns.toFixed(2), row.real, row.cover,
                 row.stay, sysLoad.toFixed(1), row.uart].join('\t'));
  }
}

console.log('');
console.log(['leg', 'best wall s', 'best ns/instr', 'best real time', 'vs interpreter'].join('\t'));
const best = new Map();
for (const leg of legs) {
  const mine = rows.filter((x) => x.leg === leg.name);
  if (mine.length) best.set(leg.name, mine.reduce((a, b) => (a.wall <= b.wall ? a : b)));
}
const ref = best.get('--interpreter');
for (const [name, b] of best) {
  const speedup = ref ? (ref.wall / b.wall).toFixed(3) + 'x' : '-';
  console.log([name, b.wall.toFixed(2), b.ns.toFixed(2), b.real, speedup].join('\t'));
}
const uarts = new Set(rows.map((x) => x.uart));
console.log('');
console.log(uarts.size === 1
  ? `identity: every row's UART0 sha256 is ${[...uarts][0]}`
  : `identity: ROWS DISAGREE — ${[...uarts].join(', ')}`);
