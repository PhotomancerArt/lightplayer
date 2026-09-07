import sys, bisect
syms=[]
for line in open(sys.argv[1]):
    p=line.rstrip('\n').split(' ',2)
    if len(p)<3: continue
    try: a=int(p[0],16)
    except: continue
    if p[1].lower() in ('t','w','i','b','d','r','a'): syms.append((a,p[1],p[2]))
syms.sort(); addrs=[s[0] for s in syms]
for x in sys.argv[2:]:
    pc=int(x,16); i=bisect.bisect_right(addrs,pc)-1
    a,t,n=syms[i]; print(f"{x} -> {n}+0x{pc-a:x} [{t}]")
