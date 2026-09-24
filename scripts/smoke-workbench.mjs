#!/usr/bin/env node
// Real daemon + SQLite + IPC + worktree + child supervision. AI is a fixture.
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import net from 'node:net';
import { spawn, execFileSync } from 'node:child_process';
import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const scratch = fs.mkdtempSync(path.join(os.tmpdir(), 'neko-workbench-smoke-'));
const repo = path.join(scratch, 'repo');
const data = path.join(scratch, 'data');
fs.mkdirSync(repo); fs.mkdirSync(data);
const git = (...args) => execFileSync('git', ['-C', repo, ...args], { encoding: 'utf8' });
git('init', '--quiet');
git('-c', 'user.name=Neko Test', '-c', 'user.email=test@example.invalid', 'commit', '--allow-empty', '-qm', 'fixture');
const fixture = path.join(root, 'scripts/fixtures/codex-workbench.mjs');
// Explicit opt-in only: this invokes an authenticated model and may use quota.
const live = process.env.NEKO_SMOKE_LIVE === '1';
let daemon; let socket; let nextId = 1; let buffered = Buffer.alloc(0);
const pending = new Map();
let diagnostics = '';
function launch() {
  daemon = spawn(path.join(root, 'target/debug/neko-daemon'), [], {
    env: { ...process.env, NEKO_DATA_DIR: data, ...(live ? {} : { NEKO_CODEX_PATH: fixture }) },
    stdio: ['ignore', 'ignore', 'pipe'],
  });
  daemon.stderr.on('data', chunk => { diagnostics = (diagnostics + chunk).slice(-8000); });
}
async function connect() {
  for (let n = 0; n < (live ? 3000 : 200); n++) {
    try {
      socket = await new Promise((resolve, reject) => {
        const client = net.connect(path.join(data, 'neko.sock'));
        client.once('connect', () => resolve(client)); client.once('error', reject);
      });
      buffered = Buffer.alloc(0);
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
    } catch { await new Promise(resolve => setTimeout(resolve, 100)); }
  }
  throw new Error(`Daemon did not listen: ${diagnostics}`);
}
function request(request) {
  const id = nextId++;
  const bytes = Buffer.from(JSON.stringify({ Request: { id, request } }));
  const header = Buffer.alloc(4); header.writeUInt32LE(bytes.length);
  return new Promise((resolve, reject) => {
    const timeout = setTimeout(() => { pending.delete(id); reject(new Error('IPC timeout')); }, 15000);
    pending.set(id, value => { clearTimeout(timeout); resolve(value); });
    socket.write(Buffer.concat([header, bytes]));
  });
}
async function command(command) {
  const response = await request({ Workbench: command });
  assert.ok(response.Workbench, JSON.stringify(response));
  return response.Workbench;
}
async function waitFor(id, status) {
  for (let n = 0; n < (live ? 3000 : 600); n++) {
    const snapshot = await command('Snapshot');
    const task = snapshot.tasks.find(task => task.id === id);
    if (task.status === status) return task;
    assert.notEqual(task.status, 'Failed', JSON.stringify(task.events));
    await new Promise(resolve => setTimeout(resolve, 100));
  }
  throw new Error(`Task did not reach ${status}`);
}
async function waitSnapshot(predicate, description) {
  for (let n = 0; n < 600; n++) {
    const snapshot = await command('Snapshot');
    if (predicate(snapshot)) return snapshot;
    await new Promise(resolve => setTimeout(resolve, 100));
  }
  const snapshot = await command('Snapshot');
  throw new Error(`${description}: ${JSON.stringify(snapshot.mcp)}; daemon: ${diagnostics}`);
}
const mcp = commandValue => command({ Mcp: commandValue });
async function stop() {
  socket?.destroy();
  if (daemon && daemon.exitCode === null) {
    daemon.kill('SIGTERM');
    await new Promise(resolve => daemon.once('exit', resolve));
  }
}
try {
  launch(); await connect();
  assert.equal(fs.statSync(path.join(data, 'neko.sock')).mode & 0o777, 0o600);
  const created = await command({ SaveWorkspace: { workspace: {
    id: '', name: 'Smoke workspace', repository: repo, instructions: 'Only this disposable test repository. Do not access outside files, use network, or publish anything.', away_enabled: false,
  } } });
  const workspaceId = created.workspaces[0].id;
  const queued = await command({ CreateTask: { workspace_id: workspaceId, title: 'Isolated smoke task', goal: 'Create exactly one file neko-smoke.txt containing exactly Isolated task output followed by a newline. Verify its bytes locally. No other changes or network access.' } });
  const taskId = queued.tasks[0].id;
  const plan = await waitFor(taskId, 'AwaitingApproval');
  assert.ok(plan.plan.length > 0);
  assert.equal(fs.existsSync(path.join(repo, 'neko-smoke.txt')), false);
  assert.equal(fs.existsSync(path.join(plan.worktree, 'neko-smoke.txt')), false);
  await command({ ApproveTask: { task_id: taskId } });
  const ready = await waitFor(taskId, 'ReadyForReview');
  assert.match(ready.result, /Independent review/);
  assert.equal(fs.readFileSync(path.join(ready.worktree, 'neko-smoke.txt'), 'utf8'), 'Isolated task output\n');
  assert.equal(fs.existsSync(path.join(repo, 'neko-smoke.txt')), false);
  const search = await request({ Search: { query: 'Isolated smoke', limit: 10, provider: 'neko-task' } });
  assert.equal(search.SearchResults.items[0].id, taskId);
  const denied = await request({ Workbench: { ApproveTask: { task_id: taskId } } });
  assert.ok(denied.Error);
  const acknowledged = await command({ CompleteTask: { task_id: taskId } });
  assert.equal(acknowledged.tasks.find(task => task.id === taskId).status, 'Completed');
  const cancelled = await command({ CreateTask: { workspace_id: workspaceId, title: 'Cancel fixture', goal: 'Must not build.' } });
  const cancelId = cancelled.tasks.at(-1).id;
  await command({ CancelTask: { task_id: cancelId } });
  const retried = await command({ RetryTask: { task_id: cancelId } });
  assert.equal(retried.tasks.find(task => task.id === cancelId).status, 'Queued');
  await command({ CancelTask: { task_id: cancelId } });
  await stop(); launch(); await connect();
  const restored = await command('Snapshot');
  assert.equal(restored.tasks.find(task => task.id === taskId).status, 'Completed');
  assert.equal(restored.tasks.find(task => task.id === cancelId).status, 'Cancelled');
  // Configure everything through user-facing IPC. No direct database seeding.
  if (!live) {
    await command({ SaveWorkspace: { workspace: { ...restored.workspaces.find(w => w.id === workspaceId), away_enabled: true } } });
    const otherRepo = path.join(scratch, 'other-repo'); fs.mkdirSync(otherRepo);
    execFileSync('git', ['-C', otherRepo, 'init', '--quiet']);
    execFileSync('git', ['-C', otherRepo, '-c', 'user.name=Neko Test', '-c', 'user.email=test@example.invalid', 'commit', '--allow-empty', '-qm', 'fixture']);
    const second = await command({ SaveWorkspace: { workspace: { id: '', name: 'Other workspace', repository: otherRepo, instructions: 'Disposable scope-isolation fixture.', away_enabled: false } } });
    const otherWorkspaceId = second.workspaces.find(w => w.id !== workspaceId).id;
    const fixtureServer = path.join(root, 'scripts/fixtures/mcp-workbench.mjs');
    async function addConnection(workspace_id, label, namespace) {
      const added = await mcp({ AddConnection: { workspace_id, label, config: { transport: 'stdio', command: process.execPath, args: [fixtureServer, namespace] }, trust_local_process: true, credentials: null } });
      const connection = added.mcp.connections.find(c => c.label === label);
      const discovered = await mcp({ Discover: { connection_id: connection.id } });
      const ready = discovered.mcp.connections.find(c => c.id === connection.id);
      assert.equal(ready.error, null, JSON.stringify(ready));
      assert.equal(ready.tools.length, 1);
      assert.ok(!discovered.mcp.grants.some(g => g.connection_id === ready.id), 'Discovery must not grant tools');
      await mcp({ SetToolGrant: { connection_id: ready.id, tool_name: ready.tools[0].name, schema_hash: ready.tools[0].schema_hash, allowed: true } });
      return ready;
    }
    const own = await addConnection(workspaceId, 'Workspace A fixture', 'alpha');
    const foreign = await addConnection(otherWorkspaceId, 'Workspace B fixture', 'beta');
    const responsibility = { id: '', workspace_id: workspaceId, instruction: `Watch assigned actionable fixture bugs and prepare isolated low-risk fixes. FOREIGN_CONNECTION_ID=${foreign.id}`, connection_ids: [own.id], enabled: true, prepare_low_risk: true, next_due_ms: 0, last_attempt_ms: null, last_result: '', failures: 0 };
    const crossScope = await request({ Workbench: { Mcp: { SaveResponsibility: { responsibility: { ...responsibility, connection_ids: [foreign.id] } } } } });
    assert.ok(crossScope.Error, 'Cross-workspace responsibility must fail');
    const registered = await mcp({ SaveResponsibility: { responsibility } });
    const responsibilityId = registered.mcp.responsibilities.at(-1).id;
    const observed = await waitSnapshot(s => s.mcp.sources.filter(source => source.responsibility_id === responsibilityId).length === 2, 'Responsibility did not observe fixture sources');
    const lowId = observed.mcp.sources.find(s => s.title === 'Low-risk fixture').task_id;
    const sensitiveId = observed.mcp.sources.find(s => s.title === 'Sensitive fixture').task_id;
    assert.ok(lowId && sensitiveId);
    assert.ok(observed.mcp.receipts.every(receipt => receipt.workspace_id === workspaceId && receipt.connection_id === own.id && receipt.success));
    assert.match(observed.mcp.responsibilities.find(r => r.id === responsibilityId).last_result, /foreign scope denied/);
    const autoReady = await waitFor(lowId, 'ReadyForReview');
    assert.equal(autoReady.supervision.risk, 'low');
    assert.ok(autoReady.events.some(e => e.message.includes('Standing responsibility authorized')));
    assert.equal(fs.readFileSync(path.join(autoReady.worktree, 'neko-smoke.txt'), 'utf8'), 'Isolated task output\n');
    await waitFor(sensitiveId, 'AwaitingApproval');
    const manual = await command({ CreateTask: { workspace_id: workspaceId, title: 'Manual task under Away', goal: 'Still needs approval.' } });
    const manualId = manual.tasks.at(-1).id;
    await waitFor(manualId, 'AwaitingApproval');
    await new Promise(resolve => setTimeout(resolve, 2500));
    const held = await command('Snapshot');
    for (const id of [sensitiveId, manualId]) {
      const task = held.tasks.find(t => t.id === id);
      assert.equal(task.status, 'AwaitingApproval');
      assert.equal(fs.existsSync(path.join(task.worktree, 'neko-smoke.txt')), false);
    }
    assert.equal(fs.existsSync(path.join(repo, 'neko-smoke.txt')), false);
    assert.equal(fs.existsSync(path.join(otherRepo, 'neko-smoke.txt')), false);
    const beforeWake = await command('Snapshot');
    const receiptCount = beforeWake.mcp.receipts.length;
    await mcp({ Wake: { responsibility_id: responsibilityId } });
    const repeated = await waitSnapshot(s => s.mcp.receipts.length > receiptCount && s.mcp.sources.every(source => source.retrieved_ms > beforeWake.mcp.sources.find(old => old.id === source.id).retrieved_ms), 'Second wake did not retrieve fresh evidence');
    assert.equal(repeated.tasks.length, beforeWake.tasks.length, 'Unchanged sources created duplicate tasks');
    assert.equal(repeated.mcp.sources.length, 2);
    const currentResponsibility = repeated.mcp.responsibilities.find(r => r.id === responsibilityId);
    await mcp({ SaveResponsibility: { responsibility: { ...currentResponsibility, enabled: false } } });
    const paused = await request({ Workbench: { Mcp: { Wake: { responsibility_id: responsibilityId } } } });
    assert.ok(paused.Error, 'Paused responsibility must reject wake');
    await mcp({ SetToolGrant: { connection_id: own.id, tool_name: own.tools[0].name, schema_hash: own.tools[0].schema_hash, allowed: false } });
    await mcp({ SaveResponsibility: { responsibility: { ...currentResponsibility, enabled: true } } });
    await mcp({ Wake: { responsibility_id: responsibilityId } });
    const revoked = await waitSnapshot(s => /no granted tools|Grant at least one MCP tool/.test(s.mcp.responsibilities.find(r => r.id === responsibilityId).last_result), 'Revoked tool remained callable');
    assert.equal(revoked.mcp.receipts.length, repeated.mcp.receipts.length);
    assert.equal(revoked.tasks.length, repeated.tasks.length);
    await mcp({ SaveResponsibility: { responsibility: { ...revoked.mcp.responsibilities.find(r => r.id === responsibilityId), enabled: false } } });
    await mcp({ SetEnabled: { connection_id: own.id, enabled: false } });
    await stop(); launch(); await connect();
    const durable = await command('Snapshot');
    assert.equal(durable.tasks.find(t => t.id === lowId).supervision.risk, 'low');
    assert.equal(durable.tasks.find(t => t.id === sensitiveId).status, 'AwaitingApproval');
    assert.equal(durable.mcp.responsibilities.find(r => r.id === responsibilityId).enabled, false);
    assert.equal(durable.mcp.connections.find(c => c.id === own.id).enabled, false);
    assert.ok(!durable.mcp.grants.some(g => g.connection_id === own.id));
    assert.equal(durable.mcp.sources.length, 2);
    assert.equal(durable.mcp.receipts.length, repeated.mcp.receipts.length);
  }
  console.log(JSON.stringify({ passed: true, agent: live ? 'live Codex CLI' : 'deterministic fixture', scratch, taskId, checks: ['real IPC', 'private socket', 'durable storage', 'read-only plan', 'approval gate', 'isolated build', 'independent review', 'palette task', 'invalid approval', 'cancellation', 'daemon restart', ...(!live ? ['user-added generic MCPs', 'explicit schema grants', 'real stdio bridge and receipts', 'workspace scope rejection', 'low-risk standing delegation', 'sensitive work held', 'manual task held under Away', 'wake deduplication', 'pause and revoke', 'durable MCP policy'] : [])] }, null, 2));
} finally { await stop(); }
