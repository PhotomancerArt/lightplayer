// The DD41 arithmetic: best of N spaced presses quoted with the sequence,
// the spread, and the same-press translated ÷ interpreter ratio — computed by
// the server from the job's own presses, never typed by a human (D5).
//
// Pure functions, importable by the server and by `node --test`; the shape
// they produce is `jobs/<id>/report.json`, and `renderReportMd` writes the
// G-M7B five-press table beside it.
'use strict';

/// The key a row is grouped under: `render-basic/t2/jit/8`, `render-basic/t2/interp`.
export function rowKey(r) {
  return r.slug + '/' + r.grade + '/' + r.mode + (r.mode === 'jit' ? '/' + (r.fnBlocks ?? '?') : '');
}

const round = (x, d = 3) => (x === null || x === undefined || !Number.isFinite(x) ? null : Number(x.toFixed(d)));

function median(xs) {
  const s = xs.slice().sort((a, b) => a - b);
  if (!s.length) return null;
  const m = s.length >> 1;
  return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2;
}

/// `seq` is in press order with `null` for excluded presses; best/median over
/// the rest; `spreadPct = (max − min) / max × 100`.
function stats(seq) {
  const xs = seq.filter((x) => x !== null && x !== undefined);
  if (!xs.length) return { seq, best: null, bestPress: null, median: null, spreadPct: null, n: 0 };
  const best = Math.max(...xs);
  const min = Math.min(...xs);
  return {
    seq: seq.map((x) => round(x)),
    best: round(best), bestPress: seq.indexOf(best) + 1,
    median: round(median(xs)), spreadPct: round(((best - min) / best) * 100, 1), n: xs.length,
  };
}

