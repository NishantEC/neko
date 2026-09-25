#!/usr/bin/env node
// Opt-in real model proof against one local MCP fixture; never a provider OAuth test.
// Usage: NEKO_SMOKE_LIVE=1 node scripts/smoke-mcp-live.mjs
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import net from 'node:net';
import { spawn, execFileSync } from 'node:child_process';
import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';

assert.equal(process.env.NEKO_SMOKE_LIVE, '1', 'Set NEKO_SMOKE_LIVE=1: this test invokes real Codex and uses account quota.');
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const codex = '/opt/homebrew/bin/codex';
fs.accessSync(codex, fs.constants.X_OK);
const scratch = fs.mkdtempSync(path.join(os.tmpdir(), 'neko-mcp-live-'));
const repo = path.join(scratch, 'repo');
const data = path.join(scratch, 'data');
fs.mkdirSync(repo); fs.mkdirSync(data);
execFileSync('git', ['-C', repo, 'init', '--quiet']);
execFileSync('git', ['-C', repo, '-c', 'user.name=Neko Test', '-c', 'user.email=test@example.invalid', 'commit', '--allow-empty', '-qm', 'local MCP proof']);
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
let socket; let daemon; let responsibilityId; let nextId = 1; let buffered = Buffer.alloc(0);
const pending = new Map();
let outcome = { passed: false, codex, scratch, boundary: 'Real Codex agent + local fixture MCP + actual daemon bridge; no provider OAuth or live external service.' };

async function connect() {
  const deadline = Date.now() + 20000;
  while (Date.now() < deadline) {
    if (daemon.exitCode !== null) throw new Error('Test daemon exited before listening');
    try {
      socket = await new Promise((resolve, reject) => {
        const client = net.connect(path.join(data, 'neko.sock'));
        client.once('connect', () => resolve(client));
        client.once('error', error => { client.destroy(); reject(error); });
      });
      socket.on('data', chunk => {
        buffered = Buffer.concat([buffered, chunk]);
        while (buffered.length >= 4 && buffered.length >= buffered.readUInt32LE(0) + 4) {
          const length = buffered.readUInt32LE(0);
          const frame = JSON.parse(buffered.subarray(4, length + 4));
          buffered = buffered.subarray(length + 4);
          if (frame.Response) {
            pending.get(frame.Response.id)?.(frame.Response.response);
            pending.delete(frame.Response.id);
          }
        }
      });
      return;
    } catch { await sleep(100); }
  }
  throw new Error('Test daemon did not listen within 20 seconds');
}
function request(command) {
  const id = nextId++;
  const bytes = Buffer.from(JSON.stringify({ Request: { id, request: { Workbench: command } } }));
  const header = Buffer.alloc(4); header.writeUInt32LE(bytes.length);
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => { pending.delete(id); reject(new Error('Test IPC deadline exceeded')); }, 5000);
    pending.set(id, response => {
      clearTimeout(timer);
      if (response.Workbench) resolve(response.Workbench);
      else reject(new Error(response.Error?.message ?? 'Unexpected test IPC response'));
    });
    socket.write(Buffer.concat([header, bytes]));
  });
}
const mcp = command => request({ Mcp: command });

