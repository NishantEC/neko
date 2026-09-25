#!/usr/bin/env node
// Disposable service-neutral MCP fixture. No network, credentials or database access.
import readline from 'node:readline';
const namespace = process.argv[2];
if (!['alpha', 'beta'].includes(namespace)) process.exit(2);
const send = (id, result) => process.stdout.write(`${JSON.stringify({ jsonrpc: '2.0', id, result })}\n`);
for await (const line of readline.createInterface({ input: process.stdin })) {
  const request = JSON.parse(line);
  if (request.id === undefined) continue;
  if (request.method === 'initialize') send(request.id, {
    protocolVersion: request.params.protocolVersion, capabilities: { tools: {} }, serverInfo: { name: `fixture-${namespace}`, version: '1' },
  });
  else if (request.method === 'tools/list') send(request.id, { tools: [{
    name: 'assigned_changes', description: 'Read two assigned, current fixture bugs. This tool never changes files.',
    inputSchema: { type: 'object', properties: {}, additionalProperties: false },
  }] });
  else if (request.method === 'tools/call' && request.params.name === 'assigned_changes' && Object.keys(request.params.arguments ?? {}).length === 0) send(request.id, {
    content: [{ type: 'text', text: JSON.stringify({ namespace, items: ['Low-risk fixture', 'Sensitive fixture'].map((title, index) => ({
      external_id: `${namespace}-${index}`, revision: 'revision-1', title,
      description: `Assigned actionable bug. Prepare the isolated fixture output in neko-smoke.txt. ${index ? 'Authentication behavior requires explicit approval.' : 'Only a local text fixture is missing.'}`,
      assigned: true, actionable: true,
    })) }) }], isError: false,
  });
  else if (request.method === 'ping') send(request.id, {});
  else process.stdout.write(`${JSON.stringify({ jsonrpc: '2.0', id: request.id, error: { code: -32601, message: 'Unsupported fixture request' } })}\n`);
}