/// Compute the report for one job from its presses. Each press is
/// `{n, build, state, tainted, taintReasons, results}` (the press file), in
/// press order. Tainted and failed presses are listed under `excluded` and
/// contribute `null` to every sequence. A `skipped` press — one the stability
/// rule stood down before it was ever sent (F3) — is neither: it was not
/// taken, so it is not a hole in the sequence, only a line in `rep.presses`.
export function computeReport(job, presses) {
  const builds = job.builds;
  const excluded = [];
  const perBuild = {};
  for (const b of builds) perBuild[b] = { presses: [], rows: {}, ratio: {}, identity: { uartSha256: null, consistent: true } };

  for (const p of presses) {
    const pb = perBuild[p.build];
    if (!pb) continue;
    if (p.state === 'skipped') continue;
    const ok = p.state === 'done' && !p.tainted;
    if (!ok) {
      const reason = p.state === 'failed' ? ['failed']
        : p.state !== 'done' ? ['not taken (' + p.state + ')']
          : (p.taintReasons && p.taintReasons.length ? p.taintReasons : ['tainted']);
      excluded.push({ press: p.n, build: p.build, reason });
    }
    pb.presses.push({ n: p.n, ok, results: ok ? (p.results || []) : [] });
  }

  for (const b of builds) {
    const pb = perBuild[b];
    const keys = new Set();
    for (const p of pb.presses) for (const r of p.results) if (r.realtime) keys.add(rowKey(r));
    for (const key of keys) {
      pb.rows[key] = stats(pb.presses.map((p) => {
        const r = p.results.find((x) => rowKey(x) === key && x.realtime);
        return r ? r.realtime : null;
      }));
      pb.rows[key].nsPerInstr = stats(pb.presses.map((p) => {
        const r = p.results.find((x) => rowKey(x) === key && x.nsPerInstr);
        return r ? r.nsPerInstr : null;
      }));
      delete pb.rows[key].nsPerInstr.bestPress;
    }
    // The same-press ratio: translated ÷ interpreter for the same slug/grade in
    // the SAME press — the thermal half cancels because both rows saw the
    // same device state (DD41). Omitted for a key whose press has no
    // interpreter row.
    for (const key of keys) {
      const [slug, grade, mode] = key.split('/');
      if (mode !== 'jit') continue;
      const interpKey = slug + '/' + grade + '/interp';
      if (!keys.has(interpKey)) continue;
      const seq = pb.presses.map((p) => {
        const t = p.results.find((x) => rowKey(x) === key && x.realtime);
        const i = p.results.find((x) => rowKey(x) === interpKey && x.realtime);
        return t && i ? t.realtime / i.realtime : null;
      });
      const s = stats(seq);
      pb.ratio[key] = { seq: s.seq.map((x) => round(x, 2)), best: round(s.best, 2), bestPress: s.bestPress, median: round(s.median, 2), spreadPct: s.spreadPct };
    }
    // Byte-identity across the build's rows: every non-failed row of the
    // build shares one UART sha, or the build ran two different things. A
    // null sha is unknown, not inconsistent (DD46).
    const shas = new Set();
    for (const p of pb.presses) for (const r of p.results) if (r.uartSha256 && !r.failed) shas.add(r.uartSha256);
    pb.identity = { uartSha256: shas.size ? Array.from(shas)[0] : null, consistent: shas.size <= 1, shas: Array.from(shas) };
    pb.presses = pb.presses.map((p) => ({ n: p.n, ok: p.ok }));
  }

  let ab = null;
  if (builds.length === 2) {
    const [A, B] = builds;
    ab = {};
    for (const key of Object.keys(perBuild[A].rows)) {
      const a = perBuild[A].rows[key], b = perBuild[B].rows[key];
      if (!b || a.best === null || b.best === null) continue;
      const e = { bestDelta: round(b.best / a.best - 1), medianDelta: round(b.median / a.median - 1) };
      const ra = perBuild[A].ratio[key], rb = perBuild[B].ratio[key];
      if (ra && rb && ra.median && rb.median) e.medianRatioDelta = round(rb.median / ra.median - 1);
      ab[key] = e;
    }
  }

  const device = job.boundDevice ? { id: job.boundDevice, name: job.boundDeviceName ?? null, ua: job.boundDeviceUa ?? null } : null;
  return {
    job: job.id, note: job.note ?? null, builds, rows: job.rows, repeats: job.repeats, spacingMs: job.spacingMs,
    device, state: job.state, computedAt: new Date().toISOString(),
    stopWhenStable: job.stopWhenStable ?? null,
    stoppedEarly: Array.isArray(job.stoppedEarly) && job.stoppedEarly.length ? job.stoppedEarly : null,
    presses: presses.map((p) => ({ n: p.n, build: p.build, state: p.state, tainted: !!p.tainted, at: p.resultAt ?? null })),
    excluded, perBuild, ab,
  };
}

// --- F3: the stability stopping rule --------------------------------------
//
// The rule reads the report's own `seq` — the rounded, excluded-aware press
// sequence `stats()` already produced — so there is one arithmetic in this
// file, not two. A tainted or failed press is `null` in that sequence and is
// filtered out here, which is how "excluded presses never count toward
// stability" is enforced rather than remembered.

/// The shape `makeJob` fills in for `stopWhenStable: {}` and the shape
/// `lab.sh queue --stop-when-stable` sends with no arguments.
///
/// `minPresses` is **4**, not the 3 the control row's own settling suggests.
/// Director's ruling, 2026-09-13, from `j-20260912-1718-5940` — the one job in
/// the sizing data where a real decision turned on the number: a window of 3
/// would have stopped that build with its `jit/8` row still climbing and
/// quoted a best 8.6 % low. A window of 4 fires on 2 of the 11 real runs
/// rather than 8, and that is the trade the lab wants.
export const DEFAULT_STOP_WHEN_STABLE = { row: 'interp', pct: 5, minPresses: 4 };

