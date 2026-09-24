#!/usr/bin/env node
// Deterministic process fixture. Never contacts an AI service.
import fs from 'node:fs';
import { spawn } from 'node:child_process';
import readline from 'node:readline';
import assert from 'node:assert/strict';
const args = process.argv.slice(2);
if (!args.includes('--ephemeral') || !args.includes('--ignore-user-config')) process.exit(2);
let prompt = '';
for await (const chunk of process.stdin) prompt += chunk;
const responsibility = prompt.includes("Investigate the user's standing responsibility");
const chat = prompt.includes("You are Neko, the user's personal engineering agent");
if (chat) {
  // Read-only chat turn: never touches files. Asking for a fix proposes one ticket.
  const message = prompt.slice(prompt.lastIndexOf('\nUser: ') + 7).split('\n')[0];
  const wantsWork = /\b(fix|investigate|review|build)\b/i.test(message);
  const reply = wantsWork
    ? { reply: 'On it. I opened a ticket and will plan it read-only first.', tickets: [{ title: 'Fix the empty cart total crash', goal: 'Checkout must show 0 for carts with only free items. Add a test for a free-only cart and run the checkout tests.' }] }
    : { reply: 'Quiet so far. Nothing is blocked, and I will tell you when something needs you.', tickets: [] };
  process.stdout.write(`${JSON.stringify({ type: 'item.completed', item: { type: 'agent_message', text: JSON.stringify(reply) } })}\n`);
  process.stdout.write(`${JSON.stringify({ type: 'turn.completed' })}\n`);
  process.exit(0);
}
async function observeThroughBridge() {
  const config = args.find(arg => arg.startsWith('mcp_servers.neko='));
  const command = config?.match(/command=("(?:[^"\\]|\\.)*")/);
  assert.ok(command, 'Responsibility requires the real configured bridge');
  assert.ok(process.env.NEKO_MCP_TOKEN && process.env.NEKO_MCP_SOCKET, 'Scoped bridge capability missing');
  const bridge = spawn(JSON.parse(command[1]), ['--mcp-bridge'], { env: process.env, stdio: ['pipe', 'pipe', 'pipe'] });
  const pending = new Map(); let next = 1;
  bridge.stderr.resume();
  const lines = readline.createInterface({ input: bridge.stdout });
  lines.on('line', line => {
    const response = JSON.parse(line);
    if (response.id !== undefined) pending.get(response.id)?.(response);
  });
  const rpc = (method, params) => new Promise((resolve, reject) => {
    const id = next++;
    const timeout = setTimeout(() => { pending.delete(id); reject(new Error('Fixture bridge request timed out')); }, 12000);
    pending.set(id, response => {
      clearTimeout(timeout); pending.delete(id);
      if (response.error) reject(new Error(JSON.stringify(response.error))); else resolve(response.result);
    });
    bridge.stdin.write(`${JSON.stringify({ jsonrpc: '2.0', id, method, params })}\n`);
  });
  try {
    await rpc('initialize', { protocolVersion: '2025-11-25', capabilities: {}, clientInfo: { name: 'neko-smoke-agent', version: '1' } });
    bridge.stdin.write(`${JSON.stringify({ jsonrpc: '2.0', method: 'notifications/initialized' })}\n`);
    const list = await rpc('tools/call', { name: 'neko_list_tools', arguments: {} });
    assert.ok(!list.isError, 'Bridge catalog failed');
    const tools = JSON.parse(list.content.find(item => item.type === 'text').text);
    const foreign = prompt.match(/FOREIGN_CONNECTION_ID=([a-zA-Z0-9_-]+)/)?.[1];
    assert.ok(foreign, 'Scope probe must name the other workspace connection');
    assert.ok(tools.every(tool => tool.connection_id !== foreign && tool.connection_label === 'Workspace A fixture'), 'Cross-workspace catalog leak');
    const denied = await rpc('tools/call', { name: 'neko_call_tool', arguments: { connection_id: foreign, tool_name: 'assigned_changes', arguments: {} } });
    assert.equal(denied.isError, true, 'Cross-workspace call unexpectedly authorized');
    if (tools.length === 0) return { summary: 'Fixture blocked: no granted tools; foreign scope denied.', observations: [] };
    assert.equal(tools.length, 1);
    const tool = tools[0];
    const call = await rpc('tools/call', { name: 'neko_call_tool', arguments: { connection_id: tool.connection_id, tool_name: tool.tool_name, arguments: {} } });
    assert.ok(!call.isError, 'Granted fixture call failed');
    const receipt = JSON.parse(call.content.find(item => item.type === 'text').text);
    assert.ok(receipt.receipt_id, 'Real daemon receipt missing');
    const result = typeof receipt.result === 'string' ? JSON.parse(receipt.result) : receipt.result;
    assert.ok(!result.isError, 'Fixture server returned an error');
    const source = JSON.parse(result.content.find(item => item.type === 'text').text);
    assert.equal(source.namespace, 'alpha');
    return { summary: 'Fixture checked real MCP tools; foreign scope denied.', observations: source.items.map(item => ({
      connection_id: tool.connection_id, external_id: item.external_id, revision: item.revision,
      title: item.title, description: item.description, receipt_ids: [receipt.receipt_id], eligible: item.assigned && item.actionable,
    })) };
  } finally {
    lines.close(); bridge.stdin.end();
    if (bridge.exitCode === null) {
      await new Promise(resolve => { const timeout = setTimeout(() => { bridge.kill('SIGKILL'); resolve(); }, 1000); bridge.once('exit', () => { clearTimeout(timeout); resolve(); }); });
    }
  }
}
const scout = prompt.includes("Neko's scout");
const supervisor = prompt.includes("Neko's supervisor");
const review = prompt.includes("Neko's reviewer");
if (scout && fs.existsSync('neko-smoke.txt')) throw new Error('Scout changed source');
if (!responsibility && !scout && !supervisor && !review) fs.writeFileSync('neko-smoke.txt', 'Isolated task output\n');
if (review && !fs.existsSync('neko-smoke.txt')) throw new Error('Builder output missing');
const text = responsibility ? JSON.stringify(await observeThroughBridge()) : supervisor ? JSON.stringify({
  action: prompt.includes('Sensitive fixture') ? 'ask_user' : 'prepare_fix',
  risk: prompt.includes('Sensitive fixture') ? 'high' : 'low',
  is_bug: true, reason: 'Deterministic fixture assessment, not a real model judgment.',
  evidence: ['Fixture repository missing its expected output'], files: ['neko-smoke.txt'],
  tests: ['Read neko-smoke.txt and compare exactly with Isolated task output followed by newline'],
  sensitive_areas: prompt.includes('Sensitive fixture') ? ['authentication'] : [], uncertainties: [],
  plan: 'Add neko-smoke.txt with the exact requested text and verify its bytes.'
}) : scout ? 'Plan: add the isolated smoke file, then verify its contents.'
  : review ? 'Reviewed actual task output. Fixture check passed; no publication.'
  : 'Created neko-smoke.txt in the task worktree.';
process.stdout.write(`${JSON.stringify({ type: 'item.completed', item: { type: 'agent_message', text } })}\n`);
process.stdout.write(`${JSON.stringify({ type: 'turn.completed' })}\n`);
