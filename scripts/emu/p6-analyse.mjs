// M7 P6 scratch: read the rig's JSON rows and print the gate table's numbers.
//
//   node scripts/emu/p6-analyse.mjs <rows.json> [...]
//
// Everything here is arithmetic over numbers the runs themselves reported —
// the `stopped after` line, the `--jit-report` boot lines, the coverage line
// and the exit census. It computes nothing the rig could have measured and
// measures nothing itself.
import { readFileSync } from 'node:fs';

const files = process.argv.slice(2);
const all = [];
for (const f of files) {
  const j = JSON.parse(readFileSync(f, 'utf8'));
  for (const r of j.rows) { r.engine ||= j.engine; all.push(r); }
}

const bootMs = (r) => (r.boot || []).reduce((a, b) => a + b.discoverMs + b.emitMs + b.compileMs + b.instantiateMs, 0);

console.log('== rows ==');
const head = ['engine', 'image', 'grade', 'mode', 'fn', 'bound', 'wall s', 'boot s', 'run s', 'ns/instr', 'real', 'cover %', 'entries', 'stay', 'uart'];
console.log(head.join('\t'));
for (const r of all) {
  const boot = bootMs(r) / 1000;
  console.log([
    (r.engine || '').split(' ')[0], r.slug, r.grade, r.mode, r.fnBlocks ?? '-', r.timeout,
    (r.wallMs / 1000).toFixed(2), boot.toFixed(2), (r.wallMs / 1000 - boot).toFixed(2),
    r.nsPerInstr ? r.nsPerInstr.toFixed(2) : '-',
    r.realtime ? r.realtime.toFixed(3) : '-',
    r.coverage !== undefined ? r.coverage.toFixed(2) : '-',
    r.entries ?? '-', r.meanStay ? r.meanStay.toFixed(1) : '-',
    (r.uartSha256 || '-').slice(0, 12),
  ].join('\t'));
}

// Where a translated run's time goes, solved from two images that differ by
// 4.5x in mean stay length. The model is deliberately the simplest one that
// can separate the two costs the exit census says matter:
//
//   wall = boot + entries * E + translated_instr * T + interpreted_instr * I
//
// with I taken from that image's own `--interpreter` row, so the interpreter's
// cost is measured rather than fitted. Two images, two unknowns.
console.log('\n== where a translated run\'s time goes (E = per entry, T = per translated instruction) ==');
const engines = [...new Set(all.map((r) => (r.engine || '').split(' ')[0]))];
for (const eng of engines) {
  for (const grade of ['t1', 't2']) {
    const pick = (slug, mode) => all.find((r) => (r.engine || '').split(' ')[0] === eng && r.slug === slug && r.grade === grade && r.mode === mode && (mode !== 'jit' || r.fnBlocks === 64));
    const rows = [['render-basic', pick('render-basic', 'jit'), pick('render-basic', 'interp')],
                  ['render-rocaille', pick('render-rocaille', 'jit'), pick('render-rocaille', 'interp')]];
    if (rows.some(([, j, i]) => !j || !i)) continue;
    const eq = rows.map(([slug, j, i]) => {
      const I = i.nsPerInstr * 1e-9;                 // seconds per interpreted instruction
      const translated = j.coveredInstr;
      const interpreted = j.retiredInstr - j.coveredInstr;
      const rhs = j.wallMs / 1000 - bootMs(j) / 1000 - interpreted * I;
      return { slug, a: j.entries, b: translated, rhs, I };
    });
    // [a1 b1][E]   [r1]
    // [a2 b2][T] = [r2]
    const [p, q] = eq;
    const det = p.a * q.b - q.a * p.b;
    const E = (p.rhs * q.b - q.rhs * p.b) / det;
    const T = (p.a * q.rhs - q.a * p.rhs) / det;
    console.log(`${eng} ${grade}: per entry ${(E * 1e9).toFixed(0)} ns, per translated instruction ${(T * 1e9).toFixed(2)} ns ` +
      `(interpreted: ${eq.map((e) => (e.I * 1e9).toFixed(2)).join(' / ')} ns)`);
    for (const e of eq) {
      console.log(`    ${e.slug}: ${e.a} entries, ${e.b} translated instr, ${e.rhs.toFixed(2)} s of run after boot and the interpreted remainder` +
        ` -> entries cost ${(e.a * E).toFixed(2)} s (${(100 * e.a * E / e.rhs).toFixed(0)} %), code ${(e.b * T).toFixed(2)} s`);
    }
  }
}

console.log('\n== exit census, from the run\'s own --jit-report ==');
for (const r of all) {
  if (!r.exits || !r.exits.length) continue;
  console.log(`${(r.engine || '').split(' ')[0]} ${r.slug} ${r.grade} fn=${r.fnBlocks} ${r.timeout}: entries ${r.entries}, mean stay ${r.meanStay.toFixed(2)}, cross ${r.cross}, indirect_miss ${r.indirectMiss}, escape ${r.escapeHatch}`);
  for (const e of r.exits) console.log(`    ${e.why.padEnd(18)} ${String(e.exits).padStart(10)} exit(s), ${String(e.interpretedAfter).padStart(12)} instructions interpreted after`);
}

console.log('\n== boot cost (JD20), per translation event ==');
for (const r of all) {
  for (const b of r.boot || []) {
    console.log(`${(r.engine || '').split(' ')[0]} ${r.slug} ${r.grade} fn=${r.fnBlocks}: ${b.line}`);
  }
}
