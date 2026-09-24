#!/usr/bin/env node
// Deterministic process fixture. Never contacts an AI service.
import fs from 'node:fs';
import { spawn, spawnSync, execFileSync } from 'node:child_process';
import readline from 'node:readline';
import assert from 'node:assert/strict';
const args = process.argv.slice(2);
if (!args.includes('--ephemeral') || !args.includes('--ignore-user-config')) process.exit(2);
let prompt = '';
for await (const chunk of process.stdin) prompt += chunk;
assert.ok(args.includes('skills.include_instructions=false'), 'Neko must control injected skill instructions');
for (const feature of ['apps','browser_use','computer_use','plugins','remote_plugin','multi_agent','hooks','workspace_dependencies','skill_mcp_dependency_install']) assert.ok(args.includes(`features.${feature}=false`), `Ambient ${feature} bypasses Neko authority`);
if (prompt.startsWith('Extract bounded memory proposals')) {
  for (const feature of ['shell_tool', 'unified_exec', 'view_image', 'image_generation', 'skill_search', 'tool_suggest', 'sleep_tool', 'apps', 'browser_use', 'computer_use', 'remote_plugin', 'plugins', 'goals', 'hooks', 'workspace_dependencies', 'code_mode_host', 'multi_agent', 'memories', 'skill_mcp_dependency_install']) assert.ok(args.includes(`features.${feature}=false`), `Extraction must disable ${feature}`);
  assert.ok(args.includes('features.skip_host_skill_discovery=true'));
  assert.ok(args.includes('read-only'));
  assert.ok(!args.some(arg => arg.startsWith('mcp_servers.')), 'Learning must never get an MCP bridge');
  assert.deepEqual(fs.readdirSync(process.cwd()), [], 'Learning requires an independent empty scratch directory');
  const probe = prompt.includes('MEMORY_LEARNING_PROBE');
  const memories = probe ? [{text:'Bounded verification batches were useful for this work.',kind:'workspace'},{text:'Keep verification evidence attached to the work.',kind:'decision'}] : [];
  process.stdout.write(`${JSON.stringify({type:'item.completed',item:{type:'agent_message',text:JSON.stringify({memories})}})}\n${JSON.stringify({type:'turn.completed'})}\n`);
  process.exit(0);
}
if (prompt.startsWith('Extract one reusable skill')) {
  process.stdout.write(`${JSON.stringify({ type: 'item.completed', item: { type: 'agent_message', text: '---\nname: isolated-checkout\ndescription: Verify an isolated checkout before accepting work\n---\nRun the relevant tests and inspect the actual diff before accepting the result.' } })}\n`);
  process.stdout.write(`${JSON.stringify({ type: 'turn.completed' })}\n`);
  process.exit(0);
}
const responsibility = prompt.includes("Investigate the user's standing responsibility");
assert.ok(!args.includes('features.code_mode_host=false'), 'Ordinary scoped tools require code-mode host transport');
const chat = prompt.includes("You are Neko, the user's personal engineering agent");
if (chat) {
  // Read-only chat turn: never touches files. Asking for a fix proposes one ticket.
  const message = prompt.slice(prompt.lastIndexOf('\nUser: ') + 7).split('\n')[0];
  if (message.startsWith('PROFILE_PROMPT_CHECK ')) {
    const check = JSON.parse(message.slice('PROFILE_PROMPT_CHECK '.length));
    // Earlier test requests name forbidden sentinels themselves. Inspect the
    // actual injected profile/state blocks, not those user-authored probes.
    const context = prompt.slice(0, prompt.indexOf('\n\nRecent conversation'));
    for (const value of check.present) assert.ok(context.includes(value), `Missing profile context: ${value}`);
    for (const value of check.absent) assert.ok(!context.includes(value), `Profile context leaked: ${value}`);
    process.stdout.write(`${JSON.stringify({ type: 'item.completed', item: { type: 'agent_message', text: JSON.stringify({ reply: 'Profile context verified.', tickets: [] }) } })}\n`);
    process.stdout.write(`${JSON.stringify({ type: 'turn.completed' })}\n`);
    process.exit(0);
  }
  if (message.includes('SKILL_PROMPT_CHECK')) {
    assert.ok(prompt.includes('SKILL_PROBE_SENTINEL'), 'Enabled skill never reached chat runner');
    process.stdout.write(`${JSON.stringify({ type: 'item.completed', item: { type: 'agent_message', text: JSON.stringify({ reply: 'Enabled skill reached chat.', tickets: [] }) } })}\n`);
    process.stdout.write(`${JSON.stringify({ type: 'turn.completed' })}\n`);
    process.exit(0);
  }
  if (message.includes('CHAT_TOOL_PROBE')) {
    const result = await observeThroughBridge();
    process.stdout.write(`${JSON.stringify({ type: 'item.completed', item: { type: 'agent_message', text: JSON.stringify({ reply: result.summary, tickets: [] }) } })}\n`);
    process.stdout.write(`${JSON.stringify({ type: 'turn.completed' })}\n`);
    process.exit(0);
  }
  const wantsWork = /\b(fix|investigate|review|build)\b/i.test(message);
  const remembers = /\b(always|remember|prefer|decided)\b/i.test(message);
  const reply = remembers
    ? { reply: "Got it, I'll remember that.", tickets: [], remember: [{ text: message.trim(), workspace_id: null, decision: /\bdecided\b/i.test(message) }] }
    : wantsWork
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
    if (chat && prompt.slice(prompt.lastIndexOf('\nUser: ')).includes('CHAT_TOOL_DENY')) {
      assert.equal(call.isError, true, 'Denied chat action unexpectedly executed');
      return { summary: 'Chat action denied without execution.', observations: [] };
    }
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
const splitter = prompt.includes("Neko's splitter");
if (prompt.includes('SKILL_ROLE_CHECK')) assert.ok(prompt.includes('SKILL_PROBE_SENTINEL'), 'Enabled skill never reached ticket role');
if (splitter) {
  const conflict = prompt.includes('SPLIT_CONFLICT');
  const plans = ['left', 'right'].map((name, i) => ({title: `Subtask ${name}`, goal:`Add the verified fixture file. SUBTASK_FILE=${conflict ? 'shared' : name}.txt${i===0 && prompt.includes('SPLIT_DEPENDENCY_FAILURE') ? ' BROKEN_BUILDER' : ''}${prompt.includes('SPLIT_HOLD') ? ' PARALLEL_HOLD' : ''}`, files:[`${conflict ? 'shared' : name}.txt`], tests:['node fixture-check'], depends_on:i===1 && prompt.includes('SPLIT_DEPEND')?[0]:[]}));
  process.stdout.write(`${JSON.stringify({type:'item.completed',item:{type:'agent_message',text:JSON.stringify(plans)}})}\n${JSON.stringify({type:'turn.completed'})}\n`);
  process.exit(0);
}
if (scout && fs.existsSync('neko-smoke.txt')) throw new Error('Scout changed source');
const outputFile = prompt.match(/SUBTASK_FILE=([a-z]+\.txt)/)?.[1] || 'neko-smoke.txt';
if (!responsibility && !scout && !supervisor && !review && prompt.includes('PARALLEL_HOLD')) await new Promise(resolve=>setTimeout(resolve,9000));
if (!responsibility && !scout && !supervisor && !review) fs.writeFileSync(outputFile, prompt.includes('BROKEN_BUILDER') ? 'broken\n' : 'Isolated task output\n');
let verdict;
if (review) {
  const files = [...new Set((execFileSync('git',['diff','--name-only','HEAD'],{encoding:'utf8'}) + execFileSync('git',['ls-files','--others','--exclude-standard'],{encoding:'utf8'})).trim().split('\n').filter(Boolean))];
  const check = spawnSync(process.execPath, ['-e', 'const fs=require("fs");for(const f of process.argv.slice(1)){if(fs.readFileSync(f,"utf8")!=="Isolated task output\\n")throw Error("broken builder output: "+f)}console.log("Verified "+process.argv.slice(1).join(", "))', ...files], {encoding:'utf8'});
  const command = prompt.includes('PARENT_MISSING_CHECK') ? 'node unrelated-check' : 'node fixture-check';
  process.stdout.write(`${JSON.stringify({type:'item.completed',item:{type:'command_execution',command,status:check.status===0?'completed':'failed',exit_code:check.status,aggregated_output:check.stdout+check.stderr}})}\n`);
  verdict = {passed:check.status===0,findings:check.status===0?[]:['Broken builder output'],files,tests:[command],summary:check.status===0?'Actual isolated files checked by independent child process':'Independent check failed'};
}
const text = responsibility ? JSON.stringify(await observeThroughBridge()) : supervisor ? JSON.stringify({
  action: prompt.includes('Sensitive fixture') ? 'ask_user' : 'prepare_fix',
  risk: prompt.includes('Sensitive fixture') ? 'high' : 'low',
  is_bug: true, reason: 'Deterministic fixture assessment, not a real model judgment.',
  evidence: ['Fixture repository missing its expected output'], files: ['neko-smoke.txt'],
  tests: ['node fixture-check'],
  sensitive_areas: prompt.includes('Sensitive fixture') ? ['authentication'] : [], uncertainties: [],
  plan: 'Add neko-smoke.txt with the exact requested text and verify its bytes.'
}) : scout ? 'Plan: add the isolated smoke file, then verify its contents.'
  : review ? JSON.stringify(verdict)
  : 'Created neko-smoke.txt in the task worktree.';
process.stdout.write(`${JSON.stringify({ type: 'item.completed', item: { type: 'agent_message', text } })}\n`);
process.stdout.write(`${JSON.stringify({ type: 'turn.completed' })}\n`);
