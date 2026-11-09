const test = require('node:test');
const assert = require('node:assert/strict');
const { parseSse, proposalIds, toolLabel } = require('./agent_chat_core.js');

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

const {
  refQuery,
  splitRefs,
  fuzzyScore,
  rankRefs,
  insertRef,
  prettyValue,
  formatDuration,
  toolCallsFromMessages,
  toolDetail,
} = require('./agent_chat_core.js');

test('fuzzyScore matches subsequences and prefers word starts', () => {
  assert.ok(fuzzyScore('nxtc', 'nextcloud') > 0);
  assert.equal(fuzzyScore('xyz', 'nextcloud'), -1);
  assert.equal(fuzzyScore('', 'anything'), 0);
  // prefix beats a scattered match
  assert.ok(fuzzyScore('next', 'nextcloud') > fuzzyScore('next', 'ownext-thing'));
  // camelCase / dotted word starts count: "lip" → localIP
  assert.ok(fuzzyScore('lip', 'pihole.localIP') > fuzzyScore('lip', 'pihole.flipflop'));
  assert.ok(fuzzyScore('nc', 'nextCloud') > fuzzyScore('nc', 'nextcloud'));
});

const ITEMS = [
  { kind: 'service', token: 'nextcloud', label: 'nextcloud', enabled: true },
  { kind: 'service', token: 'immich', label: 'immich', enabled: false },
  { kind: 'service', token: 'pihole', label: 'pihole', enabled: true },
  { kind: 'guide', token: 'guide:adblock-at-home', label: 'Block ads for the whole home', enabled: true },
  { kind: 'option', token: 'nextcloud.subdomain', label: 'Subdomain', enabled: true },
  { kind: 'option', token: 'nextcloud.containers.nextcloud', label: 'Containers › Nextcloud', enabled: true, advanced: true },
  { kind: 'option', token: 'pihole.localIP', label: 'Home network address', enabled: true },
];

test('rankRefs: empty query lists installed apps and guides', () => {
  assert.deepEqual(
    rankRefs(ITEMS, '', 8).map((i) => i.token),
    ['nextcloud', 'pihole', 'guide:adblock-at-home']
  );
});

test('rankRefs: fuzzy query, apps before settings, dotted query finds settings', () => {
  const r = rankRefs(ITEMS, 'nxtc', 8).map((i) => i.token);
  assert.equal(r[0], 'nextcloud');
  assert.ok(r.indexOf('nextcloud.subdomain') > 0);
  // plumbing ranks after the everyday setting
  assert.ok(r.indexOf('nextcloud.containers.nextcloud') > r.indexOf('nextcloud.subdomain'));
  assert.equal(rankRefs(ITEMS, 'nextcloud.sub', 8)[0].token, 'nextcloud.subdomain');
  // labels match too
  assert.equal(rankRefs(ITEMS, 'block ads', 8)[0].token, 'guide:adblock-at-home');
  assert.equal(rankRefs(ITEMS, 'home netw', 8)[0].token, 'pihole.localIP');
  assert.equal(rankRefs(ITEMS, 'qqq', 8).length, 0);
  assert.equal(rankRefs(ITEMS, 'e', 2).length, 2);
});

test('refQuery finds the @query at the caret', () => {
  assert.deepEqual(refQuery('hi @nxt', 7), { start: 3, query: 'nxt' });
  assert.deepEqual(refQuery('@', 1), { start: 0, query: '' });
  assert.deepEqual(refQuery('(@pi', 4), { start: 1, query: 'pi' });
  assert.deepEqual(refQuery('x @nextcloud.sub', 16), { start: 2, query: 'nextcloud.sub' });
  assert.equal(refQuery('mail me@host', 12), null);
  assert.equal(refQuery('no at here', 5), null);
  assert.equal(refQuery('@pi hole', 8), null);
});

test('insertRef replaces the query and adds one space', () => {
  assert.deepEqual(insertRef('hi @nxt', 3, 7, 'nextcloud'), { text: 'hi @nextcloud ', caret: 14 });
  // rest of a half-typed token is replaced, existing space kept
  assert.deepEqual(insertRef('@nx is it on?', 0, 2, 'nextcloud'), { text: '@nextcloud is it on?', caret: 10 });
});

