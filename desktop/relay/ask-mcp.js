#!/usr/bin/env node
'use strict';
/* A tiny MCP server (stdio) that gives Claude a tool to ask YOU a question with options.
   Claude's built-in question tool does not work in headless mode, so chats started from the apps use this one.
   It forwards the question to the relay, which shows it on your phone and computer, and returns your answer. */
const http = require('http');
const PORT = Number(process.env.CM_RELAY_PORT) || 47601, JOB = process.env.CM_JOB || '';
const TOOL = {
  name: 'ask_user',
  description: 'Ask the user a question and wait for their answer. Use this whenever you need the user to choose between options or decide something, instead of writing the options in your reply. Give 2-4 short options; the user can also type their own answer.',
  inputSchema: { type: 'object', properties: {
    question: { type: 'string', description: 'The question, one sentence.' },
    options: { type: 'array', items: { type: 'object', properties: { label: { type: 'string' }, description: { type: 'string' } }, required: ['label'] }, description: 'The choices.' },
    multiSelect: { type: 'boolean', description: 'True if more than one option may be chosen.' } }, required: ['question'] },
};
const HUB = process.env.CM_HUB === '1';   // chats on the server also get tools to hand work to your laptop
const HUB_TOOLS = [
  { name: 'laptop_status', description: "Check whether the user's own laptop is online right now and which tasks are already waiting for it.", inputSchema: { type: 'object', properties: {} } },
  { name: 'run_on_laptop', description: "Hand a task to the user's own laptop. It runs there in this same project, continuing this conversation, as soon as the laptop is online (right away if it is online now; otherwise when the user next opens it). Use it for anything that must happen on the laptop itself: installing dependencies or tools, running or testing the app, using hardware, a GUI, or files that only exist there. Write the task as complete instructions. After this call, tell the user it is queued and stop; do not try to do the laptop part yourself.", inputSchema: { type: 'object', properties: { task: { type: 'string', description: 'Full instructions for Claude on the laptop.' } }, required: ['task'] } },
  { name: 'list_computers', description: "List the user's own computers that are linked to this server (for example a laptop and a desktop PC) and whether each is online.", inputSchema: { type: 'object', properties: {} } },
  { name: 'run_on_computer', description: "Hand a task to one of the user's own computers by name (see list_computers). It runs there in this same project, continuing this conversation, once this reply is finished (when that computer is online; otherwise as soon as the user opens it). Use it when the user asks you to set something up or run something on another PC.", inputSchema: { type: 'object', properties: { computer: { type: 'string', description: 'The name of the computer, exactly as list_computers shows it.' }, task: { type: 'string', description: 'What to do there, in full sentences.' } }, required: ['computer', 'task'] } },
];
function hub(tool, args) {
  return new Promise((resolve) => {
    const body = Buffer.from(JSON.stringify({ job: JOB, tool, ...args }));
    const req = http.request({ host: '127.0.0.1', port: PORT, path: '/mcp/hub', method: 'POST', headers: { 'Content-Type': 'application/json', 'Content-Length': body.length }, timeout: 20000 }, (r) => {
      let b = ''; r.on('data', (d) => { b += d; }); r.on('end', () => { try { resolve(JSON.parse(b).text || 'Done.'); } catch { resolve('The server did not answer.'); } });
    });
    req.on('error', () => resolve('The server could not be reached.')); req.on('timeout', () => { req.destroy(); resolve('The server did not answer in time.'); });
    req.end(body);
  });
}
const send = (o) => process.stdout.write(JSON.stringify(o) + '\n');
function ask(args) {
  return new Promise((resolve) => {
    const body = Buffer.from(JSON.stringify({ job: JOB, ...args }));
    const req = http.request({ host: '127.0.0.1', port: PORT, path: '/question', method: 'POST', headers: { 'Content-Type': 'application/json', 'Content-Length': body.length }, timeout: 16 * 60000 }, (r) => {
      let b = ''; r.on('data', (d) => { b += d; }); r.on('end', () => { try { resolve(JSON.parse(b).answer || '(no answer)'); } catch { resolve('(no answer)'); } });
    });
    req.on('error', () => resolve('(the user could not be reached)')); req.on('timeout', () => { req.destroy(); resolve('(no answer in time)'); });
    req.end(body);
  });
}
let buf = '';
process.stdin.setEncoding('utf8');
process.stdin.on('data', (d) => {
  buf += d;
  let i;
  while ((i = buf.indexOf('\n')) >= 0) {
    const line = buf.slice(0, i).trim(); buf = buf.slice(i + 1);
    if (!line) continue;
    let m; try { m = JSON.parse(line); } catch { continue; }
    if (m.method === 'initialize') send({ jsonrpc: '2.0', id: m.id, result: { protocolVersion: (m.params && m.params.protocolVersion) || '2024-11-05', capabilities: { tools: {} }, serverInfo: { name: 'meter', version: '1.0.0' } } });
    else if (m.method === 'tools/list') send({ jsonrpc: '2.0', id: m.id, result: { tools: HUB ? [TOOL, ...HUB_TOOLS] : [TOOL] } });
    else if (m.method === 'tools/call') {
      const name = m.params && m.params.name, args = (m.params && m.params.arguments) || {};
      const reply = (text) => send({ jsonrpc: '2.0', id: m.id, result: { content: [{ type: 'text', text }] } });
      if (HUB && ['laptop_status', 'run_on_laptop', 'list_computers', 'run_on_computer'].includes(name)) hub(name, args).then(reply);
      else ask(args).then((answer) => reply('The user answered: ' + answer));
    }
    else if (m.id !== undefined && m.method) send({ jsonrpc: '2.0', id: m.id, result: {} });
  }
});
