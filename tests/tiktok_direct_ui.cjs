'use strict';

// Run with node --test tests/tiktok_direct_ui.cjs. Requires Playwright, Chromium and FFmpeg.
// Set SNAPMATIC_TEST_BROWSER_CHANNEL=chrome to use an installed Chrome instead of bundled Chromium.
const test = require('node:test');
const assert = require('node:assert/strict');
const { execFileSync } = require('node:child_process');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { chromium } = require('playwright');

const web = path.join(__dirname, '..', 'web');
const origin = 'http://127.0.0.1:31234';
const firstClip = '1'.repeat(64);
const secondClip = '2'.repeat(64);
const digest = 'a'.repeat(64);
const seededCaption = 'Look at #GTA\n\nOne turn.\n\nOne chance.\n\n#Replay';

function makePreview(t) {
 const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'snapmatic-tiktok-ui-'));
 t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
 const file = path.join(directory, 'fixture.mp4');
 execFileSync('ffmpeg', [
  '-loglevel', 'error', '-f', 'lavfi', '-i', 'color=blue:size=64x96:rate=4:duration=2',
  '-c:v', 'libx264', '-pix_fmt', 'yuv420p', '-movflags', '+faststart', file,
 ], { windowsHide: true });
 return fs.readFileSync(file);
}

async function mount(t, page, preview) {
 const posts = [];
 const states = new Map();
 const unexpected = [];
 const errors = [];
 page.on('pageerror', error => errors.push(error.message));
 await page.route('**/*', async route => {
  const request = route.request();
  const url = new URL(request.url());
  if (url.origin === origin) {
   if (url.pathname === '/') return route.fulfill({ contentType: 'text/html', body: '<!doctype html><body></body>' });
   if (url.pathname.startsWith('/synthetic-preview/')) {
    return route.fulfill({ contentType: 'video/mp4', body: preview });
   }
   const match = /^\/api\/oauth\/tiktok\/direct\/(creator|status|post)\/([12]{64})$/.exec(url.pathname);
   if (match) {
    const [, action, clip] = match;
    if (action === 'creator' && request.method() === 'GET') {
     return route.fulfill({ json: {
      creator_username: 'fixture', creator_nickname: 'Synthetic creator', creator_avatar_url: '',
      privacy_level_options: ['SELF_ONLY', 'PUBLIC_TO_EVERYONE'],
      comment_disabled: false, duet_disabled: false, stitch_disabled: true,
      max_video_post_duration_sec: 60, duration_ms: 2000, account_id: 'test-account-id',
      sha256: digest, preview_url: '/synthetic-preview/' + clip + '.mp4',
     } });
    }
    if (action === 'status' && request.method() === 'GET') {
     return route.fulfill({ json: states.get(clip) ?? { state: 'none' } });
    }
    if (action === 'post' && request.method() === 'POST') {
     posts.push({ clip, body: request.postDataJSON() });
     states.set(clip, { state: 'pending', publish_id: 'test-receipt', message: 'Awaiting confirmation' });
     return route.fulfill({ json: states.get(clip) });
    }
   }
  }
  unexpected.push(request.method() + ' ' + url.href);
  return route.abort();
 });
 t.after(() => {
  assert.deepEqual(unexpected, [], 'Requests outside the synthetic fixture');
  assert.deepEqual(errors, [], 'Unhandled browser errors');
 });
 await page.goto(origin);
 await page.evaluate(html => {
  const template = document.createElement('template');
  template.innerHTML = html;
  document.body.append(template.content.querySelector('#tt-direct'));
 }, fs.readFileSync(path.join(web, 'workspace.html'), 'utf8'));
 await page.addScriptTag({ path: path.join(web, 'workspace-platform-draft.js') });
 await page.addScriptTag({ path: path.join(web, 'workspace-tiktok-direct.js') });
 await page.evaluate(({ firstClip }) => {
  window.selection = { clipId: firstClip, useSource: true, version: 'one' };
  window.controller = window.snapmaticTikTokDirect.mount({
   get: async (url, options) => (await fetch(url, options)).json(),
   getSelection: () => window.selection,
   getCaption: () => window.snapmaticDraft.tiktokPostText(
    { title: window.selection.clipId === firstClip ? 'Look at #GTA' : 'Clip two', caption: 'Shared description', tags: '#gta #Replay' },
    { tiktok: 'One turn.\n\nOne chance.' },
   ),
  });
  window.controller.sync();
 }, { firstClip });
 return { posts, states };
}