test('splitRefs mirrors the server token rules', () => {
  assert.deepEqual(splitRefs('is @nextcloud.subdomain set? (@guide:x).'), [
    { text: 'is ' },
    { ref: 'nextcloud.subdomain' },
    { text: ' set? (' },
    { ref: 'guide:x' },
    { text: ').' },
  ]);
  assert.deepEqual(splitRefs('me@host @ @.x'), [{ text: 'me@host @ @.x' }]);
  assert.deepEqual(splitRefs('@pihole.'), [{ ref: 'pihole' }, { text: '.' }]);
});

test('prettyValue pretty-prints JSON and cuts long text', () => {
  assert.equal(prettyValue({ a: 1 }), '{\n  "a": 1\n}');
  assert.equal(prettyValue('{"a":[1]}'), '{\n  "a": [\n    1\n  ]\n}');
  assert.equal(prettyValue('plain text'), 'plain text');
  assert.equal(prettyValue(null), '');
  const long = prettyValue('x'.repeat(10000), 100);
  assert.ok(long.startsWith('x'.repeat(100)) && long.includes('9900 more characters'));
});

test('formatDuration', () => {
  assert.equal(formatDuration(0.0004), '1 ms');
  assert.equal(formatDuration(0.25), '250 ms');
  assert.equal(formatDuration(3.21), '3.2 s');
  assert.equal(formatDuration(125), '2 min 5 s');
  assert.equal(formatDuration(undefined), '');
  assert.equal(formatDuration(-1), '');
});

test('toolCallsFromMessages pairs calls with results', () => {
  const calls = toolCallsFromMessages([
    { role: 'assistant', content: '', tool_calls: [
      { id: 'a', function: { name: 'terminal', arguments: '{"command":"ls"}' } },
      { id: 'b', function: { name: 'mcp_neo_neo_settings_get', arguments: 'oops' } },
    ] },
    { role: 'tool', tool_call_id: 'b', content: [{ type: 'text', text: '[]' }] },
    { role: 'tool', tool_call_id: 'a', content: 'file1' },
    { role: 'assistant', content: 'Done.' },
  ]);
  assert.deepEqual(calls, [
    { id: 'a', name: 'terminal', args: { command: 'ls' }, result: 'file1' },
    { id: 'b', name: 'mcp_neo_neo_settings_get', args: 'oops', result: '[]' },
  ]);
  assert.deepEqual(toolCallsFromMessages(undefined), []);
});

test('toolDetail shows status, duration, input and output', () => {
  const d = toolDetail({ name: 'terminal', args: { command: 'ls' }, status: 'done', started: 10, ended: 11.5, result: '{"ok":true}', final: true });
  assert.equal(d.status, 'done');
  assert.equal(d.meta, 'Done · 1.5 s');
  assert.deepEqual(d.sections.map((s) => s.label), ['Tool', 'Input', 'Output']);
  assert.equal(d.sections[0].text, 'terminal');
  assert.equal(d.sections[1].text, '{\n  "command": "ls"\n}');
  assert.equal(d.sections[2].text, '{\n  "ok": true\n}');

  const running = toolDetail({ name: 'x', preview: 'ls -la', status: 'running', started: 1 });
  assert.equal(running.meta, 'Running');
  assert.equal(running.sections[1].text, 'ls -la');
  assert.equal(running.sections[2].text, 'Waiting for the result…');

  assert.equal(toolDetail({ name: 'x', status: 'error' }).sections[1].text, 'Shown when the reply is finished.');
  assert.equal(toolDetail({ name: 'x', status: 'done', final: true }).sections[1].text, 'No output was recorded.');
  assert.equal(toolDetail({ name: 'x', status: 'done', final: true, result: '' }).sections[1].text, '(empty)');
});

const {
  clockText,
  relativeTime,
  countdownText,
  statusLine,
  groupSessions,
  outcomeText,
  parseSavedRun,
} = require('./agent_chat_core.js');

test('clockText formats elapsed time', () => {
  assert.equal(clockText(0), '0:00');
  assert.equal(clockText(42.9), '0:42');
  assert.equal(clockText(605), '10:05');
  assert.equal(clockText(3723), '1:02:03');
  assert.equal(clockText(-5), '0:00');
});

