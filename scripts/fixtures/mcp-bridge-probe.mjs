// A credential-free fixture: it never reads files or calls the network.
import readline from 'node:readline';
const lines = readline.createInterface({ input: process.stdin });
lines.on('line', line => {
  const request = JSON.parse(line);
  if (request.id === undefined) return;
  let result;
  if (request.method === 'initialize') result = { protocolVersion: '2025-06-18', capabilities: { tools: {} }, serverInfo: { name: 'neko-probe', version: '1' } };
  else if (request.method === 'tools/list') result = { tools: [{ name: 'probe', description: 'Return the fixed Neko bridge probe marker.', inputSchema: { type: 'object', properties: {}, additionalProperties: false } }] };
  else if (request.method === 'tools/call' && request.params.name === 'probe') result = { content: [{ type: 'text', text: 'NEKO_BRIDGE_PROBE_OK' }], isError: false };
  else if (request.method === 'ping') result = {};
  else { process.stdout.write(JSON.stringify({ jsonrpc: '2.0', id: request.id, error: { code: -32601, message: 'Unsupported probe operation' } }) + '\n'); return; }
  process.stdout.write(JSON.stringify({ jsonrpc: '2.0', id: request.id, result }) + '\n');
});
