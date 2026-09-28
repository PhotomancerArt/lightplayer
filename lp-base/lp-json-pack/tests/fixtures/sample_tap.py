"""Cut the committed fixture sample out of a wire-tap recording.

usage: python3 sample_tap.py <recording.tap> <out.txt>

A .tap file is records of "<t_us> <dir> <len>\n<len bytes>\n" (the emu-serve
door tap). Bytes are reassembled into lines per direction, and M! lines are
sampled evenly per message class (quotas below; 3 for any other class), then
written in stream order as "<dir> M!{json}". See README.md for provenance.
"""
import sys,json,collections
path=sys.argv[1]; out=sys.argv[2]
data=open(path,'rb').read()
i=0; recs=[]
while i<len(data):
    nl=data.index(b'\n',i); us,d,n=data[i:nl].split(b' '); n=int(n)
    recs.append((int(us),chr(d[0]),data[nl+1:nl+1+n])); i=nl+1+n+1
lines=[]; buf={'>':b'','<':b''}
for us,d,b in recs:
    buf[d]+=b
    while b'\n' in buf[d]:
        ln,buf[d]=buf[d].split(b'\n',1)
        lines.append((d,ln))
def kind(ln):
    j=json.loads(ln[2:]); m=j.get('msg',j)
    if isinstance(m,str): return m
    k=next(iter(m)); v=m[k]
    if isinstance(v,dict) and v:
        k2=next(iter(v))
        if k=='projectCommand' and isinstance(v.get('command'),dict): k2='command.'+next(iter(v['command']))
        return f"{k}.{k2}"
    return k
by=collections.defaultdict(list)
for idx,(d,ln) in enumerate(lines):
    if not ln.startswith(b'M!'): continue
    by[(d,kind(ln))].append((idx,ln))
# quotas per class
quota={('<','projectRead.events'):72,('>','projectRead.handle'):48,('<','heartbeat.fps'):24,('>','filesystem.writeChunk'):1,
       ('>','projectCommand.command.panel_write'):8,('<','projectCommand.response'):6}
# Classes never sampled: they carry key material (a browser's access key rides
# `accessAdd`'s entry as "k"), and a committed fixture holds no secrets.
excluded={('>','accessAdd.entry')}
chosen=[]
for key,v in sorted(by.items()):
    if key in excluded: continue
    q=quota.get(key,3)
    if len(v)<=q: pick=v
    else:
        step=len(v)/q; pick=[v[int(k*step)] for k in range(q)]
    chosen+= [(idx,key[0],ln) for idx,ln in pick]
chosen.sort()
with open(out,'wb') as f:
    for idx,d,ln in chosen: f.write(d.encode()+b' '+ln+b'\n')
import os
print(len(chosen), os.path.getsize(out))
c=collections.Counter((d,kind(ln)) for _,d,ln in chosen)
for k,v in sorted(c.items()): print(k,v)
