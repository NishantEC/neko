"""Lines Rune's text elements up against the web's (see identity-texts.mjs).

    python3 -I identity-texts.py <web.json> <rune.tree.json> <x,y,w,h> [rows]

<x,y,w,h> is the content region's rect in Rune's window (points). For every text that appears once on each side it prints the
offset (Rune minus web) of its left/top edge and its width, so a spacing or size error shows as a number, not a feeling.
"""
import json
import sys

web = json.load(open(sys.argv[1]))
tree = json.load(open(sys.argv[2]))
ox, oy, rw_, rh_ = (float(v) for v in sys.argv[3].split(","))
rune = [e for e in tree["elements"] if e.get("role") == "text" and e.get("text", "").strip() and ox - 1 <= e["rect"][0] < ox + rw_ and oy - 1 <= e["rect"][1] < oy + rh_]
by_web = {}
for w in web:
    by_web.setdefault(w["text"], []).append(w)
by_rune = {}
for e in rune:
    t = " ".join(e["text"].split())
    by_rune.setdefault(t, []).append(e)
rows = []
for t, ws in by_web.items():
    es = by_rune.get(t)
    if not es:
        rows.append((ws[0]["y"], f"MISSING in Rune  {t[:50]!r}  web at ({ws[0]['x']:.0f},{ws[0]['y']:.0f}) {ws[0]['fs']}/{ws[0]['fw']}"))
        continue
    for w, e in zip(ws, es):
        rx, ry, rw, rh = e["rect"]
        rows.append((w["y"], f"dx {rx - ox - w['x']:+6.1f}  dy {ry + rh / 2 - oy - w['y'] - w['h'] / 2:+6.1f}  dw {rw - w['w']:+6.1f}  web {w['fs']}/{w['fw']} h{w['h']:.0f}  {t[:50]!r}"))
for t, es in by_rune.items():
    if t not in by_web and len(t) > 2:
        rows.append((es[0]["rect"][1] - oy, f"EXTRA in Rune    {t[:50]!r} at ({es[0]['rect'][0] - ox:.0f},{es[0]['rect'][1] - oy:.0f})"))
for _, line in sorted(rows, key=lambda r: r[0])[: int(sys.argv[4]) if len(sys.argv) > 4 else 60]:
    print(line)
