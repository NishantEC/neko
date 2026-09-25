#!/usr/bin/env node
// Real isolated daemon IPC and child prompts; deterministic model, no external calls.
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import net from 'node:net';
import assert from 'node:assert/strict';
import {spawn, execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const live = process.env.NEKO_SMOKE_LIVE === '1';
const scratch = fs.mkdtempSync(path.join(os.tmpdir(), 'neko-profiles-smoke-'));
const data = path.join(scratch, 'data'), repo = path.join(scratch, 'repo');
fs.mkdirSync(data); fs.mkdirSync(repo);
execFileSync('git', ['init', '-q', repo]);
execFileSync('git', ['-C', repo, '-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid', 'commit', '--allow-empty', '-qm', 'fixture']);
let daemon, socket, buffer = Buffer.alloc(0), serial = 1, diagnostics = '';
const pending = new Map();
const pause = () => new Promise(resolve => setTimeout(resolve, 100));
async function start() {
  daemon = spawn(path.join(root, 'target/debug/neko-daemon'), [], {env: {...process.env, NEKO_DATA_DIR: data, ...(live?{}:{HOME:scratch,NEKO_CODEX_PATH:path.join(root, 'scripts/fixtures/codex-workbench.mjs')})}, stdio: ['ignore','ignore','pipe']});
  daemon.stderr.on('data', chunk => diagnostics = (diagnostics + chunk).slice(-4000));
  for (let i=0;i<200;i++) {
    if (daemon.exitCode !== null) throw Error(diagnostics);
    try {
      socket = await new Promise((resolve,reject) => {const s=net.connect(path.join(data,'neko.sock'));s.once('connect',()=>resolve(s));s.once('error',reject);});
      break;
    } catch {await pause();}
  }
  assert.ok(socket, diagnostics); buffer=Buffer.alloc(0);
  socket.on('data', chunk => {
    buffer=Buffer.concat([buffer,chunk]);
    while (buffer.length>=4 && buffer.length>=buffer.readUInt32LE(0)+4) {
      const n=buffer.readUInt32LE(0), frame=JSON.parse(buffer.subarray(4,n+4));buffer=buffer.subarray(n+4);
      if(frame.Response){pending.get(frame.Response.id)?.(frame.Response.response);pending.delete(frame.Response.id);}
    }
  });
}
async function stop() {socket?.destroy();socket=null;if(daemon?.exitCode===null){daemon.kill('SIGTERM');await new Promise(resolve=>daemon.once('exit',resolve));}}
async function command(command, error=false) {
  const id=serial++, body=Buffer.from(JSON.stringify({Request:{id,request:{Workbench:command}}})), header=Buffer.alloc(4);header.writeUInt32LE(body.length);
  const result=await new Promise((resolve,reject)=>{const timer=setTimeout(()=>reject(Error(`IPC timeout: ${diagnostics}`)),20000);pending.set(id,value=>{clearTimeout(timer);resolve(value);});socket.write(Buffer.concat([header,body]));});
  if(error){assert.ok(!result.Workbench,JSON.stringify(result));return result;}
  assert.ok(result.Workbench,JSON.stringify(result));return result.Workbench;
}
const profile = value => command({AgentProfiles:value});
async function check(present, absent, workspace_id=null) {
  const text=live?'For this diagnostic, reply only with the uppercase tokens ending in _SENTINEL that appear in your CURRENT injected profile instructions and memory sections. Ignore tokens appearing only in recent conversation. Do not use tools, inspect files or create tickets.':`PROFILE_PROMPT_CHECK ${JSON.stringify({present,absent})}`;
  let state=await command({SendMessage:{text,workspace_id}});
  const id=state.conversation.at(-1).id;
  for(let i=0;i<(live?3000:200);i++) {
    state=await command('Snapshot');const turn=state.conversation.find(m=>m.id===id);
    if(!turn.pending){assert.equal(turn.failed,false,turn.text);if(live){for(const token of present)assert.ok(turn.text.includes(token),`Missing ${token}: ${turn.text}`);for(const token of absent)assert.ok(!turn.text.includes(token),`Unexpected ${token}: ${turn.text}`);}else{assert.equal(turn.text,'Profile context verified.');}return;}
    await pause();
  }
  throw Error('Profile chat did not finish');
}
const memory=(agent_profile_id,text,workspace_id=null)=>command({SaveMemory:{entry:{agent_profile_id,id:'',kind:workspace_id?'workspace':'profile',workspace_id,text,source:'user',created_at_ms:0,updated_at_ms:0}}});
try {
  await start();
  let state=await command('Snapshot');assert.equal(state.agent_profiles.active_profile_id,'default');
  await profile({Save:{profile:{id:'default',name:'Work',instructions:'WORK_STYLE_SENTINEL'}}});
  state=await profile({Save:{profile:{id:'',name:'Personal',instructions:'PERSONAL_STYLE_SENTINEL'}}});
  const personal=state.agent_profiles.profiles.find(p=>p.name==='Personal').id;
  state=await command({SaveWorkspace:{workspace:{id:'',name:'PERSONAL_WORKSPACE_SENTINEL',repository:repo,instructions:'',away_enabled:false}}});
  const workspace=state.workspaces[0].id;
  await profile({AssignWorkspace:{workspace_id:workspace,profile_id:personal}});
  await memory('default','WORK_MEMORY_SENTINEL');await memory(personal,'PERSONAL_MEMORY_SENTINEL');await memory(personal,'WORKSPACE_MEMORY_SENTINEL',workspace);
  await check(['WORK_STYLE_SENTINEL','WORK_MEMORY_SENTINEL'],['PERSONAL_STYLE_SENTINEL','PERSONAL_MEMORY_SENTINEL','PERSONAL_WORKSPACE_SENTINEL']);
  await profile({SetActive:{profile_id:personal}});
  await check(['PERSONAL_STYLE_SENTINEL','PERSONAL_MEMORY_SENTINEL'],['WORK_STYLE_SENTINEL','WORK_MEMORY_SENTINEL','WORKSPACE_MEMORY_SENTINEL']);
  await check(['PERSONAL_STYLE_SENTINEL','WORKSPACE_MEMORY_SENTINEL'],['WORK_STYLE_SENTINEL','WORK_MEMORY_SENTINEL'],workspace);
  await profile({SetReadGrant:{reader_id:personal,source_id:'default',allowed:true}});
  await check(['PERSONAL_STYLE_SENTINEL','PERSONAL_MEMORY_SENTINEL','WORK_MEMORY_SENTINEL'],['WORK_STYLE_SENTINEL']);
  await profile({SetReadGrant:{reader_id:personal,source_id:'default',allowed:false}});
  await stop();await start();
  await check(['PERSONAL_STYLE_SENTINEL','PERSONAL_MEMORY_SENTINEL'],['WORK_STYLE_SENTINEL','WORK_MEMORY_SENTINEL']);
  state=await command('Snapshot');assert.equal(state.agent_profiles.active_profile_id,personal);assert.equal(state.mcp.grants.length,0);
  await command({SaveMemory:{entry:{...state.memory[0],agent_profile_id:personal}}},true);
  console.log(`PASS ${live?'live-model':'fixture'} profiles: persistence, scoped/global instructions and memory, cross-profile denial, explicit directional sharing and revocation, no tool grants (${scratch})`);
} finally {await stop();}
