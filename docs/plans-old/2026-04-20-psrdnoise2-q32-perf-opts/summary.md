# psrdnoise2_q32 perf plan — summary

**Baseline profile (Phase 0, from commit `ab33c51a`):**  
`profiles/2026-04-20T11-01-51--examples-perf-fastmath--steady-render--p0-baseline/`

**Final profile (Phase 6):**  
`profiles/2026-04-20T11-49-50--examples-perf-fastmath--steady-render--p6-final/`

Both runs target `examples/perf/fastmath` steady render; figures are **self cycles** and **self %** from each `report.txt` (ESP32-C6 cycle model).

## Key symbols

| Symbol | p0-baseline self (cyc / %) | p6-final self (cyc / %) |
|--------|------------------------------|-------------------------|
| `__lp_lpfn_psrdnoise2_q32` | 2,886,902 / 36.3% | 1,107,968 / 18.7% |
| `__lps_sin_q32` | 356,096 / 4.5% | 567,096 / 9.6% |
| `__lps_cos_q32` | 221,177 / 2.8% | *(not in top 20)* |
| `__lp_lpir_fdiv_recip_q32` | 400,384 / 5.0% | 614,400 / 10.4% |
| `int::specialized_div_rem::u64_div_rem` | 217,686 / 2.7% | 92,771 / 1.6% |
| `__divdi3` | 163,500 / 2.1% | 43,788 / 0.7% |

## Notes

- **psrdnoise2** self share roughly halved vs baseline (36.3% → 18.7%), matching the plan goal.
- **Sincos / LUT work** removed standalone `__lps_cos_q32` from the hot top 20; residual `__lps_sin_q32` share rose as other callees shrank (same attributed total cycles; percentages renormalize).
- **Total attributed cycles** dropped from **7,948,830** (p0) to **5,931,473** (p6-final) in these two reports.