/// Which of a build's rows the rule watches. `'interp'` is every interpreter
/// row the build has produced (the control rows); `'all'` is every row;
/// anything else is one exact row key. Every named row must be stable — with
/// one row that is the brief's rule, with several it is the conservative one.
export function controlRowKeys(rep, build, row) {
  const pb = rep.perBuild && rep.perBuild[build];
  const keys = pb ? Object.keys(pb.rows) : [];
  if (row === 'all') return keys;
  if (row === 'interp') return keys.filter((k) => k.split('/')[2] === 'interp');
  return keys.filter((k) => k === row);
}

/// Is this build's control row settled? `{stable, afterPress, why, rows}`
/// where `afterPress` counts the presses of THIS build that were taken
/// (skipped presses are not among them) and `why` is the sentence the report
/// and the log quote. Never stable before `minPresses` counted values.
export function stabilityVerdict(rep, build, opt) {
  const o = { ...DEFAULT_STOP_WHEN_STABLE, ...(opt || {}) };
  const pb = rep.perBuild && rep.perBuild[build];
  if (!pb) return { stable: false, afterPress: 0, why: 'no build ' + build + ' in the report' };
  const afterPress = pb.presses.length;
  const keys = controlRowKeys(rep, build, o.row).sort(keyOrder);
  if (!keys.length) return { stable: false, afterPress, why: 'no control row matching "' + o.row + '" yet' };
  const rows = [];
  for (const key of keys) {
    const vals = pb.rows[key].seq.filter((x) => x !== null && x !== undefined);
    if (vals.length < o.minPresses) {
      return { stable: false, afterPress, why: shortKey(key) + ': ' + vals.length + ' of ' + o.minPresses + ' presses counted' };
    }
    const w = vals.slice(-o.minPresses);
    const max = Math.max(...w), min = Math.min(...w);
    const spreadPct = round(((max - min) / max) * 100, 1);
    if (spreadPct > o.pct) {
      return { stable: false, afterPress, why: shortKey(key) + ': last ' + o.minPresses + ' (' + w.map((x) => fmt(x)).join(' ') + ') spread ' + pct(spreadPct) + ' > ' + o.pct + ' %' };
    }
    rows.push({ key, window: w, spreadPct });
  }
  const why = rows.map((r) => shortKey(r.key) + ' last ' + o.minPresses + ' (' + r.window.map((x) => fmt(x)).join(' ') + ') within ' + pct(r.spreadPct)).join('; ') +
    ' — at or under ' + o.pct + ' % after ' + afterPress + ' press' + (afterPress === 1 ? '' : 'es');
  return { stable: true, afterPress, rows, why };
}

const fmt = (x, d = 3) => (x === null || x === undefined ? '–' : Number(x).toFixed(d));
const pct = (x) => (x === null || x === undefined ? '–' : Number(x).toFixed(1) + ' %');

