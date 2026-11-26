#!/usr/bin/env python3
"""Writes THIRD_PARTY_LICENSES/crates.txt: every Rust crate that ships in Neko.app (both workspaces' normal and build
dependencies, macOS), its licence and the licence files it carries. Run after a dependency change:

    python3 scripts/licenses.py

Prints the licences found and fails if one is outside MIT, Apache-2.0, OFL and the permissive set already reviewed
(docs/knowledge/rune-ui-in-neko.md, "Licences").
"""
import json, os, subprocess, sys

root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
REVIEWED = ["MIT", "Apache-2.0", "Zlib", "BSD-2-Clause", "BSD-3-Clause", "ISC", "Unicode-3.0", "Unlicense", "0BSD", "CC0-1.0", "MIT-0", "MPL-2.0", "LLVM-exception"]
crates = {}
for ws in [root, os.path.join(root, "apps", "neko")]:
    meta = json.loads(subprocess.check_output(["cargo", "metadata", "--format-version", "1", "--filter-platform", "aarch64-apple-darwin"], cwd=ws))
    nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
    pkgs = {p["id"]: p for p in meta["packages"]}
    seen, stack = set(), list(meta["workspace_members"])
    while stack:
        x = stack.pop()
        if x in seen:
            continue
        seen.add(x)
        for d in nodes[x]["deps"]:
            if any(k["kind"] in (None, "build") for k in d["dep_kinds"]):
                stack.append(d["pkg"])
    for x in seen - set(meta["workspace_members"]):
        p = pkgs[x]
        if p.get("source") is None:
            continue  # Neko's own crates
        crates[(p["name"], p["version"])] = p

out, bad = [], []
for (name, version), p in sorted(crates.items()):
    lic = p.get("license") or "see licence file"
    words = lic.replace("(", " ").replace(")", " ").replace("/", " ").split()
    if not any(w in REVIEWED for w in words):
        bad.append(f"{name} {version}: {lic}")
    out.append(f"=== {name} {version} ({lic})")
    folder = os.path.dirname(p["manifest_path"])
    files = sorted(f for f in os.listdir(folder) if f.upper().startswith(("LICENSE", "LICENCE", "COPYING", "NOTICE")))
    for f in files:
        path = os.path.join(folder, f)
        if os.path.isfile(path):
            with open(path, encoding="utf-8", errors="replace") as fh:
                out.append(f"--- {f}\n{fh.read().strip()}")
    out.append("")
with open(os.path.join(root, "THIRD_PARTY_LICENSES", "crates.txt"), "w") as fh:
    fh.write("Rust crates in Neko.app, with their licences (written by scripts/licenses.py).\n\n" + "\n".join(out))
print(f"{len(crates)} crates")
if bad:
    print("Licences not reviewed:\n  " + "\n  ".join(bad), file=sys.stderr)
    sys.exit(1)
