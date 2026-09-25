"""P7 lens client: one connection; optional packed opt-in; N lens reads like Studio's.
usage: lens_client.py host:port json|packed capture.bin N lpcli"""
import socket, sys, time, json, re, subprocess
addr, enc, cap_path, N, LPCLI = sys.argv[1], sys.argv[2], sys.argv[3], int(sys.argv[4]), sys.argv[5]
host, port = addr.split(':')
TMPL = {"id":0,"msg":{"projectRead":{"handle":1,"request":{"since":None,"queries":[{"shapes":{"level":"detail"}},{"nodes":{"level":"detail","nodes":"all","include_slots":True}},{"runtime":None}],"probes":[{"output_frame":{"geometry":{"if_changed":{"known":[{"node":3,"revision":24}]}},"samples":"srgb8"}},{"binding_graph":{"structure":{"if_changed":{"known":[{"revision":0}]}},"include_values":True}}]}}}}
s = socket.create_connection((host, int(port))); s.settimeout(0.1)
cap = bytearray()
def drain(sec, until=None):
    end = time.time() + sec
    start = len(cap)
    while time.time() < end:
        try:
            b = s.recv(65536)
            if not b: break
            cap.extend(b)
        except socket.timeout:
            pass
        if until and until(bytes(cap[start:])): return True
    return False
def unpack(b):
    return subprocess.run([LPCLI, 'wire', 'unpack'], input=b, capture_output=True).stdout
def send(obj):
    s.sendall(b'M!' + json.dumps(obj, separators=(',', ':')).encode() + b'\n')
drain(3)
send({"id":8999,"msg":"listLoadedProjects"})
drain(5, lambda b: b'"listLoadedProjects":{' in b)
m = re.search(rb'"handle":(\d+)', bytes(cap))
TMPL['msg']['projectRead']['handle'] = int(m.group(1)) if m else 1
print('handle', TMPL['msg']['projectRead']['handle'], flush=True)
if enc == 'packed':
    send({"id":9000,"msg":{"setEncoding":{"encoding":"packed","dictionary":0x28ec2da6}}})
    drain(3, lambda b: b'"setEncoding"' in b)
since = None
for k in range(N):
    rid = 4311744544 + k
    TMPL['id'] = rid; TMPL['msg']['projectRead']['request']['since'] = since
    send(TMPL)
    before = len(cap)
    pat = re.compile(rb'M!\{"id":%d,[^\n]*"end":\{"revision":(\d+)\}' % rid)
    ok = drain(30, lambda b: pat.search(unpack(b)) is not None if (b'\x00' in b or b'"end"' in b) else False)
    ends = pat.findall(unpack(bytes(cap[before:])))
    if ends: since = int(ends[-1])
    for line in unpack(bytes(cap[before:])).split(b'\n'):
        if not line.startswith(b'M!{"id":%d,' % rid): continue
        d = json.loads(line[2:])
        for e in d['msg']['projectRead']['events']:
            r = e.get('probe', {}).get('event', {}).get('result', {})
            if 'output_frame' in r:
                for o in r['output_frame']['frame']['outputs']:
                    g = o.get('geometry', {})
                    if isinstance(g, dict) and 'changed' in g:
                        TMPL['msg']['projectRead']['request']['probes'][0]['output_frame']['geometry'] = {"if_changed":{"known":[{"node":o['node'],"revision":g['changed']['revision']}]}}
            if 'binding_graph' in r:
                st = r['binding_graph']['graph'].get('structure', {})
                if isinstance(st, dict) and 'changed' in st:
                    TMPL['msg']['projectRead']['request']['probes'][1]['binding_graph']['structure'] = {"if_changed":{"known":[{"revision":st['changed']['revision']}]}}
    open(cap_path, 'wb').write(cap)
    if k == 0:
        open(cap_path + '.first.txt', 'wb').write(unpack(bytes(cap[before:]))[:4000])
    print(f'request {k}: +{len(cap)-before} B ok={ok} since->{since}', flush=True)
    drain(0.5)
open(cap_path, 'wb').write(cap)
