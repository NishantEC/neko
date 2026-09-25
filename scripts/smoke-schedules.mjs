#!/usr/bin/env node
// Real daemon + SQLite + IPC; isolated HOME and deterministic scout, no external calls.
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import net from 'node:net';
import assert from 'node:assert/strict';
import { spawn, execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
const scratch=fs.mkdtempSync(path.join(os.tmpdir(),'neko-schedules-smoke-'));
const repository=path.join(scratch,'repo'),data=path.join(scratch,'data');
fs.mkdirSync(repository);fs.mkdirSync(data);
execFileSync('git',['init','-q',repository]);
fs.writeFileSync(path.join(repository,'README.md'),'Schedule planning fixture\n');
execFileSync('git',['-C',repository,'add','README.md']);
execFileSync('git',['-C',repository,'-c','user.name=Fixture','-c','user.email=fixture@example.invalid','commit','-qm','Fixture']);
fs.mkdirSync(path.join(scratch,'.codex/automations/review'),{recursive:true});
fs.writeFileSync(path.join(scratch,'.codex/automations/review/automation.toml'),`id='review'\nname='Review changes'\nprompt='Review the fixture repository'\nrrule='FREQ=HOURLY'\nstatus='ACTIVE'\ncreated_at=${Date.now()}\ncwds=[${JSON.stringify(repository)}]\n`);
fs.mkdirSync(path.join(scratch,'.claude/scheduled-tasks/brief'),{recursive:true});
fs.writeFileSync(path.join(scratch,'.claude/scheduled-tasks/brief/SKILL.md'),'---\nname: Morning brief\ndescription: Summarize changes\n---\nSummarize repository changes.\n');
let daemon,socket,diagnostics='',serial=1,buffer=Buffer.alloc(0);const pending=new Map();
const pause=()=>new Promise(resolve=>setTimeout(resolve,100));
async function start(){
  daemon=spawn(path.join(root,'target/debug/neko-daemon'),[],{env:{...process.env,HOME:scratch,NEKO_DATA_DIR:data,NEKO_CODEX_PATH:path.join(root,'scripts/fixtures/codex-workbench.mjs')},stdio:['ignore','ignore','pipe']});
  daemon.stderr.on('data',chunk=>diagnostics=(diagnostics+chunk).slice(-4000));
  for(let n=0;n<200;n++){
    if(daemon.exitCode!==null)throw Error(`Daemon exited: ${diagnostics}`);
    if(fs.existsSync(path.join(data,'neko.sock'))){
      const probe=net.connect(path.join(data,'neko.sock'));
      const connected=await new Promise(resolve=>{probe.once('connect',()=>resolve(true));probe.once('error',()=>resolve(false));});
      if(connected){socket=probe;break;}probe.destroy();
    }
    await pause();
  }
  assert.ok(socket,'Socket unavailable');buffer=Buffer.alloc(0);
  socket.on('data',chunk=>{
    buffer=Buffer.concat([buffer,chunk]);
    while(buffer.length>=4&&buffer.length>=buffer.readUInt32LE(0)+4){
      const length=buffer.readUInt32LE(0),frame=JSON.parse(buffer.subarray(4,length+4));buffer=buffer.subarray(length+4);
      if(frame.Response){pending.get(frame.Response.id)?.(frame.Response.response);pending.delete(frame.Response.id);}
    }
  });
}
async function stop(){socket?.destroy();socket=null;if(daemon&&daemon.exitCode===null){daemon.kill('SIGTERM');await new Promise(resolve=>daemon.once('exit',resolve));}}
async function request(command,expectError=false){
  const id=serial++,body=Buffer.from(JSON.stringify({Request:{id,request:{Workbench:command}}})),length=Buffer.alloc(4);length.writeUInt32LE(body.length);
  const response=await new Promise((resolve,reject)=>{const timer=setTimeout(()=>reject(Error(`IPC timeout ${diagnostics}`)),20000);pending.set(id,value=>{clearTimeout(timer);resolve(value);});socket.write(Buffer.concat([length,body]));});
  if(expectError){assert.ok(!response.Workbench,JSON.stringify(response));return response;}
  assert.ok(response.Workbench,JSON.stringify(response));return response.Workbench;
}
async function awaitPlan(count){
  for(let n=0;n<300;n++){const state=await request('Snapshot');if(state.tasks.length===count&&state.tasks.at(-1).status==='AwaitingApproval')return state;if(state.tasks.some(t=>t.status==='Failed'))throw Error(JSON.stringify(state.tasks));await pause();}
  throw Error(`Planning did not finish: ${diagnostics}`);
}
try{
  await start();
  const preview=(await request({SetupImport:{Discover:{repositories:[]}}})).import_preview;
  assert.equal(preview.schedules.length,2);
  const apply={SetupImport:{Apply:{preview_id:preview.preview_id,connection_ids:[],schedule_ids:preview.schedules.map(s=>s.id),repositories:[repository],include_credentials:false,trust_local_processes:false}}};
  let state=await request(apply);
  assert.equal(state.tasks.length,0);assert.equal(state.mcp.grants.length,0);assert.ok(state.schedules.every(s=>!s.enabled));
  const codex=state.schedules.find(s=>s.name==='Review changes'),claude=state.schedules.find(s=>s.name==='Morning brief');
  assert.equal(codex.workspace_id,state.workspaces[0].id);assert.equal(claude.workspace_id,null);assert.equal(claude.rule,'');
  await request({Schedules:{SetEnabled:{id:codex.id,enabled:true}}},true);
  state=await request({Schedules:{Save:{schedule:{...codex,prompt:'Review my modified instruction',timezone:'UTC',anchor_ms:Date.now()}}}});
  state=await request(apply);assert.equal(state.schedules.length,2);assert.equal(state.schedules.find(s=>s.id===codex.id).prompt,'Review my modified instruction');
  state=await request({Schedules:{SetEnabled:{id:codex.id,enabled:true}}});assert.ok(state.schedules.find(s=>s.id===codex.id).next_due_ms>Date.now());
  await stop();
  // Clock injection only while the isolated fixture DB has no writer. The real
  // daemon must claim this due occurrence and persist its task itself on restart.
  const database=path.join(data,'neko.db');
  const row=execFileSync('sqlite3',[database,"SELECT value FROM settings WHERE key='workbench_snapshot_v1'"],{encoding:'utf8'});
  const saved=JSON.parse(row),due=Math.floor((Date.now()-1)/1000)*1000,finite=saved.schedules.find(s=>s.id===codex.id);
  finite.rule='FREQ=HOURLY;COUNT=1';finite.anchor_ms=due;finite.next_due_ms=due;
  const sql=`UPDATE settings SET value='${JSON.stringify(saved).replaceAll("'","''")}' WHERE key='workbench_snapshot_v1'`;
  execFileSync('sqlite3',[database,sql]);
  await start();state=await awaitPlan(1);
  assert.equal(state.schedules.find(s=>s.id===codex.id).enabled,false);
  assert.equal(state.schedules.find(s=>s.id===codex.id).next_due_ms,null);
  assert.ok(state.tasks[0].plan);assert.equal(state.tasks[0].supervision,null);assert.equal(state.mcp.grants.length,0);
  assert.ok(state.schedules.find(s=>s.id===codex.id).last_result.includes('awaiting explicit approval'));
  await request({Schedules:{RunNow:{id:codex.id}}},true);
  await stop();await start();
  state=await request('Snapshot');assert.equal(state.tasks.length,1);assert.equal(state.tasks[0].status,'AwaitingApproval');
  await request({Schedules:{SetEnabled:{id:codex.id,enabled:false}}});
  await request({CancelTask:{task_id:state.tasks[0].id}});
  await request({Schedules:{RunNow:{id:codex.id}}});state=await awaitPlan(2);
  assert.equal(state.schedules.find(s=>s.id===codex.id).enabled,false);
  await request({Schedules:{Remove:{id:codex.id}}});state=await request('Snapshot');assert.equal(state.schedules.length,1);assert.equal(state.tasks.length,2);
  assert.equal(execFileSync('git',['-C',repository,'status','--porcelain'],{encoding:'utf8'}),'');
  assert.ok(state.tasks.every(t=>!fs.existsSync(path.join(t.worktree,'neko-smoke.txt'))));
  console.log(`PASS schedules: paused Codex/Claude import, missing-zone rejection, edited reimport, final finite due claim, read-only plan, restart dedupe, overlap rejection, manual paused run, removal preserves tasks (${scratch})`);
}finally{await stop();}
