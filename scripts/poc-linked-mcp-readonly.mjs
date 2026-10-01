#!/usr/bin/env node
// Existing-source discovery through the actual Neko daemon. No auth tokens are
// read, copied, logged, or embedded; no model or external mutating tool runs.
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import net from 'node:net';
import {spawn} from 'node:child_process';
import {fileURLToPath} from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const scratch = fs.mkdtempSync(path.join(os.tmpdir(), 'neko-linked-readonly-'));
const daemon = spawn(process.env.NEKO_TEST_DAEMON || path.join(root, 'target/debug/neko-daemon'), [], {
  env: {...process.env, NEKO_DATA_DIR: scratch, NEKO_LEGACY_AGENTS: '0'},
  stdio: ['ignore', 'ignore', 'ignore'],
});
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
const result = {boundary: 'Real daemon source discovery, source linking and MCP tools/list only; no OAuth import, model call or issue mutation.', scratch, candidates: [], discoveries: []};
function request(request) {
  return new Promise((resolve, reject) => {
    const socket = net.connect(path.join(scratch, 'neko.sock'));
    const timer = setTimeout(() => { socket.destroy(); reject(new Error('IPC deadline')); }, 45000);
    let buffer = Buffer.alloc(0);
    socket.on('error', () => { clearTimeout(timer); socket.destroy(); reject(new Error('IPC unavailable')); });
    socket.on('connect', () => {
      const bytes = Buffer.from(JSON.stringify({Request: {id: 1, request}}));
      const header = Buffer.alloc(4); header.writeUInt32LE(bytes.length);
      socket.write(Buffer.concat([header, bytes]));
    });
    socket.on('data', data => {
      buffer = Buffer.concat([buffer, data]);
      while (buffer.length >= 4 && buffer.length >= buffer.readUInt32LE(0) + 4) {
        const length = buffer.readUInt32LE(0);
        const frame = JSON.parse(buffer.subarray(4, length + 4));
        buffer = buffer.subarray(length + 4);
        if (frame.Response?.id === 1) { clearTimeout(timer); socket.destroy(); resolve(frame.Response.response); return; }
      }
    });
  });
}
async function command(command) {
  const response = await request({Workbench: command});
  if (!response.Workbench) throw new Error('Daemon rejected command');
  return response.Workbench;
}
try {
  let ready = false;
  for (let i = 0; i < 100; i++) {
    if (daemon.exitCode !== null) throw new Error('Daemon startup failed');
    try { if (await request('Ping') === 'Pong') { ready = true; break; } } catch {}
    await delay(100);
  }
  if (!ready) throw new Error('Daemon did not become ready');
  const created = await command({SaveWorkspaceWithFolders: {workspace: {
    id: '', name: 'Read-only integration proof', repository: root, instructions: 'Read-only MCP inspection. No tasks, writes, messages, publication or background work.', away_enabled: false,
  }, folders: [root]}});
  const workspace = created.workspaces[0].id;
  const preview = await command({SetupImport: {Discover: {repositories: [root], source_id: null}}});
  const candidates = preview.import_preview.candidates.filter(c => c.kind === 'connection' && /linear|slack/i.test(c.name));
  result.candidates = candidates.map(c => ({name: c.name, source: c.source, scope: c.scope, has_credentials: c.metadata.has_credentials === 'true', available: !c.problem}));
  const candidate = candidates.find(c => c.name === (process.env.NEKO_POC_CONNECTION || 'linear-personal') && !c.problem);
  if (!candidate) throw new Error('Selected existing integration is not available');
  const linked = await command({Mcp: {LinkSource: {workspace_id: workspace, candidate_id: candidate.id, trust_local_process: false}}});
  const connection = linked.mcp.connections.find(c => c.source_link?.candidate_id === candidate.id);
  if (!connection) throw new Error('Source link missing');
  const discovered = await command({Mcp: {Discover: {connection_id: connection.id}}});
  const checked = discovered.mcp.connections.find(c => c.id === connection.id);
  result.discoveries.push({name: checked.label, linked: true, oauth: checked.oauth, has_credentials: checked.has_credentials, tool_names: checked.tools.map(t => t.name), discovery_failed: Boolean(checked.error), error: checked.error});
  result.authenticated = checked.tools.length > 0 && !checked.error;
  result.issue_query_performed = false;
  result.external_writes = 0;
  result.task_count = discovered.tasks.length;
} catch (error) {
  result.failed_stage = error.message;
  process.exitCode = 1;
} finally {
  if (daemon.exitCode === null) {
    await new Promise(resolve => {
      const timer = setTimeout(() => daemon.kill('SIGKILL'), 2000);
      daemon.once('exit', () => { clearTimeout(timer); resolve(); });
      daemon.kill('SIGTERM');
    });
  }
  // Scratch is retained for inspection. It contains sanitized source metadata,
  // no copied OAuth credentials, and no scheduled responsibilities.
  console.log(JSON.stringify(result, null, 2));
}