test('relativeTime reads like a person would say it', () => {
  const now = Date.UTC(2026, 10, 5, 12, 0, 0);
  const s = now / 1000;
  assert.equal(relativeTime(s - 10, now), 'just now');
  assert.equal(relativeTime(s - 5 * 60, now), '5 min ago');
  assert.equal(relativeTime(s - 3 * 3600, now), '3 h ago');
  assert.equal(relativeTime(s - 30 * 3600, now), 'yesterday');
  assert.equal(relativeTime(s - 4 * 86400, now), '4 days ago');
  assert.match(relativeTime(s - 20 * 86400, now), /^\d{1,2} Oct$/);
  assert.match(relativeTime(s - 400 * 86400, now), /2025$/);
  assert.equal(relativeTime(0, now), '');
  // A clock slightly ahead never shows "in the future".
  assert.equal(relativeTime(s + 30, now), 'just now');
});

test('countdownText counts down and expires', () => {
  assert.equal(countdownText(300), 'Expires in 5:00');
  assert.equal(countdownText(64.2), 'Expires in 1:05');
  assert.equal(countdownText(0), 'Expired');
  assert.equal(countdownText(-3), 'Expired');
  assert.equal(countdownText(null), '');
});

test('statusLine names the phase, time and tool', () => {
  assert.equal(statusLine({ phase: 'connecting' }), 'Connecting to the assistant…');
  assert.equal(
    statusLine({ phase: 'working', elapsed: 42, activity: 'Running a command' }),
    'Working · 0:42 · Running a command'
  );
  assert.equal(statusLine({ phase: 'working', elapsed: 5 }), 'Working · 0:05');
  assert.equal(statusLine({ phase: 'working', elapsed: 70, background: true, activity: 'x' }), 'Still working · 1:10');
  assert.equal(statusLine({ phase: 'waiting', elapsed: 61 }), 'Waiting for your approval · 1:01');
  assert.equal(statusLine({ phase: 'stopping', elapsed: 9 }), 'Stopping…');
  assert.equal(statusLine({ phase: 'idle' }), '');
});

test('groupSessions puts this page first and sorts newest first', () => {
  const items = [
    { id: 'a', last_active: 10, here: false },
    { id: 'b', last_active: 30, here: true },
    { id: 'c', last_active: 20, here: false },
    { id: 'd', last_active: 5, here: true },
  ];
  const g = groupSessions(items, 'service:pihole');
  assert.deepEqual(g.map((x) => x.label), ['About this page', 'Other chats']);
  assert.deepEqual(g[0].items.map((x) => x.id), ['b', 'd']);
  assert.deepEqual(g[1].items.map((x) => x.id), ['c', 'a']);
  // From the header (general): one list, newest first.
  const all = groupSessions(items, 'general');
  assert.equal(all.length, 1);
  assert.deepEqual(all[0].items.map((x) => x.id), ['b', 'c', 'a', 'd']);
  // No chats of this page: just the recent ones.
  assert.deepEqual(groupSessions([{ id: 'x', last_active: 1 }], 'guide:y').map((x) => x.label), ['Recent chats']);
  assert.deepEqual(groupSessions([], 'general'), []);
  // The input keeps its order.
  assert.equal(items[0].id, 'a');
});

test('outcomeText shows stopped and failed turns as such', () => {
  assert.equal(outcomeText('completed'), null);
  assert.equal(outcomeText('gone'), null);
  assert.equal(outcomeText('cancelled').error, false);
  assert.match(outcomeText('cancelled').text, /^Stopped/);
  assert.deepEqual(outcomeText('failed', 'model offline'), { text: 'The reply failed: model offline', error: true });
  assert.equal(outcomeText('failed').text, 'The reply failed.');
  assert.equal(outcomeText('interrupted').error, true);
});

test('parseSavedRun accepts only well-formed saved turns', () => {
  const saved = { session_id: 'neo-general-1730000000', run_id: 'run_ab12', context: 'general', title: 'Assistant', started: 5 };
  assert.deepEqual(parseSavedRun(JSON.stringify(saved)), saved);
  assert.equal(parseSavedRun(null), null);
  assert.equal(parseSavedRun('{bad'), null);
  assert.equal(parseSavedRun(JSON.stringify({ session_id: '../x' })), null);
  assert.equal(parseSavedRun(JSON.stringify({ session_id: 'neo-x', run_id: 'run_zz' })), null);
  const noRun = parseSavedRun(JSON.stringify({ session_id: 'neo-x' }));
  assert.equal(noRun.run_id, null);
  assert.equal(noRun.title, 'Assistant');
});
