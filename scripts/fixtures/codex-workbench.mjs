#!/usr/bin/env node
// Deterministic process fixture. Never contacts an AI service.
import fs from 'node:fs';
import { spawn, spawnSync, execFileSync } from 'node:child_process';
import readline from 'node:readline';
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
const args = process.argv.slice(2);
if (args[0] === 'app-server') {
  for await (const line of readline.createInterface({ input: process.stdin })) {
    const request = JSON.parse(line);
    if (request.id == null) continue;
    const result = request.method === 'account/read' ? { account: { type: 'chatgpt', planType: 'pro' } }
      : request.method === 'model/list' ? { data: [{ id: 'fixture', model: 'fixture', displayName: 'Fixture', isDefault: true,
          supportedReasoningEfforts: [{ reasoningEffort: 'low', description: 'Fixture low' }, { reasoningEffort: 'high', description: 'Fixture high' }],
          defaultReasoningEffort: 'low', serviceTiers: [{ id: 'priority', name: 'Fast' }], additionalSpeedTiers: ['fast'] }], nextCursor: null }
      : {};
    process.stdout.write(`${JSON.stringify({ id: request.id, result })}\n`);
  }
  process.exit(0);
}
assert.equal(args[0], 'exec');
assert.ok(args.includes('--ignore-user-config'));
const fullAccess = args.includes('--dangerously-bypass-approvals-and-sandbox');
assert.equal(args.includes('default_permissions="neko"'), !fullAccess, 'Full access must not be overridden by a restricted profile');
let prompt = '';
for await (const chunk of process.stdin) prompt += chunk;
assert.ok(args.includes('skills.include_instructions=false'), 'Neko must control injected skill instructions');
for (const feature of ['apps','browser_use','computer_use','plugins','remote_plugin','multi_agent','hooks','workspace_dependencies','skill_mcp_dependency_install']) assert.ok(args.includes(`features.${feature}=false`), `Ambient ${feature} bypasses Neko authority`);
if (prompt.startsWith('Select runtime settings for this Neko conversation.') || prompt.startsWith('This is a connection check.')) {
  assert.ok(args.includes('features.shell_tool=false'));
  assert.ok(!args.some(arg => arg.startsWith('mcp_servers.')));
  assert.deepEqual(fs.readdirSync(process.cwd()), []);
  const text = prompt.startsWith('This is a connection check.') ? 'NEKO_OK'
    : JSON.stringify({ candidate: 0, reasoning_effort: 'high', service_tier: 'default', reason: 'The fixture task needs investigation.' });
  process.stdout.write(`${JSON.stringify({type:'item.completed',item:{type:'agent_message',text}})}\n${JSON.stringify({type:'turn.completed'})}\n`);
  process.exit(0);
}
if (prompt.startsWith("Classify the user's latest ticket message.")) {
  for (const feature of ['shell_tool','unified_exec','view_image','code_mode_host']) assert.ok(args.includes(`features.${feature}=false`));
  assert.ok(args.includes('--ephemeral'));
  assert.ok(!args.some(arg => arg.startsWith('mcp_servers.')), 'Reply interpretation has no tools');
  assert.deepEqual(fs.readdirSync(process.cwd()), [], 'Reply interpretation uses an empty scratch folder');
  const message = JSON.parse(prompt.split('User message (JSON string): ')[1]);
  const intent = message === 'Investigate the cause and fix it locally.' ? 'work'
    : message === 'Explain the cause only. Do not change anything.' ? 'read_only' : 'context';
  process.stdout.write(`${JSON.stringify({type:'item.completed',item:{type:'agent_message',text:JSON.stringify({intent})}})}\n${JSON.stringify({type:'turn.completed'})}\n`);
  process.exit(0);
}
if (prompt.startsWith('Extract bounded memory proposals')) {
  for (const feature of ['shell_tool', 'unified_exec', 'view_image', 'image_generation', 'skill_search', 'tool_suggest', 'sleep_tool', 'apps', 'browser_use', 'computer_use', 'remote_plugin', 'plugins', 'goals', 'hooks', 'workspace_dependencies', 'code_mode_host', 'multi_agent', 'memories', 'skill_mcp_dependency_install']) assert.ok(args.includes(`features.${feature}=false`), `Extraction must disable ${feature}`);
  assert.ok(args.includes('features.skip_host_skill_discovery=true'));
  assert.ok(args.some(arg => arg.startsWith('permissions.neko={extends=":read-only"')));
  assert.ok(args.includes('--ephemeral'));
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
const coordinator = prompt.includes("Neko's coordinator");
if (coordinator) {
  assert.ok(args.includes('--ephemeral'), 'Recovery supervisor is independent');
  assert.equal(fullAccess, !prompt.includes('Phase: read-only planning'));
  if (prompt.includes('COORDINATOR_HOLD')) await new Promise(resolve=>setTimeout(resolve,9000));
  if (prompt.includes('COORDINATOR_MUTATES')) fs.writeFileSync('unexpected.txt', 'Supervisor changed source');
  const decision = prompt.includes('REAL_DECISION')
    ? {action:'ask_user',reason:'Inspected the available configuration; two deployments match.',question:'Which deployment is in scope?'}
    : {action:'retry',reason:'The available repository conventions give a concrete recovery step.',instructions:'SUPERVISOR_RECOVERY: resolve the local test prerequisites and Broken builder output, preserve scope, run the regression check.'};
  process.stdout.write(`${JSON.stringify({type:'item.completed',item:{type:'agent_message',text:JSON.stringify(decision)}}) }\n${JSON.stringify({type:'turn.completed'})}\n`);
  process.exit(0);
}
const splitter = prompt.includes("Neko's splitter");
if (scout || supervisor || splitter || responsibility) assert.equal(fullAccess, false);
if (responsibility || review) assert.ok(args.includes('--ephemeral'), 'Background checks and reviews must stay fresh');
if (!args.includes('--ephemeral')) {
  // Session persistence is simulated; the real daemon must retain and resume
  // this id between planning and building. The fixture never contacts a model.
  const resumed = args[1] === 'resume';
  const ids = args.filter(arg => /^[a-f0-9]{8}(?:-[a-f0-9]{4}){3}-[a-f0-9]{12}$/.test(arg));
  assert.equal(ids.length, resumed ? 1 : 0);
  process.stdout.write(`${JSON.stringify({type:'thread.started',thread_id:resumed ? ids[0] : randomUUID()})}\n`);
}
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
if (!responsibility && !scout && !supervisor && !review) {
  assert.equal(fullAccess, true, 'Authorized builder has full local access, including resume');
  if (prompt.includes('WORKER_ERROR') && !prompt.includes('SUPERVISOR_RECOVERY')) throw new Error('Transient worker startup failure');
  const repair = prompt.includes('REPAIR_REQUIRED');
  if (prompt.includes('ENVIRONMENT_RECOVERY') && prompt.includes('SUPERVISOR_RECOVERY')) {
    fs.mkdirSync('.fixture-cache', {recursive:true});
    fs.writeFileSync('.fixture-cache/ready', 'setup complete');
  }
  const receivedFinding = prompt.includes('SUPERVISOR_RECOVERY') && prompt.includes('Broken builder output');
  if (prompt.includes('REPAIR_HOLD') && receivedFinding) await new Promise(resolve=>setTimeout(resolve,9000));
  fs.writeFileSync(outputFile, prompt.includes('BROKEN_BUILDER') || (repair && !receivedFinding) ? 'broken\n' : 'Isolated task output\n');
  if (prompt.includes('COMMITTED_REPAIR') && !fs.existsSync('committed.txt')) {
    fs.writeFileSync('committed.txt', 'Isolated task output\n');
    execFileSync('git', ['add', outputFile, 'committed.txt']);
    execFileSync('git', ['-c','core.hooksPath=/dev/null','-c','user.name=Neko Test','-c','user.email=test@example.invalid','commit','-qm','Builder fixture commit']);
  }
}
let verdict;
if (review) {
  assert.equal(fullAccess, true, 'Reviewer can write dependencies, caches and temp files');
  if (prompt.includes('REVIEWER_MUTATES')) fs.writeFileSync(outputFile, 'reviewer replaced builder output\n');
  fs.mkdirSync('.fixture-cache', {recursive:true});
  fs.writeFileSync('.fixture-cache/test-output', 'ignored test output');
  const base = prompt.match(/Host-observed diff base: ([a-f0-9]+)/)?.[1] || 'HEAD';
  const files = [...new Set((execFileSync('git',['diff','--name-only',base],{encoding:'utf8'}) + execFileSync('git',['ls-files','--others','--exclude-standard'],{encoding:'utf8'})).trim().split('\n').filter(Boolean))];
  if (prompt.includes('COMMITTED_REPAIR')) assert.ok(files.includes('committed.txt'), 'Every review must include unchanged committed builder edits');
  const check = spawnSync(process.execPath, ['-e', 'const fs=require("fs");for(const f of process.argv.slice(1)){if(fs.readFileSync(f,"utf8")!=="Isolated task output\\n")throw Error("broken builder output: "+f)}console.log("Verified "+process.argv.slice(1).join(", "))', ...files], {encoding:'utf8'});
  const command = prompt.includes('PARENT_MISSING_CHECK') ? 'node unrelated-check' : 'node fixture-check';
  process.stdout.write(`${JSON.stringify({type:'item.completed',item:{type:'command_execution',command,status:check.status===0?'completed':'failed',exit_code:check.status,aggregated_output:check.stdout+check.stderr}})}\n`);
  verdict = {passed:check.status===0,findings:check.status===0?[]:['Broken builder output'],files,tests:[command],summary:check.status===0?'Actual isolated files checked by independent child process':'Independent check failed'};
  if (prompt.includes('ENVIRONMENT_RECOVERY') && !fs.existsSync('.fixture-cache/ready')) {
    verdict.passed = false; verdict.findings = ['Missing test dependency'];
  }
  if (prompt.includes('REVIEWER_MUTATES')) { verdict.passed = true; verdict.findings = []; }
}
const text = responsibility ? JSON.stringify(await observeThroughBridge()) : supervisor ? JSON.stringify({
  action: prompt.includes('Sensitive fixture') ? 'ask_user' : 'prepare_fix',
  risk: prompt.includes('Sensitive fixture') ? 'high' : 'low',
  is_bug: true, reason: 'Deterministic fixture assessment, not a real model judgment.',
  evidence: ['Fixture repository missing its expected output'], files: ['neko-smoke.txt'],
  tests: ['node fixture-check'],
  sensitive_areas: prompt.includes('Sensitive fixture') ? ['authentication'] : [], uncertainties: [],
  plan: 'Add neko-smoke.txt with the exact requested text and verify its bytes.'
}) : scout ? (prompt.includes('SCOUT_QUESTION') && !prompt.includes('SUPERVISOR_RECOVERY')
    ? 'QUESTION: Please investigate the local test setup for me.'
    : 'Plan: add the isolated smoke file, then verify its contents.')
  : review ? JSON.stringify(verdict)
  : 'Created neko-smoke.txt in the task worktree.';
process.stdout.write(`${JSON.stringify({ type: 'item.completed', item: { type: 'agent_message', text } })}\n`);
process.stdout.write(`${JSON.stringify({ type: 'turn.completed' })}\n`);