test('reviews, edits and publishes the exact TikTok caption once per selected clip', async t => {
 const preview = makePreview(t);
 const browser = await chromium.launch({ channel: process.env.SNAPMATIC_TEST_BROWSER_CHANNEL });
 t.after(() => browser.close());
 const page = await browser.newPage();
 page.setDefaultTimeout(6000);
 const { posts, states } = await mount(t, page, preview);
 const review = page.locator('#tt-direct-review');
 const publish = page.locator('#tt-direct-publish');
 const caption = page.locator('#tt-direct-title');
 const privacy = page.locator('#tt-direct-privacy');
 const consent = page.locator('#tt-direct-consent');
 const form = page.locator('#tt-direct-form');
 const changeClip = async clipId => {
  await page.evaluate(clipId => {
   window.selection = { clipId, useSource: true, version: clipId };
   window.controller.sync();
  }, clipId);
 };

 await review.click();
 await form.waitFor();
 await page.waitForFunction(() => document.querySelector('#tt-direct-preview').readyState >= 1);
 assert.deepEqual(posts, []);
 assert.equal(await caption.inputValue(), seededCaption);
 assert.equal(await privacy.inputValue(), '');
 for (const control of ['comment', 'duet', 'stitch', 'consent']) {
  assert.equal(await page.locator('#tt-direct-' + control).isChecked(), false);
 }
 assert.equal(await page.locator('#tt-direct-stitch').isDisabled(), true);
 await consent.check();
 assert.equal(await publish.isDisabled(), true, 'Audience must be chosen explicitly');
 await privacy.selectOption('SELF_ONLY');
 await consent.check();
 assert.equal(await publish.isEnabled(), true);
 await changeClip(secondClip);
 assert.equal(await form.isVisible(), false, 'Switching an authorized preview invalidates consent');
 assert.equal(await publish.isDisabled(), true);
 await changeClip(firstClip);
 await review.click();
 await form.waitFor();
 assert.equal(await consent.isChecked(), false);
 assert.equal(await privacy.inputValue(), '');
 await privacy.selectOption('SELF_ONLY');
 const editedCaption = seededCaption + '\n\nEdited after the preview 🎬 #Final';
 await caption.fill(editedCaption);
 await page.locator('#tt-direct-comment').check();
 await consent.check();
 const reviewedText = await caption.inputValue();
 assert.equal(reviewedText, editedCaption);
 assert.deepEqual(posts, []);

 await publish.click();
 await page.locator('#tt-direct-check').waitFor();
 assert.equal(posts.length, 1);
 assert.equal(posts[0].clip, firstClip);
 assert.equal(posts[0].body.title, reviewedText);
 assert.equal(posts[0].body.privacy_level, 'SELF_ONLY');
 assert.equal(posts[0].body.allow_comment, true);
 assert.equal(posts[0].body.allow_stitch, false);
 assert.equal(posts[0].body.consent, true);
 assert.equal(await publish.isDisabled(), true, 'Pending receipt blocks another submission');
 await review.click();
 await page.locator('#tt-direct-check').waitFor();
 assert.equal(await consent.isDisabled(), true, 'Reopening a pending post cannot start a duplicate');
 states.set(firstClip, { state: 'complete', publish_id: 'test-receipt', post_id: 'test-post-id', message: 'Published' });
 await page.locator('#tt-direct-check').click();
 await page.waitForFunction(() => /published|complete/i.test(document.querySelector('#tt-direct-status').textContent));
 assert.equal(await publish.isDisabled(), true, 'Completed receipt blocks another submission');
 assert.equal(posts.length, 1);

 await changeClip(secondClip);
 assert.equal(await form.isVisible(), false, 'Changing clip requires a new review');
 await review.click();
 await form.waitFor();
 assert.equal(await privacy.inputValue(), '');
 assert.equal(await consent.isChecked(), false);
 const secondCaption = 'Clip two\n\nOne turn.\n\nOne chance.\n\n#gta #Replay';
 assert.equal(await caption.inputValue(), secondCaption);
 assert.equal(await publish.isDisabled(), true);
 assert.equal(posts.length, 1);
 await privacy.selectOption('SELF_ONLY');
 await consent.check();
 await publish.click();
 await page.locator('#tt-direct-check').waitFor();
 assert.equal(posts.length, 2);
 assert.equal(posts[1].clip, secondClip);
 assert.equal(posts[1].body.title, secondCaption);

 await changeClip(firstClip);
 await review.click();
 await page.locator('#tt-direct-check').waitFor();
 assert.equal(await consent.isDisabled(), true, 'Completed posts still cannot be sent after switching away and back');
 assert.equal(await publish.isDisabled(), true);
 assert.equal(posts.length, 2);
});
