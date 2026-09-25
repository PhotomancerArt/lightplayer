"""Board->host wire messages of a tap, in order, as `kind<TAB>json` lines.

usage: extract.py <session.tap> > session.jsonl
Uses scripts/wire-tap/tapstat.py's reader and its kind labels (lens/card/sync).
"""
import os, sys
sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..', '..', 'scripts', 'wire-tap'))
import tapstat as T

recs = T.read_records(sys.argv[1])
lines = T.reassemble_lines(recs)
labels = T.read_labels(lines)
for us, d, line, wire in lines:
    if d != '<' or not line.startswith(b'M!'):
        continue
    m, raw = T.parse_message(line)
    if m is None:
        continue
    sys.stdout.write(T.kind_of(line, labels) + '\t' + raw.decode() + '\n')
