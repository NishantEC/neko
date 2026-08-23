// neko's wire protocol, in JavaScript.
//
// **The daemon does not know or care that this client is not Rust.** It speaks
// length-prefixed JSON over a Unix socket (`neko-protocol`), which is exactly
// why swapping the client is a real option and swapping the daemon would not
// be: the search ranking, the app index, SQLite, the clipboard capture loop and
// every provider stay where they are, untouched.
//
// Framing, from `neko-protocol/src/lib.rs`: a `u32` **little-endian** byte
// length followed by that many bytes of JSON. Nothing else.

import net from "node:net";
import os from "node:os";
import path from "node:path";

export const SOCKET_PATH = path.join(
  os.homedir(),
  "Library/Application Support/neko/neko.sock",
);

export class NekoClient {
  #socket = null;
  #buffer = Buffer.alloc(0);
  #nextId = 1;
  /** id -> { resolve, onPartial } */
  #pending = new Map();

  connect() {
    return new Promise((resolve, reject) => {
      const socket = net.createConnection(SOCKET_PATH);
      socket.once("connect", () => {
        this.#socket = socket;
        resolve();
      });
      socket.once("error", reject);
      socket.on("data", (chunk) => this.#onData(chunk));
    });
  }

  #onData(chunk) {
    this.#buffer = Buffer.concat([this.#buffer, chunk]);
    // A frame can arrive split across reads, or several can arrive in one —
    // neither is an error, so drain everything complete and keep the rest.
    for (;;) {
      if (this.#buffer.length < 4) return;
      const length = this.#buffer.readUInt32LE(0);
      if (this.#buffer.length < 4 + length) return;
      const payload = this.#buffer.subarray(4, 4 + length);
      this.#buffer = this.#buffer.subarray(4 + length);
      this.#dispatch(JSON.parse(payload.toString("utf8")));
    }
  }

  #dispatch(frame) {
    // `Event` frames are broadcasts with no request id — ignored for now; a
    // real client wants them for live icon and theme changes.
    if (!frame.Response) return;
    const { id, response } = frame.Response;
    const waiting = this.#pending.get(id);
    if (!waiting) return;

    // **A search is answered by up to two frames**, not one: the fast
    // providers land immediately with `complete: false`, then the whole set
    // once file search returns. See `AGENTS.md`, "Two-phase search".
    const results = response.SearchResults;
    if (results && results.complete === false) {
      waiting.onPartial?.(results.items);
      return;
    }
    this.#pending.delete(id);
    waiting.resolve(response);
  }

  request(request, onPartial) {
    const id = this.#nextId++;
    const payload = Buffer.from(JSON.stringify({ Request: { id, request } }), "utf8");
    const header = Buffer.alloc(4);
    header.writeUInt32LE(payload.length, 0);
    this.#socket.write(Buffer.concat([header, payload]));
    return new Promise((resolve) => this.#pending.set(id, { resolve, onPartial }));
  }

  search(query, { limit = 8, provider = null, onPartial } = {}) {
    return this.request({ Search: { query, limit, provider } }, onPartial);
  }

  activate(kind, id, { action = null, query = "" } = {}) {
    return this.request({ Activate: { kind, id, action, query } });
  }
}
