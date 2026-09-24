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
fs.writeFileSync(path.join(scratch, '.codex/config.toml'), `[mcp_servers.global]\nurl="https://example.org/mcp"\nbearer_token_env_var="NEKO_IMPORT_TEST_TOKEN"\n[mcp_servers.paused]\nurl="https://example.org/paused"\nenabled=false\n[projects.${JSON.stringify(repository)}]\ntrust_level="trusted"\n`);
fs.writeFileSync(path.join(repository, '.codex/config.toml'), '[mcp_servers.scoped]\nurl="https://example.org/scoped"\n');
const daemon = spawn(path.join(root, 'target/debug/neko-daemon'), [], {env:{...process.env, HOME:scratch, NEKO_DATA_DIR:data, NEKO_IMPORT_TEST_TOKEN:'fixture-secret-not-for-snapshots'},stdio:['ignore','ignore','pipe']});
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
  const discovered=await request({SetupImport:{Discover:{repositories:[]}}});
  assert.equal(discovered.workspaces.length,0);
  assert.equal(discovered.import_preview.connections.length,3);
  assert.ok(discovered.import_preview.preview_id);
  const apply={SetupImport:{Apply:{preview_id:discovered.import_preview.preview_id,connection_ids:discovered.import_preview.connections.map(c=>c.id),repositories:[repository],include_credentials:false,trust_local_processes:false}}};
  const imported=await request(apply);
  assert.equal(imported.workspaces.length,1); assert.equal(imported.mcp.connections.length,3);
  assert.equal(imported.mcp.connections.find(c=>c.label==='global').workspace_id,'');
  assert.equal(imported.mcp.connections.find(c=>c.label==='scoped').workspace_id,imported.workspaces[0].id);
  assert.equal(imported.mcp.connections.find(c=>c.label==='paused').enabled,false);
  assert.ok(imported.mcp.connections.every(c=>!c.has_credentials&&c.tools.length===0));
  assert.equal(imported.mcp.grants.length,0);
  assert.equal((await request(apply)).mcp.connections.length,3);
  console.log(`PASS import: fresh preview, secret redaction, workspace/global scope, disabled source, no grants, idempotent retry (${scratch})`);
} finally {
  socket?.destroy();
  if(daemon.exitCode===null){daemon.kill('SIGTERM');await new Promise(resolve=>daemon.once('exit',resolve));}
}