/// The G-M7B five-press table: one section per build with the press
/// sequence, best and spread per row, the same-press ratio row, the excluded
/// presses; then an A/B section. Three decimals, percentages to one.
export function renderReportMd(rep) {
  const out = [];
  out.push('# Lab report — job ' + rep.job + (rep.note ? ' — ' + rep.note : ''));
  out.push('');
  out.push('- builds: ' + rep.builds.join(' vs ') + ' · rows: ' + (Array.isArray(rep.rows) ? rep.rows.map(rowKey).join(', ') : rep.rows) +
    ' · repeats: ' + rep.repeats + ' · spacing: ' + Math.round(rep.spacingMs / 1000) + ' s');
  out.push('- device: ' + (rep.device ? (rep.device.name || rep.device.id) + (rep.device.ua ? ' · ' + rep.device.ua : '') : 'none'));
  out.push('- state: ' + rep.state + ' · computed ' + rep.computedAt);
  out.push('- protocol: DD41 — best of N spaced presses, quoted with the sequence; the same-press translated ÷ interpreter ratio removes the thermal half.');
  if (rep.stopWhenStable) {
    out.push('- stopping rule: stop a build when its `' + rep.stopWhenStable.row + '` row\'s last ' + rep.stopWhenStable.minPresses +
      ' counted presses are within ' + rep.stopWhenStable.pct + ' % of each other (repeats ' + rep.repeats + ' stays the cap).');
  }
  if (rep.stoppedEarly) {
    for (const s of rep.stoppedEarly) {
      out.push('- **stopped early: ' + s.build + ' after press ' + s.afterPress + ' of ' + rep.repeats + '** — ' + s.why +
        (s.skipped ? ' (' + s.skipped + ' press' + (s.skipped === 1 ? '' : 'es') + ' not taken)' : ''));
    }
  }
  out.push('');
  for (const b of rep.builds) {
    const pb = rep.perBuild[b];
    const n = pb.presses.length;
    out.push('## ' + b + ' (' + pb.presses.filter((p) => p.ok).length + ' of ' + n + ' presses quoted)');
    out.push('');
    const head = ['row', ...Array.from({ length: n }, (_, i) => 'press ' + (i + 1)), 'best', 'median', 'spread'];
    out.push('| ' + head.join(' | ') + ' |');
    out.push('|' + head.map((_, i) => (i === 0 ? '---' : '---:')).join('|') + '|');
    const keys = Object.keys(pb.rows).sort(keyOrder);
    for (const key of keys) {
      const r = pb.rows[key];
      out.push('| ' + [shortKey(key), ...r.seq.map((x) => fmt(x)), r.best === null ? '–' : '**' + fmt(r.best) + '×** (p' + r.bestPress + ')', fmt(r.median), pct(r.spreadPct)].join(' | ') + ' |');
    }
    for (const key of Object.keys(pb.ratio).sort(keyOrder)) {
      const r = pb.ratio[key];
      out.push('| ' + ['**' + shortKey(key) + ' ÷ interpreter, same press**', ...r.seq.map((x) => fmt(x, 2)), r.best === null ? '–' : '**' + fmt(r.best, 2) + '×**', fmt(r.median, 2), pct(r.spreadPct)].join(' | ') + ' |');
    }
    out.push('');
    out.push('- byte-identity: ' + (pb.identity.consistent ? (pb.identity.uartSha256 ? 'UART `' + pb.identity.uartSha256.slice(0, 16) + '` on every row' : 'no sha reported') : '**INCONSISTENT** — ' + pb.identity.shas.map((s) => s.slice(0, 16)).join(', ')));
    const ex = rep.excluded.filter((e) => e.build === b);
    if (ex.length) out.push('- excluded: ' + ex.map((e) => 'press ' + e.press + ' (' + e.reason.join(', ') + ')').join('; '));
    out.push('');
  }
  if (rep.ab) {
    const [A, B] = rep.builds;
    out.push('## A/B: ' + B + ' over ' + A);
    out.push('');
    out.push('| row | best Δ | median Δ | same-press ratio, median Δ |');
    out.push('|---|---:|---:|---:|');
    for (const key of Object.keys(rep.ab).sort(keyOrder)) {
      const e = rep.ab[key];
      out.push('| ' + [shortKey(key), signed(e.bestDelta), signed(e.medianDelta), e.medianRatioDelta === undefined ? '–' : signed(e.medianRatioDelta)].join(' | ') + ' |');
    }
    out.push('');
  }
  return out.join('\n');
}

function signed(x) { return x === null || x === undefined ? '–' : (x >= 0 ? '+' : '') + (x * 100).toFixed(1) + ' %'; }
function shortKey(key) {
  const [slug, grade, mode, fn] = key.split('/');
  return slug + ' ' + grade + ' ' + (mode === 'jit' ? fn + '/fn' : 'interpreter');
}
function keyOrder(a, b) {
  const pa = a.split('/'), pb = b.split('/');
  if (pa[2] !== pb[2]) return pa[2] === 'jit' ? -1 : 1;
  return (Number(pa[3]) || 0) - (Number(pb[3]) || 0) || a.localeCompare(b);
}
