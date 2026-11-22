'use strict';
const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const source = fs.readFileSync(path.join(__dirname, '../web/workspace.js'), 'utf8');
function section(start, end) {
  const from = source.indexOf(start), to = source.indexOf(end, from);
  assert.ok(from >= 0 && to > from);
  return source.slice(from, to);
}
function harness() {
  const elements = new Map(), storage = new Map(), posts = [];
  for (const id of ['ship-chk-yt','ship-chk-ig','ship-chk-tt','ship-chk-x','multi-ship-go',
    'ship-recipe-go','multi-ship-log','title','caption','tags']) {
    elements.set(id, {value:'', checked:false, disabled:false, append(){}, replaceChildren(){}});
  }
  elements.get('title').value = 'The escape route';
  const account = name => ({connected:true, account_id:name, handle:'@'+name});
  const box = {
    $: id => elements.get(id),
    node: () => ({}),
    localStorage:{getItem:key=>storage.get(key), setItem:(key,value)=>storage.set(key,value)},
    window:{confirm:()=>true, snapmaticTikTokDirect:{
      prepareForShip:async()=>({ready:true, snapshot:'reviewed'}),
      publishReviewed:async proof=>{assert.equal(proof,'reviewed');posts.push('tiktok');return {state:'pending'};}
    }},
    selected:{clip_id:'clip1',from_sources:true,exported:true,exported_file:'video.mp4'},
    oauthStatus:{tiktok:{...account('cj'),direct_post_granted:true},instagram:account('ig'),x:account('x')},
    directExportVersion:0,tikTokDirect:null,pdState:null,coverUrl:null,
    pd:{materialize:()=>({instagram:'Reel caption'})},
    xPostText:()=> 'X caption',
    note(){},renderClipShippedBanner(){},updateMockOverlay(){},reload:async()=>{},loadExportHistory(){},
    get:async(url,opts)=>{
      if(url==='/api/oauth/status')return box.oauthStatus;
      const body = JSON.parse(opts.body);
      if(url.includes('/instagram/')){
        assert.equal(body.publish,true);assert.equal(body.expected_account_id,'ig');
        assert.equal(body.caption,'Reel caption');posts.push('instagram');return {published:true,media_id:'id'};
      }
      if(url.includes('/x/')){
        assert.equal(body.confirm_publish,true);assert.equal(body.expected_account_id,'x');
        posts.push('x');return {tweet_id:'x-id'};
      }
      throw Error('Unexpected request '+url);
    }
  };
  const context = vm.createContext(box);
  vm.runInContext(section('const shipDestinations =','function brandIcon(') +
    section('function syncYoutubeDraftButton()', 'function renderOAuthCard(') +
    section('async function runMultiShip(', 'function extractHashtags('), context);
  box.syncYoutubeDraftButton();
  return {box, posts, elements};
}

test('new connected accounts are selected and a previous account keeps its own choices', () => {
  const {box,elements} = harness();
  assert.equal(elements.get('ship-chk-yt').checked,false);
  const ig = elements.get('ship-chk-ig');
  assert.equal(ig.checked,true);
  ig.checked=false;ig.onchange();
  assert.equal(ig.checked,false);
  const original=box.oauthStatus.instagram;
  box.oauthStatus.instagram={connected:true,account_id:'another'};
  box.syncYoutubeDraftButton();
  assert.equal(ig.checked,true);
  box.oauthStatus.instagram=original;box.syncYoutubeDraftButton();
  assert.equal(ig.checked,false);
  box.oauthStatus.tiktok.direct_post_granted=false;box.syncYoutubeDraftButton();
  assert.equal(elements.get('ship-chk-tt').checked,false);
});

test('TikTok review or final cancellation stops all posts', async () => {
  const {box,posts} = harness();
  box.window.snapmaticTikTokDirect.prepareForShip=async()=>({ready:false,message:'Select an audience.'});
  assert.equal((await box.runMultiShip()).cancelled,true);
  assert.deepEqual(posts,[]);
  box.window.snapmaticTikTokDirect.prepareForShip=async()=>({ready:true,snapshot:'reviewed'});
  box.window.confirm=()=>false;
  assert.equal((await box.runMultiShip()).cancelled,true);
  assert.deepEqual(posts,[]);
});

test('direct post is attempted first, a processing video is counted separately', async () => {
  const {box,posts} = harness();
  const result=await box.runMultiShip();
  assert.deepEqual(posts,['tiktok','instagram','x']);
  assert.equal(result.okCount,2);
  assert.equal(result.pendingCount,1);
});

test('a changed clip during review cannot be sent and a changed account halts later destinations', async () => {
  const {box,posts} = harness();
  box.window.snapmaticTikTokDirect.prepareForShip=async()=>{
    box.selected={...box.selected,clip_id:'changed'};return {ready:true,snapshot:'reviewed'};
  };
  assert.equal((await box.runMultiShip()).cancelled,true);
  assert.deepEqual(posts,[]);
  box.window.snapmaticTikTokDirect.prepareForShip=async()=>({ready:true,snapshot:'reviewed'});
  box.window.snapmaticTikTokDirect.publishReviewed=async()=>{
    posts.push('tiktok');box.oauthStatus.x={connected:true,account_id:'changed'};
    return {state:'pending'};
  };
  const result=await box.runMultiShip();
  assert.deepEqual(posts,['tiktok']);
  assert.match(result.summary,/2 skipped/);
});

test('the full selection requests a public YouTube Short and reports restricted and failed outcomes', async () => {
  const {box,posts}=harness();
  box.oauthStatus.youtube={connected:true,account_id:'yt',handle:'@yt'};
  box.syncYoutubeDraftButton();
  const standard=box.get;
  box.get=async(url,opts)=>{
    if(url.includes('/youtube/')){
      const body=JSON.parse(opts.body);
      assert.equal(body.privacy,'public');
      assert.equal(body.confirm_publish,true);
      assert.equal(body.made_for_kids,false);
      assert.equal(body.as_short,true);
      assert.equal(body.expected_account_id,'yt');
      posts.push('youtube');
      return {video_id:'yt-id',privacy:'private'};
    }
    if(url.includes('/instagram/')){posts.push('instagram');throw Error('Lost the publication response');}
    return standard(url,opts);
  };
  const result=await box.runMultiShip();
  assert.deepEqual(posts,['tiktok','youtube','instagram','x']);
  assert.deepEqual([...result.failedTargets],['instagram']);
  assert.equal(result.restrictedCount,1);
  assert.equal(result.pendingCount,1);
  assert.equal(result.okCount,1);
});

test('a repeated click while review is open cannot submit a second batch', async () => {
  const {box,posts}=harness();
  let ready, release;
  const opened=new Promise(resolve=>{ready=resolve;});
  box.window.snapmaticTikTokDirect.prepareForShip=async()=>{
    ready();
    await new Promise(resolve=>{release=resolve;});
    return {ready:true,snapshot:'reviewed'};
  };
  const first=box.runMultiShip();
  await opened;
  const second=await box.runMultiShip();
  assert.equal(second.cancelled,true);
  release();
  await first;
  assert.deepEqual(posts,['tiktok','instagram','x']);
});
