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
  if (!live) {
    const parallelRepo = path.join(scratch,'parallel-repo'); fs.mkdirSync(parallelRepo);
    execFileSync('git',['-C',parallelRepo,'init','--quiet']);
    execFileSync('git',['-C',parallelRepo,'-c','user.name=Neko Test','-c','user.email=test@example.invalid','commit','--allow-empty','-qm','parallel']);
    const parallelWorkspace = await command({SaveWorkspace:{workspace:{id:'',name:'Parallel capacity',repository:parallelRepo,instructions:'Disposable fixture',away_enabled:false}}});
    const parallelWorkspaceId = parallelWorkspace.workspaces.find(w=>w.name==='Parallel capacity').id;
    const parallelIds = [];
    for (const workspace of [workspaceId,workspaceId,parallelWorkspaceId]) {
      const queued = await command({CreateTask:{workspace_id:workspace,title:'Concurrent ticket',goal:'PARALLEL_HOLD: create and verify the isolated smoke output'}});
      parallelIds.push(queued.tasks.at(-1).id);
    }
    for (const id of parallelIds) await waitFor(id,'AwaitingApproval');
    for (const id of parallelIds) await command({ApproveTask:{task_id:id}});
    await waitSnapshot(s=>parallelIds.every(id=>s.tasks.find(t=>t.id===id).events.some(e=>e.role==='builder' && e.message.includes('Agent started'))) && parallelIds.every(id=>s.tasks.find(t=>t.id===id).status==='Building'),'Three builders did not overlap');
    for (const id of parallelIds) await waitFor(id,'ReadyForReview');
    console.log('Three real fixture child processes overlapped across two workspaces.');
    const broken = await command({ CreateTask: {workspace_id:workspaceId,title:'Broken builder gate',goal:'BROKEN_BUILDER: create the requested file and verify it'} });
    const id = broken.tasks.at(-1).id;
    await waitFor(id,'AwaitingApproval');
    await command({ApproveTask:{task_id:id}});
    const rejected = await waitFor(id,'Failed');
    assert.ok(rejected.events.some(e => e.message.includes('Independent verification failed')));
    assert.match(rejected.result,/Broken builder output/);
    for (const conflict of [false,true]) {
      const created = await command({CreateTask:{workspace_id:workspaceId,title:conflict?'Conflicting split':'Parallel split',goal:conflict?'SPLIT_CONFLICT':'Split into independent left and right files'}});
      const parentId = created.tasks.at(-1).id;
      await waitFor(parentId,'AwaitingApproval');
      await command({ProposeSplit:{task_id:parentId}});
      await waitFor(parentId,'AwaitingApproval');
      const proposed = await command('Snapshot');
      assert.equal(proposed.splits.find(s=>s.parent_id===parentId).approved,false);
      assert.ok(proposed.splits.find(s=>s.parent_id===parentId).subtasks.every(p=>p.task_id===null));
      await command({ApproveTask:{task_id:parentId}});
      const duplicate = await request({Workbench:{ApproveTask:{task_id:parentId}}});
      assert.ok(duplicate.Error);
      const done = await waitFor(parentId,conflict?'Failed':'ReadyForReview');
      const state = await command('Snapshot');
      const split = state.splits.find(s=>s.parent_id===parentId);
      assert.ok(split.subtasks.every(p=>state.tasks.find(t=>t.id===p.task_id).status==='ReadyForReview'));
      if (conflict) assert.ok(done.events.some(e=>e.message.includes('integration conflict')));
      else for(const file of ['left.txt','right.txt']) assert.equal(fs.readFileSync(path.join(done.worktree,file),'utf8'),'Isolated task output\n');
      assert.equal(git('status','--porcelain'),'');
    }
    console.log('Parallel decomposition, duplicate approval rejection, preserved conflict, and broken-builder verification gate passed.');
    for (const fail of [false,true]) {
      const created = await command({CreateTask:{workspace_id:workspaceId,title:'Dependency split',goal:fail?'SPLIT_DEPENDENCY_FAILURE':'SPLIT_DEPENDENT'}});
      const id = created.tasks.at(-1).id;
      await waitFor(id,'AwaitingApproval');
      await command({ProposeSplit:{task_id:id}}); await waitFor(id,'AwaitingApproval');
      await command({ApproveTask:{task_id:id}});
      await waitFor(id,fail?'Failed':'ReadyForReview');
      const state = await command('Snapshot');
      const split = state.splits.find(s=>s.parent_id===id);
      const dependent = state.tasks.find(t=>t.id===split.subtasks[1].task_id);
      if (fail) { assert.equal(dependent.status,'Cancelled'); assert.equal(dependent.worktree,null); }
      else { assert.equal(fs.readFileSync(path.join(dependent.worktree,'left.txt'),'utf8'),'Isolated task output\n'); }
    }
    console.log('Dependency seeding and dependency-failure cancellation passed.');
    {
      const created = await command({CreateTask:{workspace_id:workspaceId,title:'Parent required-check gate',goal:'PARENT_MISSING_CHECK'}});
      const id = created.tasks.at(-1).id;
      await waitFor(id,'AwaitingApproval'); await command({ProposeSplit:{task_id:id}}); await waitFor(id,'AwaitingApproval');
      await command({ApproveTask:{task_id:id}});
      const failed = await waitFor(id,'Failed');
      assert.ok(failed.events.some(e=>e.message.includes('omitted approved check: node fixture-check')));
      const state=await command('Snapshot');
      assert.ok(state.splits.find(s=>s.parent_id===id).subtasks.every(p=>state.tasks.find(t=>t.id===p.task_id).status==='ReadyForReview'));
      console.log('Parent integration cannot omit approved checks despite an unrelated successful receipt.');
    }
    for (const restart of [false,true]) {
      const created = await command({CreateTask:{workspace_id:workspaceId,title:'Split lifecycle',goal:'SPLIT_HOLD'}});
      const id = created.tasks.at(-1).id;
      await waitFor(id,'AwaitingApproval'); await command({ProposeSplit:{task_id:id}}); await waitFor(id,'AwaitingApproval');
      await command({ApproveTask:{task_id:id}});
      await waitSnapshot(s=>s.splits.find(p=>p.parent_id===id).subtasks.some(p=>s.tasks.find(t=>t.id===p.task_id).events.some(e=>e.role==='builder' && e.message.includes('Agent started'))),'Child did not start');
      if(restart) { await stop(); launch(); await connect(); }
      else await command({CancelTask:{task_id:id}});
      const state=await command('Snapshot'); const split=state.splits.find(s=>s.parent_id===id);
      assert.equal(state.tasks.find(t=>t.id===id).status,restart?'Failed':'Cancelled');
      assert.ok(split.subtasks.every(p=>state.tasks.find(t=>t.id===p.task_id).status===(restart?'Failed':'Cancelled')));
      assert.ok(split.subtasks.some(p=>state.tasks.find(t=>t.id===p.task_id).worktree));
    }
    console.log('Parent cancellation and restart fail closed while preserving child worktrees.');
  }
  if (live) {
    // Real model, real bridge and a disposable MCP server. No upstream account
    // or external write is involved; the action tool simply echoes arguments.
    for (const mode of ['readonly', 'normal']) {
      const added = await mcp({ AddConnection: { workspace_id: workspaceId, label: `Live chat ${mode}`, config: { transport: 'stdio', command: process.execPath, args: [path.join(root, 'scripts/fixtures/mcp-host.mjs'), mode] }, trust_local_process: true, credentials: null } });
      const connection = added.mcp.connections.find(c => c.label === `Live chat ${mode}`);
      const discovered = await mcp({ Discover: { connection_id: connection.id } });
      const tool = discovered.mcp.connections.find(c => c.id === connection.id).tools[0];
      assert.ok(tool, 'Live fixture discovery failed');
      await mcp({ SetToolGrant: { connection_id: connection.id, tool_name: tool.name, schema_hash: tool.schema_hash, allowed: true } });
      const start = await command({ SendMessage: { workspace_id: workspaceId, text: `Use neko_list_tools and neko_call_tool now to call the echo tool on connection ${connection.id} with arguments {"text":"neko-live-${mode}"}. This is a disposable local fixture. Wait for host approval if needed. Do not create a ticket, remember anything, or use shell. Report the returned text.` } });
      const turnId = start.conversation.at(-1).id;
      if (mode === 'normal') {
        const waiting = await waitSnapshot(s => s.conversation.find(m => m.id === turnId)?.tool_calls.some(c => c.status === 'awaiting_approval'), 'Real model did not request tool approval');
        assert.ok(!waiting.mcp.receipts.some(r => r.run_id === `chat:${turnId}`));
        const call = waiting.conversation.find(m => m.id === turnId).tool_calls.find(c => c.status === 'awaiting_approval');
        await command({ DecideChatTool: { turn_id: turnId, call_id: call.id, approve: true } });
      }
      const done = await waitSnapshot(s => !s.conversation.find(m => m.id === turnId)?.pending, 'Real model chat did not finish');
      const turn = done.conversation.find(m => m.id === turnId);
      assert.equal(turn.failed, false, turn.text);
      assert.ok(done.mcp.receipts.some(r => r.run_id === `chat:${turnId}` && r.connection_id === connection.id && r.success), 'Real model never completed the tool call');
      assert.match(turn.text, new RegExp(`neko-live-${mode}`));
      await mcp({ SetEnabled: { connection_id: connection.id, enabled: false } });
    }
    console.log('Live chat read and approved action passed with real model and local MCP fixture.');
  }
  // Configure everything through user-facing IPC. No direct database seeding.
  if (!live) {
    await command({ SaveWorkspace: { workspace: { ...restored.workspaces.find(w => w.id === workspaceId), away_enabled: true } } });
    const otherRepo = path.join(scratch, 'other-repo'); fs.mkdirSync(otherRepo);
    execFileSync('git', ['-C', otherRepo, 'init', '--quiet']);
    execFileSync('git', ['-C', otherRepo, '-c', 'user.name=Neko Test', '-c', 'user.email=test@example.invalid', 'commit', '--allow-empty', '-qm', 'fixture']);
    const second = await command({ SaveWorkspace: { workspace: { id: '', name: 'Other workspace', repository: otherRepo, instructions: 'Disposable scope-isolation fixture.', away_enabled: false } } });
    const otherWorkspaceId = second.workspaces.find(w => w.name === 'Other workspace').id;
    const fixtureServer = path.join(root, 'scripts/fixtures/mcp-workbench.mjs');
    async function addConnection(workspace_id, label, namespace) {
      const added = await mcp({ AddConnection: { workspace_id, label, config: { transport: 'stdio', command: process.execPath, args: [fixtureServer, namespace] }, trust_local_process: true, credentials: null } });
      const connection = added.mcp.connections.find(c => c.label === label);
      const discovered = await mcp({ Discover: { connection_id: connection.id } });
      const ready = discovered.mcp.connections.find(c => c.id === connection.id);
      assert.equal(ready.error, null, JSON.stringify(ready));
      assert.equal(ready.tools.length, 1);
      assert.ok(discovered.mcp.grants.some(g => g.connection_id === ready.id && g.tool_name === ready.tools[0].name), 'Discovery makes tools available in the connected workspace');
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
    // Talking to Neko: a plain question gets a reply and no work; asking for
    // a fix opens exactly one ordinary queued ticket linked to the reply.
    const before = durable.tasks.length;
    await command({ SendMessage: { text: 'Anything on fire?', workspace_id: workspaceId } });
    const queued = await command({ SendMessage: { text: 'second', workspace_id: null } });
    assert.ok(queued.conversation.some(message => message.text === 'second' && message.queued), 'Second message was not queued');
    const quiet = await waitSnapshot(s => s.conversation.length === 4 && !s.conversation[3].pending, 'Neko did not process queued reply');
    assert.equal(quiet.conversation[1].role, 'neko');
    assert.equal(quiet.conversation[1].failed, false);
    assert.equal(quiet.tasks.length, before);
    await command({ SendMessage: { text: 'Please fix the cart crash', workspace_id: workspaceId } });
    const replied = await waitSnapshot(s => s.conversation.length === 6 && !s.conversation[5].pending, 'Neko did not open a ticket');
    assert.equal(replied.conversation[5].ticket_ids.length, 1);
    const opened = replied.tasks.find(t => t.id === replied.conversation[5].ticket_ids[0]);
    assert.equal(opened.workspace_id, workspaceId);
    assert.ok(['Queued', 'Planning', 'AwaitingApproval'].includes(opened.status), opened.status);
    const noted = await command({ AddTicketNote: { task_id: opened.id, text: 'Also cover discount-only carts.' } });
    assert.ok(noted.tasks.find(t => t.id === opened.id).events.some(e => e.role === 'note' && /discount-only/.test(e.message)));
    await command({ CancelTask: { task_id: opened.id } });
    await stop(); launch(); await connect();
    const kept = await command('Snapshot');
    assert.equal(kept.conversation.length, 6);
    // Memory: a stated preference is remembered and shown under the reply; a
    // decision is filed as one; user entries can be added and forgotten.
    await command({ SendMessage: { text: 'Remember that I always want small PRs', workspace_id: workspaceId } });
    const learned = await waitSnapshot(s => s.conversation.length === 8 && !s.conversation[7].pending, 'Neko did not remember');
    assert.equal(learned.conversation[7].remembered.length, 1);
    assert.equal(learned.memory.filter(m => m.source === 'chat').length, 1);
    assert.ok(learned.memory.some(m => m.source.startsWith('ticket:') && m.kind === 'decision'));
    await command({ SendMessage: { text: 'We decided to drop IE11 support', workspace_id: workspaceId } });
    const decided = await waitSnapshot(s => s.conversation.length === 10 && !s.conversation[9].pending, 'Neko did not record the decision');
    assert.ok(decided.memory.some(m => m.kind === 'decision'));
    const added = await command({ SaveMemory: { entry: { id: '', kind: 'workspace', workspace_id: workspaceId, text: 'Run make test before committing', source: 'user', created_at_ms: 0, updated_at_ms: 0 } } });
    const note = added.memory.find(m => m.text === 'Run make test before committing');
    assert.equal(note.workspace_id, workspaceId);
    const forgotten = await command({ DeleteMemory: { id: note.id } });
    assert.ok(!forgotten.memory.some(m => m.id === note.id));
    await stop(); launch(); await connect();
    assert.equal((await command('Snapshot')).memory.filter(m => m.source === 'chat').length, 2);
    // Actual chat runner -> stdio MCP adapter -> scoped daemon host. The
    // fixture declares this lookup read-only, so it can run without a manual
    // chat approval while the foreign workspace remains inaccessible.
    await mcp({ SetEnabled: { connection_id: own.id, enabled: true } });
    await mcp({ SetToolGrant: { connection_id: own.id, tool_name: own.tools[0].name, schema_hash: own.tools[0].schema_hash, allowed: true } });
    const start = await command({ SendMessage: { text: `CHAT_TOOL_PROBE FOREIGN_CONNECTION_ID=${foreign.id}`, workspace_id: workspaceId } });
    const turnId = start.conversation.at(-1).id;
    const done = await waitSnapshot(s => !s.conversation.find(m => m.id === turnId)?.pending, 'Read-only chat tool did not settle');
    const receipts = done.mcp.receipts.filter(r => r.run_id === `chat:${turnId}`);
    assert.equal(receipts.length, 1);
    assert.equal(receipts[0].success, true);
    assert.equal(receipts[0].workspace_id, workspaceId);
  }
  console.log(JSON.stringify({ passed: true, agent: live ? 'live Codex CLI' : 'deterministic fixture', scratch, taskId, checks: ['real IPC', 'private socket', 'durable storage', 'read-only plan', 'approval gate', 'isolated build', 'independent review', 'palette task', 'invalid approval', 'cancellation', 'daemon restart', ...(!live ? ['user-added generic MCPs', 'connected tools available by default', 'real stdio bridge and receipts', 'workspace scope rejection', 'low-risk standing delegation', 'sensitive work held', 'manual task held under Away', 'wake deduplication', 'pause and revoke', 'durable MCP policy', 'Neko chat reply', 'chat opens a ticket', 'queued messages', 'ticket notes', 'durable chat', 'memory from chat', 'decisions', 'memory editing', 'durable memory'] : [])] }, null, 2));
} finally { await stop(); }
