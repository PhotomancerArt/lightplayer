"""Summarise [RTOS-TICK] lines: per-line and pooled cost at 240 MHz."""
import re, sys
CPU_MHZ = 240
pat = re.compile(r"\[RTOS-TICK\] t_ms=(\d+) dt_ms=(\d+) ticks=(\d+) tick_cycles=(\d+) tick_max=(\d+) yields=(\d+) yield_cycles=(\d+) yield_max=(\d+)")
for path in sys.argv[1:]:
    text = open(path, errors="replace").read()
    text = re.sub(r"\x1b\[[0-9;]*[A-Za-z]", "", text)
    rows = [tuple(map(int, m.groups())) for m in pat.finditer(text)]
    print(f"== {path}: {len(rows)} lines")
    print("  t_s   dt_ms ticks  us/tick  max_us  tick%   yields us/yield max_us yield%")
    tot = dict(dt=0, ticks=0, tc=0, y=0, yc=0, tmax=0, ymax=0)
    for i,(t, dt, ticks, tc, tmax, y, yc, ymax) in enumerate(rows):
        if i == 0:  # first line spans boot; skip from pooled totals
            tag = " (boot span, excluded)"
        else:
            tag = ""
            tot["dt"] += dt; tot["ticks"] += ticks; tot["tc"] += tc; tot["y"] += y; tot["yc"] += yc
            tot["tmax"] = max(tot["tmax"], tmax); tot["ymax"] = max(tot["ymax"], ymax)
        us_tick = tc / CPU_MHZ / ticks if ticks else 0
        us_y = yc / CPU_MHZ / y if y else 0
        print(f"  {t/1000:6.1f} {dt:5d} {ticks:5d} {us_tick:8.2f} {tmax/CPU_MHZ:7.1f} {100*tc/(CPU_MHZ*1000*dt):6.3f} {y:6d} {us_y:8.2f} {ymax/CPU_MHZ:6.1f} {100*yc/(CPU_MHZ*1000*dt):6.3f}{tag}")
    if tot["dt"]:
        print(f"  pooled over {tot['dt']/1000:.1f} s: {tot['ticks']/(tot['dt']/1000):.1f} ticks/s, "
              f"{tot['tc']/CPU_MHZ/max(tot['ticks'],1):.2f} us/tick (max {tot['tmax']/CPU_MHZ:.1f} us), "
              f"tick {100*tot['tc']/(CPU_MHZ*1000*tot['dt']):.3f} % of the PRO core; "
              f"{tot['y']/(tot['dt']/1000):.1f} yields/s, {tot['yc']/CPU_MHZ/max(tot['y'],1):.2f} us/yield (max {tot['ymax']/CPU_MHZ:.1f} us), "
              f"yield {100*tot['yc']/(CPU_MHZ*1000*tot['dt']):.3f} %")
