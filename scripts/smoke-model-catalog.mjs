#!/usr/bin/env node
// Model catalog smoke test over real daemon IPC, in an isolated data folder.
// Discovery is metadata only. NEKO_SMOKE_LIVE=1 also runs two model checks,
// each one short reply through the real runner, which may use quota.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import net from 'node:net';
import os from 'node:os';
import path from 'node:path';

const root = path.resolve(path.dirname(new URL(import.meta.url).pathname), '..');
const binary = process.env.NEKO_DAEMON || path.join(root, 'target/debug/neko-daemon');
const data = mkdtempSync(path.join(os.tmpdir(), 'neko-models-'));
const live = process.env.NEKO_SMOKE_LIVE === '1';
const daemon = spawn(binary, [], { env: { ...process.env, NEKO_DATA_DIR: data }, stdio: ['ignore', 'ignore', 'pipe'] });
let diagnostics = '';
daemon.stderr.on('data', chunk => { diagnostics = (diagnostics + chunk).slice(-4000); });
let socket; let buffered = Buffer.alloc(0); let nextId = 1; const pending = new Map();

for (let n = 0; n < 200 && !socket; n++) {
  try {
    socket = await new Promise((resolve, reject) => {
      const client = net.connect(path.join(data, 'neko.sock'));
      client.once('connect', () => resolve(client)); client.once('error', reject);
    });
  } catch { await new Promise(resolve => setTimeout(resolve, 100)); }
}
if (!socket) throw new Error('Daemon did not listen: ' + diagnostics);
socket.on('data', chunk => {
  buffered = Buffer.concat([buffered, chunk]);
  while (buffered.length >= 4 && buffered.length >= buffered.readUInt32LE(0) + 4) {
    const length = buffered.readUInt32LE(0);
    const frame = JSON.parse(buffered.subarray(4, length + 4));
    buffered = buffered.subarray(length + 4);
    if (frame.Response) { pending.get(frame.Response.id)?.(frame.Response.response); pending.delete(frame.Response.id); }
  }
});
function request(body, timeoutMs = 30000) {
  const id = nextId++;
  const bytes = Buffer.from(JSON.stringify({ Request: { id, request: body } }));
  const header = Buffer.alloc(4); header.writeUInt32LE(bytes.length);
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => { pending.delete(id); reject(new Error('IPC timeout')); }, timeoutMs);
    pending.set(id, value => { clearTimeout(timer); resolve(value); });
    socket.write(Buffer.concat([header, bytes]));
  });
}

try {
  let started = Date.now();
  const { AgentModels: catalog } = await request({ AgentModels: { refresh: true } });
  assert.deepEqual(catalog.sources.map(s => s.provider), ['codex', 'ollama', 'lmstudio']);
  for (const source of catalog.sources) console.log(source.label.padEnd(10), source.status.padEnd(16), source.connection, '·', source.models.length, 'models', source.default_model ? 'default ' + source.default_model : '');
  console.log('catalog read in', Date.now() - started, 'ms');
  started = Date.now();
  await request({ AgentModels: { refresh: false } });
  assert.ok(Date.now() - started < 200, 'cached catalog should answer immediately');

  if (live) {
    const codex = catalog.sources.find(s => s.provider === 'codex');
    assert.equal(codex.status, 'ready', 'Codex must be signed in for the live check');
    const model = codex.default_model;
    started = Date.now();
    const good = (await request({ CheckAgentModel: { runtime: { provider: 'codex', model }, save: true } }, 130000)).AgentModelCheck;
    console.log('check', model, '->', good.ok, good.message, Date.now() - started, 'ms');
    assert.equal(good.ok, true);
    let snapshot = (await request({ Workbench: 'Snapshot' })).Workbench;
    assert.deepEqual(snapshot.agent_runtime, { provider: 'codex', model });
    const bad = (await request({ CheckAgentModel: { runtime: { provider: 'codex', model: 'neko-no-such-model' }, save: true } }, 130000)).AgentModelCheck;
    console.log('check neko-no-such-model ->', bad.ok, bad.unavailable, bad.message);
    assert.equal(bad.ok, false);
    snapshot = (await request({ Workbench: 'Snapshot' })).Workbench;
    assert.deepEqual(snapshot.agent_runtime, { provider: 'codex', model }, 'a failed check must keep the previous model');
    const marked = (await request({ AgentModels: { refresh: false } })).AgentModels.sources[0].models.find(m => m.id === model);
    assert.equal(marked.access, 'checked');
  }
  console.log('model catalog smoke passed' + (live ? ' (live checks included)' : ''));
} finally {
  socket.destroy(); daemon.kill();
  rmSync(data, { recursive: true, force: true });
}

