# /// script
# dependencies = ["zstandard"]
# ///
"""Compare JSON-aware and dictionary compression on the recorded choker lens replies."""
import json, zlib, re, base64, statistics, sys, struct
import zstandard as zstd
D=sys.argv[1]
data=open(D,'rb').read(); i=0; buf=b''; reps=[]; t0=None
while i<len(data):
    nl=data.index(b'\n',i); us,d,n=data[i:nl].split(b' '); n=int(n); b=data[nl+1:nl+1+n]; i=nl+1+n+1; t0=t0 or int(us)
    if d!=b'<': continue
    buf+=b
    while b'\n' in buf:
        ln,buf=buf.split(b'\n',1)
        if b'"projectRead"' in ln and int(us)-t0>25e6: reps.append(ln+b'\n')
train, test = reps[:100], reps[100:400]
med=lambda xs: statistics.median(xs)
raw=[len(m) for m in test]
out={}
out['raw JSON line']=raw
out['deflate per message, no dict']=[len(zlib.compress(m,9)) for m in test]
# deflate with preset dictionary = concatenation of training messages (last 32KB)
zd=b''.join(train)[-32768:]
def defl_dict(m,wbits=15):
    c=zlib.compressobj(9,zlib.DEFLATED,-wbits,9,zlib.Z_DEFAULT_STRATEGY,zd[-(1<<wbits):]); return len(c.compress(m)+c.flush())
out['deflate + preset dict (32 KB window)']=[defl_dict(m) for m in test]
out['deflate + preset dict (4 KB window)']=[defl_dict(m,12) for m in test]
# streaming deflate across messages (context takeover), sync flush per message
for wb in (15,13,12,10):
    c=zlib.compressobj(9,zlib.DEFLATED,-wb,9); 
    for m in train: c.compress(m); c.flush(zlib.Z_SYNC_FLUSH)
    out[f'deflate stream, context kept ({1<<wb//1 if False else 2**wb//1024} KB window)']=[len(c.compress(m)+c.flush(zlib.Z_SYNC_FLUSH)) for m in test]
# zstd trained dictionary
dict_data=zstd.train_dictionary(16384, [m for m in reps[:100]]*3)
cz=zstd.ZstdCompressor(level=19, dict_data=dict_data)
out['zstd + trained 16 KB dict, per message']=[len(cz.compress(m)) for m in test]

# JSON-aware token transcoder (a sketch of "known-dictionary JSON"):
# keys and enum-ish short strings -> 1-byte ids from a dictionary built from training data;
# ints -> zigzag varint; floats -> f32; base64 values under known keys -> raw bytes; other strings -> len+utf8.
keys={}; strs={}
def collect(o):
    if isinstance(o,dict):
        for k,v in o.items(): keys.setdefault(k,len(keys)); collect(v)
    elif isinstance(o,list):
        for v in o: collect(v)
    elif isinstance(o,str) and len(o)<48: strs[o]=strs.get(o,0)+1
for m in train: collect(json.loads(m[2:]))
common=[s for s,c in sorted(strs.items(),key=lambda kv:-kv[1]) if c>=len(train)//2][:200]
sid={s:i for i,s in enumerate(common)}
B64KEYS={'bytes','data'}
def varint(n):
    n=(n<<1)^(n>>63) if n<0 else n<<1
    o=bytearray()
    while True:
        b=n&0x7f; n>>=7
        if n: o.append(b|0x80)
        else: o.append(b); return bytes(o)
def enc(o,key=None,dyn=None):
    if isinstance(o,dict):
        out=b'\x01'+varint(len(o))
        for k,v in o.items():
            out+= bytes([keys[k]]) if k in keys and keys[k]<250 else b'\xfe'+enc(k)
            out+=enc(v,k,dyn)
        return out
    if isinstance(o,list): return b'\x02'+varint(len(o))+b''.join(enc(v,key,dyn) for v in o)
    if o is None: return b'\x03'
    if o is True: return b'\x04'
    if o is False: return b'\x05'
    if isinstance(o,int): return b'\x06'+varint(o)
    if isinstance(o,float): return b'\x07'+struct.pack('<f',o)
    if isinstance(o,str):
        if key in B64KEYS:
            try: r=base64.b64decode(o,validate=True); return b'\x08'+varint(len(r))+r
            except Exception: pass
        if o in sid: return b'\x09'+bytes([sid[o]])
        if dyn is not None:
            if o in dyn: return b'\x0a'+varint(dyn[o])
            dyn[o]=len(dyn)
        e=o.encode(); return b'\x0b'+varint(len(e))+e
    raise TypeError(type(o))
out['token transcode (static key+string dict, varints, raw base64)']=[len(enc(json.loads(m[2:]))) for m in test]
dyn={}
for m in train: enc(json.loads(m[2:]),dyn=dyn)
out['token transcode + per-connection string table (HPACK-ish)']=[len(enc(json.loads(m[2:]),dyn=dyn)) for m in test]
tok=[enc(json.loads(m[2:])) for m in test]
c=zlib.compressobj(9,zlib.DEFLATED,-12,9)
for m in train: c.compress(enc(json.loads(m[2:]))); c.flush(zlib.Z_SYNC_FLUSH)
out['token transcode, then deflate stream (4 KB window)']=[len(c.compress(t)+c.flush(zlib.Z_SYNC_FLUSH)) for t in tok]
base=med(raw)
print(f"{len(test)} steady choker lens replies (train on the first 100 of {len(reps)}); dictionary: {len(keys)} keys, {len(common)} common strings")
for k,v in out.items(): print(f"{med(v):8.0f} B  ({base/med(v):4.1f}x)  {k}")
