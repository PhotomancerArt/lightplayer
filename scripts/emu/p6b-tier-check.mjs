#!/usr/bin/env node
// M7 P6b (H4): is the real run's translated code even reaching V8's optimizing
// tier?
//
//   node scripts/emu/p6b-tier-check.mjs [--stage target/emu-bench-web]
//                                       [--reps 3] [--fn-blocks 64]
//
// The replay measures a module at steady state, because it runs the same
// 20,000 entries a thousand times over and every function it touches gets hot.
// A real run enters 3,145 sub-dispatchers 7.9 million times over nine seconds
// and then throws the module away at the next `fence.i`. Whether those
// functions get optimised at all is not something a steady-state replay can
// say, and it is worth two and a half times: at 64 blocks a function the same
// module replays at 3.07 ns per guest instruction optimised and 7.46 ns under
// `--liftoff-only`.
//
// So: the same bench row, alternating between plain `node` and `node
// --liftoff-only`, from ONE parent invocation so the desk load is the same for
// both. If the two are close, the run was already effectively all baseline and
// the optimizing tier is not where its time went.
import { spawnSync } from 'node:child_process';
import { loadavg } from 'node:os';

const args = process.argv.slice(2);
let stage = 'target/emu-bench-web', reps = 3, fnBlocks = '64', image = 'render-basic', grade = 't2', timeout = '5500ms';
let legNames = ['default (tiering)', '--liftoff-only'];
for (let i = 0; i < args.length; i++) {
  if (args[i] === '--stage') stage = args[++i];
  else if (args[i] === '--reps') reps = Number(args[++i]);
  else if (args[i] === '--fn-blocks') fnBlocks = args[++i];
  else if (args[i] === '--image') image = args[++i];
  else if (args[i] === '--grade') grade = args[++i];
  else if (args[i] === '--timeout') timeout = args[++i];
  else if (args[i] === '--legs') legNames = args[++i].split('|');
  else throw new Error('unknown option ' + args[i]);
}

const ALL = [
  { name: 'default (tiering)', flags: [] },
  { name: '--liftoff-only', flags: ['--liftoff-only'] },
  // Eager TurboFan for all 3,145 sub-dispatchers. Kept because the number is
  // a finding — it is catastrophic — but off by default: compiling them all
  // takes the desk's load average from 11 to 39 and contaminates every row
  // taken beside it.
  { name: '--no-liftoff', flags: ['--no-liftoff'] },
];
const legs = ALL.filter((l) => legNames.includes(l.name));

console.log(`${image} ${grade}, ${fnBlocks} blocks/fn, ${timeout}, ${reps} rep(s) each, alternating`);
console.log('');
console.log(['leg', 'rep', 'wall s', 'ns/instr', 'real time', 'cover %', 'load', 'uart'].join('\t'));

const rows = [];
for (let r = 0; r < reps; r++) {
  for (const leg of legs) {
    const out = spawnSync('node', [...leg.flags, `${stage}/bench-cli.mjs`, '--stage', stage,
      '--image', image, '--grade', grade, '--mode', 'jit', '--fn-blocks', fnBlocks,
      '--timeout', timeout], { encoding: 'utf8', maxBuffer: 64 << 20 });
    const line = (out.stdout || '').trim().split('\n').filter((l) => l.startsWith(image)).pop();
    if (!line) {
      console.log([leg.name, r, 'FAILED', '', '', '', loadavg()[0].toFixed(1), ''].join('\t'));
      console.log('  !! ' + ((out.stderr || out.stdout || '').trim().split('\n').pop() || 'no output'));
      continue;
    }
    const f = line.trim().split(/\s+/);
    // image grade mode fn wall ns/instr realtime cover stay esc load uart
    const row = { leg: leg.name, rep: r, wall: Number(f[4]), ns: Number(f[5]), real: f[6], cover: f[7], load: f[10], uart: f[11] };
    rows.push(row);
    console.log([row.leg, r, row.wall.toFixed(2), row.ns.toFixed(2), row.real, row.cover, row.load, row.uart].join('\t'));
  }
}

console.log('');
console.log(['leg', 'best wall s', 'best ns/instr', 'rows'].join('\t'));
for (const leg of legs) {
  const mine = rows.filter((x) => x.leg === leg.name);
  if (!mine.length) continue;
  const best = mine.reduce((a, b) => (a.wall <= b.wall ? a : b));
  console.log([leg.name, best.wall.toFixed(2), best.ns.toFixed(2), mine.length].join('\t'));
}
const uarts = new Set(rows.map((r) => r.uart));
console.log('');
console.log(uarts.size === 1
  ? `identity: every row's UART0 sha256 is ${[...uarts][0]}`
  : `identity: ROWS DISAGREE — ${[...uarts].join(', ')}`);
