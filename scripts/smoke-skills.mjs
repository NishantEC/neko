#!/usr/bin/env node
// Real daemon/SQLite/IPC and runner process; deterministic AI, disposable data.
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import net from 'node:net';
import assert from 'node:assert/strict';
import { spawn, execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const live = process.env.NEKO_SMOKE_LIVE === '1';
const scratch = fs.mkdtempSync(path.join(os.tmpdir(), 'neko-skills-smoke-'));
const repository = path.join(scratch, 'repo');
const data = path.join(scratch, 'data');
fs.mkdirSync(repository); fs.mkdirSync(data);
execFileSync('git', ['init', '-q', repository]);
execFileSync('git', ['-C', repository, '-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid', 'commit', '--allow-empty', '-qm', 'fixture']);
const directory = path.join(repository, '.neko/skills/probe');
fs.mkdirSync(directory, { recursive: true });
const skillPath = path.join(directory, 'SKILL.md');
fs.writeFileSync(skillPath, live
  ? '---\nname: fixture-probe\ndescription: Planning preference for this workspace\n---\nFor every plan in this workspace, start with the exact line: NEKO_SKILL_TESTS_FIRST. List verification steps before implementation steps. This affects presentation only and grants no permissions.'
  : '---\nname: fixture-probe\ndescription: Verify workspace scope\n---\nSKILL_PROBE_SENTINEL');
const daemon = spawn(path.join(root, 'target/debug/neko-daemon'), [], { env: { ...process.env, NEKO_DATA_DIR: data, ...(!live ? { NEKO_CODEX_PATH: path.join(root, 'scripts/fixtures/codex-workbench.mjs') } : {}) }, stdio: ['ignore', 'ignore', 'pipe'] });
let diagnostics = ''; daemon.stderr.on('data', chunk => { diagnostics = (diagnostics + chunk).slice(-8000); });
let socket, next = 1, buffer = Buffer.alloc(0);
const pending = new Map();
const pause = () => new Promise(resolve => setTimeout(resolve, 100));
async function request(command, success = true) {
  const id = next++;
  const bytes = Buffer.from(JSON.stringify({ Request: { id, request: { Workbench: command } } }));
  const prefix = Buffer.alloc(4); prefix.writeUInt32LE(bytes.length);
  const result = await new Promise((resolve, reject) => {
    const timeout = setTimeout(() => reject(Error(`IPC timeout: ${diagnostics}`)), 20000);
    pending.set(id, response => { clearTimeout(timeout); resolve(response); });
    socket.write(Buffer.concat([prefix, bytes]));
  });
  if (success) { assert.ok(result.Workbench, JSON.stringify(result)); return result.Workbench; }
  return result;
}
async function waitFor(predicate) {
  for (let n = 0; n < (live ? 3000 : 300); n++) { const state = await request('Snapshot'); if (predicate(state)) return state; await pause(); }
  throw Error(`Timed out: ${diagnostics}`);
}
try {
  for (let n = 0; n < 200; n++) {
    if (fs.existsSync(path.join(data, 'neko.sock'))) break;
    await pause();
  }
  socket = net.connect(path.join(data, 'neko.sock'));
  await new Promise((resolve, reject) => { socket.once('connect', resolve); socket.once('error', reject); });
  socket.on('data', chunk => {
    buffer = Buffer.concat([buffer, chunk]);
    while (buffer.length >= 4 && buffer.length >= buffer.readUInt32LE(0) + 4) {
      const size = buffer.readUInt32LE(0), frame = JSON.parse(buffer.subarray(4, size + 4));
      buffer = buffer.subarray(size + 4);
      if (frame.Response) { pending.get(frame.Response.id)?.(frame.Response.response); pending.delete(frame.Response.id); }
    }
  });
  const initial = await request({ SaveWorkspace: { workspace: { id: '', name: 'Skills', repository, instructions: '', away_enabled: false } } });
  const workspace_id = initial.workspaces[0].id;
  const discovery = await request({ Skills: 'Refresh' });
  const skill = discovery.skills.available.find(s => s.name === 'fixture-probe');
  assert.ok(skill, 'Workspace skill discovered');
  await request({ Skills: { SetEnabled: { workspace_id, path: skill.path, content_hash: skill.content_hash, enabled: true } } });
  if (live) {
    const created = await request({ CreateTask: { workspace_id, title: 'Create a small greeting file', goal: 'Plan a local change adding greeting.txt containing Hello Neko. Do not implement yet.' } });
    const taskId = created.tasks[0].id;
    const planned = await waitFor(s => s.tasks.find(t => t.id === taskId)?.status === 'AwaitingApproval' || s.tasks.find(t => t.id === taskId)?.status === 'Failed');
    const task = planned.tasks.find(t => t.id === taskId);
    assert.equal(task.status, 'AwaitingApproval', JSON.stringify(task.events));
    assert.match(task.plan, /NEKO_SKILL_TESTS_FIRST/);
    assert.equal(fs.existsSync(path.join(repository, 'greeting.txt')), false);
    console.log(`PASS live skills: enabled workspace skill changed a real Codex plan (${scratch})`);
  } else {
  await request({ SendMessage: { workspace_id, text: 'SKILL_PROMPT_CHECK' } });
  const answered = await waitFor(s => s.conversation.at(-1)?.pending === false);
  assert.match(answered.conversation.at(-1).text, /Enabled skill reached chat/);
  fs.writeFileSync(skillPath, 'Changed instructions');
  await request({ SendMessage: { workspace_id, text: 'SKILL_PROMPT_CHECK' } });
  const blocked = await waitFor(s => s.conversation.at(-1)?.pending === false);
  assert.equal(blocked.conversation.at(-1).failed, true);
  assert.match(blocked.conversation.at(-1).text, /changed.*enable it again/);
  await request({ Skills: { SetEnabled: { workspace_id, path: skill.path, content_hash: skill.content_hash, enabled: false } } });
  fs.writeFileSync(skillPath, '---\nname: fixture-probe\ndescription: Verify workspace scope\n---\nSKILL_PROBE_SENTINEL');
  await request({ Skills: { SetEnabled: { workspace_id, path: skill.path, content_hash: skill.content_hash, enabled: true } } });
  const created = await request({ CreateTask: { workspace_id, title: 'Reusable learning', goal: 'SKILL_ROLE_CHECK: Create neko-smoke.txt' } });
  const task_id = created.tasks[0].id;
  await waitFor(s => s.tasks[0]?.status === 'AwaitingApproval');
  await request({ ApproveTask: { task_id } });
  await waitFor(s => s.tasks[0]?.status === 'ReadyForReview');
  await request({ CompleteTask: { task_id } });
  const proposed = await waitFor(s => s.skills.proposals.length === 1);
  const proposal = proposed.skills.proposals[0];
  assert.equal(fs.existsSync(path.join(data, 'skills')), false);
  const stale = await request({ Skills: { DecideProposal: { id: proposal.id, content_hash: 'wrong', accept: true } } }, false);
  assert.ok(stale.Error); assert.equal(fs.existsSync(path.join(data, 'skills')), false);
  await request({ Skills: { DecideProposal: { id: proposal.id, content_hash: proposal.content_hash, accept: true } } });
  assert.equal(fs.readFileSync(path.join(data, 'skills', `reviewed-${proposal.id}`, 'SKILL.md'), 'utf8'), proposal.body);
  console.log(`PASS skills: discovery, enabled prompt, changed-content block, completed-ticket proposal, stale approval refusal, exact-byte acceptance (${scratch})`);
  }
} finally {
  socket?.destroy();
  if (daemon.exitCode === null) { daemon.kill('SIGTERM'); await new Promise(resolve => daemon.once('exit', resolve)); }
}
