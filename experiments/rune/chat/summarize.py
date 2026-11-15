#!/usr/bin/env python3
"""Print a results/*.json from the chat bench as a table: python3 -I summarize.py results/bench-2.json"""
import json
import sys

d = json.load(open(sys.argv[1]))
print(f"run: {d['run']}")
print(f"{'phase':<20}{'drawn':>7}{'unch':>6}{'int p50':>9}{'p95':>8}{'p99':>8}{'max':>8}{'upd p50':>9}{'p95':>7}{'max':>7}{'enc p50':>9}{'p95':>7}{'>1.5x60':>9}")
for k, v in d["phases"].items():
    i, u, e = v["interval_ms"], v["update_ms"], v["encode_ms"]
    print(f"{k:<20}{v['drawn']:>7}{v['unchanged']:>6}{i['p50']:>9.2f}{i['p95']:>8.2f}{i['p99']:>8.2f}{i['max']:>8.1f}{u['p50']:>9.2f}{u['p95']:>7.2f}{u['max']:>7.2f}{e['p50']:>9.2f}{e['p95']:>7.2f}{v['intervals_over_1.5x_60hz']:>9}")
print("memory (MB):")
for m in d["memory_mb"]:
    print(f"  {m['at']:<16} footprint {m['footprint']:>7.1f}  resident {m['resident']:>7.1f}")
