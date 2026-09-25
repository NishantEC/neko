#!/usr/bin/env node
// Real daemon and IPC, synthetic configuration and no Keychain writes or tool launches.
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import net from 'node:net';
import assert from 'node:assert/strict';
import { spawn, execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const scratch = fs.mkdtempSync(path.join(os.tmpdir(), 'neko-import-smoke-'));
const repository = path.join(scratch, 'repo'), data = path.join(scratch, 'data');
fs.mkdirSync(repository); fs.mkdirSync(data); fs.mkdirSync(path.join(scratch, '.codex'));
execFileSync('git', ['init', '-q', repository]);
fs.mkdirSync(path.join(repository, '.codex'));
const bin = path.join(scratch, 'bin'), sentinel = path.join(scratch, 'external-tool-ran');
fs.mkdirSync(bin);
const fixtureTool = path.join(bin, 'fixture-tool');
fs.writeFileSync(fixtureTool, '#!/bin/sh\ntouch "$NEKO_IMPORT_SENTINEL"\n');
fs.chmodSync(fixtureTool, 0o755);
fs.writeFileSync(path.join(scratch, '.codex/config.toml'), `[mcp_servers.global]\nurl="https://example.org/mcp"\nbearer_token_env_var="NEKO_IMPORT_TEST_TOKEN"\n[mcp_servers.paused]\nurl="https://example.org/paused"\nenabled=false\n[mcp_servers.sentinel]\ncommand="fixture-tool"\n`);
fs.writeFileSync(path.join(scratch, '.claude.json'), JSON.stringify({mcpServers:{claude:{url:'https://example.org/claude',headers:{Authorization:'Bearer claude-secret'}}}}));
fs.mkdirSync(path.join(scratch, '.paseo/projects'), {recursive:true});
fs.writeFileSync(path.join(scratch, '.paseo/projects/projects.json'), JSON.stringify([{rootPath:repository}]));
fs.writeFileSync(path.join(scratch, '.paseo/paseo.pid'), 'fixture-live-state');
fs.mkdirSync(path.join(scratch, '.claude'), {recursive:true});
fs.writeFileSync(path.join(scratch, '.claude/scheduled_tasks.json'), '{"undocumented":true}');
fs.mkdirSync(path.join(scratch, '.codex/automations/daily'), {recursive:true});
fs.writeFileSync(path.join(scratch, '.codex/automations/daily/automation.toml'), 'name="Daily fixture"\nprompt="Review the fixture"\nkind="cron"\nrrule="FREQ=DAILY"\ncwds=["' + repository + '"]\ncreated_at=1700000000000\n');
fs.mkdirSync(path.join(scratch, '.codex/skills/global'), {recursive:true});
fs.writeFileSync(path.join(scratch, '.codex/skills/global/SKILL.md'), '---\nname: Global fixture\ndescription: Global instructions\n---\nDo not execute this during discovery.');
fs.mkdirSync(path.join(repository, '.agents/skills/local'), {recursive:true});
fs.writeFileSync(path.join(repository, '.agents/skills/local/SKILL.md'), '---\nname: Workspace fixture\ndescription: Workspace instructions\n---\nReview only.');
fs.writeFileSync(path.join(repository, '.mcp.json'), JSON.stringify({mcpServers:{scoped:{url:'https://example.org/scoped'}}}));
const daemon = spawn(path.join(root, 'target/debug/neko-daemon'), [], {env:{...process.env, HOME:scratch, PATH:`${bin}:${process.env.PATH}`, NEKO_DATA_DIR:data, NEKO_IMPORT_TEST_TOKEN:'fixture-secret-not-for-snapshots', NEKO_IMPORT_SENTINEL:sentinel},stdio:['ignore','ignore','pipe']});
let diagnostics='', socket, serial=1, buffer=Buffer.alloc(0);
daemon.stderr.on('data', chunk => diagnostics=(diagnostics+chunk).slice(-4000));
const pending = new Map();
const pause=()=>new Promise(resolve=>setTimeout(resolve,100));
async function request(command) {
  const id=serial++, body=Buffer.from(JSON.stringify({Request:{id,request:{Workbench:command}}}));
  const length=Buffer.alloc(4); length.writeUInt32LE(body.length);
  const response=await new Promise((resolve,reject)=>{
    const timer=setTimeout(()=>reject(Error(`IPC timeout: ${diagnostics}`)),20000);
    pending.set(id, value=>{clearTimeout(timer);resolve(value);}); socket.write(Buffer.concat([length,body]));
  });
  assert.ok(response.Workbench, JSON.stringify(response));
  assert.ok(!JSON.stringify(response).includes('fixture-secret-not-for-snapshots'));
  return response.Workbench;
}
try {
  for(let n=0;n<200&&!fs.existsSync(path.join(data,'neko.sock'));n++) await pause();
  socket=net.connect(path.join(data,'neko.sock'));
  await new Promise((resolve,reject)=>{socket.once('connect',resolve);socket.once('error',reject);});
  socket.on('data',chunk=>{
    buffer=Buffer.concat([buffer,chunk]);
    while(buffer.length>=4&&buffer.length>=buffer.readUInt32LE(0)+4){
      const length=buffer.readUInt32LE(0),frame=JSON.parse(buffer.subarray(4,length+4));buffer=buffer.subarray(length+4);
      if(frame.Response){pending.get(frame.Response.id)?.(frame.Response.response);pending.delete(frame.Response.id);}
    }
  });
  const paseoOnly=await request({SetupImport:{Discover:{repositories:[]}}});
  assert.ok(paseoOnly.import_preview.repositories.includes(repository));
  assert.ok(paseoOnly.import_preview.candidates.some(c=>c.kind==='workspace' && c.workspace===repository));
  assert.ok(paseoOnly.import_preview.warnings.some(w=>w.includes('Paseo live state')));
  assert.ok(!fs.existsSync(sentinel), 'discovery must not execute the sentinel tool');
  const discovered=await request({SetupImport:{Discover:{repositories:[repository]}}});
  const preview=discovered.import_preview;
  assert.equal(discovered.workspaces.length,0);
  assert.equal(preview.connections.length,5);
  assert.ok(discovered.import_preview.preview_id);
  assert.ok(preview.connections.some(c=>c.source.includes('.claude.json')));
  assert.ok(preview.connections.some(c=>c.source.includes('.paseo') === false && c.repository === repository));
  assert.ok(preview.connections.some(c=>c.name==='sentinel' && !c.problem));
  assert.ok(preview.warnings.some(w=>w.includes('scheduled_tasks.json')));
  assert.equal(preview.candidates.filter(c=>c.kind==='skill').length,2);
  assert.ok(preview.candidates.some(c=>c.kind==='skill' && c.scope==='global'));
  assert.ok(preview.candidates.some(c=>c.kind==='skill' && c.scope.startsWith('workspace:')));
  assert.equal(preview.schedules.length,1);
  assert.ok(preview.candidates.every(c=>!JSON.stringify(c).includes('secret')));
  const workspaceIds=preview.candidates.filter(c=>c.kind==='workspace').map(c=>c.id);
  const skillIds=preview.candidates.filter(c=>c.kind==='skill').map(c=>c.id);
  const apply={SetupImport:{Apply:{preview_id:discovered.import_preview.preview_id,connection_ids:preview.connections.filter(c=>!c.problem && c.name!=='sentinel').map(c=>c.id),schedule_ids:preview.schedules.map(s=>s.id),skill_ids:skillIds,workspace_ids:workspaceIds,repositories:[],include_credentials:false,trust_local_processes:false}}};
  assert.ok(!fs.existsSync(sentinel), 'apply must not execute the sentinel tool');
  const imported=await request(apply);
  assert.equal(imported.workspaces.length,1); assert.equal(imported.mcp.connections.length,4);
  assert.equal(imported.mcp.connections.find(c=>c.label==='global').workspace_id,'');
  assert.equal(imported.mcp.connections.find(c=>c.label==='scoped').workspace_id,imported.workspaces[0].id);
  assert.equal(imported.mcp.connections.find(c=>c.label==='paused').enabled,false);
  assert.ok(imported.mcp.connections.every(c=>!c.has_credentials&&c.tools.length===0));
  assert.equal(imported.mcp.grants.length,0);
  assert.equal(imported.skills.proposals.length,2);
  assert.equal(imported.skills.enabled.length,0);
  assert.equal(imported.schedules.length,1);
  assert.equal(imported.schedules[0].enabled,false);
  assert.ok(!fs.existsSync(sentinel), 'import must not execute external tools');
  assert.equal((await request(apply)).mcp.connections.length,4);
  assert.equal((await request(apply)).skills.proposals.length,2);
  assert.equal((await request(apply)).schedules.length,1);
  console.log(`PASS import: Codex/Claude/Paseo metadata, redacted source/scope ledger, skills/workspace apply, disabled MCP/skills, paused schedule, no tool launch/grants, idempotent retry (${scratch})`);
} finally {
  socket?.destroy();
  if(daemon.exitCode===null){daemon.kill('SIGTERM');await new Promise(resolve=>daemon.once('exit',resolve));}
}
