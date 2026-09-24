#!/usr/bin/env python3
"""Tiny driver for spikes/ble-lab: ask (text->reply), frames sweep, echo stats."""
import json, sys, urllib.request, re, statistics
import os
P = int(os.environ.get("BLE_LAB_PORT", "36666"))
def cmd(obj, timeout=60):
    req = urllib.request.Request(f"http://localhost:{P}/cmd", data=json.dumps(obj).encode(), method="POST")
    with urllib.request.urlopen(req, timeout=timeout+10) as r: return json.load(r)
def ev(js, timeout=30000):
    d = cmd({"op":"eval","js":js,"timeoutMs":timeout}, timeout/1000)
    return d.get("value") if d.get("ok") else {"error": d.get("error"), "serial": d.get("serial")}
ASK = """const got=new Promise((res,rej)=>{const t=setTimeout(()=>rej(new Error('no reply')),8000); lab.S.onPacket=(v)=>{clearTimeout(t);lab.S.onPacket=null;res(lab.dec.decode(v));};}); await lab.S.rx.writeValueWithResponse(lab.enc.encode(%s)); return await got;"""
def ask(text): return ev(ASK % json.dumps(text))
def frames(fps, secs=5, size=219, ack=False):
    js = f"""const fps={fps}, secs={secs}, n=Math.round(fps*secs); const f=new Uint8Array({size}); f[0]=0xF0; let late=0, maxLateMs=0; const t0=performance.now();
for(let i=0;i<n;i++){{ f[1]=i&255; const due=t0+i*1000/fps; const w=due-performance.now(); if(w>0) await lab.sleep(w); else {{late++; maxLateMs=Math.max(maxLateMs,-w);}}
 await lab.S.rx.{'writeValueWithResponse' if ack else 'writeValueWithoutResponse'}(f); }}
const sentMs=performance.now()-t0; await lab.sleep(400);
const got=new Promise((res,rej)=>{{const t=setTimeout(()=>rej(new Error('no reply')),8000); lab.S.onPacket=(v)=>{{clearTimeout(t);lab.S.onPacket=null;res(lab.dec.decode(v));}};}});
await lab.S.rx.writeValueWithResponse(lab.enc.encode('frames')); return {{n, sentMs:Math.round(sentMs), late, maxLateMs:Math.round(maxLateMs), board: await got}};"""
    return ev(js, int(secs*1000+20000))
def echo(n=20):
    r=[]
    for _ in range(n):
        d=cmd({"op":"echo"}); 
        if d.get("rttMs") is not None: r.append(d["rttMs"])
    r.sort(); 
    return {"n":len(r),"min":r[0],"median":statistics.median(r),"p90":r[int(len(r)*0.9)-1],"max":r[-1]} if r else {"n":0}
if __name__=="__main__":
    print(eval(sys.argv[1]))
