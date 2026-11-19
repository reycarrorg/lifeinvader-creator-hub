// Exercise the actual dashboard count loader with delayed synthetic responses.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');

const source = fs.readFileSync('web/app.js', 'utf8').split('\n');
const lines = [
  'async function mapLimited(',
  'function updateSourceCountElements(',
  'const pendingSourceCounts=',
  'function sourceCount(',
  'async function loadSourceCounts(',
].map(prefix => {
  const line = source.find(item => item.startsWith(prefix));
  assert.ok(line, `Missing UI function: ${prefix}`);
  return line;
});

let active = 0, maxActive = 0;
const calls = new Map();
const elements = Array.from({length: 5}, (_, i) => ({
  dataset: {sourceCount: String(i + 1)},
  textContent: '',
  classList: {contains: () => false},
}));
const context = vm.createContext({
  data: {profiles: elements.map((_, i) => ({id: i + 1, role: 'source'}))},
  current: {type: 'sources'}, categoryRole: 'source', sourceCountGeneration: 0,
  sourceCounts: new Map(), document: {querySelectorAll: () => elements},
  api: async url => {
    const id = Number(url.split('/')[3]);
    calls.set(id, (calls.get(id) || 0) + 1);
    maxActive = Math.max(maxActive, ++active);
    await new Promise(resolve => setTimeout(resolve, 10));
    active--;
    if (id === 2) throw Error('storage busy');
    return {configured: true, videos: id, photos: id + 1};
  },
});
vm.runInContext(lines.join('\n'), context);

(async () => {
  await Promise.all([context.loadSourceCounts(), context.loadSourceCounts()]);
  assert.equal(maxActive, 1);
  for (const [id, count] of calls) assert.equal(count, 1, `source ${id} was scanned twice`);
  assert.equal(calls.size, 5);
  assert.equal(elements[0].textContent, '1 videos · 2 photos');
  assert.equal(elements[1].textContent, 'Count unavailable');
  assert.equal(elements[4].textContent, '5 videos · 6 photos');
  console.log('PASS: dashboard source counts are bounded, deduplicated, and report unavailable folders');
})().catch(error => {console.error(error); process.exitCode = 1;});