try {
  daemon = spawn(path.join(root, 'target/debug/neko-daemon'), [], {
    env: { ...process.env, NEKO_DATA_DIR: data, NEKO_CODEX_PATH: codex }, stdio: ['ignore', 'ignore', 'ignore'],
  });
  await connect();
  const created = await request({ SaveWorkspace: { workspace: {
    id: '', name: 'Real model local MCP proof', repository: repo, away_enabled: false,
    instructions: 'Observation-only verification. Do not edit any files, execute shell commands, install anything, publish, send messages, or contact external providers. Use only the configured local fixture through the Neko MCP bridge. Do not prepare fixes or queue tasks.',
  } } });
  const workspaceId = created.workspaces[0].id;
  const added = await mcp({ AddConnection: {
    workspace_id: workspaceId, label: 'Local observation fixture', trust_local_process: true, credentials: null,
    config: { transport: 'stdio', command: process.execPath, args: [path.join(root, 'scripts/fixtures/mcp-workbench.mjs'), 'alpha'] },
  } });
  const connectionId = added.mcp.connections[0].id;
  const discovered = await mcp({ Discover: { connection_id: connectionId } });
  const connection = discovered.mcp.connections.find(c => c.id === connectionId);
  assert.equal(connection.error, null);
  assert.equal(connection.tools.length, 1);
  const tool = connection.tools[0];
  await mcp({ SetToolGrant: { connection_id: connectionId, tool_name: tool.name, schema_hash: tool.schema_hash, allowed: true } });
  const registered = await mcp({ SaveResponsibility: { responsibility: {
    id: '', workspace_id: workspaceId, connection_ids: [connectionId], enabled: true, prepare_low_risk: false,
    next_due_ms: 0, last_attempt_ms: null, last_result: '', failures: 0,
    instruction: 'This is an observation-only runtime proof, not a request to work on bugs. First call neko_list_tools. Then call the sole granted assigned_changes tool exactly once through neko_call_tool with arguments {}. Return both fixture records using their exact external_id, revision, title, and description, with the real receipt ID from that call. Set eligible=false for BOTH records regardless of assigned/actionable fields, because no bug work is requested or authorized in this observation-only responsibility. Return only the required WakeResult JSON with a concise factual summary. Do not edit files, run shell commands, install software, create tasks, publish, or contact any other provider. Do not invent observations or receipts.',
  } } });
  responsibilityId = registered.mcp.responsibilities[0].id;
  const start = Date.now(); const deadline = start + 120000;
  let completed;
  while (Date.now() < deadline) {
    const state = await request('Snapshot');
    const responsibility = state.mcp.responsibilities.find(r => r.id === responsibilityId);
    if (responsibility.failures > 0) throw new Error(`Single wake failed: ${responsibility.last_result}`);
    if (responsibility.last_attempt_ms !== null && responsibility.last_result && responsibility.last_result !== 'Checking this responsibility…') {
      completed = state; break;
    }
    await sleep(250);
  }
  assert.ok(completed, 'Real model wake did not complete within 120 seconds; no retries attempted');
  const responsibility = completed.mcp.responsibilities.find(r => r.id === responsibilityId);
  const receipts = completed.mcp.receipts.filter(r => r.connection_id === connectionId);
  assert.equal(receipts.length, 1, 'Expected exactly one real upstream call receipt');
  assert.equal(receipts[0].success, true);
  assert.equal(receipts[0].workspace_id, workspaceId);
  assert.equal(receipts[0].tool_name, 'assigned_changes');
  assert.equal(completed.mcp.sources.length, 2, 'WakeResult must contain both real observations');
  assert.ok(completed.mcp.sources.every(source => source.eligible === false && source.task_id === null && source.receipt_ids.includes(receipts[0].id)));
  assert.equal(completed.tasks.length, 0, 'Observation-only wake queued work');
  assert.equal(execFileSync('git', ['-C', repo, 'status', '--porcelain'], { encoding: 'utf8' }), '');
  outcome = { ...outcome, passed: true, elapsed_ms: Date.now() - start, summary: responsibility.last_result, receipt_id: receipts[0].id, observations: completed.mcp.sources.length, tasks: completed.tasks.length };
} catch (error) {
  outcome = { ...outcome, error: error.message };
  process.exitCode = 1;
} finally {
  if (responsibilityId && socket && !socket.destroyed) {
    try {
      const state = await request('Snapshot');
      const responsibility = state.mcp.responsibilities.find(r => r.id === responsibilityId);
      await mcp({ SaveResponsibility: { responsibility: { ...responsibility, enabled: false } } });
      outcome.responsibility_paused = true;
      // Allow the daemon's authority watchdog to cancel an in-flight wake.
      await sleep(250);
    } catch (error) { outcome.cleanup_error = error.message; process.exitCode = 1; }
  }
  socket?.destroy();
  if (daemon && daemon.exitCode === null) {
    await new Promise(resolve => {
      const timer = setTimeout(() => { daemon.kill('SIGKILL'); }, 2000);
      daemon.once('exit', () => { clearTimeout(timer); resolve(); });
      daemon.kill('SIGTERM');
    });
  }
  console.log(JSON.stringify(outcome, null, 2));
}
