#!/usr/bin/env node
// Real private daemon IPC + SQLite + child process; deterministic model only.
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import net from 'node:net';
import assert from 'node:assert/strict';
import {spawn, execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const scratch = fs.mkdtempSync(path.join(os.tmpdir(), 'neko-memory-learning-'));
const data = path.join(scratch, 'data'), repo = path.join(scratch, 'repo');
fs.mkdirSync(data); fs.mkdirSync(repo);
execFileSync('git', ['init', '-q', repo]);
execFileSync('git', ['-C', repo, '-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid', 'commit', '--allow-empty', '-qm', 'fixture']);
let daemon, socket, buffer = Buffer.alloc(0), serial = 1, diagnostics = '';
const pending = new Map();
const pause = () => new Promise(resolve => setTimeout(resolve, 100));
async function start() {
  daemon = spawn(path.join(root, 'target/debug/neko-daemon'), [], {env: {...process.env, NEKO_DATA_DIR:data, NEKO_CODEX_PATH:path.join(root,'scripts/fixtures/codex-workbench.mjs')}, stdio:['ignore','ignore','pipe']});
  daemon.stderr.on('data', chunk => diagnostics = (diagnostics + chunk).slice(-8000));
  for(let n=0;n<200;n++) {
    if(daemon.exitCode!==null) throw Error(diagnostics);
    try {socket=await new Promise((resolve,reject)=>{const s=net.connect(path.join(data,'neko.sock'));s.once('connect',()=>resolve(s));s.once('error',reject);});break;} catch {await pause();}
  }
  assert.ok(socket,diagnostics);buffer=Buffer.alloc(0);
  socket.on('data', chunk => {
    buffer=Buffer.concat([buffer,chunk]);
    while(buffer.length>=4 && buffer.length>=buffer.readUInt32LE(0)+4){const n=buffer.readUInt32LE(0), frame=JSON.parse(buffer.subarray(4,n+4));buffer=buffer.subarray(n+4);if(frame.Response){pending.get(frame.Response.id)?.(frame.Response.response);pending.delete(frame.Response.id);}}
  });
}
async function stop(){socket?.destroy();socket=null;if(daemon?.exitCode===null){daemon.kill('SIGTERM');await new Promise(resolve=>daemon.once('exit',resolve));}}
async function command(command, error=false){
  const id=serial++, bytes=Buffer.from(JSON.stringify({Request:{id,request:{Workbench:command}}})), header=Buffer.alloc(4);header.writeUInt32LE(bytes.length);
  const result=await new Promise((resolve,reject)=>{const timer=setTimeout(()=>{pending.delete(id);reject(Error('IPC timeout'));},10000);pending.set(id,r=>{clearTimeout(timer);resolve(r);});socket.write(Buffer.concat([header,bytes]));});
  if(error){assert.ok(result.Error,JSON.stringify(result));return result.Error;}
  assert.ok(result.Workbench,JSON.stringify(result));return result.Workbench;
}
async function until(predicate){for(let n=0;n<600;n++){const s=await command('Snapshot');if(predicate(s))return s;await pause();}throw Error(`Learning did not settle: ${diagnostics}`);}
try {
  await start();
  let state=await command({SaveWorkspace:{workspace:{id:'',name:'Learning fixture',repository:repo,instructions:'Disposable isolated test',away_enabled:false}}});
  const workspace=state.workspaces[0].id;
  state=await command({SendMessage:{text:'MEMORY_LEARNING_PROBE: small verification batches worked well.',workspace_id:workspace}});
  const turn=state.conversation.at(-1).id;
  state=await until(s=>s.memory_proposals.filter(p=>p.source===`chat:${turn}`).length===2);
  assert.equal(state.memory.length,0,'Inference silently persisted');
  assert.ok(state.memory_proposals.every(p=>p.workspace_id===workspace && p.agent_profile_id==='default'));
  const [accepted,rejected]=state.memory_proposals;
  await stop();await start();
  assert.equal((await command('Snapshot')).memory_proposals[0].id,accepted.id,'Restart changed proposal identity');
  state=await command({DecideMemoryProposal:{id:accepted.id,accept:true}});
  assert.equal(state.memory[0].text,accepted.text);
  state=await command({DecideMemoryProposal:{id:rejected.id,accept:false}});
  assert.equal(state.memory.length,1);
  assert.equal(state.memory_proposals.length,0);
  await command({DecideMemoryProposal:{id:accepted.id,accept:true}},true);
  await stop();await start();
  state=await command('Snapshot');assert.equal(state.memory.length,1);assert.equal(state.memory_proposals.length,0);
  state=await command({CreateTask:{workspace_id:workspace,title:'MEMORY_LEARNING_PROBE completed ticket',goal:'Create neko-smoke.txt and verify its exact bytes.'}});
  const task=state.tasks.at(-1).id;
  await until(s=>s.tasks.find(t=>t.id===task)?.status==='AwaitingApproval');
  await command({AddTicketNote:{task_id:task,text:'Use the existing fixture verification.'}});
  state=await command({ApproveTask:{task_id:task}});
  assert.ok(state.memory.some(m=>m.source.startsWith(`ticket:${task}:approval:`)));
  assert.ok(state.memory.some(m=>m.source.startsWith(`ticket:${task}:note:`)));
  await until(s=>s.tasks.find(t=>t.id===task)?.status==='ReadyForReview');
  await command({CompleteTask:{task_id:task}});
  // The accepted chat fact is deduped; the previously rejected source does not
  // block an independent completed-ticket source from suggesting its evidence.
  state=await until(s=>s.memory_proposals.some(p=>p.source===`ticket:${task}:completed`));
  const proposal=state.memory_proposals.find(p=>p.source===`ticket:${task}:completed`);
  await command({AgentProfiles:{Save:{profile:{id:'default',name:'Neko',instructions:'Updated after extraction'}}}});
  state=await command('Snapshot');assert.equal(state.memory_proposals.length,0);
  await command({DecideMemoryProposal:{id:proposal.id,accept:true}},true);
  const cancelled=await command({CreateTask:{workspace_id:workspace,title:'Cancel decision',goal:'No execution'}});
  const cancelId=cancelled.tasks.at(-1).id;
  state=await command({CancelTask:{task_id:cancelId}});
  assert.ok(state.memory.some(m=>m.source.startsWith(`ticket:${cancelId}:cancellation:`)&&m.text==='Cancelled this task.'));
  console.log(JSON.stringify({passed:true,scratch,checks:['independent chat extraction','no tools or bridge','no inferred autosave','immutable proposals survive restart','exact accept and reject','no duplicate accept','completed-ticket extraction','approval/note/cancel decisions','profile revocation fence']},null,2));
} finally {await stop();}
