const test = require('node:test');
const assert = require('node:assert/strict');
const { parseSse, proposalIds, toolLabel } = require('./agent_chat.js');

test('parseSse splits complete frames and keeps the rest', () => {
  const buf =
    'event: run.started\ndata: {"run_id":"run_ab"}\n\n' +
    ': keepalive\n\n' +
    'event: assistant.delta\ndata: {"delta":"Hi"}\n\n' +
    'event: assistant.de';
  const { events, rest } = parseSse(buf);
  assert.deepEqual(events, [
    { event: 'run.started', data: { run_id: 'run_ab' } },
    { event: 'assistant.delta', data: { delta: 'Hi' } },
  ]);
  assert.equal(rest, 'event: assistant.de');
});

test('parseSse handles CRLF and non-JSON data', () => {
  const { events } = parseSse('event: x\r\ndata: plain\r\n\r\n');
  assert.deepEqual(events, [{ event: 'x', data: { text: 'plain' } }]);
});

test('proposalIds finds proposal links once', () => {
  const t =
    'Review it at https://neo.example.com/proposals/0123456789abcdef or /proposals/0123456789abcdef and /proposals/fedcba9876543210.';
  assert.deepEqual(proposalIds(t), ['0123456789abcdef', 'fedcba9876543210']);
  assert.deepEqual(proposalIds('/proposals/xyz'), []);
});

test('toolLabel names neo tools plainly', () => {
  assert.equal(toolLabel('mcp_neo_neo_settings_propose'), 'Preparing a settings change');
  assert.equal(toolLabel('mcp_neo_neo_settings_get'), 'Reading settings');
  assert.equal(toolLabel('terminal'), 'Running a command');
  assert.equal(toolLabel('mcp_foo_bar'), 'Working (bar)');
});
