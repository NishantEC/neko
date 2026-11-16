#!/bin/bash
# Headless snapshots of the shell with live rows from an isolated test daemon.
#
#   apps/neko-rune/scripts/snap-harness.sh [out-dir]     (default docs/design/rune)
#
# Starts the daemon's verification harness (`web_harness`: a real `server` over an isolated data
# folder under /tmp, seeded with the made-up workbench of scripts/fixtures/web-seed.mjs, no agents,
# no clipboard; never the person's own Neko), asks it for the workbench through the Rune client's
# own link (`neko-rune probe json`, read-only), and renders the `harness` scene in light and dark
# without opening a window. Then seeds one artifact of each viewer kind through the daemon (as an agent's
# SaveArtifact would) and renders the context panel's Files tab and Canvas from what the daemon says
# (`p3a-harness-*`). Then stops the harness it started, by its own pid.
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
repo="$(cd "$here/../.." && pwd)"
out="${1:-$repo/docs/design/rune}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/Library/Caches/neko/rune-target}"

if command -v "$HOME/.agents/bin/machine-ok" >/dev/null; then
  verdict="$("$HOME/.agents/bin/machine-ok" 2>&1 | head -n 1)"
  case "$verdict" in WAIT*) echo "machine says: $verdict. Try again later." >&2; exit 3;; esac
fi

# The harness is the daemon's own workspace (target/ beside the repo), the client is Rune's.
harness="${NEKO_WEB_HARNESS:-$repo/target/debug/web_harness}"
[[ -x "$harness" ]] || (cd "$repo" && env -u CARGO_TARGET_DIR cargo build --quiet -p neko-daemon --bin web_harness)
(cd "$here" && cargo build --quiet)
bin="$CARGO_TARGET_DIR/debug/neko-rune"

data="/tmp/neko-rune-harness-data-$$"
port="$(python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')"
seed="$data-seed.json"
web="$data-web"
rm -rf "$data" "$web"; mkdir -p "$data" "$web" "$out"
echo '<html><script src="/assets/x.js"></script></html>' > "$web/index.html"
(cd "$repo" && node --input-type=module -e 'import { webSeed } from "./scripts/fixtures/web-seed.mjs"; import fs from "node:fs"; fs.writeFileSync(process.argv[1], JSON.stringify(webSeed()));' "$seed")

( cd "$data" && exec env -i PATH="$PATH" HOME="$data" NEKO_DATA_DIR="$data" NEKO_WEB_PORT="$port" NEKO_WEB_DIR="$web" NEKO_WEB_SEED="$seed" "$harness" >"$data.log" 2>&1 ) &
pid=$!
cleanup() {
  # By pid, and only after confirming it is the harness this script started.
  if ps -p "$pid" -o command= 2>/dev/null | grep -q web_harness; then kill -TERM "$pid" 2>/dev/null || true; fi
  rm -rf "$data" "$web" "$seed" "$data.log" "$data-summary.json"
}
trap cleanup EXIT

for _ in $(seq 1 50); do [[ -S "$data/neko.sock" ]] && break; sleep 0.2; done
[[ -S "$data/neko.sock" ]] || { echo "the harness did not start: see $data.log" >&2; exit 1; }

summary="$data-summary.json"
NEKO_DATA_DIR="$data" NEKO_WEB_PORT="$port" "$bin" probe json > "$summary"
echo "workbench from the harness daemon: $(python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); print(len(d["tasks"]), "tasks")' "$summary")"
for theme in light dark; do
  NEKO_RUNE_SUMMARY="$summary" "$bin" --snap "$out/shell-$theme.png" --scene harness --theme "$theme" --size 1280x800 --scale 2 >/dev/null 2>&1
  echo "wrote $out/shell-$theme.png"
  # The session view, Neko column and runs, live from the same daemon: the app asks it itself,
  # on the calling thread (NEKO_RUNE_SOCKET), as the headless tests do.
  NEKO_RUNE_SOCKET="$data/neko.sock" "$bin" --snap "$out/harness-session-$theme.png" --scene default --theme "$theme" --size 1360x840 --scale 1 >/dev/null 2>&1
  echo "wrote $out/harness-session-$theme.png"
done

# Artifacts: a PNG, a CSV and a model, saved over the socket, then shown by the client the way an event shows them.
samples="$data/samples"; mkdir -p "$samples"
ids="$(python3 - "$data/neko.sock" "$samples" <<'PY'
import json, socket, struct, sys, zlib
sock_path, samples = sys.argv[1], sys.argv[2]
def png(w, h):
    raw = b"".join(b"\x00" + bytes(v for x in range(w) for v in (x * 255 // w, y * 255 // h, 150)) for y in range(h))
    chunk = lambda t, d: struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b"")
files = {"Board mockup": ("png", png(320, 200)), "Events": ("csv", ("id,name,amount\n" + "".join(f"{i},user-{i},{i % 977}.{i % 100:02}\n" for i in range(5000))).encode())}
out = []
for title, (kind, body) in files.items():
    path = f"{samples}/{title.replace(' ', '-')}.{kind}"
    open(path, "wb").write(body)
    s = socket.socket(socket.AF_UNIX); s.connect(sock_path)
    req = json.dumps({"Request": {"id": 1, "request": {"SaveArtifact": {"title": title, "file": path, "show": False}}}}).encode()
    s.sendall(struct.pack("<I", len(req)) + req)
    while True:
        n = struct.unpack("<I", s.recv(4))[0]; buf = b""
        while len(buf) < n: buf += s.recv(n - len(buf))
        frame = json.loads(buf)
        if "Response" in frame: out.append(frame["Response"]["response"]["ArtifactSaved"]["artifact"]["id"]); break
print(" ".join(out))
PY
)"
read -r png_id csv_id <<< "$ids"
for theme in light dark; do
  NEKO_DATA_DIR="$data" NEKO_RUNE_SOCKET="$data/neko.sock" NEKO_RUNE_OPEN_TASK="Stop the nightly export running twice" NEKO_RUNE_SHOW="$png_id" "$bin" --snap "$out/p3a-harness-canvas-$theme.png" --scene default --theme "$theme" --size 1360x840 --scale 1 --at 0.5 >/dev/null 2>&1
  echo "wrote $out/p3a-harness-canvas-$theme.png"
  NEKO_DATA_DIR="$data" NEKO_RUNE_SOCKET="$data/neko.sock" NEKO_RUNE_OPEN_TASK="Stop the nightly export running twice" NEKO_RUNE_SHOW="$csv_id" "$bin" --snap "$out/p3a-harness-table-$theme.png" --scene default --theme "$theme" --size 1360x840 --scale 1 --at 0.5 >/dev/null 2>&1
  echo "wrote $out/p3a-harness-table-$theme.png"
done
