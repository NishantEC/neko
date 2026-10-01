#!/usr/bin/env node
// Resume the isolated proof session. Never prints OAuth URLs, tokens or state.
import fs from 'node:fs';
import path from 'node:path';
import net from 'node:net';
import {spawn, execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const data = process.env.NEKO_POC_DATA_DIR;
if (!data || !path.isAbsolute(data) || !fs.existsSync(path.join(data, 'neko.db'))) throw new Error('NEKO_POC_DATA_DIR must name an existing proof directory');
const operation = process.argv[2] || 'status';
function request(command) {
  return new Promise((resolve, reject) => {
    const socket = net.connect(path.join(data, 'neko.sock'));
    let bytes = Buffer.alloc(0);
    const timer = setTimeout(() => { socket.destroy(); reject(new Error('IPC deadline')); }, 10000);
    socket.on('error', () => { clearTimeout(timer); socket.destroy(); reject(new Error('IPC unavailable')); });
    socket.on('connect', () => {
      const payload = Buffer.from(JSON.stringify({Request: {id: 1, request: {Workbench: command}}}));
      const header = Buffer.alloc(4); header.writeUInt32LE(payload.length); socket.write(Buffer.concat([header, payload]));
    });
    socket.on('data', chunk => {
      bytes = Buffer.concat([bytes, chunk]);
      while (bytes.length >= 4 && bytes.length >= bytes.readUInt32LE(0) + 4) {
        const length = bytes.readUInt32LE(0);
        const frame = JSON.parse(bytes.subarray(4, length + 4)); bytes = bytes.subarray(length + 4);
        if (frame.Response?.id === 1) {
          clearTimeout(timer); socket.destroy();
          if (frame.Response.response.Workbench) resolve(frame.Response.response.Workbench);
          else reject(new Error(frame.Response.response.Error?.message || 'Daemon rejected command'));
        }
      }
    });
  });
}
if (operation === 'start') {
  let running = false;
  try { await request('Snapshot'); running = true; } catch {}
  if (!running) {
    const daemon = spawn(process.env.NEKO_TEST_DAEMON || path.join(root, 'target/debug/neko-daemon'), [], {detached: true, stdio: 'ignore', env: {...process.env, NEKO_DATA_DIR: data, NEKO_LEGACY_AGENTS: '0'}});
    daemon.unref();
    console.log(JSON.stringify({started_pid: daemon.pid, data_directory: data}));
    for (let i = 0; i < 100; i++) { try { await request('Snapshot'); break; } catch { await new Promise(resolve => setTimeout(resolve, 100)); } }
  }
}
let state = await request('Snapshot');
if (operation === 'create-fixture-task') {
  const fixture = fs.mkdtempSync(path.join(data, 'real-task-fixture-'));
  fs.writeFileSync(path.join(fixture, 'stats.mjs'), 'export function mean(values) {\n  return values.reduce((sum, value) => sum + value, 0) / values.length;\n}\n');
  fs.writeFileSync(path.join(fixture, 'stats.test.mjs'), 'import test from "node:test";\nimport assert from "node:assert/strict";\nimport { mean } from "./stats.mjs";\ntest("empty input has mean zero", () => assert.equal(mean([]), 0));\ntest("averages numbers without changing input", () => { const input = [2, 4, 6]; assert.equal(mean(input), 4); assert.deepEqual(input, [2, 4, 6]); });\n');
  execFileSync('git', ['-C', fixture, 'init', '--quiet']);
  execFileSync('git', ['-C', fixture, 'add', 'stats.mjs', 'stats.test.mjs']);
  execFileSync('git', ['-C', fixture, '-c', 'user.name=Neko Fixture', '-c', 'user.email=fixture@example.invalid', 'commit', '-qm', 'Seed empty mean regression']);
  let baselineFails = false;
  try { execFileSync(process.execPath, ['--test'], {cwd: fixture, stdio: 'pipe'}); } catch { baselineFails = true; }
  if (!baselineFails) throw new Error('Fixture must reproduce the edge-case failure');
  state = await request({SaveWorkspaceWithFolders: {workspace: {id: '', name: 'Real model lifecycle fixture', repository: fixture, instructions: 'This workspace is a disposable, isolated JavaScript fixture. Work only in this repository/task worktree. Fix only the requested function and test. Use node --test to validate. No external tools, network, installs, messages, publishing, or PRs. Preserve the original fixture checkout.', away_enabled: false}, folders: [fixture]}});
  const workspace = state.workspaces.find(w => w.repository === fs.realpathSync(fixture));
  if (!workspace) throw new Error('Fixture workspace missing');
  state = await request({CreateTask: {workspace_id: workspace.id, title: 'Fix empty mean in isolated fixture', goal: 'Fix mean([]) returning NaN: empty input must return 0. Preserve normal averages and input immutability. Only change stats.mjs, plus focused tests if needed. Run node --test and report actual output. Do not install packages, access network, use external tools, or modify any directory outside the isolated task worktree. This is a local proof; do not publish or create PRs.'}});
  const task = state.tasks.findLast(t => t.workspace_id === workspace.id);
  console.log(JSON.stringify({fixture, baseline_failed: baselineFails, workspace_id: workspace.id, task_id: task.id, title: task.title, status: task.status}, null, 2));
  process.exit(0);
}
if (operation === 'task-status') {
  const task = state.tasks.find(t => t.id === process.env.NEKO_POC_TASK_ID);
  if (!task) throw new Error('NEKO_POC_TASK_ID must select the proof task');
  console.log(JSON.stringify({id: task.id, title: task.title, status: task.status, plan: task.plan, result: task.result, worktree: task.worktree, events: task.events}, null, 2));
  process.exit(0);
}
if (operation === 'plan-existing-fixture') {
  const workspace = state.workspaces.findLast(w => w.name === 'Real model lifecycle fixture');
  if (!workspace || state.tasks.some(t => t.workspace_id === workspace.id)) throw new Error('Expected fresh fixture workspace');
  state = await request({CreateTask: {workspace_id: workspace.id, title: 'Fix empty mean in isolated fixture', goal: 'Fix mean([]) returning NaN: empty input must return 0. Preserve normal averages and input immutability. Only change stats.mjs, plus focused tests if needed. Run node --test and report actual output. Do not install packages, access network, use external tools, or modify any directory outside the isolated task worktree. This is a local proof; do not publish or create PRs.'}});
  const task = state.tasks.findLast(t => t.workspace_id === workspace.id);
  console.log(JSON.stringify({fixture: workspace.repository, workspace_id: workspace.id, task_id: task.id, title: task.title, status: task.status}, null, 2));
  process.exit(0);
}
const connection = state.mcp.connections.find(c => c.label === (process.env.NEKO_POC_CONNECTION || 'linear-personal'));
if (!connection) throw new Error('Proof connection not found');
if (operation === 'authenticate') state = await request({Mcp: {Authenticate: {connection_id: connection.id, client_id: null}}});
if (operation === 'discover') state = await request({Mcp: {Discover: {connection_id: connection.id}}});
if (operation === 'read-issues') {
  const tool = connection.tools.find(t => t.name === 'list_issues' && t.read_only);
  if (!tool || !connection.oauth || !connection.has_credentials) throw new Error('Authenticated read-only list_issues tool required');
  const workspace = state.workspaces.find(w => w.id === connection.workspace_id);
  if (!workspace || state.tasks.length || state.mcp.grants.length) throw new Error('Expected untouched isolated proof workspace');
  state = await request({Mcp: {SetWorkspaceToolGrant: {workspace_id: workspace.id, connection_id: connection.id, tool_name: tool.name, schema_hash: tool.schema_hash, allowed: true}}});
  let turn;
  try {
    state = await request({SendMessage: {workspace_id: workspace.id, text: 'This is a read-only integration proof. First list available Neko tools, then call the sole granted list_issues tool exactly once using {"limit":3,"includeArchived":false}. Do not use shell, write files, propose or create tickets, remember anything, send external messages, or call other tools. Return a short factual statement saying whether the lookup succeeded and how many issues it returned. Do not include issue titles or descriptions. Return no tickets and no memories in your response.'}});
    turn = state.conversation.findLast(m => m.pending)?.id;
    if (!turn) throw new Error('No pending chat turn');
    const deadline = Date.now() + 200000;
    while (Date.now() < deadline) {
      await new Promise(resolve => setTimeout(resolve, 1000));
      state = await request('Snapshot');
      const reply = state.conversation.find(m => m.id === turn);
      if (reply && !reply.pending) {
        const receipts = state.mcp.receipts.filter(r => r.connection_id === connection.id);
        console.log(JSON.stringify({proof: 'real-model-linear-read', turn_id: turn, failed: reply.failed, reply: reply.text, receipt_count: receipts.length, receipts: receipts.map(r => ({id: r.id, tool_name: r.tool_name, success: r.success, schema_hash: r.schema_hash})), tasks: state.tasks.length, runtime_writable: false}, null, 2));
        if (reply.failed || !receipts.some(r => r.success)) process.exitCode = 1;
        break;
      }
    }
    if (state.conversation.find(m => m.id === turn)?.pending) { await request({CancelChat: {turn_id: turn}}); throw new Error('Read-only chat deadline reached'); }
  } finally {
    state = await request({Mcp: {SetWorkspaceToolGrant: {workspace_id: workspace.id, connection_id: connection.id, tool_name: tool.name, schema_hash: tool.schema_hash, allowed: false}}});
  }
}
const current = state.mcp.connections.find(c => c.id === connection.id);
const endpoint = new URL(current.config.url);
console.log(JSON.stringify({connection_id: current.id, name: current.label, endpoint: endpoint.origin + endpoint.pathname, oauth: current.oauth, has_credentials: current.has_credentials, error: current.error, tools: current.tools.map(t => ({name: t.name, read_only: t.read_only})), tasks: state.tasks.length}, null, 2));
