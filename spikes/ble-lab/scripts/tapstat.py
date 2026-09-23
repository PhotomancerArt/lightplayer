import sys,json,collections,re
path=sys.argv[1]
data=open(path,'rb').read()
# parse records: "<us> <dir> <len>\n<bytes>\n"
i=0; recs=[]
while i<len(data):
    nl=data.index(b'\n',i); us,d,n=data[i:nl].split(b' '); n=int(n)
    recs.append((int(us),chr(d[0]),data[nl+1:nl+1+n])); i=nl+1+n+1
# reassemble lines per direction
lines=[]  # (t_us, dir, bytes)
buf={'>':b'','<':b''}; start={'>':None,'<':None}
for us,d,b in recs:
    if start[d] is None: start[d]=us
    buf[d]+=b
    while b'\n' in buf[d]:
        ln,buf[d]=buf[d].split(b'\n',1)
        lines.append((us,d,ln+b'\n'))
t0=recs[0][0]; t1=recs[-1][0]
print(f"span {(t1-t0)/1e6:.1f} s, chunks {len(recs)}")
def kind(ln):
    s=ln.decode('utf-8','replace')
    if not s.startswith('M!'): return 'console'
    try: j=json.loads(s[2:])
    except Exception: return 'M!(unparsed)'
    m=j.get('msg',j)
    if isinstance(m,str): return m
    k=next(iter(m)); v=m[k]
    if isinstance(v,dict) and v:
        k2=next(iter(v)); 
        if k in('projectCommand',) and isinstance(v.get('command'),dict): k2='command.'+next(iter(v['command']))
        return f"{k}.{k2}"
    return k
st=collections.defaultdict(list)
for us,d,ln in lines: st[(d,kind(ln))].append((us,len(ln)))
tot={'>':0,'<':0}
print(f"{'dir':3} {'kind':42} {'n':>5} {'min':>6} {'med':>6} {'max':>6} {'total':>8}")
for (d,k),v in sorted(st.items(), key=lambda kv:-sum(x[1] for x in kv[1])):
    sz=sorted(x[1] for x in v); tot[d]+=sum(sz)
    print(f"{d:3} {k:42} {len(v):5} {sz[0]:6} {sz[len(sz)//2]:6} {sz[-1]:6} {sum(sz):8}")
span=(t1-t0)/1e6
print(f"host->board {tot['>']} B = {tot['>']/span:.0f} B/s ; board->host {tot['<']} B = {tot['<']/span:.0f} B/s over {span:.1f}s")
# per-second timeline of bytes
sec=collections.defaultdict(lambda:[0,0])
for us,d,ln in lines: sec[int((us-t0)/1e6)][0 if d=='>' else 1]+=len(ln)
if '--timeline' in sys.argv:
    for s in sorted(sec): print(s, sec[s])
