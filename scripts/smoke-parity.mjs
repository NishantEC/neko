#!/usr/bin/env node
// Real daemon IPC, isolated data folder, no model calls: diagnostics, memory
// chat verbs (handled in code), secret refusal, stop-all and ticket history.
import assert from 'node:assert/strict';
import { spawn, execFileSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, rmSync } from 'node:fs';
import net from 'node:net';
import os from 'node:os';
import path from 'node:path';

const root = path.resolve(path.dirname(new URL(import.meta.url).pathname), '..');
const binary = process.env.NEKO_DAEMON || path.join(root, 'target/debug/neko-daemon');
const data = mkdtempSync(path.join(os.tmpdir(), 'neko-parity-'));
const repo = path.join(data, 'repo');
mkdirSync(repo);
execFileSync('git', ['init', '-q', repo]);
execFileSync('git', ['-C', repo, '-c', 'user.email=t@t', '-c', 'user.name=t', 'commit', '-q', '--allow-empty', '-m', 'init']);
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
async function command(body) {
  const response = await request({ Workbench: body });
  assert.ok(response.Workbench, JSON.stringify(response));
  return response.Workbench;
}
async function say(text) {
  const before = (await command('Snapshot')).conversation.length;
  await command({ SendMessage: { text, workspace_id: null } });
  for (let n = 0; n < 100; n++) {
    const snapshot = await command('Snapshot');
    const reply = snapshot.conversation.slice(before).find(m => String(m.role).toLowerCase() !== 'user' && !m.pending);
    if (reply) return { reply: reply.text, snapshot };
    await new Promise(resolve => setTimeout(resolve, 50));
  }
  throw new Error('No reply to ' + text);
}

try {
  const report = (await request('Diagnostics')).Diagnostics;
  for (const check of report.checks) console.log(check.ok ? 'ok  ' : 'warn', check.name.padEnd(26), String(check.millis).padStart(5), 'ms ', check.detail);
  assert.ok(report.checks.some(c => c.name === 'Codex CLI'));
  const malformed = await request({ Workbench: { NoSuchCommand: {} } }, 5000);
  assert.ok(malformed.Error, 'a malformed request is answered, not dropped');
  console.log('malformed ->', malformed.Error.message.slice(0, 80));

  let started = Date.now();
  let reply, snapshot;
  if (!process.env.SKIP_CHAT) {
  ({ reply, snapshot } = await say('remember that tests in this repo use pnpm'));
  console.log('remember ->', reply, Date.now() - started, 'ms');
  assert.match(reply, /remember/i);
  assert.ok(snapshot.memory.some(m => m.text === 'Tests in this repo use pnpm'));
  ({ reply } = await say('What do you remember?'));
  assert.match(reply, /Tests in this repo use pnpm/);
  ({ reply, snapshot } = await say('remember that my openai key is sk-proj-abcdefghijklmnopqrstuvwxyz123456'));
  console.log('secret ->', reply);
  assert.match(reply, /didn’t save/);
  assert.ok(!snapshot.memory.some(m => m.text.includes('sk-proj')));
  ({ reply, snapshot } = await say('forget pnpm'));
  console.log('forget ->', reply);
  assert.match(reply, /Forgotten/);
  assert.equal(snapshot.memory.length, 0);
  const off = await command({ SetMemoryOptions: { options: { learning: false, use_memory: false } } });
  assert.deepEqual(off.memory_options, { learning: false, use_memory: false });
  ({ reply, snapshot } = await say('remember that the release branch is main'));
  assert.ok(snapshot.memory.some(m => m.text === 'The release branch is main'), 'told memories still save with learning off');
  await command({ SetMemoryOptions: { options: { learning: true, use_memory: true } } });
  console.log('memory switches -> ok');
  }

  console.log('saving workspace');
  started = Date.now();
  const workspace = await command({ SaveWorkspace: { workspace: { id: '', name: 'Repo', repository: repo, instructions: '', away_enabled: false } } });
  console.log('saved in', Date.now() - started, 'ms');
  const workspaceId = workspace.workspaces[0].id;
  const refused = await request({ Workbench: { SaveWorkspace: { workspace: { id: '', name: 'Keys', repository: path.join(os.homedir(), '.ssh'), instructions: '', away_enabled: false } } } });
  console.log('protected ->', refused.Error?.message);
  assert.ok(refused.Error && /won’t use/.test(refused.Error.message));
  await command({ CreateTask: { workspace_id: workspaceId, title: 'Smoke', goal: 'Do nothing; this is a smoke test.' } });
  const stopped = await command('CancelAllWork');
  const task = stopped.tasks[0];
  assert.equal(task.status, 'Cancelled');
  console.log('stop all ->', task.events.at(-1).message);
  for (let n = 0; n < 50; n++) {
    const outcome = await request({ Workbench: { DeleteTask: { task_id: task.id } } });
    if (outcome.Workbench) { assert.equal(outcome.Workbench.tasks.length, 0); break; }
    assert.match(outcome.Error.message, /still stopping/);
    await new Promise(resolve => setTimeout(resolve, 100));
  }
  assert.equal((await command('Snapshot')).tasks.length, 0, 'deleted');
  console.log('parity smoke passed');
} catch (error) {
  console.error('daemon stderr:', diagnostics.slice(-1500));
  throw error;
} finally {
  socket.destroy(); daemon.kill();
  rmSync(data, { recursive: true, force: true });
}
