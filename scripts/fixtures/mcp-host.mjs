// Isolated MCP fixture. No packages, network access, or user data.
import readline from 'node:readline';
import { spawn } from 'node:child_process';
const mode = process.argv[2] ?? 'normal';
const descendant = mode === 'descendant' ? spawn(process.execPath, ['-e', 'setInterval(() => {}, 1000)'], { stdio: 'ignore' }) : null;
const reply = (id, result) => process.stdout.write(JSON.stringify({ jsonrpc: '2.0', id, result }) + '\n');
readline.createInterface({ input: process.stdin }).on('line', (line) => {
  const request = JSON.parse(line);
  if (request.id === undefined) return;
  if (mode === 'hang') return;
  if (request.method === 'initialize' || request.method === 'discovery') {
    reply(request.id, { protocolVersion: '2025-03-26', capabilities: { tools: {} }, serverInfo: { name: 'bounded-fixture', version: '1.0' } });
  } else if (request.method === 'tools/list') {
    const tool = { name: 'echo', description: mode === 'changed' ? 'Changed description' : 'Echo arguments', inputSchema: { type: 'object', properties: { text: { type: 'string' } } } };
    if (mode === 'readonly' || mode === 'work') tool.annotations = { readOnlyHint: true };
    if (mode === 'work') tool.name = 'list_issues';
    if (mode === 'schema') tool.inputSchema.description = 'x'.repeat(33000);
    const tools = mode === 'many' ? Array.from({ length: 129 }, (_, i) => ({ ...tool, name: `tool${i}` })) : [tool];
    if (mode === 'pages') { tools[0].name = `page${request.params?.cursor ?? 0}`; reply(request.id, { tools, nextCursor: String(Number(request.params?.cursor ?? 0) + 1) }); }
    else reply(request.id, { tools });
  } else if (request.method === 'tools/call') {
    if (mode === 'error') process.stdout.write(JSON.stringify({ jsonrpc: '2.0', id: request.id, error: { code: -32000, message: 'secret-token-from-server' } }) + '\n');
    else reply(request.id, { content: [{ type: 'text', text: mode === 'oversize' ? 'x'.repeat(300000) : JSON.stringify({ arguments: request.params.arguments, inherited: process.env.HOME ?? null, explicit: process.env.FIXTURE_VALUE ?? null, pid: process.pid, descendant: descendant?.pid ?? null }) }] });
  } else process.stdout.write(JSON.stringify({ jsonrpc: '2.0', id: request.id, error: { code: -32601, message: 'Method not found' } }) + '\n');
});
