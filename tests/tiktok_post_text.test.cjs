'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const { materialize, tiktokPostText } = require('../web/workspace-platform-draft.js');

test('uses the TikTok caption and preserves its internal paragraphs and spelling', () => {
 const draft = { title: 'How CJ escaped 🏃', caption: 'Default description', tags: '#GTA' };
 const overrides = { tiktok: 'Watch the left turn.\r\n\r\nThe second car  misses him.' };
 assert.equal(tiktokPostText(draft, overrides), draft.title + '\n\n' + overrides.tiktok + '\n\n#GTA');
 assert.equal(materialize(draft, overrides, {}).instagram, 'Default description\n\n#GTA');
});

test('uses common caption by default, and honors an intentionally blank override', () => {
 const draft = { title: 'The escape', caption: 'Watch the replay.', tags: '#GTA' };
 assert.equal(tiktokPostText(draft), 'The escape\n\nWatch the replay.\n\n#GTA');
 assert.equal(tiktokPostText(draft, { tiktok: '' }), 'The escape\n\n#GTA');
});

test('adds the headline only once when it matches the caption or its first line', () => {
 const draft = { title: 'Photo finish', caption: 'Photo finish', tags: '#Racing' };
 assert.equal(tiktokPostText(draft), 'Photo finish\n\n#Racing');
 const caption = 'Photo finish\r\nA late overtake';
 assert.equal(tiktokPostText(draft, { tiktok: caption }), caption + '\n\n#Racing');
});

test('appends each missing hashtag once, including Unicode tags and case variants', () => {
 const draft = { title: 'A #GTA surprise', caption: 'Again in #México', tags: '#gta #MÉXICO #Replay #replay' };
 assert.equal(tiktokPostText(draft), 'A #GTA surprise\n\nAgain in #México\n\n#Replay');
});

test('handles empty fields and leaves a long caption intact for validation', () => {
 assert.equal(tiktokPostText({}), '');
 assert.equal(tiktokPostText({ tags: '#Only' }), '#Only');
 const caption = '🎮'.repeat(2500);
 assert.equal(tiktokPostText({ caption }), caption);
});
